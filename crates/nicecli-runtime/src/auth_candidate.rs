use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCandidateAvailability {
    Ready,
    Cooldown { until: DateTime<Utc> },
    Blocked { until: DateTime<Utc> },
    Disabled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthCandidateQuotaState {
    pub exceeded: bool,
    pub next_recover_at: Option<DateTime<Utc>>,
    pub backoff_level: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthCandidateModelState {
    pub status: String,
    pub status_message: Option<String>,
    pub unavailable: bool,
    pub next_retry_after: Option<DateTime<Utc>>,
    pub quota: AuthCandidateQuotaState,
}

impl AuthCandidateModelState {
    pub fn is_disabled(&self) -> bool {
        status_is_disabled(&self.status)
    }

    pub fn availability_at(&self, now: DateTime<Utc>) -> AuthCandidateAvailability {
        if self.is_disabled() {
            return AuthCandidateAvailability::Disabled;
        }
        availability_from_runtime_state(self.unavailable, self.next_retry_after, &self.quota, now)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthCandidateState {
    pub priority: i64,
    pub disabled: bool,
    pub status: String,
    pub status_message: Option<String>,
    pub unavailable: bool,
    pub next_retry_after: Option<DateTime<Utc>>,
    pub quota: AuthCandidateQuotaState,
    pub model_states: BTreeMap<String, AuthCandidateModelState>,
    pub supported_models: BTreeSet<String>,
    pub has_explicit_supported_models: bool,
    pub excluded_models: BTreeSet<String>,
    pub websocket_enabled: bool,
    pub virtual_parent: Option<String>,
}

impl AuthCandidateState {
    pub fn from_auth_json(
        root: &Value,
        fallback_priority: Option<i64>,
        fallback_disabled: bool,
        fallback_status: &str,
    ) -> Self {
        let mut state = Self {
            priority: fallback_priority.unwrap_or_default(),
            disabled: fallback_disabled,
            status: normalize_status(fallback_status, fallback_disabled),
            ..Self::default()
        };

        let Some(object) = root.as_object() else {
            return state;
        };

        state.priority = read_i64_path(object, &[&["priority"]]).unwrap_or(state.priority);

        if let Some(disabled) = read_bool_path(object, &[&["disabled"]]) {
            state.disabled = disabled;
        }

        if let Some(status) = read_string_path(object, &[&["status"]]) {
            state.status = normalize_status(&status, state.disabled);
        } else {
            state.status = normalize_status(&state.status, state.disabled);
        }
        state.status_message = read_string_path(object, &[&["status_message"]]);

        state.unavailable = read_bool_path(object, &[&["unavailable"]]).unwrap_or(false);
        state.next_retry_after = read_datetime_path(object, &[&["next_retry_after"]]);
        state.quota = parse_quota_state(object.get("quota"));
        state.model_states = parse_model_states(object.get("model_states"));

        let (has_explicit_supported_models, supported_models) = collect_supported_models(object);
        state.has_explicit_supported_models = has_explicit_supported_models;
        state.supported_models = supported_models;
        state.excluded_models = collect_excluded_models(object);
        state.websocket_enabled = read_bool_path(
            object,
            &[
                &["websockets"],
                &["attributes", "websockets"],
                &["metadata", "websockets"],
            ],
        )
        .unwrap_or(false);
        state.virtual_parent = read_string_path(
            object,
            &[
                &["gemini_virtual_parent"],
                &["attributes", "gemini_virtual_parent"],
                &["metadata", "gemini_virtual_parent"],
            ],
        );

        state
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled || status_is_disabled(&self.status)
    }

    pub fn supports_model(&self, model: &str) -> bool {
        let model_key = canonical_model_key(model);
        if model_key.is_empty() {
            return true;
        }
        if self.excluded_models.contains(&model_key) {
            return false;
        }
        if self.has_explicit_supported_models {
            return self.supported_models.contains(&model_key);
        }
        true
    }

    pub fn explicit_model_ids(&self) -> Vec<String> {
        if !self.has_explicit_supported_models {
            return Vec::new();
        }

        self.supported_models
            .iter()
            .filter(|model| !self.excluded_models.contains(*model))
            .cloned()
            .collect()
    }

    pub fn availability_for_model_at(
        &self,
        model: &str,
        now: DateTime<Utc>,
    ) -> AuthCandidateAvailability {
        if self.is_disabled() {
            return AuthCandidateAvailability::Disabled;
        }

        if let Some(model_state) = self.model_state_for(model) {
            return model_state.availability_at(now);
        }

        availability_from_runtime_state(self.unavailable, self.next_retry_after, &self.quota, now)
    }

    fn model_state_for(&self, model: &str) -> Option<&AuthCandidateModelState> {
        let model_key = canonical_model_key(model);
        if model_key.is_empty() {
            return None;
        }
        self.model_states.get(&model_key)
    }
}

fn availability_from_runtime_state(
    unavailable: bool,
    next_retry_after: Option<DateTime<Utc>>,
    quota: &AuthCandidateQuotaState,
    now: DateTime<Utc>,
) -> AuthCandidateAvailability {
    if !unavailable {
        return AuthCandidateAvailability::Ready;
    }

    let Some(next_retry_after) = next_retry_after.filter(|value| *value > now) else {
        return AuthCandidateAvailability::Ready;
    };

    let mut effective_until = next_retry_after;
    if let Some(next_recover_at) = quota.next_recover_at.filter(|value| *value > now) {
        effective_until = next_recover_at;
    }

    if quota.exceeded {
        AuthCandidateAvailability::Cooldown {
            until: effective_until,
        }
    } else {
        AuthCandidateAvailability::Blocked {
            until: effective_until,
        }
    }
}

fn parse_quota_state(value: Option<&Value>) -> AuthCandidateQuotaState {
    let Some(object) = value.and_then(Value::as_object) else {
        return AuthCandidateQuotaState::default();
    };

    AuthCandidateQuotaState {
        exceeded: read_bool_path(object, &[&["exceeded"]]).unwrap_or(false),
        next_recover_at: read_datetime_path(object, &[&["next_recover_at"], &["nextRecoverAt"]]),
        backoff_level: read_i64_path(object, &[&["backoff_level"], &["backoffLevel"]])
            .unwrap_or_default() as i32,
    }
}

fn parse_model_states(value: Option<&Value>) -> BTreeMap<String, AuthCandidateModelState> {
    let Some(object) = value.and_then(Value::as_object) else {
        return BTreeMap::new();
    };

    let mut states = BTreeMap::new();
    for (model, value) in object {
        let model_key = canonical_model_key(model);
        let Some(model_object) = value.as_object() else {
            continue;
        };
        if model_key.is_empty() {
            continue;
        }

        let status = read_string_path(model_object, &[&["status"]]).unwrap_or_default();
        states.insert(
            model_key,
            AuthCandidateModelState {
                status: normalize_status(&status, false),
                status_message: read_string_path(model_object, &[&["status_message"]]),
                unavailable: read_bool_path(model_object, &[&["unavailable"]]).unwrap_or(false),
                next_retry_after: read_datetime_path(
                    model_object,
                    &[&["next_retry_after"], &["nextRetryAfter"]],
                ),
                quota: parse_quota_state(model_object.get("quota")),
            },
        );
    }

    states
}

fn collect_supported_models(root: &Map<String, Value>) -> (bool, BTreeSet<String>) {
    let mut seen_explicit = false;
    let mut supported_models = BTreeSet::new();

    for value in [
        value_at(root, &["supported_models"]),
        value_at(root, &["supported-models"]),
        value_at(root, &["models"]),
        value_at(root, &["metadata", "supported_models"]),
        value_at(root, &["metadata", "supported-models"]),
        value_at(root, &["metadata", "models"]),
        value_at(root, &["attributes", "supported_models"]),
        value_at(root, &["attributes", "supported-models"]),
        value_at(root, &["attributes", "models"]),
    ] {
        let Some(value) = value else {
            continue;
        };

        seen_explicit = true;
        supported_models.extend(collect_model_keys(value));
    }

    (seen_explicit, supported_models)
}

fn collect_excluded_models(root: &Map<String, Value>) -> BTreeSet<String> {
    let mut excluded_models = BTreeSet::new();

    for value in [
        value_at(root, &["excluded_models"]),
        value_at(root, &["excluded-models"]),
        value_at(root, &["metadata", "excluded_models"]),
        value_at(root, &["metadata", "excluded-models"]),
        value_at(root, &["attributes", "excluded_models"]),
        value_at(root, &["attributes", "excluded-models"]),
    ] {
        let Some(value) = value else {
            continue;
        };

        excluded_models.extend(collect_model_keys(value));
    }

    excluded_models
}

fn collect_model_keys(value: &Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();

    match value {
        Value::Array(items) => {
            for item in items {
                keys.extend(collect_model_keys(item));
            }
        }
        Value::Object(object) => {
            for field in ["id", "name", "alias"] {
                if let Some(field_value) = object.get(field) {
                    keys.extend(collect_model_keys(field_value));
                }
            }
        }
        Value::String(text) => {
            for item in text.split(',') {
                let key = canonical_model_key(item);
                if !key.is_empty() {
                    keys.insert(key);
                }
            }
        }
        _ => {}
    }

    keys
}

fn read_bool_path(root: &Map<String, Value>, paths: &[&[&str]]) -> Option<bool> {
    paths
        .iter()
        .find_map(|path| value_at(root, path).and_then(parse_bool_value))
}

fn read_i64_path(root: &Map<String, Value>, paths: &[&[&str]]) -> Option<i64> {
    paths
        .iter()
        .find_map(|path| value_at(root, path).and_then(parse_i64_value))
}

fn read_string_path(root: &Map<String, Value>, paths: &[&[&str]]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| value_at(root, path).and_then(parse_string_value))
}

fn read_datetime_path(root: &Map<String, Value>, paths: &[&[&str]]) -> Option<DateTime<Utc>> {
    paths
        .iter()
        .find_map(|path| value_at(root, path).and_then(parse_datetime_value))
}

fn value_at<'a>(root: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        current = current.as_object()?.get(*segment)?;
    }
    Some(current)
}

fn parse_bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => text.trim().parse::<bool>().ok(),
        Value::Number(number) => number.as_i64().map(|value| value != 0),
        _ => None,
    }
}

