use super::*;

async fn spawn_usage_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/backend-api/wham/usage",
        get(|| async {
            Json(serde_json::json!({
                "plan_type": "team",
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 25,
                        "limit_window_seconds": 18000,
                        "reset_at": 1760000000
                    }
                },
                "credits": {
                    "has_credits": true,
                    "unlimited": false,
                    "balance": "12.5"
                }
            }))
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("usage server");
    });
    address
}

#[tokio::test]
async fn saves_gemini_web_tokens_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let response = build_router(state.clone())
        .oneshot(
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
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(payload["status"].as_str(), Some("ok"));
    assert_eq!(
        payload["file"].as_str(),
        Some("gemini-web-gemini-web@example.com.json")
    );

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(
            state
                .auth_dir
                .join("gemini-web-gemini-web@example.com.json"),
        )
        .expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("gemini"));
    assert_eq!(auth_payload["provider"].as_str(), Some("gemini"));
    assert_eq!(
        auth_payload["email"].as_str(),
        Some("gemini-web@example.com")
    );
    assert_eq!(auth_payload["auth_mode"].as_str(), Some("web_cookies"));
    assert_eq!(
        auth_payload["cookie"].as_str(),
        Some("Secure-1PSID=secure-psid-123; Secure-1PSIDTS=secure-psidts-456;")
    );
}

#[tokio::test]
async fn imports_vertex_credential_via_rust_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let boundary = "nicecli-boundary";
    let service_account = r#"{"type":"service_account","project_id":"vertex-demo-project","client_email":"vertex@example.com","private_key":"-----BEGIN PRIVATE KEY-----\nline1\n-----END PRIVATE KEY-----\n"}"#;
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"location\"\r\n\r\n\
asia-east1\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"service-account.json\"\r\n\
Content-Type: application/json\r\n\r\n\
{service_account}\r\n\
--{boundary}--\r\n"
    );

    let response = build_router(state.clone())
        .oneshot(
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
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&response_body).expect("json");
    assert_eq!(payload["status"].as_str(), Some("ok"));
    assert_eq!(payload["project_id"].as_str(), Some("vertex-demo-project"));
    assert_eq!(payload["email"].as_str(), Some("vertex@example.com"));
    assert_eq!(payload["location"].as_str(), Some("asia-east1"));

    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join("vertex-vertex-demo-project.json"))
            .expect("auth file"),
    )
    .expect("json");
    assert_eq!(auth_payload["type"].as_str(), Some("vertex"));
    assert_eq!(auth_payload["provider"].as_str(), Some("vertex"));
    assert_eq!(
        auth_payload["project_id"].as_str(),
        Some("vertex-demo-project")
    );
    assert_eq!(auth_payload["email"].as_str(), Some("vertex@example.com"));
    assert_eq!(auth_payload["location"].as_str(), Some("asia-east1"));
    assert_eq!(
        auth_payload["service_account"]["project_id"].as_str(),
        Some("vertex-demo-project")
    );
}

#[tokio::test]
async fn refreshes_codex_quota_snapshots_through_rust_service() {
    let usage_server = spawn_usage_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.auth_dir.join("codex-demo@example.com-team.json"),
        format!(
            r#"{{
                    "provider": "codex",
                    "id": "auth-a",
                    "label": "Primary",
                    "metadata": {{
                        "email": "demo@example.com",
                        "access_token": "token-123"
                    }},
                    "attributes": {{
                        "base_url": "http://{}/backend-api"
                    }}
                }}"#,
            usage_server
        ),
    )
    .expect("auth file");

    let router = build_router(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex/quota-snapshots?refresh=1")
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
    assert_eq!(payload.provider, PROVIDER_CODEX);
    assert_eq!(payload.snapshots.len(), 1);
    assert_eq!(payload.snapshots[0].auth_id, "auth-a");
    assert_eq!(
        payload.snapshots[0].account_email.as_deref(),
        Some("demo@example.com")
    );
    assert_eq!(payload.snapshots[0].account_plan.as_deref(), Some("team"));
    assert_eq!(
        payload.snapshots[0]
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.plan_type.as_deref()),
        Some("team")
    );
    assert!(!payload.snapshots[0].fetched_at.is_empty());
}
