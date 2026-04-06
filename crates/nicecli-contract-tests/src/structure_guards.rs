use nicecli_backend::{
    contract_summary, MANAGEMENT_ROUTE_GROUPS, OAUTH_CALLBACK_ROUTES, PUBLIC_API_ROUTES,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const PUBLIC_CONTRACT_INVENTORY_EXPECTED: &str =
    include_str!("../fixtures/public/public-contract-inventory.json");

#[test]
fn public_routes_are_seeded() {
    assert!(PUBLIC_API_ROUTES.contains(&"POST /v1/chat/completions"));
    assert!(PUBLIC_API_ROUTES.contains(&"GET /v1beta/models"));
    assert!(OAUTH_CALLBACK_ROUTES.contains(&"GET /codex/callback"));
}

#[test]
fn public_contract_inventory_fixture_matches_rust_contract_lists() {
    assert_eq!(
        json!({
            "public_api_routes": PUBLIC_API_ROUTES,
            "oauth_callback_routes": OAUTH_CALLBACK_ROUTES,
        }),
        expected_json(PUBLIC_CONTRACT_INVENTORY_EXPECTED)
    );
}

fn public_baseline_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/public/baseline")
}

#[test]
fn public_baseline_fixture_inventory_exists() {
    let mut names: Vec<_> = fs::read_dir(public_baseline_dir())
        .expect("public baseline dir should exist")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    names.sort();

    assert_eq!(names.len(), 34);

    for expected in [
        "root-response.json",
        "v1-models-openai.json",
        "v1beta-models.json",
        "v1-responses-nonstream.json",
        "v1-chat-completions-response.json",
        "v1-completions-response.json",
        "v1-messages-response.json",
        "v1-messages-count-tokens-response.json",
        "v1-responses-websocket-create-created.json",
        "v1internal-generate-content.json",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing public baseline fixture {expected}"
        );
    }
}

#[test]
fn management_groups_cover_current_migration_priority() {
    let summary = contract_summary();
    assert!(summary.management_group_count >= 5);

    let has_auth_files = MANAGEMENT_ROUTE_GROUPS
        .iter()
        .any(|group| group.name == "auth-files");
    let has_quota = MANAGEMENT_ROUTE_GROUPS
        .iter()
        .any(|group| group.name == "quota");
    let has_oauth = MANAGEMENT_ROUTE_GROUPS
        .iter()
        .any(|group| group.name == "oauth-login");

    assert!(has_auth_files);
    assert!(has_quota);
    assert!(has_oauth);
}

#[test]
fn config_and_provider_contract_groups_keep_go_method_inventory() {
    let config_group = MANAGEMENT_ROUTE_GROUPS
        .iter()
        .find(|group| group.name == "config-basics")
        .expect("config-basics group should exist");
    let provider_group = MANAGEMENT_ROUTE_GROUPS
        .iter()
        .find(|group| group.name == "provider-config")
        .expect("provider-config group should exist");

    assert!(config_group.routes.contains(&"PATCH /v0/management/debug"));
    assert!(config_group
        .routes
        .contains(&"PATCH /v0/management/proxy-url"));
    assert!(config_group
        .routes
        .contains(&"GET /v0/management/quota-exceeded/switch-project"));
    assert!(config_group
        .routes
        .contains(&"PATCH /v0/management/request-log"));
    assert!(config_group
        .routes
        .contains(&"DELETE /v0/management/ampcode/upstream-url"));
    assert!(config_group
        .routes
        .contains(&"PATCH /v0/management/ampcode/upstream-api-key"));
    assert!(config_group
        .routes
        .contains(&"PATCH /v0/management/ampcode/restrict-management-to-localhost"));
    assert!(config_group
        .routes
        .contains(&"PATCH /v0/management/force-model-prefix"));

    assert!(provider_group
        .routes
        .contains(&"PATCH /v0/management/api-keys"));
    assert!(provider_group
        .routes
        .contains(&"DELETE /v0/management/gemini-api-key"));
    assert!(provider_group
        .routes
        .contains(&"PATCH /v0/management/openai-compatibility"));
    assert!(provider_group
        .routes
        .contains(&"DELETE /v0/management/oauth-model-alias"));
}

fn read_text(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path.as_ref())
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.as_ref().display()))
}

