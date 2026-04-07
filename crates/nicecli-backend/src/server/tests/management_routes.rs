use super::*;

#[tokio::test]
async fn lists_auth_files_from_disk() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        r#"{"type":"codex","email":"demo@example.com","note":"Workspace A","priority":7}"#,
    )
    .expect("auth file");

    let router = build_router(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    let files: Vec<AuthFileEntry> =
        serde_json::from_value(payload["files"].clone()).expect("files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].provider, "codex");
    assert!(!files[0].disabled);
    assert_eq!(files[0].status, "active");
    assert_eq!(files[0].status_message, None);
    assert_eq!(files[0].email.as_deref(), Some("demo@example.com"));
    assert_eq!(files[0].note.as_deref(), Some("Workspace A"));
    assert_eq!(files[0].priority, Some(7));
}

#[tokio::test]
async fn rejects_missing_management_key() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let router = build_router(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn returns_quota_shape() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let router = build_router(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex/quota-snapshots")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: SnapshotListResponse = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload.provider, "codex");
    assert!(payload.snapshots.is_empty());
}

#[tokio::test]
async fn lists_codex_api_key_workspaces_in_quota_snapshots() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ncodex-api-key:\n  - api-key: third-party-key\n    label: Third Party Team\n    base-url: https://codex.example.com/v1\n",
            auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");
    let state = load_state_from_bootstrap(
        BackendBootstrap::new(config_path).with_local_management_password("secret"),
    )
    .expect("state should load");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex/quota-snapshots")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: SnapshotListResponse = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload.provider, "codex");
    assert_eq!(payload.snapshots.len(), 1);
    assert_eq!(payload.snapshots[0].auth_id, "codex-api-key:1");
    assert_eq!(
        payload.snapshots[0].auth_label.as_deref(),
        Some("Third Party Team")
    );
    assert_eq!(
        payload.snapshots[0].auth_note.as_deref(),
        Some("https://codex.example.com/v1")
    );
    assert_eq!(
        payload.snapshots[0].workspace_id.as_deref(),
        Some("codex-api-key:1:workspace")
    );
    assert_eq!(
        payload.snapshots[0].workspace_name.as_deref(),
        Some("Third Party Team")
    );
    assert_eq!(
        payload.snapshots[0].workspace_type.as_deref(),
        Some("third_party")
    );
    assert_eq!(payload.snapshots[0].source, "codex_api_key");
    assert!(payload.snapshots[0].snapshot.is_none());
}

#[tokio::test]
async fn serve_state_with_shutdown_stops_on_signal() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server = tokio::spawn(async move {
        serve_state_with_shutdown(state, listener, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    sleep(Duration::from_millis(50)).await;
    shutdown_tx.send(()).expect("shutdown signal");

    let join_result = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("server should stop in time");
    let server_result = join_result.expect("join handle");
    assert!(server_result.is_ok());
}

#[tokio::test]
async fn patches_auth_file_note() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("claude-demo.json"),
        r#"{"type":"claude","email":"demo@example.com"}"#,
    )
    .expect("auth file");

    let router = build_router(state.clone());
    let response = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/auth-files/fields")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"claude-demo.json","note":"My Claude Account"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let raw = fs::read_to_string(state.auth_dir.join("claude-demo.json")).expect("updated");
    let updated: Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(updated["note"].as_str(), Some("My Claude Account"));
}

#[tokio::test]
async fn patches_auth_file_status() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("codex-status@example.com-team.json"),
        r#"{"type":"codex","email":"demo@example.com","note":"Workspace A"}"#,
    )
    .expect("auth file");

    let disable_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/auth-files/status")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"codex-status@example.com-team.json","disabled":true}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(disable_response.status(), StatusCode::OK);
    let disable_body = to_bytes(disable_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let disable_payload: Value = serde_json::from_slice(&disable_body).expect("json");
    assert_eq!(
        disable_payload,
        serde_json::json!({ "status": "ok", "disabled": true })
    );

    let disabled_raw =
        fs::read_to_string(state.auth_dir.join("codex-status@example.com-team.json"))
            .expect("updated");
    let disabled_json: Value = serde_json::from_str(&disabled_raw).expect("json");
    assert_eq!(disabled_json["disabled"].as_bool(), Some(true));
    assert_eq!(disabled_json["status"].as_str(), Some("disabled"));
    assert_eq!(
        disabled_json["status_message"].as_str(),
        Some("disabled via management API")
    );

    let list_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = to_bytes(list_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let list_payload: Value = serde_json::from_slice(&list_body).expect("json");
    let files: Vec<AuthFileEntry> =
        serde_json::from_value(list_payload["files"].clone()).expect("files");
    assert_eq!(files.len(), 1);
    assert!(files[0].disabled);
    assert_eq!(files[0].status, "disabled");
    assert_eq!(
        files[0].status_message.as_deref(),
        Some("disabled via management API")
    );

    let enable_response = build_router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/auth-files/status")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"codex-status@example.com-team.json","disabled":false}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(enable_response.status(), StatusCode::OK);
    let enable_body = to_bytes(enable_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let enable_payload: Value = serde_json::from_slice(&enable_body).expect("json");
    assert_eq!(
        enable_payload,
        serde_json::json!({ "status": "ok", "disabled": false })
    );
}

#[tokio::test]
async fn returns_auth_file_models_from_provider_config() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ncodex-api-key:\n  - api-key: codex-1\n    models:\n      - name: gpt-5\n        display_name: GPT-5\n        type: chat\n        owned_by: openai\n      - alias: gpt-5\n        display_name: Duplicate GPT-5\n",
            auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        r#"{"type":"codex","email":"demo@example.com"}"#,
    )
    .expect("auth file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files/models?name=codex-demo@example.com-team.json")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        payload,
        serde_json::json!({
            "models": [
                {
                    "id": "gpt-5",
                    "display_name": "GPT-5",
                    "type": "chat",
                    "owned_by": "openai"
                }
            ]
        })
    );
}

#[tokio::test]
async fn falls_back_to_auth_file_candidate_models_when_provider_config_is_empty() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\n",
            auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        r#"{
                "type":"codex",
                "email":"demo@example.com",
                "models":[
                    {"name":"gpt-5"},
                    {"alias":"team-gpt5"}
                ],
                "excluded_models":["team-gpt5-mini"]
            }"#,
    )
    .expect("auth file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files/models?name=codex-demo@example.com-team.json")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    let model_ids = payload["models"]
        .as_array()
        .expect("models array")
        .iter()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(model_ids, vec!["gpt-5", "team-gpt5"]);
}

#[tokio::test]
async fn auth_file_models_follow_static_catalog_alias_exclusion_and_prefix() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\noauth-model-alias:\n  codex:\n    - name: gpt-5-codex\n      alias: team-codex\noauth-excluded-models:\n  codex:\n    - gpt-5-codex-mini\n",
            auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    let state = load_state_from_bootstrap(bootstrap).expect("state should load");
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        r#"{
                "type":"codex",
                "email":"demo@example.com",
                "prefix":" /lab/ "
            }"#,
    )
    .expect("auth file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/auth-files/models?name=codex-demo@example.com-team.json")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    let model_ids = payload["models"]
        .as_array()
        .expect("models array")
        .iter()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(model_ids.contains(&"lab/team-codex"));
    assert!(model_ids.contains(&"lab/gpt-5"));
    assert!(!model_ids.contains(&"team-codex"));
    assert!(!model_ids.contains(&"gpt-5-codex-mini"));
    assert!(!model_ids.contains(&"lab/gpt-5-codex-mini"));
}
