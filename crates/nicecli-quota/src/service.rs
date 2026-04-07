use crate::{
    AuthEnumerator, CodexAuthContext, CodexQuotaSnapshotEnvelope, CodexQuotaSource,
    CodexSourceError, FileBackedCodexAuthEnumerator, HttpCodexQuotaSource, ListOptions,
    RateLimitSnapshot, RefreshOptions, SnapshotCache, WorkspaceRef, DEFAULT_WORKSPACE_ID,
    PROVIDER_CODEX, SOURCE_USAGE_DASHBOARD,
};
use chrono::{SecondsFormat, Utc};
use nicecli_auth::{
    is_generic_codex_workspace_name, patch_auth_file_fields, CodexAccountProfile,
    PatchAuthFileFields,
};
use nicecli_runtime::{
    ExecutionError, ExecutionResult, FileAuthStore, RecordExecutionResultOptions,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodexQuotaError {
    #[error("failed to read auth dir: {0}")]
    ReadAuthDir(std::io::Error),
}

pub struct CodexQuotaService {
    cache: SnapshotCache,
    source: Arc<dyn CodexQuotaSource>,
    auths: Arc<dyn AuthEnumerator>,
    result_store: Option<FileAuthStore>,
}

impl std::fmt::Debug for CodexQuotaService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexQuotaService").finish_non_exhaustive()
    }
}

impl CodexQuotaService {
    pub fn new(auth_dir: impl Into<PathBuf>, default_proxy_url: Option<String>) -> Self {
        let auth_dir = auth_dir.into();
        Self::with_deps(
            Arc::new(FileBackedCodexAuthEnumerator::new(auth_dir.clone())),
            Arc::new(HttpCodexQuotaSource::new(default_proxy_url)),
        )
        .with_result_store(FileAuthStore::new(auth_dir))
    }

    pub fn with_deps(auths: Arc<dyn AuthEnumerator>, source: Arc<dyn CodexQuotaSource>) -> Self {
        Self {
            cache: SnapshotCache::new(),
            source,
            auths,
            result_store: None,
        }
    }

    pub fn with_result_store(mut self, result_store: FileAuthStore) -> Self {
        self.result_store = Some(result_store);
        self
    }

    pub async fn list_snapshots_with_options(
        &self,
        mut options: ListOptions,
    ) -> Result<Vec<CodexQuotaSnapshotEnvelope>, CodexQuotaError> {
        trim_options(&mut options.auth_id, &mut options.workspace_id);
        if options.refresh {
            return self
                .refresh_with_options(RefreshOptions {
                    auth_id: options.auth_id,
                    workspace_id: options.workspace_id,
                })
                .await;
        }

        let auths = match self.auths.list_codex_auths() {
            Ok(auths) => auths,
            Err(_) => return Ok(self.cache.list(&options.auth_id, &options.workspace_id)),
        };
        self.sync_cache_with_current_auths(&auths, &options.auth_id);

        let mut snapshots = self.cache.list(&options.auth_id, &options.workspace_id);
        apply_current_auth_metadata(&mut snapshots, &auths);
        Ok(snapshots)
    }

    pub async fn refresh_with_options(
        &self,
        mut options: RefreshOptions,
    ) -> Result<Vec<CodexQuotaSnapshotEnvelope>, CodexQuotaError> {
        trim_options(&mut options.auth_id, &mut options.workspace_id);

        let auths = self
            .auths
            .list_codex_auths()
            .map_err(CodexQuotaError::ReadAuthDir)?;
        self.sync_cache_with_current_auths(&auths, &options.auth_id);

        let mut effective_auths = auths.clone();
        for auth in effective_auths
            .iter_mut()
            .filter(|auth| options.auth_id.is_empty() || auth.auth_id == options.auth_id)
        {
            self.refresh_auth(auth, &options.workspace_id).await;
        }

        let mut snapshots = self.cache.list(&options.auth_id, &options.workspace_id);
        apply_current_auth_metadata(&mut snapshots, &effective_auths);
        Ok(snapshots)
    }