fn read_js_tree(path: impl AsRef<Path>) -> String {
    let mut combined = String::new();
    let mut entries: Vec<_> = fs::read_dir(path.as_ref())
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.as_ref().display()))
        .map(|entry| entry.expect("dir entry"))
        .collect();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            combined.push_str(&read_js_tree(&path));
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("js") {
            combined.push_str(&read_text(&path));
            combined.push('\n');
        }
    }

    combined
}

fn desktop_frontend_js_sources() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let app_root = manifest_dir.join("../../apps/nicecli");
    let mut combined = read_js_tree(app_root.join("js"));
    combined.push_str(&read_js_tree(app_root.join("dist-web/js")));
    combined
}

fn contains_route_reference(sources: &str, route: &str) -> bool {
    sources.contains(route) || sources.contains(route.trim_start_matches('/'))
}

#[test]
fn desktop_frontend_local_config_still_uses_tauri_yaml_commands() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_manager = read_text(manifest_dir.join("../../apps/nicecli/js/config-manager.js"));
    let settings_core = read_text(manifest_dir.join("../../apps/nicecli/js/settings-core.js"));

    assert!(config_manager.contains(r#"invoke("read_config_yaml")"#));
    assert!(config_manager.contains(r#""update_config_yaml""#));

    for route in [
        "/v0/management/config",
        "/v0/management/config.yaml",
        "/v0/management/debug",
        "/v0/management/proxy-url",
        "/v0/management/request-log",
        "/v0/management/request-retry",
        "/v0/management/quota-exceeded/switch-project",
        "/v0/management/api-keys",
        "/v0/management/claude-api-key",
        "/v0/management/codex-api-key",
        "/v0/management/openai-compatibility",
        "/v0/management/vertex-api-key",
    ] {
        assert!(
            !config_manager.contains(route),
            "config-manager.js should not hit {route} in local desktop mode"
        );
    }

    for endpoint_key in [
        r#"endpoint: "debug""#,
        r#"endpoint: "port""#,
        r#"endpoint: "proxy-url""#,
        r#"endpoint: "request-log""#,
        r#"endpoint: "request-retry""#,
        r#"endpoint: "quota-exceeded/switch-project""#,
        r#"endpoint: "quota-exceeded/switch-preview-model""#,
        r#"endpoint: "api-keys""#,
    ] {
        assert!(
            settings_core.contains(endpoint_key),
            "settings-core.js should continue editing YAML key {endpoint_key}"
        );
    }
}

#[test]
fn desktop_frontend_management_http_scope_matches_first_wave_migration_boundary() {
    let sources = desktop_frontend_js_sources();

    for route in [
        "/v0/management/auth-files",
        "/v0/management/auth-files/download",
        "/v0/management/auth-files/fields",
        "/v0/management/codex/quota-snapshots",
        "/v0/management/codex/quota-snapshots/refresh",
        "/v0/management/get-auth-status",
        "/v0/management/anthropic-auth-url",
        "/v0/management/codex-auth-url",
        "/v0/management/gemini-cli-auth-url",
        "/v0/management/antigravity-auth-url",
        "/v0/management/qwen-auth-url",
        "/v0/management/gemini-web-token",
        "/v0/management/vertex/import",
        "/codex/callback",
    ] {
        assert!(
            contains_route_reference(&sources, route),
            "desktop frontend should keep referencing {route}"
        );
    }

    for route in [
        "/v0/management/auth-files/models",
        "/v0/management/auth-files/status",
        "/v0/management/usage",
        "/v0/management/logs",
        "/v0/management/request-error-logs",
        "/v0/management/request-log-by-id",
        "/v0/management/model-definitions",
        "/v0/management/latest-version",
        "/v0/management/ampcode",
        "/v0/management/kimi-auth-url",
    ] {
        assert!(
            !contains_route_reference(&sources, route),
            "desktop frontend should not directly reference legacy management route {route}"
        );
    }
}

#[test]
fn desktop_frontend_auth_quota_refresh_chain_stays_dirty_flag_driven() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let auth_files = read_text(manifest_dir.join("../../apps/nicecli/js/settings-auth-files.js"));
    let workspace_quota =
        read_text(manifest_dir.join("../../apps/nicecli/js/settings-workspace-quota.js"));
    let settings_tabs = read_text(manifest_dir.join("../../apps/nicecli/js/settings-tabs.js"));

    assert!(
        auth_files.contains("window.__nicecliWorkspaceQuotaDirty = true;"),
        "settings-auth-files.js should keep marking workspace quota dirty after auth changes"
    );
    assert!(
        auth_files.contains("window.invalidateWorkspaceQuotaSnapshotsCache();"),
        "settings-auth-files.js should keep invalidating the workspace quota cache helper"
    );
    assert!(
        auth_files.contains("invalidateWorkspaceQuotaViews();"),
        "auth refresh path should keep invalidating workspace quota views"
    );
    assert!(
        auth_files.contains("await loadAuthFiles();"),
        "auth refresh path should keep reloading the auth file list"
    );
    assert!(
        auth_files
            .matches("await refreshAuthDependentViews();")
            .count()
            >= 4,
        "auth mutations should continue to funnel through refreshAuthDependentViews()"
    );

    assert!(
        workspace_quota.contains("window.__nicecliWorkspaceQuotaDirty === true"),
        "workspace quota loader should keep honoring the dirty flag"
    );
    assert!(
        workspace_quota.contains("!workspaceQuotaLoadedOnce"),
        "workspace quota loader should keep forcing the first fetch"
    );
    assert!(
        workspace_quota.contains("configManager.getCodexQuotaSnapshots(shouldRefresh)"),
        "workspace quota loader should pass through the computed refresh flag"
    );
    assert!(
        workspace_quota.contains("configManager.getAuthFiles()"),
        "workspace quota loader should keep rereading auth files alongside quota snapshots"
    );
    assert!(
        workspace_quota.contains("window.__nicecliWorkspaceQuotaDirty = false;"),
        "workspace quota loader should clear the dirty flag after a successful refresh"
    );

    assert!(
        settings_tabs.contains(r#"if (tabId === "workspace-quota") {"#)
            && settings_tabs.contains("await loadWorkspaceQuotaSnapshots();"),
        "workspace quota tab activation should keep triggering a quota load"
    );
}

#[test]
fn desktop_frontend_workspace_quota_filters_and_labels_stay_metadata_driven() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_quota =
        read_text(manifest_dir.join("../../apps/nicecli/js/settings-workspace-quota.js"));

    assert!(
        workspace_quota.contains("normalizeWorkspaceQuotaText(snapshot?.account_email)")
            && workspace_quota.contains("extractWorkspaceQuotaEmail(snapshot?.auth_id)")
            && workspace_quota.contains("extractWorkspaceQuotaEmail(snapshot?.auth_label)"),
        "workspace quota account grouping should keep resolving account identity from email first"
    );
    assert!(
        workspace_quota.contains("return `email:${normalizeWorkspaceQuotaLookupKey(email)}`;"),
        "workspace quota account filter key should stay email-based when an email is available"
    );
    assert!(
        workspace_quota.contains(
            "const workspaceName = normalizeWorkspaceQuotaText(snapshot?.workspace_name);"
        ) && workspace_quota.contains(
            "if (workspaceName && !isWorkspaceQuotaGenericWorkspaceValue(workspaceName)) {"
        ) && workspace_quota.contains("return workspaceName;"),
        "workspace quota cards should keep preferring explicit workspace names over generic values"
    );
    assert!(
        workspace_quota.contains("authNote &&")
            && workspace_quota.contains("workspaceId &&")
            && workspace_quota.contains("return `${authNote}（${workspaceId}）`;"),
        "workspace quota workspace filter should keep distinguishing renamed workspaces by note and workspace id"
    );
    assert!(
        workspace_quota.contains("normalizeWorkspaceQuotaPlanTier(snapshot?.account_plan)")
            && workspace_quota
                .contains("extractWorkspaceQuotaPlanFromFilename(snapshot?.auth_file_name)")
            && workspace_quota
                .contains("normalizeWorkspaceQuotaPlanTier(snapshot?.snapshot?.plan_type)"),
        "workspace quota plan label should keep deriving from auth metadata before falling back to snapshot plan type"
    );
    assert!(
        workspace_quota.contains("查看最新的 Codex quota 获取时间")
            && workspace_quota.contains("getWorkspaceQuotaLatestFetchedAt()")
            && !workspace_quota.contains("来源 usage_dashboard"),
        "workspace quota description should keep showing the latest fetched time instead of the raw source label"
    );
    assert!(
        !workspace_quota.contains("workspace-quota-card-meta")
            && !workspace_quota.contains("snapshot.source")
            && !workspace_quota.contains("snapshot.fetched_at"),
        "workspace quota cards should not reintroduce per-card source or fetched-at footers"
    );
}

#[test]
fn rust_refresh_chain_still_rereads_auth_state_on_demand() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend_management_auth =
        read_text(manifest_dir.join("../../crates/nicecli-backend/src/server/management_auth.rs"));
    let quota_service = read_text(manifest_dir.join("../../crates/nicecli-quota/src/service.rs"));

    assert!(
        backend_management_auth.contains("FileAuthStore::new(&state.auth_dir)")
            && backend_management_auth.contains("list_snapshots()")
            && backend_management_auth.contains("find_snapshot(auth_name)"),
        "backend auth-files flows should keep rereading directly from the runtime auth store boundary"
    );
    assert!(
        quota_service.matches("list_codex_auths()").count() >= 2,
        "quota service should keep rereading auth state for both list and refresh paths"
    );
}

#[test]
fn rust_log_chain_stays_in_dedicated_management_logs_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend_management_logs =
        read_text(manifest_dir.join("../../crates/nicecli-backend/src/server/management_logs.rs"));

    assert!(
        backend_management_logs.contains("resolve_log_directory(&state)")
            && backend_management_logs.contains("collect_log_files(&log_dir)")
            && backend_management_logs.contains("get_config_bool_value(&state, \"request-log\")")
            && backend_management_logs.contains("attachment_response("),
        "backend log management flows should stay grouped in the dedicated logs module"
    );
}

