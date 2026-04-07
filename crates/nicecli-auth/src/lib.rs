mod anthropic_login;
mod antigravity_login;
mod codex_login;
mod codex_profile;
mod gemini_cli_login;
mod gemini_web;
mod kimi_login;
mod oauth;
mod qwen_login;
mod vertex_import;

pub use anthropic_login::{
    AnthropicLoginEndpoints, AnthropicLoginError, AnthropicLoginService, CompletedAnthropicLogin,
    StartedAnthropicLogin,
};
pub use antigravity_login::{
    AntigravityLoginEndpoints, AntigravityLoginError, AntigravityLoginService,
    CompletedAntigravityLogin, StartedAntigravityLogin,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
pub use codex_login::{
    CodexLoginEndpoints, CodexLoginError, CodexLoginService, CompletedCodexLogin, StartedCodexLogin,
};
pub use codex_profile::{
    fetch_codex_account_profile, is_generic_codex_workspace_name, parse_codex_account_profile,
    CodexAccountProfile, CodexAccountProfileError, DEFAULT_CODEX_ACCOUNT_CHECK_URL,
};
pub use gemini_cli_login::{
    CompletedGeminiCliLogin, GeminiCliLoginEndpoints, GeminiCliLoginError, GeminiCliLoginService,
    StartedGeminiCliLogin,
};
pub use gemini_web::{save_gemini_web_tokens, GeminiWebTokenError, SavedGeminiWebTokens};
pub use kimi_login::{
    CompletedKimiLogin, KimiLoginEndpoints, KimiLoginError, KimiLoginService, StartedKimiLogin,
};
pub use oauth::{
    normalize_oauth_provider, resolve_oauth_callback_input, validate_oauth_state,
    write_oauth_callback_file, write_oauth_callback_file_for_pending_session, OAuthCallbackInput,
    OAuthFlowError, OAuthSessionSnapshot, OAuthSessionStore, DEFAULT_OAUTH_SESSION_TTL,
};
pub use qwen_login::{
    CompletedQwenLogin, QwenLoginEndpoints, QwenLoginError, QwenLoginService, StartedQwenLogin,
};
pub use vertex_import::{
    import_vertex_credential, ImportedVertexCredential, VertexCredentialImportError,
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthFileStoreError {
    #[error("invalid name")]
    InvalidName,
    #[error("name must end with .json")]
    InvalidExtension,
    #[error("invalid auth file: {0}")]
    InvalidAuthFile(String),
    #[error("file not found")]
    NotFound,
    #[error("failed to read auth dir: {0}")]
    ReadDir(std::io::Error),
    #[error("failed to read file: {0}")]
    ReadFile(std::io::Error),
    #[error("failed to write file: {0}")]
    WriteFile(std::io::Error),
    #[error("failed to remove file: {0}")]
    RemoveFile(std::io::Error),
    #[error("failed to encode auth file: {0}")]
    Encode(serde_json::Error),
    #[error("auth file must be a json object")]
    InvalidRoot,
    #[error("no fields to update")]
    NoFieldsToUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthFileEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub provider: String,
    pub source: String,
    pub size: u64,
    pub modtime: u128,
    #[serde(default)]
    pub disabled: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchAuthFileFields {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchAuthFileStatus {
    pub disabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
struct CodexJwtClaims {
    #[serde(default)]
    #[serde(rename = "https://api.openai.com/auth")]
    codex_auth_info: CodexAuthInfo,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
struct CodexAuthInfo {
    #[serde(default)]
    chatgpt_account_id: String,
    #[serde(default)]
    organizations: Vec<CodexOrganization>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
struct CodexOrganization {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    is_default: bool,
}

pub fn list_auth_files(auth_dir: &Path) -> Result<Vec<AuthFileEntry>, AuthFileStoreError> {
    let mut files = Vec::new();

    if !auth_dir.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(auth_dir).map_err(AuthFileStoreError::ReadDir)? {
        let entry = entry.map_err(AuthFileStoreError::ReadDir)?;
        let path = entry.path();
        let metadata = entry.metadata().map_err(AuthFileStoreError::ReadDir)?;
        if metadata.is_dir() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.to_ascii_lowercase().ends_with(".json") {
            continue;
        }

        let raw = fs::read(&path).ok();
        let json = raw
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());

        let provider = json
            .as_ref()
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .trim()
            .to_string();

        let email = json
            .as_ref()
            .and_then(|value| value.get("email"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| extract_email_from_auth_file_name(&file_name));

        let account_plan = json
            .as_ref()
            .and_then(|value| value.get("account_plan"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| extract_plan_from_auth_file_name(&file_name));

        let note = json
            .as_ref()
            .and_then(|value| resolve_auth_file_note(value, &provider));

        let disabled = json
            .as_ref()
            .and_then(|value| value.get("disabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let status = json
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                if disabled {
                    "disabled".to_string()
                } else {
                    "active".to_string()
                }
            });

        let status_message = json
            .as_ref()
            .and_then(|value| value.get("status_message"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let priority = json
            .as_ref()
            .and_then(|value| value.get("priority"))
            .and_then(|value| {
                value.as_i64().or_else(|| {
                    value
                        .as_str()
                        .and_then(|text| text.trim().parse::<i64>().ok())
                })
            });

        files.push(AuthFileEntry {
            id: file_name.clone(),
            name: file_name,
            provider_type: provider.clone(),
            provider,
            source: "file".to_string(),
            size: metadata.len(),
            modtime: metadata
                .modified()
                .ok()
                .and_then(system_time_to_unix_millis)
                .unwrap_or_default(),
            disabled,
            status,
            status_message,
            email,
            account_plan,
            note,
            priority,
        });
    }

    files.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(files)
}

pub fn read_auth_file(auth_dir: &Path, name: &str) -> Result<Vec<u8>, AuthFileStoreError> {
    let file_name = validate_auth_file_name(name)?;
    let path = auth_dir.join(file_name);
    fs::read(path).map_err(map_read_file_error)
}

pub fn write_auth_file(
    auth_dir: &Path,
    name: &str,
    data: &[u8],
) -> Result<String, AuthFileStoreError> {
    let file_name = validate_auth_file_name(name)?;
    validate_auth_file_json(data)?;
    fs::create_dir_all(auth_dir).map_err(AuthFileStoreError::WriteFile)?;
    fs::write(auth_dir.join(&file_name), data).map_err(AuthFileStoreError::WriteFile)?;
    Ok(file_name)
}

pub fn delete_auth_file(auth_dir: &Path, name: &str) -> Result<String, AuthFileStoreError> {
    let file_name = validate_auth_file_name(name)?;
    fs::remove_file(auth_dir.join(&file_name)).map_err(map_remove_file_error)?;
    Ok(file_name)
}

pub fn patch_auth_file_fields(
    auth_dir: &Path,
    name: &str,
    fields: &PatchAuthFileFields,
) -> Result<(), AuthFileStoreError> {
    let file_name = validate_auth_file_name(name)?;
    let path = auth_dir.join(file_name);
    let raw = fs::read_to_string(&path).map_err(map_read_file_error)?;
    let mut json_value: Value = serde_json::from_str(&raw)
        .map_err(|error| AuthFileStoreError::InvalidAuthFile(error.to_string()))?;
    let object = json_value
        .as_object_mut()
        .ok_or(AuthFileStoreError::InvalidRoot)?;

    let mut changed = false;
    changed |= patch_optional_string_field(object, "prefix", fields.prefix.as_deref());
    changed |= patch_optional_string_field(object, "proxy_url", fields.proxy_url.as_deref());
    changed |= patch_optional_string_field(object, "note", fields.note.as_deref());
    changed |= patch_priority_field(object, fields.priority);

    if !changed {
        return Err(AuthFileStoreError::NoFieldsToUpdate);
    }

    let pretty = serde_json::to_vec_pretty(&json_value).map_err(AuthFileStoreError::Encode)?;
    fs::write(path, pretty).map_err(AuthFileStoreError::WriteFile)?;
    Ok(())
}

pub fn patch_auth_file_status(
    auth_dir: &Path,
    name: &str,
    status: PatchAuthFileStatus,
) -> Result<(), AuthFileStoreError> {
    let file_name = validate_auth_file_name(name)?;
    let path = auth_dir.join(file_name);
    let raw = fs::read_to_string(&path).map_err(map_read_file_error)?;
    let mut json_value: Value = serde_json::from_str(&raw)
        .map_err(|error| AuthFileStoreError::InvalidAuthFile(error.to_string()))?;
    let object = json_value
        .as_object_mut()
        .ok_or(AuthFileStoreError::InvalidRoot)?;

    object.insert("disabled".to_string(), Value::Bool(status.disabled));
    object.insert(
        "status".to_string(),
        Value::String(if status.disabled {
            "disabled".to_string()
        } else {
            "active".to_string()
        }),
    );
    if status.disabled {
        object.insert(
            "status_message".to_string(),
            Value::String("disabled via management API".to_string()),
        );
    } else {
        object.remove("status_message");
    }

    let pretty = serde_json::to_vec_pretty(&json_value).map_err(AuthFileStoreError::Encode)?;
    fs::write(path, pretty).map_err(AuthFileStoreError::WriteFile)?;
    Ok(())
}

pub fn validate_auth_file_name(name: &str) -> Result<String, AuthFileStoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AuthFileStoreError::InvalidName);
    }
    if !trimmed.to_ascii_lowercase().ends_with(".json") {
        return Err(AuthFileStoreError::InvalidExtension);
    }

    let path = Path::new(trimmed);
    if path.file_name() != Some(OsStr::new(trimmed)) {
        return Err(AuthFileStoreError::InvalidName);
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        return Err(AuthFileStoreError::InvalidName);
    }

    Ok(trimmed.to_string())
}

pub fn extract_plan_from_auth_file_name(value: &str) -> Option<String> {
    let file_stem = normalized_auth_file_stem(value)?.to_ascii_lowercase();
    for plan in ["team", "pro", "plus"] {
        if file_stem.ends_with(&format!("-{plan}")) {
            return Some(plan.to_string());
        }
    }
    None
}

pub fn extract_email_from_auth_file_name(value: &str) -> Option<String> {
    let mut stem = normalized_auth_file_stem(value)?;
    if let Some(without_plan) = strip_known_plan_suffix(stem) {
        stem = without_plan;
    }
    stem = strip_known_auth_file_prefix(stem);
    if let Some((prefix, rest)) = stem.split_once('-') {
        if looks_like_generated_prefix(prefix) && looks_like_email(rest) {
            stem = rest;
        }
    }
    looks_like_email(stem).then(|| stem.to_string())
}

fn validate_auth_file_json(data: &[u8]) -> Result<(), AuthFileStoreError> {
    let value: Value = serde_json::from_slice(data)
        .map_err(|error| AuthFileStoreError::InvalidAuthFile(error.to_string()))?;
    if value.is_object() {
        Ok(())
    } else {
        Err(AuthFileStoreError::InvalidRoot)
    }
}

fn map_read_file_error(error: std::io::Error) -> AuthFileStoreError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AuthFileStoreError::NotFound
    } else {
        AuthFileStoreError::ReadFile(error)
    }
}

fn map_remove_file_error(error: std::io::Error) -> AuthFileStoreError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AuthFileStoreError::NotFound
    } else {
        AuthFileStoreError::RemoveFile(error)
    }
}

fn patch_optional_string_field(
    object: &mut Map<String, Value>,
    key: &str,
    value: Option<&str>,
) -> bool {
    let Some(value) = value else {
        return false;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        object.remove(key).is_some()
    } else {
        object.insert(key.to_string(), Value::String(trimmed.to_string()));
        true
    }
}

fn patch_priority_field(object: &mut Map<String, Value>, value: Option<i64>) -> bool {
    let Some(value) = value else {
        return false;
    };
    if value == 0 {
        object.remove("priority").is_some()
    } else {
        object.insert("priority".to_string(), Value::Number(value.into()));
        true
    }
}

fn system_time_to_unix_millis(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn resolve_auth_file_note(value: &Value, provider: &str) -> Option<String> {
    explicit_auth_file_note(value).or_else(|| {
        if provider.eq_ignore_ascii_case("codex") {
            synthesize_codex_workspace_note(value)
        } else {
            None
        }
    })
}

fn explicit_auth_file_note(value: &Value) -> Option<String> {
    value
        .get("note")
        .and_then(Value::as_str)
        .and_then(trimmed_str)
        .map(str::to_string)
}

fn synthesize_codex_workspace_note(value: &Value) -> Option<String> {
    let root = value.as_object()?;
    let id_token = first_non_empty([
        string_path(root, &["id_token"]),
        string_path(root, &["metadata", "id_token"]),
        string_path(root, &["attributes", "id_token"]),
    ])?;
    let account_id = first_non_empty([
        string_path(root, &["account_id"]),
        string_path(root, &["metadata", "account_id"]),
        string_path(root, &["attributes", "account_id"]),
    ])
    .unwrap_or_default();
    let claims = parse_codex_claims(&id_token)?;
    default_codex_workspace_note(&claims, &account_id)
}

fn parse_codex_claims(id_token: &str) -> Option<CodexJwtClaims> {
    let mut parts = id_token.trim().split('.');
    let _header = parts.next();
    let payload = parts.next()?;
    parts.next()?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn default_codex_workspace_note(claims: &CodexJwtClaims, account_id: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(account_id) = trimmed_str(account_id) {
        candidates.push(account_id.to_string());
    }
    if let Some(account_id) = trimmed_str(&claims.codex_auth_info.chatgpt_account_id) {
        candidates.push(account_id.to_string());
    }

    for candidate in candidates {
        for organization in &claims.codex_auth_info.organizations {
            if organization
                .id
                .trim()
                .eq_ignore_ascii_case(candidate.trim())
            {
                if let Some(title) = trimmed_str(&organization.title) {
                    return Some(title.to_string());
                }
            }
        }
    }

    for organization in &claims.codex_auth_info.organizations {
        if organization.is_default {
            if let Some(title) = trimmed_str(&organization.title) {
                return Some(title.to_string());
            }
        }
    }

    claims
        .codex_auth_info
        .organizations
        .iter()
        .find_map(|organization| trimmed_str(&organization.title).map(str::to_string))
}

fn normalized_auth_file_stem(value: &str) -> Option<&str> {
    let file_name = Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(value)
        .trim();
    file_name
        .strip_suffix(".json")
        .or_else(|| file_name.strip_suffix(".JSON"))
}

fn strip_known_plan_suffix(value: &str) -> Option<&str> {
    for suffix in ["-team", "-pro", "-plus"] {
        if let Some(stripped) = value.strip_suffix(suffix) {
            return Some(stripped);
        }
    }
    None
}

fn strip_known_auth_file_prefix(value: &str) -> &str {
    // Keep legacy filename prefixes so old auth files still resolve email/plan metadata.
    for prefix in [
        "gemini-web-",
        "antigravity-",
        "claude-",
        "codex-",
        "gemini-",
        "qwen-",
    ] {
        if let Some(stripped) = value.strip_prefix(prefix) {
            return stripped;
        }
    }
    value
}

fn looks_like_generated_prefix(value: &str) -> bool {
    value.len() >= 6
        && value.chars().all(|ch| ch.is_ascii_alphanumeric())
        && !value.contains('.')
        && !value.contains('@')
}

fn string_path(root: &Map<String, Value>, path: &[&str]) -> Option<String> {
    value_path(root, path)
        .and_then(Value::as_str)
        .and_then(trimmed_str)
        .map(str::to_string)
}

fn value_path<'a>(root: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        current = current.as_object()?.get(*segment)?;
    }
    Some(current)
}

fn first_non_empty<I>(values: I) -> Option<String>
where
    I: IntoIterator<Item = Option<String>>,
{
    values
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}

fn looks_like_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if !local.chars().all(is_email_local_char) {
        return false;
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    let mut labels = domain.split('.').peekable();
    if labels.peek().is_none() {
        return false;
    }
    let mut last_label = "";
    for label in labels {
        if label.is_empty()
            || !label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            || label.starts_with('-')
            || label.ends_with('-')
        {
            return false;
        }
        last_label = label;
    }
    last_label.len() >= 2 && last_label.chars().all(|ch| ch.is_ascii_alphabetic())
}

fn is_email_local_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-')
}

fn trimmed_str(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        delete_auth_file, extract_email_from_auth_file_name, extract_plan_from_auth_file_name,
        list_auth_files, patch_auth_file_fields, patch_auth_file_status, read_auth_file,
        write_auth_file, PatchAuthFileFields, PatchAuthFileStatus,
    };
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde_json::Value;
    use std::fs;
    use tempfile::TempDir;

    fn build_jwt(payload_json: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn extracts_email_and_plan_from_auth_file_name() {
        assert_eq!(
            extract_email_from_auth_file_name("codex-d2c2763e-demo.user@example.com-team.json")
                .as_deref(),
            Some("demo.user@example.com")
        );
        assert_eq!(
            extract_email_from_auth_file_name("gemini-web-demo@example.com.json").as_deref(),
            Some("demo@example.com")
        );
        assert_eq!(
            extract_email_from_auth_file_name("gemini-demo@example.com-auto-project-123.json"),
            None
        );
        assert_eq!(
            extract_plan_from_auth_file_name("codex-demo.user@example.com-team.json").as_deref(),
            Some("team")
        );
    }

    #[test]
    fn reads_and_lists_auth_files() {
        let temp_dir = TempDir::new().expect("temp dir");
        write_auth_file(
            temp_dir.path(),
            "codex-demo.json",
            br#"{"type":"codex","email":"demo@example.com","note":"Workspace A"}"#,
        )
        .expect("write");

        let files = list_auth_files(temp_dir.path()).expect("list");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].provider, "codex");
        assert!(!files[0].disabled);
        assert_eq!(files[0].status, "active");
        assert_eq!(files[0].email.as_deref(), Some("demo@example.com"));

        let raw = read_auth_file(temp_dir.path(), "codex-demo.json").expect("read");
        let json: Value = serde_json::from_slice(&raw).expect("json");
        assert_eq!(json["note"].as_str(), Some("Workspace A"));
    }

    #[test]
    fn list_auth_files_falls_back_to_filename_email_and_plan() {
        let temp_dir = TempDir::new().expect("temp dir");
        fs::write(
            temp_dir
                .path()
                .join("codex-d2c2763e-demo@example.com-team.json"),
            r#"{"type":"codex","note":"Workspace A"}"#,
        )
        .expect("seed");

        let files = list_auth_files(temp_dir.path()).expect("list");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].email.as_deref(), Some("demo@example.com"));
        assert_eq!(files[0].account_plan.as_deref(), Some("team"));
        assert_eq!(files[0].note.as_deref(), Some("Workspace A"));
    }

    #[test]
    fn list_auth_files_derives_codex_note_from_matching_account_id() {
        let temp_dir = TempDir::new().expect("temp dir");
        let token = build_jwt(
            r#"{
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "org-default",
                    "organizations": [
                        { "id": "org-default", "title": "Workspace A", "is_default": true },
                        { "id": "org-team", "title": "Workspace B", "is_default": false }
                    ]
                }
            }"#,
        );
        fs::write(
            temp_dir.path().join("codex-demo@example.com-team.json"),
            format!(
                r#"{{
                    "type": "codex",
                    "account_id": "org-team",
                    "id_token": "{token}"
                }}"#
            ),
        )
        .expect("seed");

        let files = list_auth_files(temp_dir.path()).expect("list");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].note.as_deref(), Some("Workspace B"));
    }

    #[test]
    fn list_auth_files_derives_codex_note_from_default_workspace() {
        let temp_dir = TempDir::new().expect("temp dir");
        let token = build_jwt(
            r#"{
                "https://api.openai.com/auth": {
                    "organizations": [
                        { "id": "org-default", "title": "Workspace A", "is_default": true },
                        { "id": "org-team", "title": "Workspace B", "is_default": false }
                    ]
                }
            }"#,
        );
        fs::write(
            temp_dir.path().join("codex-demo@example.com-team.json"),
            format!(
                r#"{{
                    "type": "codex",
                    "id_token": "{token}"
                }}"#
            ),
        )
        .expect("seed");

        let files = list_auth_files(temp_dir.path()).expect("list");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].note.as_deref(), Some("Workspace A"));
    }

    #[test]
    fn list_auth_files_derives_codex_note_from_first_workspace_when_default_missing() {
        let temp_dir = TempDir::new().expect("temp dir");
        let token = build_jwt(
            r#"{
                "https://api.openai.com/auth": {
                    "organizations": [
                        { "id": "org-one", "title": "Workspace A", "is_default": false },
                        { "id": "org-two", "title": "Workspace B", "is_default": false }
                    ]
                }
            }"#,
        );
        fs::write(
            temp_dir.path().join("codex-demo@example.com-team.json"),
            format!(
                r#"{{
                    "type": "codex",
                    "id_token": "{token}"
                }}"#
            ),
        )
        .expect("seed");

        let files = list_auth_files(temp_dir.path()).expect("list");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].note.as_deref(), Some("Workspace A"));
    }

    #[test]
    fn patches_and_deletes_auth_file() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("claude-demo.json");
        fs::write(&path, r#"{"type":"claude","email":"demo@example.com"}"#).expect("seed");

        patch_auth_file_fields(
            temp_dir.path(),
            "claude-demo.json",
            &PatchAuthFileFields {
                note: Some("My Claude".to_string()),
                ..PatchAuthFileFields::default()
            },
        )
        .expect("patch");

        let raw = fs::read_to_string(&path).expect("patched");
        let json: Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(json["note"].as_str(), Some("My Claude"));

        delete_auth_file(temp_dir.path(), "claude-demo.json").expect("delete");
        assert!(!path.exists());
    }

    #[test]
    fn patches_auth_file_status_and_reflects_it_in_list_results() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("codex-demo.json");
        fs::write(&path, r#"{"type":"codex","email":"demo@example.com"}"#).expect("seed");

        patch_auth_file_status(
            temp_dir.path(),
            "codex-demo.json",
            PatchAuthFileStatus { disabled: true },
        )
        .expect("disable");

        let disabled_raw = fs::read_to_string(&path).expect("disabled");
        let disabled_json: Value = serde_json::from_str(&disabled_raw).expect("json");
        assert_eq!(disabled_json["disabled"].as_bool(), Some(true));
        assert_eq!(disabled_json["status"].as_str(), Some("disabled"));
        assert_eq!(
            disabled_json["status_message"].as_str(),
            Some("disabled via management API")
        );

        let disabled_files = list_auth_files(temp_dir.path()).expect("list disabled");
        assert_eq!(disabled_files[0].status, "disabled");
        assert!(disabled_files[0].disabled);
        assert_eq!(
            disabled_files[0].status_message.as_deref(),
            Some("disabled via management API")
        );

        patch_auth_file_status(
            temp_dir.path(),
            "codex-demo.json",
            PatchAuthFileStatus { disabled: false },
        )
        .expect("enable");

        let enabled_raw = fs::read_to_string(&path).expect("enabled");
        let enabled_json: Value = serde_json::from_str(&enabled_raw).expect("json");
        assert_eq!(enabled_json["disabled"].as_bool(), Some(false));
        assert_eq!(enabled_json["status"].as_str(), Some("active"));
        assert!(enabled_json.get("status_message").is_none());

        let enabled_files = list_auth_files(temp_dir.path()).expect("list enabled");
        assert_eq!(enabled_files[0].status, "active");
        assert!(!enabled_files[0].disabled);
        assert_eq!(enabled_files[0].status_message, None);
    }
}
