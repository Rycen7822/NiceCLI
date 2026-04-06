use crate::{
    ExecuteWithRetryError, ExecuteWithRetryOptions, Executed, ExecutionError, ExecutionFailure,
    ExecutionResult, ProviderHttpResponse, RoutingStrategy, RuntimeConductor,
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use nicecli_auth::AuthFileStoreError;
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

mod auth_state;
mod helpers;
mod request_body;
mod status;

use self::auth_state::*;
use self::helpers::*;
use self::request_body::*;
use self::status::*;

const DEFAULT_ANTIGRAVITY_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
const DEFAULT_ANTIGRAVITY_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_ANTIGRAVITY_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const DEFAULT_ANTIGRAVITY_CLIENT_SECRET: &str = "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";
const DEFAULT_ANTIGRAVITY_USER_AGENT: &str = "antigravity/1.19.6 darwin/arm64";
const DEFAULT_ANTIGRAVITY_BODY_USER_AGENT: &str = "antigravity";
const DEFAULT_ANTIGRAVITY_REFRESH_USER_AGENT: &str = "Go-http-client/2.0";
const ANTIGRAVITY_GENERATE_CONTENT_PATH: &str = "/v1internal:generateContent";
const ANTIGRAVITY_STREAM_GENERATE_CONTENT_PATH: &str = "/v1internal:streamGenerateContent";
const ANTIGRAVITY_REFRESH_LEAD_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityGenerateContentRequest {
    pub model: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawAntigravityRequest {
    model: String,
    body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityCallerEndpoints {
    pub api_base_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
}

impl Default for AntigravityCallerEndpoints {
    fn default() -> Self {
        Self {
            api_base_url: DEFAULT_ANTIGRAVITY_BASE_URL.to_string(),
            token_url: DEFAULT_ANTIGRAVITY_TOKEN_URL.to_string(),
            client_id: DEFAULT_ANTIGRAVITY_CLIENT_ID.to_string(),
            client_secret: DEFAULT_ANTIGRAVITY_CLIENT_SECRET.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum AntigravityCallerError {
    #[error("failed to read antigravity auth file: {0}")]
    ReadAuthFile(AuthFileStoreError),
    #[error("failed to persist antigravity auth file: {0}")]
    WriteAuthFile(AuthFileStoreError),
    #[error("antigravity auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("antigravity auth is missing access_token")]
    MissingAccessToken,
    #[error("antigravity request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("antigravity refresh returned {status}: {body}")]
    RefreshRejected { status: u16, body: String },
    #[error("antigravity returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug)]
pub struct AntigravityGenerateContentCaller {
    auth_dir: PathBuf,
    conductor: RuntimeConductor,
    default_proxy_url: Option<String>,
    user_agent: String,
    endpoints: AntigravityCallerEndpoints,
}

impl AntigravityGenerateContentCaller {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            auth_dir: auth_dir.clone(),
            conductor: RuntimeConductor::new(auth_dir, strategy),
            default_proxy_url: None,
            user_agent: DEFAULT_ANTIGRAVITY_USER_AGENT.to_string(),
            endpoints: AntigravityCallerEndpoints::default(),
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

    pub fn with_endpoints(mut self, endpoints: AntigravityCallerEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn execute(
        &mut self,
        request: AntigravityGenerateContentRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<AntigravityCallerError>> {
        let request = RawAntigravityRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let request_time = options.pick.now;
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("antigravity", &model, options, move |selection| {
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
        request: AntigravityGenerateContentRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<reqwest::Response>, ExecuteWithRetryError<AntigravityCallerError>> {
        let request = RawAntigravityRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let request_time = options.pick.now;
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("antigravity", &model, options, move |selection| {
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
    default_user_agent: &str,
    endpoints: &AntigravityCallerEndpoints,
    auth_file_name: &str,
    request: &RawAntigravityRequest,
    now: DateTime<Utc>,
) -> Result<ProviderHttpResponse, ExecutionFailure<AntigravityCallerError>> {
    let mut auth = load_antigravity_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(antigravity_local_failure)?
        .ok_or_else(|| antigravity_auth_failure(AntigravityCallerError::MissingAccessToken))?;

    if refresh_due(&auth, now) && auth.refresh_token.is_some() {
        auth = refresh_antigravity_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(antigravity_auth_failure)?;
    }

    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(antigravity_auth_failure(
            AntigravityCallerError::MissingAccessToken,
        ));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(antigravity_request_failure)?;
    let body = normalize_request_body(&request.body, &request.model, auth.project_id.as_deref());
    let mut response = send_antigravity_request(
        &client,
        &auth,
        default_user_agent,
        &body,
        ANTIGRAVITY_GENERATE_CONTENT_PATH,
        "application/json",
    )
    .await
    .map_err(antigravity_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() {
        auth = refresh_antigravity_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(antigravity_auth_failure)?;
        response = send_antigravity_request(
            &client,
            &auth,
            default_user_agent,
            &body,
            ANTIGRAVITY_GENERATE_CONTENT_PATH,
            "application/json",
        )
        .await
        .map_err(antigravity_request_failure)?;
    }

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(antigravity_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(antigravity_status_failure(
            status,
            &headers,
            &body,
            request.model.as_str(),
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
    default_user_agent: &str,
    endpoints: &AntigravityCallerEndpoints,
    auth_file_name: &str,
    request: &RawAntigravityRequest,
    now: DateTime<Utc>,
) -> Result<reqwest::Response, ExecutionFailure<AntigravityCallerError>> {
    let mut auth = load_antigravity_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(antigravity_local_failure)?
        .ok_or_else(|| antigravity_auth_failure(AntigravityCallerError::MissingAccessToken))?;

    if refresh_due(&auth, now) && auth.refresh_token.is_some() {
        auth = refresh_antigravity_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(antigravity_auth_failure)?;
    }

    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(antigravity_auth_failure(
            AntigravityCallerError::MissingAccessToken,
        ));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(antigravity_request_failure)?;
    let body = normalize_request_body(&request.body, &request.model, auth.project_id.as_deref());
    let mut response = send_antigravity_request(
        &client,
        &auth,
        default_user_agent,
        &body,
        ANTIGRAVITY_STREAM_GENERATE_CONTENT_PATH,
        "text/event-stream",
    )
    .await
    .map_err(antigravity_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() {
        auth = refresh_antigravity_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(antigravity_auth_failure)?;
        response = send_antigravity_request(
            &client,
            &auth,
            default_user_agent,
            &body,
            ANTIGRAVITY_STREAM_GENERATE_CONTENT_PATH,
            "text/event-stream",
        )
        .await
        .map_err(antigravity_request_failure)?;
    }

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(antigravity_request_failure)?;
        return Err(antigravity_status_failure(
            status,
            &headers,
            &body,
            request.model.as_str(),
        ));
    }

    Ok(response)
}

impl From<AntigravityGenerateContentRequest> for RawAntigravityRequest {
    fn from(value: AntigravityGenerateContentRequest) -> Self {
        Self {
            model: value.model.trim().to_string(),
            body: value.body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AntigravityCallerEndpoints, AntigravityGenerateContentCaller,
        AntigravityGenerateContentRequest, DEFAULT_ANTIGRAVITY_CLIENT_ID,
        DEFAULT_ANTIGRAVITY_CLIENT_SECRET,
    };
    use crate::{ExecuteWithRetryOptions, RoutingStrategy};
    use axum::body::{to_bytes, Body};
    use axum::extract::State;
    use axum::http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use chrono::{TimeZone, Utc};
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
        body: Vec<u8>,
    }

    #[derive(Debug, Clone)]
    struct ServerState {
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
    }

    fn write_antigravity_auth(
        temp_dir: &TempDir,
        file_name: &str,
        access_token: Option<&str>,
        refresh_token: Option<&str>,
        base_url: Option<&str>,
        expired_at: Option<chrono::DateTime<Utc>>,
        project_id: Option<&str>,
        user_agent: Option<&str>,
    ) {
        let payload = json!({
            "id": file_name,
            "provider": "antigravity",
            "type": "antigravity",
            "email": "antigravity@example.com",
            "access_token": access_token,
            "refresh_token": refresh_token,
            "base_url": base_url,
            "expired": expired_at.map(|value| value.to_rfc3339()),
            "project_id": project_id,
            "user_agent": user_agent,
        });
        fs::write(
            temp_dir.path().join(file_name),
            serde_json::to_vec_pretty(&payload).expect("auth bytes"),
        )
        .expect("write auth");
    }

    #[tokio::test]
    async fn antigravity_caller_executes_with_normalized_request_body() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/v1internal:generateContent",
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
                            body: body.to_vec(),
                        });
                        (StatusCode::OK, Json(json!({ "id": "ag-ok" }))).into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_antigravity_auth(
            &temp_dir,
            "antigravity-a@example.com.json",
            Some("antigravity-token"),
            Some("antigravity-refresh"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
            Some("project-123"),
            Some("custom-antigravity-agent"),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller =
            AntigravityGenerateContentCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                AntigravityGenerateContentRequest {
                    model: "gemini-2.5-pro".to_string(),
                    body: br#"{"model":"ignored","toolConfig":{"functionCallingConfig":{"mode":"AUTO"}},"request":{"contents":[{"role":"user","parts":[{"text":"hello antigravity"}]}],"safetySettings":[{"category":"dangerous"}]}}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "antigravity-a@example.com.json");
        assert_eq!(executed.value.status, 200);

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1internal:generateContent");
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer antigravity-token")
        );
        assert_eq!(requests[0].accept.as_deref(), Some("application/json"));
        assert_eq!(
            requests[0].user_agent.as_deref(),
            Some("custom-antigravity-agent")
        );

        let body: Value = serde_json::from_slice(&requests[0].body).expect("request json");
        assert_eq!(body["model"].as_str(), Some("gemini-2.5-pro"));
        assert_eq!(body["userAgent"].as_str(), Some("antigravity"));
        assert_eq!(body["requestType"].as_str(), Some("agent"));
        assert_eq!(body["project"].as_str(), Some("project-123"));
        assert!(body["requestId"]
            .as_str()
            .is_some_and(|value| value.starts_with("agent-")));
        assert!(body["request"]["sessionId"]
            .as_str()
            .is_some_and(|value| value.starts_with('-')));
        assert_eq!(
            body["request"]["toolConfig"]["functionCallingConfig"]["mode"].as_str(),
            Some("AUTO")
        );
        assert!(body.get("toolConfig").is_none());
        assert!(body["request"].get("safetySettings").is_none());

        server.abort();
    }

    #[tokio::test]
    async fn antigravity_caller_streams_with_normalized_request_body() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/v1internal:streamGenerateContent",
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
                            body: body.to_vec(),
                        });
                        (
                            StatusCode::OK,
                            [(CONTENT_TYPE, "text/event-stream")],
                            "data: {\"id\":\"ag-stream\"}\n\n",
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

        write_antigravity_auth(
            &temp_dir,
            "antigravity-a@example.com.json",
            Some("antigravity-token"),
            Some("antigravity-refresh"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
            Some("project-123"),
            Some("custom-antigravity-agent"),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller =
            AntigravityGenerateContentCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute_stream(
                AntigravityGenerateContentRequest {
                    model: "gemini-2.5-flash".to_string(),
                    body: br#"{"model":"ignored","request":{"contents":[{"role":"user","parts":[{"text":"hello stream"}]}]}}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "antigravity-a@example.com.json");
        assert_eq!(executed.value.status().as_u16(), 200);
        assert_eq!(
            executed
                .value
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        let body = executed.value.bytes().await.expect("stream body");
        assert_eq!(body.as_ref(), b"data: {\"id\":\"ag-stream\"}\n\n");

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1internal:streamGenerateContent");
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer antigravity-token")
        );
        assert_eq!(requests[0].accept.as_deref(), Some("text/event-stream"));
        assert_eq!(
            requests[0].user_agent.as_deref(),
            Some("custom-antigravity-agent")
        );
        let body: Value = serde_json::from_slice(&requests[0].body).expect("request json");
        assert_eq!(body["model"], json!("gemini-2.5-flash"));
        assert_eq!(body["project"], json!("project-123"));
        assert_eq!(body["userAgent"], json!("antigravity"));
        assert_eq!(body["requestType"], json!("agent"));
        assert_eq!(
            body["request"]["contents"][0]["parts"][0]["text"],
            json!("hello stream")
        );

        server.abort();
    }

    #[tokio::test]
    async fn antigravity_caller_refreshes_expired_token_and_persists_it() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let addr = listener.local_addr().expect("addr");
        let base_url = format!("http://{addr}");
        let app = Router::new()
            .route(
                "/v1internal:generateContent",
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
                            body: body.to_vec(),
                        });
                        (StatusCode::OK, Json(json!({ "id": "ag-refreshed" }))).into_response()
                    },
                ),
            )
            .route(
                "/oauth2/token",
                post(|| async {
                    (
                        StatusCode::OK,
                        Json(json!({
                            "access_token": "antigravity-new-token",
                            "refresh_token": "antigravity-new-refresh",
                            "expires_in": 3600,
                            "token_type": "Bearer"
                        })),
                    )
                        .into_response()
                }),
            )
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_antigravity_auth(
            &temp_dir,
            "antigravity-a@example.com.json",
            Some("antigravity-old-token"),
            Some("antigravity-old-refresh"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 5, 11, 59, 0).unwrap()),
            Some("project-123"),
            None,
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller =
            AntigravityGenerateContentCaller::new(temp_dir.path(), RoutingStrategy::FillFirst)
                .with_endpoints(AntigravityCallerEndpoints {
                    api_base_url: base_url.clone(),
                    token_url: format!("http://{addr}/oauth2/token"),
                    client_id: DEFAULT_ANTIGRAVITY_CLIENT_ID.to_string(),
                    client_secret: DEFAULT_ANTIGRAVITY_CLIENT_SECRET.to_string(),
                });
        let executed = caller
            .execute(
                AntigravityGenerateContentRequest {
                    model: "gemini-2.5-pro".to_string(),
                    body: br#"{"model":"gemini-2.5-pro","request":{"contents":[]}}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.value.status, 200);
        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer antigravity-new-token")
        );

        let auth_payload: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("antigravity-a@example.com.json")).expect("read auth"),
        )
        .expect("auth json");
        assert_eq!(
            auth_payload["access_token"].as_str(),
            Some("antigravity-new-token")
        );
        assert_eq!(
            auth_payload["refresh_token"].as_str(),
            Some("antigravity-new-refresh")
        );
        assert_eq!(auth_payload["expires_in"].as_i64(), Some(3600));
        assert_eq!(
            auth_payload["expired"].as_str(),
            Some("2026-04-05T13:00:00+00:00")
        );
        assert_eq!(
            auth_payload["last_refresh"].as_str(),
            Some("2026-04-05T12:00:00+00:00")
        );

        server.abort();
    }

    #[tokio::test]
    async fn antigravity_caller_rotates_after_retryable_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ServerState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/v1internal:generateContent",
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

                        (StatusCode::OK, Json(json!({ "id": "ag-ok" }))).into_response()
                    },
                ),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_antigravity_auth(
            &temp_dir,
            "antigravity-a@example.com.json",
            Some("token-a"),
            Some("refresh-a"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
            Some("project-a"),
            None,
        );
        write_antigravity_auth(
            &temp_dir,
            "antigravity-b@example.com.json",
            Some("token-b"),
            Some("refresh-b"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
            Some("project-b"),
            None,
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller =
            AntigravityGenerateContentCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                AntigravityGenerateContentRequest {
                    model: "gemini-2.5-pro".to_string(),
                    body: br#"{"model":"gemini-2.5-pro","request":{"contents":[]}}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "antigravity-b@example.com.json");
        assert_eq!(executed.value.status, 200);

        let failed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("antigravity-a@example.com.json")).expect("read auth a"),
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
    async fn antigravity_caller_stops_on_terminal_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let app = Router::new().route(
            "/v1internal:generateContent",
            post(|| async { (StatusCode::BAD_REQUEST, "invalid request").into_response() }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_antigravity_auth(
            &temp_dir,
            "antigravity-a@example.com.json",
            Some("token-a"),
            Some("refresh-a"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
            Some("project-a"),
            None,
        );
        write_antigravity_auth(
            &temp_dir,
            "antigravity-b@example.com.json",
            Some("token-b"),
            Some("refresh-b"),
            Some(&base_url),
            Some(Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap()),
            Some("project-b"),
            None,
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller =
            AntigravityGenerateContentCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let error = caller
            .execute(
                AntigravityGenerateContentRequest {
                    model: "gemini-2.5-pro".to_string(),
                    body: br#"{"model":"gemini-2.5-pro","request":{"contents":[]}}"#.to_vec(),
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
            "antigravity returned 400: invalid request"
        );

        let untouched_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("antigravity-b@example.com.json")).expect("read auth b"),
        )
        .expect("auth b json");
        assert_eq!(untouched_auth["status"], Value::Null);

        server.abort();
    }
}
