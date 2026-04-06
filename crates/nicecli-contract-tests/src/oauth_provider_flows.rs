use crate::fixture_data::*;
use crate::normalization::{
    normalize_anthropic_auth_file, normalize_antigravity_auth_file, normalize_codex_auth_file,
    normalize_gemini_cli_auth_file, normalize_oauth_auth_url_response, normalize_qwen_auth_file,
};
use crate::test_support::{
    build_jwt, expected_json, load_fixture_state, request_json, spawn_anthropic_login_server,
    spawn_antigravity_login_server, spawn_codex_token_server, spawn_gemini_cli_login_server,
    spawn_qwen_login_server,
};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use nicecli_auth::{
    AnthropicLoginEndpoints, AnthropicLoginService, AntigravityLoginEndpoints,
    AntigravityLoginService, CodexLoginEndpoints, CodexLoginService, GeminiCliLoginEndpoints,
    GeminiCliLoginService, QwenLoginEndpoints, QwenLoginService,
};
use nicecli_backend::build_router;
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn codex_auth_url_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.codex_login_service = Arc::new(
        CodexLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            CodexLoginEndpoints {
                auth_url: "https://codex.local/oauth/authorize".to_string(),
                ..CodexLoginEndpoints::default()
            },
        ),
    );

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/codex-auth-url")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_oauth_auth_url_response(payload, &["code_challenge", "state"]),
        expected_json(OAUTH_CODEX_AUTH_URL_EXPECTED)
    );
}

#[tokio::test]
async fn codex_route_flow_fixture_matches_rust_response() {
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
                auth_url: "https://codex.local/oauth/authorize".to_string(),
                token_url: format!("http://{token_server}/oauth/token"),
                ..CodexLoginEndpoints::default()
            },
        ),
    );

    let (start_status, start_payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/codex-auth-url")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(start_status, StatusCode::OK);
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();

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
    let callback_status = callback_response.status();
    let callback_body = to_bytes(callback_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let callback_text = String::from_utf8(callback_body.to_vec()).expect("utf8");

    let auth_file_name = "codex-6c1edc29-demo@example.com-team.json";
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(auth_file_name)).expect("auth file"),
    )
    .expect("json");

    let (status_after_callback, status_payload) = request_json(
        build_router(state),
        Request::builder()
            .uri(format!(
                "/v0/management/get-auth-status?state={state_value}"
            ))
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status_after_callback, StatusCode::OK);
    assert_eq!(
        json!({
            "start": normalize_oauth_auth_url_response(start_payload, &["code_challenge", "state"]),
            "callback": {
                "status": callback_status.as_u16(),
                "success_text": callback_text.contains("Authentication successful")
            },
            "auth_file_name": auth_file_name,
            "auth_file": normalize_codex_auth_file(auth_payload),
            "status_after_callback": status_payload,
        }),
        expected_json(OAUTH_CODEX_ROUTE_FLOW_EXPECTED)
    );
}

#[tokio::test]
async fn qwen_auth_url_fixture_matches_rust_response() {
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

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/qwen-auth-url")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_oauth_auth_url_response(payload, &[]),
        expected_json(OAUTH_QWEN_AUTH_URL_EXPECTED)
    );
}

#[tokio::test]
async fn qwen_route_flow_fixture_matches_rust_response() {
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

    let (start_status, start_payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/qwen-auth-url")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(start_status, StatusCode::OK);
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();

    let mut final_status_payload = None;
    for _ in 0..20 {
        let (status, payload) = request_json(
            build_router(state.clone()),
            Request::builder()
                .uri(format!(
                    "/v0/management/get-auth-status?state={state_value}"
                ))
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        match payload["status"].as_str() {
            Some("ok") => {
                final_status_payload = Some(payload);
                break;
            }
            Some("wait") => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            Some("error") => panic!(
                "qwen login failed: {}",
                payload["error"].as_str().unwrap_or("unknown error")
            ),
            other => panic!("unexpected auth status: {other:?}"),
        }
    }

    let status_after_callback = final_status_payload.expect("qwen status should become ok");
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
    let auth_file_name = auth_files[0].clone();
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(&auth_file_name)).expect("auth file"),
    )
    .expect("json");

    assert_eq!(
        json!({
            "start": normalize_oauth_auth_url_response(start_payload, &[]),
            "auth_file": normalize_qwen_auth_file(&auth_file_name, auth_payload),
            "status_after_callback": status_after_callback,
        }),
        expected_json(OAUTH_QWEN_ROUTE_FLOW_EXPECTED)
    );
}

