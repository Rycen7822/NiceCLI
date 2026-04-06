use super::{CodexAuthCredentials, CodexCallerError, DEFAULT_CODEX_BASE_URL};
use crate::{ExecutionError, ExecutionFailure, ExecutionResult};
use chrono::{Duration, Utc};
use nicecli_auth::read_auth_file;
use reqwest::{Client, Proxy};
use serde_json::{Map, Value};
use std::path::Path;

pub(super) fn load_codex_auth_credentials(
    auth_dir: &Path,
    auth_file_name: &str,
) -> Result<Option<CodexAuthCredentials>, CodexCallerError> {
    let raw = read_auth_file(auth_dir, auth_file_name)?;
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|error| CodexCallerError::InvalidAuthFile(error.to_string()))?;
    let root = value
        .as_object()
        .ok_or_else(|| CodexCallerError::InvalidAuthFile("root must be an object".to_string()))?;

    let access_token = first_non_empty([
        string_path(root, &["access_token"]),
        string_path(root, &["metadata", "access_token"]),
        string_path(root, &["attributes", "access_token"]),
    ]);
    if access_token.is_none() {
        return Ok(None);
    }

    Ok(Some(CodexAuthCredentials {
        access_token: access_token.unwrap_or_default(),
        base_url: first_non_empty([
            string_path(root, &["base_url"]),
            string_path(root, &["metadata", "base_url"]),
            string_path(root, &["attributes", "base_url"]),
            Some(DEFAULT_CODEX_BASE_URL.to_string()),
        ])
        .unwrap_or_else(|| DEFAULT_CODEX_BASE_URL.to_string()),
        proxy_url: trimmed(first_non_empty([
            string_path(root, &["proxy_url"]),
            string_path(root, &["metadata", "proxy_url"]),
            string_path(root, &["attributes", "proxy_url"]),
        ])),
    }))
}

pub(super) fn build_http_client(proxy_url: Option<&str>) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy_url.and_then(|value| trimmed(Some(value.to_string()))) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

pub(super) fn codex_auth_failure(error: CodexCallerError) -> ExecutionFailure<CodexCallerError> {
    ExecutionFailure::retryable(
        error,
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message: "unauthorized".to_string(),
                http_status: Some(401),
            }),
        },
    )
}

pub(super) fn codex_local_failure(error: CodexCallerError) -> ExecutionFailure<CodexCallerError> {
    ExecutionFailure::retryable(
        error,
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message: "unauthorized".to_string(),
                http_status: Some(401),
            }),
        },
    )
}

pub(super) fn codex_request_failure(error: reqwest::Error) -> ExecutionFailure<CodexCallerError> {
    let message = error.to_string();
    ExecutionFailure::retryable(
        CodexCallerError::Request(error),
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message,
                http_status: Some(503),
            }),
        },
    )
}

pub(super) fn codex_status_failure(
    status: u16,
    body: &[u8],
    model: &str,
) -> ExecutionFailure<CodexCallerError> {
    let status = normalized_codex_status(status, body);
    let message = response_body_message(body);
    let error = CodexCallerError::UnexpectedStatus {
        status,
        body: message.clone(),
    };
    let result = ExecutionResult {
        model: normalized_model(model),
        success: false,
        retry_after: parse_retry_after(status, body),
        error: Some(ExecutionError {
            message,
            http_status: Some(status),
        }),
    };
    if is_retryable_status(status) {
        ExecutionFailure::retryable(error, result)
    } else {
        ExecutionFailure::terminal(error, result)
    }
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 401 | 402 | 403 | 408 | 429 | 500 | 502 | 503 | 504)
}

fn response_body_message(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(message) = first_non_empty([
            value_string_path(&value, &["error", "message"]),
            value_string_path(&value, &["message"]),
        ]) {
            return message;
        }
    }

    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        "request failed".to_string()
    } else {
        trimmed
    }
}

fn normalized_codex_status(status: u16, body: &[u8]) -> u16 {
    if status == 400 && is_model_capacity_error(body) {
        429
    } else {
        status
    }
}

fn is_model_capacity_error(body: &[u8]) -> bool {
    let lower = response_body_message(body).trim().to_ascii_lowercase();
    !lower.is_empty()
        && (lower.contains("selected model is at capacity")
            || lower.contains("model is at capacity. please try a different model"))
}

fn parse_retry_after(status: u16, body: &[u8]) -> Option<Duration> {
    if status != 429 {
        return None;
    }

    let value = serde_json::from_slice::<Value>(body).ok()?;
    let error = value.get("error")?;
    if !matches!(
        error
            .get("type")
            .and_then(json_string)
            .map(|value| value.eq_ignore_ascii_case("usage_limit_reached")),
        Some(true)
    ) {
        return None;
    }

    if let Some(resets_at) = error.get("resets_at").and_then(json_i64) {
        let now = Utc::now().timestamp();
        if resets_at > now {
            return Some(Duration::seconds(resets_at - now));
        }
    }

    error
        .get("resets_in_seconds")
        .and_then(json_i64)
        .filter(|seconds| *seconds > 0)
        .map(Duration::seconds)
}

fn string_path(root: &Map<String, Value>, path: &[&str]) -> Option<String> {
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

fn first_non_empty<const N: usize>(values: [Option<String>; N]) -> Option<String> {
    values.into_iter().find_map(|value| {
        value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn value_string_path(root: &Value, path: &[&str]) -> Option<String> {
    let mut current = root;
    for segment in path {
        current = current.get(*segment)?;
    }
    json_string(current)
}

fn json_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn json_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| value.trim().parse::<i64>().ok())
        })
}

fn normalized_model(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
