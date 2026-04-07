use crate::{
    AuthCandidate, AuthScheduler, AuthSnapshot, AuthStore, AuthStoreError, ExecutionResult,
    FileAuthStore, RecordExecutionResultOptions, RecordExecutionResultOutcome, RoutingStrategy,
    SchedulerError, SchedulerPick,
};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickExecutionOptions {
    pub pinned_auth_id: Option<String>,
    pub tried_auth_ids: HashSet<String>,
    pub prefer_websocket: bool,
    pub now: DateTime<Utc>,
}

impl PickExecutionOptions {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            pinned_auth_id: None,
            tried_auth_ids: HashSet::new(),
            prefer_websocket: false,
            now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSelection {
    pub auth_id: String,
    pub provider: String,
    pub priority: i64,
    pub snapshot: AuthSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteWithRetryOptions {
    pub pick: PickExecutionOptions,
    pub max_auth_attempts: Option<usize>,
    pub skip_persist: bool,
    pub runtime_only: bool,
    pub disable_cooling: bool,
}

impl ExecuteWithRetryOptions {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            pick: PickExecutionOptions::new(now),
            max_auth_attempts: None,
            skip_persist: false,
            runtime_only: false,
            disable_cooling: false,
        }
    }

    fn record_options(&self) -> RecordExecutionResultOptions {
        RecordExecutionResultOptions {
            now: self.pick.now,
            skip_persist: self.skip_persist,
            runtime_only: self.runtime_only,
            disable_cooling: self.disable_cooling,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFailure<E> {
    pub error: E,
    pub result: ExecutionResult,
    pub retryable: bool,
}

impl<E> ExecutionFailure<E> {
    pub fn retryable(error: E, result: ExecutionResult) -> Self {
        Self {
            error,
            result,
            retryable: true,
        }
    }

    pub fn terminal(error: E, result: ExecutionResult) -> Self {
        Self {
            error,
            result,
            retryable: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executed<T> {
    pub selection: ExecutionSelection,
    pub value: T,
}

#[derive(Debug)]
pub enum ExecuteWithRetryError<E> {
    Runtime(RuntimeConductorError),
    Provider(E),
}

#[derive(Debug, Error)]
pub enum RuntimeConductorError {
    #[error(transparent)]
    Store(#[from] AuthStoreError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error("selected auth is missing from the latest snapshot set: {0}")]
    SelectedAuthMissing(String),
}

#[derive(Debug)]
pub struct RuntimeConductor {
    store: FileAuthStore,
    scheduler: Arc<Mutex<AuthScheduler>>,
}

impl RuntimeConductor {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            store: FileAuthStore::new(auth_dir.clone()),
            scheduler: shared_scheduler_for_auth_dir(&auth_dir, strategy),
        }
    }

    pub fn pick_single(
        &mut self,
        provider: &str,
        model: &str,
        options: &PickExecutionOptions,
    ) -> Result<ExecutionSelection, RuntimeConductorError> {
        let (candidates, snapshots_by_id) = self.load_candidates()?;
        let pick = {
            let mut scheduler = self.lock_scheduler();
            scheduler.pick_single(
                provider,
                model,
                &candidates,
                options.pinned_auth_id.as_deref(),
                &options.tried_auth_ids,
                options.prefer_websocket,
                options.now,
            )?
        };
        build_selection(pick, snapshots_by_id)
    }

    pub fn pick_mixed(
        &mut self,
        providers: &[&str],
        model: &str,
        options: &PickExecutionOptions,
    ) -> Result<ExecutionSelection, RuntimeConductorError> {
        let (candidates, snapshots_by_id) = self.load_candidates()?;
        let pick = {
            let mut scheduler = self.lock_scheduler();
            scheduler.pick_mixed(
                providers,
                model,
                &candidates,
                options.pinned_auth_id.as_deref(),
                &options.tried_auth_ids,
                options.now,
            )?
        };
        build_selection(pick, snapshots_by_id)
    }

    pub fn record_result(
        &self,
        auth_id: &str,
        result: &ExecutionResult,
        options: RecordExecutionResultOptions,
    ) -> Result<RecordExecutionResultOutcome, RuntimeConductorError> {
        self.store
            .record_execution_result(auth_id, result, options)
            .map_err(RuntimeConductorError::from)
    }

    pub async fn execute_single_with_retry<T, E, F, Fut>(
        &mut self,
        provider: &str,
        model: &str,
        options: ExecuteWithRetryOptions,
        execute: F,
    ) -> Result<Executed<T>, ExecuteWithRetryError<E>>
    where
        F: FnMut(&ExecutionSelection) -> Fut,
        Fut: Future<Output = Result<T, ExecutionFailure<E>>>,
    {
        self.execute_with_retry(model, options, execute, |this, pick| {
            this.pick_single(provider, model, pick)
        })
        .await
    }

    pub async fn execute_mixed_with_retry<T, E, F, Fut>(
        &mut self,
        providers: &[&str],
        model: &str,
        options: ExecuteWithRetryOptions,
        execute: F,
    ) -> Result<Executed<T>, ExecuteWithRetryError<E>>
    where
        F: FnMut(&ExecutionSelection) -> Fut,
        Fut: Future<Output = Result<T, ExecutionFailure<E>>>,
    {
        self.execute_with_retry(model, options, execute, |this, pick| {
            this.pick_mixed(providers, model, pick)
        })
        .await
    }

    fn load_candidates(
        &self,
    ) -> Result<(Vec<AuthCandidate>, HashMap<String, AuthSnapshot>), AuthStoreError> {
        let snapshots = self.store.list_snapshots()?;
        let mut candidates = Vec::with_capacity(snapshots.len());
        let mut snapshots_by_id = HashMap::with_capacity(snapshots.len());
        for snapshot in snapshots {
            candidates.push(AuthCandidate::from(&snapshot));
            snapshots_by_id.insert(snapshot.id.clone(), snapshot);
        }
        Ok((candidates, snapshots_by_id))
    }

    fn lock_scheduler(&self) -> MutexGuard<'_, AuthScheduler> {
        self.scheduler
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    async fn execute_with_retry<T, E, F, Fut, P>(
        &mut self,
        model: &str,
        mut options: ExecuteWithRetryOptions,
        mut execute: F,
        mut pick: P,
    ) -> Result<Executed<T>, ExecuteWithRetryError<E>>
    where
        F: FnMut(&ExecutionSelection) -> Fut,
        Fut: Future<Output = Result<T, ExecutionFailure<E>>>,
        P: FnMut(
            &mut Self,
            &PickExecutionOptions,
        ) -> Result<ExecutionSelection, RuntimeConductorError>,
    {
        let mut last_error = None;
        loop {
            if options
                .max_auth_attempts
                .is_some_and(|limit| options.pick.tried_auth_ids.len() >= limit)
            {
                return Err(last_error.map_or_else(
                    || {
                        ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
                            SchedulerError::AuthNotFound,
                        ))
                    },
                    ExecuteWithRetryError::Provider,
                ));
            }

            let selection = match pick(self, &options.pick) {
                Ok(selection) => selection,
                Err(error) => {
                    return Err(last_error.map_or_else(
                        || ExecuteWithRetryError::Runtime(error),
                        ExecuteWithRetryError::Provider,
                    ));
                }
            };

            options
                .pick
                .tried_auth_ids
                .insert(selection.auth_id.clone());
            match execute(&selection).await {
                Ok(value) => {
                    let result = success_result_for_model(model);
                    self.record_result(&selection.auth_id, &result, options.record_options())
                        .map_err(ExecuteWithRetryError::Runtime)?;
                    return Ok(Executed { selection, value });
                }
                Err(mut failure) => {
                    if failure
                        .result
                        .model
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                    {
                        failure.result.model = normalized_model(model);
                    }
                    self.record_result(
                        &selection.auth_id,
                        &failure.result,
                        options.record_options(),
                    )
                    .map_err(ExecuteWithRetryError::Runtime)?;
                    if !failure.retryable {
                        return Err(ExecuteWithRetryError::Provider(failure.error));
                    }
                    last_error = Some(failure.error);
                }
            }
        }
    }
}

fn normalized_model(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn success_result_for_model(model: &str) -> ExecutionResult {
    ExecutionResult {
        model: normalized_model(model),
        success: true,
        retry_after: None,
        error: None,
    }
}

fn build_selection(
    pick: SchedulerPick,
    mut snapshots_by_id: HashMap<String, AuthSnapshot>,
) -> Result<ExecutionSelection, RuntimeConductorError> {
    let snapshot = snapshots_by_id
        .remove(&pick.auth_id)
        .ok_or_else(|| RuntimeConductorError::SelectedAuthMissing(pick.auth_id.clone()))?;
    Ok(ExecutionSelection {
        auth_id: pick.auth_id,
        provider: pick.provider,
        priority: pick.priority,
        snapshot,
    })
}

fn shared_scheduler_for_auth_dir(
    auth_dir: &std::path::Path,
    strategy: RoutingStrategy,
) -> Arc<Mutex<AuthScheduler>> {
    let scheduler = {
        let mut registry = scheduler_registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry
            .entry(auth_dir.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(AuthScheduler::new(strategy))))
            .clone()
    };

    scheduler
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .set_strategy(strategy);
    scheduler
}

fn scheduler_registry() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<AuthScheduler>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<AuthScheduler>>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::{
        ExecuteWithRetryError, ExecuteWithRetryOptions, ExecutionFailure, PickExecutionOptions,
        RuntimeConductor,
    };
    use crate::{ExecutionError, ExecutionResult, RecordExecutionResultOptions, RoutingStrategy};
    use chrono::{TimeZone, Utc};
    use std::fs;

    #[test]
    fn fill_first_moves_to_next_auth_after_recorded_quota_failure() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp_dir.path().join("codex-a@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth a");
        fs::write(
            temp_dir.path().join("codex-b@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth b");

        let mut conductor = RuntimeConductor::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();

        let first = conductor
            .pick_single("codex", "gpt-5", &PickExecutionOptions::new(now))
            .expect("first pick");
        assert_eq!(first.auth_id, "codex-a@example.com-team.json");

        conductor
            .record_result(
                &first.auth_id,
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
            .expect("record failure");

        let second = conductor
            .pick_single("codex", "gpt-5", &PickExecutionOptions::new(now))
            .expect("second pick");
        assert_eq!(second.auth_id, "codex-b@example.com-team.json");
    }

    #[test]
    fn cooled_down_auth_becomes_selectable_again_after_retry_deadline() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp_dir.path().join("codex-a@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth a");
        fs::write(
            temp_dir.path().join("codex-b@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth b");

        let mut conductor = RuntimeConductor::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();

        let first = conductor
            .pick_single("codex", "gpt-5", &PickExecutionOptions::new(now))
            .expect("first pick");
        assert_eq!(first.auth_id, "codex-a@example.com-team.json");

        conductor
            .record_result(
                &first.auth_id,
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
            .expect("record failure");

        let after_cooldown = conductor
            .pick_single(
                "codex",
                "gpt-5",
                &PickExecutionOptions::new(now + chrono::Duration::seconds(2)),
            )
            .expect("pick after cooldown");
        assert_eq!(after_cooldown.auth_id, "codex-a@example.com-team.json");
    }

    #[tokio::test]
    async fn execute_single_with_retry_moves_to_next_auth_after_retryable_failure() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp_dir.path().join("codex-a@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth a");
        fs::write(
            temp_dir.path().join("codex-b@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth b");

        let mut conductor = RuntimeConductor::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();

        let executed = conductor
            .execute_single_with_retry(
                "codex",
                "gpt-5",
                ExecuteWithRetryOptions::new(now),
                |selection| {
                    let auth_id = selection.auth_id.clone();
                    async move {
                        if auth_id == "codex-a@example.com-team.json" {
                            Err(ExecutionFailure::retryable(
                                "quota".to_string(),
                                ExecutionResult {
                                    model: Some("gpt-5".to_string()),
                                    success: false,
                                    retry_after: None,
                                    error: Some(ExecutionError {
                                        message: "quota".to_string(),
                                        http_status: Some(429),
                                    }),
                                },
                            ))
                        } else {
                            Ok("ok".to_string())
                        }
                    }
                },
            )
            .await
            .expect("executed");

        assert_eq!(executed.value, "ok");
        assert_eq!(executed.selection.auth_id, "codex-b@example.com-team.json");

        let failed_auth = fs::read_to_string(temp_dir.path().join("codex-a@example.com-team.json"))
            .expect("read failed auth");
        assert!(failed_auth.contains("\"status\": \"error\""));
        assert!(failed_auth.contains("\"http_status\": 429"));
    }

    #[tokio::test]
    async fn execute_single_with_retry_stops_on_terminal_failure() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp_dir.path().join("codex-a@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth a");
        fs::write(
            temp_dir.path().join("codex-b@example.com-team.json"),
            r#"{"type":"codex","models":[{"name":"gpt-5"}]}"#,
        )
        .expect("seed auth b");

        let mut conductor = RuntimeConductor::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();

        let error = conductor
            .execute_single_with_retry(
                "codex",
                "gpt-5",
                ExecuteWithRetryOptions::new(now),
                |_selection| async {
                    Err::<String, ExecutionFailure<String>>(ExecutionFailure::terminal(
                        "invalid request".to_string(),
                        ExecutionResult {
                            model: Some("gpt-5".to_string()),
                            success: false,
                            retry_after: None,
                            error: Some(ExecutionError {
                                message: "invalid request".to_string(),
                                http_status: Some(400),
                            }),
                        },
                    ))
                },
            )
            .await
            .expect_err("terminal failure");

        match error {
            ExecuteWithRetryError::Provider(message) => {
                assert_eq!(message, "invalid request");
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let untouched_auth =
            fs::read_to_string(temp_dir.path().join("codex-b@example.com-team.json"))
                .expect("read untouched auth");
        assert!(!untouched_auth.contains("invalid request"));
    }
}
