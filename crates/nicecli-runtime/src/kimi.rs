use crate::{
    ExecuteWithRetryError, ExecuteWithRetryOptions, Executed, ExecutionError, ExecutionFailure,
    ExecutionResult, ProviderHttpResponse, RoutingStrategy, RuntimeConductor,
};
use chrono::{DateTime, Duration, Utc};
use nicecli_auth::{read_auth_file, write_auth_file, AuthFileStoreError};
use reqwest::header::HeaderMap;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use thiserror::Error;

mod helpers;

use self::helpers::*;

const DEFAULT_KIMI_BASE_URL: &str = "https://api.kimi.com/coding";
const DEFAULT_KIMI_TOKEN_URL: &str = "https://auth.kimi.com/api/oauth/token";
const DEFAULT_KIMI_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const DEFAULT_KIMI_USER_AGENT: &str = "KimiCLI/1.10.6";
const DEFAULT_KIMI_PLATFORM: &str = "kimi_cli";
const DEFAULT_KIMI_VERSION: &str = "1.10.6";
const DEFAULT_KIMI_DEVICE_ID: &str = "nicecli-device";
const KIMI_REFRESH_LEAD_SECS: i64 = 5 * 60;
const KIMI_CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiChatCompletionsRequest {
    pub model: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawKimiRequest {
    model: String,
    body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiCallerEndpoints {
    pub api_base_url: String,
    pub token_url: String,
    pub client_id: String,
}

impl Default for KimiCallerEndpoints {
    fn default() -> Self {
        Self {
            api_base_url: DEFAULT_KIMI_BASE_URL.to_string(),
            token_url: DEFAULT_KIMI_TOKEN_URL.to_string(),
            client_id: DEFAULT_KIMI_CLIENT_ID.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum KimiCallerError {
    #[error("failed to read kimi auth file: {0}")]
    ReadAuthFile(AuthFileStoreError),
    #[error("failed to persist kimi auth file: {0}")]
    WriteAuthFile(AuthFileStoreError),
    #[error("kimi auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("kimi auth is missing access_token")]
    MissingAccessToken,
    #[error("kimi request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("kimi refresh returned {status}: {body}")]
    RefreshRejected { status: u16, body: String },
    #[error("kimi returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug, Clone)]
struct KimiAuthState {
    root: Map<String, Value>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    base_url: String,
    proxy_url: Option<String>,
    device_id: String,
    expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
struct RefreshedKimiToken {
    access_token: String,
    refresh_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
    expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, serde::Deserialize)]
struct RefreshTokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: f64,
    #[serde(default)]
    scope: String,
}

#[derive(Debug)]
pub struct KimiChatCaller {
    auth_dir: PathBuf,
    conductor: RuntimeConductor,
    default_proxy_url: Option<String>,
    user_agent: String,
    endpoints: KimiCallerEndpoints,
}

impl KimiChatCaller {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            auth_dir: auth_dir.clone(),
            conductor: RuntimeConductor::new(auth_dir, strategy),
            default_proxy_url: None,
            user_agent: DEFAULT_KIMI_USER_AGENT.to_string(),
            endpoints: KimiCallerEndpoints::default(),
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

    pub fn with_endpoints(mut self, endpoints: KimiCallerEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn execute(
        &mut self,
        request: KimiChatCompletionsRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<KimiCallerError>> {
        let request = RawKimiRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("kimi", &model, options, move |selection| {
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
                    )
                    .await
                }
            })
            .await
    }

    pub async fn execute_stream(
        &mut self,
        request: KimiChatCompletionsRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<reqwest::Response>, ExecuteWithRetryError<KimiCallerError>> {
        let request = RawKimiRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("kimi", &model, options, move |selection| {
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
    endpoints: &KimiCallerEndpoints,
    auth_file_name: &str,
    request: &RawKimiRequest,
) -> Result<ProviderHttpResponse, ExecutionFailure<KimiCallerError>> {
    let mut auth = load_kimi_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(kimi_local_failure)?
        .ok_or_else(|| kimi_auth_failure(KimiCallerError::MissingAccessToken))?;

    if refresh_due(&auth) && auth.refresh_token.is_some() {
        auth = refresh_kimi_auth(auth_dir, auth_file_name, auth, default_proxy_url, endpoints)
            .await
            .map_err(kimi_auth_failure)?;
    }

    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(kimi_auth_failure(KimiCallerError::MissingAccessToken));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(kimi_request_failure)?;
    let body = normalize_request_body(&request.body, &request.model);
    let mut response = send_kimi_request(&client, &auth, user_agent, &body, "application/json")
        .await
        .map_err(kimi_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() {
        auth = refresh_kimi_auth(auth_dir, auth_file_name, auth, default_proxy_url, endpoints)
            .await
            .map_err(kimi_auth_failure)?;
        response = send_kimi_request(&client, &auth, user_agent, &body, "application/json")
            .await
            .map_err(kimi_request_failure)?;
    }

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(kimi_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(kimi_status_failure(
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
    user_agent: &str,
    endpoints: &KimiCallerEndpoints,
    auth_file_name: &str,
    request: &RawKimiRequest,
) -> Result<reqwest::Response, ExecutionFailure<KimiCallerError>> {
    let mut auth = load_kimi_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(kimi_local_failure)?
        .ok_or_else(|| kimi_auth_failure(KimiCallerError::MissingAccessToken))?;

    if refresh_due(&auth) && auth.refresh_token.is_some() {
        auth = refresh_kimi_auth(auth_dir, auth_file_name, auth, default_proxy_url, endpoints)
            .await
            .map_err(kimi_auth_failure)?;
    }

    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(kimi_auth_failure(KimiCallerError::MissingAccessToken));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(kimi_request_failure)?;
    let body = normalize_request_body(&request.body, &request.model);
    let mut response = send_kimi_request(&client, &auth, user_agent, &body, "text/event-stream")
        .await
        .map_err(kimi_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() {
        auth = refresh_kimi_auth(auth_dir, auth_file_name, auth, default_proxy_url, endpoints)
            .await
            .map_err(kimi_auth_failure)?;
        response = send_kimi_request(&client, &auth, user_agent, &body, "text/event-stream")
            .await
            .map_err(kimi_request_failure)?;
    }

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(kimi_request_failure)?;
        return Err(kimi_status_failure(
            status,
            &headers,
            &body,
            request.model.as_str(),
        ));
    }

    Ok(response)
}

fn load_kimi_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    endpoints: &KimiCallerEndpoints,
) -> Result<Option<KimiAuthState>, KimiCallerError> {
    let raw = read_auth_file(auth_dir, auth_file_name).map_err(KimiCallerError::ReadAuthFile)?;
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|error| KimiCallerError::InvalidAuthFile(error.to_string()))?;
    let root = value
        .as_object()
        .ok_or_else(|| KimiCallerError::InvalidAuthFile("root must be an object".to_string()))?;

    let provider = first_non_empty([
        string_path(root, &["provider"]),
        string_path(root, &["type"]),
    ]);
    if !provider
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("kimi"))
    {
        return Ok(None);
    }

    Ok(Some(KimiAuthState {
        root: root.clone(),
        access_token: first_non_empty([
            string_path(root, &["access_token"]),
            string_path(root, &["metadata", "access_token"]),
            string_path(root, &["attributes", "access_token"]),
        ]),
        refresh_token: first_non_empty([
            string_path(root, &["refresh_token"]),
            string_path(root, &["metadata", "refresh_token"]),
            string_path(root, &["attributes", "refresh_token"]),
        ]),
        base_url: first_non_empty([
            string_path(root, &["base_url"]),
            string_path(root, &["metadata", "base_url"]),
            string_path(root, &["attributes", "base_url"]),
            Some(endpoints.api_base_url.clone()),
        ])
        .unwrap_or_else(|| endpoints.api_base_url.clone()),
        proxy_url: trimmed(first_non_empty([
            string_path(root, &["proxy_url"]),
            string_path(root, &["metadata", "proxy_url"]),
            string_path(root, &["attributes", "proxy_url"]),
        ])),
        device_id: first_non_empty([
            string_path(root, &["device_id"]),
            string_path(root, &["metadata", "device_id"]),
            string_path(root, &["attributes", "device_id"]),
        ])
        .unwrap_or_else(|| DEFAULT_KIMI_DEVICE_ID.to_string()),
        expired_at: first_non_empty([
            string_path(root, &["expired"]),
            string_path(root, &["metadata", "expired"]),
            string_path(root, &["attributes", "expired"]),
        ])
        .and_then(|value| parse_datetime(&value)),
    }))
}

fn parse_refresh_token_response(body: &[u8]) -> Result<RefreshedKimiToken, KimiCallerError> {
    let parsed: RefreshTokenResponse = serde_json::from_slice(body)
        .map_err(|error| KimiCallerError::InvalidAuthFile(error.to_string()))?;
    let access_token = parsed.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err(KimiCallerError::MissingAccessToken);
    }

    Ok(RefreshedKimiToken {
        access_token,
        refresh_token: trimmed(Some(parsed.refresh_token)),
        token_type: trimmed(Some(parsed.token_type)),
        scope: trimmed(Some(parsed.scope)),
        expired_at: (parsed.expires_in > 0.0)
            .then(|| Utc::now() + Duration::seconds(parsed.expires_in.max(0.0).round() as i64)),
    })
}

fn apply_refreshed_auth_state(auth: &mut KimiAuthState, refreshed: &RefreshedKimiToken) {
    auth.access_token = Some(refreshed.access_token.clone());
    if let Some(refresh_token) = refreshed.refresh_token.clone() {
        auth.refresh_token = Some(refresh_token);
    }
    auth.expired_at = refreshed.expired_at;
    upsert_string(
        &mut auth.root,
        "access_token",
        Some(refreshed.access_token.as_str()),
    );
    upsert_string(
        &mut auth.root,
        "refresh_token",
        auth.refresh_token.as_deref(),
    );
    upsert_string(
        &mut auth.root,
        "token_type",
        refreshed.token_type.as_deref(),
    );
    upsert_string(&mut auth.root, "scope", refreshed.scope.as_deref());
    upsert_string(&mut auth.root, "device_id", Some(auth.device_id.as_str()));
    upsert_string(
        &mut auth.root,
        "last_refresh",
        Some(&Utc::now().to_rfc3339()),
    );
    upsert_string(
        &mut auth.root,
        "expired",
        refreshed
            .expired_at
            .map(|value| value.to_rfc3339())
            .as_deref(),
    );
}

fn persist_kimi_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    auth: &KimiAuthState,
) -> Result<(), KimiCallerError> {
    let bytes = serde_json::to_vec_pretty(&Value::Object(auth.root.clone()))
        .map_err(|error| KimiCallerError::InvalidAuthFile(error.to_string()))?;
    write_auth_file(auth_dir, auth_file_name, &bytes).map_err(KimiCallerError::WriteAuthFile)?;
    Ok(())
}

fn normalize_request_body(body: &[u8], model: &str) -> Vec<u8> {
    let Some(mut value) = serde_json::from_slice::<Value>(body).ok() else {
        return body.to_vec();
    };
    let Some(object) = value.as_object_mut() else {
        return body.to_vec();
    };
    object.insert(
        "model".to_string(),
        Value::String(strip_kimi_prefix(model).to_string()),
    );
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn strip_kimi_prefix(model: &str) -> &str {
    let trimmed = model.trim();
    if trimmed.len() > 5 && trimmed[..5].eq_ignore_ascii_case("kimi-") {
        &trimmed[5..]
    } else {
        trimmed
    }
}

fn kimi_status_failure(
    status: u16,
    headers: &HeaderMap,
    body: &[u8],
    model: &str,
) -> ExecutionFailure<KimiCallerError> {
    let message = response_body_message(body);
    let error = KimiCallerError::UnexpectedStatus {
        status,
        body: message.clone(),
    };
    let result = ExecutionResult {
        model: normalized_model(model),
        success: false,
        retry_after: parse_retry_after(status, headers),
        error: Some(ExecutionError {
            message,
            http_status: Some(status),
        }),
    };
    if is_retryable_status(status) {
        ExecutionFailure::retryable(error, result)
    } else {
        ExecutionFailure::terminal(error, result)
    }
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 401 | 402 | 403 | 408 | 429 | 500 | 502 | 503 | 504)
}

fn parse_retry_after(status: u16, headers: &HeaderMap) -> Option<Duration> {
    if status != 429 {
        return None;
    }
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::seconds)
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn upsert_string(object: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        object.remove(key);
        return;
    };
    object.insert(key.to_string(), Value::String(value.to_string()));
}

fn string_path(root: &Map<String, Value>, path: &[&str]) -> Option<String> {
    let mut current = root.get(*path.first()?)?;
    for segment in path.iter().skip(1) {
        current = current.as_object()?.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn first_non_empty<const N: usize>(values: [Option<String>; N]) -> Option<String> {
    values.into_iter().find_map(trimmed)
}

fn value_string_path(root: &Value, path: &[&str]) -> Option<String> {
    let mut current = root;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl From<KimiChatCompletionsRequest> for RawKimiRequest {
    fn from(value: KimiChatCompletionsRequest) -> Self {
        Self {
            model: value.model,
            body: value.body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KimiCallerEndpoints, KimiChatCaller, KimiChatCompletionsRequest, DEFAULT_KIMI_CLIENT_ID,
    };
    use crate::{ExecuteWithRetryError, ExecuteWithRetryOptions, RoutingStrategy};
    use axum::body::Bytes;
    use axum::http::header::{HeaderValue, AUTHORIZATION, RETRY_AFTER};
    use axum::http::{HeaderMap, StatusCode};
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
        authorization: Option<String>,
        device_id: Option<String>,
        body: Vec<u8>,
    }

    fn write_kimi_auth(
        temp_dir: &TempDir,
        file_name: &str,
        access_token: Option<&str>,
        refresh_token: Option<&str>,
        base_url: &str,
        expired: Option<&str>,
        device_id: Option<&str>,
    ) {
        let mut payload = json!({
            "type": "kimi",
            "provider": "kimi",
            "email": "demo@example.com",
            "base_url": base_url,
        });
        if let Some(access_token) = access_token {
            payload["access_token"] = Value::String(access_token.to_string());
        }
        if let Some(refresh_token) = refresh_token {
            payload["refresh_token"] = Value::String(refresh_token.to_string());
        }
        if let Some(expired) = expired {
            payload["expired"] = Value::String(expired.to_string());
        }
        if let Some(device_id) = device_id {
            payload["device_id"] = Value::String(device_id.to_string());
        }
        fs::write(
            temp_dir.path().join(file_name),
            serde_json::to_vec_pretty(&payload).expect("serialize auth"),
        )
        .expect("write auth");
    }

    #[tokio::test]
    async fn kimi_chat_caller_executes_with_model_prefix_stripped() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let route_requests = requests.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |headers: HeaderMap, body: Bytes| {
                let route_requests = route_requests.clone();
                async move {
                    route_requests.lock().expect("lock").push(RecordedRequest {
                        authorization: headers
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string),
                        device_id: headers
                            .get("X-Msh-Device-Id")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string),
                        body: body.to_vec(),
                    });
                    (StatusCode::OK, r#"{"id":"kimi-ok"}"#).into_response()
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_kimi_auth(
            &temp_dir,
            "kimi-a@example.com.json",
            Some("kimi-token"),
            Some("kimi-refresh"),
            &base_url,
            None,
            Some("device-123"),
        );

        let mut caller = KimiChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                KimiChatCompletionsRequest {
                    model: "kimi-k2.5".to_string(),
                    body: br#"{"model":"kimi-k2.5","messages":[{"role":"user","content":"hi"}]}"#
                        .to_vec(),
                },
                ExecuteWithRetryOptions::new(Utc::now()),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "kimi-a@example.com.json");
        assert_eq!(executed.value.status, 200);

        let requests = requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer kimi-token")
        );
        assert_eq!(requests[0].device_id.as_deref(), Some("device-123"));
        let body: Value = serde_json::from_slice(&requests[0].body).expect("request json");
        assert_eq!(body["model"].as_str(), Some("k2.5"));

        server.abort();
    }

    #[tokio::test]
    async fn kimi_chat_caller_refreshes_expired_token_and_persists_it() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let request_route = requests.clone();
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(move |headers: HeaderMap, body: Bytes| {
                    let request_route = request_route.clone();
                    async move {
                        request_route.lock().expect("lock").push(RecordedRequest {
                            authorization: headers
                                .get(AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            device_id: headers
                                .get("X-Msh-Device-Id")
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        (StatusCode::OK, r#"{"id":"kimi-refreshed"}"#).into_response()
                    }
                }),
            )
            .route(
                "/api/oauth/token",
                post(|| async {
                    Json(json!({
                        "access_token": "refreshed-token",
                        "refresh_token": "refreshed-refresh-token",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                        "scope": "openid profile"
                    }))
                }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let token_url = format!("{base_url}/api/oauth/token");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_kimi_auth(
            &temp_dir,
            "kimi-a@example.com.json",
            Some("expired-token"),
            Some("old-refresh-token"),
            &base_url,
            Some("2020-01-01T00:00:00Z"),
            Some("device-refresh-123"),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller = KimiChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst)
            .with_endpoints(KimiCallerEndpoints {
                api_base_url: base_url.clone(),
                token_url,
                client_id: DEFAULT_KIMI_CLIENT_ID.to_string(),
            });
        let executed = caller
            .execute(
                KimiChatCompletionsRequest {
                    model: "kimi-k2".to_string(),
                    body: br#"{"model":"kimi-k2","messages":[]}"#.to_vec(),
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
            Some("Bearer refreshed-token")
        );

        let persisted: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("kimi-a@example.com.json")).expect("read auth"),
        )
        .expect("auth json");
        assert_eq!(persisted["access_token"].as_str(), Some("refreshed-token"));
        assert_eq!(
            persisted["refresh_token"].as_str(),
            Some("refreshed-refresh-token")
        );
        assert_eq!(persisted["token_type"].as_str(), Some("Bearer"));
        assert_eq!(persisted["scope"].as_str(), Some("openid profile"));
        assert_eq!(persisted["device_id"].as_str(), Some("device-refresh-123"));
        assert!(persisted["expired"].as_str().is_some());
        assert!(persisted["last_refresh"].as_str().is_some());

        server.abort();
    }

    #[tokio::test]
    async fn kimi_chat_caller_rotates_after_retryable_status() {
        let temp_dir = TempDir::new().expect("temp dir");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let route_requests = requests.clone();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |headers: HeaderMap, body: Bytes| {
                let route_requests = route_requests.clone();
                async move {
                    let authorization = headers
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    route_requests.lock().expect("lock").push(RecordedRequest {
                        authorization: authorization.clone(),
                        device_id: headers
                            .get("X-Msh-Device-Id")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string),
                        body: body.to_vec(),
                    });
                    match authorization.as_deref() {
                        Some("Bearer token-a") => {
                            let mut response = (
                                StatusCode::TOO_MANY_REQUESTS,
                                r#"{"error":{"message":"quota exhausted"}}"#,
                            )
                                .into_response();
                            response
                                .headers_mut()
                                .insert(RETRY_AFTER, HeaderValue::from_static("120"));
                            response
                        }
                        Some("Bearer token-b") => {
                            (StatusCode::OK, r#"{"id":"kimi-ok"}"#).into_response()
                        }
                        _ => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
                    }
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        write_kimi_auth(
            &temp_dir,
            "kimi-a@example.com.json",
            Some("token-a"),
            Some("refresh-a"),
            &base_url,
            None,
            Some("device-a"),
        );
        write_kimi_auth(
            &temp_dir,
            "kimi-b@example.com.json",
            Some("token-b"),
            Some("refresh-b"),
            &base_url,
            None,
            Some("device-b"),
        );

        let now = Utc.with_ymd_and_hms(2026, 4, 5, 12, 0, 0).unwrap();
        let mut caller = KimiChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let executed = caller
            .execute(
                KimiChatCompletionsRequest {
                    model: "kimi-k2-thinking".to_string(),
                    body: br#"{"model":"kimi-k2-thinking","messages":[]}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(now),
            )
            .await
            .expect("executed");

        assert_eq!(executed.selection.auth_id, "kimi-b@example.com.json");
        assert_eq!(executed.value.status, 200);

        let failed_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("kimi-a@example.com.json")).expect("read auth a"),
        )
        .expect("auth a json");
        assert_eq!(failed_auth["status"].as_str(), Some("error"));
        assert_eq!(
            failed_auth["status_message"].as_str(),
            Some("quota exhausted")
        );
        assert_eq!(failed_auth["quota"]["exceeded"].as_bool(), Some(true));
        let next_retry_after = failed_auth["next_retry_after"]
            .as_str()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc));
        assert_eq!(next_retry_after, Some(now + Duration::seconds(120)));

        server.abort();
    }

    #[tokio::test]
    async fn kimi_chat_caller_stops_on_terminal_status() {
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

        write_kimi_auth(
            &temp_dir,
            "kimi-a@example.com.json",
            Some("token-a"),
            Some("refresh-a"),
            &base_url,
            None,
            Some("device-a"),
        );
        write_kimi_auth(
            &temp_dir,
            "kimi-b@example.com.json",
            Some("token-b"),
            Some("refresh-b"),
            &base_url,
            None,
            Some("device-b"),
        );

        let mut caller = KimiChatCaller::new(temp_dir.path(), RoutingStrategy::FillFirst);
        let error = caller
            .execute(
                KimiChatCompletionsRequest {
                    model: "kimi-k2".to_string(),
                    body: br#"{"model":"kimi-k2","messages":[]}"#.to_vec(),
                },
                ExecuteWithRetryOptions::new(Utc::now()),
            )
            .await
            .expect_err("terminal error");

        match error {
            ExecuteWithRetryError::Provider(error) => {
                assert!(error.to_string().contains("400"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let untouched_auth: Value = serde_json::from_slice(
            &fs::read(temp_dir.path().join("kimi-b@example.com.json")).expect("read auth b"),
        )
        .expect("auth b json");
        assert!(untouched_auth.get("status").is_none());

        server.abort();
    }
}
