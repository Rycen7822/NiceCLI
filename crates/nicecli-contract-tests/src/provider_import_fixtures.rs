use crate::fixture_data::*;
use crate::normalization::{normalize_gemini_web_auth_file, normalize_vertex_auth_file};
use crate::test_support::{expected_json, load_fixture_state, request_json};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use nicecli_backend::build_router;
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn gemini_web_token_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let (status, payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .method("POST")
            .uri("/v0/management/gemini-web-token")
            .header("X-Management-Key", "secret")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{
                    "secure_1psid":"secure-psid-123",
                    "secure_1psidts":"secure-psidts-456",
                    "label":"gemini-web@example.com"
                }"#,
            ))
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let auth_file_name = "gemini-web-gemini-web@example.com.json";
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(auth_file_name)).expect("auth file"),
    )
    .expect("json");

    assert_eq!(
        json!({
            "response": {
                "status": payload["status"],
                "file": payload["file"],
                "email": payload["email"],
            },
            "auth_file": normalize_gemini_web_auth_file(auth_file_name, auth_payload),
        }),
        expected_json(GEMINI_WEB_TOKEN_EXPECTED)
    );
}

#[tokio::test]
async fn vertex_import_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let boundary = "nicecli-boundary";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"location\"\r\n\r\n\
asia-east1\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"service-account.json\"\r\n\
Content-Type: application/json\r\n\r\n\
{VERTEX_SERVICE_ACCOUNT_JSON}\r\n\
--{boundary}--\r\n"
    );

    let (status, payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .method("POST")
            .uri("/v0/management/vertex/import")
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
    let auth_file_name = "vertex-vertex-demo-project.json";
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(auth_file_name)).expect("auth file"),
    )
    .expect("json");

    assert_eq!(
        json!({
            "response": {
                "status": payload["status"],
                "file": payload["file"],
                "auth_file_ends_with_created": payload["auth-file"].as_str().map(|value| value.ends_with(auth_file_name)).unwrap_or(false),
                "project_id": payload["project_id"],
                "email": payload["email"],
                "location": payload["location"],
            },
            "auth_file": normalize_vertex_auth_file(auth_file_name, auth_payload),
        }),
        expected_json(VERTEX_IMPORT_EXPECTED)
    );
}
