use crate::BackendBootstrap;
use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, Multipart, Query, State};
use axum::http::header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE, ORIGIN, VARY};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::{Json, Router};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use nicecli_auth::{
    read_auth_file as read_auth_file_from_store, write_oauth_callback_file_for_pending_session,
    AnthropicLoginService, AntigravityLoginService, AuthFileStoreError, CodexLoginService,
    GeminiCliLoginService, KimiLoginService, OAuthFlowError, OAuthSessionStore, QwenLoginService,
};
use nicecli_config::{ConfigError, NiceCliConfig};
use nicecli_models::{
    refresh_global_model_catalog_from_remote, static_model_definitions_by_channel, ModelInfo,
};
use nicecli_quota::CodexQuotaService;
use nicecli_runtime::{
    AuthSnapshot, AuthStore, AuthStoreError, FileAuthStore, ProviderHttpResponse, RoutingStrategy,
    RuntimeConductor, RuntimeConductorError, SchedulerError,
};
use reqwest::header::{
    HeaderMap as ReqwestHeaderMap, ACCEPT as REQWEST_ACCEPT,
    AUTHORIZATION as REQWEST_AUTHORIZATION, CONTENT_TYPE as REQWEST_CONTENT_TYPE,
    USER_AGENT as REQWEST_USER_AGENT,
};
use reqwest::{Client, Proxy};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::time::Duration;
use thiserror::Error;
use tokio::net::TcpListener;

mod antigravity_provider;
mod claude_provider;
mod codex_provider;
mod entry_routes;
mod google_provider;
mod kimi_provider;
mod management_account;
mod management_auth;
mod management_config;
mod management_logs;
mod management_provider_config;
mod model_catalog;
mod openai_chat_provider;
mod openai_compat_provider;
mod public_api_routes;
mod public_model_routes;
mod qwen_provider;

use self::antigravity_provider::*;
use self::claude_provider::*;
use self::codex_provider::*;
use self::entry_routes::route_entry_routes;
use self::google_provider::*;
use self::kimi_provider::*;
use self::management_account::route_management_account_routes;
use self::management_auth::*;
use self::management_config::*;
use self::management_logs::*;
use self::management_provider_config::*;
use self::model_catalog::*;
use self::openai_chat_provider::*;
use self::openai_compat_provider::*;
use self::public_api_routes::route_public_api_routes;
use self::public_model_routes::route_public_model_routes;
use self::qwen_provider::*;

#[derive(Debug, Error)]
pub enum BackendServerError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("auth-dir is missing from config and could not be inferred")]
    MissingAuthDir,
    #[error("failed to bind or serve backend: {0}")]
    Serve(std::io::Error),
}

