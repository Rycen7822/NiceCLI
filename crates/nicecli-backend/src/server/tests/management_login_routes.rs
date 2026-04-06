use super::*;

fn build_jwt(payload_json: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    format!("{header}.{payload}.signature")
}

async fn spawn_codex_token_server(id_token: String) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/oauth/token",
        post(move || {
            let id_token = id_token.clone();
            async move {
                Json(serde_json::json!({
                    "access_token": "access-token-123",
                    "refresh_token": "refresh-token-456",
                    "id_token": id_token,
                    "expires_in": 3600
                }))
            }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("token server");
    });
    address
}

async fn spawn_qwen_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/api/v1/oauth2/device/code",
            post(|| async {
                Json(serde_json::json!({
                    "device_code": "device-code-123",
                    "verification_uri": "https://chat.qwen.ai/device",
                    "verification_uri_complete": "https://chat.qwen.ai/device?user_code=ABCD",
                    "expires_in": 600,
                    "interval": 1
                }))
            }),
        )
        .route(
            "/api/v1/oauth2/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "access-token-123",
                    "refresh_token": "refresh-token-456",
                    "resource_url": "https://dashscope.aliyuncs.com",
                    "expires_in": 3600
                }))
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("qwen server");
    });
    address
}

async fn spawn_kimi_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/api/oauth/device_authorization",
            post(|| async {
                Json(serde_json::json!({
                    "device_code": "kimi-device-code-123",
                    "verification_uri": "https://auth.kimi.com/device",
                    "verification_uri_complete": "https://auth.kimi.com/device?user_code=KIMI",
                    "expires_in": 600,
                    "interval": 1
                }))
            }),
        )
        .route(
            "/api/oauth/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "kimi-access-token-123",
                    "refresh_token": "kimi-refresh-token-456",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "scope": "openid profile"
                }))
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("kimi server");
    });
    address
}

#[tokio::test]
async fn starts_and_completes_codex_login_via_rust_routes() {
    let claims = build_jwt(
        r#"{
            "email": "demo@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "org_default",
                "chatgpt_plan_type": "team",
                "organizations": [
                    { "id": "org_default", "title": "Workspace A", "is_default": true }
                ]
            }
        }"#,
    );
    let token_server = spawn_codex_token_server(claims).await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.codex_login_service = Arc::new(
        CodexLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            CodexLoginEndpoints {
                auth_url: format!("http://{token_server}/oauth/authorize"),
                token_url: format!("http://{token_server}/oauth/token"),
                ..CodexLoginEndpoints::default()
            },
        ),
    );

    let start_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex-auth-url")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body = to_bytes(start_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let start_payload: Value = serde_json::from_slice(&start_body).expect("json");
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();
    assert!(start_payload["url"]
        .as_str()
        .expect("url")
        .contains("/oauth/authorize"));

    let callback_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/codex/callback?state={state_value}&code=auth-code-123"
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(callback_response.status(), StatusCode::OK);
    let callback_body = to_bytes(callback_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let callback_text = String::from_utf8(callback_body.to_vec()).expect("utf8");
    assert!(callback_text.contains("Authentication successful"));

    let auth_files: Vec<_> = fs::read_dir(&state.auth_dir)
        .expect("read auth dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(auth_files.len(), 1);
    assert!(auth_files[0].ends_with("-demo@example.com-team.json"));

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_files[0])).expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["note"].as_str(), Some("Workspace A"));

    let status_response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v0/management/get-auth-status?state={state_value}"
                ))
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(status_response.status(), StatusCode::OK);
    let status_body = to_bytes(status_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let status_payload: Value = serde_json::from_slice(&status_body).expect("json");
    assert_eq!(status_payload["status"].as_str(), Some("ok"));
}