fn parse_i64_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn parse_string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn parse_datetime_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(text) => parse_datetime_string(text),
        Value::Number(number) => number.as_i64().and_then(datetime_from_unix_like_value),
        _ => None,
    }
}

fn parse_datetime_string(value: &str) -> Option<DateTime<Utc>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.with_timezone(&Utc));
    }

    for layout in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(trimmed, layout) {
            return Some(Utc.from_utc_datetime(&parsed));
        }
    }

    trimmed
        .parse::<i64>()
        .ok()
        .and_then(datetime_from_unix_like_value)
}

fn datetime_from_unix_like_value(raw: i64) -> Option<DateTime<Utc>> {
    if raw <= 0 {
        return None;
    }
    if raw > 1_000_000_000_000 {
        Utc.timestamp_millis_opt(raw).single()
    } else {
        Utc.timestamp_opt(raw, 0).single()
    }
}

fn normalize_status(value: &str, disabled: bool) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        if disabled {
            "disabled".to_string()
        } else {
            "active".to_string()
        }
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn status_is_disabled(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("disabled")
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
    use super::{AuthCandidateAvailability, AuthCandidateState};
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    #[test]
    fn parses_candidate_state_and_model_visibility_from_auth_json() {
        let candidate = AuthCandidateState::from_auth_json(
            &json!({
                "priority": "7",
                "status": "active",
                "unavailable": true,
                "next_retry_after": "2026-04-05T12:00:00Z",
                "quota": {
                    "exceeded": true,
                    "next_recover_at": "2026-04-05T12:30:00Z"
                },
                "models": [
                    { "name": " GPT-5 " },
                    { "alias": " team-gpt5 " }
                ],
                "excluded_models": [" gpt-5-mini "],
                "attributes": {
                    "gemini_virtual_parent": " parent-1 "
                },
                "metadata": {
                    "websockets": true
                },
                "model_states": {
                    "gpt-5(high)": {
                        "unavailable": true,
                        "next_retry_after": "2026-04-05T13:00:00Z",
                        "quota": {
                            "exceeded": true,
                            "next_recover_at": "2026-04-05T14:00:00Z"
                        }
                    },
                    "gpt-5-mini": {
                        "status": "disabled"
                    }
                }
            }),
            None,
            false,
            "active",
        );

        assert_eq!(candidate.priority, 7);
        assert!(candidate.has_explicit_supported_models);
        assert!(candidate.supports_model("gpt-5(max)"));
        assert!(candidate.supports_model("team-gpt5"));
        assert!(!candidate.supports_model("gpt-5-mini"));
        assert_eq!(candidate.explicit_model_ids(), vec!["gpt-5", "team-gpt5"]);
        assert!(candidate.websocket_enabled);
        assert_eq!(candidate.virtual_parent.as_deref(), Some("parent-1"));
        assert_eq!(candidate.status_message, None);

        assert_eq!(
            candidate.availability_for_model_at(
                "gpt-5(high)",
                Utc.with_ymd_and_hms(2026, 4, 5, 11, 0, 0).unwrap()
            ),
            AuthCandidateAvailability::Cooldown {
                until: Utc.with_ymd_and_hms(2026, 4, 5, 14, 0, 0).unwrap()
            }
        );
        assert_eq!(
            candidate.availability_for_model_at(
                "gpt-5-mini",
                Utc.with_ymd_and_hms(2026, 4, 5, 11, 0, 0).unwrap()
            ),
            AuthCandidateAvailability::Disabled
        );
    }

    #[test]
    fn keeps_unavailable_auth_selectable_without_future_retry_deadline() {
        let candidate = AuthCandidateState::from_auth_json(
            &json!({
                "unavailable": true,
                "quota": {
                    "exceeded": true,
                    "next_recover_at": "2026-04-05T12:30:00Z"
                }
            }),
            Some(3),
            false,
            "active",
        );

        assert_eq!(candidate.priority, 3);
        assert_eq!(
            candidate
                .availability_for_model_at("", Utc.with_ymd_and_hms(2026, 4, 5, 11, 0, 0).unwrap()),
            AuthCandidateAvailability::Ready
        );
    }
}
