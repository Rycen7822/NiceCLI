pub(crate) const AUTH_FILE_INPUT: &str =
    include_str!("../fixtures/auth-files/input/codex-demo@example.com-team.json");
pub(crate) const AUTH_FILE_EXPECTED: &str =
    include_str!("../fixtures/auth-files/expected/list-response.json");
pub(crate) const AUTH_FILE_PATCH_REQUEST: &str =
    include_str!("../fixtures/auth-files/input/patch-fields-request.json");
pub(crate) const AUTH_FILE_PATCH_RESPONSE: &str =
    include_str!("../fixtures/auth-files/expected/patch-fields-response.json");
pub(crate) const AUTH_FILE_PATCHED: &str =
    include_str!("../fixtures/auth-files/expected/patched-auth-file.json");
pub(crate) const AUTH_FILE_STATUS_REQUEST: &str =
    include_str!("../fixtures/auth-files/input/patch-status-request.json");
pub(crate) const AUTH_FILE_STATUS_RESPONSE: &str =
    include_str!("../fixtures/auth-files/expected/patch-status-response.json");
pub(crate) const AUTH_FILE_DISABLED: &str =
    include_str!("../fixtures/auth-files/expected/disabled-auth-file.json");
pub(crate) const AUTH_FILE_MODELS_RESPONSE: &str =
    include_str!("../fixtures/auth-files/expected/models-response.json");
pub(crate) const AUTH_FILE_UPLOAD_RESPONSE: &str =
    include_str!("../fixtures/auth-files/expected/upload-response.json");
pub(crate) const AUTH_FILE_DELETE_RESPONSE: &str =
    include_str!("../fixtures/auth-files/expected/delete-response.json");
pub(crate) const GEMINI_WEB_TOKEN_EXPECTED: &str =
    include_str!("../fixtures/auth-files/expected/gemini-web-token-response.json");
pub(crate) const VERTEX_IMPORT_EXPECTED: &str =
    include_str!("../fixtures/auth-files/expected/vertex-import-response.json");
pub(crate) const OAUTH_EXCLUDED_MODELS_EXPECTED: &str =
    include_str!("../fixtures/provider-config/expected/oauth-excluded-models-response.json");
pub(crate) const OAUTH_MODEL_ALIAS_EXPECTED: &str =
    include_str!("../fixtures/provider-config/expected/oauth-model-alias-response.json");
pub(crate) const CODEX_API_KEY_EXPECTED: &str =
    include_str!("../fixtures/provider-config/expected/codex-api-key-response.json");
pub(crate) const OPENAI_COMPATIBILITY_EXPECTED: &str =
    include_str!("../fixtures/provider-config/expected/openai-compatibility-response.json");
pub(crate) const VERTEX_SERVICE_ACCOUNT_JSON: &str = r#"{"type":"service_account","project_id":"vertex-demo-project","client_email":"vertex@example.com","private_key":"-----BEGIN RSA PRIVATE KEY-----\nMIIBOQIBAAJBALOTg53yXe1zGqKxwQrKI8TN2/ojnrN8UETSbaLgWr4KfSY0J2Ep\n4ENuPQn0a+1QFjmxEnJJAwj0DhpdM4hbAekCAwEAAQJANyWP/8tUaP02KsxnugaF\noYSOj2ys6fW9OMVegexgMYfGsEKdUSh9CJDpZx6azk+I/XJWaiDigkKMrM1iqxQD\neQIhAO2fVDCDehcoC56/Q7mxbnP0+Q/65naz198N58aWDeQbAiEAwXbu2hi1lVlh\n7o2/zvGX0RCm937zECwJcsCFNHbiaksCIA2W4xWzLzRL0f5OKe1gvFRdWjETxEJd\nnqbfALZWtoypAiAJlcnptlWwy5fliNJa/29FJW0QUBLc10G0lMNEwSsglwIgEuM2\nMCAJraczq4egzLA9pHORkkixHQ7AocxLhZjnVGA=\n-----END RSA PRIVATE KEY-----\n"}"#;
pub(crate) const QUOTA_AUTH_TEMPLATE: &str =
    include_str!("../fixtures/quota/input/codex-demo@example.com-team.json.template");