#[derive(Debug, Clone)]
pub struct BackendAppState {
    pub bootstrap: BackendBootstrap,
    pub config: NiceCliConfig,
    pub auth_dir: PathBuf,
    pub quota_service: Arc<CodexQuotaService>,
    pub antigravity_login_service: Arc<AntigravityLoginService>,
    pub anthropic_login_service: Arc<AnthropicLoginService>,
    pub codex_login_service: Arc<CodexLoginService>,
    pub gemini_cli_login_service: Arc<GeminiCliLoginService>,
    pub kimi_login_service: Arc<KimiLoginService>,
    pub qwen_login_service: Arc<QwenLoginService>,
    pub oauth_sessions: Arc<OAuthSessionStore>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PublicApiAuthQuery {
    key: Option<String>,
    auth_token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigStringValueRequest {
    value: Option<String>,
    #[serde(rename = "proxy-url")]
    proxy_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigBoolValueRequest {
    value: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigIntValueRequest {
    value: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StringListItemsRequest {
    items: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PatchStringListRequest {
    old: Option<String>,
    new: Option<String>,
    index: Option<usize>,
    value: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DeleteStringListQuery {
    index: Option<usize>,
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemsWrapper<T> {
    items: T,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OAuthExcludedModelsPatchRequest {
    provider: Option<String>,
    models: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct OAuthModelAliasEntry {
    name: String,
    alias: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    fork: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OAuthModelAliasPatchRequest {
    provider: Option<String>,
    channel: Option<String>,
    aliases: Vec<OAuthModelAliasEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProviderQuery {
    provider: Option<String>,
    channel: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApiKeyIndexQuery {
    index: Option<usize>,
    #[serde(rename = "api-key")]
    api_key: Option<String>,
}

const OAUTH_CALLBACK_SUCCESS_HTML: &str = "<html><head><meta charset=\"utf-8\"><title>Authentication successful</title><script>setTimeout(function(){window.close();},5000);</script></head><body><h1>Authentication successful!</h1><p>You can close this window.</p><p>This window will close automatically in 5 seconds.</p></body></html>";
const MODEL_CATALOG_REFRESH_INTERVAL: Duration = Duration::from_secs(3 * 60 * 60);

static MODEL_CATALOG_REFRESH_TASK: Once = Once::new();

pub fn load_state_from_bootstrap(
    bootstrap: BackendBootstrap,
) -> Result<BackendAppState, BackendServerError> {
    let config = bootstrap.load_config()?;
    let auth_dir = resolve_auth_dir(bootstrap.config_path(), &config)?;
    let quota_service = Arc::new(CodexQuotaService::new(
        auth_dir.clone(),
        config.proxy_url.clone(),
    ));
    let oauth_sessions = Arc::new(OAuthSessionStore::default());
    let antigravity_login_service = Arc::new(AntigravityLoginService::new(
        oauth_sessions.clone(),
        config.proxy_url.clone(),
    ));
    let anthropic_login_service = Arc::new(AnthropicLoginService::new(
        oauth_sessions.clone(),
        config.proxy_url.clone(),
    ));
    let codex_login_service = Arc::new(CodexLoginService::new(
        oauth_sessions.clone(),
        config.proxy_url.clone(),
    ));
    let gemini_cli_login_service = Arc::new(GeminiCliLoginService::new(
        oauth_sessions.clone(),
        config.proxy_url.clone(),
    ));
    let kimi_login_service = Arc::new(KimiLoginService::new(
        oauth_sessions.clone(),
        config.proxy_url.clone(),
    ));
    let qwen_login_service = Arc::new(QwenLoginService::new(
        oauth_sessions.clone(),
        config.proxy_url.clone(),
    ));
    Ok(BackendAppState {
        bootstrap,
        config,
        auth_dir,
        quota_service,
        antigravity_login_service,
        anthropic_login_service,
        codex_login_service,
        gemini_cli_login_service,
        kimi_login_service,
        qwen_login_service,
        oauth_sessions,
    })
}

pub fn build_router(state: BackendAppState) -> Router {
    route_management_account_routes(route_management_provider_routes(
        route_management_auth_routes(route_management_logs_routes(
            route_management_config_routes(route_public_api_routes(route_public_model_routes(
                route_entry_routes(Router::new()),
            ))),
        )),
    ))
    .layer(middleware::from_fn(desktop_cors_middleware))
    .with_state(Arc::new(state))
}

async fn desktop_cors_middleware(request: Request<Body>, next: Next) -> Response {
    let has_origin = request.headers().contains_key(ORIGIN);
    let request_headers = request
        .headers()
        .get("Access-Control-Request-Headers")
        .cloned();
    let allow_private_network = request
        .headers()
        .get("Access-Control-Request-Private-Network")
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"true"));
    let is_preflight = request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key("Access-Control-Request-Method");

    if has_origin && is_preflight {
        return desktop_cors_preflight_response(request_headers, allow_private_network);
    }

    let mut response = next.run(request).await;
    if has_origin {
        apply_desktop_cors_headers(response.headers_mut());
    }
    response
}

fn desktop_cors_preflight_response(
    request_headers: Option<HeaderValue>,
    allow_private_network: bool,
) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    apply_desktop_cors_headers(response.headers_mut());
    response.headers_mut().insert(
        HeaderName::from_static("access-control-allow-methods"),
        HeaderValue::from_static("GET, POST, PATCH, DELETE, OPTIONS"),
    );
    let allow_headers = request_headers.unwrap_or_else(|| {
        HeaderValue::from_static(
            "authorization, content-type, x-api-key, x-goog-api-key, x-management-key",
        )
    });
    response.headers_mut().insert(
        HeaderName::from_static("access-control-allow-headers"),
        allow_headers,
    );
    response.headers_mut().insert(
        HeaderName::from_static("access-control-max-age"),
        HeaderValue::from_static("600"),
    );
    if allow_private_network {
        response.headers_mut().insert(
            HeaderName::from_static("access-control-allow-private-network"),
            HeaderValue::from_static("true"),
        );
    }
    response.headers_mut().insert(
        VARY,
        HeaderValue::from_static(
            "Origin, Access-Control-Request-Method, Access-Control-Request-Headers",
        ),
    );
    response
}

fn apply_desktop_cors_headers(headers: &mut HeaderMap) {
    headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        HeaderValue::from_static("*"),
    );
    headers.insert(
        HeaderName::from_static("access-control-expose-headers"),
        HeaderValue::from_static("content-disposition"),
    );
    headers.insert(VARY, HeaderValue::from_static("Origin"));
}

pub fn start_model_catalog_refresh_task() {
    MODEL_CATALOG_REFRESH_TASK.call_once(|| {
        tokio::spawn(async {
            refresh_model_catalog_once("startup").await;
            loop {
                tokio::time::sleep(MODEL_CATALOG_REFRESH_INTERVAL).await;
                refresh_model_catalog_once("periodic").await;
            }
        });
    });
}

async fn refresh_model_catalog_once(label: &str) {
    match refresh_global_model_catalog_from_remote().await {
        Ok(Some(result)) if !result.changed_providers.is_empty() => {
            eprintln!(
                "[nicecli-models] {label} model catalog refresh updated {:?} from {}",
                result.changed_providers, result.source
            );
        }
        Ok(Some(_)) | Ok(None) => {}
        Err(error) => {
            eprintln!("[nicecli-models] {label} model catalog refresh failed: {error}");
        }
    }
}

pub async fn serve(
    bootstrap: BackendBootstrap,
    listener: TcpListener,
) -> Result<(), BackendServerError> {
    let state = load_state_from_bootstrap(bootstrap)?;
    start_model_catalog_refresh_task();
    serve_state_with_shutdown(state, listener, std::future::pending::<()>()).await
}

pub async fn serve_state_with_shutdown<F>(
    state: BackendAppState,
    listener: TcpListener,
    shutdown: F,
) -> Result<(), BackendServerError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let router = build_router(state);
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
    .map_err(BackendServerError::Serve)
}

fn auth_store_error_response(error: AuthFileStoreError) -> Response {
    let (status, message) = match error {
        AuthFileStoreError::InvalidName
        | AuthFileStoreError::InvalidExtension
        | AuthFileStoreError::InvalidAuthFile(_)
        | AuthFileStoreError::InvalidRoot
        | AuthFileStoreError::NoFieldsToUpdate => (StatusCode::BAD_REQUEST, error.to_string()),
        AuthFileStoreError::NotFound => (StatusCode::NOT_FOUND, error.to_string()),
        AuthFileStoreError::ReadDir(_)
        | AuthFileStoreError::ReadFile(_)
        | AuthFileStoreError::WriteFile(_)
        | AuthFileStoreError::RemoveFile(_)
        | AuthFileStoreError::Encode(_) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    json_error(status, message)
}

fn auth_snapshot_store_error_response(error: AuthStoreError) -> Response {
    match error {
        AuthStoreError::FileStore(inner) => auth_store_error_response(inner),
    }
}

fn resolve_auth_dir(
    config_path: &Path,
    config: &NiceCliConfig,
) -> Result<PathBuf, BackendServerError> {
    if let Some(auth_dir) = config
        .auth_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(normalize_auth_dir(config_path, auth_dir));
    }

    let parent = config_path
        .parent()
        .ok_or(BackendServerError::MissingAuthDir)?;
    Ok(parent.join("auth"))
}

fn normalize_auth_dir(config_path: &Path, auth_dir: &str) -> PathBuf {
    let raw = auth_dir.trim();
    if let Some(stripped) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        if let Some(home_dir) = home::home_dir() {
            return home_dir.join(stripped);
        }
    }

    if raw == "~" {
        if let Some(home_dir) = home::home_dir() {
            return home_dir;
        }
    }

    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        return candidate;
    }

    config_path
        .parent()
        .map(|parent| parent.join(&candidate))
        .unwrap_or(candidate)
}

fn ensure_public_api_key(
    headers: &HeaderMap,
    query: &PublicApiAuthQuery,
    state: &BackendAppState,
) -> Result<(), Response> {
    let api_keys = load_public_api_keys(state).map_err(config_error_response)?;
    if api_keys.is_empty() {
        return Ok(());
    }

    let candidates = [
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(extract_public_api_key_from_authorization),
        extract_trimmed_header_value(headers, "X-Goog-Api-Key"),
        extract_trimmed_header_value(headers, "X-Api-Key"),
        trim_optional_string(query.key.as_deref()),
        trim_optional_string(query.auth_token.as_deref()),
    ];

    let mut saw_credential = false;
    for candidate in candidates.into_iter().flatten() {
        saw_credential = true;
        if api_keys.contains(&candidate) {
            return Ok(());
        }
    }

    if saw_credential {
        Err(json_error(StatusCode::UNAUTHORIZED, "Invalid API key"))
    } else {
        Err(json_error(StatusCode::UNAUTHORIZED, "Missing API key"))
    }
}

fn ensure_management_key(headers: &HeaderMap, state: &BackendAppState) -> Result<(), Response> {
    let expected = match state.bootstrap.local_management_password() {
        Some(password) if !password.trim().is_empty() => password.trim(),
        _ => {
            return Err(json_error(StatusCode::FORBIDDEN, "management key not set"));
        }
    };

    let Some(provided) = extract_management_key(headers) else {
        return Err(json_error(
            StatusCode::UNAUTHORIZED,
            "missing management key",
        ));
    };

    if provided == expected {
        Ok(())
    } else {
        Err(json_error(
            StatusCode::UNAUTHORIZED,
            "invalid management key",
        ))
    }
}

fn load_current_config(state: &BackendAppState) -> Result<NiceCliConfig, ConfigError> {
    state.bootstrap.load_config()
}

fn load_current_config_json(state: &BackendAppState) -> Result<JsonValue, ConfigError> {
    nicecli_config::load_config_json(state.bootstrap.config_path())
}

fn load_public_api_keys(state: &BackendAppState) -> Result<HashSet<String>, ConfigError> {
    Ok(get_config_string_list_value(state, "api-keys")?
        .into_iter()
        .filter_map(|value| trim_optional_string(Some(value.as_str())))
        .collect())
}

fn extract_trimmed_header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| trim_optional_string(Some(value)))
}

fn extract_public_api_key_from_authorization(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let token = match trimmed.split_once(' ') {
        Some((scheme, token)) if scheme.eq_ignore_ascii_case("bearer") => token.trim(),
        _ => trimmed,
    };

    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn trim_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn strip_json_object_field(
    raw_body: &[u8],
    parsed_json: Option<&JsonValue>,
    field: &str,
) -> Vec<u8> {
    let Some(JsonValue::Object(object)) = parsed_json else {
        return raw_body.to_vec();
    };
    if !object.contains_key(field) {
        return raw_body.to_vec();
    }

    let mut next = object.clone();
    next.remove(field);
    serde_json::to_vec(&JsonValue::Object(next)).unwrap_or_else(|_| raw_body.to_vec())
}

fn provider_http_response(response: ProviderHttpResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    if let Some(headers) = builder.headers_mut() {
        for (name, value) in &response.headers {
            if name.as_str().eq_ignore_ascii_case("content-length")
                || name.as_str().eq_ignore_ascii_case("transfer-encoding")
                || name.as_str().eq_ignore_ascii_case("connection")
            {
                continue;
            }
            headers.insert(name.clone(), value.clone());
        }
        if !headers.contains_key(CONTENT_TYPE) {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
    }

    match builder.body(Body::from(response.body)) {
        Ok(response) => response,
        Err(_) => openai_error_response(StatusCode::BAD_GATEWAY, "Failed to build response"),
    }
}

fn reqwest_stream_response(response: reqwest::Response) -> Response {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = response.headers().clone();
    let mut builder = Response::builder().status(status);
    if let Some(next_headers) = builder.headers_mut() {
        for (name, value) in &headers {
            if name.as_str().eq_ignore_ascii_case("content-length")
                || name.as_str().eq_ignore_ascii_case("transfer-encoding")
                || name.as_str().eq_ignore_ascii_case("connection")
            {
                continue;
            }
            next_headers.insert(name.clone(), value.clone());
        }
        if !next_headers.contains_key(CONTENT_TYPE) {
            next_headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
        }
    }

    builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap_or_else(|_| {
            openai_error_response(StatusCode::BAD_GATEWAY, "Failed to build response")
        })
}

fn openai_error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    let trimmed = message.trim();
    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) {
            return (status, Json(value)).into_response();
        }
    }

    let (error_type, code) = match status {
        StatusCode::UNAUTHORIZED => ("authentication_error", Some("invalid_api_key")),
        StatusCode::FORBIDDEN => ("permission_error", Some("insufficient_quota")),
        StatusCode::TOO_MANY_REQUESTS => ("rate_limit_error", Some("rate_limit_exceeded")),
        StatusCode::NOT_FOUND => ("invalid_request_error", Some("model_not_found")),
        _ if status.is_server_error() => ("server_error", Some("internal_server_error")),
        _ => ("invalid_request_error", None),
    };

    let mut error = serde_json::Map::new();
    error.insert(
        "message".to_string(),
        json!(if trimmed.is_empty() {
            status.canonical_reason().unwrap_or("error")
        } else {
            trimmed
        }),
    );
    error.insert("type".to_string(), json!(error_type));
    if let Some(code) = code {
        error.insert("code".to_string(), json!(code));
    }

    (status, Json(json!({ "error": error }))).into_response()
}

fn get_bool_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    config_path: &str,
    response_key: &str,
) -> Response {
    get_bool_config_field_response_with_default(state, headers, config_path, response_key, false)
}

fn get_bool_config_field_response_with_default(
    state: &BackendAppState,
    headers: &HeaderMap,
    config_path: &str,
    response_key: &str,
    default_value: bool,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    match load_current_config_json(state) {
        Ok(config) => {
            let value = config_json_value(&config, config_path)
                .and_then(JsonValue::as_bool)
                .unwrap_or(default_value);
            single_field_json_response(response_key, json!(value))
        }
        Err(error) => config_error_response(error),
    }
}

fn put_bool_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    request: ConfigBoolValueRequest,
    config_path: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &json!(value),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn get_int_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    config_path: &str,
    response_key: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let value = get_config_i64_value(state, config_path).unwrap_or_default();
    single_field_json_response(response_key, json!(value))
}

fn get_string_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    config_path: &str,
    response_key: &str,
    default_value: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let value =
        get_config_string_value(state, config_path).unwrap_or_else(|_| default_value.to_string());
    single_field_json_response(response_key, json!(value))
}

