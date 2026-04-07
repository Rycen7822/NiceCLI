use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedApiKeyRequest {
    authorization: Option<String>,
    x_test: Option<String>,
    body: Vec<u8>,
}

fn write_codex_auth_file(
    state: &BackendAppState,
    file_name: &str,
    access_token: &str,
    base_url: &str,
) {
    write_codex_auth_file_with_websockets(state, file_name, access_token, base_url, false);
}

fn write_codex_auth_file_with_websockets(
    state: &BackendAppState,
    file_name: &str,
    access_token: &str,
    base_url: &str,
    websockets: bool,
) {
    let websocket_field = if websockets {
        ",\n  \"websockets\": true"
    } else {
        ""
    };
    fs::write(
        state.auth_dir.join(file_name),
        format!(
            r#"{{
  "type": "codex",
  "provider": "codex",
  "email": "demo@example.com",
  "access_token": "{access_token}",
  "base_url": "{base_url}",
  "models": [{{"name": "gpt-5"}}]{websocket_field}
}}"#
        ),
    )
    .expect("write auth");
}

async fn spawn_router_server(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    (base_url, server)
}

async fn connect_responses_websocket(base_url: &str) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let ws_url = format!(
        "{}{}",
        base_url.replacen("http://", "ws://", 1),
        "/v1/responses"
    );
    let (socket, _) = connect_async(ws_url).await.expect("connect websocket");
    socket
}

async fn next_websocket_json(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Value {
    let message = socket
        .next()
        .await
        .expect("websocket message")
        .expect("websocket read");
    match message {
        WsMessage::Text(text) => serde_json::from_str(text.as_ref()).expect("json text"),
        WsMessage::Binary(bytes) => serde_json::from_slice(&bytes).expect("json bytes"),
        other => panic!("unexpected websocket message: {other:?}"),
    }
}

#[tokio::test]
async fn forwards_public_v1_responses_through_codex_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    Json(json!({ "id": "resp_ok", "status": "completed" })),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_codex_auth_file(
        &state,
        "codex-demo@example.com-team.json",
        "token-a",
        &base_url,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"model":"gpt-5","input":"hello"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": "resp_ok", "status": "completed" })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
    assert_eq!(requests[0].body, br#"{"model":"gpt-5","input":"hello"}"#);

    server.abort();
}

#[tokio::test]
async fn forwards_public_v1_responses_through_codex_api_key_entry_with_prefix_alias_and_headers() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedApiKeyRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedApiKeyRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    x_test: headers
                        .get("x-test")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    Json(json!({ "id": "resp_api_key", "status": "completed" })),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\ncodex-api-key:\n  - api-key: codex-key\n    prefix: /lab/\n    base-url: {}\n    headers:\n      X-Test: demo\n    models:\n      - name: gpt-5\n        alias: team-gpt5\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            base_url
        ),
    )
    .expect("config file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"model":"lab/team-gpt5","input":"hello"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": "resp_api_key", "status": "completed" })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer codex-key")
    );
    assert_eq!(requests[0].x_test.as_deref(), Some("demo"));
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("gpt-5"));
    assert_eq!(upstream_body["input"].as_str(), Some("hello"));

    server.abort();
}

