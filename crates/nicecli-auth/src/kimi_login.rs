use crate::{
    should_bypass_proxy_for_url, OAuthFlowError, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::{Client, Proxy};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::time::sleep;

const DEFAULT_KIMI_DEVICE_CODE_URL: &str = "https://auth.kimi.com/api/oauth/device_authorization";
const DEFAULT_KIMI_TOKEN_URL: &str = "https://auth.kimi.com/api/oauth/token";
const DEFAULT_KIMI_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const DEFAULT_KIMI_PLATFORM: &str = "cli-proxy-api";
const DEFAULT_KIMI_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedKimiLogin {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedKimiLogin {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiLoginEndpoints {
    pub device_code_url: String,
    pub token_url: String,
    pub client_id: String,
}

impl Default for KimiLoginEndpoints {
    fn default() -> Self {
        Self {
            device_code_url: DEFAULT_KIMI_DEVICE_CODE_URL.to_string(),
            token_url: DEFAULT_KIMI_TOKEN_URL.to_string(),
            client_id: DEFAULT_KIMI_CLIENT_ID.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum KimiLoginError {
    #[error(transparent)]
    OAuthSession(#[from] OAuthFlowError),
    #[error("failed to generate secure random bytes")]
    Random(rand::Error),
    #[error("kimi oauth flow is not pending")]
    SessionNotPending,
    #[error("kimi device flow request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("kimi device flow returned {status}: {body}")]
    UnexpectedDeviceCodeStatus { status: u16, body: String },
    #[error("kimi token endpoint returned {status}: {body}")]
    UnexpectedTokenStatus { status: u16, body: String },
    #[error("failed to parse kimi device flow response: {0}")]
    ParseDeviceFlow(#[source] serde_json::Error),
    #[error("failed to parse kimi token response: {0}")]
    ParseToken(#[source] serde_json::Error),
    #[error("kimi device flow response is missing verification url")]
    MissingVerificationUrl,
    #[error("kimi device flow response is missing device_code")]
    MissingDeviceCode,
    #[error("kimi token response is missing access_token")]
    MissingAccessToken,
    #[error("device code expired. Please restart the authentication process")]
    DeviceCodeExpired,
    #[error("authorization denied by user. Please restart the authentication process")]
    AuthorizationDenied,
    #[error("authentication timeout. Please restart the authentication process")]
    AuthenticationTimeout,
    #[error("kimi token polling failed: {0}")]
    PollRejected(String),
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to encode auth file: {0}")]
    EncodeAuthFile(serde_json::Error),
    #[error("failed to write auth file: {0}")]
    WriteAuthFile(std::io::Error),
}

#[derive(Debug, Clone)]
struct PendingKimiLogin {
    device_code: String,
    device_id: String,
    poll_interval: Duration,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct KimiLoginService {
    oauth_sessions: Arc<OAuthSessionStore>,
    default_proxy_url: Option<String>,
    endpoints: KimiLoginEndpoints,
    ttl: Duration,
    pending: Arc<Mutex<HashMap<String, PendingKimiLogin>>>,
}

impl KimiLoginService {
    pub fn new(oauth_sessions: Arc<OAuthSessionStore>, default_proxy_url: Option<String>) -> Self {
        Self {
            oauth_sessions,
            default_proxy_url: trimmed(default_proxy_url),
            endpoints: KimiLoginEndpoints::default(),
            ttl: DEFAULT_OAUTH_SESSION_TTL,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_endpoints(mut self, endpoints: KimiLoginEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn start_login(&self) -> Result<StartedKimiLogin, KimiLoginError> {
        let state = generate_state()?;
        let device_id = generate_device_id()?;
        let device_flow = self.request_device_flow(&device_id).await?;
        let url = trimmed(Some(device_flow.verification_uri_complete))
            .or_else(|| trimmed(Some(device_flow.verification_uri)))
            .ok_or(KimiLoginError::MissingVerificationUrl)?;
        let device_code =
            trimmed(Some(device_flow.device_code)).ok_or(KimiLoginError::MissingDeviceCode)?;

        self.oauth_sessions.register(&state, "kimi")?;
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending.insert(
            state.clone(),
            PendingKimiLogin {
                device_code,
                device_id,
                poll_interval: normalize_poll_interval(device_flow.interval),
                expires_at: Instant::now() + normalize_login_ttl(device_flow.expires_in, self.ttl),
            },
        );

        Ok(StartedKimiLogin { state, url })
    }

    pub async fn complete_login(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<CompletedKimiLogin, KimiLoginError> {
        if !self.oauth_sessions.is_pending(state, Some("kimi"))? {
            self.remove_pending(state);
            return Err(KimiLoginError::SessionNotPending);
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
        let file_name = format!("kimi-{email}.json");
        let file_path = auth_dir.join(&file_name);

        fs::create_dir_all(auth_dir).map_err(KimiLoginError::CreateAuthDir)?;
        let payload = serde_json::json!({
            "id": file_name,
            "provider": "kimi",
            "type": "kimi",
            "email": email,
            "access_token": token.access_token,
            "refresh_token": token.refresh_token,
            "token_type": token.token_type,
            "scope": token.scope,
            "device_id": pending.device_id,
            "last_refresh": rfc3339_now(),
            "expired": token.expired(),
            "timestamp": unix_timestamp_millis(),
        });
        let bytes = serde_json::to_vec_pretty(&payload).map_err(KimiLoginError::EncodeAuthFile)?;
        fs::write(&file_path, bytes).map_err(KimiLoginError::WriteAuthFile)?;

        self.oauth_sessions.complete(state)?;

        Ok(CompletedKimiLogin {
            file_name: file_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_path,
            email,
        })
    }

    fn take_pending_login(&self, state: &str) -> Result<PendingKimiLogin, KimiLoginError> {
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending
            .remove(state)
            .ok_or(KimiLoginError::SessionNotPending)
    }

    fn remove_pending(&self, state: &str) {
        let mut pending = self.lock_pending();
        pending.remove(state);
    }

    fn lock_pending(&self) -> MutexGuard<'_, HashMap<String, PendingKimiLogin>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn build_http_client(&self, request_url: &str) -> Result<Client, reqwest::Error> {
        let mut builder = Client::builder().timeout(Duration::from_secs(30));
        if should_bypass_proxy_for_url(request_url) {
            builder = builder.no_proxy();
        } else if let Some(proxy_url) = &self.default_proxy_url {
            builder = builder.proxy(Proxy::all(proxy_url)?);
        }
        builder.build()
    }

    async fn request_device_flow(
        &self,
        device_id: &str,
    ) -> Result<DeviceFlowResponse, KimiLoginError> {
        let client = self.build_http_client(&self.endpoints.device_code_url)?;
        let response = client
            .post(&self.endpoints.device_code_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .header("X-Msh-Platform", DEFAULT_KIMI_PLATFORM)
            .header("X-Msh-Version", DEFAULT_KIMI_VERSION)
            .header("X-Msh-Device-Name", resolve_device_name())
            .header("X-Msh-Device-Model", resolve_device_model())
            .header("X-Msh-Device-Id", device_id)
            .form(&[("client_id", self.endpoints.client_id.as_str())])
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(KimiLoginError::UnexpectedDeviceCodeStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        serde_json::from_slice(&body).map_err(KimiLoginError::ParseDeviceFlow)
    }

    async fn poll_for_token(
        &self,
        state: &str,
        pending: &PendingKimiLogin,
    ) -> Result<TokenResponse, KimiLoginError> {
        let client = self.build_http_client(&self.endpoints.token_url)?;
        let mut poll_interval = pending.poll_interval;

        loop {
            if Instant::now() >= pending.expires_at {
                return Err(KimiLoginError::AuthenticationTimeout);
            }
            if !self.oauth_sessions.is_pending(state, Some("kimi"))? {
                return Err(KimiLoginError::SessionNotPending);
            }

            let response = client
                .post(&self.endpoints.token_url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .header("X-Msh-Platform", DEFAULT_KIMI_PLATFORM)
                .header("X-Msh-Version", DEFAULT_KIMI_VERSION)
                .header("X-Msh-Device-Name", resolve_device_name())
                .header("X-Msh-Device-Model", resolve_device_model())
                .header("X-Msh-Device-Id", &pending.device_id)
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", self.endpoints.client_id.as_str()),
                    ("device_code", pending.device_code.as_str()),
                ])
                .send()
                .await?;

            let status = response.status();
            let body = response.bytes().await?;
            if !status.is_success() {
                return Err(KimiLoginError::UnexpectedTokenStatus {
                    status: status.as_u16(),
                    body: String::from_utf8_lossy(&body).trim().to_string(),
                });
            }

            let token: TokenResponse =
                serde_json::from_slice(&body).map_err(KimiLoginError::ParseToken)?;
            if !token.error.trim().is_empty() {
                match token.error.trim() {
                    "authorization_pending" => {
                        sleep(poll_interval).await;
                        continue;
                    }
                    "slow_down" => {
                        poll_interval = slow_down_poll_interval(poll_interval);
                        sleep(poll_interval).await;
                        continue;
                    }
                    "expired_token" => return Err(KimiLoginError::DeviceCodeExpired),
                    "access_denied" => return Err(KimiLoginError::AuthorizationDenied),
                    _ => {
                        let description = first_non_empty(&token.error_description, &token.error);
                        return Err(KimiLoginError::PollRejected(description));
                    }
                }
            }

            if token.access_token.trim().is_empty() {
                return Err(KimiLoginError::MissingAccessToken);
            }
            return Ok(token);
        }
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
    error: String,
    #[serde(default)]
    error_description: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: f64,
    #[serde(default)]
    scope: String,
}

fn generate_state() -> Result<String, KimiLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(KimiLoginError::Random)?;
    Ok(format!("kimi-{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn generate_device_id() -> Result<String, KimiLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(KimiLoginError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn purge_expired_pending(pending: &mut HashMap<String, PendingKimiLogin>) {
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

fn session_error_message(error: &KimiLoginError) -> String {
    match error {
        KimiLoginError::CreateAuthDir(_)
        | KimiLoginError::EncodeAuthFile(_)
        | KimiLoginError::WriteAuthFile(_) => "Failed to save authentication tokens".to_string(),
        KimiLoginError::SessionNotPending => "Authentication failed".to_string(),
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

fn resolve_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn resolve_device_model() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

impl TokenResponse {
    fn expired(&self) -> String {
        let now = SystemTime::now();
        let future = now
            .checked_add(Duration::from_secs(self.expires_in.max(0.0) as u64))
            .unwrap_or(now);
        let datetime: chrono::DateTime<chrono::Utc> = future.into();
        datetime.to_rfc3339()
    }
}

#[cfg(test)]
mod tests {
    use super::{KimiLoginEndpoints, KimiLoginService};
    use crate::OAuthSessionStore;
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    async fn spawn_kimi_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local addr");
        let router = Router::new()
            .route(
                "/api/oauth/device_authorization",
                post(|| async {
                    Json(json!({
                        "device_code": "kimi-device-code-123",
                        "verification_uri": "https://auth.kimi.com/device",
                        "verification_uri_complete": "https://auth.kimi.com/device?user_code=KIMI",
                        "expires_in": 600,
                        "interval": 1
                    }))
                }),
            )
            .route(
                "/api/oauth/token",
                post(|| async {
                    Json(json!({
                        "access_token": "kimi-access-token-123",
                        "refresh_token": "kimi-refresh-token-456",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                        "scope": "openid profile"
                    }))
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("kimi server");
        });
        address
    }

    #[tokio::test]
    async fn completes_kimi_login_and_writes_auth_file() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_kimi_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            KimiLoginService::new(sessions.clone(), None).with_endpoints(KimiLoginEndpoints {
                device_code_url: format!("http://{server}/api/oauth/device_authorization"),
                token_url: format!("http://{server}/api/oauth/token"),
                ..KimiLoginEndpoints::default()
            });

        let started = service.start_login().await.expect("start login");
        assert!(started.url.contains("auth.kimi.com/device"));
        assert!(sessions
            .is_pending(&started.state, Some("kimi"))
            .expect("pending"));

        let completed = service
            .complete_login(temp_dir.path(), &started.state)
            .await
            .expect("complete login");

        assert!(completed.file_name.starts_with("kimi-"));
        assert!(completed.file_name.ends_with(".json"));
        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&completed.file_path).expect("auth file"))
                .expect("json");
        assert_eq!(payload["type"].as_str(), Some("kimi"));
        assert_eq!(payload["provider"].as_str(), Some("kimi"));
        assert_eq!(payload["email"].as_str(), Some(completed.email.as_str()));
        assert_eq!(
            payload["access_token"].as_str(),
            Some("kimi-access-token-123")
        );
        assert_eq!(
            payload["refresh_token"].as_str(),
            Some("kimi-refresh-token-456")
        );
        assert_eq!(payload["token_type"].as_str(), Some("Bearer"));
        assert_eq!(payload["scope"].as_str(), Some("openid profile"));
        assert!(payload["device_id"]
            .as_str()
            .is_some_and(|value| !value.trim().is_empty()));
        assert!(sessions.get(&started.state).expect("get session").is_none());
    }

    #[tokio::test]
    async fn completes_kimi_login_with_loopback_endpoints_even_when_proxy_is_configured() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_kimi_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            KimiLoginService::new(sessions.clone(), Some("http://127.0.0.1:9".to_string()))
                .with_endpoints(KimiLoginEndpoints {
                    device_code_url: format!("http://{server}/api/oauth/device_authorization"),
                    token_url: format!("http://{server}/api/oauth/token"),
                    ..KimiLoginEndpoints::default()
                });

        let started = service.start_login().await.expect("start login");
        let completed = service
            .complete_login(temp_dir.path(), &started.state)
            .await
            .expect("complete login");

        assert!(completed.file_name.starts_with("kimi-"));
    }
}
