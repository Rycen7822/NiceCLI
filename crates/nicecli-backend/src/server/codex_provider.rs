use super::*;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use futures_util::StreamExt;
use nicecli_auth::CodexLoginError;
use nicecli_quota::{ListOptions, RefreshOptions, SnapshotListResponse};
use nicecli_runtime::{
    CodexCallerError, CodexCompactCaller, CodexCompactRequest, CodexResponsesCaller,
    CodexResponsesRequest, ExecuteWithRetryError, ExecuteWithRetryOptions, RuntimeConductorError,
    SchedulerError,
};

mod websocket;

use self::websocket::*;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct CodexQuotaSnapshotQuery {
    refresh: Option<String>,
    auth_id: Option<String>,
    workspace_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct CodexQuotaRefreshRequest {
    auth_id: String,
    workspace_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct CodexApiKeyModelEntry {
    name: String,
    alias: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct CodexApiKeyEntry {
    #[serde(rename = "api-key")]
    api_key: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    priority: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    prefix: String,
    #[serde(rename = "base-url")]
    base_url: String,
    websockets: bool,
    #[serde(rename = "proxy-url")]
    proxy_url: String,
    models: Vec<CodexApiKeyModelEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<BTreeMap<String, String>>,
    #[serde(rename = "excluded-models", skip_serializing_if = "Vec::is_empty")]
    excluded_models: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct CodexApiKeyPatchValue {
    #[serde(rename = "api-key")]
    api_key: Option<String>,
    prefix: Option<String>,
    #[serde(rename = "base-url")]
    base_url: Option<String>,
    #[serde(rename = "proxy-url")]
    proxy_url: Option<String>,
    models: Option<Vec<CodexApiKeyModelEntry>>,
    headers: Option<BTreeMap<String, String>>,
    #[serde(rename = "excluded-models")]
    excluded_models: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct CodexApiKeyPatchRequest {
    index: Option<usize>,
    #[serde(rename = "match")]
    match_value: Option<String>,
    value: Option<CodexApiKeyPatchValue>,
}

pub(super) async fn execute_public_codex_request(
    state: Arc<BackendAppState>,
    headers: HeaderMap,
    query: PublicApiAuthQuery,
    request: Request<Body>,
    compact: bool,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let config = match load_current_config(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let raw_body = match to_bytes(request.into_body(), usize::MAX).await {
        Ok(body) => body.to_vec(),
        Err(_) => {
            return openai_error_response(StatusCode::BAD_REQUEST, "Invalid request");
        }
    };

    let parsed_json = serde_json::from_slice::<JsonValue>(&raw_body).ok();
    let stream_requested = parsed_json
        .as_ref()
        .and_then(|value| value.get("stream"))
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    if stream_requested && compact {
        let message = "Streaming not supported for compact responses";
        return openai_error_response(StatusCode::BAD_REQUEST, message);
    }

    let upstream_body = if compact {
        strip_json_object_field(&raw_body, parsed_json.as_ref(), "stream")
    } else {
        raw_body.clone()
    };
    let model = parsed_json
        .as_ref()
        .and_then(|value| value.get("model"))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
    let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
    let user_agent = extract_trimmed_header_value(&headers, "User-Agent").unwrap_or_default();
    let options = nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now());

    if stream_requested {
        let mut caller = CodexResponsesCaller::new(&state.auth_dir, strategy)
            .with_default_proxy_url(default_proxy_url)
            .with_user_agent(user_agent);
        return match caller
            .execute_stream(
                CodexResponsesRequest {
                    model,
                    body: upstream_body,
                },
                options,
            )
            .await
            .map(|executed| executed.value)
        {
            Ok(response) => reqwest_stream_response(response),
            Err(error) => public_responses_error_response(error),
        };
    }

    let execution = if compact {
        let mut caller = CodexCompactCaller::new(&state.auth_dir, strategy)
            .with_default_proxy_url(default_proxy_url.clone())
            .with_user_agent(user_agent.clone());
        caller
            .execute(
                CodexCompactRequest {
                    model,
                    body: upstream_body,
                },
                options,
            )
            .await
            .map(|executed| executed.value)
    } else {
        let mut caller = CodexResponsesCaller::new(&state.auth_dir, strategy)
            .with_default_proxy_url(default_proxy_url.clone())
            .with_user_agent(user_agent.clone());
        caller
            .execute(
                CodexResponsesRequest {
                    model,
                    body: upstream_body,
                },
                options,
            )
            .await
            .map(|executed| executed.value)
    };

    match execution {
        Ok(response) => provider_http_response(response),
        Err(error) => public_responses_error_response(error),
    }
}

pub(super) async fn get_public_codex_responses_websocket(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    websocket: WebSocketUpgrade,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let user_agent = extract_trimmed_header_value(&headers, "User-Agent").unwrap_or_default();
    let turn_state = headers
        .get("x-codex-turn-state")
        .cloned()
        .filter(|value| !value.as_bytes().is_empty());

    let mut response = websocket
        .on_upgrade(move |socket| {
            handle_public_codex_responses_websocket(state, user_agent, socket)
        })
        .into_response();
    if let Some(turn_state) = turn_state {
        response
            .headers_mut()
            .insert("x-codex-turn-state", turn_state);
    }
    response
}

async fn handle_public_codex_responses_websocket(
    state: Arc<BackendAppState>,
    user_agent: String,
    mut socket: WebSocket,
) {
    let mut last_request: Option<Vec<u8>> = None;
    let mut last_response_output = b"[]".to_vec();
    let mut pinned_auth_id: Option<String> = None;
    let mut pinned_incremental_input = false;

    while let Some(message) = socket.next().await {
        let payload = match message {
            Ok(Message::Text(text)) => text.as_bytes().to_vec(),
            Ok(Message::Binary(bytes)) => bytes.to_vec(),
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(_) => break,
        };

        let normalized = match normalize_responses_websocket_request(
            &payload,
            last_request.as_deref(),
            &last_response_output,
            pinned_incremental_input,
        ) {
            Ok(normalized) => normalized,
            Err(error) => {
                if write_responses_websocket_error(
                    &mut socket,
                    error.status,
                    error.message.as_str(),
                    error.headers.as_ref(),
                )
                .await
                .is_err()
                {
                    break;
                }
                continue;
            }
        };

        if should_handle_responses_websocket_prewarm(
            &payload,
            last_request.is_some(),
            pinned_incremental_input,
        ) {
            last_request = Some(remove_generate_flag(&normalized.last_request));
            last_response_output = b"[]".to_vec();
            let payloads = synthetic_responses_websocket_prewarm_payloads(&normalized.request);
            let mut write_failed = false;
            for payload in payloads {
                if send_websocket_json(&mut socket, &payload).await.is_err() {
                    write_failed = true;
                    break;
                }
            }
            if write_failed {
                break;
            }
            continue;
        }

        last_request = Some(normalized.last_request);

        let config = match load_current_config(&state) {
            Ok(config) => config,
            Err(error) => {
                if write_responses_websocket_error(
                    &mut socket,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error.to_string().as_str(),
                    None,
                )
                .await
                .is_err()
                {
                    break;
                }
                continue;
            }
        };

        let parsed_request = match serde_json::from_slice::<JsonValue>(&normalized.request) {
            Ok(parsed) => parsed,
            Err(error) => {
                if write_responses_websocket_error(
                    &mut socket,
                    StatusCode::BAD_REQUEST,
                    format!("invalid websocket request body: {error}").as_str(),
                    None,
                )
                .await
                .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let model = parsed_request
            .get("model")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
        let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
        let mut options = ExecuteWithRetryOptions::new(chrono::Utc::now());
        options.pick.prefer_websocket = true;
        if let Some(pinned_auth_id) = pinned_auth_id.as_deref() {
            options.pick.pinned_auth_id = Some(pinned_auth_id.to_string());
        }

        let execution = CodexResponsesCaller::new(&state.auth_dir, strategy)
            .with_default_proxy_url(default_proxy_url)
            .with_user_agent(user_agent.clone())
            .execute_stream(
                CodexResponsesRequest {
                    model,
                    body: normalized.request.clone(),
                },
                options,
            )
            .await;

        match execution {
            Ok(executed) => {
                if executed
                    .selection
                    .snapshot
                    .candidate_state
                    .websocket_enabled
                {
                    pinned_auth_id = Some(executed.selection.auth_id);
                    pinned_incremental_input = true;
                }

                match forward_responses_websocket_stream(&mut socket, executed.value).await {
                    Ok(output) => {
                        last_response_output = output;
                    }
                    Err(_) => break,
                }
            }
            Err(error) => {
                let error = responses_websocket_error_from_codex_error(error);
                if write_responses_websocket_error(
                    &mut socket,
                    error.status,
                    error.message.as_str(),
                    error.headers.as_ref(),
                )
                .await
                .is_err()
                {
                    break;
                }
            }
        }
    }
}

pub(super) async fn get_codex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_codex_api_key_entries(&state) {
        Ok(entries) => single_field_json_response("codex-api-key", json!(entries)),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_codex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: Vec<CodexApiKeyEntry> = match parse_json_or_items_wrapper(&body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body")
        }
    };

    match persist_codex_api_key_entries(&state, normalize_codex_api_key_entries_for_write(parsed)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn patch_codex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<CodexApiKeyPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    let mut entries = match load_codex_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    let target_index = request
        .index
        .filter(|index| *index < entries.len())
        .or_else(|| {
            request
                .match_value
                .as_deref()
                .map(str::trim)
                .and_then(|needle| entries.iter().position(|entry| entry.api_key == needle))
        });
    let Some(target_index) = target_index else {
        return json_error_response(StatusCode::NOT_FOUND, "item not found", "item not found");
    };

    let mut entry = entries[target_index].clone();
    if let Some(api_key) = value.api_key {
        entry.api_key = api_key.trim().to_string();
    }
    if let Some(prefix) = value.prefix {
        entry.prefix = prefix.trim().to_string();
    }
    if let Some(base_url) = value.base_url {
        let trimmed = base_url.trim();
        if trimmed.is_empty() {
            entries.remove(target_index);
            return match persist_codex_api_key_entries(
                &state,
                sanitize_loaded_codex_api_key_entries(entries),
            ) {
                Ok(response) => response,
                Err(error) => config_error_response(error),
            };
        }
        entry.base_url = trimmed.to_string();
    }
    if let Some(proxy_url) = value.proxy_url {
        entry.proxy_url = proxy_url.trim().to_string();
    }
    if let Some(models) = value.models {
        entry.models = normalize_codex_api_key_models(models);
    }
    if let Some(headers) = value.headers {
        entry.headers = normalize_headers(headers);
    }
    if let Some(excluded_models) = value.excluded_models {
        entry.excluded_models = normalize_excluded_models(excluded_models);
    }

    entries[target_index] = normalize_codex_api_key_entry_for_write(entry);
    match persist_codex_api_key_entries(&state, sanitize_loaded_codex_api_key_entries(entries)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn delete_codex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<ApiKeyIndexQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut entries = match load_codex_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    if let Some(api_key) = query.api_key {
        if api_key.is_empty() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing api-key or index",
                "missing api-key or index",
            );
        }
        entries.retain(|entry| entry.api_key != api_key);
    } else if let Some(index) = query.index {
        if index >= entries.len() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing api-key or index",
                "missing api-key or index",
            );
        }
        entries.remove(index);
    } else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing api-key or index",
            "missing api-key or index",
        );
    }

    match persist_codex_api_key_entries(&state, sanitize_loaded_codex_api_key_entries(entries)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn handle_codex_callback(
    State(state): State<Arc<BackendAppState>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    let state_value = query
        .state
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let code = query
        .code
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let callback_error = query
        .error
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    match state
        .codex_login_service
        .complete_login(&state.auth_dir, &state_value, &code, &callback_error)
        .await
    {
        Ok(_) => Html(OAUTH_CALLBACK_SUCCESS_HTML).into_response(),
        Err(error) => oauth_callback_page_response(error),
    }
}

pub(super) async fn get_codex_quota_snapshots(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<CodexQuotaSnapshotQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match state
        .quota_service
        .list_snapshots_with_options(ListOptions {
            refresh: parse_management_bool(query.refresh.as_deref()),
            auth_id: query.auth_id.unwrap_or_default(),
            workspace_id: query.workspace_id.unwrap_or_default(),
        })
        .await
    {
        Ok(snapshots) => Json(SnapshotListResponse::from_snapshots(snapshots)).into_response(),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load codex quota snapshots: {error}"),
        ),
    }
}

pub(super) async fn refresh_codex_quota_snapshots(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    request: Request<Body>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let request = match parse_optional_json_body::<CodexQuotaRefreshRequest>(request).await {
        Ok(request) => request.unwrap_or_default(),
        Err(response) => return response,
    };

    match state
        .quota_service
        .refresh_with_options(RefreshOptions {
            auth_id: request.auth_id,
            workspace_id: request.workspace_id,
        })
        .await
    {
        Ok(snapshots) => Json(SnapshotListResponse::from_snapshots(snapshots)).into_response(),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to refresh codex quota snapshots: {error}"),
        ),
    }
}

pub(super) async fn get_codex_auth_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match state.codex_login_service.start_login() {
        Ok(started) => Json(json!({
            "status": "ok",
            "url": started.url,
            "state": started.state,
        }))
        .into_response(),
        Err(error) => oauth_status_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to generate codex authentication url: {error}"),
        ),
    }
}