#[tokio::test]
async fn starts_and_completes_qwen_login_via_rust_routes() {
    let qwen_server = spawn_qwen_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.qwen_login_service = Arc::new(
        QwenLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            QwenLoginEndpoints {
                device_code_url: format!("http://{qwen_server}/api/v1/oauth2/device/code"),
                token_url: format!("http://{qwen_server}/api/v1/oauth2/token"),
                ..QwenLoginEndpoints::default()
            },
        ),
    );

    let start_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/qwen-auth-url")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body = to_bytes(start_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let start_payload: Value = serde_json::from_slice(&start_body).expect("json");
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();
    assert!(start_payload["url"]
        .as_str()
        .expect("url")
        .contains("chat.qwen.ai/device"));

    let mut final_status = None;
    for _ in 0..20 {
        let status_response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v0/management/get-auth-status?state={state_value}"
                    ))
                    .header("X-Management-Key", "secret")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(status_response.status(), StatusCode::OK);
        let status_body = to_bytes(status_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let status_payload: Value = serde_json::from_slice(&status_body).expect("json");

        match status_payload["status"].as_str() {
            Some("ok") => {
                final_status = Some("ok".to_string());
                break;
            }
            Some("wait") => sleep(Duration::from_millis(50)).await,
            Some("error") => {
                panic!(
                    "qwen login failed: {}",
                    status_payload["error"].as_str().unwrap_or("unknown error")
                );
            }
            other => panic!("unexpected auth status: {other:?}"),
        }
    }

    assert_eq!(final_status.as_deref(), Some("ok"));

    let auth_files: Vec<_> = fs::read_dir(&state.auth_dir)
        .expect("read auth dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(auth_files.len(), 1);
    assert!(auth_files[0].starts_with("qwen-"));
    assert!(auth_files[0].ends_with(".json"));

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_files[0])).expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("qwen"));
    assert_eq!(auth_payload["provider"].as_str(), Some("qwen"));
    assert_eq!(
        auth_payload["access_token"].as_str(),
        Some("access-token-123")
    );
    assert_eq!(
        auth_payload["refresh_token"].as_str(),
        Some("refresh-token-456")
    );
}

#[tokio::test]
async fn starts_and_completes_kimi_login_via_rust_routes() {
    let kimi_server = spawn_kimi_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.kimi_login_service = Arc::new(
        KimiLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            KimiLoginEndpoints {
                device_code_url: format!("http://{kimi_server}/api/oauth/device_authorization"),
                token_url: format!("http://{kimi_server}/api/oauth/token"),
                ..KimiLoginEndpoints::default()
            },
        ),
    );

    let start_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/kimi-auth-url")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(start_response.status(), StatusCode::OK);
    let start_body = to_bytes(start_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let start_payload: Value = serde_json::from_slice(&start_body).expect("json");
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();
    assert!(start_payload["url"]
        .as_str()
        .expect("url")
        .contains("auth.kimi.com/device"));

    let mut final_status = None;
    for _ in 0..20 {
        let status_response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v0/management/get-auth-status?state={state_value}"
                    ))
                    .header("X-Management-Key", "secret")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(status_response.status(), StatusCode::OK);
        let status_body = to_bytes(status_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let status_payload: Value = serde_json::from_slice(&status_body).expect("json");

        match status_payload["status"].as_str() {
            Some("ok") => {
                final_status = Some("ok".to_string());
                break;
            }
            Some("wait") => sleep(Duration::from_millis(50)).await,
            Some("error") => {
                panic!(
                    "kimi login failed: {}",
                    status_payload["error"].as_str().unwrap_or("unknown error")
                );
            }
            other => panic!("unexpected auth status: {other:?}"),
        }
    }

    assert_eq!(final_status.as_deref(), Some("ok"));

    let auth_files: Vec<_> = fs::read_dir(&state.auth_dir)
        .expect("read auth dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(auth_files.len(), 1);
    assert!(auth_files[0].starts_with("kimi-"));
    assert!(auth_files[0].ends_with(".json"));

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_files[0])).expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("kimi"));
    assert_eq!(auth_payload["provider"].as_str(), Some("kimi"));
    assert_eq!(
        auth_payload["access_token"].as_str(),
        Some("kimi-access-token-123")
    );
    assert_eq!(
        auth_payload["refresh_token"].as_str(),
        Some("kimi-refresh-token-456")
    );
    assert_eq!(auth_payload["token_type"].as_str(), Some("Bearer"));
    assert_eq!(auth_payload["scope"].as_str(), Some("openid profile"));
    assert!(auth_payload["device_id"]
        .as_str()
        .is_some_and(|value| !value.trim().is_empty()));
}
