use super::*;

async fn spawn_anthropic_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/v1/oauth/token",
        post(|| async {
            Json(serde_json::json!({
                "access_token": "claude-route-access-token",
                "refresh_token": "claude-route-refresh-token",
                "expires_in": 3600,
                "organization": {
                    "uuid": "org-route-123",
                    "name": "Claude Route Org"
                },
                "account": {
                    "uuid": "acct-route-456",
                    "email_address": "claude-route@example.com"
                }
            }))
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("anthropic route server");
    });
    address
}

async fn spawn_gemini_cli_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/oauth2/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "gemini-route-access-token",
                    "refresh_token": "gemini-route-refresh-token",
                    "token_type": "Bearer",
                    "expires_in": 3600
                }))
            }),
        )
        .route(
            "/userinfo",
            get(|| async {
                Json(serde_json::json!({
                    "email": "gemini-route@example.com"
                }))
            }),
        )
        .route(
            "/projects",
            get(|| async {
                Json(serde_json::json!({
                    "projects": [
                        { "projectId": "auto-route-project-123" }
                    ]
                }))
            }),
        )
        .route(
            "/v1/projects/auto-route-project-123/services/cloudaicompanion.googleapis.com",
            get(|| async {
                Json(serde_json::json!({
                    "state": "ENABLED"
                }))
            }),
        )
        .route(
            "/v1/projects/manual-route-project-456/services/cloudaicompanion.googleapis.com",
            get(|| async {
                Json(serde_json::json!({
                    "state": "ENABLED"
                }))
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("gemini route server");
    });
    address
}

async fn spawn_antigravity_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/oauth2/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token": "antigravity-access-token",
                    "refresh_token": "antigravity-refresh-token",
                    "expires_in": 3600
                }))
            }),
        )
        .route(
            "/userinfo",
            get(|| async {
                Json(serde_json::json!({
                    "email": "antigravity-route@example.com"
                }))
            }),
        )
        .route(
            "/v1internal:loadCodeAssist",
            post(|| async {
                Json(serde_json::json!({
                    "cloudaicompanionProject": {
                        "id": "route-project-123"
                    }
                }))
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("antigravity route server");
    });
    address
}

#[tokio::test]
async fn auth_status_reflects_pending_and_failed_oauth_sessions() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-state-01", "codex")
        .expect("register session");

    let pending_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/get-auth-status?state=codex-state-01")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(pending_response.status(), StatusCode::OK);
    let pending_body = to_bytes(pending_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let pending_payload: Value = serde_json::from_slice(&pending_body).expect("json");
    assert_eq!(pending_payload["status"].as_str(), Some("wait"));

    state
        .oauth_sessions
        .set_error("codex-state-01", "Bad Request")
        .expect("set error");

    let failed_response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v0/management/get-auth-status?state=codex-state-01")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(failed_response.status(), StatusCode::OK);
    let failed_body = to_bytes(failed_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let failed_payload: Value = serde_json::from_slice(&failed_body).expect("json");
    assert_eq!(failed_payload["status"].as_str(), Some("error"));
    assert_eq!(failed_payload["error"].as_str(), Some("Bad Request"));
}

#[tokio::test]
async fn persists_oauth_callback_file_for_pending_session() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    state
        .oauth_sessions
        .register("codex-callback-01", "codex")
        .expect("register session");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v0/management/oauth-callback")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{
                            "provider":"openai",
                            "redirect_url":"http://127.0.0.1:1455/codex/callback?state=codex-callback-01&code=code-123"
                        }"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload["status"].as_str(), Some("ok"));

    let callback_path = state.auth_dir.join(".oauth-codex-codex-callback-01.oauth");
    let callback_payload: Value =
        serde_json::from_str(&fs::read_to_string(callback_path).expect("callback file"))
            .expect("json");
    assert_eq!(
        callback_payload["state"].as_str(),
        Some("codex-callback-01")
    );
    assert_eq!(callback_payload["code"].as_str(), Some("code-123"));
}