#[tokio::test]
async fn prefers_codex_api_key_entry_for_prefixed_model_when_official_codex_also_exists() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let official_requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let official_requests_state = official_requests.clone();
    let official_app = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = official_requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    Json(json!({ "id": "resp_official", "status": "completed" })),
                )
            }
        }),
    );
    let official_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("official listener");
    let official_base_url = format!(
        "http://{}",
        official_listener.local_addr().expect("official addr")
    );
    let official_server = tokio::spawn(async move {
        axum::serve(official_listener, official_app)
            .await
            .expect("official serve");
    });

    let api_requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let api_requests_state = api_requests.clone();
    let api_app = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = api_requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    Json(json!({ "id": "resp_api_key", "status": "completed" })),
                )
            }
        }),
    );
    let api_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("api listener");
    let api_base_url = format!("http://{}", api_listener.local_addr().expect("api addr"));
    let api_server = tokio::spawn(async move {
        axum::serve(api_listener, api_app).await.expect("api serve");
    });

    write_codex_auth_file(
        &state,
        "codex-demo@example.com-team.json",
        "official-token",
        &official_base_url,
    );
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\ncodex-api-key:\n  - api-key: codex-key\n    prefix: /lab/\n    base-url: {}\n    models:\n      - name: gpt-5\n        alias: team-gpt5\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            api_base_url
        ),
    )
    .expect("config file");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"model":"lab/team-gpt5","input":"hello"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": "resp_api_key", "status": "completed" })
    );

    assert!(official_requests.lock().expect("lock").is_empty());
    let api_requests = api_requests.lock().expect("lock");
    assert_eq!(api_requests.len(), 1);
    assert_eq!(
        api_requests[0].authorization.as_deref(),
        Some("Bearer codex-key")
    );

    official_server.abort();
    api_server.abort();
}

#[tokio::test]
async fn forwards_streaming_public_v1_responses_through_codex_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    [("Content-Type", "text/event-stream")],
                    concat!(
                        "event: response.created\n",
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_http_stream\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
                        "event: response.completed\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_http_stream\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
                    ),
                )
            }
        }),
    );
    let (base_url, server) = spawn_router_server(app).await;

    write_codex_auth_file(
        &state,
        "codex-demo@example.com-team.json",
        "token-a",
        &base_url,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"gpt-5","input":"hello","stream":true}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body_text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_text.contains("event: response.created"));
    assert!(body_text.contains("event: response.completed"));

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("gpt-5"));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));
    assert_eq!(upstream_body["input"].as_str(), Some("hello"));

    server.abort();
}

#[tokio::test]
async fn prewarms_public_v1_responses_websocket_locally() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let (base_url, server) = spawn_router_server(build_router(state)).await;

    let mut socket = connect_responses_websocket(&base_url).await;
    socket
        .send(WsMessage::Text(
            r#"{"type":"response.create","model":"gpt-5","generate":false,"input":[]}"#.into(),
        ))
        .await
        .expect("send websocket request");

    let created = next_websocket_json(&mut socket).await;
    let completed = next_websocket_json(&mut socket).await;

    assert_eq!(created["type"].as_str(), Some("response.created"));
    assert_eq!(created["sequence_number"].as_i64(), Some(0));
    assert_eq!(created["response"]["model"].as_str(), Some("gpt-5"));
    assert_eq!(created["response"]["status"].as_str(), Some("in_progress"));
    assert!(created["response"]["id"]
        .as_str()
        .is_some_and(|value| value.starts_with("resp_prewarm_")));

    assert_eq!(completed["type"].as_str(), Some("response.completed"));
    assert_eq!(completed["sequence_number"].as_i64(), Some(1));
    assert_eq!(completed["response"]["model"].as_str(), Some("gpt-5"));
    assert_eq!(completed["response"]["status"].as_str(), Some("completed"));
    assert_eq!(
        completed["response"]["id"].as_str(),
        created["response"]["id"].as_str()
    );
    assert_eq!(
        completed["response"]["usage"]["total_tokens"].as_i64(),
        Some(0)
    );

    server.abort();
}

#[tokio::test]
async fn rejects_append_before_create_on_public_v1_responses_websocket() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let (base_url, server) = spawn_router_server(build_router(state)).await;

    let mut socket = connect_responses_websocket(&base_url).await;
    socket
        .send(WsMessage::Text(
            r#"{"type":"response.append","input":[{"role":"user","content":[{"type":"input_text","text":"hello"}]}]}"#
                .into(),
        ))
        .await
        .expect("send websocket request");

    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "error",
            "status": 400,
            "error": {
                "message": "websocket request received before response.create",
                "type": "invalid_request_error"
            }
        })
    );

    server.abort();
}

