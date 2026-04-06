use super::*;

#[tokio::test]
async fn returns_openai_and_claude_public_model_shapes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ncodex-api-key:\n  - api-key: codex-1\n    models:\n      - name: gpt-test\n        created: 1730000000\n        owned_by: openai\nclaude-api-key:\n  - api-key: claude-1\n    models:\n      - name: claude-test\n        created_at: 1730000100\n        owned_by: anthropic\n        type: model\n        display_name: Claude Test\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let openai_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(openai_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(openai_response).await,
        json!({
            "object": "list",
            "data": [
                {
                    "id": "gpt-test",
                    "object": "model",
                    "created": 1730000000i64,
                    "owned_by": "openai"
                }
            ]
        })
    );

    let claude_response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("User-Agent", "claude-cli/1.0")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(claude_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(claude_response).await,
        json!({
            "data": [
                {
                    "id": "claude-test",
                    "object": "model",
                    "created_at": 1730000100i64,
                    "owned_by": "anthropic",
                    "type": "model",
                    "display_name": "Claude Test"
                }
            ],
            "has_more": false,
            "first_id": "claude-test",
            "last_id": "claude-test"
        })
    );
}

#[tokio::test]
async fn returns_openai_public_models_from_qwen_and_kimi_auth_snapshots() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    write_qwen_auth_file(
        &state,
        "qwen-demo@example.com.json",
        "qwen-token",
        "https://qwen.local",
        None,
    );
    write_kimi_auth_file(
        &state,
        "kimi-demo@example.com.json",
        "kimi-token",
        "https://kimi.local",
    );

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
    assert!(ids.contains(&"qwen3-coder-plus"));
    assert!(ids.contains(&"kimi-k2"));
}

#[tokio::test]
async fn forwards_public_v1_chat_completions_through_qwen_runtime_with_prefix_and_alias() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\noauth-model-alias:\n  qwen:\n    - name: qwen3-coder-plus\n      alias: team-qwen\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
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
                (StatusCode::OK, Json(json!({ "id": "qwen-chat-ok" })))
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_qwen_auth_file(
        &state,
        "qwen-demo@example.com.json",
        "qwen-token",
        &base_url,
        Some("lab"),
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .header("User-Agent", "NiceCLI-Test")
                .body(Body::from(
                    r#"{"model":"lab/team-qwen","messages":[{"role":"user","content":"hello"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": "qwen-chat-ok" })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer qwen-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("qwen3-coder-plus"));

    server.abort();
}

#[tokio::test]
async fn forwards_public_v1_chat_completions_through_kimi_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
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
                (StatusCode::OK, Json(json!({ "id": "kimi-chat-ok" })))
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_kimi_auth_file(
        &state,
        "kimi-demo@example.com.json",
        "kimi-token",
        &base_url,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"kimi-k2","messages":[{"role":"user","content":"hello"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": "kimi-chat-ok" })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer kimi-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("k2"));

    server.abort();
}

#[tokio::test]
async fn forwards_streaming_public_v1_chat_completions_through_qwen_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
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
                    "data: {\"id\":\"qwen-chat-chunk\"}\n\n",
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_qwen_auth_file(
        &state,
        "qwen-demo@example.com.json",
        "qwen-token",
        &base_url,
        None,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .header("User-Agent", "NiceCLI-Test")
                .body(Body::from(
                    r#"{"model":"qwen3-coder-plus","messages":[],"stream":true}"#,
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
        "data: {\"id\":\"qwen-chat-chunk\"}\n\n"
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer qwen-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("qwen3-coder-plus"));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));

    server.abort();
}

#[tokio::test]
async fn forwards_streaming_public_v1_chat_completions_through_kimi_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
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
                    "data: {\"id\":\"kimi-chat-chunk\"}\n\n",
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_kimi_auth_file(
        &state,
        "kimi-demo@example.com.json",
        "kimi-token",
        &base_url,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"kimi-k2","messages":[],"stream":true}"#,
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
        "data: {\"id\":\"kimi-chat-chunk\"}\n\n"
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer kimi-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("k2"));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));

    server.abort();
}

