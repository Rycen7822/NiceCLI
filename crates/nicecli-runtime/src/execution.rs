use crate::{AuthCandidateModelState, AuthCandidateQuotaState, AuthCandidateState};
use chrono::{DateTime, Duration, Utc};

const QUOTA_BACKOFF_BASE_SECS: i64 = 1;
const QUOTA_BACKOFF_MAX_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionError {
    pub message: String,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionResult {
    pub model: Option<String>,
    pub success: bool,
    pub retry_after: Option<Duration>,
    pub error: Option<ExecutionError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistDecision {
    Persist,
    SkipRequested,
    SkipRuntimeOnly,
    SkipMissingMetadata,
}

pub fn decide_persist(
    skip_persist: bool,
    runtime_only: bool,
    has_metadata: bool,
) -> PersistDecision {
    if skip_persist {
        PersistDecision::SkipRequested
    } else if runtime_only {
        PersistDecision::SkipRuntimeOnly
    } else if !has_metadata {
        PersistDecision::SkipMissingMetadata
    } else {
        PersistDecision::Persist
    }
}

pub fn apply_execution_result(
    state: &mut AuthCandidateState,
    result: &ExecutionResult,
    now: DateTime<Utc>,
    disable_cooling: bool,
) {
    let model = result.model.as_deref().map(canonical_model_key);
    let model = model.as_deref().filter(|value| !value.is_empty());

    if result.success {
        if let Some(model) = model {
            let model_state = ensure_model_state(state, model);
            reset_model_state(model_state);
            update_aggregated_availability(state, now);
            if !has_model_error(state, now) {
                state.status = "active".to_string();
                state.status_message = None;
            }
        } else {
            clear_auth_state_on_success(state);
        }
        return;
    }

    if let Some(model) = model {
        if is_request_scoped_not_found_result_error(result.error.as_ref()) {
            return;
        }

        let top_level_status_message = result
            .error
            .as_ref()
            .and_then(canonical_result_error_message);

        let model_state = ensure_model_state(state, model);
        model_state.unavailable = true;
        model_state.status = "error".to_string();
        model_state.status_message = result
            .error
            .as_ref()
            .and_then(canonical_result_error_message);

        if is_model_support_result_error(result.error.as_ref()) {
            model_state.next_retry_after = Some(now + Duration::hours(12));
        } else {
            match result
                .error
                .as_ref()
                .and_then(|error| error.http_status)
                .unwrap_or_default()
            {
                401 => {
                    model_state.next_retry_after = Some(now + Duration::minutes(30));
                }
                402 | 403 => {
                    model_state.next_retry_after = Some(now + Duration::minutes(30));
                }
                404 => {
                    model_state.next_retry_after = Some(now + Duration::hours(12));
                }
                429 => {
                    let mut next_retry_after = None;
                    let mut next_backoff_level = model_state.quota.backoff_level;
                    if let Some(retry_after) = result.retry_after {
                        next_retry_after = Some(now + retry_after);
                    } else {
                        let (cooldown, backoff_level) =
                            next_quota_cooldown(model_state.quota.backoff_level, disable_cooling);
                        next_backoff_level = backoff_level;
                        if cooldown > Duration::zero() {
                            next_retry_after = Some(now + cooldown);
                        }
                    }
                    model_state.next_retry_after = next_retry_after;
                    model_state.quota.exceeded = true;
                    model_state.quota.next_recover_at = next_retry_after;
                    model_state.quota.backoff_level = next_backoff_level;
                }
                408 | 500 | 502 | 503 | 504 => {
                    model_state.next_retry_after = if disable_cooling {
                        None
                    } else {
                        Some(now + Duration::minutes(1))
                    };
                }
                _ => {
                    model_state.next_retry_after = None;
                }
            }
        }

        state.status = "error".to_string();
        state.status_message = top_level_status_message;
        update_aggregated_availability(state, now);
        return;
    }

    apply_auth_failure_state(
        state,
        result.error.as_ref(),
        result.retry_after,
        now,
        disable_cooling,
    );
}

fn ensure_model_state<'a>(
    state: &'a mut AuthCandidateState,
    model: &str,
) -> &'a mut AuthCandidateModelState {
    state
        .model_states
        .entry(model.to_string())
        .or_insert_with(|| AuthCandidateModelState {
            status: "active".to_string(),
            ..AuthCandidateModelState::default()
        })
}

