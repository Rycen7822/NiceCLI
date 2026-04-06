use super::*;

fn write_gemini_auth_file(
    state: &BackendAppState,
    file_name: &str,
    access_token: &str,
    project_id: &str,
) {
    fs::write(
        state.auth_dir.join(file_name),
        format!(
            r#"{{
  "type": "gemini",
  "provider": "gemini",
  "email": "gemini@example.com",
  "project_id": "{project_id}",
  "access_token": "{access_token}",
  "refresh_token": "gemini-refresh",
  "client_id": "gemini-client",
  "client_secret": "gemini-secret",
  "token_uri": "https://oauth2.googleapis.com/token",
  "expiry": "2099-01-01T00:00:00Z"
}}"#
        ),
    )
    .expect("write gemini auth");
}

fn write_antigravity_auth_file(
    state: &BackendAppState,
    file_name: &str,
    access_token: &str,
    base_url: &str,
    project_id: &str,
) {
    fs::write(
        state.auth_dir.join(file_name),
        format!(
            r#"{{
  "type": "antigravity",
  "provider": "antigravity",
  "email": "antigravity@example.com",
  "project_id": "{project_id}",
  "access_token": "{access_token}",
  "refresh_token": "antigravity-refresh",
  "base_url": "{base_url}",
  "expired": "2099-01-01T00:00:00Z"
}}"#
        ),
    )
    .expect("write antigravity auth");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedGeminiInternalRequest {
    path: String,
    query: Option<String>,
    authorization: Option<String>,
    api_key: Option<String>,
    user_agent: Option<String>,
    api_client: Option<String>,
    body: Value,
}

const TEST_VERTEX_SERVICE_ACCOUNT_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDPCixUvyzhATcZ\nlELovKhLygOVJb1nlzd9snwnq5l74Sl0Pc7u3yjxmKBij6Yq25CJe55x5l+zxkU5\naGz2zDExR4aBPVzJa/3duz7WFD0zrgem3BpBwvVSle9hwTzAaUTfbAQnWPqQypVz\nXpfiXSTgHAaDsv72Ug6ZM62kfQY9zB2e0OnS8rW8n87W0ayaIagQfK9EVg/s+Qer\nVgBidLBAZ/GuJpgXy6u5xH1GuRTIUod88BhrsyZYTnkkkSDwIo3jgbcTT2Bg69j0\ntUWO3Nh+DaX6qKuPydNFzZ5VGeVvhHnlLBJP3iPQZ5SOn36EtMdOwaGl1xRuYN7P\ncTkhAsHhAgMBAAECggEAPO1Lemz/8f7/jgF5ZzANfrAmOb/vawqvA8iAjNQMyM3L\n+y8tNFJrpB08JYUMa1RNxoegavhWdXsFaA1482/Hki7wQBwksAmdnaC6rxkpoYm3\nKr1e5LdQpksFNQk+rYjNAcqxtjqTEgTr0hWmMLEkZucYa61DJo2jSiZRFTTNn2Vn\n6c71kak5xC89dsAjkGNgziFLmf7quTLGWLjad4dFeWUC26SE8ttUhtkzTjx0xa9z\nDJTLGnI8DLusZFojsq5W/2NHB8LE6fcci37IXYg22Wt8nmEi8lZVujk4MPlaKdEu\nlJmoCmsRd7Sq3HvlMfwgop4wrdEbiagIYoQAIJif2QKBgQD5gYXzMJz4vlBSdwa1\nwYuNa/NgLmi5dJ1hzdLQ/VxKziLtH0lr+yXizOy5Vc+262TaWLhZiunzte4p6KEU\ndELQDONv5Mzq4I6xDhdKFWLQjEJKJ61zigokf12h0GRQZSPnj1RRLEu7kO9wRQNm\nM9PajeS4JUI1/I4QMssNN9mlPwKBgQDUbbHDu90BtPIYctU1Hr1TaIl/kj5Mg2HJ\nVvqiXj/cBr6oAQBCtlr7QeEAgQQWPDtfMmV3DMjLAWu8lYC08e5tPjQ2102KVOem\nEwOOTlQyBw3+N/IYaTuUid6ULuLv7K3R/+5dWE69Dsdn467QvqmeG+ko84k/5Cub\n2GcqSJAw3wKBgC1M+gAUlHuJOlYurDY15NuRfQe6hWMerDCEyUEOr0IZuTeqVY9Y\ncyGBqX1g+iyxAoeuUhJX6XBJWOudBBoNnc/edzDqrtX6XY4CC/J0fZN109dY6uIu\nbvb/dQWbK4t5QZKacGmojDuK7h5JOXvF7zIgTyWsBiB9MWH5hupoeIjLAoGBAJWL\n195czdyavuhZRyGLT2t9p3aoxLTmtRuh4PYXdct28BekBMPyTqCdo0HQkcj5hC6j\ncuzZki3gBTGQ6jf4LYq4hNeqwMrGtQGkVxeCqyFA+CfkyMlIpAoQ+SHG1Dplm4TA\nMNWECoJr+hN4JSSNZSmKqp0Kva+9+LlRImeRB/lvAoGBALV0f8tQOG8M1b0mPksd\nfYoqbvz/0LyNQQ80Cr9hVXVaSCdJTImblHCia2a6BTf18harmOxxgq062Jf5U13Q\nGWVTVJCCKohd6NlIbE2A8oJqzWVognBq9S3VwOw7BC5bIH34vFJb5ee8T45P7V8r\naTDCyHARNkaMC4TM6roML485\n-----END PRIVATE KEY-----\n";