fn put_int_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    request: ConfigIntValueRequest,
    config_path: &str,
    normalize: fn(i64) -> i64,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };
    let normalized = normalize(value);

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &json!(normalized),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn get_config_bool_value(state: &BackendAppState, config_path: &str) -> Result<bool, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(config_json_value(&config, config_path)
        .and_then(JsonValue::as_bool)
        .unwrap_or(false))
}

fn get_config_i64_value(state: &BackendAppState, config_path: &str) -> Result<i64, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(config_json_value(&config, config_path)
        .and_then(json_value_to_i64)
        .unwrap_or_default())
}

fn get_config_string_value(
    state: &BackendAppState,
    config_path: &str,
) -> Result<String, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(config_json_value(&config, config_path)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string())
}

fn get_config_string_list_value(
    state: &BackendAppState,
    config_path: &str,
) -> Result<Vec<String>, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(config_json_value(&config, config_path)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

fn parse_json_or_items_wrapper<T>(body: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    serde_json::from_str::<T>(body)
        .or_else(|_| serde_json::from_str::<ItemsWrapper<T>>(body).map(|wrapper| wrapper.items))
}

fn persist_top_level_config_value(
    state: &BackendAppState,
    config_path: &str,
    value: Option<JsonValue>,
) -> Result<Response, ConfigError> {
    match value {
        Some(value) => nicecli_config::update_config_value(
            state.bootstrap.config_path(),
            config_path,
            &value,
            false,
        )
        .map(|()| Json(json!({ "status": "ok" })).into_response()),
        None => nicecli_config::update_config_value(
            state.bootstrap.config_path(),
            config_path,
            &JsonValue::Null,
            true,
        )
        .map(|()| Json(json!({ "status": "ok" })).into_response()),
    }
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

fn normalize_headers(headers: BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    let mut normalized = BTreeMap::new();
    for (key, value) in headers {
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        normalized.insert(key.to_string(), value.to_string());
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn put_string_list_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    body: String,
    config_path: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let parsed = match serde_json::from_str::<Vec<String>>(&body) {
        Ok(items) => items,
        Err(_) => match serde_json::from_str::<StringListItemsRequest>(&body) {
            Ok(request) if !request.items.is_empty() => request.items,
            _ => {
                return json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid body",
                    "invalid body",
                );
            }
        },
    };

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &json!(parsed),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn put_string_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    request: ConfigStringValueRequest,
    config_path: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let value = request.value.or(request.proxy_url).unwrap_or_default();
    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &json!(value),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn delete_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    config_path: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &JsonValue::Null,
        true,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn patch_string_list_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    request: PatchStringListRequest,
    config_path: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let mut values = match get_config_string_list_value(state, config_path) {
        Ok(values) => values,
        Err(error) => return config_error_response(error),
    };

    if let (Some(index), Some(value)) = (request.index, request.value) {
        if index >= values.len() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing fields",
                "missing fields",
            );
        }
        values[index] = value;
    } else if let Some(new_value) = request.new {
        if let Some(old_value) = request.old {
            if let Some(index) = values.iter().position(|item| item == &old_value) {
                values[index] = new_value;
            } else {
                values.push(new_value);
            }
        } else {
            values.push(new_value);
        }
    } else {
        return json_error_response(StatusCode::BAD_REQUEST, "missing fields", "missing fields");
    }

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &json!(values),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn delete_string_list_config_field_response(
    state: &BackendAppState,
    headers: &HeaderMap,
    query: DeleteStringListQuery,
    config_path: &str,
) -> Response {
    if let Err(response) = ensure_management_key(headers, state) {
        return response;
    }

    let mut values = match get_config_string_list_value(state, config_path) {
        Ok(values) => values,
        Err(error) => return config_error_response(error),
    };

    if let Some(index) = query.index {
        if index >= values.len() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing index or value",
                "missing index or value",
            );
        }
        values.remove(index);
    } else if let Some(value) = query.value.map(|value| value.trim().to_string()) {
        if value.is_empty() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing index or value",
                "missing index or value",
            );
        }
        values.retain(|item| item.trim() != value);
    } else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing index or value",
            "missing index or value",
        );
    }

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        config_path,
        &json!(values),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