fn reset_model_state(state: &mut AuthCandidateModelState) {
    state.unavailable = false;
    state.status = "active".to_string();
    state.status_message = None;
    state.next_retry_after = None;
    state.quota = AuthCandidateQuotaState::default();
}

fn update_aggregated_availability(state: &mut AuthCandidateState, now: DateTime<Utc>) {
    if state.model_states.is_empty() {
        return;
    }

    let mut all_unavailable = true;
    let mut earliest_retry: Option<DateTime<Utc>> = None;
    let mut quota_exceeded = false;
    let mut quota_recover: Option<DateTime<Utc>> = None;
    let mut max_backoff_level = 0;

    for model_state in state.model_states.values_mut() {
        let mut state_unavailable = false;
        if model_state.status.eq_ignore_ascii_case("disabled") {
            state_unavailable = true;
        } else if model_state.unavailable {
            if let Some(next_retry_after) = model_state.next_retry_after {
                if next_retry_after > now {
                    state_unavailable = true;
                    earliest_retry = Some(match earliest_retry {
                        Some(current) => current.min(next_retry_after),
                        None => next_retry_after,
                    });
                } else {
                    model_state.unavailable = false;
                    model_state.next_retry_after = None;
                }
            }
        }

        if !state_unavailable {
            all_unavailable = false;
        }

        if model_state.quota.exceeded {
            quota_exceeded = true;
            if let Some(next_recover_at) = model_state.quota.next_recover_at {
                quota_recover = Some(match quota_recover {
                    Some(current) => current.min(next_recover_at),
                    None => next_recover_at,
                });
            }
            max_backoff_level = max_backoff_level.max(model_state.quota.backoff_level);
        }
    }

    state.unavailable = all_unavailable;
    state.next_retry_after = if all_unavailable {
        earliest_retry
    } else {
        None
    };

    if quota_exceeded {
        state.quota.exceeded = true;
        state.quota.next_recover_at = quota_recover;
        state.quota.backoff_level = max_backoff_level;
    } else {
        state.quota = AuthCandidateQuotaState::default();
    }
}

fn has_model_error(state: &AuthCandidateState, now: DateTime<Utc>) -> bool {
    state.model_states.values().any(|model_state| {
        model_state.status.eq_ignore_ascii_case("error")
            && (model_state.status_message.is_some()
                || (model_state.unavailable
                    && model_state
                        .next_retry_after
                        .map(|value| value > now)
                        .unwrap_or(true)))
    })
}

fn clear_auth_state_on_success(state: &mut AuthCandidateState) {
    state.unavailable = false;
    state.status = "active".to_string();
    state.status_message = None;
    state.quota = AuthCandidateQuotaState::default();
    state.next_retry_after = None;
}

fn apply_auth_failure_state(
    state: &mut AuthCandidateState,
    error: Option<&ExecutionError>,
    retry_after: Option<Duration>,
    now: DateTime<Utc>,
    disable_cooling: bool,
) {
    if is_request_scoped_not_found_result_error(error) {
        return;
    }

    state.unavailable = true;
    state.status = "error".to_string();
    state.status_message = error.and_then(canonical_result_error_message);

    match error
        .and_then(|error| error.http_status)
        .unwrap_or_default()
    {
        401 => {
            state.status_message = Some("unauthorized".to_string());
            state.next_retry_after = Some(now + Duration::minutes(30));
        }
        402 | 403 => {
            if state.status_message.as_deref() != Some("deactivated") {
                state.status_message = Some("payment_required".to_string());
            }
            state.next_retry_after = Some(now + Duration::minutes(30));
        }
        404 => {
            state.status_message = Some("not_found".to_string());
            state.next_retry_after = Some(now + Duration::hours(12));
        }
        429 => {
            state.status_message = Some("quota exhausted".to_string());
            state.quota.exceeded = true;
            let (cooldown, backoff_level) =
                next_quota_cooldown(state.quota.backoff_level, disable_cooling);
            let next_retry_after = retry_after
                .map(|retry_after| now + retry_after)
                .or_else(|| (cooldown > Duration::zero()).then_some(now + cooldown));
            state.quota.next_recover_at = next_retry_after;
            state.quota.backoff_level = backoff_level;
            state.next_retry_after = next_retry_after;
        }
        408 | 500 | 502 | 503 | 504 => {
            state.status_message = Some("transient upstream error".to_string());
            state.next_retry_after = if disable_cooling {
                None
            } else {
                Some(now + Duration::minutes(1))
            };
        }
        _ => {
            if state.status_message.is_none() {
                state.status_message = Some("request failed".to_string());
            }
        }
    }
}

