use crate::fixture_data::*;
use crate::test_support::{expected_json, load_fixture_state, request_json};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use nicecli_backend::build_router;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn oauth_status_wait_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-state-01", "codex")
        .expect("register session");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/get-auth-status?state=codex-state-01")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(OAUTH_WAIT_EXPECTED));
}

#[tokio::test]
async fn oauth_status_error_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-state-02", "codex")
        .expect("register session");
    state
        .oauth_sessions
        .set_error("codex-state-02", " Timeout waiting ")
        .expect("set error");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/get-auth-status?state=codex-state-02")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(OAUTH_ERROR_EXPECTED));
}

#[tokio::test]
async fn oauth_status_invalid_state_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/get-auth-status?state=bad/state")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload, expected_json(OAUTH_INVALID_STATE_EXPECTED));
}

#[tokio::test]
async fn oauth_callback_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-callback-01", "codex")
        .expect("register session");

    let (status, payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .method("POST")
            .uri("/v0/management/oauth-callback")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(OAUTH_CALLBACK_REQUEST))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(OAUTH_CALLBACK_RESPONSE));

    let callback_path = state.auth_dir.join(".oauth-codex-codex-callback-01.oauth");
    let callback_payload: Value =
        serde_json::from_str(&fs::read_to_string(callback_path).expect("callback file"))
            .expect("callback json");
    assert_eq!(callback_payload, expected_json(OAUTH_CALLBACK_FILE));
}

#[tokio::test]
async fn oauth_callback_unknown_state_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("POST")
            .uri("/v0/management/oauth-callback")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"provider":"codex","state":"codex-missing-01","code":"code-123"}"#,
            ))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        payload,
        expected_json(OAUTH_CALLBACK_UNKNOWN_STATE_EXPECTED)
    );
}

#[tokio::test]
async fn oauth_callback_provider_mismatch_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-callback-02", "codex")
        .expect("register session");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("POST")
            .uri("/v0/management/oauth-callback")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"provider":"anthropic","state":"codex-callback-02","code":"code-123"}"#,
            ))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        payload,
        expected_json(OAUTH_CALLBACK_PROVIDER_MISMATCH_EXPECTED)
    );
}

#[tokio::test]
async fn oauth_callback_not_pending_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-callback-03", "codex")
        .expect("register session");
    state
        .oauth_sessions
        .set_error("codex-callback-03", "failed")
        .expect("set error");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .method("POST")
            .uri("/v0/management/oauth-callback")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"provider":"codex","state":"codex-callback-03","code":"code-123"}"#,
            ))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(payload, expected_json(OAUTH_CALLBACK_NOT_PENDING_EXPECTED));
}