#[tokio::test]
async fn forwards_public_v1_responses_websocket_stream_through_codex_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let upstream = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    [("Content-Type", "text/event-stream")],
                    concat!(
                        "event: response.created\n",
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_ws_stream\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
                        "event: response.completed\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ws_stream\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
                    ),
                )
            }
        }),
    );
    let (upstream_base_url, upstream_server) = spawn_router_server(upstream).await;
    write_codex_auth_file(
        &state,
        "codex-demo@example.com-team.json",
        "token-a",
        &upstream_base_url,
    );

    let (base_url, server) = spawn_router_server(build_router(state)).await;
    let mut socket = connect_responses_websocket(&base_url).await;
    socket
        .send(WsMessage::Text(
            r#"{"type":"response.create","model":"gpt-5","generate":true,"input":[]}"#.into(),
        ))
        .await
        .expect("send websocket request");

    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "response.created",
            "response": {
                "id": "resp_ws_stream",
                "object": "response",
                "created_at": 1730000260,
                "model": "gpt-5",
                "status": "in_progress",
                "output": []
            }
        })
    );
    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "response.completed",
            "response": {
                "id": "resp_ws_stream",
                "object": "response",
                "created_at": 1730000260,
                "model": "gpt-5",
                "status": "completed",
                "output": [],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 2,
                    "total_tokens": 3
                }
            }
        })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("gpt-5"));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));
    assert_eq!(upstream_body["generate"].as_bool(), Some(true));
    assert_eq!(upstream_body["input"].as_array().map(Vec::len), Some(0));
    assert!(upstream_body.get("type").is_none());

    server.abort();
    upstream_server.abort();
}

#[tokio::test]
async fn forwards_public_v1_responses_websocket_stream_through_codex_api_key_entry() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let upstream = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    [("Content-Type", "text/event-stream")],
                    concat!(
                        "event: response.created\n",
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_ws_api_key\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
                        "event: response.completed\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ws_api_key\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n"
                    ),
                )
            }
        }),
    );
    let (upstream_base_url, upstream_server) = spawn_router_server(upstream).await;
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\ncodex-api-key:\n  - api-key: codex-key\n    prefix: /lab/\n    base-url: {}\n    models:\n      - name: gpt-5\n        alias: team-gpt5\n",
            state.auth_dir.to_string_lossy().replace('\\', "/"),
            upstream_base_url
        ),
    )
    .expect("config file");

    let (base_url, server) = spawn_router_server(build_router(state)).await;
    let mut socket = connect_responses_websocket(&base_url).await;
    socket
        .send(WsMessage::Text(
            r#"{"type":"response.create","model":"lab/team-gpt5","generate":true,"input":[]}"#
                .into(),
        ))
        .await
        .expect("send websocket request");

    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "response.created",
            "response": {
                "id": "resp_ws_api_key",
                "object": "response",
                "created_at": 1730000260,
                "model": "gpt-5",
                "status": "in_progress",
                "output": []
            }
        })
    );
    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "response.completed",
            "response": {
                "id": "resp_ws_api_key",
                "object": "response",
                "created_at": 1730000260,
                "model": "gpt-5",
                "status": "completed",
                "output": [],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 2,
                    "total_tokens": 3
                }
            }
        })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer codex-key")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("gpt-5"));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));

    server.abort();
    upstream_server.abort();
}

