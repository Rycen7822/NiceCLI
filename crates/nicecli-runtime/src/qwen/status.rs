use super::*;
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use reqwest::header::HeaderMap;
use serde_json::Value;

pub(super) fn qwen_status_failure(
    status: u16,
    headers: &HeaderMap,
    body: &[u8],
    model: &str,
    now: DateTime<Utc>,
) -> ExecutionFailure<QwenCallerError> {
    let status = normalized_qwen_status(status, body);
    let message = response_body_message(body);
    let error = QwenCallerError::UnexpectedStatus {
        status,
        body: message.clone(),
    };
    let result = ExecutionResult {
        model: normalized_model(model),
        success: false,
        retry_after: parse_retry_after(status, headers, body, now),
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

pub(super) fn time_until_next_beijing_midnight(now: DateTime<Utc>) -> Duration {
    let offset = FixedOffset::east_opt(8 * 60 * 60).expect("valid offset");
    let local_now = now.with_timezone(&offset);
    let next_date = local_now
        .date_naive()
        .succ_opt()
        .unwrap_or_else(|| local_now.date_naive());
    let next_midnight = offset
        .from_local_datetime(
            &next_date
                .and_hms_opt(0, 0, 0)
                .expect("valid midnight datetime"),
        )
        .single()
        .expect("single offset datetime")
        .with_timezone(&Utc);
    let wait = next_midnight - now;
    if wait <= Duration::zero() {
        Duration::seconds(1)
    } else {
        wait
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

fn normalized_qwen_status(status: u16, body: &[u8]) -> u16 {
    if matches!(status, 403 | 429) && is_qwen_quota_error(body) {
        429
    } else {
        status
    }
}

fn parse_retry_after(
    status: u16,
    headers: &HeaderMap,
    body: &[u8],
    now: DateTime<Utc>,
) -> Option<Duration> {
    if status != 429 {
        return None;
    }

    let header_retry_after = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::seconds);
    if header_retry_after.is_some() {
        return header_retry_after;
    }

    if is_qwen_quota_error(body) {
        return Some(time_until_next_beijing_midnight(now));
    }

    None
}

fn is_qwen_quota_error(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };

    let code = value_string_path(&value, &["error", "code"])
        .unwrap_or_default()
        .to_ascii_lowercase();
    let error_type = value_string_path(&value, &["error", "type"])
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(code.as_str(), "insufficient_quota" | "quota_exceeded")
        || matches!(error_type.as_str(), "insufficient_quota" | "quota_exceeded")
    {
        return true;
    }

    let message = response_body_message(body).to_ascii_lowercase();
    !message.is_empty()
        && (message.contains("insufficient_quota")
            || message.contains("quota exceeded")
            || message.contains("free allocated quota exceeded"))
}
