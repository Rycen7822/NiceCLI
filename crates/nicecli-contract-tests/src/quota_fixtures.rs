use crate::fixture_data::*;
use crate::normalization::{
    normalize_quota_filtered_response, normalize_quota_go_common_response,
    normalize_quota_metadata_response, normalize_quota_response,
};
use crate::test_support::{
    build_jwt, expected_json, load_fixture_state, request_json, spawn_usage_server,
    spawn_usage_server_with_response, spawn_workspace_usage_server, FailingWorkspaceListSource,
    FlakyAuthEnumerator, StaticAuthEnumerator, StaticWorkspaceSource,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use nicecli_backend::build_router;
use nicecli_quota::{CodexAuthContext, CodexQuotaService};
use serde_json::json;
use std::fs;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn quota_refresh_fixture_matches_rust_response() {
    let usage_server = spawn_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_payload = QUOTA_AUTH_TEMPLATE.replace("__USAGE_SERVER__", &usage_server.to_string());
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        auth_payload,
    )
    .expect("quota auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_response(payload.clone()),
        expected_json(QUOTA_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_REFRESH_SUCCESS_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_filtered_fixture_matches_rust_response() {
    let usage_server = spawn_workspace_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let auth_a_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );
    let auth_b_claims = build_jwt(
        r#"{
            "email": "other@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_secondary",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_secondary", "title": "Workspace Shared", "is_default": true }
                ]
            }
        }"#,
    );

    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_a_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");
    fs::write(
        state.auth_dir.join("codex-other@example.com-team.json"),
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-b",
                "label": "Secondary",
                "metadata": {{
                    "email": "other@example.com",
                    "access_token": "token-456",
                    "id_token": "{auth_b_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_filtered_response(payload.clone()),
        expected_json(QUOTA_FILTERED_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_FILTERED_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_post_body_filtered_fixture_matches_rust_response() {
    let usage_server = spawn_workspace_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let auth_a_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );
    let auth_b_claims = build_jwt(
        r#"{
            "email": "other@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_secondary",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_secondary", "title": "Workspace Shared", "is_default": true }
                ]
            }
        }"#,
    );

    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_a_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");
    fs::write(
        state.auth_dir.join("codex-other@example.com-team.json"),
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-b",
                "label": "Secondary",
                "metadata": {{
                    "email": "other@example.com",
                    "access_token": "token-456",
                    "id_token": "{auth_b_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("POST")
            .uri("/v0/management/codex/quota-snapshots/refresh")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"auth_id":"auth-a","workspace_id":"org_secondary"}"#,
            ))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_filtered_response(payload.clone()),
        expected_json(QUOTA_FILTERED_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_FILTERED_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_post_empty_body_fixture_matches_rust_response() {
    let usage_server = spawn_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_payload = QUOTA_AUTH_TEMPLATE.replace("__USAGE_SERVER__", &usage_server.to_string());
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        auth_payload,
    )
    .expect("quota auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("POST")
            .uri("/v0/management/codex/quota-snapshots/refresh")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_response(payload),
        expected_json(QUOTA_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_post_invalid_body_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("POST")
            .uri("/v0/management/codex/quota-snapshots/refresh")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from("{not-json"))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload, expected_json(QUOTA_INVALID_BODY_EXPECTED));
}

#[tokio::test]
async fn quota_refresh_failure_preserves_cached_workspace_fixture_matches_rust_response() {
    let success_server = spawn_workspace_usage_server().await;
    let failure_server = spawn_usage_server_with_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "message": "server down" }),
    )
    .await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_path = state.auth_dir.join("codex-demo@example.com-team.json");

    let auth_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );

    fs::write(
        &auth_path,
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{success_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (first_status, _) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);

    fs::write(
        &auth_path,
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{failure_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_filtered_response(payload.clone()),
        expected_json(QUOTA_FILTERED_FAILURE_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_REFRESH_FAILURE_PRESERVE_CACHE_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_unknown_auth_fixture_matches_rust_response() {
    let usage_server = spawn_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_payload = QUOTA_AUTH_TEMPLATE.replace("__USAGE_SERVER__", &usage_server.to_string());
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        auth_payload,
    )
    .expect("quota auth fixture");

    let (warm_status, _) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(warm_status, StatusCode::OK);

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-missing")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(QUOTA_UNKNOWN_AUTH_EXPECTED));
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_UNKNOWN_AUTH_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_unknown_workspace_fixture_matches_rust_response() {
    let usage_server = spawn_workspace_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let auth_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );

    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_missing")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_filtered_response(payload.clone()),
        expected_json(QUOTA_UNKNOWN_WORKSPACE_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_UNKNOWN_WORKSPACE_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_workspace_list_failure_without_workspace_id_uses_current_workspace_fixture()
{
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    let auth_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );
    state.quota_service = Arc::new(CodexQuotaService::with_deps(
        Arc::new(StaticAuthEnumerator {
            auths: vec![CodexAuthContext {
                auth_id: "auth-a".to_string(),
                auth_label: "Primary".to_string(),
                auth_note: "Workspace Alpha".to_string(),
                auth_file_name: "codex-demo@example.com-team.json".to_string(),
                account_email: "demo@example.com".to_string(),
                account_plan: "team".to_string(),
                account_id: "org_default".to_string(),
                cookies: Default::default(),
                access_token: "token-123".to_string(),
                refresh_token: String::new(),
                id_token: auth_claims,
                base_url: "https://chatgpt.com/backend-api".to_string(),
                proxy_url: String::new(),
            }],
        }),
        Arc::new(FailingWorkspaceListSource),
    ));

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_filtered_response(payload.clone()),
        expected_json(QUOTA_WORKSPACE_LIST_FAILURE_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_WORKSPACE_LIST_FAILURE_EXPECTED)
    );
}

#[tokio::test]
async fn quota_list_after_auth_list_failure_keeps_cached_snapshot_fixture() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    let auth_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_secondary",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": false },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": true }
                ]
            }
        }"#,
    );
    state.quota_service = Arc::new(CodexQuotaService::with_deps(
        Arc::new(FlakyAuthEnumerator {
            auths: vec![CodexAuthContext {
                auth_id: "auth-a".to_string(),
                auth_label: "Primary".to_string(),
                auth_note: "Workspace Beta".to_string(),
                auth_file_name: "codex-demo@example.com-team.json".to_string(),
                account_email: "demo@example.com".to_string(),
                account_plan: "team".to_string(),
                account_id: "org_secondary".to_string(),
                cookies: Default::default(),
                access_token: "token-123".to_string(),
                refresh_token: String::new(),
                id_token: auth_claims,
                base_url: "https://chatgpt.com/backend-api".to_string(),
                proxy_url: String::new(),
            }],
            calls: Arc::new(AtomicUsize::new(0)),
        }),
        Arc::new(StaticWorkspaceSource),
    ));

    let (refresh_status, _) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(refresh_status, StatusCode::OK);

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_filtered_response(payload.clone()),
        expected_json(QUOTA_AUTH_LIST_FAILURE_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_AUTH_LIST_FAILURE_EXPECTED)
    );
}