#[test]
fn rust_config_chain_stays_in_dedicated_management_config_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend_management_config = read_text(
        manifest_dir.join("../../crates/nicecli-backend/src/server/management_config.rs"),
    );

    assert!(
        backend_management_config.contains("NiceCliConfig::from_yaml_str")
            && backend_management_config.contains(
                "put_string_list_config_field_response(&state, &headers, body, \"api-keys\")"
            )
            && backend_management_config.contains("RoutingStrategy::parse(value)")
            && backend_management_config.contains("empty_usage_statistics_snapshot()"),
        "backend config management flows should stay grouped in the dedicated config module"
    );
}

#[test]
fn rust_provider_config_chain_stays_in_dedicated_management_provider_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend_management_provider = read_text(
        manifest_dir.join("../../crates/nicecli-backend/src/server/management_provider_config.rs"),
    );

    assert!(
        backend_management_provider.contains("normalize_oauth_excluded_models_map(parsed)")
            && backend_management_provider.contains("normalize_oauth_model_alias_entries(request.aliases)")
            && backend_management_provider.contains("load_oauth_model_alias_map(&state)")
            && backend_management_provider.contains("persist_top_level_config_value("),
        "backend provider-config management flows should stay grouped in the dedicated provider-config module"
    );
}

#[test]
fn rust_google_public_transport_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_public_transport = read_text(
        manifest_dir
            .join("../../crates/nicecli-backend/src/server/google_provider/public_transport.rs"),
    );

    assert!(
        google_public_transport.contains("build_public_gemini_url")
            && google_public_transport.contains("execute_public_vertex_http_request")
            && google_public_transport.contains("patch_gemini_public_request_body")
            && google_public_transport.contains("pending_provider_stream_from_reqwest"),
        "google public transport helpers should stay grouped in the dedicated transport submodule"
    );
}

