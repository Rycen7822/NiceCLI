use crate::{
    should_bypass_proxy_for_url, OAuthFlowError, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Proxy};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::time::sleep;
use url::Url;

const DEFAULT_ANTIGRAVITY_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const DEFAULT_ANTIGRAVITY_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_ANTIGRAVITY_USER_INFO_URL: &str =
    "https://www.googleapis.com/oauth2/v1/userinfo?alt=json";
const DEFAULT_ANTIGRAVITY_LOAD_CODE_ASSIST_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const DEFAULT_ANTIGRAVITY_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const DEFAULT_ANTIGRAVITY_CLIENT_SECRET: &str = "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";
const DEFAULT_ANTIGRAVITY_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
const DEFAULT_ANTIGRAVITY_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
];
const ANTIGRAVITY_API_USER_AGENT: &str = "google-api-nodejs-client/9.15.1";
const ANTIGRAVITY_API_CLIENT: &str = "google-cloud-sdk vscode_cloudshelleditor/0.1";
const ANTIGRAVITY_CLIENT_METADATA: &str =
    r#"{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedAntigravityLogin {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedAntigravityLogin {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityLoginEndpoints {
    pub authorize_url: String,
    pub token_url: String,
    pub user_info_url: String,
    pub load_code_assist_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl Default for AntigravityLoginEndpoints {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_ANTIGRAVITY_AUTHORIZE_URL.to_string(),
            token_url: DEFAULT_ANTIGRAVITY_TOKEN_URL.to_string(),
            user_info_url: DEFAULT_ANTIGRAVITY_USER_INFO_URL.to_string(),
            load_code_assist_url: DEFAULT_ANTIGRAVITY_LOAD_CODE_ASSIST_URL.to_string(),
            client_id: DEFAULT_ANTIGRAVITY_CLIENT_ID.to_string(),
            client_secret: DEFAULT_ANTIGRAVITY_CLIENT_SECRET.to_string(),
            redirect_uri: DEFAULT_ANTIGRAVITY_REDIRECT_URI.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum AntigravityLoginError {
    #[error(transparent)]
    OAuthSession(#[from] OAuthFlowError),
    #[error("failed to generate secure random bytes")]
    Random(rand::Error),
    #[error("failed to build antigravity auth url: {0}")]
    BuildAuthUrl(url::ParseError),
    #[error("antigravity oauth flow is not pending")]
    SessionNotPending,
    #[error("authentication timeout. Please restart the authentication process")]
    AuthenticationTimeout,
    #[error("failed to read oauth callback file: {0}")]
    ReadCallbackFile(std::io::Error),
    #[error("failed to parse oauth callback file: {0}")]
    ParseCallbackFile(serde_json::Error),
    #[error("antigravity oauth callback returned an error: {0}")]
    CallbackRejected(String),
    #[error("antigravity oauth callback state does not match")]
    StateMismatch,
    #[error("antigravity oauth authorization code is empty")]
    MissingAuthorizationCode,
    #[error("antigravity token request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("antigravity token exchange returned {status}: {body}")]
    UnexpectedTokenStatus { status: u16, body: String },
    #[error("failed to parse antigravity token response: {0}")]
    ParseToken(serde_json::Error),
    #[error("antigravity token response is missing access_token")]
    MissingAccessToken,
    #[error("antigravity user info returned {status}: {body}")]
    UnexpectedUserInfoStatus { status: u16, body: String },
    #[error("failed to parse antigravity user info response: {0}")]
    ParseUserInfo(serde_json::Error),
    #[error("antigravity user info is missing email")]
    MissingEmail,
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to encode auth file: {0}")]
    EncodeAuthFile(serde_json::Error),
    #[error("failed to write auth file: {0}")]
    WriteAuthFile(std::io::Error),
}

#[derive(Debug, Clone)]
pub struct AntigravityLoginService {
    oauth_sessions: Arc<OAuthSessionStore>,
    default_proxy_url: Option<String>,
    endpoints: AntigravityLoginEndpoints,
    ttl: Duration,
}

impl AntigravityLoginService {
    pub fn new(oauth_sessions: Arc<OAuthSessionStore>, default_proxy_url: Option<String>) -> Self {
        Self {
            oauth_sessions,
            default_proxy_url: trimmed(default_proxy_url),
            endpoints: AntigravityLoginEndpoints::default(),
            ttl: DEFAULT_OAUTH_SESSION_TTL,
        }
    }

    pub fn with_endpoints(mut self, endpoints: AntigravityLoginEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn start_login(&self) -> Result<StartedAntigravityLogin, AntigravityLoginError> {
        let state = generate_state()?;
        let scopes = DEFAULT_ANTIGRAVITY_SCOPES.join(" ");
        let url = Url::parse_with_params(
            &self.endpoints.authorize_url,
            &[
                ("access_type", "offline"),
                ("client_id", self.endpoints.client_id.as_str()),
                ("prompt", "consent"),
                ("redirect_uri", self.endpoints.redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", scopes.as_str()),
                ("state", state.as_str()),
            ],
        )
        .map_err(AntigravityLoginError::BuildAuthUrl)?;

        self.oauth_sessions.register(&state, "antigravity")?;
        Ok(StartedAntigravityLogin {
            state,
            url: url.to_string(),
        })
    }

    pub async fn complete_login(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<CompletedAntigravityLogin, AntigravityLoginError> {
        if !self.oauth_sessions.is_pending(state, Some("antigravity"))? {
            return Err(AntigravityLoginError::SessionNotPending);
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
            let error = AntigravityLoginError::CallbackRejected(callback.error);
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }
        if !callback.state.trim().eq_ignore_ascii_case(state.trim()) {
            let error = AntigravityLoginError::StateMismatch;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }
        if callback.code.trim().is_empty() {
            let error = AntigravityLoginError::MissingAuthorizationCode;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }

        let tokens = match self.exchange_code_for_tokens(&callback.code).await {
            Ok(tokens) => tokens,
            Err(error) => {
                let _ = self
                    .oauth_sessions
                    .set_error(state, &session_error_message(&error));
                return Err(error);
            }
        };

        let email = match self.fetch_user_email(&tokens.access_token).await {
            Ok(email) => email,
            Err(error) => {
                let _ = self
                    .oauth_sessions
                    .set_error(state, &session_error_message(&error));
                return Err(error);
            }
        };

        let project_id = self
            .fetch_project_id(&tokens.access_token)
            .await
            .ok()
            .flatten();
        let file_name = format!("antigravity-{email}.json");
        let file_path = auth_dir.join(&file_name);
        let now = SystemTime::now();
        let expires_at = now
            .checked_add(Duration::from_secs(tokens.expires_in.max(0) as u64))
            .unwrap_or(now);

        fs::create_dir_all(auth_dir).map_err(AntigravityLoginError::CreateAuthDir)?;
        let mut payload = serde_json::Map::new();
        payload.insert("id".to_string(), Value::String(file_name.clone()));
        payload.insert(
            "provider".to_string(),
            Value::String("antigravity".to_string()),
        );
        payload.insert("type".to_string(), Value::String("antigravity".to_string()));
        payload.insert(
            "access_token".to_string(),
            Value::String(tokens.access_token),
        );
        payload.insert(
            "refresh_token".to_string(),
            Value::String(tokens.refresh_token),
        );
        payload.insert(
            "expires_in".to_string(),
            Value::Number(tokens.expires_in.into()),
        );
        payload.insert(
            "timestamp".to_string(),
            Value::Number(unix_timestamp_millis(now).into()),
        );
        payload.insert("expired".to_string(), Value::String(rfc3339(expires_at)));
        payload.insert("email".to_string(), Value::String(email.clone()));
        if let Some(project_id) = &project_id {
            payload.insert("project_id".to_string(), Value::String(project_id.clone()));
        }

        let bytes = serde_json::to_vec_pretty(&Value::Object(payload))
            .map_err(AntigravityLoginError::EncodeAuthFile)?;
        fs::write(&file_path, bytes).map_err(AntigravityLoginError::WriteAuthFile)?;

        self.oauth_sessions.complete(state)?;

        Ok(CompletedAntigravityLogin {
            file_name,
            file_path,
            email,
            project_id,
        })
    }

    async fn wait_for_callback(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<OAuthCallbackFile, AntigravityLoginError> {
        let path = auth_dir.join(format!(".oauth-antigravity-{state}.oauth"));
        let deadline = Instant::now() + self.ttl;

        loop {
            if !self.oauth_sessions.is_pending(state, Some("antigravity"))? {
                return Err(AntigravityLoginError::SessionNotPending);
            }
            if Instant::now() >= deadline {
                return Err(AntigravityLoginError::AuthenticationTimeout);
            }
            if path.exists() {
                let raw =
                    fs::read_to_string(&path).map_err(AntigravityLoginError::ReadCallbackFile)?;
                let _ = fs::remove_file(&path);
                return serde_json::from_str(&raw)
                    .map_err(AntigravityLoginError::ParseCallbackFile);
            }
            sleep(Duration::from_millis(500)).await;
        }
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
    ) -> Result<TokenResponse, AntigravityLoginError> {
        let client = self.build_http_client(&self.endpoints.token_url)?;
        let response = client
            .post(&self.endpoints.token_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&[
                ("code", code),
                ("client_id", self.endpoints.client_id.as_str()),
                ("client_secret", self.endpoints.client_secret.as_str()),
                ("redirect_uri", self.endpoints.redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(AntigravityLoginError::UnexpectedTokenStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let token: TokenResponse =
            serde_json::from_slice(&body).map_err(AntigravityLoginError::ParseToken)?;
        if token.access_token.trim().is_empty() {
            return Err(AntigravityLoginError::MissingAccessToken);
        }
        Ok(token)
    }

    async fn fetch_user_email(&self, access_token: &str) -> Result<String, AntigravityLoginError> {
        let client = self.build_http_client(&self.endpoints.user_info_url)?;
        let response = client
            .get(&self.endpoints.user_info_url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(AntigravityLoginError::UnexpectedUserInfoStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let user_info: UserInfoResponse =
            serde_json::from_slice(&body).map_err(AntigravityLoginError::ParseUserInfo)?;
        trimmed(Some(user_info.email)).ok_or(AntigravityLoginError::MissingEmail)
    }

    async fn fetch_project_id(&self, access_token: &str) -> Result<Option<String>, reqwest::Error> {
        let client = self.build_http_client(&self.endpoints.load_code_assist_url)?;
        let response = client
            .post(&self.endpoints.load_code_assist_url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(CONTENT_TYPE, "application/json")
            .header("User-Agent", ANTIGRAVITY_API_USER_AGENT)
            .header("X-Goog-Api-Client", ANTIGRAVITY_API_CLIENT)
            .header("Client-Metadata", ANTIGRAVITY_CLIENT_METADATA)
            .json(&json!({
                "metadata": {
                    "ideType": "ANTIGRAVITY",
                    "platform": "PLATFORM_UNSPECIFIED",
                    "pluginType": "GEMINI"
                }
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let body = response.bytes().await?;
        let value: Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        Ok(extract_project_id(&value))
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
}

#[derive(Debug, Default, Deserialize)]
struct UserInfoResponse {
    #[serde(default)]
    email: String,
}

fn generate_state() -> Result<String, AntigravityLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(AntigravityLoginError::Random)?;
    Ok(format!("antigravity-{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn unix_timestamp_millis(now: SystemTime) -> i64 {
    now.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn rfc3339(time: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = time.into();
    datetime.to_rfc3339()
}

fn extract_project_id(value: &Value) -> Option<String> {
    if let Some(project_id) = value
        .get("cloudaicompanionProject")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(project_id.to_string());
    }

    value
        .get("cloudaicompanionProject")
        .and_then(Value::as_object)
        .and_then(|project| project.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_error_message(error: &AntigravityLoginError) -> String {
    match error {
        AntigravityLoginError::CreateAuthDir(_)
        | AntigravityLoginError::EncodeAuthFile(_)
        | AntigravityLoginError::WriteAuthFile(_) => {
            "Failed to save authentication tokens".to_string()
        }
        AntigravityLoginError::AuthenticationTimeout => error.to_string(),
        AntigravityLoginError::SessionNotPending => "Authentication failed".to_string(),
        _ => "Authentication failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{AntigravityLoginEndpoints, AntigravityLoginService};
    use crate::{write_oauth_callback_file_for_pending_session, OAuthSessionStore};
    use axum::{
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, Duration};

    async fn spawn_antigravity_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local addr");
        let router = Router::new()
            .route(
                "/oauth2/token",
                post(|| async {
                    Json(serde_json::json!({
                        "access_token": "antigravity-access-token",
                        "refresh_token": "antigravity-refresh-token",
                        "expires_in": 3600
                    }))
                }),
            )
            .route(
                "/userinfo",
                get(|| async {
                    Json(serde_json::json!({
                        "email": "antigravity@example.com"
                    }))
                }),
            )
            .route(
                "/v1internal:loadCodeAssist",
                post(|| async {
                    Json(serde_json::json!({
                        "cloudaicompanionProject": {
                            "id": "project-123"
                        }
                    }))
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("antigravity server");
        });
        address
    }

    #[tokio::test]
    async fn completes_antigravity_login_after_callback_file_is_written() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_antigravity_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service = AntigravityLoginService::new(sessions.clone(), None).with_endpoints(
            AntigravityLoginEndpoints {
                authorize_url: format!("http://{server}/oauth2/authorize"),
                token_url: format!("http://{server}/oauth2/token"),
                user_info_url: format!("http://{server}/userinfo"),
                load_code_assist_url: format!("http://{server}/v1internal:loadCodeAssist"),
                ..AntigravityLoginEndpoints::default()
            },
        );

        let started = service.start_login().expect("start login");
        assert!(started.url.contains("/oauth2/authorize"));

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
            "antigravity",
            &started.state,
            "auth-code-123",
            "",
        )
        .expect("write callback file");

        let completed = completion
            .await
            .expect("join task")
            .expect("complete login");

        assert_eq!(completed.email, "antigravity@example.com");
        assert_eq!(completed.project_id.as_deref(), Some("project-123"));
        assert_eq!(
            completed.file_name,
            "antigravity-antigravity@example.com.json"
        );

        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&completed.file_path).expect("auth file"))
                .expect("json");
        assert_eq!(payload["type"].as_str(), Some("antigravity"));
        assert_eq!(payload["provider"].as_str(), Some("antigravity"));
        assert_eq!(payload["email"].as_str(), Some("antigravity@example.com"));
        assert_eq!(payload["project_id"].as_str(), Some("project-123"));
        assert_eq!(
            payload["access_token"].as_str(),
            Some("antigravity-access-token")
        );
        assert!(sessions.get(&started.state).expect("get session").is_none());
    }

    #[tokio::test]
    async fn completes_antigravity_login_with_loopback_endpoints_even_when_proxy_is_configured() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_antigravity_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            AntigravityLoginService::new(sessions.clone(), Some("http://127.0.0.1:9".to_string()))
                .with_endpoints(AntigravityLoginEndpoints {
                    authorize_url: format!("http://{server}/oauth2/authorize"),
                    token_url: format!("http://{server}/oauth2/token"),
                    user_info_url: format!("http://{server}/userinfo"),
                    load_code_assist_url: format!("http://{server}/v1internal:loadCodeAssist"),
                    ..AntigravityLoginEndpoints::default()
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
            "antigravity",
            &started.state,
            "auth-code-123",
            "",
        )
        .expect("write callback file");

        let completed = completion
            .await
            .expect("join task")
            .expect("complete login");

        assert_eq!(completed.email, "antigravity@example.com");
        assert_eq!(completed.project_id.as_deref(), Some("project-123"));
    }
}
