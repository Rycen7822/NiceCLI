use crate::{
    normalize_codex_usage, NormalizeError, RateLimitSnapshot, WorkspaceRef, DEFAULT_WORKSPACE_ID,
    PROVIDER_CODEX,
};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use nicecli_auth::{
    extract_email_from_auth_file_name, extract_plan_from_auth_file_name,
    fetch_codex_account_profile, read_auth_file, AuthFileStoreError, CodexAccountProfile,
    DEFAULT_CODEX_ACCOUNT_CHECK_URL,
};
use nicecli_runtime::{AuthStore, AuthStoreError, FileAuthStore};
use reqwest::{Client, Proxy};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use thiserror::Error;

const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODEX_QUOTA_USER_AGENT: &str = "codex_cli_rs/0.116.0 (Windows NT 10.0; Win64; x64) NiceCLI";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexAuthContext {
    pub auth_id: String,
    pub auth_label: String,
    pub auth_note: String,
    pub auth_file_name: String,
    pub account_email: String,
    pub account_plan: String,
    pub account_id: String,
    pub cookies: HashMap<String, String>,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub base_url: String,
    pub proxy_url: String,
}

pub trait AuthEnumerator: Send + Sync {
    fn list_codex_auths(&self) -> Result<Vec<CodexAuthContext>, std::io::Error>;
}

#[derive(Debug, Clone)]
pub struct FileBackedCodexAuthEnumerator {
    store: FileAuthStore,
}

impl FileBackedCodexAuthEnumerator {
    pub fn new(auth_dir: impl Into<PathBuf>) -> Self {
        Self {
            store: FileAuthStore::new(auth_dir),
        }
    }
}

impl AuthEnumerator for FileBackedCodexAuthEnumerator {
    fn list_codex_auths(&self) -> Result<Vec<CodexAuthContext>, std::io::Error> {
        let mut auths = Vec::new();
        for snapshot in self.store.list_snapshots().map_err(map_auth_store_error)? {
            if snapshot.disabled {
                continue;
            }

            let raw = match read_auth_file(self.store.auth_dir(), &snapshot.name) {
                Ok(raw) => raw,
                Err(_) => continue,
            };
            let value: Value = match serde_json::from_slice(&raw) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Some(root) = value.as_object() else {
                continue;
            };

            if !is_codex_auth(root) || has_api_key_credential(root) {
                continue;
            }
            if let Some(auth) = build_codex_auth_context(root, &snapshot.name) {
                auths.push(auth);
            }
        }

        auths.sort_by(|left, right| {
            left.auth_id
                .cmp(&right.auth_id)
                .then_with(|| left.account_email.cmp(&right.account_email))
        });
        Ok(auths)
    }
}

fn map_auth_store_error(error: AuthStoreError) -> std::io::Error {
    match error {
        AuthStoreError::FileStore(AuthFileStoreError::ReadDir(inner))
        | AuthStoreError::FileStore(AuthFileStoreError::ReadFile(inner))
        | AuthStoreError::FileStore(AuthFileStoreError::WriteFile(inner))
        | AuthStoreError::FileStore(AuthFileStoreError::RemoveFile(inner)) => inner,
        AuthStoreError::FileStore(other) => std::io::Error::other(other.to_string()),
    }
}

