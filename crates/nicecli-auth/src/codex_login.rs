use crate::{OAuthFlowError, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::{Client, Proxy};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use url::Url;

const DEFAULT_CODEX_AUTH_URL: &str = "https://auth.openai.com/oauth/authorize";
const DEFAULT_CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_CODEX_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedCodexLogin {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedCodexLogin {
    pub file_name: String,
    pub file_path: PathBuf,
    pub email: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexLoginEndpoints {
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub redirect_uri: String,
}

impl Default for CodexLoginEndpoints {
    fn default() -> Self {
        Self {
            auth_url: DEFAULT_CODEX_AUTH_URL.to_string(),
            token_url: DEFAULT_CODEX_TOKEN_URL.to_string(),
            client_id: DEFAULT_CODEX_CLIENT_ID.to_string(),
            redirect_uri: DEFAULT_CODEX_REDIRECT_URI.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum CodexLoginError {
    #[error(transparent)]
    OAuthSession(#[from] OAuthFlowError),
    #[error("failed to generate secure random bytes")]
    Random(rand::Error),
    #[error("failed to build codex auth url: {0}")]
    BuildAuthUrl(url::ParseError),
    #[error("codex oauth flow is not pending")]
    SessionNotPending,
    #[error("codex oauth callback returned an error: {0}")]
    CallbackRejected(String),
    #[error("codex oauth authorization code is empty")]
    MissingAuthorizationCode,
    #[error("codex token request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("codex token exchange returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    #[error("failed to parse codex token response: {0}")]
    ParseToken(#[from] serde_json::Error),
    #[error("codex token response is missing id_token")]
    MissingIdToken,
    #[error("codex token response is missing email")]
    MissingEmail,
    #[error("failed to decode codex id_token payload")]
    InvalidIdToken,
    #[error("failed to create auth dir: {0}")]
    CreateAuthDir(std::io::Error),
    #[error("failed to read existing auth file: {0}")]
    ReadExistingAuthFile(std::io::Error),
    #[error("failed to write auth file: {0}")]
    WriteAuthFile(std::io::Error),
}

#[derive(Debug, Clone)]
struct PendingCodexLogin {
    pkce: PkceCodes,
    expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

#[derive(Debug, Clone)]
pub struct CodexLoginService {
    oauth_sessions: Arc<OAuthSessionStore>,
    default_proxy_url: Option<String>,
    endpoints: CodexLoginEndpoints,
    ttl: Duration,
    pending: Arc<Mutex<HashMap<String, PendingCodexLogin>>>,
}

impl CodexLoginService {
    pub fn new(oauth_sessions: Arc<OAuthSessionStore>, default_proxy_url: Option<String>) -> Self {
        Self {
            oauth_sessions,
            default_proxy_url: trimmed(default_proxy_url),
            endpoints: CodexLoginEndpoints::default(),
            ttl: DEFAULT_OAUTH_SESSION_TTL,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_endpoints(mut self, endpoints: CodexLoginEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn start_login(&self) -> Result<StartedCodexLogin, CodexLoginError> {
        let state = generate_state()?;
        let pkce = generate_pkce_codes()?;
        let auth_url = Url::parse_with_params(
            &self.endpoints.auth_url,
            &[
                ("client_id", self.endpoints.client_id.as_str()),
                ("response_type", "code"),
                ("redirect_uri", self.endpoints.redirect_uri.as_str()),
                ("scope", "openid email profile offline_access"),
                ("state", state.as_str()),
                ("code_challenge", pkce.code_challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("prompt", "login"),
                ("id_token_add_organizations", "true"),
                ("codex_cli_simplified_flow", "true"),
            ],
        )
        .map_err(CodexLoginError::BuildAuthUrl)?;

        self.oauth_sessions.register(&state, "codex")?;
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending.insert(
            state.clone(),
            PendingCodexLogin {
                pkce,
                expires_at: Instant::now() + self.ttl,
            },
        );

        Ok(StartedCodexLogin {
            state,
            url: auth_url.to_string(),
        })
    }

    pub async fn complete_login(
        &self,
        auth_dir: &Path,
        state: &str,
        code: &str,
        callback_error: &str,
    ) -> Result<CompletedCodexLogin, CodexLoginError> {
        if !self.oauth_sessions.is_pending(state, Some("codex"))? {
            self.remove_pending(state);
            return Err(CodexLoginError::SessionNotPending);
        }

        let state = state.trim();
        let callback_error = callback_error.trim();
        if !callback_error.is_empty() {
            self.remove_pending(state);
            self.oauth_sessions.set_error(state, callback_error)?;
            return Err(CodexLoginError::CallbackRejected(
                callback_error.to_string(),
            ));
        }

        let code = code.trim();
        if code.is_empty() {
            self.remove_pending(state);
            self.oauth_sessions
                .set_error(state, "Missing authorization code")?;
            return Err(CodexLoginError::MissingAuthorizationCode);
        }

        let pkce = self.take_pending_pkce(state)?;
        let tokens = match self.exchange_code_for_tokens(code, &pkce).await {
            Ok(tokens) => tokens,
            Err(error) => {
                self.oauth_sessions
                    .set_error(state, "Failed to exchange authorization code for tokens")?;
                return Err(error);
            }
        };

        let claims = parse_claims(&tokens.id_token).map_err(|_| {
            let _ = self
                .oauth_sessions
                .set_error(state, "Failed to parse ID token");
            CodexLoginError::InvalidIdToken
        })?;

        let email = trimmed(Some(tokens.email.clone()))
            .or_else(|| trimmed(Some(claims.email.clone())))
            .ok_or_else(|| {
                let _ = self
                    .oauth_sessions
                    .set_error(state, "Failed to resolve account email");
                CodexLoginError::MissingEmail
            })?;
        let account_id = trimmed(Some(claims.codex_auth_info.chatgpt_account_id.clone()))
            .or_else(|| trimmed(Some(tokens.account_id.clone())))
            .unwrap_or_default();
        let plan_type =
            trimmed(Some(claims.codex_auth_info.chatgpt_plan_type.clone())).unwrap_or_default();
        let hash_account_id = if account_id.is_empty() {
            String::new()
        } else {
            hash_account_id(&account_id)
        };
        let file_name = credential_file_name(&email, &plan_type, &hash_account_id, true);
        let file_path = auth_dir.join(&file_name);
        let note = read_existing_note(&file_path)
            .map_err(CodexLoginError::ReadExistingAuthFile)?
            .or_else(|| default_workspace_note(&claims, &account_id))
            .unwrap_or_default();

        fs::create_dir_all(auth_dir).map_err(CodexLoginError::CreateAuthDir)?;
        let payload = serde_json::json!({
            "id": file_name,
            "provider": "codex",
            "type": "codex",
            "email": email,
            "account_id": account_id,
            "account_plan": normalize_plan_type_for_filename(&plan_type),
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "id_token": tokens.id_token,
            "last_refresh": rfc3339_now(),
            "expired": tokens.expired,
            "note": note,
        });
        let bytes = serde_json::to_vec_pretty(&payload)?;
        fs::write(&file_path, bytes).map_err(CodexLoginError::WriteAuthFile)?;

        self.oauth_sessions.complete(state)?;

        Ok(CompletedCodexLogin {
            file_name: file_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_path,
            email,
            note,
        })
    }

    fn take_pending_pkce(&self, state: &str) -> Result<PkceCodes, CodexLoginError> {
        let mut pending = self.lock_pending();
        purge_expired_pending(&mut pending);
        pending
            .remove(state)
            .map(|pending| pending.pkce)
            .ok_or(CodexLoginError::SessionNotPending)
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

    async fn exchange_code_for_tokens(
        &self,
        code: &str,
        pkce: &PkceCodes,
    ) -> Result<TokenResponse, CodexLoginError> {
        let client = self.build_http_client()?;
        let response = client
            .post(&self.endpoints.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", self.endpoints.client_id.as_str()),
                ("code", code),
                ("redirect_uri", self.endpoints.redirect_uri.as_str()),
                ("code_verifier", pkce.code_verifier.as_str()),
            ])
            .send()
            .await?;

        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(CodexLoginError::UnexpectedStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        let mut tokens: TokenResponse = serde_json::from_slice(&body)?;
        if tokens.id_token.trim().is_empty() {
            return Err(CodexLoginError::MissingIdToken);
        }
        if tokens.expired.trim().is_empty() && tokens.expires_in > 0 {
            tokens.expired = rfc3339_after_seconds(tokens.expires_in);
        }
        Ok(tokens)
    }

    fn lock_pending(&self) -> MutexGuard<'_, HashMap<String, PendingCodexLogin>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    id_token: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    account_id: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    expired: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct JwtClaims {
    #[serde(default)]
    email: String,
    #[serde(default)]
    #[serde(rename = "https://api.openai.com/auth")]
    codex_auth_info: CodexAuthInfo,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CodexAuthInfo {
    #[serde(default)]
    chatgpt_account_id: String,
    #[serde(default)]
    chatgpt_plan_type: String,
    #[serde(default)]
    organizations: Vec<Organization>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Organization {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    is_default: bool,
}

fn generate_state() -> Result<String, CodexLoginError> {
    let mut bytes = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(CodexLoginError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn generate_pkce_codes() -> Result<PkceCodes, CodexLoginError> {
    let mut verifier_bytes = [0_u8; 96];
    OsRng
        .try_fill_bytes(&mut verifier_bytes)
        .map_err(CodexLoginError::Random)?;
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);
    Ok(PkceCodes {
        code_verifier,
        code_challenge,
    })
}

fn purge_expired_pending(pending: &mut HashMap<String, PendingCodexLogin>) {
    let now = Instant::now();
    pending.retain(|_, entry| entry.expires_at > now);
}

fn parse_claims(id_token: &str) -> Result<JwtClaims, CodexLoginError> {
    let mut parts = id_token.trim().split('.');
    let _header = parts.next();
    let Some(payload) = parts.next() else {
        return Err(CodexLoginError::InvalidIdToken);
    };
    if parts.next().is_none() {
        return Err(CodexLoginError::InvalidIdToken);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| CodexLoginError::InvalidIdToken)?;
    serde_json::from_slice(&decoded).map_err(CodexLoginError::ParseToken)
}

fn credential_file_name(
    email: &str,
    plan_type: &str,
    hash_account_id: &str,
    include_provider_prefix: bool,
) -> String {
    let email = email.trim();
    let plan = normalize_plan_type_for_filename(plan_type);
    let prefix = if include_provider_prefix { "codex" } else { "" };
    if plan.is_empty() {
        format!("{prefix}-{email}.json")
    } else if plan == "team" {
        format!("{prefix}-{hash_account_id}-{email}-{plan}.json")
    } else {
        format!("{prefix}-{email}-{plan}.json")
    }
}

fn normalize_plan_type_for_filename(plan_type: &str) -> String {
    plan_type
        .trim()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_ascii_lowercase())
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn hash_account_id(account_id: &str) -> String {
    let digest = Sha256::digest(account_id.as_bytes());
    let mut out = String::with_capacity(8);
    for byte in digest.iter().take(4) {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

fn read_existing_note(path: &Path) -> Result<Option<String>, std::io::Error> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let value: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(value
        .as_object()
        .and_then(|root| root.get("note"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string))
}

fn default_workspace_note(claims: &JwtClaims, account_id: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(account_id) = trimmed(Some(account_id.to_string())) {
        candidates.push(account_id);
    }
    if let Some(account_id) = trimmed(Some(claims.codex_auth_info.chatgpt_account_id.clone())) {
        candidates.push(account_id);
    }

    for candidate in candidates {
        for organization in &claims.codex_auth_info.organizations {
            if organization
                .id
                .trim()
                .eq_ignore_ascii_case(candidate.trim())
            {
                if let Some(title) = trimmed(Some(organization.title.clone())) {
                    return Some(title);
                }
            }
        }
    }

    for organization in &claims.codex_auth_info.organizations {
        if organization.is_default {
            if let Some(title) = trimmed(Some(organization.title.clone())) {
                return Some(title);
            }
        }
    }

    claims
        .codex_auth_info
        .organizations
        .iter()
        .find_map(|organization| trimmed(Some(organization.title.clone())))
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

#[cfg(test)]
mod tests {
    use super::{CodexLoginEndpoints, CodexLoginService};
    use crate::OAuthSessionStore;
    use axum::{routing::post, Json, Router};
    use base64::Engine;
    use serde_json::{json, Value};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    fn build_jwt(payload_json: &str) -> String {
        let header = super::URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = super::URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        format!("{header}.{payload}.signature")
    }

    async fn spawn_token_server(id_token: String) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local addr");
        let router = Router::new().route(
            "/oauth/token",
            post(move || {
                let id_token = id_token.clone();
                async move {
                    Json(json!({
                        "access_token": "access-token-123",
                        "refresh_token": "refresh-token-456",
                        "id_token": id_token,
                        "expires_in": 3600
                    }))
                }
            }),
        );
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("token server");
        });
        address
    }

    #[tokio::test]
    async fn completes_codex_login_and_writes_auth_file() {
        let temp_dir = TempDir::new().expect("temp dir");
        let claims = build_jwt(
            r#"{
                "email": "demo@example.com",
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "org_default",
                    "chatgpt_plan_type": "team",
                    "organizations": [
                        { "id": "org_default", "title": "Workspace A", "is_default": true }
                    ]
                }
            }"#,
        );
        let token_server = spawn_token_server(claims).await;
        let sessions = Arc::new(OAuthSessionStore::default());
        let service =
            CodexLoginService::new(sessions.clone(), None).with_endpoints(CodexLoginEndpoints {
                auth_url: format!("http://{token_server}/oauth/authorize"),
                token_url: format!("http://{token_server}/oauth/token"),
                ..CodexLoginEndpoints::default()
            });

        let started = service.start_login().expect("start login");
        assert!(sessions
            .is_pending(&started.state, Some("codex"))
            .expect("pending"));

        let completed = service
            .complete_login(temp_dir.path(), &started.state, "auth-code-123", "")
            .await
            .expect("complete login");

        assert_eq!(completed.email, "demo@example.com");
        assert_eq!(completed.note, "Workspace A");
        let files: Vec<_> = fs::read_dir(temp_dir.path())
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].starts_with("codex-"));
        assert!(files[0].ends_with("-demo@example.com-team.json"));

        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&completed.file_path).expect("auth file"))
                .expect("json");
        assert_eq!(payload["type"].as_str(), Some("codex"));
        assert_eq!(payload["email"].as_str(), Some("demo@example.com"));
        assert_eq!(payload["note"].as_str(), Some("Workspace A"));
        assert_eq!(payload["account_id"].as_str(), Some("org_default"));
        assert!(sessions.get(&started.state).expect("get session").is_none());
    }
}
