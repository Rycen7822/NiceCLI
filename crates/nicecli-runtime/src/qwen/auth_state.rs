use super::*;
use nicecli_auth::{read_auth_file, write_auth_file};
use reqwest::Proxy;
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Clone)]
pub(super) struct QwenAuthState {
    pub(super) root: Map<String, Value>,
    pub(super) access_token: Option<String>,
    pub(super) refresh_token: Option<String>,
    pub(super) base_url: String,
    pub(super) proxy_url: Option<String>,
    pub(super) expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RefreshedQwenToken {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) token_type: Option<String>,
    pub(super) resource_url: Option<String>,
    pub(super) base_url: Option<String>,
    pub(super) expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct RefreshTokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    resource_url: String,
    #[serde(default)]
    expires_in: i64,
}

pub(super) fn load_qwen_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    endpoints: &QwenCallerEndpoints,
) -> Result<Option<QwenAuthState>, QwenCallerError> {
    let raw = read_auth_file(auth_dir, auth_file_name).map_err(QwenCallerError::ReadAuthFile)?;
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|error| QwenCallerError::InvalidAuthFile(error.to_string()))?;
    let root = value
        .as_object()
        .ok_or_else(|| QwenCallerError::InvalidAuthFile("root must be an object".to_string()))?;

    let provider = first_non_empty([
        string_path(root, &["provider"]),
        string_path(root, &["type"]),
    ]);
    if !provider
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("qwen"))
    {
        return Ok(None);
    }

    Ok(Some(QwenAuthState {
        root: root.clone(),
        access_token: first_non_empty([
            string_path(root, &["access_token"]),
            string_path(root, &["metadata", "access_token"]),
            string_path(root, &["attributes", "access_token"]),
        ]),
        refresh_token: first_non_empty([
            string_path(root, &["refresh_token"]),
            string_path(root, &["metadata", "refresh_token"]),
            string_path(root, &["attributes", "refresh_token"]),
        ]),
        base_url: first_non_empty([
            string_path(root, &["base_url"]),
            string_path(root, &["metadata", "base_url"]),
            string_path(root, &["attributes", "base_url"]),
        ])
        .or_else(|| {
            first_non_empty([
                string_path(root, &["resource_url"]),
                string_path(root, &["metadata", "resource_url"]),
                string_path(root, &["attributes", "resource_url"]),
            ])
            .and_then(|value| normalize_resource_base_url(&value))
        })
        .unwrap_or_else(|| endpoints.api_base_url.trim().to_string()),
        proxy_url: trimmed(first_non_empty([
            string_path(root, &["proxy_url"]),
            string_path(root, &["metadata", "proxy_url"]),
            string_path(root, &["attributes", "proxy_url"]),
        ])),
        expired_at: first_non_empty([
            string_path(root, &["expired"]),
            string_path(root, &["metadata", "expired"]),
            string_path(root, &["attributes", "expired"]),
        ])
        .and_then(|value| parse_datetime(&value)),
    }))
}

pub(super) fn parse_refresh_token_response(
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<RefreshedQwenToken, QwenCallerError> {
    let parsed: RefreshTokenResponse = serde_json::from_slice(body)
        .map_err(|error| QwenCallerError::InvalidAuthFile(error.to_string()))?;
    let access_token = parsed.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err(QwenCallerError::MissingAccessToken);
    }

    let resource_url = trimmed(Some(parsed.resource_url));
    Ok(RefreshedQwenToken {
        access_token,
        refresh_token: trimmed(Some(parsed.refresh_token)),
        token_type: trimmed(Some(parsed.token_type)),
        base_url: resource_url
            .as_deref()
            .and_then(normalize_resource_base_url),
        resource_url,
        expired_at: (parsed.expires_in > 0).then(|| now + Duration::seconds(parsed.expires_in)),
    })
}

pub(super) fn apply_refreshed_auth_state(
    auth: &mut QwenAuthState,
    refreshed: &RefreshedQwenToken,
    now: DateTime<Utc>,
) {
    auth.access_token = Some(refreshed.access_token.clone());
    if let Some(refresh_token) = refreshed.refresh_token.clone() {
        auth.refresh_token = Some(refresh_token);
    }
    if let Some(base_url) = refreshed.base_url.clone() {
        auth.base_url = base_url;
    }
    auth.expired_at = refreshed.expired_at;

    upsert_string(
        &mut auth.root,
        "access_token",
        Some(refreshed.access_token.as_str()),
    );
    upsert_string(
        &mut auth.root,
        "refresh_token",
        auth.refresh_token.as_deref(),
    );
    upsert_string(
        &mut auth.root,
        "token_type",
        refreshed.token_type.as_deref(),
    );
    upsert_string(
        &mut auth.root,
        "resource_url",
        refreshed.resource_url.as_deref(),
    );
    upsert_string(&mut auth.root, "provider", Some("qwen"));
    upsert_string(&mut auth.root, "type", Some("qwen"));
    upsert_string(&mut auth.root, "last_refresh", Some(&now.to_rfc3339()));
    upsert_string(
        &mut auth.root,
        "expired",
        refreshed
            .expired_at
            .map(|value| value.to_rfc3339())
            .as_deref(),
    );
}

pub(super) fn persist_qwen_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    auth: &QwenAuthState,
) -> Result<(), QwenCallerError> {
    let bytes = serde_json::to_vec_pretty(&Value::Object(auth.root.clone()))
        .map_err(|error| QwenCallerError::InvalidAuthFile(error.to_string()))?;
    write_auth_file(auth_dir, auth_file_name, &bytes).map_err(QwenCallerError::WriteAuthFile)?;
    Ok(())
}

pub(super) fn build_http_client(proxy_url: Option<&str>) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy_url.and_then(|value| trimmed(Some(value.to_string()))) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

pub(super) fn refresh_due(auth: &QwenAuthState, now: DateTime<Utc>) -> bool {
    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return auth.refresh_token.is_some();
    }

    auth.expired_at
        .map(|expired_at| expired_at <= now + Duration::seconds(QWEN_REFRESH_LEAD_SECS))
        .unwrap_or(false)
}

pub(super) fn normalize_resource_base_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let normalized = with_scheme.trim_end_matches('/').to_string();
    if normalized.to_ascii_lowercase().ends_with("/v1") {
        Some(normalized)
    } else {
        Some(format!("{normalized}/v1"))
    }
}

pub(super) fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

pub(super) fn upsert_string(root: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            root.insert(key.to_string(), Value::String(value.to_string()));
        }
        None => {
            root.remove(key);
        }
    }
}

pub(super) fn string_path(root: &Map<String, Value>, path: &[&str]) -> Option<String> {
    let mut current = root.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        current = current.as_object()?.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn value_string_path(root: &Value, path: &[&str]) -> Option<String> {
    let mut current = root;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn first_non_empty<const N: usize>(values: [Option<String>; N]) -> Option<String> {
    values.into_iter().find_map(|value| {
        value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