fn config_json_value<'a>(value: &'a JsonValue, config_path: &str) -> Option<&'a JsonValue> {
    let mut current = value;
    for segment in config_path
        .split(['.', '/'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        current = current.get(segment)?;
    }
    Some(current)
}

fn json_value_to_i64(value: &JsonValue) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
}

fn single_field_json_response(key: &str, value: JsonValue) -> Response {
    let mut object = serde_json::Map::new();
    object.insert(key.to_string(), value);
    Json(JsonValue::Object(object)).into_response()
}

fn config_error_response(error: ConfigError) -> Response {
    match error {
        ConfigError::Missing { .. } => {
            json_error_response(StatusCode::NOT_FOUND, "not_found", "config file not found")
        }
        other => json_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_failed",
            &other.to_string(),
        ),
    }
}

fn json_error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": code,
            "message": message
        })),
    )
        .into_response()
}

fn attachment_response(filename: &str, body: Vec<u8>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    (headers, body).into_response()
}

fn extract_management_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        let trimmed = value.trim();
        if let Some((scheme, token)) = trimmed.split_once(' ') {
            if scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() {
                return Some(token.trim().to_string());
            }
        }
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    headers
        .get("X-Management-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_management_bool(raw: Option<&str>) -> bool {
    matches!(
        raw.unwrap_or_default().trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

async fn parse_optional_json_body<T>(request: Request<Body>) -> Result<Option<T>, Response>
where
    T: DeserializeOwned,
{
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|_| json_error(StatusCode::BAD_REQUEST, "invalid body"))?;
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }

    serde_json::from_slice::<T>(&body)
        .map(Some)
        .map_err(|_| json_error(StatusCode::BAD_REQUEST, "invalid body"))
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn oauth_status_error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "status": "error", "error": message.into() })),
    )
        .into_response()
}

