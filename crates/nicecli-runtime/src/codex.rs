use crate::{
    ExecuteWithRetryError, ExecuteWithRetryOptions, Executed, ExecutionFailure, RoutingStrategy,
    RuntimeConductor,
};
use nicecli_auth::AuthFileStoreError;
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use std::path::{Path, PathBuf};
use thiserror::Error;

mod helpers;

use self::helpers::*;

const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const DEFAULT_CODEX_USER_AGENT: &str = "codex_cli_rs/0.116.0 (Windows NT 10.0; Win64; x64) NiceCLI";
const CODEX_RESPONSES_PATH: &str = "/responses";
const CODEX_RESPONSES_COMPACT_PATH: &str = "/responses/compact";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCompactRequest {
    pub model: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexResponsesRequest {
    pub model: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawCodexRequest {
    model: String,
    body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ProviderHttpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum CodexCallerError {
    #[error(transparent)]
    ReadAuthFile(#[from] AuthFileStoreError),
    #[error("codex auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("codex auth is missing access_token")]
    MissingAccessToken,
    #[error("codex request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("codex returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexAuthCredentials {
    access_token: String,
    base_url: String,
    proxy_url: Option<String>,
}

#[derive(Debug)]
pub struct CodexCompactCaller {
    auth_dir: PathBuf,
    conductor: RuntimeConductor,
    default_proxy_url: Option<String>,
    user_agent: String,
}

impl CodexCompactCaller {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            auth_dir: auth_dir.clone(),
            conductor: RuntimeConductor::new(auth_dir, strategy),
            default_proxy_url: None,
            user_agent: DEFAULT_CODEX_USER_AGENT.to_string(),
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

    pub async fn execute(
        &mut self,
        request: CodexCompactRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<CodexCallerError>> {
        let request = RawCodexRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("codex", &model, options, move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let user_agent = user_agent.clone();
                let auth_file_name = selection.snapshot.name.clone();
                async move {
                    execute_request_once(
                        &auth_dir,
                        default_proxy_url.as_deref(),
                        &user_agent,
                        auth_file_name.as_str(),
                        CODEX_RESPONSES_COMPACT_PATH,
                        &request,
                    )
                    .await
                }
            })
            .await
    }
}

#[derive(Debug)]
pub struct CodexResponsesCaller {
    auth_dir: PathBuf,
    conductor: RuntimeConductor,
    default_proxy_url: Option<String>,
    user_agent: String,
}

impl CodexResponsesCaller {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            auth_dir: auth_dir.clone(),
            conductor: RuntimeConductor::new(auth_dir, strategy),
            default_proxy_url: None,
            user_agent: DEFAULT_CODEX_USER_AGENT.to_string(),
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

    pub async fn execute(
        &mut self,
        request: CodexResponsesRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<CodexCallerError>> {
        let request = RawCodexRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("codex", &model, options, move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let user_agent = user_agent.clone();
                let auth_file_name = selection.snapshot.name.clone();
                async move {
                    execute_request_once(
                        &auth_dir,
                        default_proxy_url.as_deref(),
                        &user_agent,
                        auth_file_name.as_str(),
                        CODEX_RESPONSES_PATH,
                        &request,
                    )
                    .await
                }
            })
            .await
    }

    pub async fn execute_stream(
        &mut self,
        request: CodexResponsesRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<reqwest::Response>, ExecuteWithRetryError<CodexCallerError>> {
        let request = RawCodexRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("codex", &model, options, move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let user_agent = user_agent.clone();
                let auth_file_name = selection.snapshot.name.clone();
                async move {
                    execute_stream_once(
                        &auth_dir,
                        default_proxy_url.as_deref(),
                        &user_agent,
                        auth_file_name.as_str(),
                        &request,
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
    auth_file_name: &str,
    endpoint_path: &str,
    request: &RawCodexRequest,
) -> Result<ProviderHttpResponse, ExecutionFailure<CodexCallerError>> {
    let credentials = load_codex_auth_credentials(auth_dir, auth_file_name)
        .map_err(codex_local_failure)?
        .ok_or_else(|| codex_auth_failure(CodexCallerError::MissingAccessToken))?;
    let client = build_http_client(credentials.proxy_url.as_deref().or(default_proxy_url))
        .map_err(codex_request_failure)?;
    let response = send_codex_request(
        &client,
        &credentials,
        user_agent,
        endpoint_path,
        &request.body,
        "application/json",
    )
    .await
    .map_err(codex_request_failure)?;

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(codex_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(codex_status_failure(status, &body, request.model.as_str()));
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
    auth_file_name: &str,
    request: &RawCodexRequest,
) -> Result<reqwest::Response, ExecutionFailure<CodexCallerError>> {
    let credentials = load_codex_auth_credentials(auth_dir, auth_file_name)
        .map_err(codex_local_failure)?
        .ok_or_else(|| codex_auth_failure(CodexCallerError::MissingAccessToken))?;
    let client = build_http_client(credentials.proxy_url.as_deref().or(default_proxy_url))
        .map_err(codex_request_failure)?;
    let response = send_codex_request(
        &client,
        &credentials,
        user_agent,
        CODEX_RESPONSES_PATH,
        &request.body,
        "text/event-stream",
    )
    .await
    .map_err(codex_request_failure)?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(codex_request_failure)?;
        return Err(codex_status_failure(status, &body, request.model.as_str()));
    }

    Ok(response)
}

async fn send_codex_request(
    client: &Client,
    credentials: &CodexAuthCredentials,
    user_agent: &str,
    endpoint_path: &str,
    body: &[u8],
    accept: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let url = format!(
        "{}{}",
        credentials.base_url.trim_end_matches('/'),
        endpoint_path
    );
    client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, accept)
        .header(
            USER_AGENT,
            if user_agent.trim().is_empty() {
                DEFAULT_CODEX_USER_AGENT
            } else {
                user_agent.trim()
            },
        )
        .header(
            AUTHORIZATION,
            format!("Bearer {}", credentials.access_token.trim()),
        )
        .body(body.to_vec())
        .send()
        .await
}

impl From<CodexCompactRequest> for RawCodexRequest {
    fn from(value: CodexCompactRequest) -> Self {
        Self {
            model: value.model,
            body: value.body,
        }
    }
}

impl From<CodexResponsesRequest> for RawCodexRequest {
    fn from(value: CodexResponsesRequest) -> Self {
        Self {
            model: value.model,
            body: value.body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodexCompactCaller, CodexCompactRequest, CodexResponsesCaller, CodexResponsesRequest,
    };
    use crate::{ExecuteWithRetryError, ExecuteWithRetryOptions, RoutingStrategy};
    use axum::body::Bytes;
    use axum::extract::State;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::Router;
    use chrono::TimeZone;
    use serde_json::Value;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedRequest {
        authorization: Option<String>,
        body: Vec<u8>,
    }

    #[derive(Clone)]
    struct ServerState {
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
    }

    fn write_codex_auth(temp_dir: &TempDir, file_name: &str, access_token: &str, base_url: &str) {
        fs::write(
            temp_dir.path().join(file_name),
            format!(
                r#"{{
  "type": "codex",
  "provider": "codex",
  "email": "demo@example.com",
  "access_token": "{access_token}",
  "base_url": "{base_url}",
  "models": [{{"name": "gpt-5"}}]
}}"#
            ),
        )
        .expect("write auth");
    }

    #[tokio::test]
    async fn codex_responses_caller_succeeds_on_ok_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/responses",
                post(
                    |State(state): State<ServerState>, headers: HeaderMap, body: Bytes| async move {
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        (
                            StatusCode::OK,
                            r#"{"id":"resp_responses","status":"completed"}"#,
                        )
                            .into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_codex_auth(
            &temp_dir,
            "codex-a@example.com-team.json",
            "token-a",
            &base_url,
        );

        let mut caller = CodexResponsesCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                CodexResponsesRequest {
                    model: "gpt-5".to_string(),
                    body: br#"{"model":"gpt-5","input":"hello"}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(chrono::Utc::now()),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "codex-a@example.com-team.json");
        assert_eq!(executed.value.status, 200);
        assert_eq!(
            serde_json::from_slice::<Value>(&executed.value.body).expect("response json")["id"]
                .as_str(),
            Some("resp_responses")
        );

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
        assert_eq!(requests[0].body, br#"{"model":"gpt-5","input":"hello"}"#);

        server.abort();
    }

    #[tokio::test]
    async fn codex_responses_caller_rotates_to_next_auth_after_retryable_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/responses",
                post(
                    |State(state): State<ServerState>, headers: HeaderMap, body: Bytes| async move {
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        match headers
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                        {
                            "Bearer token-a" => (
                                StatusCode::TOO_MANY_REQUESTS,
                                r#"{"error":{"type":"usage_limit_reached","message":"quota exhausted","resets_in_seconds":120}}"#,
                            )
                                .into_response(),
                            "Bearer token-b" => (
                                StatusCode::OK,
                                r#"{"id":"resp_ok","status":"completed"}"#,
                            )
                                .into_response(),
                            _ => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
                        }
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_codex_auth(
            &temp_dir,
            "codex-a@example.com-team.json",
            "token-a",
            &base_url,
        );
        write_codex_auth(
            &temp_dir,
            "codex-b@example.com-team.json",
            "token-b",
            &base_url,
        );

        let now = chrono::Utc
            .with_ymd_and_hms(2026, 4, 5, 12, 0, 0)
            .single()
            .expect("now");
        let mut caller = CodexResponsesCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                CodexResponsesRequest {
                    model: "gpt-5".to_string(),
                    body: br#"{"model":"gpt-5","input":"hi"}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "codex-b@example.com-team.json");
        assert_eq!(executed.value.status, 200);

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
        assert_eq!(requests[1].authorization.as_deref(), Some("Bearer token-b"));

        let failed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("codex-a@example.com-team.json")).expect("read auth a"),
        )
        .expect("auth a json");
        assert_eq!(failed_auth["status"].as_str(), Some("error"));
        assert_eq!(
            failed_auth["status_message"].as_str(),
            Some("quota exhausted")
        );
        assert_eq!(failed_auth["quota"]["exceeded"].as_bool(), Some(true));
        assert_eq!(
            failed_auth["next_retry_after"].as_str(),
            Some("2026-04-05T12:02:00Z")
        );

        server.abort();
    }

    #[tokio::test]
    async fn codex_responses_caller_stops_on_terminal_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/responses",
                post(
                    |State(state): State<ServerState>, headers: HeaderMap, body: Bytes| async move {
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        (StatusCode::BAD_REQUEST, "invalid_request_error").into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_codex_auth(
            &temp_dir,
            "codex-a@example.com-team.json",
            "token-a",
            &base_url,
        );
        write_codex_auth(
            &temp_dir,
            "codex-b@example.com-team.json",
            "token-b",
            &base_url,
        );

        let mut caller = CodexResponsesCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let error = caller
            .execute(
                CodexResponsesRequest {
                    model: "gpt-5".to_string(),
                    body: br#"{"model":"gpt-5","input":"hi"}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(chrono::Utc::now()),
            )
            .await
            .expect_err("terminal error");

        match error {
            ExecuteWithRetryError::Provider(error) => {
                assert!(error.to_string().contains("400"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));

        let untouched_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("codex-b@example.com-team.json")).expect("read auth b"),
        )
        .expect("auth b json");
        assert!(untouched_auth.get("status").is_none());

        server.abort();
    }

    #[tokio::test]
    async fn codex_compact_caller_rotates_to_next_auth_after_retryable_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/responses/compact",
                post(
                    |State(state): State<ServerState>, headers: HeaderMap, body: Bytes| async move {
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        match headers
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                        {
                            "Bearer token-a" => {
                                (StatusCode::TOO_MANY_REQUESTS, "quota exhausted").into_response()
                            }
                            "Bearer token-b" => {
                                (StatusCode::OK, r#"{"id":"resp_ok","status":"completed"}"#)
                                    .into_response()
                            }
                            _ => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
                        }
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_codex_auth(
            &temp_dir,
            "codex-a@example.com-team.json",
            "token-a",
            &base_url,
        );
        write_codex_auth(
            &temp_dir,
            "codex-b@example.com-team.json",
            "token-b",
            &base_url,
        );

        let mut caller = CodexCompactCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                CodexCompactRequest {
                    model: "gpt-5".to_string(),
                    body: br#"{"model":"gpt-5","input":"hi"}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(chrono::Utc::now()),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "codex-b@example.com-team.json");
        assert_eq!(executed.value.status, 200);
        assert_eq!(
            serde_json::from_slice::<Value>(&executed.value.body).expect("response json")["id"]
                .as_str(),
            Some("resp_ok")
        );

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));
        assert_eq!(requests[1].authorization.as_deref(), Some("Bearer token-b"));
        assert_eq!(requests[0].body, br#"{"model":"gpt-5","input":"hi"}"#);

        let failed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("codex-a@example.com-team.json")).expect("read auth a"),
        )
        .expect("auth a json");
        assert_eq!(failed_auth["status"].as_str(), Some("error"));
        assert_eq!(failed_auth["quota"]["exceeded"].as_bool(), Some(true));

        server.abort();
    }

    #[tokio::test]
    async fn codex_compact_caller_stops_on_terminal_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/responses/compact",
                post(
                    |State(state): State<ServerState>, headers: HeaderMap, body: Bytes| async move {
                        state.requests.lock().expect("lock").push(RecordedRequest {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        (StatusCode::BAD_REQUEST, "invalid_request_error").into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_codex_auth(
            &temp_dir,
            "codex-a@example.com-team.json",
            "token-a",
            &base_url,
        );
        write_codex_auth(
            &temp_dir,
            "codex-b@example.com-team.json",
            "token-b",
            &base_url,
        );

        let mut caller = CodexCompactCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let error = caller
            .execute(
                CodexCompactRequest {
                    model: "gpt-5".to_string(),
                    body: br#"{"model":"gpt-5","input":"hi"}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(chrono::Utc::now()),
            )
            .await
            .expect_err("terminal error");

        match error {
            ExecuteWithRetryError::Provider(error) => {
                assert!(error.to_string().contains("400"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].authorization.as_deref(), Some("Bearer token-a"));

        let untouched_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("codex-b@example.com-team.json")).expect("read auth b"),
        )
        .expect("auth b json");
        assert!(untouched_auth.get("status").is_none());

        server.abort();
    }
}