#[tokio::test]
async fn anthropic_auth_url_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.anthropic_login_service = Arc::new(
        AnthropicLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            AnthropicLoginEndpoints {
                authorize_url: "https://claude.local/oauth/authorize".to_string(),
                ..AnthropicLoginEndpoints::default()
            },
        ),
    );

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/anthropic-auth-url")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_oauth_auth_url_response(payload, &["code_challenge", "state"]),
        expected_json(OAUTH_ANTHROPIC_AUTH_URL_EXPECTED)
    );
}

#[tokio::test]
async fn anthropic_route_flow_fixture_matches_rust_response() {
    let anthropic_server = spawn_anthropic_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.anthropic_login_service = Arc::new(
        AnthropicLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            AnthropicLoginEndpoints {
                authorize_url: "https://claude.local/oauth/authorize".to_string(),
                token_url: format!("http://{anthropic_server}/v1/oauth/token"),
                ..AnthropicLoginEndpoints::default()
            },
        ),
    );

    let (start_status, start_payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/anthropic-auth-url")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(start_status, StatusCode::OK);
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();

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
    let callback_status = callback_response.status();
    let callback_body = to_bytes(callback_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let callback_text = String::from_utf8(callback_body.to_vec()).expect("utf8");

    let mut status_after_callback = None;
    for _ in 0..20 {
        let (status, payload) = request_json(
            build_router(state.clone()),
            Request::builder()
                .uri(format!(
                    "/v0/management/get-auth-status?state={state_value}"
                ))
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        match payload["status"].as_str() {
            Some("ok") => {
                status_after_callback = Some(payload);
                break;
            }
            Some("wait") => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            Some("error") => panic!(
                "anthropic login failed: {}",
                payload["error"].as_str().unwrap_or("unknown error")
            ),
            other => panic!("unexpected auth status: {other:?}"),
        }
    }

    let status_after_callback = status_after_callback.expect("anthropic status should become ok");
    let auth_file_name = "claude-claude-route@example.com.json";
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(auth_file_name)).expect("auth file"),
    )
    .expect("json");

    assert_eq!(
        json!({
            "start": normalize_oauth_auth_url_response(start_payload, &["code_challenge", "state"]),
            "callback": {
                "status": callback_status.as_u16(),
                "success_text": callback_text.contains("Authentication successful")
            },
            "auth_file_name": auth_file_name,
            "auth_file": normalize_anthropic_auth_file(auth_file_name, auth_payload),
            "status_after_callback": status_after_callback,
        }),
        expected_json(OAUTH_ANTHROPIC_ROUTE_FLOW_EXPECTED)
    );
}

#[tokio::test]
async fn gemini_cli_auth_url_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.gemini_cli_login_service = Arc::new(
        GeminiCliLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            GeminiCliLoginEndpoints {
                authorize_url: "https://gemini.local/oauth/authorize".to_string(),
                ..GeminiCliLoginEndpoints::default()
            },
        ),
    );

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/gemini-cli-auth-url?project_id=manual-route-project-456")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_oauth_auth_url_response(payload, &["state"]),
        expected_json(OAUTH_GEMINI_CLI_AUTH_URL_EXPECTED)
    );
}

#[tokio::test]
async fn gemini_cli_route_flow_fixture_matches_rust_response() {
    let gemini_server = spawn_gemini_cli_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.gemini_cli_login_service = Arc::new(
        GeminiCliLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            GeminiCliLoginEndpoints {
                authorize_url: "https://gemini.local/oauth/authorize".to_string(),
                token_url: format!("http://{gemini_server}/oauth2/token"),
                user_info_url: format!("http://{gemini_server}/userinfo"),
                projects_url: format!("http://{gemini_server}/projects"),
                service_usage_url: format!("http://{gemini_server}"),
                ..GeminiCliLoginEndpoints::default()
            },
        ),
    );

    let (start_status, start_payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/gemini-cli-auth-url?project_id=manual-route-project-456")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(start_status, StatusCode::OK);
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();

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
    let callback_status = callback_response.status();
    let callback_body = to_bytes(callback_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let callback_text = String::from_utf8(callback_body.to_vec()).expect("utf8");

    let mut status_after_callback = None;
    for _ in 0..20 {
        let (status, payload) = request_json(
            build_router(state.clone()),
            Request::builder()
                .uri(format!(
                    "/v0/management/get-auth-status?state={state_value}"
                ))
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        match payload["status"].as_str() {
            Some("ok") => {
                status_after_callback = Some(payload);
                break;
            }
            Some("wait") => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            Some("error") => panic!(
                "gemini cli login failed: {}",
                payload["error"].as_str().unwrap_or("unknown error")
            ),
            other => panic!("unexpected auth status: {other:?}"),
        }
    }

    let status_after_callback = status_after_callback.expect("gemini status should become ok");
    let auth_file_name = "gemini-gemini-route@example.com-manual-route-project-456.json";
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(auth_file_name)).expect("auth file"),
    )
    .expect("json");

    assert_eq!(
        json!({
            "start": normalize_oauth_auth_url_response(start_payload, &["state"]),
            "callback": {
                "status": callback_status.as_u16(),
                "success_text": callback_text.contains("Authentication successful")
            },
            "auth_file_name": auth_file_name,
            "auth_file": normalize_gemini_cli_auth_file(auth_file_name, auth_payload),
            "status_after_callback": status_after_callback,
        }),
        expected_json(OAUTH_GEMINI_CLI_ROUTE_FLOW_EXPECTED)
    );
}