fn oauth_callback_file_page_response(error: OAuthFlowError) -> Response {
    let (status, message) = match error {
        OAuthFlowError::SessionNotPending => (
            StatusCode::CONFLICT,
            "OAuth flow is not pending".to_string(),
        ),
        OAuthFlowError::InvalidState(_) | OAuthFlowError::MissingState => {
            (StatusCode::BAD_REQUEST, "Invalid state".to_string())
        }
        other => (StatusCode::BAD_REQUEST, other.to_string()),
    };
    let html = format!(
        "<html><head><meta charset=\"utf-8\"><title>Authentication failed</title></head><body><h1>Authentication failed</h1><p>{message}</p></body></html>"
    );
    (status, Html(html)).into_response()
}

#[cfg(test)]
mod tests {
    use super::{
        build_router, handle_v1internal_method, load_state_from_bootstrap, normalize_auth_dir,
        parse_gemini_public_action, serve_state_with_shutdown, BackendAppState,
    };
    use crate::BackendBootstrap;
    use axum::body::{to_bytes, Body, Bytes};
    use axum::extract::ConnectInfo;
    use axum::http::{HeaderMap, Request, StatusCode};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use nicecli_auth::{
        AnthropicLoginEndpoints, AnthropicLoginService, AntigravityLoginEndpoints,
        AntigravityLoginService, AuthFileEntry, CodexLoginEndpoints, CodexLoginService,
        GeminiCliLoginEndpoints, GeminiCliLoginService, KimiLoginEndpoints, KimiLoginService,
        QwenLoginEndpoints, QwenLoginService,
    };
    use nicecli_quota::SnapshotListResponse;
    use nicecli_quota::PROVIDER_CODEX;
    use serde_json::{json, Value};
    use std::fs;
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::time::sleep;
    use tower::ServiceExt;