async fn spawn_gemini_internal_server(
    recorded: Arc<Mutex<Vec<RecordedGeminiInternalRequest>>>,
) -> std::net::SocketAddr {
    async fn record_request(
        recorded: Arc<Mutex<Vec<RecordedGeminiInternalRequest>>>,
        request: Request<Body>,
    ) -> Response {
        let path = request.uri().path().to_string();
        let query = request.uri().query().map(str::to_string);
        let authorization = request
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let api_key = request
            .headers()
            .get("X-Goog-Api-Key")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let user_agent = request
            .headers()
            .get("User-Agent")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let api_client = request
            .headers()
            .get("X-Goog-Api-Client")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = to_bytes(request.into_body(), usize::MAX)
            .await
            .expect("request body");
        let parsed: Value = serde_json::from_slice(&body).expect("json body");

        recorded
            .lock()
            .expect("recorded lock")
            .push(RecordedGeminiInternalRequest {
                path: path.clone(),
                query,
                authorization,
                api_key,
                user_agent,
                api_client,
                body: parsed.clone(),
            });

        if path.ends_with("streamGenerateContent") {
            (
                StatusCode::OK,
                [("Content-Type", "text/event-stream")],
                "data: {\"chunk\":\"hello\"}\n\n",
            )
                .into_response()
        } else if path.ends_with("countTokens") {
            Json(json!({
                "totalTokens": 11,
                "path": path,
                "model": parsed.get("model").and_then(Value::as_str).unwrap_or_default()
            }))
            .into_response()
        } else {
            Json(json!({
                "ok": true,
                "path": path,
                "project": parsed.get("project").and_then(Value::as_str).unwrap_or_default(),
                "model": parsed.get("model").and_then(Value::as_str).unwrap_or_default()
            }))
            .into_response()
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/*path",
        post({
            let recorded = recorded.clone();
            move |request| {
                let recorded = recorded.clone();
                async move { record_request(recorded, request).await }
            }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    address
}

async fn spawn_vertex_service_account_server(
    recorded: Arc<Mutex<Vec<RecordedGeminiInternalRequest>>>,
) -> std::net::SocketAddr {
    async fn handle_request(
        recorded: Arc<Mutex<Vec<RecordedGeminiInternalRequest>>>,
        request: Request<Body>,
    ) -> Response {
        let path = request.uri().path().to_string();
        if path == "/oauth2/token" {
            return Json(json!({
                "access_token": "vertex-access-token",
                "token_type": "Bearer",
                "expires_in": 3600
            }))
            .into_response();
        }

        let query = request.uri().query().map(str::to_string);
        let authorization = request
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let api_key = request
            .headers()
            .get("X-Goog-Api-Key")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let user_agent = request
            .headers()
            .get("User-Agent")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let api_client = request
            .headers()
            .get("X-Goog-Api-Client")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = to_bytes(request.into_body(), usize::MAX)
            .await
            .expect("request body");
        let parsed: Value = serde_json::from_slice(&body).expect("json body");

        recorded
            .lock()
            .expect("recorded lock")
            .push(RecordedGeminiInternalRequest {
                path: path.clone(),
                query,
                authorization,
                api_key,
                user_agent,
                api_client,
                body: parsed.clone(),
            });

        if path.ends_with("streamGenerateContent") {
            (
                StatusCode::OK,
                [("Content-Type", "text/event-stream")],
                "data: {\"chunk\":\"hello\"}\n\n",
            )
                .into_response()
        } else if path.ends_with("countTokens") {
            Json(json!({
                "totalTokens": 11,
                "path": path,
                "model": parsed.get("model").and_then(Value::as_str).unwrap_or_default()
            }))
            .into_response()
        } else {
            Json(json!({
                "ok": true,
                "path": path,
                "project": "",
                "model": parsed.get("model").and_then(Value::as_str).unwrap_or_default()
            }))
            .into_response()
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/*path",
        post({
            let recorded = recorded.clone();
            move |request| {
                let recorded = recorded.clone();
                async move { handle_request(recorded, request).await }
            }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    address
}

#[test]
fn gemini_public_post_action_parser_accepts_supported_methods() {
    let generate = parse_gemini_public_action("/models/gemini-2.5-pro:generateContent")
        .expect("generate action");
    assert_eq!(generate.model, "gemini-2.5-pro");
    assert_eq!(generate.method.as_str(), "generateContent");

    let count = parse_gemini_public_action("gemini-2.5-pro:countTokens").expect("count action");
    assert_eq!(count.model, "gemini-2.5-pro");
    assert_eq!(count.method.as_str(), "countTokens");

    assert!(parse_gemini_public_action("/models/gemini-2.5-pro:unknown").is_none());
    assert!(parse_gemini_public_action("/models/:generateContent").is_none());
}

#[tokio::test]
async fn v1internal_generate_content_requires_loopback_when_connect_info_exists() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = Arc::new(load_fixture_state(&temp_dir));
    let request = Request::builder()
        .uri("/v1internal:generateContent")
        .body(Body::from(r#"{"model":"gemini-2.5-pro"}"#))
        .expect("request");

    let response = handle_v1internal_method(
        state,
        Some(ConnectInfo(
            "10.0.0.8:4567".parse::<SocketAddr>().expect("socket"),
        )),
        "generateContent".to_string(),
        request,
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "CLI reply only allow local access" })
    );
}

#[tokio::test]
async fn v1internal_generate_content_uses_gemini_auth_and_project() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    write_gemini_auth_file(
        &state,
        "gemini-gemini@example.com-demo-project.json",
        "gemini-access-token",
        "demo-project",
    );

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    let request = Request::builder()
        .uri("/v1internal:generateContent")
        .header("User-Agent", "NiceCLI-Test/0.1")
        .body(Body::from(
            r#"{"model":"gemini-2.5-pro","contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
        ))
        .expect("request");

    let response = handle_v1internal_method(
        Arc::new(state),
        Some(ConnectInfo(
            "127.0.0.1:4567".parse::<SocketAddr>().expect("socket"),
        )),
        "generateContent".to_string(),
        request,
        Some(&format!("http://{server}")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "ok": true,
            "path": "/v1internal:generateContent",
            "project": "demo-project",
            "model": "gemini-2.5-pro"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:generateContent");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer gemini-access-token")
    );
    assert_eq!(
        calls[0].api_client.as_deref(),
        Some("google-genai-sdk/1.41.0 gl-node/v22.19.0")
    );
    assert_eq!(calls[0].body["project"], json!("demo-project"));
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn v1internal_stream_generate_content_forwards_streaming_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    write_gemini_auth_file(
        &state,
        "gemini-gemini@example.com-demo-project.json",
        "gemini-access-token",
        "demo-project",
    );

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    let request = Request::builder()
        .uri("/v1internal:streamGenerateContent")
        .body(Body::from(
            r#"{"model":"gemini-2.5-pro","contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
        ))
        .expect("request");

    let response = handle_v1internal_method(
        Arc::new(state),
        Some(ConnectInfo(
            "127.0.0.1:4567".parse::<SocketAddr>().expect("socket"),
        )),
        "streamGenerateContent".to_string(),
        request,
        Some(&format!("http://{server}")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "data: {\"chunk\":\"hello\"}\n\n"
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:streamGenerateContent");
    assert_eq!(calls[0].query.as_deref(), Some("alt=sse"));
}

#[tokio::test]
async fn public_gemini_generate_content_uses_gemini_api_key_config() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\ngemini-api-key:\n  - api-key: gemini-upstream\n    base-url: http://{}\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            server
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:generateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "ok": true,
            "path": "/v1beta/models/gemini-2.5-pro:generateContent",
            "project": "",
            "model": "gemini-2.5-pro"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1beta/models/gemini-2.5-pro:generateContent"
    );
    assert_eq!(calls[0].api_key.as_deref(), Some("gemini-upstream"));
    assert_eq!(calls[0].authorization, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_generate_content_uses_gemini_auth_file_internal_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    write_gemini_auth_file(
        &state,
        "gemini-gemini@example.com-demo-project.json",
        "gemini-access-token",
        "demo-project",
    );

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state
            .auth_dir
            .join("gemini-gemini@example.com-demo-project.json"),
        format!(
            r#"{{
  "type": "gemini",
  "provider": "gemini",
  "email": "gemini@example.com",
  "project_id": "demo-project",
  "base_url": "http://{}",
  "access_token": "gemini-access-token",
  "expiry": "2099-01-01T00:00:00Z",
  "token": {{
    "access_token": "gemini-access-token",
    "refresh_token": "gemini-refresh",
    "token_type": "Bearer",
    "expiry": "2099-01-01T00:00:00Z",
    "token_uri": "https://oauth2.googleapis.com/token",
    "client_id": "gemini-client",
    "client_secret": "gemini-secret"
  }}
}}"#,
            server
        ),
    )
    .expect("write gemini auth with base url");
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\ngemini-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:generateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "ok": true,
            "path": "/v1internal:generateContent",
            "project": "demo-project",
            "model": "gemini-2.5-pro"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:generateContent");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer gemini-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["project"], json!("demo-project"));
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_generate_content_uses_antigravity_auth_file_internal_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    write_antigravity_auth_file(
        &state,
        "antigravity-antigravity@example.com.json",
        "antigravity-access-token",
        &format!("http://{server}"),
        "antigravity-project",
    );
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\ngemini-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-flash:generateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}],"toolConfig":{"functionCallingConfig":{"mode":"AUTO"}}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "ok": true,
            "path": "/v1internal:generateContent",
            "project": "antigravity-project",
            "model": "gemini-2.5-flash"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:generateContent");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer antigravity-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["project"], json!("antigravity-project"));
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-flash"));
    assert_eq!(calls[0].body["userAgent"], json!("antigravity"));
    assert_eq!(calls[0].body["requestType"], json!("agent"));
    assert_eq!(
        calls[0].body["request"]["contents"][0]["parts"][0]["text"],
        json!("hello")
    );
    assert_eq!(
        calls[0].body["request"]["toolConfig"]["functionCallingConfig"]["mode"],
        json!("AUTO")
    );
}

#[tokio::test]
async fn public_gemini_stream_generate_content_uses_antigravity_auth_file_internal_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    write_antigravity_auth_file(
        &state,
        "antigravity-antigravity@example.com.json",
        "antigravity-access-token",
        &format!("http://{server}"),
        "antigravity-project",
    );
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\ngemini-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-flash:streamGenerateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}],"toolConfig":{"functionCallingConfig":{"mode":"AUTO"}}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "data: {\"chunk\":\"hello\"}\n\n"
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:streamGenerateContent");
    assert_eq!(calls[0].query, None);
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer antigravity-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["project"], json!("antigravity-project"));
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-flash"));
    assert_eq!(calls[0].body["userAgent"], json!("antigravity"));
    assert_eq!(calls[0].body["requestType"], json!("agent"));
    assert_eq!(
        calls[0].body["request"]["contents"][0]["parts"][0]["text"],
        json!("hello")
    );
    assert_eq!(
        calls[0].body["request"]["toolConfig"]["functionCallingConfig"]["mode"],
        json!("AUTO")
    );
}

#[tokio::test]
async fn public_gemini_stream_generate_content_uses_gemini_auth_file_internal_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    write_gemini_auth_file(
        &state,
        "gemini-gemini@example.com-demo-project.json",
        "gemini-access-token",
        "demo-project",
    );

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state
            .auth_dir
            .join("gemini-gemini@example.com-demo-project.json"),
        format!(
            r#"{{
  "type": "gemini",
  "provider": "gemini",
  "email": "gemini@example.com",
  "project_id": "demo-project",
  "base_url": "http://{}",
  "access_token": "gemini-access-token",
  "expiry": "2099-01-01T00:00:00Z",
  "token": {{
    "access_token": "gemini-access-token",
    "refresh_token": "gemini-refresh",
    "token_type": "Bearer",
    "expiry": "2099-01-01T00:00:00Z",
    "token_uri": "https://oauth2.googleapis.com/token",
    "client_id": "gemini-client",
    "client_secret": "gemini-secret"
  }}
}}"#,
            server
        ),
    )
    .expect("write gemini auth with base url");
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\ngemini-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:streamGenerateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "data: {\"chunk\":\"hello\"}\n\n"
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:streamGenerateContent");
    assert_eq!(calls[0].query.as_deref(), Some("alt=sse"));
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer gemini-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["project"], json!("demo-project"));
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_count_tokens_uses_gemini_auth_file_internal_route() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    write_gemini_auth_file(
        &state,
        "gemini-gemini@example.com-demo-project.json",
        "gemini-access-token",
        "demo-project",
    );

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state
            .auth_dir
            .join("gemini-gemini@example.com-demo-project.json"),
        format!(
            r#"{{
  "type": "gemini",
  "provider": "gemini",
  "email": "gemini@example.com",
  "project_id": "demo-project",
  "base_url": "http://{}",
  "access_token": "gemini-access-token",
  "expiry": "2099-01-01T00:00:00Z",
  "token": {{
    "access_token": "gemini-access-token",
    "refresh_token": "gemini-refresh",
    "token_type": "Bearer",
    "expiry": "2099-01-01T00:00:00Z",
    "token_uri": "https://oauth2.googleapis.com/token",
    "client_id": "gemini-client",
    "client_secret": "gemini-secret"
  }}
}}"#,
            server
        ),
    )
    .expect("write gemini auth with base url");
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:countTokens")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "totalTokens": 11,
            "path": "/v1internal:countTokens",
            "model": ""
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1internal:countTokens");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer gemini-access-token")
    );
    assert_eq!(calls[0].body.get("project"), None);
    assert_eq!(calls[0].body.get("model"), None);
}

#[tokio::test]
async fn public_gemini_stream_generate_content_uses_gemini_api_key_config() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\ngemini-api-key:\n  - api-key: gemini-upstream\n    base-url: http://{}\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            server
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:streamGenerateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "data: {\"chunk\":\"hello\"}\n\n"
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1beta/models/gemini-2.5-pro:streamGenerateContent"
    );
    assert_eq!(calls[0].query.as_deref(), Some("alt=sse"));
    assert_eq!(calls[0].api_key.as_deref(), Some("gemini-upstream"));
    assert_eq!(calls[0].authorization, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_count_tokens_uses_vertex_api_key_config() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\nvertex-api-key:\n  - api-key: vertex-upstream\n    base-url: http://{}\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            server
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:countTokens")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "totalTokens": 11,
            "path": "/v1/publishers/google/models/gemini-2.5-pro:countTokens",
            "model": "gemini-2.5-pro"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1/publishers/google/models/gemini-2.5-pro:countTokens"
    );
    assert_eq!(calls[0].api_key.as_deref(), Some("vertex-upstream"));
    assert_eq!(calls[0].authorization, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_stream_generate_content_uses_vertex_api_key_config() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_gemini_internal_server(recorded.clone()).await;
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\nvertex-api-key:\n  - api-key: vertex-upstream\n    base-url: http://{}\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            server
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:streamGenerateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "data: {\"chunk\":\"hello\"}\n\n"
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1/publishers/google/models/gemini-2.5-pro:streamGenerateContent"
    );
    assert_eq!(calls[0].query.as_deref(), Some("alt=sse"));
    assert_eq!(calls[0].api_key.as_deref(), Some("vertex-upstream"));
    assert_eq!(calls[0].authorization, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_generate_content_uses_vertex_service_account_auth_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_vertex_service_account_server(recorded.clone()).await;
    fs::write(
        state.auth_dir.join("vertex-vertex-demo-project.json"),
        format!(
            r#"{{
  "id": "vertex-vertex-demo-project.json",
  "provider": "vertex",
  "type": "vertex",
  "email": "vertex@example.com",
  "project_id": "vertex-demo-project",
  "location": "us-central1",
  "base_url": "http://{}",
  "service_account": {{
    "project_id": "vertex-demo-project",
    "client_email": "vertex@example.com",
    "private_key": "{}",
    "token_uri": "http://{}/oauth2/token"
  }}
}}"#,
            server,
            TEST_VERTEX_SERVICE_ACCOUNT_PRIVATE_KEY.replace('\n', "\\n"),
            server
        ),
    )
    .expect("write vertex auth");
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\nvertex-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:generateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "ok": true,
            "path": "/v1/projects/vertex-demo-project/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent",
            "project": "",
            "model": "gemini-2.5-pro"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1/projects/vertex-demo-project/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent"
    );
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer vertex-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_count_tokens_uses_vertex_service_account_auth_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_vertex_service_account_server(recorded.clone()).await;
    fs::write(
        state.auth_dir.join("vertex-vertex-demo-project.json"),
        format!(
            r#"{{
  "id": "vertex-vertex-demo-project.json",
  "provider": "vertex",
  "type": "vertex",
  "email": "vertex@example.com",
  "project_id": "vertex-demo-project",
  "location": "us-central1",
  "base_url": "http://{}",
  "service_account": {{
    "project_id": "vertex-demo-project",
    "client_email": "vertex@example.com",
    "private_key": "{}",
    "token_uri": "http://{}/oauth2/token"
  }}
}}"#,
            server,
            TEST_VERTEX_SERVICE_ACCOUNT_PRIVATE_KEY.replace('\n', "\\n"),
            server
        ),
    )
    .expect("write vertex auth");
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\nvertex-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:countTokens")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "totalTokens": 11,
            "path": "/v1/projects/vertex-demo-project/locations/us-central1/publishers/google/models/gemini-2.5-pro:countTokens",
            "model": "gemini-2.5-pro"
        })
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1/projects/vertex-demo-project/locations/us-central1/publishers/google/models/gemini-2.5-pro:countTokens"
    );
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer vertex-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}

#[tokio::test]
async fn public_gemini_stream_generate_content_uses_vertex_service_account_auth_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let server = spawn_vertex_service_account_server(recorded.clone()).await;
    fs::write(
        state.auth_dir.join("vertex-vertex-demo-project.json"),
        format!(
            r#"{{
  "id": "vertex-vertex-demo-project.json",
  "provider": "vertex",
  "type": "vertex",
  "email": "vertex@example.com",
  "project_id": "vertex-demo-project",
  "location": "us-central1",
  "base_url": "http://{}",
  "service_account": {{
    "project_id": "vertex-demo-project",
    "client_email": "vertex@example.com",
    "private_key": "{}",
    "token_uri": "http://{}/oauth2/token"
  }}
}}"#,
            server,
            TEST_VERTEX_SERVICE_ACCOUNT_PRIVATE_KEY.replace('\n', "\\n"),
            server
        ),
    )
    .expect("write vertex auth");
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\napi-keys:\n  - public-key\nvertex-api-key: []\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/gemini-2.5-pro:streamGenerateContent")
                .header("Authorization", "Bearer public-key")
                .header("User-Agent", "NiceCLI-Test/0.1")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8_lossy(&body),
        "data: {\"chunk\":\"hello\"}\n\n"
    );

    let calls = recorded.lock().expect("recorded lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].path,
        "/v1/projects/vertex-demo-project/locations/us-central1/publishers/google/models/gemini-2.5-pro:streamGenerateContent"
    );
    assert_eq!(calls[0].query.as_deref(), Some("alt=sse"));
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer vertex-access-token")
    );
    assert_eq!(calls[0].api_key, None);
    assert_eq!(calls[0].body["model"], json!("gemini-2.5-pro"));
}