#[tokio::test]
async fn forwards_public_v1_completions_through_qwen_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
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
                    Json(json!({
                        "id": "chatcmpl-completions",
                        "object": "chat.completion",
                        "created": 1730000400,
                        "model": "qwen3-coder-plus",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": "hello completion"
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": 4,
                            "completion_tokens": 2,
                            "total_tokens": 6
                        }
                    })),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_qwen_auth_file(
        &state,
        "qwen-demo@example.com.json",
        "qwen-token",
        &base_url,
        None,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen3-coder-plus","prompt":"hello","max_tokens":32}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "id": "chatcmpl-completions",
            "object": "text_completion",
            "created": 1730000400,
            "model": "qwen3-coder-plus",
            "choices": [{
                "index": 0,
                "text": "hello completion",
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 4,
                "completion_tokens": 2,
                "total_tokens": 6
            }
        })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer qwen-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("qwen3-coder-plus"));
    assert_eq!(upstream_body["messages"][0]["role"].as_str(), Some("user"));
    assert_eq!(
        upstream_body["messages"][0]["content"].as_str(),
        Some("hello")
    );
    assert_eq!(upstream_body["max_tokens"].as_i64(), Some(32));
    assert!(upstream_body.get("prompt").is_none());

    server.abort();
}

#[tokio::test]
async fn forwards_streaming_public_v1_completions_through_qwen_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
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
                        "data: {\"id\":\"chatcmpl-completions-stream\",\"object\":\"chat.completion.chunk\",\"created\":1730000450,\"model\":\"qwen3-coder-plus\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
                        "data: {\"id\":\"chatcmpl-completions-stream\",\"object\":\"chat.completion.chunk\",\"created\":1730000450,\"model\":\"qwen3-coder-plus\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello \"},\"finish_reason\":null}]}\n\n",
                        "data: {\"id\":\"chatcmpl-completions-stream\",\"object\":\"chat.completion.chunk\",\"created\":1730000450,\"model\":\"qwen3-coder-plus\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"world\"},\"finish_reason\":null}]}\n\n",
                        "data: {\"id\":\"chatcmpl-completions-stream\",\"object\":\"chat.completion.chunk\",\"created\":1730000450,\"model\":\"qwen3-coder-plus\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_qwen_auth_file(
        &state,
        "qwen-demo@example.com.json",
        "qwen-token",
        &base_url,
        None,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/completions")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"model":"qwen3-coder-plus","prompt":"hello","max_tokens":32,"stream":true}"#,
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
    let body_text = String::from_utf8_lossy(&body);
    assert!(!body_text.contains("\"role\":\"assistant\""));

    let data_lines = body_text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .collect::<Vec<_>>();
    assert_eq!(data_lines.len(), 4);
    assert_eq!(data_lines[3], "[DONE]");

    let first_chunk: Value = serde_json::from_str(data_lines[0]).expect("first chunk");
    assert_eq!(first_chunk["object"].as_str(), Some("text_completion"));
    assert_eq!(first_chunk["choices"][0]["text"].as_str(), Some("hello "));
    assert_eq!(
        first_chunk["choices"][0]["finish_reason"].as_str(),
        Some("")
    );

    let second_chunk: Value = serde_json::from_str(data_lines[1]).expect("second chunk");
    assert_eq!(second_chunk["choices"][0]["text"].as_str(), Some("world"));
    assert_eq!(
        second_chunk["choices"][0]["finish_reason"].as_str(),
        Some("")
    );

    let final_chunk: Value = serde_json::from_str(data_lines[2]).expect("final chunk");
    assert_eq!(final_chunk["choices"][0]["text"].as_str(), Some(""));
    assert_eq!(
        final_chunk["choices"][0]["finish_reason"].as_str(),
        Some("stop")
    );
    assert_eq!(final_chunk["usage"]["total_tokens"].as_i64(), Some(6));

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer qwen-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["messages"][0]["role"].as_str(), Some("user"));
    assert_eq!(
        upstream_body["messages"][0]["content"].as_str(),
        Some("hello")
    );
    assert_eq!(upstream_body["max_tokens"].as_i64(), Some(32));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));
    assert!(upstream_body.get("prompt").is_none());

    server.abort();
}

