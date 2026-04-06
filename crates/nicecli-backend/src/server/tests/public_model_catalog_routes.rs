use super::*;

#[tokio::test]
async fn public_models_require_and_accept_configured_api_keys() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - key-a\n  - key-b\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let missing_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(missing_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_json(missing_response).await,
        json!({ "error": "Missing API key" })
    );

    let invalid_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("Authorization", "Bearer wrong-key")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(invalid_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_json(invalid_response).await,
        json!({ "error": "Invalid API key" })
    );

    let authorization_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("Authorization", "Bearer key-a")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(authorization_response.status(), StatusCode::OK);

    let goog_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("X-Goog-Api-Key", "key-a")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(goog_response.status(), StatusCode::OK);

    let api_key_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("X-Api-Key", "key-a")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(api_key_response.status(), StatusCode::OK);

    let query_key_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models?key=key-a")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(query_key_response.status(), StatusCode::OK);

    let auth_token_response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models?auth_token=key-a")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(auth_token_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn returns_public_root_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "message": "CLI Proxy API Server",
            "endpoints": [
                "POST /v1/chat/completions",
                "POST /v1/completions",
                "GET /v1/models"
            ]
        })
    );
}

#[tokio::test]
async fn returns_gemini_public_models_and_single_model() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ngemini-api-key:\n  - api-key: gem-1\n    models:\n      - name: gemini-2.5-pro-exp\n        version: 2026-04\n        inputTokenLimit: 1048576\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let list_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1beta/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(list_response).await,
        json!({
            "models": [
                {
                    "name": "models/gemini-2.5-pro-exp",
                    "version": "2026-04",
                    "displayName": "gemini-2.5-pro-exp",
                    "description": "gemini-2.5-pro-exp",
                    "inputTokenLimit": 1048576i64,
                    "supportedGenerationMethods": ["generateContent"]
                }
            ]
        })
    );

    let get_response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1beta/models/gemini-2.5-pro-exp")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(get_response).await,
        json!({
            "name": "models/gemini-2.5-pro-exp",
            "version": "2026-04",
            "displayName": "gemini-2.5-pro-exp",
            "inputTokenLimit": 1048576i64
        })
    );
}

#[tokio::test]
async fn returns_static_codex_public_models_from_api_key_prefix_and_exclusion() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\ncodex-api-key:\n  - api-key: codex-1\n    prefix: /lab/\n    excluded-models:\n      - gpt-5-codex-mini\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(ids.contains(&"lab/gpt-5"));
    assert!(!ids.contains(&"gpt-5"));
    assert!(!ids.contains(&"lab/gpt-5-codex-mini"));
}

#[tokio::test]
async fn returns_static_codex_public_models_from_auth_snapshot_with_alias_prefix_and_exclusion() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\noauth-model-alias:\n  codex:\n    - name: gpt-5-codex\n      alias: team-codex\noauth-excluded-models:\n  codex:\n    - gpt-5-codex-mini\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        r#"{
  "type": "codex",
  "provider": "codex",
  "email": "demo@example.com",
  "account_plan": "team",
  "prefix": " /lab/ "
}"#,
    )
    .expect("auth file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(ids.contains(&"lab/team-codex"));
    assert!(!ids.contains(&"lab/gpt-5-codex"));
    assert!(!ids.contains(&"lab/gpt-5-codex-mini"));
}

#[tokio::test]
async fn returns_static_gemini_public_models_from_auth_snapshot_without_explicit_models() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("gemini-demo@example.com.json"),
        r#"{
  "type": "gemini-cli",
  "provider": "gemini-cli",
  "email": "demo@example.com"
}"#,
    )
    .expect("auth file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1beta/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let names = payload["models"]
        .as_array()
        .expect("models array")
        .iter()
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(names.contains(&"models/gemini-2.5-pro"));
}

#[tokio::test]
async fn returns_static_gemini_public_models_from_antigravity_auth_snapshot() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("antigravity-demo@example.com.json"),
        r#"{
  "type": "antigravity",
  "provider": "antigravity",
  "email": "demo@example.com"
}"#,
    )
    .expect("auth file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1beta/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let names = payload["models"]
        .as_array()
        .expect("models array")
        .iter()
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(names.contains(&"models/gemini-2.5-flash"));
}
