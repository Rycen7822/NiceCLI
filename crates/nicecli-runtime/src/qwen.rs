use crate::{
    ExecuteWithRetryError, ExecuteWithRetryOptions, Executed, ExecutionError, ExecutionFailure,
    ExecutionResult, ProviderHttpResponse, RoutingStrategy, RuntimeConductor,
};
use chrono::{DateTime, Duration, Utc};
use nicecli_auth::AuthFileStoreError;
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

mod auth_state;
mod helpers;
mod status;

use self::auth_state::*;
use self::helpers::*;
use self::status::*;

const DEFAULT_QWEN_BASE_URL: &str = "https://portal.qwen.ai/v1";
const DEFAULT_QWEN_TOKEN_URL: &str = "https://chat.qwen.ai/api/v1/oauth2/token";
const DEFAULT_QWEN_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";
const DEFAULT_QWEN_USER_AGENT: &str = "QwenCode/0.10.3 (darwin; arm64)";
const QWEN_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const QWEN_REFRESH_LEAD_SECS: i64 = 3 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QwenChatCompletionsRequest {
    pub model: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawQwenRequest {
    model: String,
    body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QwenCallerEndpoints {
    pub api_base_url: String,
    pub token_url: String,
    pub client_id: String,
}

impl Default for QwenCallerEndpoints {
    fn default() -> Self {
        Self {
            api_base_url: DEFAULT_QWEN_BASE_URL.to_string(),
            token_url: DEFAULT_QWEN_TOKEN_URL.to_string(),
            client_id: DEFAULT_QWEN_CLIENT_ID.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum QwenCallerError {
    #[error("failed to read qwen auth file: {0}")]
    ReadAuthFile(AuthFileStoreError),
    #[error("failed to persist qwen auth file: {0}")]
    WriteAuthFile(AuthFileStoreError),
    #[error("qwen auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("qwen auth is missing access_token")]
    MissingAccessToken,
    #[error("qwen request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("qwen refresh returned {status}: {body}")]
    RefreshRejected { status: u16, body: String },
    #[error("qwen returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug)]
pub struct QwenChatCaller {
    auth_dir: PathBuf,
    conductor: RuntimeConductor,
    default_proxy_url: Option<String>,
    user_agent: String,
    endpoints: QwenCallerEndpoints,
}

impl QwenChatCaller {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            auth_dir: auth_dir.clone(),
            conductor: RuntimeConductor::new(auth_dir, strategy),
            default_proxy_url: None,
            user_agent: DEFAULT_QWEN_USER_AGENT.to_string(),
            endpoints: QwenCallerEndpoints::default(),
        }
    }

    pub fn with_default_proxy_url(mut self, default_proxy_url: Option<String>) -> Self {
        self.default_proxy_url = trimmed(default_proxy_url);
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into().trim().to_string();
        self
    }

    pub fn with_endpoints(mut self, endpoints: QwenCallerEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn execute(
        &mut self,
        request: QwenChatCompletionsRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<QwenCallerError>> {
        let request = RawQwenRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let request_time = options.pick.now;
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("qwen", &model, options, move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let user_agent = user_agent.clone();
                let endpoints = endpoints.clone();
                let auth_file_name = selection.snapshot.name.clone();
                async move {
                    execute_request_once(
                        &auth_dir,
                        default_proxy_url.as_deref(),
                        &user_agent,
                        &endpoints,
                        auth_file_name.as_str(),
                        &request,
                        request_time,
                    )
                    .await
                }
            })
            .await
    }

    pub async fn execute_stream(
        &mut self,
        request: QwenChatCompletionsRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<reqwest::Response>, ExecuteWithRetryError<QwenCallerError>> {
        let request = RawQwenRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let request_time = options.pick.now;
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("qwen", &model, options, move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let user_agent = user_agent.clone();
                let endpoints = endpoints.clone();
                let auth_file_name = selection.snapshot.name.clone();
                async move {
                    execute_stream_once(
                        &auth_dir,
                        default_proxy_url.as_deref(),
                        &user_agent,
                        &endpoints,
                        auth_file_name.as_str(),
                        &request,
                        request_time,
                    )
                    .await
                }
            })
            .await
    }
}

async fn execute_request_once(
    auth_dir: &Path,
    default_proxy_url: Option<&str>,
    user_agent: &str,
    endpoints: &QwenCallerEndpoints,
    auth_file_name: &str,
    request: &RawQwenRequest,
    now: DateTime<Utc>,
) -> Result<ProviderHttpResponse, ExecutionFailure<QwenCallerError>> {
    let mut auth = load_qwen_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(qwen_local_failure)?
        .ok_or_else(|| qwen_auth_failure(QwenCallerError::MissingAccessToken))?;

    if refresh_due(&auth, now) && auth.refresh_token.is_some() {
        auth = refresh_qwen_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(qwen_auth_failure)?;
    }

    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(qwen_auth_failure(QwenCallerError::MissingAccessToken));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(qwen_request_failure)?;
    let mut response = send_qwen_request(
        &client,
        &auth,
        user_agent,
        &request.body,
        "application/json",
    )
    .await
    .map_err(qwen_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() {
        auth = refresh_qwen_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(qwen_auth_failure)?;
        response = send_qwen_request(
            &client,
            &auth,
            user_agent,
            &request.body,
            "application/json",
        )
        .await
        .map_err(qwen_request_failure)?;
    }

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(qwen_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(qwen_status_failure(
            status,
            &headers,
            &body,
            request.model.as_str(),
            now,
        ));
    }

    Ok(ProviderHttpResponse {
        status,
        headers,
        body,
    })
}

async fn execute_stream_once(
    auth_dir: &Path,
    default_proxy_url: Option<&str>,
    user_agent: &str,
    endpoints: &QwenCallerEndpoints,
    auth_file_name: &str,
    request: &RawQwenRequest,
    now: DateTime<Utc>,
) -> Result<reqwest::Response, ExecutionFailure<QwenCallerError>> {
    let mut auth = load_qwen_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(qwen_local_failure)?
        .ok_or_else(|| qwen_auth_failure(QwenCallerError::MissingAccessToken))?;

    if refresh_due(&auth, now) && auth.refresh_token.is_some() {
        auth = refresh_qwen_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(qwen_auth_failure)?;
    }

    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(qwen_auth_failure(QwenCallerError::MissingAccessToken));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(qwen_request_failure)?;
    let mut response = send_qwen_request(
        &client,
        &auth,
        user_agent,
        &request.body,
        "text/event-stream",
    )
    .await
    .map_err(qwen_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() {
        auth = refresh_qwen_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(qwen_auth_failure)?;
        response = send_qwen_request(
            &client,
            &auth,
            user_agent,
            &request.body,
            "text/event-stream",
        )
        .await
        .map_err(qwen_request_failure)?;
    }

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(qwen_request_failure)?;
        return Err(qwen_status_failure(
            status,
            &headers,
            &body,
            request.model.as_str(),
            now,
        ));
    }

    Ok(response)
}

impl From<QwenChatCompletionsRequest> for RawQwenRequest {
    fn from(value: QwenChatCompletionsRequest) -> Self {
        Self {
            model: value.model.trim().to_string(),
            body: value.body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        time_until_next_beijing_midnight, QwenCallerEndpoints, QwenChatCaller,
        QwenChatCompletionsRequest, DEFAULT_QWEN_CLIENT_ID,
    };
    use crate::{ExecuteWithRetryOptions, RoutingStrategy};
    use axum::body::{to_bytes, Body};
    use axum::extract::State;
    use axum::http::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::{json, Value};
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedRequest {
        path: String,
        authorization: Option<String>,
        accept: Option<String>,
        user_agent: Option<String>,
        dashscope_auth_type: Option<String>,
        body: Vec<u8>,
    }

    #[derive(Debug, Clone)]
    struct ServerState {
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
        first_old_token_unauthorized: Arc<Mutex<bool>>,
    }

    fn write_qwen_auth(
        temp_dir: &TempDir,
        file_name: &str,
        access_token: Option<&str>,
        refresh_token: Option<&str>,
        resource_url: Option<&str>,
        expired_at: Option<chrono::DateTime<Utc>>,
    ) {
        let payload = json!({
            "id": file_name,
            "provider": "qwen",
            "type": "qwen",
            "email": "qwen@example.com",
            "access_token": access_token,
            "refresh_token": refresh_token,
            "resource_url": resource_url,
            "expired": expired_at.map(|value| value.to_rfc3339()),
        });
        fs::write(
            temp_dir.path().join(file_name),
            serde_json::to_vec_pretty(&payload).expect("auth bytes"),
        )
        .expect("write auth");
    }

    #[tokio::test]
    async fn qwen_chat_caller_executes_with_normalized_resource_url() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
            first_old_token_unauthorized: Arc::new(Mutex::new(false)),
        };
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(
                    |State(state): State<ServerState>, req: Request<Body>| async move {
                        let (parts, body) = req.into_parts();
                        let body = to_bytes(body, usize::MAX).await.expect("body");
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            path: parts.uri.path().to_string(),
                            authorization: parts
                                .headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            accept: parts
                                .headers
                                .get(ACCEPT)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            user_agent: parts
                                .headers
                                .get(USER_AGENT)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            dashscope_auth_type: parts
                                .headers
                                .get("X-Dashscope-Authtype")
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        (StatusCode::OK, Json(json!({ "id": "qwen-ok" }))).into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_qwen_auth(
            &temp_dir,
            "qwen-a@example.com.json",
            Some("qwen-token"),
            Some("qwen-refresh"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller = QwenChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                QwenChatCompletionsRequest {
                    model: "qwen-max".to_string(),
                    body: br#"{"model":"qwen-max","messages":[{"role":"user","content":"hi"}]}"#
                        .to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "qwen-a@example.com.json");
        assert_eq!(executed.value.status, 200);

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1/chat/completions");
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer qwen-token")
        );
        assert_eq!(requests[0].accept.as_deref(), Some("application/json"));
        assert_eq!(
            requests[0].user_agent.as_deref(),
            Some("QwenCode/0.10.3 (darwin; arm64)")
        );
        assert_eq!(
            requests[0].dashscope_auth_type.as_deref(),
            Some("qwen-oauth")
        );

        server.abort();
    }

    #[tokio::test]
    async fn qwen_chat_caller_refreshes_after_unauthorized_and_persists_it() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
            first_old_token_unauthorized: Arc::new(Mutex::new(true)),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let addr = listener.local_addr().expect("addr");
        let base_url = format!("http://{addr}");
        let refreshed_resource_url = base_url.clone();
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(
                    |State(state): State<ServerState>, req: Request<Body>| async move {
                        let (parts, body) = req.into_parts();
                        let body = to_bytes(body, usize::MAX).await.expect("body");
                        let authorization = parts
                            .headers
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string);
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            path: parts.uri.path().to_string(),
                            authorization: authorization.clone(),
                            accept: parts
                                .headers
                                .get(ACCEPT)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            user_agent: parts
                                .headers
                                .get(USER_AGENT)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            dashscope_auth_type: parts
                                .headers
                                .get("X-Dashscope-Authtype")
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });

                        if authorization.as_deref() == Some("Bearer qwen-old-token") {
                            let mut first =
                                state.first_old_token_unauthorized.lock().expect("lock");
                            if *first {
                                *first = false;
                                return (StatusCode::UNAUTHORIZED, "expired token").into_response();
                            }
                        }

                        (StatusCode::OK, Json(json!({ "id": "qwen-refreshed" }))).into_response()
                    },
                ),
            )
            .route(
                "/api/v1/oauth2/token",
                post(move || {
                    let refreshed_resource_url = refreshed_resource_url.clone();
                    async move {
                        (
                            StatusCode::OK,
                            Json(json!({
                                "access_token": "qwen-new-token",
                                "refresh_token": "qwen-new-refresh",
                                "resource_url": refreshed_resource_url,
                                "expires_in": 7200,
                                "token_type": "Bearer"
                            })),
                        )
                            .into_response()
                    }
                }),
            )
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_qwen_auth(
            &temp_dir,
            "qwen-a@example.com.json",
            Some("qwen-old-token"),
            Some("qwen-old-refresh"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller = QwenChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst)
            .with_endpoints(QwenCallerEndpoints {
                api_base_url: base_url.clone(),
                token_url: format!("http://{addr}/api/v1/oauth2/token"),
                client_id: DEFAULT_QWEN_CLIENT_ID.to_string(),
            });
        let executed = caller
            .execute(
                QwenChatCompletionsRequest {
                    model: "qwen-max".to_string(),
                    body: br#"{"model":"qwen-max","messages":[]}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.value.status, 200);
        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer qwen-old-token")
        );
        assert_eq!(
            requests[1].authorization.as_deref(),
            Some("Bearer qwen-new-token")
        );

        let auth_payload: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("qwen-a@example.com.json")).expect("read auth"),
        )
        .expect("auth json");
        assert_eq!(
            auth_payload["access_token"].as_str(),
            Some("qwen-new-token")
        );
        assert_eq!(
            auth_payload["refresh_token"].as_str(),
            Some("qwen-new-refresh")
        );
        assert_eq!(
            auth_payload["resource_url"].as_str(),
            Some(base_url.as_str())
        );
        assert_eq!(
            auth_payload["expired"].as_str(),
            Some("2026-04-05T14:00:00+00:00")
        );
        assert_eq!(
            auth_payload["last_refresh"].as_str(),
            Some("2026-04-05T12:00:00+00:00")
        );

        server.abort();
    }

    #[tokio::test]
    async fn qwen_chat_caller_rotates_after_retryable_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
            first_old_token_unauthorized: Arc::new(Mutex::new(false)),
        };
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(
                    |State(state): State<ServerState>, req: Request<Body>| async move {
                        let (parts, body) = req.into_parts();
                        let body = to_bytes(body, usize::MAX).await.expect("body");
                        let authorization = parts
                            .headers
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string);
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            path: parts.uri.path().to_string(),
                            authorization: authorization.clone(),
                            accept: parts
                                .headers
                                .get(ACCEPT)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            user_agent: parts
                                .headers
                                .get(USER_AGENT)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            dashscope_auth_type: parts
                                .headers
                                .get("X-Dashscope-Authtype")
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });

                        if authorization.as_deref() == Some("Bearer token-a") {
                            return (
                                StatusCode::TOO_MANY_REQUESTS,
                                [("retry-after", "120")],
                                Json(json!({ "error": { "message": "rate limited" } })),
                            )
                                .into_response();
                        }

                        (StatusCode::OK, Json(json!({ "id": "qwen-ok" }))).into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_qwen_auth(
            &temp_dir,
            "qwen-a@example.com.json",
            Some("token-a"),
            Some("refresh-a"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
        );
        write_qwen_auth(
            &temp_dir,
            "qwen-b@example.com.json",
            Some("token-b"),
            Some("refresh-b"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller = QwenChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                QwenChatCompletionsRequest {
                    model: "qwen-max".to_string(),
                    body: br#"{"model":"qwen-max","messages":[]}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "qwen-b@example.com.json");
        assert_eq!(executed.value.status, 200);

        let failed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("qwen-a@example.com.json")).expect("read auth a"),
        )
        .expect("auth a json");
        assert_eq!(failed_auth["status"].as_str(), Some("error"));
        assert_eq!(failed_auth["status_message"].as_str(), Some("rate limited"));
        assert_eq!(failed_auth["quota"]["exceeded"].as_bool(), Some(true));
        assert_eq!(
            failed_auth["next_retry_after"].as_str(),
            Some("2026-04-05T12:02:00Z")
        );

        server.abort();
    }

    #[tokio::test]
    async fn qwen_chat_caller_stops_on_terminal_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|| async { (StatusCode::BAD_REQUEST, "invalid request").into_response() }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_qwen_auth(
            &temp_dir,
            "qwen-a@example.com.json",
            Some("token-a"),
            Some("refresh-a"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
        );
        write_qwen_auth(
            &temp_dir,
            "qwen-b@example.com.json",
            Some("token-b"),
            Some("refresh-b"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller = QwenChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let error = caller
            .execute(
                QwenChatCompletionsRequest {
                    model: "qwen-max".to_string(),
                    body: br#"{"model":"qwen-max","messages":[]}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect_err("terminal error");

        let provider_error = match error {
            crate::ExecuteWithRetryError::Provider(error) => error,
            other => panic!("unexpected error: {other:?}"),
        };
        assert_eq!(
            provider_error.to_string(),
            "qwen returned 400: invalid request"
        );

        let untouched_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("qwen-b@example.com.json")).expect("read auth b"),
        )
        .expect("auth b json");
        assert_eq!(untouched_auth["status"], Value::Null);

        server.abort();
    }

    #[test]
    fn qwen_quota_retry_after_targets_next_beijing_midnight() {
        let now = Utc.with_ymd_and_hms(2026, 4, 5, 15, 30, 0).unwrap();
        assert_eq!(time_until_next_beijing_midnight(now), Duration::minutes(30));
    }
}
