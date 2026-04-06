use crate::fixture_data::*;
use crate::normalization::normalize_auth_files_response;
use crate::test_support::{expected_json, load_fixture_state, request_json};
use axum::body::{to_bytes, Body};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use nicecli_backend::{build_router, load_state_from_bootstrap, BackendBootstrap};
use serde_json::Value;
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn auth_files_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        AUTH_FILE_INPUT,
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/auth-files")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_auth_files_response(payload),
        expected_json(AUTH_FILE_EXPECTED)
    );
}

#[tokio::test]
async fn auth_file_patch_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_path = state.auth_dir.join("codex-demo@example.com-team.json");
    fs::write(&auth_path, AUTH_FILE_INPUT).expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("PATCH")
            .uri("/v0/management/auth-files/fields")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(AUTH_FILE_PATCH_REQUEST))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(AUTH_FILE_PATCH_RESPONSE));

    let updated: Value =
        serde_json::from_str(&fs::read_to_string(auth_path).expect("patched auth file"))
            .expect("patched auth json");
    assert_eq!(updated, expected_json(AUTH_FILE_PATCHED));
}

#[tokio::test]
async fn auth_file_status_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_path = state.auth_dir.join("codex-demo@example.com-team.json");
    fs::write(&auth_path, AUTH_FILE_INPUT).expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("PATCH")
            .uri("/v0/management/auth-files/status")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(AUTH_FILE_STATUS_REQUEST))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(AUTH_FILE_STATUS_RESPONSE));

    let updated: Value =
        serde_json::from_str(&fs::read_to_string(auth_path).expect("patched auth file"))
            .expect("patched auth json");
    assert_eq!(updated, expected_json(AUTH_FILE_DISABLED));
}

#[tokio::test]
async fn auth_file_models_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    let config_body = format!(
        "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ncodex-api-key:\n  - api-key: codex-1\n    models:\n      - name: gpt-5\n        display_name: GPT-5\n        type: chat\n        owned_by: openai\n      - alias: gpt-5\n        display_name: Duplicate GPT-5\n",
        auth_dir.to_string_lossy().replace('\\', "/")
    );
    fs::write(&config_path, config_body).expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        AUTH_FILE_INPUT,
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/auth-files/models?name=codex-demo@example.com-team.json")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(AUTH_FILE_MODELS_RESPONSE));
}

#[tokio::test]
async fn auth_file_upload_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let boundary = "nicecli-auth-upload";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"codex-demo@example.com-team.json\"\r\n\
Content-Type: application/json\r\n\r\n\
{AUTH_FILE_INPUT}\r\n\
--{boundary}--\r\n"
    );

    let (status, payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .method("POST")
            .uri("/v0/management/auth-files")
            .header("X-Management-Key", "secret")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(AUTH_FILE_UPLOAD_RESPONSE));

    let saved = fs::read_to_string(state.auth_dir.join("codex-demo@example.com-team.json"))
        .expect("saved auth file");
    let saved_json: Value = serde_json::from_str(&saved).expect("saved auth json");
    let expected_saved: Value = serde_json::from_str(AUTH_FILE_INPUT).expect("fixture json");
    assert_eq!(saved_json, expected_saved);

    let (list_status, list_payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/auth-files")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(
        normalize_auth_files_response(list_payload),
        expected_json(AUTH_FILE_EXPECTED)
    );
}

#[tokio::test]
async fn auth_file_download_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        AUTH_FILE_INPUT,
    )
    .expect("auth fixture");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files/download?name=codex-demo@example.com-team.json")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(
        response
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok()),
        Some("attachment; filename=\"codex-demo@example.com-team.json\"")
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let actual: Value = serde_json::from_slice(&body).expect("downloaded json");
    let expected: Value = serde_json::from_str(AUTH_FILE_INPUT).expect("fixture json");
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn auth_file_delete_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        AUTH_FILE_INPUT,
    )
    .expect("auth fixture");

    let (status, payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .method("DELETE")
            .uri("/v0/management/auth-files?name=codex-demo@example.com-team.json")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(AUTH_FILE_DELETE_RESPONSE));
    assert!(!state
        .auth_dir
        .join("codex-demo@example.com-team.json")
        .exists());

    let (list_status, list_payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/auth-files")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_payload, serde_json::json!({ "files": [] }));
}
