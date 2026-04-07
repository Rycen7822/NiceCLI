use crate::{
    should_bypass_proxy_for_url, OAuthFlowError, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Proxy};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::time::sleep;
use url::Url;

const DEFAULT_GEMINI_CLI_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const DEFAULT_GEMINI_CLI_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_GEMINI_CLI_USER_INFO_URL: &str =
    "https://www.googleapis.com/oauth2/v1/userinfo?alt=json";
const DEFAULT_GEMINI_CLI_PROJECTS_URL: &str =
    "https://cloudresourcemanager.googleapis.com/v1/projects";
const DEFAULT_GEMINI_CLI_SERVICE_USAGE_URL: &str = "https://serviceusage.googleapis.com";
const DEFAULT_GEMINI_CLI_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const DEFAULT_GEMINI_CLI_CLIENT_SECRET: &str = "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";
const DEFAULT_GEMINI_CLI_REDIRECT_URI: &str = "http://localhost:8085/oauth2callback";
const DEFAULT_GEMINI_CLI_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
];
const GEMINI_REQUIRED_SERVICE: &str = "cloudaicompanion.googleapis.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedGeminiCliLogin {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedGeminiCliLogin {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiCliLoginEndpoints {
    pub authorize_url: String,
    pub token_url: String,
    pub user_info_url: String,
    pub projects_url: String,
    pub service_usage_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

impl Default for GeminiCliLoginEndpoints {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_GEMINI_CLI_AUTHORIZE_URL.to_string(),
            token_url: DEFAULT_GEMINI_CLI_TOKEN_URL.to_string(),
            user_info_url: DEFAULT_GEMINI_CLI_USER_INFO_URL.to_string(),
            projects_url: DEFAULT_GEMINI_CLI_PROJECTS_URL.to_string(),
            service_usage_url: DEFAULT_GEMINI_CLI_SERVICE_USAGE_URL.to_string(),
            client_id: DEFAULT_GEMINI_CLI_CLIENT_ID.to_string(),
            client_secret: DEFAULT_GEMINI_CLI_CLIENT_SECRET.to_string(),
            redirect_uri: DEFAULT_GEMINI_CLI_REDIRECT_URI.to_string(),
            scopes: DEFAULT_GEMINI_CLI_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Error)]