pub(crate) const QUOTA_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-response.json");
pub(crate) const QUOTA_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-failure-response.json");
pub(crate) const QUOTA_FILTERED_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-filtered-response.json");
pub(crate) const QUOTA_FILTERED_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-filtered-failure-response.json");
pub(crate) const QUOTA_UNKNOWN_AUTH_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-unknown-auth-response.json");
pub(crate) const QUOTA_UNKNOWN_WORKSPACE_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-unknown-workspace-response.json");
pub(crate) const QUOTA_REMOVED_AUTH_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/list-after-auth-removed-response.json");
pub(crate) const QUOTA_METADATA_SYNC_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/list-after-auth-note-change-response.json");
pub(crate) const QUOTA_METADATA_DRIFT_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/list-after-auth-metadata-change-response.json");
pub(crate) const QUOTA_WORKSPACE_LIST_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-workspace-list-failure-response.json");
pub(crate) const QUOTA_AUTH_LIST_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/list-after-auth-list-failure-response.json");
pub(crate) const QUOTA_INVALID_BODY_EXPECTED: &str =
    include_str!("../fixtures/quota/expected/refresh-invalid-body-response.json");
pub(crate) const BASELINE_QUOTA_WORKSPACE_LIST_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-workspace-list-failure.json");
pub(crate) const BASELINE_QUOTA_AUTH_LIST_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/list-after-auth-list-failure.json");
pub(crate) const BASELINE_QUOTA_UNKNOWN_WORKSPACE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-unknown-workspace.json");
pub(crate) const BASELINE_QUOTA_UNKNOWN_AUTH_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-unknown-auth.json");
pub(crate) const BASELINE_QUOTA_AUTH_REMOVED_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/list-after-auth-removed.json");
pub(crate) const BASELINE_QUOTA_AUTH_NOTE_CHANGE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/list-after-auth-note-change.json");
pub(crate) const BASELINE_QUOTA_AUTH_METADATA_CHANGE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/list-after-auth-metadata-change.json");
pub(crate) const BASELINE_QUOTA_REFRESH_FAILURE_PRESERVE_CACHE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-filtered-failure-preserve-cache.json");
pub(crate) const BASELINE_QUOTA_FILTERED_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-filtered.json");
pub(crate) const BASELINE_QUOTA_REFRESH_FAILURE_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-failure.json");
pub(crate) const BASELINE_QUOTA_REFRESH_SUCCESS_EXPECTED: &str =
    include_str!("../fixtures/quota/baseline/refresh-success.json");
pub(crate) const OAUTH_WAIT_EXPECTED: &str = include_str!("../fixtures/oauth/expected/wait.json");
pub(crate) const OAUTH_ERROR_EXPECTED: &str = include_str!("../fixtures/oauth/expected/error.json");
pub(crate) const OAUTH_INVALID_STATE_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/invalid-state.json");
pub(crate) const OAUTH_CALLBACK_REQUEST: &str =
    include_str!("../fixtures/oauth/input/callback-request.json");
pub(crate) const OAUTH_CALLBACK_RESPONSE: &str =
    include_str!("../fixtures/oauth/expected/callback-response.json");
pub(crate) const OAUTH_CALLBACK_FILE: &str =
    include_str!("../fixtures/oauth/expected/callback-file.json");
pub(crate) const OAUTH_CALLBACK_UNKNOWN_STATE_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/callback-unknown-state.json");
pub(crate) const OAUTH_CALLBACK_PROVIDER_MISMATCH_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/callback-provider-mismatch.json");
pub(crate) const OAUTH_CALLBACK_NOT_PENDING_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/callback-not-pending.json");
pub(crate) const OAUTH_CODEX_AUTH_URL_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/codex-auth-url.json");
pub(crate) const OAUTH_CODEX_ROUTE_FLOW_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/codex-route-flow.json");
pub(crate) const OAUTH_QWEN_AUTH_URL_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/qwen-auth-url.json");
pub(crate) const OAUTH_QWEN_ROUTE_FLOW_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/qwen-route-flow.json");
pub(crate) const OAUTH_ANTHROPIC_AUTH_URL_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/anthropic-auth-url.json");
pub(crate) const OAUTH_ANTHROPIC_ROUTE_FLOW_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/anthropic-route-flow.json");
pub(crate) const OAUTH_GEMINI_CLI_AUTH_URL_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/gemini-cli-auth-url.json");
pub(crate) const OAUTH_GEMINI_CLI_ROUTE_FLOW_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/gemini-cli-route-flow.json");
pub(crate) const OAUTH_ANTIGRAVITY_AUTH_URL_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/antigravity-auth-url.json");
pub(crate) const OAUTH_ANTIGRAVITY_ROUTE_FLOW_EXPECTED: &str =
    include_str!("../fixtures/oauth/expected/antigravity-route-flow.json");