    fn create_fixture_config(temp_dir: &TempDir) -> std::path::PathBuf {
        let auth_dir = temp_dir.path().join("auth");
        fs::create_dir_all(&auth_dir).expect("auth dir");
        let config_path = temp_dir.path().join("config.yaml");
        fs::write(
            &config_path,
            format!(
                "host: 127.0.0.1\nport: 8317\nauth-dir: {}\n",
                auth_dir.to_string_lossy().replace('\\', "/")
            ),
        )
        .expect("config file");
        config_path
    }

    fn load_fixture_state(temp_dir: &TempDir) -> BackendAppState {
        let config_path = create_fixture_config(temp_dir);
        let bootstrap = BackendBootstrap::new(config_path).with_local_management_password("secret");
        load_state_from_bootstrap(bootstrap).expect("state should load")
    }

    #[test]
    fn normalize_auth_dir_expands_home_tilde_path() {
        let config_path = PathBuf::from("C:/demo/config.yaml");
        let expected = home::home_dir().expect("home dir").join(".cli-proxy-api");

        assert_eq!(
            normalize_auth_dir(&config_path, "~/.cli-proxy-api"),
            expected
        );
    }

    #[test]
    fn normalize_auth_dir_resolves_relative_path_from_config_directory() {
        let config_path = PathBuf::from("C:/demo/config/config.yaml");

        assert_eq!(
            normalize_auth_dir(&config_path, "auth"),
            PathBuf::from("C:/demo/config/auth")
        );
    }