#[tokio::test]
async fn forwards_public_v1_messages_through_claude_runtime_with_prefix_and_alias() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nforce-model-prefix: true\noauth-model-alias:\n  claude:\n    - name: claude-sonnet-4\n      alias: team-sonnet\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("config file");

    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/messages",
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
                    Json(json!({
                        "id": "msg_test",
                        "type": "message",
                        "role": "assistant",
                        "model": "claude-sonnet-4",
                        "content": [{
                            "type": "text",
                            "text": "hello claude"
                        }],
                        "stop_reason": "end_turn",
                        "usage": {
                            "input_tokens": 5,
                            "output_tokens": 3
                        }
                    })),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_claude_auth_file(
        &state,
        "claude-demo@example.com.json",
        "claude-token",
        &base_url,
        Some("lab"),
        "claude-sonnet-4",
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("Content-Type", "application/json")
                .header("User-Agent", "claude-cli/1.0")
                .body(Body::from(
                    r#"{"model":"lab/team-sonnet","messages":[{"role":"user","content":"hello"}],"max_tokens":32}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4",
            "content": [{
                "type": "text",
                "text": "hello claude"
            }],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 5,
                "output_tokens": 3
            }
        })
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer claude-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("claude-sonnet-4"));
    assert_eq!(upstream_body["messages"][0]["role"].as_str(), Some("user"));
    assert_eq!(
        upstream_body["messages"][0]["content"].as_str(),
        Some("hello")
    );
    assert_eq!(upstream_body["max_tokens"].as_i64(), Some(32));

    server.abort();
}

#[tokio::test]
async fn forwards_streaming_public_v1_messages_through_claude_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/messages",
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
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello claude\"}}\n\n",
                        "event: message_delta\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n"
                    ),
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_claude_auth_file(
        &state,
        "claude-demo@example.com.json",
        "claude-token",
        &base_url,
        None,
        "claude-sonnet-4",
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("Content-Type", "application/json")
                .header("User-Agent", "claude-cli/1.0")
                .body(Body::from(
                    r#"{"model":"claude-sonnet-4","messages":[],"max_tokens":32,"stream":true}"#,
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
        concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello claude\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        )
    );

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer claude-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("claude-sonnet-4"));
    assert_eq!(upstream_body["stream"].as_bool(), Some(true));

    server.abort();
}

#[tokio::test]
async fn forwards_public_v1_messages_count_tokens_through_claude_runtime() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    let requests = Arc::new(Mutex::new(Vec::<RecordedPublicRequest>::new()));
    let requests_state = requests.clone();
    let app = Router::new().route(
        "/v1/messages/count_tokens",
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
                (StatusCode::OK, Json(json!({ "input_tokens": 11 })))
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    write_claude_auth_file(
        &state,
        "claude-demo@example.com.json",
        "claude-token",
        &base_url,
        None,
        "claude-sonnet-4",
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages/count_tokens")
                .header("Content-Type", "application/json")
                .header("User-Agent", "claude-cli/1.0")
                .body(Body::from(
                    r#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":"hello"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!({ "input_tokens": 11 }));

    let requests = requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer claude-token")
    );
    let upstream_body: Value = serde_json::from_slice(&requests[0].body).expect("body json");
    assert_eq!(upstream_body["model"].as_str(), Some("claude-sonnet-4"));
    assert_eq!(upstream_body["messages"][0]["role"].as_str(), Some("user"));
    assert_eq!(
        upstream_body["messages"][0]["content"].as_str(),
        Some("hello")
    );

    server.abort();
}