#[tokio::test]
async fn keeps_previous_response_id_for_websocket_enabled_codex_auth() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let upstream = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                let mut requests = requests.lock().expect("lock");
                requests.push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                let call_index = requests.len();
                drop(requests);

                let body = if call_index == 1 {
                    concat!(
                        "event: response.created\n",
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_ws_first\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
                        "event: response.completed\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ws_first\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n"
                    )
                } else {
                    concat!(
                        "event: response.created\n",
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_ws_second\",\"object\":\"response\",\"created_at\":1730000300,\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
                        "event: response.completed\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_ws_second\",\"object\":\"response\",\"created_at\":1730000300,\"model\":\"gpt-5\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":2,\"output_tokens\":1,\"total_tokens\":3}}}\n\n"
                    )
                };

                (StatusCode::OK, [("Content-Type", "text/event-stream")], body)
            }
        }),
    );
    let (upstream_base_url, upstream_server) = spawn_router_server(upstream).await;
    write_codex_auth_file_with_websockets(
        &state,
        "codex-demo@example.com-team.json",
        "token-a",
        &upstream_base_url,
        true,
    );

    let (base_url, server) = spawn_router_server(build_router(state)).await;
    let mut socket = connect_responses_websocket(&base_url).await;

    socket
        .send(WsMessage::Text(
            r#"{"type":"response.create","model":"gpt-5","generate":true,"input":[]}"#.into(),
        ))
        .await
        .expect("send first websocket request");
    let _ = next_websocket_json(&mut socket).await;
    let _ = next_websocket_json(&mut socket).await;

    socket
        .send(WsMessage::Text(
            r#"{"type":"response.create","previous_response_id":"resp_ws_first","input":[{"role":"user","content":[{"type":"input_text","text":"again"}]}]}"#
                .into(),
        ))
        .await
        .expect("send second websocket request");
    let _ = next_websocket_json(&mut socket).await;
    let _ = next_websocket_json(&mut socket).await;

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 2);
    let second_body: Value = serde_json::from_slice(&requests[1].body).expect("body json");
    assert_eq!(second_body["model"].as_str(), Some("gpt-5"));
    assert_eq!(
        second_body["previous_response_id"].as_str(),
        Some("resp_ws_first")
    );
    assert_eq!(second_body["stream"].as_bool(), Some(true));
    assert_eq!(second_body["input"].as_array().map(Vec::len), Some(1));
    assert!(second_body.get("type").is_none());

    server.abort();
    upstream_server.abort();
}

#[tokio::test]
async fn returns_error_when_public_v1_responses_websocket_stream_ends_early() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let upstream = Router::new().route(
        "/responses",
        post(|_: HeaderMap, _: Bytes| async move {
            (
                StatusCode::OK,
                [("Content-Type", "text/event-stream")],
                concat!(
                    "event: response.created\n",
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_ws_stream\",\"object\":\"response\",\"created_at\":1730000260,\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n"
                ),
            )
        }),
    );
    let (upstream_base_url, upstream_server) = spawn_router_server(upstream).await;
    write_codex_auth_file(
        &state,
        "codex-demo@example.com-team.json",
        "token-a",
        &upstream_base_url,
    );

    let (base_url, server) = spawn_router_server(build_router(state)).await;
    let mut socket = connect_responses_websocket(&base_url).await;
    socket
        .send(WsMessage::Text(
            r#"{"type":"response.create","model":"gpt-5","generate":true,"input":[]}"#.into(),
        ))
        .await
        .expect("send websocket request");

    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "response.created",
            "response": {
                "id": "resp_ws_stream",
                "object": "response",
                "created_at": 1730000260,
                "model": "gpt-5",
                "status": "in_progress",
                "output": []
            }
        })
    );
    assert_eq!(
        next_websocket_json(&mut socket).await,
        json!({
            "type": "error",
            "status": 408,
            "error": {
                "message": "stream closed before response.completed",
                "type": "invalid_request_error"
            }
        })
    );

    server.abort();
    upstream_server.abort();
}

#[tokio::test]
async fn rejects_streaming_compact_requests_and_strips_stream_flag() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let stream_error = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses/compact")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"gpt-5","input":"hello","stream":true}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(stream_error.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(stream_error).await,
        json!({
            "error": {
                "message": "Streaming not supported for compact responses",
                "type": "invalid_request_error"
            }
        })
    );

    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/responses/compact",
        post(move |headers: HeaderMap, body: Bytes| {
            let requests = requests_state.clone();
            async move {
                requests.lock().expect("lock").push(RecordedPublicRequest {
                    authorization: headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: body.to_vec(),
                });
                (
                    StatusCode::OK,
                    Json(json!({ "id": "resp_compact", "status": "completed" })),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_codex_auth_file(
        &state,
        "codex-demo@example.com-team.json",
        "token-a",
        &base_url,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses/compact")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"gpt-5","input":"hello","stream":false}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": "resp_compact", "status": "completed" })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
    assert_eq!(requests[0].body, br#"{"input":"hello","model":"gpt-5"}"#);

    server.abort();
}