#[tokio::test]
async fn starts_and_completes_anthropic_login_via_rust_routes() {
    let anthropic_server = spawn_anthropic_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.anthropic_login_service = Arc::new(
        AnthropicLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            AnthropicLoginEndpoints {
                authorize_url: format!("http://{anthropic_server}/oauth/authorize"),
                token_url: format!("http://{anthropic_server}/v1/oauth/token"),
                ..AnthropicLoginEndpoints::default()
            },
        ),
    );

    let start_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/anthropic-auth-url")
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
                    "/anthropic/callback?state={state_value}&code=auth-code-123"
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
                    "anthropic login failed: {}",
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
    assert_eq!(auth_files[0], "claude-claude-route@example.com.json");

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_files[0])).expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("claude"));
    assert_eq!(auth_payload["provider"].as_str(), Some("claude"));
    assert_eq!(
        auth_payload["email"].as_str(),
        Some("claude-route@example.com")
    );
    assert_eq!(
        auth_payload["organization_name"].as_str(),
        Some("Claude Route Org")
    );
    assert_eq!(
        auth_payload["access_token"].as_str(),
        Some("claude-route-access-token")
    );
}

#[tokio::test]
async fn starts_and_completes_gemini_cli_login_via_rust_routes() {
    let gemini_server = spawn_gemini_cli_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.gemini_cli_login_service = Arc::new(
        GeminiCliLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            GeminiCliLoginEndpoints {
                authorize_url: format!("http://{gemini_server}/oauth/authorize"),
                token_url: format!("http://{gemini_server}/oauth2/token"),
                user_info_url: format!("http://{gemini_server}/userinfo"),
                projects_url: format!("http://{gemini_server}/projects"),
                service_usage_url: format!("http://{gemini_server}"),
                ..GeminiCliLoginEndpoints::default()
            },
        ),
    );

    let start_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/gemini-cli-auth-url?project_id=manual-route-project-456")
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
                    "/google/callback?state={state_value}&code=auth-code-123"
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
                    "gemini cli login failed: {}",
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
    assert_eq!(
        auth_files[0],
        "gemini-gemini-route@example.com-manual-route-project-456.json"
    );

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_files[0])).expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("gemini"));
    assert_eq!(
        auth_payload["project_id"].as_str(),
        Some("manual-route-project-456")
    );
    assert_eq!(auth_payload["auto"].as_bool(), Some(false));
    assert_eq!(auth_payload["checked"].as_bool(), Some(true));
    assert_eq!(
        auth_payload["email"].as_str(),
        Some("gemini-route@example.com")
    );
    assert_eq!(
        auth_payload["access_token"].as_str(),
        Some("gemini-route-access-token")
    );
}

#[tokio::test]
async fn starts_and_completes_antigravity_login_via_rust_routes() {
    let antigravity_server = spawn_antigravity_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.antigravity_login_service = Arc::new(
        AntigravityLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            AntigravityLoginEndpoints {
                authorize_url: format!("http://{antigravity_server}/oauth2/authorize"),
                token_url: format!("http://{antigravity_server}/oauth2/token"),
                user_info_url: format!("http://{antigravity_server}/userinfo"),
                load_code_assist_url: format!(
                    "http://{antigravity_server}/v1internal:loadCodeAssist"
                ),
                ..AntigravityLoginEndpoints::default()
            },
        ),
    );

    let start_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/antigravity-auth-url?is_webui=true")
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
        .contains("/oauth2/authorize"));

    let callback_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/antigravity/callback?state={state_value}&code=auth-code-123"
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
                    "antigravity login failed: {}",
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
    assert_eq!(
        auth_files[0],
        "antigravity-antigravity-route@example.com.json"
    );

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_files[0])).expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("antigravity"));
    assert_eq!(
        auth_payload["email"].as_str(),
        Some("antigravity-route@example.com")
    );
    assert_eq!(
        auth_payload["project_id"].as_str(),
        Some("route-project-123")
    );
    assert_eq!(
        auth_payload["access_token"].as_str(),
        Some("antigravity-access-token")
    );
}