#[tokio::test]
async fn antigravity_auth_url_fixture_matches_rust_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.antigravity_login_service = Arc::new(
        AntigravityLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            AntigravityLoginEndpoints {
                authorize_url: "https://antigravity.local/oauth2/authorize".to_string(),
                ..AntigravityLoginEndpoints::default()
            },
        ),
    );

    let (status, payload) = request_json(
        build_router(state),
        Request::builder()
            .uri("/v0/management/antigravity-auth-url?is_webui=true")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        normalize_oauth_auth_url_response(payload, &["state"]),
        expected_json(OAUTH_ANTIGRAVITY_AUTH_URL_EXPECTED)
    );
}

#[tokio::test]
async fn antigravity_route_flow_fixture_matches_rust_response() {
    let antigravity_server = spawn_antigravity_login_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let mut state = load_fixture_state(&temp_dir);
    state.antigravity_login_service = Arc::new(
        AntigravityLoginService::new(state.oauth_sessions.clone(), None).with_endpoints(
            AntigravityLoginEndpoints {
                authorize_url: "https://antigravity.local/oauth2/authorize".to_string(),
                token_url: format!("http://{antigravity_server}/oauth2/token"),
                user_info_url: format!("http://{antigravity_server}/userinfo"),
                load_code_assist_url: format!(
                    "http://{antigravity_server}/v1internal:loadCodeAssist"
                ),
                ..AntigravityLoginEndpoints::default()
            },
        ),
    );

    let (start_status, start_payload) = request_json(
        build_router(state.clone()),
        Request::builder()
            .uri("/v0/management/antigravity-auth-url?is_webui=true")
            .header("X-Management-Key", "secret")
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    assert_eq!(start_status, StatusCode::OK);
    let state_value = start_payload["state"]
        .as_str()
        .expect("state should exist")
        .to_string();

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
    let callback_status = callback_response.status();
    let callback_body = to_bytes(callback_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let callback_text = String::from_utf8(callback_body.to_vec()).expect("utf8");

    let mut status_after_callback = None;
    for _ in 0..20 {
        let (status, payload) = request_json(
            build_router(state.clone()),
            Request::builder()
                .uri(format!(
                    "/v0/management/get-auth-status?state={state_value}"
                ))
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        match payload["status"].as_str() {
            Some("ok") => {
                status_after_callback = Some(payload);
                break;
            }
            Some("wait") => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            Some("error") => panic!(
                "antigravity login failed: {}",
                payload["error"].as_str().unwrap_or("unknown error")
            ),
            other => panic!("unexpected auth status: {other:?}"),
        }
    }

    let status_after_callback = status_after_callback.expect("antigravity status should become ok");
    let auth_file_name = "antigravity-antigravity-route@example.com.json";
    let auth_payload: Value = serde_json::from_str(
        &fs::read_to_string(state.auth_dir.join(auth_file_name)).expect("auth file"),
    )
    .expect("json");

    assert_eq!(
        json!({
            "start": normalize_oauth_auth_url_response(start_payload, &["state"]),
            "callback": {
                "status": callback_status.as_u16(),
                "success_text": callback_text.contains("Authentication successful")
            },
            "auth_file_name": auth_file_name,
            "auth_file": normalize_antigravity_auth_file(auth_file_name, auth_payload),
            "status_after_callback": status_after_callback,
        }),
        expected_json(OAUTH_ANTIGRAVITY_ROUTE_FLOW_EXPECTED)
    );
}
