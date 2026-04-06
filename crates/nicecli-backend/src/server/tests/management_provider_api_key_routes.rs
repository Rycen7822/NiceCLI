use super::*;

#[tokio::test]
async fn manages_api_keys_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/api-keys")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"["alpha","beta"]"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_response.status(), StatusCode::OK);

    let add_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/api-keys")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"new":"gamma"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(add_response.status(), StatusCode::OK);

    let edit_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/api-keys")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"index":1,"value":"beta-edited"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(edit_response.status(), StatusCode::OK);

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/api-keys?index=0")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), StatusCode::OK);

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/api-keys")
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
    assert_eq!(
        payload["api-keys"],
        serde_json::json!(["beta-edited", "gamma"])
    );
}

#[tokio::test]
async fn returns_provider_config_lists_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ngemini-api-key:\n  - api-key: gem-1\n    prefix: gem\n    base-url: https://gemini.local\nclaude-api-key:\n  - api-key: claude-1\n    prefix: claude\n    base-url: https://claude.local\ncodex-api-key:\n  - api-key: codex-1\n    base-url: https://codex.local\n    models:\n      - name: gpt-5\n        alias: demo-gpt5\nopenai-compatibility:\n  - name: demo\n    base-url: https://openai.local\n    api-key-entries:\n      - api-key: openai-1\n        proxy-url: http://127.0.0.1:7890\n    models:\n      - name: gpt-5\n        alias: demo-gpt5\nvertex-api-key:\n  - api-key: vertex-1\n    base-url: https://vertex.local\n    models:\n      - name: gemini-2.5-pro\n        alias: demo-pro\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    for (route, key, expected) in [
        (
            "/v0/management/gemini-api-key",
            "gemini-api-key",
            serde_json::json!([
                {
                    "api-key": "gem-1",
                    "prefix": "gem",
                    "base-url": "https://gemini.local"
                }
            ]),
        ),
        (
            "/v0/management/claude-api-key",
            "claude-api-key",
            serde_json::json!([
                {
                    "api-key": "claude-1",
                    "prefix": "claude",
                    "base-url": "https://claude.local",
                    "proxy-url": "",
                    "models": []
                }
            ]),
        ),
        (
            "/v0/management/codex-api-key",
            "codex-api-key",
            serde_json::json!([
                {
                    "api-key": "codex-1",
                    "base-url": "https://codex.local",
                    "websockets": false,
                    "proxy-url": "",
                    "models": [
                        {
                            "name": "gpt-5",
                            "alias": "demo-gpt5"
                        }
                    ]
                }
            ]),
        ),
        (
            "/v0/management/openai-compatibility",
            "openai-compatibility",
            serde_json::json!([
                {
                    "name": "demo",
                    "base-url": "https://openai.local",
                    "api-key-entries": [
                        {
                            "api-key": "openai-1",
                            "proxy-url": "http://127.0.0.1:7890"
                        }
                    ],
                    "models": [
                        {
                            "name": "gpt-5",
                            "alias": "demo-gpt5"
                        }
                    ]
                }
            ]),
        ),
        (
            "/v0/management/vertex-api-key",
            "vertex-api-key",
            serde_json::json!([
                {
                    "api-key": "vertex-1",
                    "base-url": "https://vertex.local",
                    "models": [
                        {
                            "name": "gemini-2.5-pro",
                            "alias": "demo-pro"
                        }
                    ]
                }
            ]),
        ),
    ] {
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(route)
                    .header("X-Management-Key", "secret")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK, "route={route}");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload[key], expected, "route={route}");
    }
}