fn public_responses_error_response(error: ExecuteWithRetryError<CodexCallerError>) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            CodexCallerError::UnexpectedStatus { status, body } => openai_error_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
            CodexCallerError::MissingAccessToken => {
                openai_error_response(StatusCode::UNAUTHORIZED, "Missing access token")
            }
            CodexCallerError::InvalidAuthFile(message) => {
                openai_error_response(StatusCode::UNAUTHORIZED, message)
            }
            CodexCallerError::ReadAuthFile(error) => {
                openai_error_response(StatusCode::UNAUTHORIZED, error.to_string())
            }
            CodexCallerError::Request(error) => {
                openai_error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
            }
        },
        ExecuteWithRetryError::Runtime(error) => match error {
            RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
                model,
                until,
                ..
            }) => openai_error_response(
                StatusCode::TOO_MANY_REQUESTS,
                format!("model cooldown for {model} until {until}"),
            ),
            RuntimeConductorError::Scheduler(
                SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
            ) => openai_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available"),
            RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
                openai_error_response(StatusCode::BAD_REQUEST, "No provider supplied")
            }
            RuntimeConductorError::Store(error) => {
                openai_error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            RuntimeConductorError::SelectedAuthMissing(auth_id) => openai_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("selected auth is missing from the latest snapshot set: {auth_id}"),
            ),
        },
    }
}

