use reqwest::Client;
use serde_json::{Map, Value};
use thiserror::Error;

pub const DEFAULT_CODEX_ACCOUNT_CHECK_URL: &str =
    "https://chatgpt.com/backend-api/wham/accounts/check";

const ACCOUNT_ID_KEYS: &[&str] = &["id", "account_id", "chatgpt_account_id", "workspace_id"];
const ACCOUNT_NAME_KEYS: &[&str] = &[
    "name",
    "display_name",
    "account_name",
    "organization_name",
    "workspace_name",
    "title",
];
const ACCOUNT_STRUCTURE_KEYS: &[&str] = &[
    "structure",
    "account_structure",
    "kind",
    "type",
    "account_type",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexAccountProfile {
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub account_structure: Option<String>,
}

#[derive(Debug, Error)]
pub enum CodexAccountProfileError {
    #[error("codex account access token is empty")]
    MissingAccessToken,
    #[error("codex account check request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("codex account check returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    #[error("failed to parse codex account check response: {0}")]
    Parse(#[from] serde_json::Error),
}

pub async fn fetch_codex_account_profile(
    client: &Client,
    account_check_url: &str,
    access_token: &str,
    account_id: Option<&str>,
) -> Result<Option<CodexAccountProfile>, CodexAccountProfileError> {
    let access_token =
        trimmed_str(access_token).ok_or(CodexAccountProfileError::MissingAccessToken)?;
    let mut request = client
        .get(account_check_url.trim())
        .bearer_auth(access_token)
        .header("Accept", "application/json");

    if let Some(account_id) = account_id.and_then(trimmed_str) {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let response = request.send().await?;
    let status = response.status();
    let body = response.bytes().await?;
    if !status.is_success() {
        return Err(CodexAccountProfileError::UnexpectedStatus {
            status: status.as_u16(),
            body: String::from_utf8_lossy(&body).trim().to_string(),
        });
    }

    let payload: Value = serde_json::from_slice(&body)?;
    Ok(parse_codex_account_profile(&payload, account_id))
}

pub fn parse_codex_account_profile(
    payload: &Value,
    expected_account_id: Option<&str>,
) -> Option<CodexAccountProfile> {
    let records = collect_account_records(payload);
    if records.is_empty() {
        return None;
    }

    let selected = expected_account_id
        .and_then(trimmed_str)
        .and_then(|expected_id| select_record_by_id(&records, expected_id))
        .or_else(|| {
            payload
                .get("default_account_id")
                .and_then(Value::as_str)
                .and_then(trimmed_str)
                .and_then(|expected_id| select_record_by_id(&records, expected_id))
        })
        .or_else(|| {
            payload
                .get("account_ordering")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(Value::as_str)
                .and_then(trimmed_str)
                .and_then(|expected_id| select_record_by_id(&records, expected_id))
        })
        .or_else(|| records.first().copied())?;

    let profile = CodexAccountProfile {
        account_id: extract_record_field(selected, ACCOUNT_ID_KEYS),
        account_name: extract_record_field(selected, ACCOUNT_NAME_KEYS),
        account_structure: extract_record_field(selected, ACCOUNT_STRUCTURE_KEYS),
    };

    if profile.account_id.is_none()
        && profile.account_name.is_none()
        && profile.account_structure.is_none()
    {
        None
    } else {
        Some(profile)
    }
}

pub fn is_generic_codex_workspace_name(value: &str) -> bool {
    let normalized = normalize_lookup(value);
    if normalized.is_empty() {
        return true;
    }

    matches!(
        normalized.as_str(),
        "personal"
            | "personal account"
            | "personal workspace"
            | "current workspace"
            | "workspace"
            | "unknown"
            | "business"
            | "business workspace"
    )
}

fn collect_account_records(payload: &Value) -> Vec<&Map<String, Value>> {
    let mut records = Vec::new();

    if let Some(accounts_value) = payload.get("accounts") {
        match accounts_value {
            Value::Array(items) => {
                for item in items {
                    if let Some(record) = item.as_object() {
                        records.push(record);
                    }
                }
            }
            Value::Object(object) => {
                for value in object.values() {
                    if let Some(record) = value.as_object() {
                        records.push(record);
                    }
                }
            }
            _ => {}
        }
    }

    if records.is_empty() {
        if let Some(items) = payload.as_array() {
            for item in items {
                if let Some(record) = item.as_object() {
                    records.push(record);
                }
            }
        }
    }

    records
}

fn select_record_by_id<'a>(
    records: &[&'a Map<String, Value>],
    expected_id: &str,
) -> Option<&'a Map<String, Value>> {
    records.iter().copied().find(|record| {
        extract_record_field(record, ACCOUNT_ID_KEYS)
            .as_deref()
            .map(|candidate| candidate.eq_ignore_ascii_case(expected_id))
            .unwrap_or(false)
    })
}

fn extract_record_field(record: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        record
            .get(*key)
            .and_then(Value::as_str)
            .and_then(trimmed_str)
            .map(str::to_string)
    })
}

fn normalize_lookup(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
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
    use super::{is_generic_codex_workspace_name, parse_codex_account_profile};
    use serde_json::json;

    #[test]
    fn parses_matching_account_profile_from_accounts_payload() {
        let payload = json!({
            "accounts": [
                {
                    "id": "org-personal",
                    "structure": "workspace",
                    "name": "Personal"
                },
                {
                    "id": "org-team",
                    "structure": "workspace",
                    "name": "MyTeam"
                }
            ],
            "default_account_id": "org-personal",
            "account_ordering": ["org-personal", "org-team"]
        });

        let profile = parse_codex_account_profile(&payload, Some("org-team")).expect("profile");
        assert_eq!(profile.account_id.as_deref(), Some("org-team"));
        assert_eq!(profile.account_name.as_deref(), Some("MyTeam"));
        assert_eq!(profile.account_structure.as_deref(), Some("workspace"));
    }

    #[test]
    fn detects_generic_workspace_names() {
        assert!(is_generic_codex_workspace_name("Personal"));
        assert!(is_generic_codex_workspace_name("Current Workspace"));
        assert!(is_generic_codex_workspace_name("business"));
        assert!(!is_generic_codex_workspace_name("MyTeam"));
    }
}
