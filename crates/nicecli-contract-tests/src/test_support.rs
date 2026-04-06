use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use nicecli_backend::{load_state_from_bootstrap, BackendAppState, BackendBootstrap};
use nicecli_quota::{
    AuthEnumerator, CodexAuthContext, CodexQuotaSource, CodexSourceError, RateLimitSnapshot,
    WorkspaceRef,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tower::ServiceExt;

pub(crate) fn create_fixture_config(temp_dir: &TempDir) -> PathBuf {
    let auth_dir = temp_dir.path().join("auth");
    fs::create_dir_all(&auth_dir).expect("auth dir");
    let config_path = temp_dir.path().join("config.yaml");
    let config_body = format!(
        "host: 127.0.0.1\nport: 8317\nauth-dir: {}\nproxy-url: \"\"\ndebug: false\nlogging-to-file: false\nusage-statistics-enabled: false\nws-auth: false\n",
        auth_dir.to_string_lossy().replace('\\', "/")
    );
    fs::write(&config_path, config_body).expect("config file");
    config_path
}

pub(crate) fn load_fixture_state(temp_dir: &TempDir) -> BackendAppState {
    let config_path = create_fixture_config(temp_dir);
    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
    load_state_from_bootstrap(bootstrap).expect("state should load")
}

#[derive(Clone)]
pub(crate) struct StaticAuthEnumerator {
    pub(crate) auths: Vec<CodexAuthContext>,
}

impl AuthEnumerator for StaticAuthEnumerator {
    fn list_codex_auths(&self) -> Result<Vec<CodexAuthContext>, std::io::Error> {
        Ok(self.auths.clone())
    }
}

#[derive(Clone)]
pub(crate) struct FailingWorkspaceListSource;

#[async_trait]
impl CodexQuotaSource for FailingWorkspaceListSource {
    async fn list_workspaces(
        &self,
        _auth: &CodexAuthContext,
    ) -> Result<Vec<WorkspaceRef>, CodexSourceError> {
        Err(CodexSourceError::UnexpectedStatus {
            status: 503,
            body: "workspace list down".to_string(),
        })
    }

    async fn fetch_workspace_snapshot(
        &self,
        _auth: &CodexAuthContext,
        _workspace: &WorkspaceRef,
    ) -> Result<RateLimitSnapshot, CodexSourceError> {
        Err(CodexSourceError::UnexpectedStatus {
            status: 500,
            body: "fetch should not run when workspace list fails".to_string(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct FlakyAuthEnumerator {
    pub(crate) auths: Vec<CodexAuthContext>,
    pub(crate) calls: Arc<AtomicUsize>,
}

impl AuthEnumerator for FlakyAuthEnumerator {
    fn list_codex_auths(&self) -> Result<Vec<CodexAuthContext>, std::io::Error> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            Ok(self.auths.clone())
        } else {
            Err(std::io::Error::other("auth list unavailable"))
        }
    }
}

#[derive(Clone)]
pub(crate) struct StaticWorkspaceSource;

#[async_trait]
impl CodexQuotaSource for StaticWorkspaceSource {
    async fn list_workspaces(
        &self,
        _auth: &CodexAuthContext,
    ) -> Result<Vec<WorkspaceRef>, CodexSourceError> {
        Ok(vec![WorkspaceRef {
            id: "org_secondary".to_string(),
            name: "Workspace Beta".to_string(),
            r#type: "business".to_string(),
        }])
    }

    async fn fetch_workspace_snapshot(
        &self,
        _auth: &CodexAuthContext,
        _workspace: &WorkspaceRef,
    ) -> Result<RateLimitSnapshot, CodexSourceError> {
        Ok(RateLimitSnapshot {
            limit_id: None,
            limit_name: None,
            primary: Some(nicecli_quota::RateLimitWindow {
                used_percent: 80.0,
                window_minutes: Some(300),
                resets_at: Some(1760003600),
            }),
            secondary: None,
            credits: None,
            plan_type: Some("team".to_string()),
        })
    }
}

pub(crate) async fn request_json(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value = serde_json::from_slice(&body).expect("json body");
    (status, value)
}

pub(crate) async fn spawn_usage_server_with_response(
    status: StatusCode,
    body: Value,
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/backend-api/wham/usage",
        get(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("usage server");
    });
    address
}

pub(crate) async fn spawn_usage_server() -> std::net::SocketAddr {
    spawn_usage_server_with_response(
        StatusCode::OK,
        json!({
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
        }),
    )
    .await
}

pub(crate) async fn spawn_workspace_usage_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/backend-api/wham/usage",
        get(|headers: HeaderMap| async move {
            let authorization = headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            let account_id = headers
                .get("ChatGPT-Account-Id")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();

            let body = match (authorization, account_id) {
                ("Bearer token-123", "org_secondary") => json!({
                    "plan_type": "team",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 80,
                            "limit_window_seconds": 18000,
                            "reset_at": 1760003600
                        }
                    },
                    "credits": {
                        "has_credits": true,
                        "unlimited": false,
                        "balance": "3.0"
                    }
                }),
                ("Bearer token-123", "org_default") => json!({
                    "plan_type": "team",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 10,
                            "limit_window_seconds": 18000,
                            "reset_at": 1760001800
                        }
                    }
                }),
                ("Bearer token-456", "org_secondary") => json!({
                    "plan_type": "team",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 45,
                            "limit_window_seconds": 18000,
                            "reset_at": 1760005400
                        }
                    }
                }),
                _ => json!({
                    "message": format!("unexpected quota request: auth={authorization}, workspace={account_id}")
                }),
            };
            let status = match (authorization, account_id) {
                ("Bearer token-123", "org_secondary")
                | ("Bearer token-123", "org_default")
                | ("Bearer token-456", "org_secondary") => StatusCode::OK,
                _ => StatusCode::BAD_REQUEST,
            };
            (status, Json(body))
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("workspace usage server");
    });
    address
}