fn normalize_codex_api_key_models(
    models: Vec<CodexApiKeyModelEntry>,
) -> Vec<CodexApiKeyModelEntry> {
    let mut normalized = Vec::new();
    for mut model in models {
        model.name = model.name.trim().to_string();
        model.alias = model.alias.trim().to_string();
        if model.name.is_empty() && model.alias.is_empty() {
            continue;
        }
        normalized.push(model);
    }
    normalized
}

fn sanitize_loaded_codex_api_key_entries(entries: Vec<CodexApiKeyEntry>) -> Vec<CodexApiKeyEntry> {
    let mut normalized = Vec::new();

    for mut entry in entries {
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().to_string();
        entry.headers = entry.headers.and_then(normalize_headers);
        entry.excluded_models = normalize_excluded_models(entry.excluded_models);
        if entry.base_url.is_empty() {
            continue;
        }
        normalized.push(entry);
    }

    normalized
}

fn normalize_codex_api_key_entry_for_write(mut entry: CodexApiKeyEntry) -> CodexApiKeyEntry {
    entry.api_key = entry.api_key.trim().to_string();
    entry.prefix = normalize_model_prefix(&entry.prefix);
    entry.base_url = entry.base_url.trim().to_string();
    entry.proxy_url = entry.proxy_url.trim().to_string();
    entry.models = normalize_codex_api_key_models(entry.models);
    entry.headers = entry.headers.and_then(normalize_headers);
    entry.excluded_models = normalize_excluded_models(entry.excluded_models);
    entry
}

