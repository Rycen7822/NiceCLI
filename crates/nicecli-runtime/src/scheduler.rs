use crate::{AuthCandidateAvailability, AuthCandidateState, AuthSnapshot, RoutingStrategy};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCandidate {
    pub auth_id: String,
    pub provider: String,
    pub state: AuthCandidateState,
}

impl From<&AuthSnapshot> for AuthCandidate {
    fn from(snapshot: &AuthSnapshot) -> Self {
        Self {
            auth_id: snapshot.id.clone(),
            provider: normalize_provider(&snapshot.provider),
            state: snapshot.candidate_state.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerPick {
    pub auth_id: String,
    pub provider: String,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchedulerError {
    #[error("no provider supplied")]
    NoProvider,
    #[error("no auth available")]
    AuthNotFound,
    #[error("no auth available")]
    AuthUnavailable,
    #[error("model cooldown for {model} until {until}")]
    ModelCooldown {
        provider: Option<String>,
        model: String,
        until: DateTime<Utc>,
    },
}

#[derive(Debug, Default)]
pub struct AuthScheduler {
    strategy: RoutingStrategy,
    cursors: HashMap<String, usize>,
    mixed_cursors: HashMap<String, usize>,
}

impl AuthScheduler {
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            strategy,
            cursors: HashMap::new(),
            mixed_cursors: HashMap::new(),
        }
    }

    pub fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }

    pub fn set_strategy(&mut self, strategy: RoutingStrategy) {
        self.strategy = strategy;
        self.mixed_cursors.clear();
    }

    pub fn pick_single(
        &mut self,
        provider: &str,
        model: &str,
        candidates: &[AuthCandidate],
        pinned_auth_id: Option<&str>,
        tried_auth_ids: &HashSet<String>,
        prefer_websocket: bool,
        now: DateTime<Utc>,
    ) -> Result<SchedulerPick, SchedulerError> {
        let provider_key = normalize_provider(provider);
        if provider_key.is_empty() {
            return Err(SchedulerError::NoProvider);
        }

        let eligible = eligible_candidates(
            candidates,
            Some(&provider_key),
            pinned_auth_id,
            tried_auth_ids,
            model,
        );
        if eligible.is_empty() {
            return Err(SchedulerError::AuthNotFound);
        }

        let best_priority = highest_ready_priority(
            &eligible,
            model,
            now,
            prefer_websocket && provider_key == "codex",
        );
        let Some(priority) = best_priority else {
            return Err(summarize_unavailable(
                &eligible,
                Some(&provider_key),
                model,
                now,
            ));
        };

        let ready = ready_candidates(
            &eligible,
            model,
            now,
            priority,
            prefer_websocket && provider_key == "codex",
        );
        let Some(picked) = self.pick_from_ready(&provider_key, model, &ready) else {
            return Err(summarize_unavailable(
                &eligible,
                Some(&provider_key),
                model,
                now,
            ));
        };

        Ok(SchedulerPick {
            auth_id: picked.auth_id.clone(),
            provider: picked.provider.clone(),
            priority,
        })
    }

    pub fn pick_mixed(
        &mut self,
        providers: &[&str],
        model: &str,
        candidates: &[AuthCandidate],
        pinned_auth_id: Option<&str>,
        tried_auth_ids: &HashSet<String>,
        now: DateTime<Utc>,
    ) -> Result<SchedulerPick, SchedulerError> {
        let provider_keys = normalize_provider_list(providers);
        if provider_keys.is_empty() {
            return Err(SchedulerError::NoProvider);
        }

        if let Some(pinned_auth_id) = pinned_auth_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let eligible = eligible_candidates(
                candidates,
                None,
                Some(pinned_auth_id),
                tried_auth_ids,
                model,
            );
            let eligible: Vec<_> = eligible
                .into_iter()
                .filter(|candidate| {
                    provider_keys
                        .iter()
                        .any(|provider| provider == &candidate.provider)
                })
                .collect();
            if eligible.is_empty() {
                return Err(SchedulerError::AuthNotFound);
            }
            let candidate = eligible[0];
            return match candidate.state.availability_for_model_at(model, now) {
                AuthCandidateAvailability::Ready => Ok(SchedulerPick {
                    auth_id: candidate.auth_id.clone(),
                    provider: candidate.provider.clone(),
                    priority: candidate.state.priority,
                }),
                _ => Err(summarize_unavailable(&eligible, None, model, now)),
            };
        }

        let eligible = eligible_candidates(candidates, None, None, tried_auth_ids, model);
        let eligible: Vec<_> = eligible
            .into_iter()
            .filter(|candidate| {
                provider_keys
                    .iter()
                    .any(|provider| provider == &candidate.provider)
            })
            .collect();
        if eligible.is_empty() {
            return Err(SchedulerError::AuthNotFound);
        }

        let mut best_priority: Option<i64> = None;
        let mut ready_by_provider: Vec<Vec<&AuthCandidate>> =
            Vec::with_capacity(provider_keys.len());
        for provider in &provider_keys {
            let provider_candidates: Vec<_> = eligible
                .iter()
                .copied()
                .filter(|candidate| candidate.provider == *provider)
                .collect();
            let provider_best = highest_ready_priority(&provider_candidates, model, now, false);
            if let Some(priority) = provider_best {
                best_priority =
                    Some(best_priority.map_or(priority, |current| current.max(priority)));
            }
            ready_by_provider.push(provider_candidates);
        }

        let Some(best_priority) = best_priority else {
            return Err(summarize_unavailable(&eligible, None, model, now));
        };

        let mut provider_ready_counts = Vec::with_capacity(provider_keys.len());
        for provider_candidates in &ready_by_provider {
            provider_ready_counts.push(ready_candidates(
                provider_candidates,
                model,
                now,
                best_priority,
                false,
            ));
        }

        if self.strategy == RoutingStrategy::FillFirst {
            for ready in provider_ready_counts {
                if let Some(picked) = ready.first() {
                    return Ok(SchedulerPick {
                        auth_id: picked.auth_id.clone(),
                        provider: picked.provider.clone(),
                        priority: best_priority,
                    });
                }
            }
            return Err(summarize_unavailable(&eligible, None, model, now));
        }

        let weights: Vec<_> = provider_ready_counts.iter().map(Vec::len).collect();
        let total_weight: usize = weights.iter().sum();
        if total_weight == 0 {
            return Err(summarize_unavailable(&eligible, None, model, now));
        }

        let cursor_key = format!("{}:{}", provider_keys.join(","), canonical_model_key(model));
        let start_slot = self
            .mixed_cursors
            .get(&cursor_key)
            .copied()
            .unwrap_or_default()
            % total_weight;

        let mut segment_start = 0usize;
        let mut chosen_provider_index = None;
        let mut chosen_slot = start_slot;
        for (provider_index, weight) in weights.iter().enumerate() {
            let segment_end = segment_start + weight;
            if *weight > 0 && start_slot < segment_end {
                chosen_provider_index = Some(provider_index);
                break;
            }
            segment_start = segment_end;
        }

        let Some(start_provider_index) = chosen_provider_index else {
            return Err(summarize_unavailable(&eligible, None, model, now));
        };

        for offset in 0..provider_keys.len() {
            let provider_index = (start_provider_index + offset) % provider_keys.len();
            if weights[provider_index] == 0 {
                continue;
            }
            if provider_index != start_provider_index {
                chosen_slot = weights[..provider_index].iter().sum();
            }

            let ready = &provider_ready_counts[provider_index];
            let provider_key = &provider_keys[provider_index];
            if let Some(picked) = self.pick_from_ready(provider_key, model, ready) {
                self.mixed_cursors
                    .insert(cursor_key.clone(), chosen_slot + 1);
                return Ok(SchedulerPick {
                    auth_id: picked.auth_id.clone(),
                    provider: picked.provider.clone(),
                    priority: best_priority,
                });
            }
        }

        Err(summarize_unavailable(&eligible, None, model, now))
    }