    async fn refresh_auth(&self, auth: &mut CodexAuthContext, requested_workspace_id: &str) {
        let original_note = auth.auth_note.clone();
        let account_profile = self.source.fetch_account_profile(auth).await;
        let note_changed = apply_account_profile(auth, account_profile.as_ref());
        if note_changed {
            self.persist_auth_note(auth, &original_note);
        }

        let workspaces = match self.source.list_workspaces(auth).await {
            Ok(workspaces) => workspaces,
            Err(error) => {
                let mut workspace =
                    workspace_for_failure(auth, requested_workspace_id, &self.cache);
                apply_account_profile_to_workspace(auth, &mut workspace, account_profile.as_ref());
                self.mark_refresh_failure(auth, workspace, error);
                return;
            }
        };

        let mut targets =
            filter_target_workspaces(auth, &workspaces, requested_workspace_id, &self.cache);
        if targets.is_empty() {
            let mut workspace = workspace_for_failure(auth, requested_workspace_id, &self.cache);
            apply_account_profile_to_workspace(auth, &mut workspace, account_profile.as_ref());
            targets.push(workspace);
        }

        for mut workspace in targets {
            workspace = normalize_workspace_ref(auth, workspace);
            apply_account_profile_to_workspace(auth, &mut workspace, account_profile.as_ref());
            match self.source.fetch_workspace_snapshot(auth, &workspace).await {
                Ok(snapshot) => {
                    self.record_refresh_success(auth);
                    self.upsert_snapshot(auth, workspace, snapshot);
                }
                Err(error) => {
                    self.record_refresh_failure(auth, &error);
                    self.mark_refresh_failure(auth, workspace, error);
                }
            }
        }
    }