#[tokio::test]
async fn quota_list_after_auth_removed_fixture_matches_rust_response() {
    let usage_server = spawn_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_path = state.auth_dir.join("codex-demo@example.com-team.json");
    let auth_payload = QUOTA_AUTH_TEMPLATE.replace("__USAGE_SERVER__", &usage_server.to_string());
    fs::write(&auth_path, auth_payload).expect("quota auth fixture");

    let (warm_status, _) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(warm_status, StatusCode::OK);

    fs::remove_file(&auth_path).expect("remove auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(QUOTA_REMOVED_AUTH_EXPECTED));
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_AUTH_REMOVED_EXPECTED)
    );
}

#[tokio::test]
async fn quota_list_after_auth_note_change_fixture_matches_rust_response() {
    let usage_server = spawn_workspace_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_path = state.auth_dir.join("codex-demo@example.com-team.json");
    let auth_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );

    fs::write(
        &auth_path,
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "note": "Old Workspace Note",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (warm_status, _) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(warm_status, StatusCode::OK);

    fs::write(
        &auth_path,
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "note": "Renamed Workspace Note",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_metadata_response(payload.clone()),
        expected_json(QUOTA_METADATA_SYNC_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_AUTH_NOTE_CHANGE_EXPECTED)
    );
}

#[tokio::test]
async fn quota_list_after_auth_metadata_change_fixture_matches_rust_response() {
    let usage_server = spawn_workspace_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let original_auth_path = state.auth_dir.join("codex-demo@example.com-team.json");
    let renamed_auth_path = state.auth_dir.join("codex-renamed@example.com-pro.json");
    let auth_claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace Alpha", "is_default": true },
                    { "id": "org_secondary", "title": "Workspace Beta", "is_default": false }
                ]
            }
        }"#,
    );

    fs::write(
        &original_auth_path,
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Primary",
                "note": "Workspace Beta Note",
                "metadata": {{
                    "email": "demo@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("auth fixture");

    let (warm_status, _) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(warm_status, StatusCode::OK);

    fs::remove_file(&original_auth_path).expect("remove original auth fixture");
    fs::write(
        &renamed_auth_path,
        format!(
            r#"{{
                "provider": "codex",
                "id": "auth-a",
                "label": "Renamed Primary",
                "note": "Workspace Beta Note",
                "metadata": {{
                    "email": "renamed@example.com",
                    "access_token": "token-123",
                    "id_token": "{auth_claims}"
                }},
                "attributes": {{
                    "base_url": "http://{usage_server}/backend-api"
                }}
            }}"#
        ),
    )
    .expect("renamed auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?auth_id=auth-a&workspace_id=org_secondary")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_metadata_response(payload.clone()),
        expected_json(QUOTA_METADATA_DRIFT_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_AUTH_METADATA_CHANGE_EXPECTED)
    );
}

#[tokio::test]
async fn quota_refresh_failure_fixture_matches_rust_response() {
    let usage_server = spawn_usage_server_with_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "message": "server down" }),
    )
    .await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_payload = QUOTA_AUTH_TEMPLATE.replace("__USAGE_SERVER__", &usage_server.to_string());
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        auth_payload,
    )
    .expect("quota auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex/quota-snapshots?refresh=1")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_quota_response(payload.clone()),
        expected_json(QUOTA_FAILURE_EXPECTED)
    );
    assert_eq!(
        normalize_quota_go_common_response(payload),
        expected_json(BASELINE_QUOTA_REFRESH_FAILURE_EXPECTED)
    );
}
