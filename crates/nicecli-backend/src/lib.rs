mod contract;
mod server;

pub use nicecli_auth::{AuthFileEntry, AuthFileStoreError, PatchAuthFileFields};
use nicecli_config::{ConfigError, NiceCliConfig};
pub use nicecli_quota::{
    CodexQuotaSnapshotEnvelope, RateLimitSnapshot, RateLimitWindow, SnapshotListResponse,
};
use std::path::{Path, PathBuf};

pub use contract::{
    ContractRouteGroup, MANAGEMENT_ROUTE_GROUPS, OAUTH_CALLBACK_ROUTES, PUBLIC_API_ROUTES,
};
pub use server::{
    build_router, load_state_from_bootstrap, serve, serve_state_with_shutdown,
    start_model_catalog_refresh_task, BackendAppState, BackendServerError,
};

pub const MANAGEMENT_PREFIX: &str = "/v0/management";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSummary {
    pub public_route_count: usize,
    pub oauth_callback_route_count: usize,
    pub management_group_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendBootstrap {
    config_path: PathBuf,
    local_management_password: Option<String>,
}

impl BackendBootstrap {
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
            local_management_password: None,
        }
    }

    pub fn with_local_management_password(mut self, password: impl Into<String>) -> Self {
        let password = password.into();
        let trimmed = password.trim();
        if !trimmed.is_empty() {
            self.local_management_password = Some(trimmed.to_string());
        }
        self
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn local_management_password(&self) -> Option<&str> {
        self.local_management_password.as_deref()
    }

    pub fn load_config(&self) -> Result<NiceCliConfig, ConfigError> {
        NiceCliConfig::load_from_path(&self.config_path)
    }
}

pub fn contract_summary() -> ContractSummary {
    ContractSummary {
        public_route_count: PUBLIC_API_ROUTES.len(),
        oauth_callback_route_count: OAUTH_CALLBACK_ROUTES.len(),
        management_group_count: MANAGEMENT_ROUTE_GROUPS.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::{contract_summary, BackendBootstrap, MANAGEMENT_PREFIX};
    use std::path::PathBuf;

    #[test]
    fn contract_summary_is_non_empty() {
        let summary = contract_summary();
        assert!(summary.public_route_count >= 10);
        assert!(summary.oauth_callback_route_count >= 5);
        assert!(summary.management_group_count >= 5);
    }

    #[test]
    fn bootstrap_keeps_core_inputs() {
        let bootstrap = BackendBootstrap::new(PathBuf::from("config.yaml"))
            .with_local_management_password(" local-secret ");

        assert_eq!(
            bootstrap.config_path(),
            PathBuf::from("config.yaml").as_path()
        );
        assert_eq!(bootstrap.local_management_password(), Some("local-secret"));
        assert_eq!(MANAGEMENT_PREFIX, "/v0/management");
    }
}