fn next_quota_cooldown(prev_level: i32, disable_cooling: bool) -> (Duration, i32) {
    let prev_level = prev_level.max(0);
    if disable_cooling {
        return (Duration::zero(), prev_level);
    }

    let secs = (1_i64 << prev_level.min(30)) * QUOTA_BACKOFF_BASE_SECS;
    let cooldown = Duration::seconds(secs.min(QUOTA_BACKOFF_MAX_SECS));
    if cooldown >= Duration::seconds(QUOTA_BACKOFF_MAX_SECS) {
        (Duration::seconds(QUOTA_BACKOFF_MAX_SECS), prev_level)
    } else {
        (cooldown, prev_level + 1)
    }
}

fn is_model_support_result_error(error: Option<&ExecutionError>) -> bool {
    let Some(error) = error else {
        return false;
    };
    match error.http_status {
        Some(400) | Some(422) => is_model_support_error_message(&error.message),
        _ => false,
    }
}

fn is_model_support_error_message(message: &str) -> bool {
    let lower = message.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    [
        "model_not_supported",
        "requested model is not supported",
        "requested model is unsupported",
        "requested model is unavailable",
        "model is not supported",
        "model not supported",
        "unsupported model",
        "model unavailable",
        "not available for your plan",
        "not available for your account",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

fn is_request_scoped_not_found_result_error(error: Option<&ExecutionError>) -> bool {
    let Some(error) = error else {
        return false;
    };
    error.http_status == Some(404) && is_request_scoped_not_found_message(&error.message)
}

fn is_request_scoped_not_found_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("item with id")
        && lower.contains("not found")
        && lower.contains("items are not persisted when `store` is set to false")
}

fn canonical_result_error_message(error: &ExecutionError) -> Option<String> {
    let message = error.message.trim();
    if message.is_empty() {
        return None;
    }
    if is_workspace_deactivated_message(message) {
        return Some("deactivated".to_string());
    }
    Some(message.to_string())
}

fn is_workspace_deactivated_message(message: &str) -> bool {
    let lower = message.trim().to_ascii_lowercase();
    !lower.is_empty()
        && lower.contains("workspace")
        && (lower.contains("deactivat")
            || lower.contains("inactive")
            || lower.contains("disabled"))
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
        apply_execution_result, decide_persist, ExecutionError, ExecutionResult, PersistDecision,
    };
    use crate::AuthCandidateState;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn model_quota_failure_updates_model_and_auth_cooldown() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut state = AuthCandidateState {
            status: "active".to_string(),
            ..AuthCandidateState::default()
        };

        apply_execution_result(
            &mut state,
            &ExecutionResult {
                model: Some("gpt-5".to_string()),
                success: false,
                retry_after: None,
                error: Some(ExecutionError {
                    message: "quota".to_string(),
                    http_status: Some(429),
                }),
            },
            now,
            false,
        );

        assert_eq!(state.status, "error");
        assert!(state.unavailable);
        assert_eq!(state.next_retry_after, Some(now + Duration::seconds(1)));
        assert!(state.quota.exceeded);
        assert_eq!(state.quota.backoff_level, 1);
        let model_state = state.model_states.get("gpt-5").expect("model state");
        assert!(model_state.unavailable);
        assert_eq!(model_state.status, "error");
        assert_eq!(
            model_state.next_retry_after,
            Some(now + Duration::seconds(1))
        );
        assert!(model_state.quota.exceeded);
    }

    #[test]
    fn model_success_clears_auth_state_when_last_model_error_is_resolved() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut state = AuthCandidateState {
            status: "active".to_string(),
            ..AuthCandidateState::default()
        };

        apply_execution_result(
            &mut state,
            &ExecutionResult {
                model: Some("gpt-5".to_string()),
                success: false,
                retry_after: None,
                error: Some(ExecutionError {
                    message: "quota".to_string(),
                    http_status: Some(429),
                }),
            },
            now,
            false,
        );
        apply_execution_result(
            &mut state,
            &ExecutionResult {
                model: Some("gpt-5".to_string()),
                success: true,
                retry_after: None,
                error: None,
            },
            now + Duration::minutes(5),
            false,
        );

        assert_eq!(state.status, "active");
        assert_eq!(state.status_message, None);
        assert!(!state.unavailable);
        assert_eq!(state.next_retry_after, None);
        assert!(!state.quota.exceeded);
        let model_state = state.model_states.get("gpt-5").expect("model state");
        assert_eq!(model_state.status, "active");
        assert_eq!(model_state.status_message, None);
        assert!(!model_state.unavailable);
    }

    #[test]
    fn auth_failure_uses_top_level_retry_policy() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut state = AuthCandidateState {
            status: "active".to_string(),
            ..AuthCandidateState::default()
        };

        apply_execution_result(
            &mut state,
            &ExecutionResult {
                model: None,
                success: false,
                retry_after: Some(Duration::seconds(45)),
                error: Some(ExecutionError {
                    message: "quota".to_string(),
                    http_status: Some(429),
                }),
            },
            now,
            false,
        );

        assert_eq!(state.status, "error");
        assert_eq!(state.status_message.as_deref(), Some("quota exhausted"));
        assert_eq!(state.next_retry_after, Some(now + Duration::seconds(45)));
        assert!(state.quota.exceeded);
    }

    #[test]
    fn auth_failure_marks_deactivated_workspace_explicitly() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut state = AuthCandidateState {
            status: "active".to_string(),
            ..AuthCandidateState::default()
        };

        apply_execution_result(
            &mut state,
            &ExecutionResult {
                model: None,
                success: false,
                retry_after: None,
                error: Some(ExecutionError {
                    message: r#"{"message":"workspace is deactivated"}"#.to_string(),
                    http_status: Some(403),
                }),
            },
            now,
            false,
        );

        assert_eq!(state.status, "error");
        assert_eq!(state.status_message.as_deref(), Some("deactivated"));
        assert_eq!(state.next_retry_after, Some(now + Duration::minutes(30)));
        assert!(!state.quota.exceeded);
    }

    #[test]
    fn auth_failure_keeps_generic_payment_required_for_other_403s() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut state = AuthCandidateState {
            status: "active".to_string(),
            ..AuthCandidateState::default()
        };

        apply_execution_result(
            &mut state,
            &ExecutionResult {
                model: None,
                success: false,
                retry_after: None,
                error: Some(ExecutionError {
                    message: r#"{"message":"payment required"}"#.to_string(),
                    http_status: Some(403),
                }),
            },
            now,
            false,
        );

        assert_eq!(state.status, "error");
        assert_eq!(state.status_message.as_deref(), Some("payment_required"));
        assert_eq!(state.next_retry_after, Some(now + Duration::minutes(30)));
        assert!(!state.quota.exceeded);
    }

    #[test]
    fn persist_decision_matches_go_skip_rules() {
        assert_eq!(
            decide_persist(true, false, true),
            PersistDecision::SkipRequested
        );
        assert_eq!(
            decide_persist(false, true, true),
            PersistDecision::SkipRuntimeOnly
        );
        assert_eq!(
            decide_persist(false, false, false),
            PersistDecision::SkipMissingMetadata
        );
        assert_eq!(decide_persist(false, false, true), PersistDecision::Persist);
    }
}