pub enum GeminiCliLoginError {
    #[error(transparent)]
    OAuthSession(#[from] OAuthFlowError),
    #[error("failed to generate secure random bytes")]
    Random(rand::Error),
    #[error("failed to build gemini auth url: {0}")]
    BuildAuthUrl(url::ParseError),
    #[error("gemini oauth flow is not pending")]
    SessionNotPending,
    #[error("authentication timeout. Please restart the authentication process")]
    AuthenticationTimeout,
    #[error("failed to read oauth callback file: {0}")]
    ReadCallbackFile(std::io::Error),
    #[error("failed to parse oauth callback file: {0}")]
    ParseCallbackFile(serde_json::Error),
    #[error("gemini oauth callback returned an error: {0}")]
    CallbackRejected(String),
    #[error("gemini oauth callback state does not match")]
    StateMismatch,
    #[error("gemini oauth authorization code is empty")]
    MissingAuthorizationCode,
    #[error("gemini token request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("gemini token exchange returned {status}: {body}")]
    UnexpectedTokenStatus { status: u16, body: String },
    #[error("failed to parse gemini token response: {0}")]
    ParseToken(serde_json::Error),
    #[error("gemini token response is missing access_token")]
    MissingAccessToken,
    #[error("gemini user info returned {status}: {body}")]
    UnexpectedUserInfoStatus { status: u16, body: String },
    #[error("failed to parse gemini user info response: {0}")]
    ParseUserInfo(serde_json::Error),
    #[error("gemini user info is missing email")]
    MissingEmail,
    #[error("gemini project list returned {status}: {body}")]
    UnexpectedProjectsStatus { status: u16, body: String },
    #[error("failed to parse gemini project list response: {0}")]
    ParseProjects(serde_json::Error),
    #[error("no Google Cloud projects available for this account")]
    MissingProjectId,
    #[error("gemini service check returned {status}: {body}")]
    UnexpectedServiceStatus { status: u16, body: String },
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to encode auth file: {0}")]
    EncodeAuthFile(serde_json::Error),
    #[error("failed to write auth file: {0}")]
    WriteAuthFile(std::io::Error),
}

#[derive(Debug, Clone)]
struct PendingGeminiCliLogin {
    requested_project_id: Option<String>,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct GeminiCliLoginService {
    oauth_sessions: Arc<OAuthSessionStore>,
    default_proxy_url: Option<String>,
    endpoints: GeminiCliLoginEndpoints,
    ttl: Duration,
    pending: Arc<Mutex<HashMap<String, PendingGeminiCliLogin>>>,
}

impl GeminiCliLoginService {
    pub fn new(oauth_sessions: Arc<OAuthSessionStore>, default_proxy_url: Option<String>) -> Self {
        Self {
            oauth_sessions,
            default_proxy_url: trimmed(default_proxy_url),
            endpoints: GeminiCliLoginEndpoints::default(),
            ttl: DEFAULT_OAUTH_SESSION_TTL,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_endpoints(mut self, endpoints: GeminiCliLoginEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn start_login(
        &self,
        requested_project_id: Option<&str>,
    ) -> Result<StartedGeminiCliLogin, GeminiCliLoginError> {
        let state = generate_state()?;
        let scopes = self.endpoints.scopes.join(" ");
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
        .map_err(GeminiCliLoginError::BuildAuthUrl)?;

        self.oauth_sessions.register(&state, "gemini")?;
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending.insert(
            state.clone(),
            PendingGeminiCliLogin {
                requested_project_id: requested_project_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                expires_at: Instant::now() + self.ttl,
            },
        );

        Ok(StartedGeminiCliLogin {
            state,
            url: url.to_string(),
        })
    }

    pub async fn complete_login(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<CompletedGeminiCliLogin, GeminiCliLoginError> {
        if !self.oauth_sessions.is_pending(state, Some("gemini"))? {
            self.remove_pending(state);
            return Err(GeminiCliLoginError::SessionNotPending);
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
            let error = GeminiCliLoginError::CallbackRejected(callback.error);
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }
        if !callback.state.trim().eq_ignore_ascii_case(state.trim()) {
            let error = GeminiCliLoginError::StateMismatch;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }
        if callback.code.trim().is_empty() {
            let error = GeminiCliLoginError::MissingAuthorizationCode;
            let _ = self
                .oauth_sessions
                .set_error(state, &session_error_message(&error));
            return Err(error);
        }

        let pending = self.take_pending_login(state)?;
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

        let auto = pending.requested_project_id.is_none();
        let project_id = match pending.requested_project_id {
            Some(project_id) => project_id,
            None => match self.fetch_first_project_id(&tokens.access_token).await {
                Ok(project_id) => project_id,
                Err(error) => {
                    let _ = self
                        .oauth_sessions
                        .set_error(state, &session_error_message(&error));
                    return Err(error);
                }
            },
        };

        let checked = match self
            .ensure_required_service_enabled(&tokens.access_token, &project_id)
            .await
        {
            Ok(checked) => checked,
            Err(error) => {
                let _ = self
                    .oauth_sessions
                    .set_error(state, &session_error_message(&error));
                return Err(error);
            }
        };

        let file_name = credential_file_name(&email, &project_id, true);
        let file_path = auth_dir.join(&file_name);
        let token_map = build_token_map(&self.endpoints, &tokens);

        fs::create_dir_all(auth_dir).map_err(GeminiCliLoginError::CreateAuthDir)?;
        let payload = serde_json::json!({
            "id": file_name,
            "provider": "gemini",
            "type": "gemini",
            "token": token_map,
            "project_id": project_id,
            "email": email,
            "auto": auto,
            "checked": checked,
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "token_type": tokens.token_type,
            "expiry": expiry_string(tokens.expires_in),
        });
        let bytes =
            serde_json::to_vec_pretty(&payload).map_err(GeminiCliLoginError::EncodeAuthFile)?;
        fs::write(&file_path, bytes).map_err(GeminiCliLoginError::WriteAuthFile)?;

        self.oauth_sessions.complete(state)?;

        Ok(CompletedGeminiCliLogin {
            file_name: file_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_path,
            email,
            project_id,
        })
    }

    async fn wait_for_callback(
        &self,
        auth_dir: &Path,
        state: &str,
    ) -> Result<OAuthCallbackFile, GeminiCliLoginError> {
        let path = auth_dir.join(format!(".oauth-gemini-{state}.oauth"));
        let deadline = Instant::now() + self.ttl;

        loop {
            if !self.oauth_sessions.is_pending(state, Some("gemini"))? {
                return Err(GeminiCliLoginError::SessionNotPending);
            }
            if Instant::now() >= deadline {
                return Err(GeminiCliLoginError::AuthenticationTimeout);
            }
            if path.exists() {
                let raw =
                    fs::read_to_string(&path).map_err(GeminiCliLoginError::ReadCallbackFile)?;
                let _ = fs::remove_file(&path);
                return serde_json::from_str(&raw).map_err(GeminiCliLoginError::ParseCallbackFile);
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    fn take_pending_login(
        &self,
        state: &str,
    ) -> Result<PendingGeminiCliLogin, GeminiCliLoginError> {
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending
            .remove(state)
            .ok_or(GeminiCliLoginError::SessionNotPending)
    }

    fn remove_pending(&self, state: &str) {
        let mut pending = self.lock_pending();
        pending.remove(state);
    }

    fn lock_pending(&self) -> MutexGuard<'_, HashMap<String, PendingGeminiCliLogin>> {
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
    ) -> Result<TokenResponse, GeminiCliLoginError> {
        let client = self.build_http_client(&self.endpoints.token_url)?;
        let response = client
            .post(&self.endpoints.token_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, "application/json")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.trim()),
                ("redirect_uri", self.endpoints.redirect_uri.as_str()),
                ("client_id", self.endpoints.client_id.as_str()),
                ("client_secret", self.endpoints.client_secret.as_str()),
            ])
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(GeminiCliLoginError::UnexpectedTokenStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let token: TokenResponse =
            serde_json::from_slice(&body).map_err(GeminiCliLoginError::ParseToken)?;
        if token.access_token.trim().is_empty() {
            return Err(GeminiCliLoginError::MissingAccessToken);
        }
        Ok(token)
    }

    async fn fetch_user_email(&self, access_token: &str) -> Result<String, GeminiCliLoginError> {
        let client = self.build_http_client(&self.endpoints.user_info_url)?;
        let response = client
            .get(&self.endpoints.user_info_url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(GeminiCliLoginError::UnexpectedUserInfoStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let user_info: UserInfoResponse =
            serde_json::from_slice(&body).map_err(GeminiCliLoginError::ParseUserInfo)?;
        trimmed(Some(user_info.email)).ok_or(GeminiCliLoginError::MissingEmail)
    }

    async fn fetch_first_project_id(
        &self,
        access_token: &str,
    ) -> Result<String, GeminiCliLoginError> {
        let client = self.build_http_client(&self.endpoints.projects_url)?;
        let response = client
            .get(&self.endpoints.projects_url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(GeminiCliLoginError::UnexpectedProjectsStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let projects: ProjectsResponse =
            serde_json::from_slice(&body).map_err(GeminiCliLoginError::ParseProjects)?;
        projects
            .projects
            .into_iter()
            .find_map(|project| trimmed(Some(project.project_id)))
            .ok_or(GeminiCliLoginError::MissingProjectId)
    }

    async fn ensure_required_service_enabled(
        &self,
        access_token: &str,
        project_id: &str,
    ) -> Result<bool, GeminiCliLoginError> {
        let service_url = format!(
            "{}/v1/projects/{}/services/{}",
            self.endpoints.service_usage_url.trim_end_matches('/'),
            project_id.trim(),
            GEMINI_REQUIRED_SERVICE
        );
        let enable_url = format!("{service_url}:enable");
        let client = self.build_http_client(&service_url)?;

        let check_response = client
            .get(&service_url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?;

        let check_status = check_response.status();
        let check_body = check_response.bytes().await?;
        if check_status.is_success() {
            let value: Value = serde_json::from_slice(&check_body).unwrap_or_default();
            if value
                .get("state")
                .and_then(Value::as_str)
                .map(|state| state.eq_ignore_ascii_case("ENABLED"))
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        let enable_response = client
            .post(&enable_url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(CONTENT_TYPE, "application/json")
            .body("{}")
            .send()
            .await?;

        let enable_status = enable_response.status();
        let enable_body = enable_response.bytes().await?;
        if enable_status.is_success() {
            return Ok(true);
        }

        let body_text = String::from_utf8_lossy(&enable_body).trim().to_string();
        if enable_status.as_u16() == 400
            && body_text.to_ascii_lowercase().contains("already enabled")
        {
            return Ok(true);
        }

        Err(GeminiCliLoginError::UnexpectedServiceStatus {
            status: enable_status.as_u16(),
            body: body_text,
        })
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
    token_type: String,
    #[serde(default)]
    expires_in: i64,
}

#[derive(Debug, Default, Deserialize)]
struct UserInfoResponse {
    #[serde(default)]
    email: String,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectsResponse {
    #[serde(default)]
    projects: Vec<ProjectEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectEntry {
    #[serde(default)]
    #[serde(rename = "projectId")]
    project_id: String,
}

fn generate_state() -> Result<String, GeminiCliLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(GeminiCliLoginError::Random)?;
    Ok(format!("gemini-{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn purge_expired_pending(pending: &mut HashMap<String, PendingGeminiCliLogin>) {
    let now = Instant::now();
    pending.retain(|_, entry| entry.expires_at > now);
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn credential_file_name(email: &str, project_id: &str, include_provider_prefix: bool) -> String {
    let email = email.trim();
    let project = project_id.trim();
    if project.eq_ignore_ascii_case("all") || project.contains(',') {
        return format!("gemini-{email}-all.json");
    }
    let prefix = if include_provider_prefix {
        "gemini-"
    } else {
        ""
    };
    format!("{prefix}{email}-{project}.json")
}

fn build_token_map(endpoints: &GeminiCliLoginEndpoints, tokens: &TokenResponse) -> Value {
    json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "token_type": tokens.token_type,
        "expiry": expiry_string(tokens.expires_in),
        "token_uri": endpoints.token_url.as_str(),
        "client_id": endpoints.client_id.as_str(),
        "client_secret": endpoints.client_secret.as_str(),
        "scopes": &endpoints.scopes,
        "universe_domain": "googleapis.com",
    })
}

fn expiry_string(expires_in: i64) -> String {
    let now = SystemTime::now();
    let future = now
        .checked_add(Duration::from_secs(expires_in.max(0) as u64))
        .unwrap_or(now);
    let datetime: chrono::DateTime<chrono::Utc> = future.into();
    datetime.to_rfc3339()
}

fn session_error_message(error: &GeminiCliLoginError) -> String {
    match error {
        GeminiCliLoginError::CreateAuthDir(_)
        | GeminiCliLoginError::EncodeAuthFile(_)
        | GeminiCliLoginError::WriteAuthFile(_) => {
            "Failed to save authentication tokens".to_string()
        }
        GeminiCliLoginError::AuthenticationTimeout => error.to_string(),
        GeminiCliLoginError::SessionNotPending => "Authentication failed".to_string(),
        _ => "Authentication failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{GeminiCliLoginEndpoints, GeminiCliLoginService};
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

    async fn spawn_gemini_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local addr");
        let router = Router::new()
            .route(
                "/oauth2/token",
                post(|| async {
                    Json(serde_json::json!({
                        "access_token": "gemini-access-token",
                        "refresh_token": "gemini-refresh-token",
                        "token_type": "Bearer",
                        "expires_in": 3600
                    }))
                }),
            )
            .route(
                "/userinfo",
                get(|| async {
                    Json(serde_json::json!({
                        "email": "gemini@example.com"
                    }))
                }),
            )
            .route(
                "/projects",
                get(|| async {
                    Json(serde_json::json!({
                        "projects": [
                            { "projectId": "auto-project-123" }
                        ]
                    }))
                }),
            )
            .route(
                "/v1/projects/auto-project-123/services/cloudaicompanion.googleapis.com",
                get(|| async {
                    Json(serde_json::json!({
                        "state": "ENABLED"
                    }))
                }),
            )
            .route(
                "/v1/projects/manual-project-456/services/cloudaicompanion.googleapis.com",
                get(|| async {
                    Json(serde_json::json!({
                        "state": "ENABLED"
                    }))
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("gemini server");
        });
        address
    }

    #[tokio::test]
    async fn completes_gemini_login_and_resolves_default_project() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_gemini_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service = GeminiCliLoginService::new(sessions.clone(), None).with_endpoints(
            GeminiCliLoginEndpoints {
                authorize_url: format!("http://{server}/oauth/authorize"),
                token_url: format!("http://{server}/oauth2/token"),
                user_info_url: format!("http://{server}/userinfo"),
                projects_url: format!("http://{server}/projects"),
                service_usage_url: format!("http://{server}"),
                ..GeminiCliLoginEndpoints::default()
            },
        );

        let started = service.start_login(None).expect("start login");
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
            "gemini",
            &started.state,
            "auth-code-123",
            "",
        )
        .expect("write callback file");

        let completed = completion
            .await
            .expect("join task")
            .expect("complete login");

        assert_eq!(completed.email, "gemini@example.com");
        assert_eq!(completed.project_id, "auto-project-123");
        assert_eq!(
            completed.file_name,
            "gemini-gemini@example.com-auto-project-123.json"
        );

        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&completed.file_path).expect("auth file"))
                .expect("json");
        assert_eq!(payload["type"].as_str(), Some("gemini"));
        assert_eq!(payload["project_id"].as_str(), Some("auto-project-123"));
        assert_eq!(payload["auto"].as_bool(), Some(true));
        assert_eq!(payload["checked"].as_bool(), Some(true));
        let expected_token_uri = format!("http://{server}/oauth2/token");
        assert_eq!(
            payload["token"]["token_uri"].as_str(),
            Some(expected_token_uri.as_str())
        );
        assert!(sessions.get(&started.state).expect("get session").is_none());
    }

    #[tokio::test]
    async fn completes_gemini_login_with_loopback_endpoints_even_when_proxy_is_configured() {
        let temp_dir = TempDir::new().expect("temp dir");
        let server = spawn_gemini_server().await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            GeminiCliLoginService::new(sessions.clone(), Some("http://127.0.0.1:9".to_string()))
                .with_endpoints(GeminiCliLoginEndpoints {
                    authorize_url: format!("http://{server}/oauth/authorize"),
                    token_url: format!("http://{server}/oauth2/token"),
                    user_info_url: format!("http://{server}/userinfo"),
                    projects_url: format!("http://{server}/projects"),
                    service_usage_url: format!("http://{server}"),
                    ..GeminiCliLoginEndpoints::default()
                });

        let started = service.start_login(None).expect("start login");
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
            "gemini",
            &started.state,
            "auth-code-123",
            "",
        )
        .expect("write callback file");

        let completed = completion
            .await
            .expect("join task")
            .expect("complete login");

        assert_eq!(completed.email, "gemini@example.com");
        assert_eq!(completed.project_id, "auto-project-123");
    }
}