fn normalize_codex_api_key_entries_for_write(
    entries: Vec<CodexApiKeyEntry>,
) -> Vec<CodexApiKeyEntry> {
    entries
        .into_iter()
        .map(normalize_codex_api_key_entry_for_write)
        .filter(|entry| !entry.base_url.is_empty())
        .collect()
}

fn load_codex_api_key_entries(
    state: &BackendAppState,
) -> Result<Vec<CodexApiKeyEntry>, ConfigError> {
    let config = load_current_config_json(state)?;
    let entries = config_json_value(&config, "codex-api-key")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<CodexApiKeyEntry>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(sanitize_loaded_codex_api_key_entries(entries))
}

fn persist_codex_api_key_entries(
    state: &BackendAppState,
    entries: Vec<CodexApiKeyEntry>,
) -> Result<Response, ConfigError> {
    if entries.is_empty() {
        persist_top_level_config_value(state, "codex-api-key", None)
    } else {
        persist_top_level_config_value(state, "codex-api-key", Some(json!(entries)))
    }
}

fn oauth_callback_page_response(error: CodexLoginError) -> Response {
    let (status, message) = match error {
        CodexLoginError::SessionNotPending => (
            StatusCode::CONFLICT,
            "OAuth flow is not pending".to_string(),
        ),
        CodexLoginError::CallbackRejected(message) => (
            StatusCode::BAD_REQUEST,
            format!("Authentication failed: {message}"),
        ),
        CodexLoginError::MissingAuthorizationCode => (
            StatusCode::BAD_REQUEST,
            "Missing authorization code".to_string(),
        ),
        CodexLoginError::UnexpectedStatus { .. }
        | CodexLoginError::Request(_)
        | CodexLoginError::ParseToken(_)
        | CodexLoginError::MissingIdToken => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to exchange authorization code for tokens".to_string(),
        ),
        CodexLoginError::MissingEmail => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to resolve account email".to_string(),
        ),
        CodexLoginError::InvalidIdToken => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to parse ID token".to_string(),
        ),
        other => (StatusCode::BAD_REQUEST, other.to_string()),
    };
    let html = format!(
        "<html><head><meta charset=\"utf-8\"><title>Authentication failed</title></head><body><h1>Authentication failed</h1><p>{message}</p></body></html>"
    );
    (status, Html(html)).into_response()
}
