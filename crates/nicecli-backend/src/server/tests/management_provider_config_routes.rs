use super::*;

#[tokio::test]
async fn manages_openai_compatibility_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nopenai-compatibility:\n  - name: \" demo \"\n    prefix: \"/team/\"\n    base-url: \" https://openai.local \"\n    api-key-entries:\n      - api-key: \" openai-1 \"\n        proxy-url: \" http://127.0.0.1:7890 \"\n      - api-key: \"\"\n        proxy-url: http://127.0.0.1:7891\n    headers:\n      \" X-Test \": \" demo \"\n      Empty: \"\"\n    models:\n      - name: \" gpt-5 \"\n        alias: \" demo-gpt5 \"\n      - name: \"\"\n        alias: \"\"\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/openai-compatibility")
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
            "openai-compatibility": [
                {
                    "name": "demo",
                    "prefix": "team",
                    "base-url": "https://openai.local",
                    "api-key-entries": [
                        {
                            "api-key": "openai-1",
                            "proxy-url": " http://127.0.0.1:7890 "
                        },
                        {
                            "api-key": "",
                            "proxy-url": "http://127.0.0.1:7891"
                        }
                    ],
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
                    }
                }
            ]
        })
    );

    let put_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v0/management/openai-compatibility")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"items":[{"name":" demo-put ","prefix":" /alpha/ ","base-url":" https://openai-a.local ","api-key-entries":[{"api-key":" openai-put ","proxy-url":" http://127.0.0.1:7001 "},{"api-key":" ","proxy-url":" http://127.0.0.1:7002 "}],"headers":{" Authorization ":" Bearer demo ","Empty":" "},"models":[{"name":" gpt-5 ","alias":" team-gpt5 "},{"name":"","alias":""}]},{"name":"skip","base-url":" "}]} "#,
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
                .uri("/v0/management/openai-compatibility")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"demo-put","value":{"name":" demo-next ","prefix":" /lab/ ","base-url":" https://openai-b.local ","api-key-entries":[{"api-key":" openai-next ","proxy-url":" http://127.0.0.1:7101 "}],"models":[{"name":" gpt-5-mini ","alias":" team-mini "}],"headers":{"X-New":" value "}}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let patched_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/openai-compatibility")
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
            "openai-compatibility": [
                {
                    "name": "demo-next",
                    "prefix": "lab",
                    "base-url": "https://openai-b.local",
                    "api-key-entries": [
                        {
                            "api-key": "openai-next",
                            "proxy-url": " http://127.0.0.1:7101 "
                        }
                    ],
                    "models": [
                        {
                            "name": " gpt-5-mini ",
                            "alias": " team-mini "
                        }
                    ],
                    "headers": {
                        "X-New": "value"
                    }
                }
            ]
        })
    );

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/openai-compatibility?name=demo-next")
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
                .uri("/v0/management/openai-compatibility")
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
            "openai-compatibility": []
        })
    );
}

#[tokio::test]
async fn manages_oauth_excluded_models_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\noauth-excluded-models:\n  Codex:\n    - GPT-5\n    - gpt-5\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/oauth-excluded-models")
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
            "oauth-excluded-models": {
                "codex": ["gpt-5"]
            }
        })
    );

    let patch_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/oauth-excluded-models")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"provider":"Claude","models":[" Claude-3.7-Sonnet ","claude-3.7-sonnet"]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/oauth-excluded-models?provider=codex")
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
                .uri("/v0/management/oauth-excluded-models")
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
            "oauth-excluded-models": {
                "claude": ["claude-3.7-sonnet"]
            }
        })
    );
}

#[tokio::test]
async fn manages_oauth_model_alias_via_rust_routes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let state = load_fixture_state(&temp_dir);
    fs::write(
        state.bootstrap.config_path(),
        format!(
            "host: 127.0.0.1\nport: 8317\nauth-dir: {}\noauth-model-alias:\n  Codex:\n    - name: gpt-5\n      alias: demo-gpt5\n    - name: gpt-5\n      alias: DEMO-GPT5\n",
            state.auth_dir.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("write config");

    let get_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v0/management/oauth-model-alias")
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
            "oauth-model-alias": {
                "codex": [
                    {
                        "name": "gpt-5",
                        "alias": "demo-gpt5"
                    }
                ]
            }
        })
    );

    let patch_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v0/management/oauth-model-alias")
                .header("X-Management-Key", "secret")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"provider":"Claude","aliases":[{"name":"claude-sonnet-4","alias":"team-sonnet","fork":true},{"name":"claude-sonnet-4","alias":"TEAM-SONNET"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(patch_response.status(), StatusCode::OK);

    let delete_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v0/management/oauth-model-alias?channel=codex")
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
                .uri("/v0/management/oauth-model-alias")
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
            "oauth-model-alias": {
                "claude": [
                    {
                        "name": "claude-sonnet-4",
                        "alias": "team-sonnet",
                        "fork": true
                    }
                ]
            }
        })
    );
}
