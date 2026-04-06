use crate::{
    apply_execution_result, decide_persist, AuthCandidateModelState, AuthCandidateQuotaState,
    AuthCandidateState, ExecutionError, ExecutionResult, PersistDecision,
};
use chrono::{DateTime, SecondsFormat, Utc};
use nicecli_auth::{
    list_auth_files, read_auth_file, write_auth_file, AuthFileEntry, AuthFileStoreError,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthStoreError {
    #[error("failed to read auth snapshots: {0}")]
    FileStore(#[from] AuthFileStoreError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordExecutionResultOptions {
    pub now: DateTime<Utc>,
    pub skip_persist: bool,
    pub runtime_only: bool,
    pub disable_cooling: bool,
}

impl RecordExecutionResultOptions {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            now,
            skip_persist: false,
            runtime_only: false,
            disable_cooling: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordExecutionResultOutcome {
    pub decision: PersistDecision,
    pub changed: bool,
    pub snapshot: Option<AuthSnapshot>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthSnapshot {
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
    #[serde(skip)]
    pub prefix: Option<String>,
    #[serde(skip)]
    pub proxy_url: Option<String>,
    #[serde(skip)]
    pub candidate_state: AuthCandidateState,
}

impl From<AuthFileEntry> for AuthSnapshot {
    fn from(entry: AuthFileEntry) -> Self {
        Self {
            id: entry.id,
            name: entry.name,
            provider_type: entry.provider_type,
            provider: entry.provider,
            source: entry.source,
            size: entry.size,
            modtime: entry.modtime,
            disabled: entry.disabled,
            status: entry.status,
            status_message: entry.status_message,
            email: entry.email,
            account_plan: entry.account_plan,
            note: entry.note,
            priority: entry.priority,
            prefix: None,
            proxy_url: None,
            candidate_state: AuthCandidateState::default(),
        }
    }
}

pub trait AuthStore {
    fn list_snapshots(&self) -> Result<Vec<AuthSnapshot>, AuthStoreError>;

    fn find_snapshot(&self, name_or_id: &str) -> Result<Option<AuthSnapshot>, AuthStoreError> {
        let needle = name_or_id.trim();
        if needle.is_empty() {
            return Ok(None);
        }

        Ok(self.list_snapshots()?.into_iter().find(|snapshot| {
            snapshot.name.eq_ignore_ascii_case(needle) || snapshot.id.eq_ignore_ascii_case(needle)
        }))
    }
}

#[derive(Debug, Clone)]
pub struct FileAuthStore {
    auth_dir: PathBuf,
}

impl FileAuthStore {
    pub fn new(auth_dir: impl Into<PathBuf>) -> Self {
        Self {
            auth_dir: auth_dir.into(),
        }
    }

    pub fn auth_dir(&self) -> &Path {
        &self.auth_dir
    }

    pub fn record_execution_result(
        &self,
        name_or_id: &str,
        result: &ExecutionResult,
        options: RecordExecutionResultOptions,
    ) -> Result<RecordExecutionResultOutcome, AuthStoreError> {
        let snapshot = self
            .find_snapshot(name_or_id)?
            .ok_or(AuthStoreError::FileStore(AuthFileStoreError::NotFound))?;

        let decision = decide_persist(options.skip_persist, options.runtime_only, true);
        if decision != PersistDecision::Persist {
            return Ok(RecordExecutionResultOutcome {
                decision,
                changed: false,
                snapshot: Some(snapshot),
            });
        }

        let raw = read_auth_file(&self.auth_dir, &snapshot.name)?;
        let mut root: Value = serde_json::from_slice(&raw).map_err(|error| {
            AuthStoreError::FileStore(AuthFileStoreError::InvalidAuthFile(error.to_string()))
        })?;

        let current_state = AuthCandidateState::from_auth_json(
            &root,
            snapshot.priority,
            snapshot.disabled,
            &snapshot.status,
        );
        let mut next_state = current_state.clone();
        apply_execution_result(
            &mut next_state,
            result,
            options.now,
            options.disable_cooling,
        );

        if next_state == current_state {
            return Ok(RecordExecutionResultOutcome {
                decision,
                changed: false,
                snapshot: Some(snapshot),
            });
        }

        let object = root
            .as_object_mut()
            .ok_or(AuthStoreError::FileStore(AuthFileStoreError::InvalidRoot))?;
        sync_execution_state(object, &next_state, result, options.now);

        let pretty = serde_json::to_vec_pretty(&root)
            .map_err(|error| AuthStoreError::FileStore(AuthFileStoreError::Encode(error)))?;
        write_auth_file(&self.auth_dir, &snapshot.name, &pretty)?;

        Ok(RecordExecutionResultOutcome {
            decision,
            changed: true,
            snapshot: self.find_snapshot(&snapshot.name)?,
        })
    }
}

impl AuthStore for FileAuthStore {
    fn list_snapshots(&self) -> Result<Vec<AuthSnapshot>, AuthStoreError> {
        let mut snapshots = Vec::new();
        for entry in list_auth_files(&self.auth_dir)? {
            let mut snapshot = AuthSnapshot::from(entry);
            enrich_runtime_fields(&self.auth_dir, &mut snapshot);
            snapshots.push(snapshot);
        }
        Ok(snapshots)
    }
}

fn enrich_runtime_fields(auth_dir: &Path, snapshot: &mut AuthSnapshot) {
    let Ok(raw) = read_auth_file(auth_dir, &snapshot.name) else {
        return;
    };
    let Ok(json) = serde_json::from_slice::<Value>(&raw) else {
        return;
    };

    snapshot.prefix = read_optional_string(&json, "prefix");
    snapshot.proxy_url = read_optional_string(&json, "proxy_url");
    snapshot.candidate_state = AuthCandidateState::from_auth_json(
        &json,
        snapshot.priority,
        snapshot.disabled,
        &snapshot.status,
    );
}

fn read_optional_string(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sync_execution_state(
    object: &mut Map<String, Value>,
    state: &AuthCandidateState,
    result: &ExecutionResult,
    now: DateTime<Utc>,
) {
    object.insert("status".to_string(), Value::String(state.status.clone()));
    write_optional_string_field(object, "status_message", state.status_message.as_deref());
    write_bool_field(object, "unavailable", state.unavailable);
    write_optional_datetime_field(object, "next_retry_after", state.next_retry_after);
    write_quota_field(object, "quota", &state.quota);
    write_datetime_field(object, "updated_at", now);

    if state.status.eq_ignore_ascii_case("active") && state.status_message.is_none() {
        object.remove("last_error");
    } else if !result.success {
        write_error_field(object, "last_error", result.error.as_ref());
    }

    sync_model_states_field(object, &state.model_states, result, now);
}

fn sync_model_states_field(
    root: &mut Map<String, Value>,
    model_states: &std::collections::BTreeMap<String, AuthCandidateModelState>,
    result: &ExecutionResult,
    now: DateTime<Utc>,
) {
    let existing = root
        .get("model_states")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let affected_model = result
        .model
        .as_deref()
        .map(canonical_model_key)
        .filter(|value| !value.is_empty());

    let mut next_states = Map::new();
    for (model, state) in model_states {
        if !should_persist_model_state(state) {
            continue;
        }

        let mut object = existing
            .get(model)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        object.insert("status".to_string(), Value::String(state.status.clone()));
        write_optional_string_field(
            &mut object,
            "status_message",
            state.status_message.as_deref(),
        );
        write_bool_field(&mut object, "unavailable", state.unavailable);
        write_optional_datetime_field(&mut object, "next_retry_after", state.next_retry_after);
        write_quota_field(&mut object, "quota", &state.quota);

        if affected_model.as_deref() == Some(model.as_str()) {
            write_datetime_field(&mut object, "updated_at", now);
            if result.success {
                object.remove("last_error");
            } else {
                write_error_field(&mut object, "last_error", result.error.as_ref());
            }
        }

        next_states.insert(model.clone(), Value::Object(object));
    }

    if next_states.is_empty() {
        root.remove("model_states");
    } else {
        root.insert("model_states".to_string(), Value::Object(next_states));
    }
}

fn should_persist_model_state(state: &AuthCandidateModelState) -> bool {
    !state.status.eq_ignore_ascii_case("active")
        || state.status_message.is_some()
        || state.unavailable
        || state.next_retry_after.is_some()
        || should_persist_quota(&state.quota)
}

fn write_optional_string_field(object: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        object.remove(key);
        return;
    };
    object.insert(key.to_string(), Value::String(value.to_string()));
}

fn write_bool_field(object: &mut Map<String, Value>, key: &str, value: bool) {
    if value {
        object.insert(key.to_string(), Value::Bool(true));
    } else {
        object.remove(key);
    }
}

fn write_optional_datetime_field(
    object: &mut Map<String, Value>,
    key: &str,
    value: Option<DateTime<Utc>>,
) {
    let Some(value) = value else {
        object.remove(key);
        return;
    };
    object.insert(key.to_string(), Value::String(format_datetime(value)));
}

fn write_datetime_field(object: &mut Map<String, Value>, key: &str, value: DateTime<Utc>) {
    object.insert(key.to_string(), Value::String(format_datetime(value)));
}

fn write_quota_field(object: &mut Map<String, Value>, key: &str, quota: &AuthCandidateQuotaState) {
    if !should_persist_quota(quota) {
        object.remove(key);
        return;
    }

    let mut quota_object = Map::new();
    quota_object.insert("exceeded".to_string(), Value::Bool(quota.exceeded));
    if quota.exceeded {
        quota_object.insert("reason".to_string(), Value::String("quota".to_string()));
    }
    if let Some(next_recover_at) = quota.next_recover_at {
        quota_object.insert(
            "next_recover_at".to_string(),
            Value::String(format_datetime(next_recover_at)),
        );
    }
    if quota.backoff_level > 0 {
        quota_object.insert(
            "backoff_level".to_string(),
            Value::Number(quota.backoff_level.into()),
        );
    }
    object.insert(key.to_string(), Value::Object(quota_object));
}

fn should_persist_quota(quota: &AuthCandidateQuotaState) -> bool {
    quota.exceeded || quota.next_recover_at.is_some() || quota.backoff_level > 0
}

fn write_error_field(object: &mut Map<String, Value>, key: &str, error: Option<&ExecutionError>) {
    let Some(error) = error else {
        object.remove(key);
        return;
    };

    let mut error_object = Map::new();
    let message = error.message.trim();
    if !message.is_empty() {
        error_object.insert("message".to_string(), Value::String(message.to_string()));
    }
    if let Some(http_status) = error.http_status {
        error_object.insert(
            "http_status".to_string(),
            Value::Number(u64::from(http_status).into()),
        );
    }

    if error_object.is_empty() {
        object.remove(key);
    } else {
        object.insert(key.to_string(), Value::Object(error_object));
    }
}

fn format_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn canonical_model_key(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let model_name = if trimmed.ends_with(')') {
        trimmed
            .rsplit_once('(')
            .map(|(head, _)| head)
            .unwrap_or(trimmed)
    } else {
        trimmed
    };

    model_name.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        AuthStore, FileAuthStore, RecordExecutionResultOptions, RecordExecutionResultOutcome,
    };
    use crate::{ExecutionError, ExecutionResult, PersistDecision};
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::Value;
    use std::fs;

    #[test]
    fn file_auth_store_lists_runtime_snapshots_with_hidden_runtime_fields() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let auth_path = temp_dir
            .path()
            .join("codex-demo.user@example.com-team.json");
        fs::write(
            &auth_path,
            r#"{
  "type": "codex",
  "note": "demo account",
  "priority": "5",
  "prefix": " /team/ ",
  "proxy_url": " http://127.0.0.1:7890 ",
  "status": "active",
  "models": [
    { "name": " gpt-5 " }
  ],
  "excluded_models": [" gpt-5-mini "]
}"#,
        )
        .expect("write auth");

        let store = FileAuthStore::new(temp_dir.path());
        let snapshots = store.list_snapshots().expect("snapshots");

        assert_eq!(snapshots.len(), 1);
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.provider, "codex");
        assert_eq!(snapshot.email.as_deref(), Some("demo.user@example.com"));
        assert_eq!(snapshot.account_plan.as_deref(), Some("team"));
        assert_eq!(snapshot.note.as_deref(), Some("demo account"));
        assert_eq!(snapshot.priority, Some(5));
        assert_eq!(snapshot.prefix.as_deref(), Some("/team/"));
        assert_eq!(snapshot.proxy_url.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(snapshot.candidate_state.priority, 5);
        assert!(snapshot.candidate_state.supports_model("gpt-5(high)"));
        assert!(!snapshot.candidate_state.supports_model("gpt-5-mini"));

        let json = serde_json::to_value(snapshot).expect("serialize");
        assert!(json.get("prefix").is_none());
        assert!(json.get("proxy_url").is_none());
        assert!(json.get("candidate_state").is_none());
    }

    #[test]
    fn file_auth_store_keeps_listing_when_runtime_fields_cannot_be_read() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(temp_dir.path().join("broken.json"), b"{not-json").expect("write broken");

        let store = FileAuthStore::new(temp_dir.path());
        let snapshots = store.list_snapshots().expect("snapshots");

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].provider, "unknown");
        assert_eq!(snapshots[0].prefix, None);
        assert_eq!(snapshots[0].proxy_url, None);
    }

    #[test]
    fn file_auth_store_can_find_snapshot_by_name_or_id() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp_dir.path().join("codex-demo@example.com-team.json"),
            r#"{"type":"codex"}"#,
        )
        .expect("write auth");

        let store = FileAuthStore::new(temp_dir.path());
        let by_name = store
            .find_snapshot("codex-demo@example.com-team.json")
            .expect("find by name");
        let by_id = store
            .find_snapshot("CODEX-DEMO@EXAMPLE.COM-TEAM.JSON")
            .expect("find by id");

        assert_eq!(
            by_name.as_ref().map(|snapshot| snapshot.provider.as_str()),
            Some("codex")
        );
        assert_eq!(
            by_id.as_ref().map(|snapshot| snapshot.provider.as_str()),
            Some("codex")
        );
        assert!(store
            .find_snapshot("missing.json")
            .expect("missing")
            .is_none());
    }

    #[test]
    fn file_auth_store_records_execution_result_into_auth_file() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let auth_path = temp_dir.path().join("codex-demo@example.com-team.json");
        fs::write(
            &auth_path,
            r#"{
  "type": "codex",
  "email": "demo@example.com",
  "status": "active",
  "models": [
    { "name": "gpt-5" }
  ]
}"#,
        )
        .expect("write auth");

        let store = FileAuthStore::new(temp_dir.path());
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let outcome = store
            .record_execution_result(
                "codex-demo@example.com-team.json",
                &ExecutionResult {
                    model: Some("gpt-5".to_string()),
                    success: false,
                    retry_after: None,
                    error: Some(ExecutionError {
                        message: "quota".to_string(),
                        http_status: Some(429),
                    }),
                },
                RecordExecutionResultOptions::new(now),
            )
            .expect("record result");

        assert_eq!(
            outcome,
            RecordExecutionResultOutcome {
                decision: PersistDecision::Persist,
                changed: true,
                snapshot: outcome.snapshot.clone(),
            }
        );
        let snapshot = outcome.snapshot.expect("updated snapshot");
        assert_eq!(snapshot.status, "error");
        assert_eq!(snapshot.status_message.as_deref(), Some("quota"));
        assert!(snapshot.candidate_state.unavailable);
        assert!(snapshot.candidate_state.quota.exceeded);
        assert_eq!(
            snapshot.candidate_state.next_retry_after,
            Some(now + Duration::seconds(1))
        );
        let model_state = snapshot
            .candidate_state
            .model_states
            .get("gpt-5")
            .expect("model state");
        assert_eq!(model_state.status, "error");
        assert!(model_state.unavailable);
        assert!(model_state.quota.exceeded);

        let raw = fs::read_to_string(auth_path).expect("auth file");
        let json: Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(json["status"].as_str(), Some("error"));
        assert_eq!(json["status_message"].as_str(), Some("quota"));
        assert_eq!(json["unavailable"].as_bool(), Some(true));
        assert_eq!(
            json["next_retry_after"].as_str(),
            Some("2026-04-05T12:00:01Z")
        );
        assert_eq!(json["quota"]["reason"].as_str(), Some("quota"));
        assert_eq!(
            json["model_states"]["gpt-5"]["status"].as_str(),
            Some("error")
        );
        assert_eq!(
            json["model_states"]["gpt-5"]["next_retry_after"].as_str(),
            Some("2026-04-05T12:00:01Z")
        );
        assert_eq!(
            json["model_states"]["gpt-5"]["last_error"]["http_status"].as_u64(),
            Some(429)
        );
    }

    #[test]
    fn file_auth_store_skips_execution_result_persist_when_runtime_only() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let auth_path = temp_dir.path().join("codex-demo@example.com-team.json");
        let original = r#"{"type":"codex","status":"active"}"#;
        fs::write(&auth_path, original).expect("write auth");

        let store = FileAuthStore::new(temp_dir.path());
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let outcome = store
            .record_execution_result(
                "codex-demo@example.com-team.json",
                &ExecutionResult {
                    model: Some("gpt-5".to_string()),
                    success: false,
                    retry_after: None,
                    error: Some(ExecutionError {
                        message: "quota".to_string(),
                        http_status: Some(429),
                    }),
                },
                RecordExecutionResultOptions {
                    runtime_only: true,
                    ..RecordExecutionResultOptions::new(now)
                },
            )
            .expect("record result");

        assert_eq!(outcome.decision, PersistDecision::SkipRuntimeOnly);
        assert!(!outcome.changed);
        assert_eq!(fs::read_to_string(auth_path).expect("auth file"), original);
    }

    #[test]
    fn file_auth_store_clears_runtime_fields_after_success() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let auth_path = temp_dir.path().join("codex-demo@example.com-team.json");
        fs::write(
            &auth_path,
            r#"{
  "type": "codex",
  "status": "error",
  "status_message": "quota",
  "unavailable": true,
  "next_retry_after": "2026-04-05T12:00:01Z",
  "quota": {
    "exceeded": true,
    "reason": "quota",
    "next_recover_at": "2026-04-05T12:00:01Z",
    "backoff_level": 1
  },
  "last_error": {
    "message": "quota",
    "http_status": 429
  },
  "model_states": {
    "gpt-5": {
      "status": "error",
      "status_message": "quota",
      "unavailable": true,
      "next_retry_after": "2026-04-05T12:00:01Z",
      "last_error": {
        "message": "quota",
        "http_status": 429
      },
      "quota": {
        "exceeded": true,
        "reason": "quota",
        "next_recover_at": "2026-04-05T12:00:01Z",
        "backoff_level": 1
      }
    }
  }
}"#,
        )
        .expect("write auth");

        let store = FileAuthStore::new(temp_dir.path());
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 5, 0).unwrap();
        let outcome = store
            .record_execution_result(
                "codex-demo@example.com-team.json",
                &ExecutionResult {
                    model: Some("gpt-5".to_string()),
                    success: true,
                    retry_after: None,
                    error: None,
                },
                RecordExecutionResultOptions::new(now),
            )
            .expect("record result");

        assert_eq!(outcome.decision, PersistDecision::Persist);
        assert!(outcome.changed);
        let snapshot = outcome.snapshot.expect("updated snapshot");
        assert_eq!(snapshot.status, "active");
        assert_eq!(snapshot.status_message, None);
        assert!(!snapshot.candidate_state.unavailable);
        assert!(!snapshot.candidate_state.quota.exceeded);
        assert!(!snapshot.candidate_state.model_states.contains_key("gpt-5"));

        let raw = fs::read_to_string(auth_path).expect("auth file");
        let json: Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(json["status"].as_str(), Some("active"));
        assert!(json.get("status_message").is_none());
        assert!(json.get("unavailable").is_none());
        assert!(json.get("next_retry_after").is_none());
        assert!(json.get("quota").is_none());
        assert!(json.get("last_error").is_none());
        assert!(json.get("model_states").is_none());
    }
}
