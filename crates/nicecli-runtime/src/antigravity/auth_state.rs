use super::*;
use nicecli_auth::{read_auth_file, write_auth_file};
use reqwest::Proxy;
use serde_json::{Map, Number, Value};
use std::path::Path;

#[derive(Debug, Clone)]
pub(super) struct AntigravityAuthState {
    pub(super) root: Map<String, Value>,
    pub(super) access_token: Option<String>,
    pub(super) refresh_token: Option<String>,
    pub(super) base_url: String,
    pub(super) proxy_url: Option<String>,
    pub(super) user_agent: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RefreshedAntigravityToken {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) token_type: Option<String>,
    pub(super) expires_in: Option<i64>,
    pub(super) expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Deserialize)]
struct RefreshTokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: i64,
}

pub(super) fn load_antigravity_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    endpoints: &AntigravityCallerEndpoints,
) -> Result<Option<AntigravityAuthState>, AntigravityCallerError> {
    let raw =
        read_auth_file(auth_dir, auth_file_name).map_err(AntigravityCallerError::ReadAuthFile)?;
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|error| AntigravityCallerError::InvalidAuthFile(error.to_string()))?;
    let root = value.as_object().ok_or_else(|| {
        AntigravityCallerError::InvalidAuthFile("root must be an object".to_string())
    })?;

    let provider = first_non_empty([
        string_path(root, &["provider"]),
        string_path(root, &["type"]),
    ]);
    if !provider
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("antigravity"))
    {
        return Ok(None);
    }

    Ok(Some(AntigravityAuthState {
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
        .and_then(|value| normalize_base_url(&value))
        .unwrap_or_else(|| endpoints.api_base_url.trim_end_matches('/').to_string()),
        proxy_url: trimmed(first_non_empty([
            string_path(root, &["proxy_url"]),
            string_path(root, &["metadata", "proxy_url"]),
            string_path(root, &["attributes", "proxy_url"]),
        ])),
        user_agent: trimmed(first_non_empty([
            string_path(root, &["user_agent"]),
            string_path(root, &["metadata", "user_agent"]),
            string_path(root, &["attributes", "user_agent"]),
        ])),
        project_id: trimmed(first_non_empty([
            string_path(root, &["project_id"]),
            string_path(root, &["metadata", "project_id"]),
            string_path(root, &["attributes", "project_id"]),
        ])),
        expired_at: parse_expired_at(root),
    }))
}

pub(super) fn parse_refresh_token_response(
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<RefreshedAntigravityToken, AntigravityCallerError> {
    let parsed: RefreshTokenResponse = serde_json::from_slice(body)
        .map_err(|error| AntigravityCallerError::InvalidAuthFile(error.to_string()))?;
    let access_token = parsed.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err(AntigravityCallerError::MissingAccessToken);
    }

    let expires_in = (parsed.expires_in > 0).then_some(parsed.expires_in);
    Ok(RefreshedAntigravityToken {
        access_token,
        refresh_token: trimmed(Some(parsed.refresh_token)),
        token_type: trimmed(Some(parsed.token_type)),
        expires_in,
        expired_at: expires_in.map(|value| now + Duration::seconds(value)),
    })
}

pub(super) fn apply_refreshed_auth_state(
    auth: &mut AntigravityAuthState,
    refreshed: &RefreshedAntigravityToken,
    now: DateTime<Utc>,
) {
    auth.access_token = Some(refreshed.access_token.clone());
    if let Some(refresh_token) = refreshed.refresh_token.clone() {
        auth.refresh_token = Some(refresh_token);
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
    upsert_string(&mut auth.root, "provider", Some("antigravity"));
    upsert_string(&mut auth.root, "type", Some("antigravity"));
    upsert_string(&mut auth.root, "last_refresh", Some(&now.to_rfc3339()));
    upsert_string(
        &mut auth.root,
        "expired",
        refreshed
            .expired_at
            .map(|value| value.to_rfc3339())
            .as_deref(),
    );
    upsert_i64(&mut auth.root, "timestamp", Some(now.timestamp_millis()));
    upsert_i64(&mut auth.root, "expires_in", refreshed.expires_in);
}

pub(super) fn persist_antigravity_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    auth: &AntigravityAuthState,
) -> Result<(), AntigravityCallerError> {
    let bytes = serde_json::to_vec_pretty(&Value::Object(auth.root.clone()))
        .map_err(|error| AntigravityCallerError::InvalidAuthFile(error.to_string()))?;
    write_auth_file(auth_dir, auth_file_name, &bytes)
        .map_err(AntigravityCallerError::WriteAuthFile)?;
    Ok(())
}

pub(super) fn build_http_client(proxy_url: Option<&str>) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy_url.and_then(|value| trimmed(Some(value.to_string()))) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

pub(super) fn refresh_due(auth: &AntigravityAuthState, now: DateTime<Utc>) -> bool {
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
        .map(|expired_at| expired_at <= now + Duration::seconds(ANTIGRAVITY_REFRESH_LEAD_SECS))
        .unwrap_or(false)
}

pub(super) fn parse_expired_at(root: &Map<String, Value>) -> Option<DateTime<Utc>> {
    first_non_empty([
        string_path(root, &["expired"]),
        string_path(root, &["metadata", "expired"]),
        string_path(root, &["attributes", "expired"]),
    ])
    .and_then(|value| parse_datetime(&value))
    .or_else(|| {
        let expires_in = first_non_empty_i64([
            i64_path(root, &["expires_in"]),
            i64_path(root, &["metadata", "expires_in"]),
            i64_path(root, &["attributes", "expires_in"]),
        ])?;
        let timestamp = first_non_empty_i64([
            i64_path(root, &["timestamp"]),
            i64_path(root, &["metadata", "timestamp"]),
            i64_path(root, &["attributes", "timestamp"]),
        ])?;
        Utc.timestamp_millis_opt(timestamp)
            .single()
            .map(|base| base + Duration::seconds(expires_in))
    })
}

pub(super) fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

pub(super) fn normalize_base_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    Some(with_scheme.trim_end_matches('/').to_string())
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

pub(super) fn upsert_i64(root: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    match value {
        Some(value) => {
            root.insert(key.to_string(), Value::Number(Number::from(value)));
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

pub(super) fn i64_path(root: &Map<String, Value>, path: &[&str]) -> Option<i64> {
    let mut current = root.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        current = current.as_object()?.get(*segment)?;
    }
    match current {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.trim().parse::<i64>().ok(),
        _ => None,
    }
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

pub(super) fn first_non_empty_i64<const N: usize>(values: [Option<i64>; N]) -> Option<i64> {
    values.into_iter().flatten().next()
}

pub(super) fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