    fn pick_from_ready<'a>(
        &mut self,
        provider: &str,
        model: &str,
        ready: &[&'a AuthCandidate],
    ) -> Option<&'a AuthCandidate> {
        if ready.is_empty() {
            return None;
        }

        if self.strategy == RoutingStrategy::FillFirst {
            return ready.first().copied();
        }

        let cursor_key = format!("{provider}:{}", canonical_model_key(model));
        let cursor = self.cursors.entry(cursor_key).or_default();
        let index = *cursor % ready.len();
        *cursor = index + 1;
        ready.get(index).copied()
    }
}

fn eligible_candidates<'a>(
    candidates: &'a [AuthCandidate],
    provider: Option<&str>,
    pinned_auth_id: Option<&str>,
    tried_auth_ids: &HashSet<String>,
    model: &str,
) -> Vec<&'a AuthCandidate> {
    let provider_key = provider.map(normalize_provider);
    let pinned_auth_id = pinned_auth_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut eligible: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            if let Some(provider_key) = provider_key.as_deref() {
                if candidate.provider != provider_key {
                    return false;
                }
            }
            if let Some(pinned_auth_id) = pinned_auth_id.as_deref() {
                if candidate.auth_id != pinned_auth_id {
                    return false;
                }
            }
            if tried_auth_ids.contains(&candidate.auth_id) {
                return false;
            }
            candidate.state.supports_model(model)
        })
        .collect();

    eligible.sort_by(|left, right| left.auth_id.cmp(&right.auth_id));
    eligible
}