#[test]
fn rust_google_internal_auth_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_internal_auth = read_text(
        manifest_dir
            .join("../../crates/nicecli-backend/src/server/google_provider/internal_auth.rs"),
    );

    assert!(
        google_internal_auth.contains("load_gemini_internal_credentials")
            && google_internal_auth.contains("refresh_gemini_internal_access_token")
            && google_internal_auth.contains("build_gemini_internal_http_client")
            && google_internal_auth.contains("patch_gemini_internal_request_body"),
        "google internal auth and token helpers should stay grouped in the dedicated auth submodule"
    );
}

#[test]
fn rust_google_auth_flows_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_auth_flows = read_text(
        manifest_dir.join("../../crates/nicecli-backend/src/server/google_provider/auth_flows.rs"),
    );

    assert!(
        google_auth_flows.contains("handle_google_callback")
            && google_auth_flows.contains("get_gemini_cli_auth_url")
            && google_auth_flows.contains("save_gemini_web_tokens")
            && google_auth_flows.contains("import_vertex_credential")
            && google_auth_flows.contains("gemini_web_token_error_response")
            && google_auth_flows.contains("vertex_credential_import_error_response"),
        "google auth/login callback, gemini web-token, and vertex import flows should stay grouped in the dedicated auth-flows submodule"
    );
}

