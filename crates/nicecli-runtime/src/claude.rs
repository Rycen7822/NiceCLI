use crate::{
    ExecuteWithRetryError, ExecuteWithRetryOptions, Executed, ExecutionError, ExecutionFailure,
    ExecutionResult, ProviderHttpResponse, RoutingStrategy, RuntimeConductor,
};
use chrono::{DateTime, Duration, Utc};
use nicecli_auth::{read_auth_file, write_auth_file, AuthFileStoreError};
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::{Client, Proxy, Url};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

const DEFAULT_CLAUDE_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_CLAUDE_TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
const DEFAULT_CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const DEFAULT_CLAUDE_USER_AGENT: &str = "claude-cli/1.0";
const CLAUDE_MESSAGES_PATH: &str = "/v1/messages";
const CLAUDE_COUNT_TOKENS_PATH: &str = "/v1/messages/count_tokens";
const CLAUDE_REFRESH_LEAD_SECS: i64 = 5 * 60;
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_ANTHROPIC_BETA: &str = "claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27,prompt-caching-scope-2026-01-05";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeMessagesRequest {
    pub model: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawClaudeRequest {
    model: String,
    body: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudeRequestKind {
    Messages,
    CountTokens,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedClaudeRequest {
    body: Vec<u8>,
    extra_betas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCallerEndpoints {
    pub api_base_url: String,
    pub token_url: String,
    pub client_id: String,
}

impl Default for ClaudeCallerEndpoints {
    fn default() -> Self {
        Self {
            api_base_url: DEFAULT_CLAUDE_BASE_URL.to_string(),
            token_url: DEFAULT_CLAUDE_TOKEN_URL.to_string(),
            client_id: DEFAULT_CLAUDE_CLIENT_ID.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ClaudeCallerError {
    #[error("failed to read claude auth file: {0}")]
    ReadAuthFile(AuthFileStoreError),
    #[error("failed to persist claude auth file: {0}")]
    WriteAuthFile(AuthFileStoreError),
    #[error("claude auth file is invalid: {0}")]
    InvalidAuthFile(String),
    #[error("claude auth is missing access token")]
    MissingAccessToken,
    #[error("claude request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("claude refresh returned {status}: {body}")]
    RefreshRejected { status: u16, body: String },
    #[error("claude returned {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug, Clone)]
struct ClaudeAuthState {
    root: Map<String, Value>,
    access_token: Option<String>,
    api_key: Option<String>,
    refresh_token: Option<String>,
    base_url: String,
    proxy_url: Option<String>,
    expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RefreshedClaudeToken {
    access_token: String,
    refresh_token: Option<String>,
    token_type: Option<String>,
    email: Option<String>,
    expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct RefreshTokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    account: RefreshAccount,
}

#[derive(Debug, Default, Deserialize)]
struct RefreshAccount {
    #[serde(default)]
    email_address: String,
}

#[derive(Debug)]
pub struct ClaudeMessagesCaller {
    auth_dir: PathBuf,
    conductor: RuntimeConductor,
    default_proxy_url: Option<String>,
    user_agent: String,
    endpoints: ClaudeCallerEndpoints,
}

impl ClaudeMessagesCaller {
    pub fn new(auth_dir: impl Into<PathBuf>, strategy: RoutingStrategy) -> Self {
        let auth_dir = auth_dir.into();
        Self {
            auth_dir: auth_dir.clone(),
            conductor: RuntimeConductor::new(auth_dir, strategy),
            default_proxy_url: None,
            user_agent: DEFAULT_CLAUDE_USER_AGENT.to_string(),
            endpoints: ClaudeCallerEndpoints::default(),
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

    pub fn with_endpoints(mut self, endpoints: ClaudeCallerEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn execute(
        &mut self,
        request: ClaudeMessagesRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<ClaudeCallerError>> {
        self.execute_http(ClaudeRequestKind::Messages, request, options)
            .await
    }

    pub async fn execute_stream(
        &mut self,
        request: ClaudeMessagesRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<reqwest::Response>, ExecuteWithRetryError<ClaudeCallerError>> {
        let request = RawClaudeRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let request_time = options.pick.now;
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("claude", &model, options, move |selection| {
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

    pub async fn count_tokens(
        &mut self,
        request: ClaudeMessagesRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<ClaudeCallerError>> {
        self.execute_http(ClaudeRequestKind::CountTokens, request, options)
            .await
    }

    async fn execute_http(
        &mut self,
        kind: ClaudeRequestKind,
        request: ClaudeMessagesRequest,
        options: ExecuteWithRetryOptions,
    ) -> Result<Executed<ProviderHttpResponse>, ExecuteWithRetryError<ClaudeCallerError>> {
        let request = RawClaudeRequest::from(request);
        let auth_dir = self.auth_dir.clone();
        let default_proxy_url = self.default_proxy_url.clone();
        let user_agent = self.user_agent.clone();
        let endpoints = self.endpoints.clone();
        let request_time = options.pick.now;
        let model = request.model.trim().to_string();
        self.conductor
            .execute_single_with_retry("claude", &model, options, move |selection| {
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
                        kind,
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
    endpoints: &ClaudeCallerEndpoints,
    auth_file_name: &str,
    request: &RawClaudeRequest,
    kind: ClaudeRequestKind,
    now: DateTime<Utc>,
) -> Result<ProviderHttpResponse, ExecutionFailure<ClaudeCallerError>> {
    let mut auth = load_claude_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(claude_local_failure)?
        .ok_or_else(|| claude_auth_failure(ClaudeCallerError::MissingAccessToken))?;

    if refresh_due(&auth, now) && auth.refresh_token.is_some() {
        auth = refresh_claude_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(claude_auth_failure)?;
    }

    if resolved_credential(&auth).is_none() {
        return Err(claude_auth_failure(ClaudeCallerError::MissingAccessToken));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(claude_request_failure)?;
    let mut response = send_claude_request(&client, &auth, user_agent, request, kind, false)
        .await
        .map_err(claude_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() && auth.api_key.is_none() {
        auth = refresh_claude_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(claude_auth_failure)?;
        response = send_claude_request(&client, &auth, user_agent, request, kind, false)
            .await
            .map_err(claude_request_failure)?;
    }

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(claude_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(claude_status_failure(
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
    endpoints: &ClaudeCallerEndpoints,
    auth_file_name: &str,
    request: &RawClaudeRequest,
    now: DateTime<Utc>,
) -> Result<reqwest::Response, ExecutionFailure<ClaudeCallerError>> {
    let mut auth = load_claude_auth_state(auth_dir, auth_file_name, endpoints)
        .map_err(claude_local_failure)?
        .ok_or_else(|| claude_auth_failure(ClaudeCallerError::MissingAccessToken))?;

    if refresh_due(&auth, now) && auth.refresh_token.is_some() {
        auth = refresh_claude_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(claude_auth_failure)?;
    }

    if resolved_credential(&auth).is_none() {
        return Err(claude_auth_failure(ClaudeCallerError::MissingAccessToken));
    }

    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))
        .map_err(claude_request_failure)?;
    let mut response = send_claude_request(
        &client,
        &auth,
        user_agent,
        request,
        ClaudeRequestKind::Messages,
        true,
    )
    .await
    .map_err(claude_request_failure)?;

    if response.status().as_u16() == 401 && auth.refresh_token.is_some() && auth.api_key.is_none() {
        auth = refresh_claude_auth(
            auth_dir,
            auth_file_name,
            auth,
            default_proxy_url,
            endpoints,
            now,
        )
        .await
        .map_err(claude_auth_failure)?;
        response = send_claude_request(
            &client,
            &auth,
            user_agent,
            request,
            ClaudeRequestKind::Messages,
            true,
        )
        .await
        .map_err(claude_request_failure)?;
    }

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(claude_request_failure)?;
        return Err(claude_status_failure(
            status,
            &headers,
            &body,
            request.model.as_str(),
        ));
    }

    Ok(response)
}

async fn send_claude_request(
    client: &Client,
    auth: &ClaudeAuthState,
    user_agent: &str,
    request: &RawClaudeRequest,
    kind: ClaudeRequestKind,
    stream: bool,
) -> Result<reqwest::Response, reqwest::Error> {
    let prepared = prepare_request_body(&request.body, &request.model);
    let url = format!(
        "{}{}?beta=true",
        auth.base_url.trim_end_matches('/'),
        request_path(kind)
    );
    let mut builder = client
        .post(url.as_str())
        .header(CONTENT_TYPE, "application/json")
        .header(
            ACCEPT,
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .header(
            USER_AGENT,
            if user_agent.trim().is_empty() {
                DEFAULT_CLAUDE_USER_AGENT
            } else {
                user_agent.trim()
            },
        )
        .header("Anthropic-Version", DEFAULT_ANTHROPIC_VERSION)
        .header("Anthropic-Beta", merged_betas(&prepared.extra_betas))
        .header("Anthropic-Dangerous-Direct-Browser-Access", "true")
        .header("X-App", "cli")
        .header("Connection", "keep-alive")
        .header(
            "Accept-Encoding",
            if stream {
                "identity"
            } else {
                "gzip, deflate, br, zstd"
            },
        );

    if let Some((credential, is_api_key)) = resolved_credential(auth) {
        if is_api_key && should_use_api_key_header(url.as_str()) {
            builder = builder.header("x-api-key", credential);
        } else {
            builder = builder.header(AUTHORIZATION, format!("Bearer {credential}"));
        }
    }

    builder.body(prepared.body).send().await
}

async fn refresh_claude_auth(
    auth_dir: &Path,
    auth_file_name: &str,
    mut auth: ClaudeAuthState,
    default_proxy_url: Option<&str>,
    endpoints: &ClaudeCallerEndpoints,
    now: DateTime<Utc>,
) -> Result<ClaudeAuthState, ClaudeCallerError> {
    let refresh_token = auth
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ClaudeCallerError::MissingAccessToken)?;
    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))?;
    let response = client
        .post(&endpoints.token_url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .json(&serde_json::json!({
            "client_id": endpoints.client_id,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await?;

    let status = response.status().as_u16();
    let body = response.bytes().await?.to_vec();
    if !(200..300).contains(&status) {
        return Err(ClaudeCallerError::RefreshRejected {
            status,
            body: response_body_message(&body),
        });
    }

    let refreshed = parse_refresh_token_response(&body, now)?;
    apply_refreshed_auth_state(&mut auth, &refreshed, now);
    persist_claude_auth_state(auth_dir, auth_file_name, &auth)?;
    Ok(auth)
}

fn load_claude_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    endpoints: &ClaudeCallerEndpoints,
) -> Result<Option<ClaudeAuthState>, ClaudeCallerError> {
    let raw = read_auth_file(auth_dir, auth_file_name).map_err(ClaudeCallerError::ReadAuthFile)?;
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|error| ClaudeCallerError::InvalidAuthFile(error.to_string()))?;
    let root = value
        .as_object()
        .ok_or_else(|| ClaudeCallerError::InvalidAuthFile("root must be an object".to_string()))?;

    let provider = first_non_empty([
        string_path(root, &["provider"]),
        string_path(root, &["type"]),
    ]);
    if !provider
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("claude"))
    {
        return Ok(None);
    }

    Ok(Some(ClaudeAuthState {
        root: root.clone(),
        access_token: first_non_empty([
            string_path(root, &["access_token"]),
            string_path(root, &["metadata", "access_token"]),
            string_path(root, &["attributes", "access_token"]),
        ]),
        api_key: first_non_empty([
            string_path(root, &["api_key"]),
            string_path(root, &["api-key"]),
            string_path(root, &["metadata", "api_key"]),
            string_path(root, &["metadata", "api-key"]),
            string_path(root, &["attributes", "api_key"]),
            string_path(root, &["attributes", "api-key"]),
        ]),
        refresh_token: first_non_empty([
            string_path(root, &["refresh_token"]),
            string_path(root, &["metadata", "refresh_token"]),
            string_path(root, &["attributes", "refresh_token"]),
        ]),
        base_url: first_non_empty([
            string_path(root, &["base_url"]),
            string_path(root, &["base-url"]),
            string_path(root, &["metadata", "base_url"]),
            string_path(root, &["metadata", "base-url"]),
            string_path(root, &["attributes", "base_url"]),
            string_path(root, &["attributes", "base-url"]),
            Some(endpoints.api_base_url.clone()),
        ])
        .unwrap_or_else(|| endpoints.api_base_url.clone()),
        proxy_url: trimmed(first_non_empty([
            string_path(root, &["proxy_url"]),
            string_path(root, &["proxy-url"]),
            string_path(root, &["metadata", "proxy_url"]),
            string_path(root, &["metadata", "proxy-url"]),
            string_path(root, &["attributes", "proxy_url"]),
            string_path(root, &["attributes", "proxy-url"]),
        ])),
        expired_at: first_non_empty([
            string_path(root, &["expired"]),
            string_path(root, &["metadata", "expired"]),
            string_path(root, &["attributes", "expired"]),
        ])
        .and_then(|value| parse_datetime(&value)),
    }))
}

fn prepare_request_body(body: &[u8], model: &str) -> PreparedClaudeRequest {
    let Some(mut value) = serde_json::from_slice::<Value>(body).ok() else {
        return PreparedClaudeRequest {
            body: body.to_vec(),
            extra_betas: Vec::new(),
        };
    };
    let Some(object) = value.as_object_mut() else {
        return PreparedClaudeRequest {
            body: body.to_vec(),
            extra_betas: Vec::new(),
        };
    };

    object.insert("model".to_string(), Value::String(model.trim().to_string()));
    let extra_betas = extract_body_betas(object);
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec());
    PreparedClaudeRequest { body, extra_betas }
}

fn extract_body_betas(object: &mut Map<String, Value>) -> Vec<String> {
    let Some(value) = object.remove("betas") else {
        return Vec::new();
    };

    let mut betas = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(beta) = item
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    betas.push(beta.to_string());
                }
            }
        }
        Value::String(item) => {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                betas.push(trimmed.to_string());
            }
        }
        _ => {}
    }

    betas
}

fn merged_betas(extra_betas: &[String]) -> String {
    let mut values = BTreeSet::new();
    for beta in DEFAULT_ANTHROPIC_BETA.split(',') {
        let trimmed = beta.trim();
        if !trimmed.is_empty() {
            values.insert(trimmed.to_string());
        }
    }
    for beta in extra_betas {
        let trimmed = beta.trim();
        if !trimmed.is_empty() {
            values.insert(trimmed.to_string());
        }
    }
    values.into_iter().collect::<Vec<_>>().join(",")
}

fn request_path(kind: ClaudeRequestKind) -> &'static str {
    match kind {
        ClaudeRequestKind::Messages => CLAUDE_MESSAGES_PATH,
        ClaudeRequestKind::CountTokens => CLAUDE_COUNT_TOKENS_PATH,
    }
}

fn should_use_api_key_header(base_url: &str) -> bool {
    Url::parse(base_url).ok().is_some_and(|url| {
        url.scheme().eq_ignore_ascii_case("https")
            && url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("api.anthropic.com"))
    })
}

fn resolved_credential(auth: &ClaudeAuthState) -> Option<(&str, bool)> {
    auth.api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| (value, true))
        .or_else(|| {
            auth.access_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| (value, false))
        })
}

fn parse_refresh_token_response(
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<RefreshedClaudeToken, ClaudeCallerError> {
    let parsed: RefreshTokenResponse = serde_json::from_slice(body)
        .map_err(|error| ClaudeCallerError::InvalidAuthFile(error.to_string()))?;
    let access_token = parsed.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err(ClaudeCallerError::MissingAccessToken);
    }

    Ok(RefreshedClaudeToken {
        access_token,
        refresh_token: trimmed(Some(parsed.refresh_token)),
        token_type: trimmed(Some(parsed.token_type)),
        email: trimmed(Some(parsed.account.email_address)),
        expired_at: (parsed.expires_in > 0).then(|| now + Duration::seconds(parsed.expires_in)),
    })
}

fn apply_refreshed_auth_state(
    auth: &mut ClaudeAuthState,
    refreshed: &RefreshedClaudeToken,
    now: DateTime<Utc>,
) {
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
    upsert_string(&mut auth.root, "provider", Some("claude"));
    upsert_string(&mut auth.root, "type", Some("claude"));
    upsert_string(
        &mut auth.root,
        "token_type",
        refreshed.token_type.as_deref(),
    );
    upsert_string(&mut auth.root, "email", refreshed.email.as_deref());
    upsert_string(&mut auth.root, "last_refresh", Some(&now.to_rfc3339()));
    upsert_string(
        &mut auth.root,
        "expired",
        refreshed
            .expired_at
            .map(|value| value.to_rfc3339())
            .as_deref(),
    );
}

fn persist_claude_auth_state(
    auth_dir: &Path,
    auth_file_name: &str,
    auth: &ClaudeAuthState,
) -> Result<(), ClaudeCallerError> {
    let bytes = serde_json::to_vec_pretty(&Value::Object(auth.root.clone()))
        .map_err(|error| ClaudeCallerError::InvalidAuthFile(error.to_string()))?;
    write_auth_file(auth_dir, auth_file_name, &bytes).map_err(ClaudeCallerError::WriteAuthFile)?;
    Ok(())
}

fn build_http_client(proxy_url: Option<&str>) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy_url.and_then(|value| trimmed(Some(value.to_string()))) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

fn refresh_due(auth: &ClaudeAuthState, now: DateTime<Utc>) -> bool {
    if auth
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return false;
    }
    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return auth.refresh_token.is_some();
    }

    auth.expired_at
        .map(|expired_at| expired_at <= now + Duration::seconds(CLAUDE_REFRESH_LEAD_SECS))
        .unwrap_or(false)
}

fn claude_auth_failure(error: ClaudeCallerError) -> ExecutionFailure<ClaudeCallerError> {
    ExecutionFailure::retryable(
        error,
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message: "unauthorized".to_string(),
                http_status: Some(401),
            }),
        },
    )
}

fn claude_local_failure(error: ClaudeCallerError) -> ExecutionFailure<ClaudeCallerError> {
    ExecutionFailure::retryable(
        error,
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message: "unauthorized".to_string(),
                http_status: Some(401),
            }),
        },
    )
}

fn claude_request_failure(error: reqwest::Error) -> ExecutionFailure<ClaudeCallerError> {
    let message = error.to_string();
    ExecutionFailure::retryable(
        ClaudeCallerError::Request(error),
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message,
                http_status: Some(503),
            }),
        },
    )
}

fn claude_status_failure(
    status: u16,
    headers: &HeaderMap,
    body: &[u8],
    model: &str,
) -> ExecutionFailure<ClaudeCallerError> {
    let message = response_body_message(body);
    let error = ClaudeCallerError::UnexpectedStatus {
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

fn response_body_message(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(message) = first_non_empty([
            value_string_path(&value, &["error", "message"]),
            value_string_path(&value, &["message"]),
            value_string_path(&value, &["error_description"]),
            value_string_path(&value, &["error"]),
        ]) {
            return message;
        }
    }

    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        "request failed".to_string()
    } else {
        trimmed
    }
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn string_path(root: &Map<String, Value>, path: &[&str]) -> Option<String> {
    let mut current = Value::Object(root.clone());
    for key in path {
        current = current.get(*key)?.clone();
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_string_path(root: &Value, path: &[&str]) -> Option<String> {
    let mut current = root;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn first_non_empty<const N: usize>(values: [Option<String>; N]) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn upsert_string(root: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        root.remove(key);
        return;
    };
    root.insert(key.to_string(), Value::String(value.to_string()));
}

fn normalized_model(model: &str) -> Option<String> {
    let trimmed = model.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl From<ClaudeMessagesRequest> for RawClaudeRequest {
    fn from(value: ClaudeMessagesRequest) -> Self {
        Self {
            model: value.model.trim().to_string(),
            body: value.body,
        }
    }
}