fn ready_candidates<'a>(
    candidates: &[&'a AuthCandidate],
    model: &str,
    now: DateTime<Utc>,
    priority: i64,
    prefer_websocket: bool,
) -> Vec<&'a AuthCandidate> {
    let mut ready: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.state.priority == priority)
        .filter(|candidate| {
            matches!(
                candidate.state.availability_for_model_at(model, now),
                AuthCandidateAvailability::Ready
            )
        })
        .collect();

    if prefer_websocket {
        let websocket_ready: Vec<_> = ready
            .iter()
            .copied()
            .filter(|candidate| candidate.state.websocket_enabled)
            .collect();
        if !websocket_ready.is_empty() {
            ready = websocket_ready;
        }
    }

    ready
}

fn highest_ready_priority(
    candidates: &[&AuthCandidate],
    model: &str,
    now: DateTime<Utc>,
    prefer_websocket: bool,
) -> Option<i64> {
    let mut priorities: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.state.priority)
        .collect();
    priorities.sort_unstable();
    priorities.dedup();
    priorities.reverse();

    priorities.into_iter().find(|&priority| {
        !ready_candidates(candidates, model, now, priority, prefer_websocket).is_empty()
    })
}

fn summarize_unavailable(
    candidates: &[&AuthCandidate],
    provider: Option<&str>,
    model: &str,
    now: DateTime<Utc>,
) -> SchedulerError {
    if candidates.is_empty() {
        return SchedulerError::AuthNotFound;
    }

    let mut cooldown_count = 0usize;
    let mut earliest_cooldown: Option<DateTime<Utc>> = None;
    for candidate in candidates {
        match candidate.state.availability_for_model_at(model, now) {
            AuthCandidateAvailability::Cooldown { until } => {
                cooldown_count += 1;
                earliest_cooldown = Some(match earliest_cooldown {
                    Some(current) => current.min(until),
                    None => until,
                });
            }
            AuthCandidateAvailability::Ready
            | AuthCandidateAvailability::Blocked { .. }
            | AuthCandidateAvailability::Disabled => {}
        }
    }

    if cooldown_count == candidates.len() {
        if let Some(until) = earliest_cooldown {
            return SchedulerError::ModelCooldown {
                provider: provider.map(str::to_string),
                model: canonical_model_key(model),
                until,
            };
        }
    }

    SchedulerError::AuthUnavailable
}