pub(crate) fn build_jwt(payload_json: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    format!("{header}.{payload}.signature")
}

pub(crate) async fn spawn_codex_token_server(id_token: String) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/oauth/token",
        post(move || {
            let id_token = id_token.clone();
            async move {
                Json(json!({
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

pub(crate) async fn spawn_qwen_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/api/v1/oauth2/device/code",
            get(|| async {
                Json(json!({
                    "device_code": "device-code-123",
                    "user_code": "ABCD",
                    "verification_uri": "https://chat.qwen.ai/device",
                    "verification_uri_complete": "https://chat.qwen.ai/device?user_code=ABCD",
                    "expires_in": 3600,
                    "interval": 1
                }))
            })
            .post(|| async {
                Json(json!({
                    "device_code": "device-code-123",
                    "user_code": "ABCD",
                    "verification_uri": "https://chat.qwen.ai/device",
                    "verification_uri_complete": "https://chat.qwen.ai/device?user_code=ABCD",
                    "expires_in": 3600,
                    "interval": 1
                }))
            }),
        )
        .route(
            "/api/v1/oauth2/token",
            post(|| async {
                Json(json!({
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

pub(crate) async fn spawn_anthropic_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new().route(
        "/v1/oauth/token",
        post(|| async {
            Json(json!({
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

pub(crate) async fn spawn_gemini_cli_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/oauth2/token",
            post(|| async {
                Json(json!({
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
                Json(json!({
                    "email": "gemini-route@example.com"
                }))
            }),
        )
        .route(
            "/projects",
            get(|| async {
                Json(json!({
                    "projects": [
                        { "projectId": "auto-route-project-123" }
                    ]
                }))
            }),
        )
        .route(
            "/v1/projects/auto-route-project-123/services/cloudaicompanion.googleapis.com",
            get(|| async {
                Json(json!({
                    "state": "ENABLED"
                }))
            }),
        )
        .route(
            "/v1/projects/manual-route-project-456/services/cloudaicompanion.googleapis.com",
            get(|| async {
                Json(json!({
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

pub(crate) async fn spawn_antigravity_login_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local addr");
    let router = Router::new()
        .route(
            "/oauth2/token",
            post(|| async {
                Json(json!({
                    "access_token": "antigravity-access-token",
                    "refresh_token": "antigravity-refresh-token",
                    "expires_in": 3600
                }))
            }),
        )
        .route(
            "/userinfo",
            get(|| async {
                Json(json!({
                    "email": "antigravity-route@example.com"
                }))
            }),
        )
        .route(
            "/v1internal:loadCodeAssist",
            post(|| async {
                Json(json!({
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

pub(crate) fn expected_json(raw: &str) -> Value {
    serde_json::from_str(raw).expect("fixture json")
}
