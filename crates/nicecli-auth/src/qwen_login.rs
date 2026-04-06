use crate::{OAuthFlowError, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::{Client, Proxy};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::time::sleep;

const DEFAULT_QWEN_DEVICE_CODE_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/device/code";
const DEFAULT_QWEN_TOKEN_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/token";
const DEFAULT_QWEN_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";
const DEFAULT_QWEN_SCOPE: &str = "openid profile email model.completion";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedQwenLogin {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedQwenLogin {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QwenLoginEndpoints {
    pub device_code_url: String,
    pub token_url: String,
    pub client_id: String,
    pub scope: String,
}

impl Default for QwenLoginEndpoints {
    fn default() -> Self {
        Self {
            device_code_url: DEFAULT_QWEN_DEVICE_CODE_URL.to_string(),
            token_url: DEFAULT_QWEN_TOKEN_URL.to_string(),
            client_id: DEFAULT_QWEN_CLIENT_ID.to_string(),
            scope: DEFAULT_QWEN_SCOPE.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum QwenLoginError {
    #[error(transparent)]
    OAuthSession(#[from] OAuthFlowError),
    #[error("failed to generate secure random bytes")]
    Random(rand::Error),
    #[error("qwen oauth flow is not pending")]
    SessionNotPending,
    #[error("qwen device flow request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("qwen device flow returned {status}: {body}")]
    UnexpectedDeviceCodeStatus { status: u16, body: String },
    #[error("qwen token endpoint returned {status}: {body}")]
    UnexpectedTokenStatus { status: u16, body: String },
    #[error("failed to parse qwen device flow response: {0}")]
    ParseDeviceFlow(#[source] serde_json::Error),
    #[error("failed to parse qwen token response: {0}")]
    ParseToken(#[source] serde_json::Error),
    #[error("qwen device flow response is missing verification url")]
    MissingVerificationUrl,
    #[error("qwen device flow response is missing device_code")]
    MissingDeviceCode,
    #[error("qwen token response is missing access_token")]
    MissingAccessToken,
    #[error("device code expired. Please restart the authentication process")]
    DeviceCodeExpired,
    #[error("authorization denied by user. Please restart the authentication process")]
    AuthorizationDenied,
    #[error("authentication timeout. Please restart the authentication process")]
    AuthenticationTimeout,
    #[error("qwen token polling failed: {0}")]
    PollRejected(String),
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to encode auth file: {0}")]
    EncodeAuthFile(serde_json::Error),
    #[error("failed to write auth file: {0}")]
    WriteAuthFile(std::io::Error),
}

#[derive(Debug, Clone)]
struct PendingQwenLogin {
    device_code: String,
    code_verifier: String,
    poll_interval: Duration,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct QwenLoginService {
    oauth_sessions: Arc<OAuthSessionStore>,
    default_proxy_url: Option<String>,
    endpoints: QwenLoginEndpoints,
    ttl: Duration,
    pending: Arc<Mutex<HashMap<String, PendingQwenLogin>>>,
}

impl QwenLoginService {
    pub fn new(oauth_sessions: Arc<OAuthSessionStore>, default_proxy_url: Option<String>) -> Self {
        Self {
            oauth_sessions,
            default_proxy_url: trimmed(default_proxy_url),
            endpoints: QwenLoginEndpoints::default(),
            ttl: DEFAULT_OAUTH_SESSION_TTL,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_endpoints(mut self, endpoints: QwenLoginEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn start_login(&self) -> Result<StartedQwenLogin, QwenLoginError> {
        let state = generate_state()?;
        let pkce = generate_pkce_codes()?;
        let device_flow = self.request_device_flow(&pkce.code_challenge).await?;
        let url = trimmed(Some(device_flow.verification_uri_complete))
            .or_else(|| trimmed(Some(device_flow.verification_uri)))
            .ok_or(QwenLoginError::MissingVerificationUrl)?;
        let device_code =
            trimmed(Some(device_flow.device_code)).ok_or(QwenLoginError::MissingDeviceCode)?;

        self.oauth_sessions.register(&state, "qwen")?;
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending.insert(
            state.clone(),
            PendingQwenLogin {
                device_code,
                code_verifier: pkce.code_verifier,
                poll_interval: normalize_poll_interval(device_flow.interval),
                expires_at: Instant::now() + normalize_login_ttl(device_flow.expires_in, self.ttl),
            },
        );

        Ok(StartedQwenLogin { state, url })
    }

    pub async fn complete_login(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<CompletedQwenLogin, QwenLoginError> {
        if !self.oauth_sessions.is_pending(state, Some("qwen"))? {
            self.remove_pending(state);
            return Err(QwenLoginError::SessionNotPending);
        }

        let state = state.trim();
        let pending = self.take_pending_login(state)?;
        let token = match self.poll_for_token(state, &pending).await {
            Ok(token) => token,
            Err(error) => {
                let _ = self
                    .oauth_sessions
                    .set_error(state, &session_error_message(&error));
                return Err(error);
            }
        };

        let email = unix_timestamp_millis().to_string();
        let file_name = format!("qwen-{email}.json");
        let file_path = auth_dir.join(&file_name);

        fs::create_dir_all(auth_dir).map_err(QwenLoginError::CreateAuthDir)?;
        let payload = serde_json::json!({
            "id": file_name,
            "provider": "qwen",
            "type": "qwen",
            "email": email,
            "access_token": token.access_token,
            "refresh_token": token.refresh_token,
            "resource_url": token.resource_url,
            "last_refresh": rfc3339_now(),
            "expired": token.expired(),
        });
        let bytes = serde_json::to_vec_pretty(&payload).map_err(QwenLoginError::EncodeAuthFile)?;
        fs::write(&file_path, bytes).map_err(QwenLoginError::WriteAuthFile)?;

        self.oauth_sessions.complete(state)?;

        Ok(CompletedQwenLogin {
            file_name: file_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_path,
            email,
        })
    }

    fn take_pending_login(&self, state: &str) -> Result<PendingQwenLogin, QwenLoginError> {
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending
            .remove(state)
            .ok_or(QwenLoginError::SessionNotPending)
    }

    fn remove_pending(&self, state: &str) {
        let mut pending = self.lock_pending();
        pending.remove(state);
    }

    fn build_http_client(&self) -> Result<Client, reqwest::Error> {
        let mut builder = Client::builder().timeout(Duration::from_secs(30));
        if let Some(proxy_url) = &self.default_proxy_url {
            builder = builder.proxy(Proxy::all(proxy_url)?);
        }
        builder.build()
    }

    async fn request_device_flow(
        &self,
        code_challenge: &str,
    ) -> Result<DeviceFlowResponse, QwenLoginError> {
        let client = self.build_http_client()?;
        let response = client
            .post(&self.endpoints.device_code_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.endpoints.client_id.as_str()),
                ("scope", self.endpoints.scope.as_str()),
                ("code_challenge", code_challenge),
                ("code_challenge_method", "S256"),
            ])
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(QwenLoginError::UnexpectedDeviceCodeStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        serde_json::from_slice(&body).map_err(QwenLoginError::ParseDeviceFlow)
    }

    async fn poll_for_token(
        &self,
        state: &str,
        pending: &PendingQwenLogin,
    ) -> Result<TokenResponse, QwenLoginError> {
        let client = self.build_http_client()?;
        let mut poll_interval = pending.poll_interval;

        loop {
            if Instant::now() >= pending.expires_at {
                return Err(QwenLoginError::AuthenticationTimeout);
            }
            if !self.oauth_sessions.is_pending(state, Some("qwen"))? {
                return Err(QwenLoginError::SessionNotPending);
            }

            let response = client
                .post(&self.endpoints.token_url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", self.endpoints.client_id.as_str()),
                    ("device_code", pending.device_code.as_str()),
                    ("code_verifier", pending.code_verifier.as_str()),
                ])
                .send()
                .await?;

            let status = response.status();
            let body = response.bytes().await?;
            if status.is_success() {
                let token: TokenResponse =
                    serde_json::from_slice(&body).map_err(QwenLoginError::ParseToken)?;
                if token.access_token.trim().is_empty() {
                    return Err(QwenLoginError::MissingAccessToken);
                }
                return Ok(token);
            }

            let error_response = serde_json::from_slice::<TokenErrorResponse>(&body).ok();
            if status.as_u16() == 400 {
                if let Some(error_response) = error_response {
                    match error_response.error.trim() {
                        "authorization_pending" => {
                            sleep(poll_interval).await;
                            continue;
                        }
                        "slow_down" => {
                            poll_interval = slow_down_poll_interval(poll_interval);
                            sleep(poll_interval).await;
                            continue;
                        }
                        "expired_token" => return Err(QwenLoginError::DeviceCodeExpired),
                        "access_denied" => return Err(QwenLoginError::AuthorizationDenied),
                        _ => {
                            let description = first_non_empty(
                                &error_response.error_description,
                                &error_response.error,
                            );
                            return Err(QwenLoginError::PollRejected(description));
                        }
                    }
                }
            }

            return Err(QwenLoginError::UnexpectedTokenStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }
    }

    fn lock_pending(&self) -> MutexGuard<'_, HashMap<String, PendingQwenLogin>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Deserialize)]
struct DeviceFlowResponse {
    #[serde(default)]
    device_code: String,
    #[serde(default)]
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    interval: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    resource_url: String,
    #[serde(default)]
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

fn generate_state() -> Result<String, QwenLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(QwenLoginError::Random)?;
    Ok(format!("qwen-{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn generate_pkce_codes() -> Result<PkceCodes, QwenLoginError> {
    let mut verifier_bytes = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut verifier_bytes)
        .map_err(QwenLoginError::Random)?;
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);
    Ok(PkceCodes {
        code_verifier,
        code_challenge,
    })
}

fn purge_expired_pending(pending: &mut HashMap<String, PendingQwenLogin>) {
    let now = Instant::now();
    pending.retain(|_, entry| entry.expires_at > now);
}

fn normalize_poll_interval(seconds: i64) -> Duration {
    if seconds <= 0 {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(seconds as u64)
    }
}

fn slow_down_poll_interval(current: Duration) -> Duration {
    let current_secs = current.as_secs().max(1);
    let next_secs = (current_secs.saturating_mul(3) / 2).max(current_secs + 1);
    Duration::from_secs(next_secs.min(10))
}

fn normalize_login_ttl(expires_in: i64, fallback: Duration) -> Duration {
    if expires_in <= 0 {
        fallback
    } else {
        Duration::from_secs(expires_in as u64)
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_non_empty(primary: &str, fallback: &str) -> String {
    let primary = primary.trim();
    if !primary.is_empty() {
        primary.to_string()
    } else {
        fallback.trim().to_string()
    }
}

fn session_error_message(error: &QwenLoginError) -> String {
    match error {
        QwenLoginError::CreateAuthDir(_)
        | QwenLoginError::EncodeAuthFile(_)
        | QwenLoginError::WriteAuthFile(_) => "Failed to save authentication tokens".to_string(),
        QwenLoginError::SessionNotPending => "Authentication failed".to_string(),
        _ => error.to_string(),
    }
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn rfc3339_now() -> String {
    let now = SystemTime::now();
    let datetime: chrono::DateTime<chrono::Utc> = now.into();
    datetime.to_rfc3339()
}

impl TokenResponse {
    fn expired(&self) -> String {
        let now = SystemTime::now();
        let future = now
            .checked_add(Duration::from_secs(self.expires_in.max(0) as u64))
            .unwrap_or(now);
        let datetime: chrono::DateTime<chrono::Utc> = future.into();
        datetime.to_rfc3339()
    }
}

#[cfg(test)]
mod tests {
    use super::{QwenLoginEndpoints, QwenLoginService};
    use crate::OAuthSessionStore;
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    async fn spawn_qwen_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local addr");
        let router = Router::new()
            .route(
                "/api/v1/oauth2/device/code",
                post(|| async {
                    Json(json!({
                        "device_code": "device-code-123",
                        "verification_uri": "https://chat.qwen.ai/device",
                        "verification_uri_complete": "https://chat.qwen.ai/device?user_code=ABCD",
                        "expires_in": 600,
                        "interval": 1
                    }))
                }),
            )
            .route(
                "/api/v1/oauth2/token",
                post(|| async {
                    Json(json!({
                        "access_token": "access-token-123",
                        "refresh_token": "refresh-token-456",
                        "resource_url": "https://dashscope.aliyuncs.com",
                        "expires_in": 3600
                    }))
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("qwen server");
        });
        address
    }

    #[tokio::test]
    async fn completes_qwen_login_and_writes_auth_file() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_qwen_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            QwenLoginService::new(sessions.clone(), None).with_endpoints(QwenLoginEndpoints {
                device_code_url: format!("http://{server}/api/v1/oauth2/device/code"),
                token_url: format!("http://{server}/api/v1/oauth2/token"),
                ..QwenLoginEndpoints::default()
            });

        let started = service.start_login().await.expect("start login");
        assert!(started.url.contains("chat.qwen.ai/device"));
        assert!(sessions
            .is_pending(&started.state, Some("qwen"))
            .expect("pending"));

        let completed = service
            .complete_login(temp_dir.path(), &started.state)
            .await
            .expect("complete login");

        assert!(completed.file_name.starts_with("qwen-"));
        assert!(completed.file_name.ends_with(".json"));
        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&completed.file_path).expect("auth file"))
                .expect("json");
        assert_eq!(payload["type"].as_str(), Some("qwen"));
        assert_eq!(payload["provider"].as_str(), Some("qwen"));
        assert_eq!(payload["email"].as_str(), Some(completed.email.as_str()));
        assert_eq!(payload["access_token"].as_str(), Some("access-token-123"));
        assert_eq!(payload["refresh_token"].as_str(), Some("refresh-token-456"));
        assert_eq!(
            payload["resource_url"].as_str(),
            Some("https://dashscope.aliyuncs.com")
        );
        assert!(sessions.get(&started.state).expect("get session").is_none());
    }
}