#[test]
fn rust_google_api_key_management_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_api_keys = read_text(
        manifest_dir.join("../../crates/nicecli-backend/src/server/google_provider/api_keys.rs"),
    );

    assert!(
        google_api_keys.contains("get_gemini_api_keys")
            && google_api_keys.contains("patch_vertex_api_keys")
            && google_api_keys.contains("gemini_api_key_entries_from_config_json")
            && google_api_keys.contains("resolve_vertex_api_key_entry_model"),
        "google api-key config, normalization, and route handlers should stay grouped in the dedicated api-keys submodule"
    );
}

#[test]
fn rust_google_public_actions_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_public_actions = read_text(
        manifest_dir
            .join("../../crates/nicecli-backend/src/server/google_provider/public_actions.rs"),
    );
    let google_public_action_auth_files = read_text(manifest_dir.join(
        "../../crates/nicecli-backend/src/server/google_provider/public_action_auth_files.rs",
    ));
    let google_public_action_requests =
        read_text(manifest_dir.join(
            "../../crates/nicecli-backend/src/server/google_provider/public_action_requests.rs",
        ));

    assert!(
        google_public_actions.contains("execute_public_gemini_model_action")
            && google_public_actions.contains("try_execute_public_vertex_api_key_entries_stream_request")
            && google_public_action_auth_files.contains("try_execute_public_gemini_auth_request")
            && google_public_action_auth_files.contains("try_execute_public_antigravity_auth_request")
            && google_public_action_auth_files.contains("try_execute_public_vertex_auth_stream_request")
            && google_public_action_requests.contains("parse_gemini_public_action")
            && google_public_action_requests.contains("read_public_gemini_request_body")
            && google_public_action_requests.contains("requested_public_gemini_model_candidates")
            && google_public_action_requests.contains("patch_public_gemini_request_for_antigravity"),
        "google public action orchestration, auth-file execution, and request parsing should stay grouped in dedicated submodules"
    );
}

#[test]
fn rust_google_model_catalog_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_model_catalog = read_text(
        manifest_dir
            .join("../../crates/nicecli-backend/src/server/google_provider/model_catalog.rs"),
    );

    assert!(
        google_model_catalog.contains("collect_public_gemini_models")
            && google_model_catalog.contains("find_public_gemini_model")
            && google_model_catalog.contains("collect_public_gemini_model_infos")
            && google_model_catalog.contains("gemini_public_detail_payload_from_model_info"),
        "google public model listing and payload shaping should stay grouped in the dedicated model-catalog submodule"
    );
}

#[test]
fn rust_google_response_helpers_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_response_helpers = read_text(
        manifest_dir
            .join("../../crates/nicecli-backend/src/server/google_provider/response_helpers.rs"),
    );

    assert!(
        google_response_helpers.contains("provider_stream_response")
            && google_response_helpers.contains("gemini_internal_error_response")
            && google_response_helpers.contains("gemini_public_runtime_error_response")
            && google_response_helpers.contains("antigravity_public_error_response")
            && google_response_helpers.contains("upstream_json_error_response"),
        "google provider error mapping and response helpers should stay grouped in the dedicated response-helpers submodule"
    );
}

#[test]
fn rust_google_internal_methods_chain_stays_in_dedicated_submodule() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let google_internal_methods = read_text(
        manifest_dir
            .join("../../crates/nicecli-backend/src/server/google_provider/internal_methods.rs"),
    );

    assert!(
        google_internal_methods.contains("handle_v1internal_method")
            && google_internal_methods.contains("execute_gemini_internal_generate_content")
            && google_internal_methods.contains("pass_through_v1internal_request")
            && google_internal_methods.contains("read_json_request_body"),
        "google v1internal route and request parsing should stay grouped in the dedicated internal-methods submodule"
    );
}

fn expected_json(raw: &str) -> Value {
    serde_json::from_str(raw).expect("valid json fixture")
}
