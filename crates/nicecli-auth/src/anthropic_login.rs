use crate::{
    should_bypass_proxy_for_url, OAuthFlowError, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::{Client, Proxy};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::time::sleep;
use url::Url;

const DEFAULT_ANTHROPIC_AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const DEFAULT_ANTHROPIC_TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
const DEFAULT_ANTHROPIC_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const DEFAULT_ANTHROPIC_REDIRECT_URI: &str = "http://localhost:54545/callback";
const DEFAULT_ANTHROPIC_SCOPE: &str = "org:create_api_key user:profile user:inference";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedAnthropicLogin {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedAnthropicLogin {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicLoginEndpoints {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
}

impl Default for AnthropicLoginEndpoints {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_ANTHROPIC_AUTHORIZE_URL.to_string(),
            token_url: DEFAULT_ANTHROPIC_TOKEN_URL.to_string(),
            client_id: DEFAULT_ANTHROPIC_CLIENT_ID.to_string(),
            redirect_uri: DEFAULT_ANTHROPIC_REDIRECT_URI.to_string(),
            scope: DEFAULT_ANTHROPIC_SCOPE.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum AnthropicLoginError {
    #[error(transparent)]
    OAuthSession(#[from] OAuthFlowError),
    #[error("failed to generate secure random bytes")]
    Random(rand::Error),
    #[error("failed to build anthropic auth url: {0}")]
    BuildAuthUrl(url::ParseError),
    #[error("anthropic oauth flow is not pending")]
    SessionNotPending,
    #[error("authentication timeout. Please restart the authentication process")]
    AuthenticationTimeout,
    #[error("failed to read oauth callback file: {0}")]
    ReadCallbackFile(std::io::Error),
    #[error("failed to parse oauth callback file: {0}")]
    ParseCallbackFile(serde_json::Error),
    #[error("anthropic oauth callback returned an error: {0}")]
    CallbackRejected(String),
    #[error("anthropic oauth callback state does not match")]
    StateMismatch,
    #[error("anthropic oauth authorization code is empty")]
    MissingAuthorizationCode,
    #[error("anthropic token request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("anthropic token exchange returned {status}: {body}")]
    UnexpectedTokenStatus { status: u16, body: String },
    #[error("failed to parse anthropic token response: {0}")]
    ParseToken(serde_json::Error),
    #[error("anthropic token response is missing access_token")]
    MissingAccessToken,
    #[error("anthropic token response is missing email")]
    MissingEmail,
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to encode auth file: {0}")]
    EncodeAuthFile(serde_json::Error),
    #[error("failed to write auth file: {0}")]
    WriteAuthFile(std::io::Error),
}

#[derive(Debug, Clone)]
struct PendingAnthropicLogin {
    pkce: PkceCodes,
    expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

#[derive(Debug, Clone)]
pub struct AnthropicLoginService {
    oauth_sessions: Arc<OAuthSessionStore>,
    default_proxy_url: Option<String>,
    endpoints: AnthropicLoginEndpoints,
    ttl: Duration,
    pending: Arc<Mutex<HashMap<String, PendingAnthropicLogin>>>,
}

impl AnthropicLoginService {
    pub fn new(oauth_sessions: Arc<OAuthSessionStore>, default_proxy_url: Option<String>) -> Self {
        Self {
            oauth_sessions,
            default_proxy_url: trimmed(default_proxy_url),
            endpoints: AnthropicLoginEndpoints::default(),
            ttl: DEFAULT_OAUTH_SESSION_TTL,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_endpoints(mut self, endpoints: AnthropicLoginEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn start_login(&self) -> Result<StartedAnthropicLogin, AnthropicLoginError> {
        let state = generate_state()?;
        let pkce = generate_pkce_codes()?;
        let url = Url::parse_with_params(
            &self.endpoints.authorize_url,
            &[
                ("code", "true"),
                ("client_id", self.endpoints.client_id.as_str()),
                ("response_type", "code"),
                ("redirect_uri", self.endpoints.redirect_uri.as_str()),
                ("scope", self.endpoints.scope.as_str()),
                ("code_challenge", pkce.code_challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("state", state.as_str()),
            ],
        )
        .map_err(AnthropicLoginError::BuildAuthUrl)?;

        self.oauth_sessions.register(&state, "anthropic")?;
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending.insert(
            state.clone(),
            PendingAnthropicLogin {
                pkce,
                expires_at: Instant::now() + self.ttl,
            },
        );

        Ok(StartedAnthropicLogin {
            state,
            url: url.to_string(),
        })
    }

    pub async fn complete_login(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<CompletedAnthropicLogin, AnthropicLoginError> {
        if !self.oauth_sessions.is_pending(state, Some("anthropic"))? {
            self.remove_pending(state);
            return Err(AnthropicLoginError::SessionNotPending);
        }

        let callback = match self.wait_for_callback(auth_dir, state).await {
            Ok(callback) => callback,
            Err(error) => {
                let _ = self
                    .oauth_sessions
                    .set_error(state, &session_error_message(&error));
                return Err(error);
            }
        };

        if !callback.error.trim().is_empty() {
            let error = AnthropicLoginError::CallbackRejected(callback.error);
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }
        if !callback.state.trim().eq_ignore_ascii_case(state.trim()) {
            let error = AnthropicLoginError::StateMismatch;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }
        if callback.code.trim().is_empty() {
            let error = AnthropicLoginError::MissingAuthorizationCode;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }

        let pending = self.take_pending_login(state)?;
        let tokens = match self
            .exchange_code_for_tokens(&callback.code, state, &pending.pkce)
            .await
        {
            Ok(tokens) => tokens,
            Err(error) => {
                let _ = self
                    .oauth_sessions
                    .set_error(state, &session_error_message(&error));
                return Err(error);
            }
        };

        let email = trimmed(Some(tokens.account.email_address)).ok_or_else(|| {
            let error = AnthropicLoginError::MissingEmail;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            error
        })?;

        let file_name = format!("claude-{email}.json");
        let file_path = auth_dir.join(&file_name);
        fs::create_dir_all(auth_dir).map_err(AnthropicLoginError::CreateAuthDir)?;
        let payload = serde_json::json!({
            "id": file_name,
            "provider": "claude",
            "type": "claude",
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "last_refresh": rfc3339_now(),
            "email": email,
            "expired": rfc3339_after_seconds(tokens.expires_in),
            "account_uuid": tokens.account.uuid,
            "organization_uuid": tokens.organization.uuid,
            "organization_name": tokens.organization.name,
        });
        let bytes =
            serde_json::to_vec_pretty(&payload).map_err(AnthropicLoginError::EncodeAuthFile)?;
        fs::write(&file_path, bytes).map_err(AnthropicLoginError::WriteAuthFile)?;

        self.oauth_sessions.complete(state)?;

        Ok(CompletedAnthropicLogin {
            file_name: file_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_path,
            email,
        })
    }

    async fn wait_for_callback(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<OAuthCallbackFile, AnthropicLoginError> {
        let path = auth_dir.join(format!(".oauth-anthropic-{state}.oauth"));
        let deadline = Instant::now() + self.ttl;

        loop {
            if !self.oauth_sessions.is_pending(state, Some("anthropic"))? {
                return Err(AnthropicLoginError::SessionNotPending);
            }
            if Instant::now() >= deadline {
                return Err(AnthropicLoginError::AuthenticationTimeout);
            }
            if path.exists() {
                let raw =
                    fs::read_to_string(&path).map_err(AnthropicLoginError::ReadCallbackFile)?;
                let _ = fs::remove_file(&path);
                return serde_json::from_str(&raw).map_err(AnthropicLoginError::ParseCallbackFile);
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    fn take_pending_login(
        &self,
        state: &str,
    ) -> Result<PendingAnthropicLogin, AnthropicLoginError> {
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending
            .remove(state)
            .ok_or(AnthropicLoginError::SessionNotPending)
    }

    fn remove_pending(&self, state: &str) {
        let mut pending = self.lock_pending();
        pending.remove(state);
    }

    fn lock_pending(&self) -> MutexGuard<'_, HashMap<String, PendingAnthropicLogin>> {
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

    async fn exchange_code_for_tokens(
        &self,
        code: &str,
        state: &str,
        pkce: &PkceCodes,
    ) -> Result<TokenResponse, AnthropicLoginError> {
        let client = self.build_http_client(&self.endpoints.token_url)?;
        let (parsed_code, parsed_state) = parse_code_and_state(code);
        let payload = serde_json::json!({
            "code": parsed_code,
            "state": parsed_state.unwrap_or_else(|| state.trim().to_string()),
            "grant_type": "authorization_code",
            "client_id": self.endpoints.client_id.as_str(),
            "redirect_uri": self.endpoints.redirect_uri.as_str(),
            "code_verifier": pkce.code_verifier.as_str(),
        });

        let response = client
            .post(&self.endpoints.token_url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(AnthropicLoginError::UnexpectedTokenStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let tokens: TokenResponse =
            serde_json::from_slice(&body).map_err(AnthropicLoginError::ParseToken)?;
        if tokens.access_token.trim().is_empty() {
            return Err(AnthropicLoginError::MissingAccessToken);
        }
        Ok(tokens)
    }
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackFile {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Default, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    account: AccountInfo,
    #[serde(default)]
    organization: OrganizationInfo,
}

#[derive(Debug, Default, Deserialize)]
struct AccountInfo {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    email_address: String,
}

#[derive(Debug, Default, Deserialize)]
struct OrganizationInfo {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    name: String,
}

fn generate_state() -> Result<String, AnthropicLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(AnthropicLoginError::Random)?;
    Ok(format!("anthropic-{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn generate_pkce_codes() -> Result<PkceCodes, AnthropicLoginError> {
    let mut verifier_bytes = [0_u8; 96];
    OsRng
        .try_fill_bytes(&mut verifier_bytes)
        .map_err(AnthropicLoginError::Random)?;
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);
    Ok(PkceCodes {
        code_verifier,
        code_challenge,
    })
}

fn parse_code_and_state(code: &str) -> (String, Option<String>) {
    let mut parts = code.trim().split('#');
    let parsed_code = parts.next().unwrap_or_default().trim().to_string();
    let parsed_state = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    (parsed_code, parsed_state)
}

fn purge_expired_pending(pending: &mut HashMap<String, PendingAnthropicLogin>) {
    let now = Instant::now();
    pending.retain(|_, entry| entry.expires_at > now);
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn rfc3339_now() -> String {
    let now = SystemTime::now();
    let datetime: chrono::DateTime<chrono::Utc> = now.into();
    datetime.to_rfc3339()
}

fn rfc3339_after_seconds(seconds: i64) -> String {
    let now = SystemTime::now();
    let future = now
        .checked_add(Duration::from_secs(seconds.max(0) as u64))
        .unwrap_or(now);
    let datetime: chrono::DateTime<chrono::Utc> = future.into();
    datetime.to_rfc3339()
}

fn session_error_message(error: &AnthropicLoginError) -> String {
    match error {
        AnthropicLoginError::CreateAuthDir(_)
        | AnthropicLoginError::EncodeAuthFile(_)
        | AnthropicLoginError::WriteAuthFile(_) => {
            "Failed to save authentication tokens".to_string()
        }
        AnthropicLoginError::AuthenticationTimeout => error.to_string(),
        AnthropicLoginError::SessionNotPending => "Authentication failed".to_string(),
        _ => "Authentication failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{AnthropicLoginEndpoints, AnthropicLoginService};
    use crate::{write_oauth_callback_file_for_pending_session, OAuthSessionStore};
    use axum::{routing::post, Json, Router};
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, Duration};

    async fn spawn_anthropic_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local addr");
        let router = Router::new().route(
            "/v1/oauth/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "claude-access-token",
                    "refresh_token": "claude-refresh-token",
                    "expires_in": 3600,
                    "organization": {
                        "uuid": "org-123",
                        "name": "Claude Org"
                    },
                    "account": {
                        "uuid": "acct-456",
                        "email_address": "claude@example.com"
                    }
                }))
            }),
        );
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("anthropic server");
        });
        address
    }

    #[tokio::test]
    async fn completes_anthropic_login_after_callback_file_is_written() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_anthropic_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service = AnthropicLoginService::new(sessions.clone(), None).with_endpoints(
            AnthropicLoginEndpoints {
                authorize_url: format!("http://{server}/oauth/authorize"),
                token_url: format!("http://{server}/v1/oauth/token"),
                ..AnthropicLoginEndpoints::default()
            },
        );

        let started = service.start_login().expect("start login");
        assert!(started.url.contains("/oauth/authorize"));

        let service_clone = service.clone();
        let auth_dir = temp_dir.path().to_path_buf();
        let state_value = started.state.clone();
        let completion =
            tokio::spawn(
                async move { service_clone.complete_login(&auth_dir, &state_value).await },
            );

        sleep(Duration::from_millis(50)).await;
        write_oauth_callback_file_for_pending_session(
            temp_dir.path(),
            sessions.as_ref(),
            "anthropic",
            &started.state,
            "auth-code-123",
            "",
        )
        .expect("write callback file");

        let completed = completion
            .await
            .expect("join task")
            .expect("complete login");

        assert_eq!(completed.email, "claude@example.com");
        assert_eq!(completed.file_name, "claude-claude@example.com.json");

        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&completed.file_path).expect("auth file"))
                .expect("json");
        assert_eq!(payload["type"].as_str(), Some("claude"));
        assert_eq!(payload["provider"].as_str(), Some("claude"));
        assert_eq!(payload["email"].as_str(), Some("claude@example.com"));
        assert_eq!(payload["organization_name"].as_str(), Some("Claude Org"));
        assert_eq!(
            payload["access_token"].as_str(),
            Some("claude-access-token")
        );
        assert!(sessions.get(&started.state).expect("get session").is_none());
    }

    #[tokio::test]
    async fn completes_anthropic_login_with_loopback_token_endpoint_even_when_proxy_is_configured()
    {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_anthropic_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            AnthropicLoginService::new(sessions.clone(), Some("http://127.0.0.1:9".to_string()))
                .with_endpoints(AnthropicLoginEndpoints {
                    authorize_url: format!("http://{server}/oauth/authorize"),
                    token_url: format!("http://{server}/v1/oauth/token"),
                    ..AnthropicLoginEndpoints::default()
                });

        let started = service.start_login().expect("start login");
        let service_clone = service.clone();
        let auth_dir = temp_dir.path().to_path_buf();
        let state_value = started.state.clone();
        let completion =
            tokio::spawn(
                async move { service_clone.complete_login(&auth_dir, &state_value).await },
            );

        sleep(Duration::from_millis(50)).await;
        write_oauth_callback_file_for_pending_session(
            temp_dir.path(),
            sessions.as_ref(),
            "anthropic",
            &started.state,
            "auth-code-123",
            "",
        )
        .expect("write callback file");

        let completed = completion
            .await
            .expect("join task")
            .expect("complete login");

        assert_eq!(completed.email, "claude@example.com");
    }
}
