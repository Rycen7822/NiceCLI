use crate::{CreditsSnapshot, RateLimitSnapshot, RateLimitWindow, PROVIDER_CODEX};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NormalizeError {
    #[error("failed to decode codex usage payload: {0}")]
    Decode(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct CodexUsagePayload {
    #[serde(default)]
    plan_type: Option<String>,
    #[serde(default)]
    rate_limit: Option<CodexUsageRateLimitDetails>,
    #[serde(default)]
    credits: Option<CodexUsageCreditsDetails>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageRateLimitDetails {
    #[serde(default)]
    primary_window: Option<CodexUsageWindow>,
    #[serde(default)]
    secondary_window: Option<CodexUsageWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageCreditsDetails {
    #[serde(default)]
    has_credits: bool,
    #[serde(default)]
    unlimited: bool,
    #[serde(default)]
    balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageWindow {
    #[serde(default)]
    used_percent: f64,
    #[serde(default)]
    limit_window_seconds: i64,
    #[serde(default)]
    reset_at: i64,
}

pub fn normalize_codex_usage(raw: &[u8]) -> Result<Option<RateLimitSnapshot>, NormalizeError> {
    if raw.is_empty() {
        return Ok(None);
    }

    let payload: CodexUsagePayload = serde_json::from_slice(raw)?;
    Ok(Some(RateLimitSnapshot {
        limit_id: Some(PROVIDER_CODEX.to_string()),
        limit_name: None,
        primary: payload
            .rate_limit
            .as_ref()
            .and_then(|rate_limit| rate_limit.primary_window.as_ref())
            .map(normalize_window),
        secondary: payload
            .rate_limit
            .as_ref()
            .and_then(|rate_limit| rate_limit.secondary_window.as_ref())
            .map(normalize_window),
        credits: payload.credits.as_ref().map(normalize_credits),
        plan_type: payload.plan_type.and_then(trimmed),
    }))
}

fn normalize_window(window: &CodexUsageWindow) -> RateLimitWindow {
    RateLimitWindow {
        used_percent: window.used_percent,
        window_minutes: seconds_to_minutes(window.limit_window_seconds),
        resets_at: positive_i64(window.reset_at),
    }
}

fn normalize_credits(credits: &CodexUsageCreditsDetails) -> CreditsSnapshot {
    CreditsSnapshot {
        has_credits: credits.has_credits,
        unlimited: credits.unlimited,
        balance: credits.balance.as_ref().and_then(value_to_string),
    }
}

fn seconds_to_minutes(seconds: i64) -> Option<i64> {
    if seconds <= 0 {
        None
    } else {
        Some((seconds + 59) / 60)
    }
}

fn positive_i64(value: i64) -> Option<i64> {
    if value > 0 {
        Some(value)
    } else {
        None
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => trimmed(text.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        other => trimmed(other.to_string()),
    }
}

fn trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_codex_usage;

    #[test]
    fn normalizes_codex_usage_payload() {
        let snapshot = normalize_codex_usage(
            br#"{
                "plan_type": "team",
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 25,
                        "limit_window_seconds": 18000,
                        "reset_at": 1760000000
                    },
                    "secondary_window": {
                        "used_percent": 10.5,
                        "limit_window_seconds": 604800,
                        "reset_at": 1760500000
                    }
                },
                "credits": {
                    "has_credits": true,
                    "unlimited": false,
                    "balance": "12.5"
                }
            }"#,
        )
        .expect("usage should decode")
        .expect("snapshot should exist");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("team"));
        assert_eq!(
            snapshot.primary.and_then(|window| window.window_minutes),
            Some(300)
        );
        assert_eq!(
            snapshot.secondary.and_then(|window| window.window_minutes),
            Some(10080)
        );
        assert_eq!(
            snapshot.credits.and_then(|credits| credits.balance),
            Some("12.5".to_string())
        );
    }
}
