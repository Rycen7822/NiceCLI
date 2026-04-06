use super::*;
use chrono::Duration;
use reqwest::header::HeaderMap;
use serde_json::Value;

pub(super) fn antigravity_status_failure(
    status: u16,
    headers: &HeaderMap,
    body: &[u8],
    model: &str,
) -> ExecutionFailure<AntigravityCallerError> {
    let message = response_body_message(body);
    let error = AntigravityCallerError::UnexpectedStatus {
        status,
        body: message.clone(),
    };
    let result = ExecutionResult {
        model: normalized_model(model),
        success: false,
        retry_after: parse_retry_after(status, headers),
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

pub(super) fn response_body_message(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(message) = first_non_empty([
            value_string_path(&value, &["error", "message"]),
            value_string_path(&value, &["message"]),
            value_string_path(&value, &["error_description"]),
            value_string_path(&value, &["error"]),
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

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 401 | 402 | 403 | 408 | 429 | 500 | 502 | 503 | 504)
}

fn parse_retry_after(status: u16, headers: &HeaderMap) -> Option<Duration> {
    if status != 429 {
        return None;
    }
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::seconds)
}