#[tokio::test]
async fn manages_gemini_api_keys_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ngemini-api-key:\n  - api-key: \" gem-1 \"\n    prefix: \"/team/\"\n    proxy-url: \" http://127.0.0.1:7890 \"\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    excluded-models:\n      - \" GPT-5 \"\n      - gpt-5\n  - api-key: gem-1\n    prefix: duplicate\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/gemini-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_payload: Value = serde_json::from_slice(&get_body).expect("json");
    assert_eq!(
        get_payload,
        serde_json::json!({
            "gemini-api-key": [
                {
                    "api-key": "gem-1",
                    "prefix": "team",
                    "proxy-url": "http://127.0.0.1:7890",
                    "headers": {
                        "X-Test": "demo"
                    },
                    "excluded-models": ["gpt-5"]
                }
            ]
        })
    );

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/gemini-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"items":[{"api-key":" put-1 ","prefix":" /alpha/ ","headers":{" Authorization ":" Bearer demo ","Empty":" "},"excluded-models":[" Gemini-2.5-Pro ","gemini-2.5-pro"]},{"api-key":"put-1","prefix":"duplicate"},{"api-key":" ","prefix":"skip"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_response.status(), StatusCode::OK);

    let patch_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/gemini-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"match":"put-1","value":{"api-key":"put-2","prefix":" /lab/ ","base-url":" https://gemini.local ","proxy-url":" http://127.0.0.1:8899 ","headers":{"X-New":" value "},"excluded-models":[" Gemini-2.5-Flash ","gemini-2.5-flash"]}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let patched_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/gemini-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patched_response.status(), StatusCode::OK);
    let patched_body = to_bytes(patched_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let patched_payload: Value = serde_json::from_slice(&patched_body).expect("json");
    assert_eq!(
        patched_payload,
        serde_json::json!({
            "gemini-api-key": [
                {
                    "api-key": "put-2",
                    "prefix": "lab",
                    "base-url": "https://gemini.local",
                    "proxy-url": "http://127.0.0.1:8899",
                    "headers": {
                        "X-New": "value"
                    },
                    "excluded-models": ["gemini-2.5-flash"]
                }
            ]
        })
    );

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/gemini-api-key?api-key=put-2")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), StatusCode::OK);

    let final_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/gemini-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(final_response.status(), StatusCode::OK);
    let final_body = to_bytes(final_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let final_payload: Value = serde_json::from_slice(&final_body).expect("json");
    assert_eq!(final_payload, serde_json::json!({ "gemini-api-key": [] }));
}

#[tokio::test]
async fn manages_vertex_api_keys_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nvertex-api-key:\n  - api-key: \" vertex-1 \"\n    prefix: \"/team/\"\n    base-url: \" https://vertex.local \"\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    models:\n      - name: \" gemini-2.5-pro \"\n        alias: \" demo-pro \"\n      - name: \"\"\n        alias: invalid\n    excluded-models:\n      - \" Gemini-2.5-Pro \"\n      - gemini-2.5-pro\n  - api-key: vertex-1\n    base-url: https://vertex.local\n    prefix: duplicate\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/vertex-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_payload: Value = serde_json::from_slice(&get_body).expect("json");
    assert_eq!(
        get_payload,
        serde_json::json!({
            "vertex-api-key": [
                {
                    "api-key": "vertex-1",
                    "prefix": "team",
                    "base-url": "https://vertex.local",
                    "headers": {
                        "X-Test": "demo"
                    },
                    "models": [
                        {
                            "name": "gemini-2.5-pro",
                            "alias": "demo-pro"
                        }
                    ],
                    "excluded-models": ["gemini-2.5-pro"]
                }
            ]
        })
    );

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/vertex-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"items":[{"api-key":" vert-1 ","prefix":" /alpha/ ","base-url":" https://vertex2.local ","headers":{" Authorization ":" Bearer demo ","Empty":" "},"models":[{"name":" gemini-2.5-flash ","alias":" flash "},{"name":"","alias":"invalid"}],"excluded-models":[" Gemini-2.5-Flash ","gemini-2.5-flash"]},{"api-key":"vert-1","base-url":"https://vertex2.local","prefix":"duplicate"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_response.status(), StatusCode::OK);

    let patch_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/vertex-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"match":"vert-1","value":{"api-key":"vert-2","prefix":" /lab/ ","base-url":" https://vertex3.local ","proxy-url":" http://127.0.0.1:8899 ","headers":{"X-New":" value "},"models":[{"name":" gemini-2.5-pro ","alias":" prod "},{"name":" ","alias":"skip"}],"excluded-models":[" Gemini-2.5-Pro ","gemini-2.5-pro"]}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let patched_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/vertex-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patched_response.status(), StatusCode::OK);
    let patched_body = to_bytes(patched_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let patched_payload: Value = serde_json::from_slice(&patched_body).expect("json");
    assert_eq!(
        patched_payload,
        serde_json::json!({
            "vertex-api-key": [
                {
                    "api-key": "vert-2",
                    "prefix": "lab",
                    "base-url": "https://vertex3.local",
                    "proxy-url": "http://127.0.0.1:8899",
                    "headers": {
                        "X-New": "value"
                    },
                    "models": [
                        {
                            "name": "gemini-2.5-pro",
                            "alias": "prod"
                        }
                    ],
                    "excluded-models": ["gemini-2.5-pro"]
                }
            ]
        })
    );

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/vertex-api-key?api-key=vert-2")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), StatusCode::OK);

    let final_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/vertex-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(final_response.status(), StatusCode::OK);
    let final_body = to_bytes(final_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let final_payload: Value = serde_json::from_slice(&final_body).expect("json");
    assert_eq!(final_payload, serde_json::json!({ "vertex-api-key": [] }));
}

#[tokio::test]
async fn manages_claude_api_keys_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nclaude-api-key:\n  - api-key: \" claude-1 \"\n    prefix: \"/team/\"\n    base-url: \" https://claude.local \"\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    models:\n      - name: \" claude-sonnet-4 \"\n        alias: \" sonnet \"\n      - name: \"\"\n        alias: \"\"\n    excluded-models:\n      - \" Claude-Sonnet-4 \"\n      - claude-sonnet-4\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/claude-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_payload: Value = serde_json::from_slice(&get_body).expect("json");
    assert_eq!(
        get_payload,
        serde_json::json!({
            "claude-api-key": [
                {
                    "api-key": " claude-1 ",
                    "prefix": "team",
                    "base-url": " https://claude.local ",
                    "proxy-url": "",
                    "headers": {
                        "X-Test": "demo"
                    },
                    "models": [
                        {
                            "name": " claude-sonnet-4 ",
                            "alias": " sonnet "
                        },
                        {
                            "name": "",
                            "alias": ""
                        }
                    ],
                    "excluded-models": ["claude-sonnet-4"]
                }
            ]
        })
    );

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/claude-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"items":[{"api-key":" claude-put ","prefix":" /alpha/ ","base-url":" https://claude-a.local ","headers":{" Authorization ":" Bearer demo ","Empty":" "},"models":[{"name":" claude-sonnet-4 ","alias":" team-sonnet "},{"name":"","alias":""}],"excluded-models":[" Claude-Sonnet-4 ","claude-sonnet-4"]},{"api-key":"claude-put","prefix":" /beta/ ","base-url":" https://claude-b.local "}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_response.status(), StatusCode::OK);

    let patch_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/claude-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"index":0,"value":{"prefix":" /lab/ ","base-url":" https://claude-c.local ","proxy-url":" http://127.0.0.1:8899 ","headers":{"X-New":" value "},"models":[{"name":" claude-opus-4 ","alias":" team-opus "}],"excluded-models":[" Claude-Opus-4 ","claude-opus-4"]}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let patched_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/claude-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patched_response.status(), StatusCode::OK);
    let patched_body = to_bytes(patched_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let patched_payload: Value = serde_json::from_slice(&patched_body).expect("json");
    assert_eq!(
        patched_payload,
        serde_json::json!({
            "claude-api-key": [
                {
                    "api-key": "claude-put",
                    "prefix": "lab",
                    "base-url": "https://claude-c.local",
                    "proxy-url": "http://127.0.0.1:8899",
                    "headers": {
                        "Authorization": "Bearer demo",
                        "X-New": "value"
                    },
                    "models": [
                        {
                            "name": "claude-opus-4",
                            "alias": "team-opus"
                        }
                    ],
                    "excluded-models": ["claude-opus-4"]
                },
                {
                    "api-key": "claude-put",
                    "prefix": "beta",
                    "base-url": "https://claude-b.local",
                    "proxy-url": "",
                    "models": []
                }
            ]
        })
    );

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/claude-api-key?api-key=claude-put")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), StatusCode::OK);

    let final_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/claude-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(final_response.status(), StatusCode::OK);
    let final_body = to_bytes(final_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let final_payload: Value = serde_json::from_slice(&final_body).expect("json");
    assert_eq!(final_payload, serde_json::json!({ "claude-api-key": [] }));
}

#[tokio::test]
async fn manages_codex_api_keys_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\ncodex-api-key:\n  - api-key: \" codex-1 \"\n    prefix: \"/team/\"\n    base-url: \" https://codex.local \"\n    websockets: true\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    models:\n      - name: \" gpt-5 \"\n        alias: \" demo-gpt5 \"\n      - name: \"\"\n        alias: \"\"\n    excluded-models:\n      - \" GPT-5 \"\n      - gpt-5\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let get_payload: Value = serde_json::from_slice(&get_body).expect("json");
    assert_eq!(
        get_payload,
        serde_json::json!({
            "codex-api-key": [
                {
                    "api-key": " codex-1 ",
                    "prefix": "team",
                    "base-url": "https://codex.local",
                    "websockets": true,
                    "proxy-url": "",
                    "models": [
                        {
                            "name": " gpt-5 ",
                            "alias": " demo-gpt5 "
                        },
                        {
                            "name": "",
                            "alias": ""
                        }
                    ],
                    "headers": {
                        "X-Test": "demo"
                    },
                    "excluded-models": ["gpt-5"]
                }
            ]
        })
    );

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/codex-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"items":[{"api-key":" codex-put ","prefix":" /alpha/ ","base-url":" https://codex-a.local ","websockets":true,"proxy-url":" http://127.0.0.1:7890 ","headers":{" Authorization ":" Bearer demo ","Empty":" "},"models":[{"name":" gpt-5 ","alias":" team-gpt5 "},{"name":"","alias":""}],"excluded-models":[" GPT-5 ","gpt-5"]},{"api-key":" ","base-url":" https://codex-empty.local "},{"api-key":"codex-second","prefix":" /beta/ ","base-url":" https://codex-b.local "}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(put_response.status(), StatusCode::OK);

    let patch_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/codex-api-key")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"match":"codex-put","value":{"api-key":"codex-next","prefix":" /lab/ ","base-url":" https://codex-c.local ","proxy-url":" http://127.0.0.1:8899 ","models":[{"name":" gpt-5-mini ","alias":" team-mini "},{"name":"","alias":""}],"headers":{"X-New":" value "},"excluded-models":[" GPT-5-MINI ","gpt-5-mini"]}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let patched_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patched_response.status(), StatusCode::OK);
    let patched_body = to_bytes(patched_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let patched_payload: Value = serde_json::from_slice(&patched_body).expect("json");
    assert_eq!(
        patched_payload,
        serde_json::json!({
            "codex-api-key": [
                {
                    "api-key": "codex-next",
                    "prefix": "lab",
                    "base-url": "https://codex-c.local",
                    "websockets": true,
                    "proxy-url": "http://127.0.0.1:8899",
                    "models": [
                        {
                            "name": "gpt-5-mini",
                            "alias": "team-mini"
                        }
                    ],
                    "headers": {
                        "X-New": "value"
                    },
                    "excluded-models": ["gpt-5-mini"]
                },
                {
                    "api-key": "",
                    "base-url": "https://codex-empty.local",
                    "websockets": false,
                    "proxy-url": "",
                    "models": []
                },
                {
                    "api-key": "codex-second",
                    "prefix": "beta",
                    "base-url": "https://codex-b.local",
                    "websockets": false,
                    "proxy-url": "",
                    "models": []
                }
            ]
        })
    );

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/codex-api-key?api-key=codex-next")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), StatusCode::OK);

    let final_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/codex-api-key")
                .header("X-Management-Key", "secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(final_response.status(), StatusCode::OK);
    let final_body = to_bytes(final_response.into_body(), usize::MAX)
        .await
        .expect("body");
    let final_payload: Value = serde_json::from_slice(&final_body).expect("json");
    assert_eq!(
        final_payload,
        serde_json::json!({
            "codex-api-key": [
                {
                    "api-key": "",
                    "base-url": "https://codex-empty.local",
                    "websockets": false,
                    "proxy-url": "",
                    "models": []
                },
                {
                    "api-key": "codex-second",
                    "prefix": "beta",
                    "base-url": "https://codex-b.local",
                    "websockets": false,
                    "proxy-url": "",
                    "models": []
                }
            ]
        })
    );
}
