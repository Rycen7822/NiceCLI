use super::*;

#[tokio::test]
async fn returns_config_json_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ndebug: true\nlogging-to-file: true\nrequest-log: true\nquota-exceeded:\n  switch-project: true\n",
            auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");
    let state = load_state_from_bootstrap(
        BackendBootstrap::new(config_path).with_local_management_password("secret"),
    )
    .expect("state");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/config")
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
    assert_eq!(payload["debug"].as_bool(), Some(true));
    assert_eq!(payload["logging-to-file"].as_bool(), Some(true));
    assert_eq!(payload["request-log"].as_bool(), Some(true));
    assert_eq!(
        payload["quota-exceeded"]["switch-project"].as_bool(),
        Some(true)
    );
}

#[tokio::test]
async fn updates_usage_statistics_enabled_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/usage-statistics-enabled")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"value":true}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/usage-statistics-enabled")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), StatusCode::OK);
    let body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload["usage-statistics-enabled"].as_bool(), Some(true));
    assert!(fs::read_to_string(state.bootstrap.config_path())
        .expect("config")
        .contains("usage-statistics-enabled: true"));
}

#[tokio::test]
async fn normalizes_error_logs_max_files_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/error-logs-max-files")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"value":-1}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/error-logs-max-files")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), StatusCode::OK);
    let body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload["error-logs-max-files"].as_i64(), Some(10));
}

#[tokio::test]
async fn returns_usage_snapshot_placeholder_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/usage")
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
    assert_eq!(payload["failed_requests"].as_i64(), Some(0));
    assert_eq!(payload["usage"]["total_requests"].as_i64(), Some(0));
    assert!(payload["usage"]["apis"].is_object());
}

#[tokio::test]
async fn returns_logs_from_auth_log_directory_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let log_dir = state.auth_dir.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    fs::write(
        log_dir.join("main.log.1"),
        "[2026-04-05 06:00:00] older line\n",
    )
    .expect("rotated log");
    fs::write(
        log_dir.join("main.log"),
        "[2026-04-05 06:10:00] newer line\ncontinuation line\n",
    )
    .expect("main log");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/logs?limit=10")
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
    let lines = payload["lines"].as_array().expect("lines array");
    assert_eq!(payload["line-count"].as_u64(), Some(3));
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].as_str(), Some("[2026-04-05 06:00:00] older line"));
    assert_eq!(lines[1].as_str(), Some("[2026-04-05 06:10:00] newer line"));
    assert_eq!(lines[2].as_str(), Some("continuation line"));
    assert!(payload["latest-timestamp"].as_i64().unwrap_or_default() > 0);
}

#[tokio::test]
async fn downloads_request_log_by_id_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let log_dir = state.auth_dir.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let file_name = "request-abc123.log";
    fs::write(log_dir.join(file_name), "request body").expect("request log");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/request-log-by-id/abc123")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(disposition.contains(file_name));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), b"request body");
}
