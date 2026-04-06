use crate::fixture_data::*;
use crate::test_support::{expected_json, request_json};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use nicecli_backend::{build_router, load_state_from_bootstrap, BackendBootstrap};
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn oauth_excluded_models_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    let config_body = format!(
        "host: 127.0.0.1\nport: 8317\nauth-dir: {}\noauth-excluded-models:\n  Codex:\n    - GPT-5\n    - gpt-5\n",
        auth_dir.to_string_lossy().replace('\\', "/")
    );
    fs::write(&config_path, config_body).expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/oauth-excluded-models")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(OAUTH_EXCLUDED_MODELS_EXPECTED));
}

#[tokio::test]
async fn oauth_model_alias_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    let config_body = format!(
        "host: 127.0.0.1\nport: 8317\nauth-dir: {}\noauth-model-alias:\n  Codex:\n    - name: gpt-5\n      alias: demo-gpt5\n    - name: gpt-5\n      alias: DEMO-GPT5\n",
        auth_dir.to_string_lossy().replace('\\', "/")
    );
    fs::write(&config_path, config_body).expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/oauth-model-alias")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(OAUTH_MODEL_ALIAS_EXPECTED));
}

#[tokio::test]
async fn codex_api_key_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    let config_body = format!(
        "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ncodex-api-key:\n  - api-key: \" codex-1 \"\n    prefix: \"/team/\"\n    base-url: \" https://codex.local \"\n    websockets: true\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    models:\n      - name: \" gpt-5 \"\n        alias: \" demo-gpt5 \"\n      - name: \"\"\n        alias: \"\"\n    excluded-models:\n      - \" GPT-5 \"\n      - gpt-5\n",
        auth_dir.to_string_lossy().replace('\\', "/")
    );
    fs::write(&config_path, config_body).expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex-api-key")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(CODEX_API_KEY_EXPECTED));
}

#[tokio::test]
async fn openai_compatibility_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    let config_body = format!(
        "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nopenai-compatibility:\n  - name: \" demo \"\n    prefix: \"/team/\"\n    base-url: \" https://openai.local \"\n    api-key-entries:\n      - api-key: \" openai-1 \"\n        proxy-url: \" http://127.0.0.1:7890 \"\n      - api-key: \"\"\n        proxy-url: http://127.0.0.1:7891\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    models:\n      - name: \" gpt-5 \"\n        alias: \" demo-gpt5 \"\n      - name: \"\"\n        alias: \"\"\n",
        auth_dir.to_string_lossy().replace('\\', "/")
    );
    fs::write(&config_path, config_body).expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/openai-compatibility")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(payload, expected_json(OPENAI_COMPATIBILITY_EXPECTED));
}