    async fn response_json(response: axum::response::Response) -> Value {
        serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body"),
        )
        .expect("json body")
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedPublicRequest {
        authorization: Option<String>,
        body: Vec<u8>,
    }

    // Keep auth fixtures far in the future so runtime tests do not expire over time.
    fn write_qwen_auth_file(
        state: &BackendAppState,
        file_name: &str,
        access_token: &str,
        resource_url: &str,
        prefix: Option<&str>,
    ) {
        let prefix_field = prefix
            .map(|value| format!(",\n  \"prefix\": \"{}\"", value))
            .unwrap_or_default();
        fs::write(
            state.auth_dir.join(file_name),
            format!(
                r#"{{
  "type": "qwen",
  "provider": "qwen",
  "email": "demo@example.com",
  "access_token": "{access_token}",
  "refresh_token": "refresh-token",
  "resource_url": "{resource_url}",
  "expired": "2099-01-01T00:00:00Z"{prefix_field}
}}"#
            ),
        )
        .expect("write auth");
    }

    fn write_kimi_auth_file(
        state: &BackendAppState,
        file_name: &str,
        access_token: &str,
        base_url: &str,
    ) {
        fs::write(
            state.auth_dir.join(file_name),
            format!(
                r#"{{
  "type": "kimi",
  "provider": "kimi",
  "email": "demo@example.com",
  "access_token": "{access_token}",
  "refresh_token": "refresh-token",
  "base_url": "{base_url}",
  "expired": "2099-01-01T00:00:00Z"
}}"#
            ),
        )
        .expect("write auth");
    }

    fn write_claude_auth_file(
        state: &BackendAppState,
        file_name: &str,
        access_token: &str,
        base_url: &str,
        prefix: Option<&str>,
        model_name: &str,
    ) {
        let prefix_field = prefix
            .map(|value| format!(",\n  \"prefix\": \"{}\"", value))
            .unwrap_or_default();
        fs::write(
            state.auth_dir.join(file_name),
            format!(
                r#"{{
  "type": "claude",
  "provider": "claude",
  "email": "demo@example.com",
  "access_token": "{access_token}",
  "refresh_token": "claude-refresh-token",
  "base_url": "{base_url}",
  "expired": "2099-01-01T00:00:00Z",
  "models": [{{"name": "{model_name}"}}]{prefix_field}
}}"#
            ),
        )
        .expect("write auth");
    }
    mod management_config_sample_routes;
    mod management_cors_routes;
    mod management_import_quota_routes;
    mod management_login_routes;
    mod management_oauth_routes;
    mod management_provider_api_key_routes;
    mod management_provider_config_routes;
    mod management_routes;
    mod management_system_routes;
    mod public_codex_responses_routes;
    mod public_gemini_routes;
    mod public_model_catalog_routes;
    mod public_oauth_provider_routes;
}