    fn upsert_snapshot(
        &self,
        auth: &CodexAuthContext,
        workspace: WorkspaceRef,
        snapshot: RateLimitSnapshot,
    ) {
        self.cache.upsert(CodexQuotaSnapshotEnvelope {
            provider: PROVIDER_CODEX.to_string(),
            auth_id: auth.auth_id.clone(),
            auth_label: non_empty(&auth.auth_label),
            auth_note: non_empty(&auth.auth_note),
            auth_file_name: non_empty(&auth.auth_file_name),
            account_email: non_empty(&auth.account_email),
            account_plan: non_empty(&auth.account_plan),
            workspace_id: non_empty(&workspace.id),
            workspace_name: non_empty(&workspace.name),
            workspace_type: non_empty(&workspace.r#type),
            snapshot: Some(snapshot),
            source: SOURCE_USAGE_DASHBOARD.to_string(),
            fetched_at: now_rfc3339(),
            stale: false,
            error: None,
        });
    }

    fn mark_refresh_failure(
        &self,
        auth: &CodexAuthContext,
        workspace: WorkspaceRef,
        error: CodexSourceError,
    ) {
        let workspace = normalize_workspace_ref(auth, workspace);
        let mut existing = self
            .cache
            .get(&auth.auth_id, Some(&workspace.id))
            .unwrap_or(CodexQuotaSnapshotEnvelope {
                provider: PROVIDER_CODEX.to_string(),
                auth_id: auth.auth_id.clone(),
                auth_label: non_empty(&auth.auth_label),
                auth_note: non_empty(&auth.auth_note),
                auth_file_name: non_empty(&auth.auth_file_name),
                account_email: non_empty(&auth.account_email),
                account_plan: non_empty(&auth.account_plan),
                workspace_id: non_empty(&workspace.id),
                workspace_name: non_empty(&workspace.name),
                workspace_type: non_empty(&workspace.r#type),
                snapshot: None,
                source: SOURCE_USAGE_DASHBOARD.to_string(),
                fetched_at: now_rfc3339(),
                stale: true,
                error: None,
            });

        existing.auth_label = non_empty(&auth.auth_label);
        existing.auth_note =
            pick_preferred_workspace_name(non_empty(&auth.auth_note), existing.auth_note);
        existing.auth_file_name = non_empty(&auth.auth_file_name);
        existing.account_email = pick_first(non_empty(&auth.account_email), existing.account_email);
        existing.account_plan = pick_first(non_empty(&auth.account_plan), existing.account_plan);
        existing.workspace_name =
            pick_preferred_workspace_name(non_empty(&workspace.name), existing.workspace_name);
        existing.workspace_type = pick_first(non_empty(&workspace.r#type), existing.workspace_type);
        existing.fetched_at = now_rfc3339();
        existing.stale = true;
        existing.error = Some(error.to_string());
        self.cache.upsert(existing);
    }

    fn sync_cache_with_current_auths(&self, auths: &[CodexAuthContext], requested_auth_id: &str) {
        let requested_auth_id = requested_auth_id.trim();
        if !requested_auth_id.is_empty() {
            if auths.iter().any(|auth| auth.auth_id == requested_auth_id) {
                return;
            }
            self.cache.delete_auth(requested_auth_id);
            return;
        }

        let auth_ids: Vec<_> = auths.iter().map(|auth| auth.auth_id.clone()).collect();
        self.cache.retain_auth_ids(&auth_ids);
    }

    fn record_refresh_success(&self, auth: &CodexAuthContext) {
        self.persist_execution_result(
            auth,
            &ExecutionResult {
                model: None,
                success: true,
                retry_after: None,
                error: None,
            },
        );
    }

    fn record_refresh_failure(&self, auth: &CodexAuthContext, error: &CodexSourceError) {
        let Some(result) = execution_result_from_source_error(error) else {
            return;
        };
        self.persist_execution_result(auth, &result);
    }

    fn persist_execution_result(&self, auth: &CodexAuthContext, result: &ExecutionResult) {
        let Some(store) = &self.result_store else {
            return;
        };

        let auth_name = if auth.auth_file_name.trim().is_empty() {
            auth.auth_id.as_str()
        } else {
            auth.auth_file_name.as_str()
        };
        let _ = store.record_execution_result(
            auth_name,
            result,
            RecordExecutionResultOptions::new(Utc::now()),
        );
    }

    fn persist_auth_note(&self, auth: &CodexAuthContext, original_note: &str) {
        if !should_replace_workspace_name(Some(original_note), Some(&auth.auth_note)) {
            return;
        }

        let Some(store) = &self.result_store else {
            return;
        };
        let auth_name = if auth.auth_file_name.trim().is_empty() {
            auth.auth_id.as_str()
        } else {
            auth.auth_file_name.as_str()
        };
        if !auth_name.to_ascii_lowercase().ends_with(".json") {
            return;
        }

        let _ = patch_auth_file_fields(
            store.auth_dir(),
            auth_name,
            &PatchAuthFileFields {
                note: non_empty(&auth.auth_note),
                ..PatchAuthFileFields::default()
            },
        );
    }
}

fn execution_result_from_source_error(error: &CodexSourceError) -> Option<ExecutionResult> {
    match error {
        CodexSourceError::UnexpectedStatus { status, body } => {
            if !matches!(*status, 401 | 402 | 403 | 408 | 429 | 500 | 502 | 503 | 504) {
                return None;
            }

            let message = body.trim();
            Some(ExecutionResult {
                model: None,
                success: false,
                retry_after: None,
                error: Some(ExecutionError {
                    message: if message.is_empty() {
                        error.to_string()
                    } else {
                        message.to_string()
                    },
                    http_status: Some(*status),
                }),
            })
        }
        _ => None,
    }
}

fn filter_target_workspaces(
    auth: &CodexAuthContext,
    workspaces: &[WorkspaceRef],
    requested_workspace_id: &str,
    cache: &SnapshotCache,
) -> Vec<WorkspaceRef> {
    if workspaces.is_empty() {
        return if requested_workspace_id.trim().is_empty() {
            Vec::new()
        } else {
            vec![workspace_for_failure(auth, requested_workspace_id, cache)]
        };
    }

    let requested_workspace_id = requested_workspace_id.trim();
    if requested_workspace_id.is_empty() {
        return workspaces
            .iter()
            .cloned()
            .map(|workspace| normalize_workspace_ref(auth, workspace))
            .collect();
    }

    let filtered: Vec<_> = workspaces
        .iter()
        .filter(|workspace| workspace.id.trim() == requested_workspace_id)
        .cloned()
        .map(|workspace| normalize_workspace_ref(auth, workspace))
        .collect();
    if filtered.is_empty() {
        vec![workspace_for_failure(auth, requested_workspace_id, cache)]
    } else {
        filtered
    }
}

fn workspace_for_failure(
    auth: &CodexAuthContext,
    requested_workspace_id: &str,
    cache: &SnapshotCache,
) -> WorkspaceRef {
    let requested_workspace_id = requested_workspace_id.trim();
    if requested_workspace_id.is_empty() {
        return crate::select_current_workspace(auth);
    }
    if let Some(existing) = cache.get(&auth.auth_id, Some(requested_workspace_id)) {
        return WorkspaceRef {
            id: existing
                .workspace_id
                .unwrap_or_else(|| requested_workspace_id.to_string()),
            name: existing
                .workspace_name
                .unwrap_or_else(|| requested_workspace_id.to_string()),
            r#type: existing
                .workspace_type
                .unwrap_or_else(|| "unknown".to_string()),
        };
    }

    let mut workspace = crate::select_current_workspace(auth);
    workspace.id = requested_workspace_id.to_string();
    if workspace.name.trim().is_empty() {
        workspace.name = requested_workspace_id.to_string();
    }
    workspace
}

fn normalize_workspace_ref(auth: &CodexAuthContext, mut workspace: WorkspaceRef) -> WorkspaceRef {
    workspace.id = workspace.id.trim().to_string();
    workspace.name = workspace.name.trim().to_string();
    workspace.r#type = workspace.r#type.trim().to_string();

    if workspace.id.is_empty() {
        let fallback = crate::select_current_workspace(auth);
        workspace.id = fallback.id;
        if workspace.name.is_empty() {
            workspace.name = fallback.name;
        }
        if workspace.r#type.is_empty() {
            workspace.r#type = fallback.r#type;
        }
    }
    if workspace.name.is_empty() {
        workspace.name = if workspace.id.is_empty() {
            DEFAULT_WORKSPACE_ID.to_string()
        } else {
            workspace.id.clone()
        };
    }
    if workspace.r#type.is_empty() {
        workspace.r#type = "unknown".to_string();
    }
    workspace
}

fn apply_current_auth_metadata(
    snapshots: &mut [CodexQuotaSnapshotEnvelope],
    auths: &[CodexAuthContext],
) {
    let auth_by_id: HashMap<_, _> = auths
        .iter()
        .map(|auth| (auth.auth_id.as_str(), auth))
        .collect();
    for snapshot in snapshots {
        let Some(auth) = auth_by_id.get(snapshot.auth_id.as_str()) else {
            continue;
        };
        snapshot.auth_label = non_empty(&auth.auth_label);
        snapshot.auth_note =
            pick_preferred_workspace_name(non_empty(&auth.auth_note), snapshot.auth_note.clone());
        snapshot.auth_file_name = non_empty(&auth.auth_file_name);
        snapshot.account_email = pick_first(
            non_empty(&auth.account_email),
            snapshot.account_email.clone(),
        );
        snapshot.account_plan =
            pick_first(non_empty(&auth.account_plan), snapshot.account_plan.clone());
    }
}

fn trim_options(auth_id: &mut String, workspace_id: &mut String) {
    *auth_id = auth_id.trim().to_string();
    *workspace_id = workspace_id.trim().to_string();
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn pick_first(primary: Option<String>, fallback: Option<String>) -> Option<String> {
    primary.or(fallback)
}

fn apply_account_profile(
    auth: &mut CodexAuthContext,
    profile: Option<&CodexAccountProfile>,
) -> bool {
    let Some(profile) = profile else {
        return false;
    };

    let mut changed = false;

    if auth.account_id.trim().is_empty() {
        if let Some(account_id) = profile.account_id.as_deref().and_then(non_empty_ref) {
            auth.account_id = account_id.to_string();
            changed = true;
        }
    }

    if let Some(next_note) = profile.account_name.as_deref().and_then(non_empty_ref) {
        if !should_replace_workspace_name(Some(auth.auth_note.as_str()), Some(next_note)) {
            return changed;
        }

        let next_note = next_note.to_string();
        if auth.auth_note != next_note {
            auth.auth_note = next_note;
            changed = true;
        }
    }

    changed
}

fn apply_account_profile_to_workspace(
    auth: &CodexAuthContext,
    workspace: &mut WorkspaceRef,
    profile: Option<&CodexAccountProfile>,
) {
    let Some(profile) = profile else {
        return;
    };
    let Some(account_name) = profile.account_name.as_deref().and_then(non_empty_ref) else {
        return;
    };
    if is_generic_codex_workspace_name(account_name) {
        return;
    }

    let workspace_id = non_empty_ref(&workspace.id);
    let profile_account_id = profile.account_id.as_deref().and_then(non_empty_ref);
    let auth_account_id = non_empty_ref(&auth.account_id);
    let matches_workspace = match (workspace_id, profile_account_id, auth_account_id) {
        (Some(workspace_id), Some(profile_account_id), _) => {
            workspace_id.eq_ignore_ascii_case(profile_account_id)
        }
        (Some(workspace_id), None, Some(auth_account_id)) => {
            workspace_id.eq_ignore_ascii_case(auth_account_id)
        }
        (None, _, _) => true,
        _ => false,
    };

    if matches_workspace
        && (workspace.name.trim().is_empty() || is_generic_codex_workspace_name(&workspace.name))
    {
        workspace.name = account_name.to_string();
    }
}

fn pick_preferred_workspace_name(
    primary: Option<String>,
    fallback: Option<String>,
) -> Option<String> {
    if primary
        .as_deref()
        .map(|value| !is_generic_codex_workspace_name(value))
        .unwrap_or(false)
    {
        return primary;
    }
    if fallback
        .as_deref()
        .map(|value| !is_generic_codex_workspace_name(value))
        .unwrap_or(false)
    {
        return fallback;
    }
    primary.or(fallback)
}

fn should_replace_workspace_name(current: Option<&str>, candidate: Option<&str>) -> bool {
    let Some(candidate) = candidate.and_then(non_empty_ref) else {
        return false;
    };
    if is_generic_codex_workspace_name(candidate) {
        return false;
    }

    match current.and_then(non_empty_ref) {
        None => true,
        Some(current) => is_generic_codex_workspace_name(current),
    }
}

fn non_empty_ref(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_account_profile, CodexQuotaService};
    use crate::{
        AuthEnumerator, CodexAuthContext, CodexQuotaSource, ListOptions, RateLimitSnapshot,
        WorkspaceRef,
    };
    use async_trait::async_trait;
    use nicecli_auth::CodexAccountProfile;
    use nicecli_runtime::FileAuthStore;
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[derive(Clone)]
    struct FakeAuthEnumerator {
        auths: Vec<CodexAuthContext>,
    }

    impl AuthEnumerator for FakeAuthEnumerator {
        fn list_codex_auths(&self) -> Result<Vec<CodexAuthContext>, std::io::Error> {
            Ok(self.auths.clone())
        }
    }

    #[derive(Clone)]
    struct AlwaysSuccessSource;

    #[async_trait]
    impl CodexQuotaSource for AlwaysSuccessSource {
        async fn list_workspaces(
            &self,
            _auth: &CodexAuthContext,
        ) -> Result<Vec<WorkspaceRef>, crate::CodexSourceError> {
            Ok(vec![WorkspaceRef {
                id: "org_default".to_string(),
                name: "Workspace A".to_string(),
                r#type: "business".to_string(),
            }])
        }

        async fn fetch_workspace_snapshot(
            &self,
            _auth: &CodexAuthContext,
            _workspace: &WorkspaceRef,
        ) -> Result<RateLimitSnapshot, crate::CodexSourceError> {
            Ok(RateLimitSnapshot {
                limit_id: Some("codex".to_string()),
                limit_name: None,
                primary: None,
                secondary: None,
                credits: None,
                plan_type: Some("team".to_string()),
            })
        }
    }

    #[derive(Clone)]
    struct QuotaExceededSource;

    #[async_trait]
    impl CodexQuotaSource for QuotaExceededSource {
        async fn list_workspaces(
            &self,
            _auth: &CodexAuthContext,
        ) -> Result<Vec<WorkspaceRef>, crate::CodexSourceError> {
            Ok(vec![WorkspaceRef {
                id: "org_default".to_string(),
                name: "Workspace A".to_string(),
                r#type: "business".to_string(),
            }])
        }

        async fn fetch_workspace_snapshot(
            &self,
            _auth: &CodexAuthContext,
            _workspace: &WorkspaceRef,
        ) -> Result<RateLimitSnapshot, crate::CodexSourceError> {
            Err(crate::CodexSourceError::UnexpectedStatus {
                status: 429,
                body: "quota exhausted".to_string(),
            })
        }
    }

    #[derive(Clone)]
    struct HydratedProfileSource;

    #[async_trait]
    impl CodexQuotaSource for HydratedProfileSource {
        async fn fetch_account_profile(
            &self,
            _auth: &CodexAuthContext,
        ) -> Option<CodexAccountProfile> {
            Some(CodexAccountProfile {
                account_id: Some("org_default".to_string()),
                account_name: Some("MyTeam".to_string()),
                account_structure: Some("workspace".to_string()),
            })
        }

        async fn list_workspaces(
            &self,
            _auth: &CodexAuthContext,
        ) -> Result<Vec<WorkspaceRef>, crate::CodexSourceError> {
            Ok(vec![WorkspaceRef {
                id: "org_default".to_string(),
                name: "Personal".to_string(),
                r#type: "business".to_string(),
            }])
        }

        async fn fetch_workspace_snapshot(
            &self,
            _auth: &CodexAuthContext,
            _workspace: &WorkspaceRef,
        ) -> Result<RateLimitSnapshot, crate::CodexSourceError> {
            Ok(RateLimitSnapshot {
                limit_id: Some("codex".to_string()),
                limit_name: None,
                primary: None,
                secondary: None,
                credits: None,
                plan_type: Some("team".to_string()),
            })
        }
    }

    fn demo_auth_context(auth_file_name: &str) -> CodexAuthContext {
        CodexAuthContext {
            auth_id: auth_file_name.to_string(),
            auth_label: "Primary".to_string(),
            auth_note: "Workspace A".to_string(),
            auth_file_name: auth_file_name.to_string(),
            account_email: "demo@example.com".to_string(),
            account_plan: "team".to_string(),
            account_id: "org_default".to_string(),
            cookies: Default::default(),
            access_token: "token".to_string(),
            refresh_token: String::new(),
            id_token: String::new(),
            base_url: String::new(),
            proxy_url: String::new(),
        }
    }

    fn build_service() -> CodexQuotaService {
        CodexQuotaService::with_deps(
            Arc::new(FakeAuthEnumerator {
                auths: vec![demo_auth_context("codex-demo@example.com-team.json")],
            }),
            Arc::new(AlwaysSuccessSource),
        )
    }

    fn seed_auth_file(temp_dir: &TempDir, auth_file_name: &str) {
        fs::write(
            temp_dir.path().join(auth_file_name),
            r#"{
  "type": "codex",
  "provider": "codex",
  "email": "demo@example.com",
  "access_token": "token"
}"#,
        )
        .expect("seed auth file");
    }

    #[tokio::test]
    async fn refresh_populates_snapshot_cache() {
        let service = build_service();
        let snapshots = service
            .list_snapshots_with_options(ListOptions {
                refresh: true,
                ..ListOptions::default()
            })
            .await
            .expect("refresh");

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].workspace_name.as_deref(), Some("Workspace A"));
        assert_eq!(snapshots[0].account_plan.as_deref(), Some("team"));
        assert_eq!(
            snapshots[0]
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.plan_type.as_deref()),
            Some("team")
        );
    }

    #[tokio::test]
    async fn refresh_failure_persists_quota_cooldown_and_success_clears_it() {
        let temp_dir = TempDir::new().expect("temp dir");
        let auth_file_name = "codex-demo@example.com-team.json";
        seed_auth_file(&temp_dir, auth_file_name);

        let auths = vec![demo_auth_context(auth_file_name)];
        let failing_service = CodexQuotaService::with_deps(
            Arc::new(FakeAuthEnumerator {
                auths: auths.clone(),
            }),
            Arc::new(QuotaExceededSource),
        )
        .with_result_store(FileAuthStore::new(temp_dir.path()));

        let failed = failing_service
            .list_snapshots_with_options(ListOptions {
                refresh: true,
                ..ListOptions::default()
            })
            .await
            .expect("refresh failure");
        assert_eq!(failed.len(), 1);
        assert!(failed[0].stale);

        let failed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join(auth_file_name)).expect("read failed auth"),
        )
        .expect("failed auth json");
        assert_eq!(failed_auth["status"].as_str(), Some("error"));
        assert_eq!(failed_auth["unavailable"].as_bool(), Some(true));
        assert_eq!(failed_auth["quota"]["exceeded"].as_bool(), Some(true));
        assert_eq!(failed_auth["last_error"]["http_status"].as_u64(), Some(429));

        let success_service = CodexQuotaService::with_deps(
            Arc::new(FakeAuthEnumerator { auths }),
            Arc::new(AlwaysSuccessSource),
        )
        .with_result_store(FileAuthStore::new(temp_dir.path()));

        let refreshed = success_service
            .list_snapshots_with_options(ListOptions {
                refresh: true,
                ..ListOptions::default()
            })
            .await
            .expect("refresh success");
        assert_eq!(refreshed.len(), 1);
        assert!(!refreshed[0].stale);

        let refreshed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join(auth_file_name)).expect("read refreshed auth"),
        )
        .expect("refreshed auth json");
        assert_eq!(refreshed_auth["status"].as_str(), Some("active"));
        assert!(refreshed_auth.get("unavailable").is_none());
        assert!(refreshed_auth.get("status_message").is_none());
        assert!(refreshed_auth.get("quota").is_none());
        assert!(refreshed_auth.get("last_error").is_none());
    }

    #[tokio::test]
    async fn refresh_uses_remote_account_name_for_generic_auth_note_and_persists_it() {
        let temp_dir = TempDir::new().expect("temp dir");
        let auth_file_name = "codex-demo@example.com-team.json";
        fs::write(
            temp_dir.path().join(auth_file_name),
            r#"{
  "type": "codex",
  "provider": "codex",
  "email": "demo@example.com",
  "note": "Personal",
  "access_token": "token"
}"#,
        )
        .expect("seed auth file");

        let service = CodexQuotaService::with_deps(
            Arc::new(FakeAuthEnumerator {
                auths: vec![CodexAuthContext {
                    auth_note: "Personal".to_string(),
                    ..demo_auth_context(auth_file_name)
                }],
            }),
            Arc::new(HydratedProfileSource),
        )
        .with_result_store(FileAuthStore::new(temp_dir.path()));

        let snapshots = service
            .list_snapshots_with_options(ListOptions {
                refresh: true,
                ..ListOptions::default()
            })
            .await
            .expect("refresh");

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].auth_note.as_deref(), Some("MyTeam"));
        assert_eq!(snapshots[0].workspace_name.as_deref(), Some("MyTeam"));

        let auth_json: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join(auth_file_name)).expect("read auth"),
        )
        .expect("auth json");
        assert_eq!(auth_json["note"].as_str(), Some("MyTeam"));
    }

    #[test]
    fn apply_account_profile_ignores_missing_remote_account_name() {
        let mut auth = demo_auth_context("codex-demo@example.com-team.json");
        auth.auth_note = "Personal".to_string();
        let profile = CodexAccountProfile {
            account_id: Some("org_default".to_string()),
            account_name: None,
            account_structure: Some("workspace".to_string()),
        };

        let changed = apply_account_profile(&mut auth, Some(&profile));

        assert!(!changed);
        assert_eq!(auth.auth_note, "Personal");
    }
}
