use serde::{Deserialize, Serialize};

pub const PROVIDER_CODEX: &str = "codex";
pub const SOURCE_INLINE_RATE_LIMITS: &str = "inline_rate_limits";
pub const SOURCE_USAGE_DASHBOARD: &str = "usage_dashboard";
pub const DEFAULT_WORKSPACE_ID: &str = "current";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitWindow {
    pub used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreditsSnapshot {
    pub has_credits: bool,
    pub unlimited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<RateLimitWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<RateLimitWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<CreditsSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexQuotaSnapshotEnvelope {
    pub provider: String,
    pub auth_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<RateLimitSnapshot>,
    pub source: String,
    pub fetched_at: String,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotListResponse {
    pub provider: String,
    pub snapshots: Vec<CodexQuotaSnapshotEnvelope>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ListOptions {
    pub refresh: bool,
    pub auth_id: String,
    pub workspace_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RefreshOptions {
    pub auth_id: String,
    pub workspace_id: String,
}

impl SnapshotListResponse {
    pub fn empty_codex() -> Self {
        Self {
            provider: PROVIDER_CODEX.to_string(),
            snapshots: Vec::new(),
        }
    }

    pub fn from_snapshots(snapshots: Vec<CodexQuotaSnapshotEnvelope>) -> Self {
        Self {
            provider: PROVIDER_CODEX.to_string(),
            snapshots,
        }
    }
}

pub fn normalize_provider(provider: &str) -> String {
    let trimmed = provider.trim();
    if trimmed.is_empty() {
        PROVIDER_CODEX.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{SnapshotListResponse, PROVIDER_CODEX};

    #[test]
    fn empty_response_uses_codex_provider() {
        let response = SnapshotListResponse::empty_codex();
        assert_eq!(response.provider, PROVIDER_CODEX);
        assert!(response.snapshots.is_empty());
    }
}