#[derive(Debug, Error)]
pub enum CodexClaimsError {
    #[error("codex id_token is empty")]
    EmptyToken,
    #[error("invalid codex JWT format")]
    InvalidFormat,
    #[error("failed to decode codex JWT payload: {0}")]
    Decode(#[from] base64::DecodeError),
    #[error("failed to parse codex JWT payload: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct JwtClaims {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    #[serde(rename = "https://api.openai.com/auth")]
    pub codex_auth_info: CodexAuthInfo,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CodexAuthInfo {
    #[serde(default)]
    pub chatgpt_account_id: String,
    #[serde(default)]
    pub chatgpt_plan_type: String,
    #[serde(default)]
    pub organizations: Vec<Organization>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Organization {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub is_default: bool,
}

pub fn parse_codex_claims(id_token: &str) -> Result<JwtClaims, CodexClaimsError> {
    let trimmed = id_token.trim();
    if trimmed.is_empty() {
        return Err(CodexClaimsError::EmptyToken);
    }

    let mut parts = trimmed.split('.');
    let _header = parts.next();
    let Some(payload) = parts.next() else {
        return Err(CodexClaimsError::InvalidFormat);
    };
    if parts.next().is_none() {
        return Err(CodexClaimsError::InvalidFormat);
    }

    let decoded = URL_SAFE_NO_PAD.decode(payload)?;
    Ok(serde_json::from_slice(&decoded)?)
}

pub fn classify_workspace_type(plan_type: &str, has_organization: bool) -> String {
    let normalized = plan_type.trim().to_ascii_lowercase();
    if normalized.contains("enterprise") {
        return "enterprise".to_string();
    }
    if normalized.contains("edu")
        || normalized.contains("education")
        || normalized.contains("k12")
        || normalized.contains("quorum")
    {
        return "edu".to_string();
    }
    if normalized.contains("business")
        || normalized.contains("team")
        || normalized.contains("workspace")
    {
        return "business".to_string();
    }
    if has_organization {
        return "business".to_string();
    }
    if normalized.is_empty()
        || normalized.contains("free")
        || normalized.contains("plus")
        || normalized.contains("pro")
        || normalized.contains("go")
        || normalized.contains("guest")
    {
        return "personal".to_string();
    }
    "unknown".to_string()
}

pub fn extract_plan_from_filename(value: &str) -> Option<String> {
    extract_plan_from_auth_file_name(value)
}

pub fn extract_email(value: &str) -> Option<String> {
    extract_email_from_auth_file_name(value)
}

pub fn select_current_workspace(auth: &CodexAuthContext) -> WorkspaceRef {
    let claims = parse_codex_claims(&auth.id_token).ok();
    let workspaces = claims
        .as_ref()
        .map(workspaces_from_claims)
        .unwrap_or_default();
    let account_id = auth.account_id.trim();
    if !account_id.is_empty() {
        if let Some(workspace) = workspaces
            .iter()
            .find(|workspace| workspace.id == account_id)
        {
            return workspace.clone();
        }
    }
    fallback_workspace(auth, claims.as_ref())
}

#[async_trait]
pub trait CodexQuotaSource: Send + Sync {
    async fn list_workspaces(
        &self,
        auth: &CodexAuthContext,
    ) -> Result<Vec<WorkspaceRef>, CodexSourceError>;

    async fn fetch_workspace_snapshot(
        &self,
        auth: &CodexAuthContext,
        workspace: &WorkspaceRef,
    ) -> Result<RateLimitSnapshot, CodexSourceError>;

    async fn fetch_account_profile(&self, _auth: &CodexAuthContext) -> Option<CodexAccountProfile> {
        None
    }
}

#[derive(Debug, Error)]
pub enum CodexSourceError {
    #[error("codex quota auth context is missing")]
    MissingAuthContext,
    #[error("codex quota access token is empty")]
    MissingAccessToken,
    #[error("codex quota request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("codex quota returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    #[error(transparent)]
    Normalize(#[from] NormalizeError),
    #[error("codex quota response is empty")]
    EmptySnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct HttpCodexQuotaSource {
    default_proxy_url: Option<String>,
    clients: Arc<RwLock<HashMap<String, Client>>>,
}

impl HttpCodexQuotaSource {
    pub fn new(default_proxy_url: Option<String>) -> Self {
        Self {
            default_proxy_url: default_proxy_url.and_then(trimmed),
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn build_http_client(&self, auth: &CodexAuthContext) -> Result<Client, reqwest::Error> {
        let proxy_url = trimmed(auth.proxy_url.clone()).or_else(|| self.default_proxy_url.clone());
        let cache_key = proxy_url.clone().unwrap_or_default();
        if let Some(client) = self
            .clients
            .read()
            .expect("quota client cache read lock")
            .get(&cache_key)
            .cloned()
        {
            return Ok(client);
        }

        let mut builder = Client::builder().timeout(Duration::from_secs(30));
        if let Some(proxy_url) = proxy_url {
            builder = builder.proxy(Proxy::all(proxy_url)?);
        }
        let client = builder.build()?;
        self.clients
            .write()
            .expect("quota client cache write lock")
            .entry(cache_key)
            .or_insert_with(|| client.clone());
        Ok(client)
    }
}

#[async_trait]
impl CodexQuotaSource for HttpCodexQuotaSource {
    async fn fetch_account_profile(&self, auth: &CodexAuthContext) -> Option<CodexAccountProfile> {
        if auth.auth_id.trim().is_empty() || auth.access_token.trim().is_empty() {
            return None;
        }

        let client = self.build_http_client(auth).ok()?;
        fetch_codex_account_profile(
            &client,
            &build_codex_account_check_url(&auth.base_url),
            &auth.access_token,
            non_empty_ref(&auth.account_id),
        )
        .await
        .ok()
        .flatten()
    }

    async fn list_workspaces(
        &self,
        auth: &CodexAuthContext,
    ) -> Result<Vec<WorkspaceRef>, CodexSourceError> {
        if auth.auth_id.trim().is_empty() {
            return Err(CodexSourceError::MissingAuthContext);
        }
        let claims = parse_codex_claims(&auth.id_token).ok();
        let workspaces = claims
            .as_ref()
            .map(workspaces_from_claims)
            .unwrap_or_default();
        if workspaces.is_empty() {
            Ok(vec![fallback_workspace(auth, claims.as_ref())])
        } else {
            Ok(workspaces)
        }
    }

    async fn fetch_workspace_snapshot(
        &self,
        auth: &CodexAuthContext,
        workspace: &WorkspaceRef,
    ) -> Result<RateLimitSnapshot, CodexSourceError> {
        if auth.auth_id.trim().is_empty() {
            return Err(CodexSourceError::MissingAuthContext);
        }
        let access_token = auth.access_token.trim();
        if access_token.is_empty() {
            return Err(CodexSourceError::MissingAccessToken);
        }

        let client = self.build_http_client(auth)?;
        let (base_url, path_style) = normalize_codex_quota_base_url(&auth.base_url);
        let usage_url = build_codex_usage_url(&base_url, path_style);
        let mut request = client
            .get(usage_url)
            .bearer_auth(access_token)
            .header("User-Agent", CODEX_QUOTA_USER_AGENT);

        if let Some(account_id) = account_id_for_workspace(auth, workspace) {
            request = request.header("ChatGPT-Account-Id", account_id);
        }
        if let Some(cookie_header) = cookie_header_value(&auth.cookies) {
            request = request.header("Cookie", cookie_header);
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(CodexSourceError::UnexpectedStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).trim().to_string(),
            });
        }

        normalize_codex_usage(&body)?.ok_or(CodexSourceError::EmptySnapshot)
    }
}

fn build_codex_auth_context(
    root: &Map<String, Value>,
    file_name: &str,
) -> Option<CodexAuthContext> {
    let auth_id = first_non_empty([string_path(root, &["id"]), Some(file_name.to_string())])?;
    let access_token = first_non_empty([
        string_path(root, &["access_token"]),
        string_path(root, &["metadata", "access_token"]),
        string_path(root, &["attributes", "access_token"]),
    ])?;
    let auth_file_name = file_name.trim().to_string();

    Some(CodexAuthContext {
        auth_id,
        auth_label: first_non_empty([
            string_path(root, &["label"]),
            string_path(root, &["auth_label"]),
        ])
        .unwrap_or_default(),
        auth_note: first_non_empty([
            string_path(root, &["note"]),
            string_path(root, &["metadata", "note"]),
            string_path(root, &["attributes", "note"]),
        ])
        .unwrap_or_default(),
        auth_file_name: auth_file_name.clone(),
        account_email: first_non_empty([
            string_path(root, &["email"]),
            string_path(root, &["metadata", "email"]),
            string_path(root, &["attributes", "email"]),
            string_path(root, &["attributes", "account_email"]),
            extract_email(&auth_file_name),
        ])
        .unwrap_or_default(),
        account_plan: first_non_empty([
            string_path(root, &["account_plan"]),
            extract_plan_from_filename(&auth_file_name),
        ])
        .unwrap_or_default(),
        account_id: first_non_empty([
            string_path(root, &["account_id"]),
            string_path(root, &["metadata", "account_id"]),
            string_path(root, &["attributes", "account_id"]),
        ])
        .unwrap_or_default(),
        cookies: cookie_map(root),
        access_token,
        refresh_token: first_non_empty([
            string_path(root, &["refresh_token"]),
            string_path(root, &["metadata", "refresh_token"]),
            string_path(root, &["attributes", "refresh_token"]),
        ])
        .unwrap_or_default(),
        id_token: first_non_empty([
            string_path(root, &["id_token"]),
            string_path(root, &["metadata", "id_token"]),
            string_path(root, &["attributes", "id_token"]),
        ])
        .unwrap_or_default(),
        base_url: first_non_empty([
            string_path(root, &["base_url"]),
            string_path(root, &["attributes", "base_url"]),
            Some(DEFAULT_CODEX_BASE_URL.to_string()),
        ])
        .unwrap_or_else(|| DEFAULT_CODEX_BASE_URL.to_string()),
        proxy_url: first_non_empty([
            string_path(root, &["proxy_url"]),
            string_path(root, &["metadata", "proxy_url"]),
            string_path(root, &["attributes", "proxy_url"]),
        ])
        .unwrap_or_default(),
    })
}

fn is_codex_auth(root: &Map<String, Value>) -> bool {
    first_non_empty([
        string_path(root, &["provider"]),
        string_path(root, &["type"]),
    ])
    .map(|provider| provider.eq_ignore_ascii_case(PROVIDER_CODEX))
    .unwrap_or(false)
}

fn has_api_key_credential(root: &Map<String, Value>) -> bool {
    first_non_empty([
        string_path(root, &["api_key"]),
        string_path(root, &["attributes", "api_key"]),
    ])
    .is_some()
}

fn workspaces_from_claims(claims: &JwtClaims) -> Vec<WorkspaceRef> {
    let mut workspaces: Vec<_> = claims
        .codex_auth_info
        .organizations
        .iter()
        .filter_map(|organization| {
            let id = organization.id.trim();
            if id.is_empty() {
                return None;
            }
            Some(WorkspaceRef {
                id: id.to_string(),
                name: trimmed(organization.title.clone()).unwrap_or_else(|| id.to_string()),
                r#type: classify_workspace_type(&claims.codex_auth_info.chatgpt_plan_type, true),
            })
        })
        .collect();

    workspaces.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    workspaces.dedup_by(|left, right| left.id == right.id);
    workspaces
}

fn fallback_workspace(auth: &CodexAuthContext, claims: Option<&JwtClaims>) -> WorkspaceRef {
    let plan_type = claims
        .map(|claims| claims.codex_auth_info.chatgpt_plan_type.clone())
        .unwrap_or_default();
    let workspace_type = classify_workspace_type(&plan_type, false);
    let workspace_id =
        trimmed(auth.account_id.clone()).unwrap_or_else(|| DEFAULT_WORKSPACE_ID.to_string());
    let workspace_name = if workspace_type == "personal" && !auth.account_email.trim().is_empty() {
        auth.account_email.trim().to_string()
    } else {
        "Current Workspace".to_string()
    };
    WorkspaceRef {
        id: workspace_id,
        name: workspace_name,
        r#type: workspace_type,
    }
}

fn normalize_codex_quota_base_url(raw: &str) -> (String, CodexQuotaPathStyle) {
    let mut base_url = raw.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        base_url = DEFAULT_CODEX_BASE_URL.to_string();
    }
    if let Some(stripped) = base_url.strip_suffix("/codex") {
        base_url = stripped.to_string();
    }
    if let Some(stripped) = base_url.strip_suffix("/api/codex") {
        base_url = stripped.to_string();
    }
    if (base_url.starts_with("https://chatgpt.com")
        || base_url.starts_with("https://chat.openai.com"))
        && !base_url.contains("/backend-api")
    {
        base_url.push_str("/backend-api");
    }
    if base_url.contains("/backend-api") {
        (base_url, CodexQuotaPathStyle::ChatGptApi)
    } else {
        (base_url, CodexQuotaPathStyle::CodexApi)
    }
}

fn build_codex_usage_url(base_url: &str, path_style: CodexQuotaPathStyle) -> String {
    match path_style {
        CodexQuotaPathStyle::ChatGptApi => format!("{}/wham/usage", base_url.trim_end_matches('/')),
        CodexQuotaPathStyle::CodexApi => {
            format!("{}/api/codex/usage", base_url.trim_end_matches('/'))
        }
    }
}

fn build_codex_account_check_url(raw_base_url: &str) -> String {
    let (base_url, path_style) = normalize_codex_quota_base_url(raw_base_url);
    match path_style {
        CodexQuotaPathStyle::ChatGptApi => {
            format!("{}/wham/accounts/check", base_url.trim_end_matches('/'))
        }
        CodexQuotaPathStyle::CodexApi => DEFAULT_CODEX_ACCOUNT_CHECK_URL.to_string(),
    }
}

fn account_id_for_workspace(auth: &CodexAuthContext, workspace: &WorkspaceRef) -> Option<String> {
    let workspace_id = workspace.id.trim();
    if !workspace_id.is_empty() && workspace_id != DEFAULT_WORKSPACE_ID {
        Some(workspace_id.to_string())
    } else {
        trimmed(auth.account_id.clone())
    }
}

fn cookie_header_value(cookies: &HashMap<String, String>) -> Option<String> {
    if cookies.is_empty() {
        return None;
    }
    let mut keys: Vec<_> = cookies
        .iter()
        .filter_map(|(key, value)| {
            if key.trim().is_empty() || value.trim().is_empty() {
                None
            } else {
                Some(key.trim().to_string())
            }
        })
        .collect();
    keys.sort();
    let pairs: Vec<_> = keys
        .into_iter()
        .filter_map(|key| {
            cookies
                .get(&key)
                .map(|value| format!("{key}={}", value.trim()))
        })
        .collect();
    if pairs.is_empty() {
        None
    } else {
        Some(pairs.join("; "))
    }
}

fn cookie_map(root: &Map<String, Value>) -> HashMap<String, String> {
    for path in [
        ["cookies"].as_slice(),
        ["metadata", "cookies"].as_slice(),
        ["cookie"].as_slice(),
        ["metadata", "cookie"].as_slice(),
    ] {
        if let Some(cookies) = cookies_path(root, path) {
            return cookies;
        }
    }
    HashMap::new()
}

fn cookies_path(root: &Map<String, Value>, path: &[&str]) -> Option<HashMap<String, String>> {
    let value = value_path(root, path)?;
    match value {
        Value::Object(object) => {
            let mut cookies = HashMap::new();
            for (key, value) in object {
                if let Some(value) = value_as_trimmed_string(value) {
                    cookies.insert(key.trim().to_string(), value);
                }
            }
            if cookies.is_empty() {
                None
            } else {
                Some(cookies)
            }
        }
        Value::String(text) => parse_cookie_header(text),
        _ => None,
    }
}

fn parse_cookie_header(raw: &str) -> Option<HashMap<String, String>> {
    let mut cookies = HashMap::new();
    for part in raw.split(';') {
        let mut segments = part.trim().splitn(2, '=');
        let Some(key) = segments.next() else {
            continue;
        };
        let Some(value) = segments.next() else {
            continue;
        };
        if !key.trim().is_empty() && !value.trim().is_empty() {
            cookies.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    if cookies.is_empty() {
        None
    } else {
        Some(cookies)
    }
}

fn string_path(root: &Map<String, Value>, path: &[&str]) -> Option<String> {
    value_path(root, path).and_then(value_as_trimmed_string)
}

fn value_path<'a>(root: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        current = current.as_object()?.get(*segment)?;
    }
    Some(current)
}

fn value_as_trimmed_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => trimmed(text.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => trimmed(value.to_string()),
    }
}

fn first_non_empty<I>(values: I) -> Option<String>
where
    I: IntoIterator<Item = Option<String>>,
{
    for value in values {
        if let Some(value) = value.and_then(trimmed) {
            return Some(value);
        }
    }
    None
}

fn trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_empty_ref(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexQuotaPathStyle {
    CodexApi,
    ChatGptApi,
}

#[cfg(test)]
mod tests {
    use super::{
        extract_email, extract_plan_from_filename, parse_codex_claims, AuthEnumerator,
        FileBackedCodexAuthEnumerator,
    };
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use std::fs;
    use tempfile::TempDir;

    fn build_jwt(payload_json: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn extracts_email_and_plan_from_filename() {
        assert_eq!(
            extract_email("codex-demo.user@example.com-team.json").as_deref(),
            Some("demo.user@example.com")
        );
        assert_eq!(
            extract_plan_from_filename("codex-demo.user@example.com-team.json").as_deref(),
            Some("team")
        );
    }

    #[test]
    fn parses_codex_claims_payload() {
        let token = build_jwt(
            r#"{
                "email": "demo@example.com",
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "org_default",
                    "chatgpt_plan_type": "business",
                    "organizations": [
                        { "id": "org_default", "title": "Workspace A", "is_default": true },
                        { "id": "org_secondary", "title": "Workspace B", "is_default": false }
                    ]
                }
            }"#,
        );
        let claims = parse_codex_claims(&token).expect("claims");
        assert_eq!(claims.email, "demo@example.com");
        assert_eq!(claims.codex_auth_info.organizations.len(), 2);
    }

    #[test]
    fn reads_codex_auth_files_from_disk() {
        let temp_dir = TempDir::new().expect("temp dir");
        let token = build_jwt(
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
        fs::write(
            temp_dir.path().join("codex-demo@example.com-team.json"),
            format!(
                r#"{{
                    "provider": "codex",
                    "label": "Primary",
                    "note": "Workspace A",
                    "metadata": {{
                        "email": "demo@example.com",
                        "account_id": "org_default",
                        "access_token": "token-123",
                        "id_token": "{token}",
                        "cookie": "foo=bar; baz=qux"
                    }},
                    "attributes": {{
                        "base_url": "https://chatgpt.com/backend-api"
                    }}
                }}"#
            ),
        )
        .expect("auth file");

        let enumerator = FileBackedCodexAuthEnumerator::new(temp_dir.path());
        let auths = enumerator.list_codex_auths().expect("auths");
        assert_eq!(auths.len(), 1);
        assert_eq!(auths[0].auth_label, "Primary");
        assert_eq!(auths[0].account_email, "demo@example.com");
        assert_eq!(auths[0].account_plan, "team");
        assert_eq!(auths[0].cookies.len(), 2);
    }
}
