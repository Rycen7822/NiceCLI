use crate::{normalize_provider, CodexQuotaSnapshotEnvelope, PROVIDER_CODEX};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

#[derive(Debug, Default)]
pub struct SnapshotCache {
    items: RwLock<HashMap<String, CodexQuotaSnapshotEnvelope>>,
}

impl SnapshotCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&self, mut snapshot: CodexQuotaSnapshotEnvelope) {
        snapshot.provider = normalize_provider(&snapshot.provider);
        let key = snapshot_cache_key(&snapshot.auth_id, snapshot.workspace_id.as_deref());
        self.items
            .write()
            .expect("snapshot cache write lock")
            .insert(key, snapshot);
    }

    pub fn get(
        &self,
        auth_id: &str,
        workspace_id: Option<&str>,
    ) -> Option<CodexQuotaSnapshotEnvelope> {
        self.items
            .read()
            .expect("snapshot cache read lock")
            .get(&snapshot_cache_key(auth_id, workspace_id))
            .cloned()
    }

    pub fn list(&self, auth_id: &str, workspace_id: &str) -> Vec<CodexQuotaSnapshotEnvelope> {
        let trimmed_auth_id = auth_id.trim();
        let trimmed_workspace_id = workspace_id.trim();
        let mut snapshots: Vec<_> = self
            .items
            .read()
            .expect("snapshot cache read lock")
            .values()
            .filter(|snapshot| {
                (trimmed_auth_id.is_empty() || snapshot.auth_id == trimmed_auth_id)
                    && (trimmed_workspace_id.is_empty()
                        || snapshot.workspace_id.as_deref().unwrap_or_default()
                            == trimmed_workspace_id)
            })
            .cloned()
            .collect();

        snapshots.sort_by(|left, right| {
            left.auth_id
                .cmp(&right.auth_id)
                .then_with(|| left.workspace_id.cmp(&right.workspace_id))
        });
        snapshots
    }

    pub fn delete_auth(&self, auth_id: &str) {
        let trimmed = auth_id.trim();
        if trimmed.is_empty() {
            return;
        }

        self.items
            .write()
            .expect("snapshot cache write lock")
            .retain(|_, snapshot| snapshot.auth_id != trimmed);
    }

    pub fn retain_auth_ids(&self, auth_ids: &[String]) {
        let allowed: HashSet<_> = auth_ids
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();

        self.items
            .write()
            .expect("snapshot cache write lock")
            .retain(|_, snapshot| allowed.contains(snapshot.auth_id.trim()));
    }
}

fn snapshot_cache_key(auth_id: &str, workspace_id: Option<&str>) -> String {
    format!(
        "{}:{}:{}",
        PROVIDER_CODEX,
        auth_id.trim(),
        workspace_id.unwrap_or_default().trim()
    )
}

#[cfg(test)]
mod tests {
    use super::SnapshotCache;
    use crate::{CodexQuotaSnapshotEnvelope, PROVIDER_CODEX, SOURCE_USAGE_DASHBOARD};

    fn build_snapshot(auth_id: &str, workspace_id: &str) -> CodexQuotaSnapshotEnvelope {
        CodexQuotaSnapshotEnvelope {
            provider: PROVIDER_CODEX.to_string(),
            auth_id: auth_id.to_string(),
            auth_label: None,
            auth_note: None,
            auth_file_name: None,
            account_email: None,
            account_plan: None,
            workspace_id: Some(workspace_id.to_string()),
            workspace_name: None,
            workspace_type: None,
            snapshot: None,
            source: SOURCE_USAGE_DASHBOARD.to_string(),
            fetched_at: "2026-04-04T00:00:00Z".to_string(),
            stale: false,
            error: None,
        }
    }

    #[test]
    fn retains_requested_auth_ids() {
        let cache = SnapshotCache::new();
        cache.upsert(build_snapshot("auth-a", "ws-a"));
        cache.upsert(build_snapshot("auth-b", "ws-b"));

        cache.retain_auth_ids(&["auth-b".to_string()]);

        assert!(cache.get("auth-a", Some("ws-a")).is_none());
        assert!(cache.get("auth-b", Some("ws-b")).is_some());
    }
}