fn normalize_provider(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_provider_list(providers: &[&str]) -> Vec<String> {
    let mut normalized = Vec::new();
    for provider in providers {
        let provider = normalize_provider(provider);
        if provider.is_empty() || normalized.iter().any(|existing| existing == &provider) {
            continue;
        }
        normalized.push(provider);
    }
    normalized
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
    use super::{AuthCandidate, AuthScheduler, SchedulerError};
    use crate::{AuthCandidateState, RoutingStrategy};
    use chrono::{TimeZone, Utc};
    use std::collections::HashSet;

    fn candidate(
        auth_id: &str,
        provider: &str,
        priority: i64,
        configure: impl FnOnce(&mut AuthCandidateState),
    ) -> AuthCandidate {
        let mut state = AuthCandidateState {
            priority,
            status: "active".to_string(),
            ..AuthCandidateState::default()
        };
        configure(&mut state);
        AuthCandidate {
            auth_id: auth_id.to_string(),
            provider: provider.to_ascii_lowercase(),
            state,
        }
    }

    #[test]
    fn round_robin_honors_priority_bucket() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut scheduler = AuthScheduler::new(RoutingStrategy::RoundRobin);
        let candidates = vec![
            candidate("auth-a", "codex", 1, |_| {}),
            candidate("auth-b", "codex", 5, |_| {}),
            candidate("auth-c", "codex", 5, |_| {}),
        ];

        let first = scheduler
            .pick_single(
                "codex",
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                false,
                now,
            )
            .expect("first pick");
        let second = scheduler
            .pick_single(
                "codex",
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                false,
                now,
            )
            .expect("second pick");
        let third = scheduler
            .pick_single(
                "codex",
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                false,
                now,
            )
            .expect("third pick");

        assert_eq!(first.auth_id, "auth-b");
        assert_eq!(second.auth_id, "auth-c");
        assert_eq!(third.auth_id, "auth-b");
    }

    #[test]
    fn fill_first_keeps_using_first_ready_candidate() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut scheduler = AuthScheduler::new(RoutingStrategy::FillFirst);
        let candidates = vec![
            candidate("auth-a", "codex", 5, |_| {}),
            candidate("auth-b", "codex", 5, |_| {}),
        ];

        let first = scheduler
            .pick_single(
                "codex",
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                false,
                now,
            )
            .expect("first pick");
        let second = scheduler
            .pick_single(
                "codex",
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                false,
                now,
            )
            .expect("second pick");

        assert_eq!(first.auth_id, "auth-a");
        assert_eq!(second.auth_id, "auth-a");
    }

    #[test]
    fn returns_cooldown_when_all_candidates_are_cooling_down() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut scheduler = AuthScheduler::new(RoutingStrategy::RoundRobin);
        let candidates = vec![
            candidate("auth-a", "codex", 5, |state| {
                state.unavailable = true;
                state.next_retry_after = Some(Utc.with_ymd_and_hms(2026, 4, 5, 12, 30, 0).unwrap());
                state.quota.exceeded = true;
                state.quota.next_recover_at =
                    Some(Utc.with_ymd_and_hms(2026, 4, 5, 12, 45, 0).unwrap());
            }),
            candidate("auth-b", "codex", 5, |state| {
                state.unavailable = true;
                state.next_retry_after = Some(Utc.with_ymd_and_hms(2026, 4, 5, 12, 20, 0).unwrap());
                state.quota.exceeded = true;
                state.quota.next_recover_at =
                    Some(Utc.with_ymd_and_hms(2026, 4, 5, 12, 25, 0).unwrap());
            }),
        ];

        let error = scheduler
            .pick_single(
                "codex",
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                false,
                now,
            )
            .expect_err("cooldown error");

        assert_eq!(
            error,
            SchedulerError::ModelCooldown {
                provider: Some("codex".to_string()),
                model: "gpt-5".to_string(),
                until: Utc.with_ymd_and_hms(2026, 4, 5, 12, 25, 0).unwrap()
            }
        );
    }

    #[test]
    fn mixed_scheduler_honors_pinned_auth() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut scheduler = AuthScheduler::new(RoutingStrategy::RoundRobin);
        let candidates = vec![
            candidate("codex-auth", "codex", 5, |_| {}),
            candidate("openai-auth", "openai", 5, |_| {}),
        ];

        let picked = scheduler
            .pick_mixed(
                &["codex", "openai"],
                "gpt-5",
                &candidates,
                Some("openai-auth"),
                &HashSet::new(),
                now,
            )
            .expect("pinned pick");

        assert_eq!(picked.auth_id, "openai-auth");
        assert_eq!(picked.provider, "openai");
    }

    #[test]
    fn mixed_scheduler_does_not_fallback_from_pinned_cooldown_auth() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut scheduler = AuthScheduler::new(RoutingStrategy::RoundRobin);
        let candidates = vec![
            candidate("codex-auth", "codex", 5, |state| {
                state.unavailable = true;
                state.next_retry_after = Some(Utc.with_ymd_and_hms(2026, 4, 5, 12, 30, 0).unwrap());
                state.quota.exceeded = true;
                state.quota.next_recover_at =
                    Some(Utc.with_ymd_and_hms(2026, 4, 5, 12, 35, 0).unwrap());
            }),
            candidate("openai-auth", "openai", 5, |_| {}),
        ];

        let error = scheduler
            .pick_mixed(
                &["codex", "openai"],
                "gpt-5",
                &candidates,
                Some("codex-auth"),
                &HashSet::new(),
                now,
            )
            .expect_err("pinned cooldown");

        assert_eq!(
            error,
            SchedulerError::ModelCooldown {
                provider: None,
                model: "gpt-5".to_string(),
                until: Utc.with_ymd_and_hms(2026, 4, 5, 12, 35, 0).unwrap()
            }
        );
    }

    #[test]
    fn mixed_round_robin_rotates_by_provider_weight() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut scheduler = AuthScheduler::new(RoutingStrategy::RoundRobin);
        let candidates = vec![
            candidate("codex-a", "codex", 5, |_| {}),
            candidate("codex-b", "codex", 5, |_| {}),
            candidate("openai-a", "openai", 5, |_| {}),
        ];

        let first = scheduler
            .pick_mixed(
                &["codex", "openai"],
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                now,
            )
            .expect("first mixed pick");
        let second = scheduler
            .pick_mixed(
                &["codex", "openai"],
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                now,
            )
            .expect("second mixed pick");
        let third = scheduler
            .pick_mixed(
                &["codex", "openai"],
                "gpt-5",
                &candidates,
                None,
                &HashSet::new(),
                now,
            )
            .expect("third mixed pick");

        assert_eq!(first.auth_id, "codex-a");
        assert_eq!(second.auth_id, "codex-b");
        assert_eq!(third.auth_id, "openai-a");
    }
}
