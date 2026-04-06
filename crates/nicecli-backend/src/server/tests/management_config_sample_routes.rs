use super::*;

#[tokio::test]
async fn supports_config_yaml_round_trip_sample() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_dir = state.auth_dir.to_string_lossy().replace('\\', "/");
    let config_path = state.bootstrap.config_path().to_path_buf();
    let initial_config = format!(
        "# original comment\nhost: 127.0.0.1\nport: 8317\nauth-dir: {auth_dir}\nproxy-url: \"\"\n"
    );
    fs::write(&config_path, &initial_config).expect("initial config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/config.yaml")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), StatusCode::OK);
    assert_eq!(
        get_response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("application/yaml; charset=utf-8")
    );
    assert_eq!(
        get_response
            .headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        get_response
            .headers()
            .get("X-Content-Type-Options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    let get_body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8(get_body.to_vec()).expect("utf8"),
        initial_config
    );

    let updated_config = format!(
        "# updated comment\nhost: 127.0.0.1\nport: 8317\nauth-dir: {auth_dir}\nproxy-url: \"http://127.0.0.1:7890\"\nrouting:\n  strategy: fill-first\n"
    );
    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/config.yaml")
                .header("X-Management-Key", "secret")
                .body(Body::from(updated_config.clone()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(put_response.status(), StatusCode::OK);
    let put_body = to_bytes(put_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let put_payload: Value = serde_json::from_slice(&put_body).expect("json");
    assert_eq!(
        put_payload,
        serde_json::json!({ "ok": true, "changed": ["config"] })
    );

    assert_eq!(
        fs::read_to_string(&config_path).expect("saved config"),
        updated_config
    );
}

#[tokio::test]
async fn supports_representative_config_basic_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_dir = state.auth_dir.to_string_lossy().replace('\\', "/");
    let config_path = state.bootstrap.config_path().to_path_buf();
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {auth_dir}\nproxy-url: \"http://127.0.0.1:7890\"\nrouting:\n  strategy: round-robin\n"
        ),
    )
    .expect("config file");

    let get_proxy_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/proxy-url")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_proxy_response.status(), StatusCode::OK);
    let get_proxy_body = to_bytes(get_proxy_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_proxy_payload: Value = serde_json::from_slice(&get_proxy_body).expect("json");
    assert_eq!(
        get_proxy_payload,
        serde_json::json!({ "proxy-url": "http://127.0.0.1:7890" })
    );

    let patch_strategy_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/routing/strategy")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"value":"ff"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(patch_strategy_response.status(), StatusCode::OK);
    let patch_strategy_body = to_bytes(patch_strategy_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let patch_strategy_payload: Value = serde_json::from_slice(&patch_strategy_body).expect("json");
    assert_eq!(
        patch_strategy_payload,
        serde_json::json!({ "status": "ok" })
    );

    let get_strategy_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/routing/strategy")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_strategy_response.status(), StatusCode::OK);
    let get_strategy_body = to_bytes(get_strategy_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_strategy_payload: Value = serde_json::from_slice(&get_strategy_body).expect("json");
    assert_eq!(
        get_strategy_payload,
        serde_json::json!({ "strategy": "fill-first" })
    );

    let reloaded = state.bootstrap.load_config().expect("reload config");
    assert_eq!(reloaded.routing.strategy.as_deref(), Some("fill-first"));
}

#[tokio::test]
async fn supports_ampcode_config_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let auth_dir = state.auth_dir.to_string_lossy().replace('\\', "/");
    let config_path = state.bootstrap.config_path().to_path_buf();
    fs::write(
        &config_path,
        format!("host: 127.0.0.1\nport: 8317\nauth-dir: {auth_dir}\n"),
    )
    .expect("config file");

    let get_default_url_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/ampcode/upstream-url")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_default_url_response.status(), StatusCode::OK);
    let get_default_url_body = to_bytes(get_default_url_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_default_url_payload: Value =
        serde_json::from_slice(&get_default_url_body).expect("json");
    assert_eq!(
        get_default_url_payload,
        serde_json::json!({ "upstream-url": "" })
    );

    let get_default_restrict_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/ampcode/restrict-management-to-localhost")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_default_restrict_response.status(), StatusCode::OK);
    let get_default_restrict_body = to_bytes(get_default_restrict_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_default_restrict_payload: Value =
        serde_json::from_slice(&get_default_restrict_body).expect("json");
    assert_eq!(
        get_default_restrict_payload,
        serde_json::json!({ "restrict-management-to-localhost": false })
    );

    for (method, route, body) in [
        (
            "PUT",
            "/v0/management/ampcode/upstream-url",
            r#"{"value":"https://amp.example.com"}"#,
        ),
        (
            "PATCH",
            "/v0/management/ampcode/upstream-api-key",
            r#"{"value":"amp-secret"}"#,
        ),
        (
            "PUT",
            "/v0/management/ampcode/restrict-management-to-localhost",
            r#"{"value":false}"#,
        ),
    ] {
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(route)
                    .header("X-Management-Key", "secret")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK, "route={route}");
    }

    let get_url_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/ampcode/upstream-url")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let get_url_body = to_bytes(get_url_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_url_payload: Value = serde_json::from_slice(&get_url_body).expect("json");
    assert_eq!(
        get_url_payload,
        serde_json::json!({ "upstream-url": "https://amp.example.com" })
    );

    let get_api_key_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/ampcode/upstream-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let get_api_key_body = to_bytes(get_api_key_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_api_key_payload: Value = serde_json::from_slice(&get_api_key_body).expect("json");
    assert_eq!(
        get_api_key_payload,
        serde_json::json!({ "upstream-api-key": "amp-secret" })
    );

    let get_restrict_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/ampcode/restrict-management-to-localhost")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let get_restrict_body = to_bytes(get_restrict_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_restrict_payload: Value = serde_json::from_slice(&get_restrict_body).expect("json");
    assert_eq!(
        get_restrict_payload,
        serde_json::json!({ "restrict-management-to-localhost": false })
    );

    let delete_api_key_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/ampcode/upstream-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_api_key_response.status(), StatusCode::OK);

    let get_deleted_api_key_response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/ampcode/upstream-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let get_deleted_api_key_body = to_bytes(get_deleted_api_key_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_deleted_api_key_payload: Value =
        serde_json::from_slice(&get_deleted_api_key_body).expect("json");
    assert_eq!(
        get_deleted_api_key_payload,
        serde_json::json!({ "upstream-api-key": "" })
    );
}
