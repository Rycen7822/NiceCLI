use super::*;
use nicecli_runtime::{
    ClaudeCallerError, ClaudeMessagesCaller, ClaudeMessagesRequest, ExecuteWithRetryError,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct ClaudeApiKeyModelEntry {
    name: String,
    alias: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct ClaudeApiKeyEntry {
    #[serde(rename = "api-key")]
    api_key: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    priority: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    prefix: String,
    #[serde(rename = "base-url")]
    base_url: String,
    #[serde(rename = "proxy-url")]
    proxy_url: String,
    models: Vec<ClaudeApiKeyModelEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<BTreeMap<String, String>>,
    #[serde(rename = "excluded-models", skip_serializing_if = "Vec::is_empty")]
    excluded_models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cloak: Option<JsonValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct ClaudeApiKeyPatchValue {
    #[serde(rename = "api-key")]
    api_key: Option<String>,
    prefix: Option<String>,
    #[serde(rename = "base-url")]
    base_url: Option<String>,
    #[serde(rename = "proxy-url")]
    proxy_url: Option<String>,
    models: Option<Vec<ClaudeApiKeyModelEntry>>,
    headers: Option<BTreeMap<String, String>>,
    #[serde(rename = "excluded-models")]
    excluded_models: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct ClaudeApiKeyPatchRequest {
    index: Option<usize>,
    #[serde(rename = "match")]
    match_value: Option<String>,
    value: Option<ClaudeApiKeyPatchValue>,
}

pub(super) async fn get_claude_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_claude_api_key_entries(&state) {
        Ok(entries) => single_field_json_response("claude-api-key", json!(entries)),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_claude_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: Vec<ClaudeApiKeyEntry> = match parse_json_or_items_wrapper(&body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body")
        }
    };

    match persist_claude_api_key_entries(&state, normalize_claude_api_key_entries_for_write(parsed))
    {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn patch_claude_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ClaudeApiKeyPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    let mut entries = match load_claude_api_key_entries(&state) {
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
        entry.base_url = base_url.trim().to_string();
    }
    if let Some(proxy_url) = value.proxy_url {
        entry.proxy_url = proxy_url.trim().to_string();
    }
    if let Some(models) = value.models {
        entry.models = normalize_claude_api_key_models(models);
    }
    if let Some(headers) = value.headers {
        let mut merged = entry.headers.unwrap_or_default();
        if let Some(normalized) = normalize_headers(headers) {
            for (key, value) in normalized {
                merged.insert(key, value);
            }
        }
        entry.headers = if merged.is_empty() {
            None
        } else {
            Some(merged)
        };
    }
    if let Some(excluded_models) = value.excluded_models {
        entry.excluded_models = normalize_excluded_models(excluded_models);
    }

    entries[target_index] = normalize_claude_api_key_entry_for_write(entry);
    match persist_claude_api_key_entries(&state, entries) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn delete_claude_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<ApiKeyIndexQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut entries = match load_claude_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    if let Some(api_key) = query.api_key.as_deref().filter(|value| !value.is_empty()) {
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

    match persist_claude_api_key_entries(&state, entries) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn handle_anthropic_callback(
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

    match write_oauth_callback_file_for_pending_session(
        &state.auth_dir,
        state.oauth_sessions.as_ref(),
        "anthropic",
        &state_value,
        &code,
        &callback_error,
    ) {
        Ok(_) => Html(OAUTH_CALLBACK_SUCCESS_HTML).into_response(),
        Err(error) => oauth_callback_file_page_response(error),
    }
}

pub(super) async fn get_anthropic_auth_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match state.anthropic_login_service.start_login() {
        Ok(started) => {
            let auth_dir = state.auth_dir.clone();
            let anthropic_login_service = state.anthropic_login_service.clone();
            let state_value = started.state.clone();
            tokio::spawn(async move {
                let _ = anthropic_login_service
                    .complete_login(&auth_dir, &state_value)
                    .await;
            });

            Json(json!({
                "status": "ok",
                "url": started.url,
                "state": started.state,
            }))
            .into_response()
        }
        Err(error) => oauth_status_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to generate anthropic authentication url: {error}"),
        ),
    }
}

pub(super) fn request_prefers_claude_models(headers: &HeaderMap) -> bool {
    headers
        .get("User-Agent")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.starts_with("claude-cli"))
}

pub(super) fn collect_public_claude_models(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<JsonValue> {
    collect_public_claude_model_infos(config, snapshots)
        .into_iter()
        .map(|model| claude_public_payload_from_model_info(&model))
        .collect()
}

pub(super) fn maybe_public_claude_models_response(
    headers: &HeaderMap,
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Option<Response> {
    if !request_prefers_claude_models(headers) {
        return None;
    }

    let data = collect_public_claude_models(config, snapshots);
    let first_id = data
        .first()
        .and_then(JsonValue::as_object)
        .and_then(|model| first_non_empty_string(model, &["id"]));
    let last_id = data
        .last()
        .and_then(JsonValue::as_object)
        .and_then(|model| first_non_empty_string(model, &["id"]));

    Some(
        Json(json!({
            "data": data,
            "has_more": false,
            "first_id": first_id.unwrap_or_default(),
            "last_id": last_id.unwrap_or_default(),
        }))
        .into_response(),
    )
}

pub(super) async fn execute_public_claude_messages_request(
    state: Arc<BackendAppState>,
    headers: HeaderMap,
    query: PublicApiAuthQuery,
    request: Request<Body>,
) -> Response {
    execute_public_claude_request(state, headers, query, request, false).await
}

pub(super) async fn execute_public_claude_count_tokens_request(
    state: Arc<BackendAppState>,
    headers: HeaderMap,
    query: PublicApiAuthQuery,
    request: Request<Body>,
) -> Response {
    execute_public_claude_request(state, headers, query, request, true).await
}

async fn execute_public_claude_request(
    state: Arc<BackendAppState>,
    headers: HeaderMap,
    query: PublicApiAuthQuery,
    request: Request<Body>,
    count_tokens: bool,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let config_json = match load_current_config_json(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let snapshots = match FileAuthStore::new(&state.auth_dir).list_snapshots() {
        Ok(snapshots) => snapshots,
        Err(error) => return auth_snapshot_store_error_response(error),
    };

    let raw_body = match to_bytes(request.into_body(), usize::MAX).await {
        Ok(body) => body.to_vec(),
        Err(_) => return claude_error_response(StatusCode::BAD_REQUEST, "Invalid request"),
    };
    let parsed_json = serde_json::from_slice::<JsonValue>(&raw_body).ok();
    let stream_requested = !count_tokens
        && parsed_json
            .as_ref()
            .and_then(|value| value.get("stream"))
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);

    let requested_model = parsed_json
        .as_ref()
        .and_then(|value| value.get("model"))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if requested_model.is_empty() {
        return claude_error_response(StatusCode::BAD_REQUEST, "Model is required");
    }

    if find_public_claude_model(&config_json, &snapshots, &requested_model).is_none() {
        return claude_error_response(StatusCode::NOT_FOUND, "Model not found");
    }

    let config = match load_current_config(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
    let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
    let user_agent = extract_trimmed_header_value(&headers, "User-Agent").unwrap_or_default();
    let mut last_error = None;

    for candidate_model in
        requested_public_claude_model_candidates(&config_json, &snapshots, &requested_model)
    {
        if count_tokens {
            match try_execute_public_claude_count_tokens_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                raw_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return provider_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_claude_error_response(error)),
            }
            continue;
        }

        if stream_requested {
            match try_execute_public_claude_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                raw_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return claude_provider_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_claude_error_response(error)),
            }
        } else {
            match try_execute_public_claude_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                raw_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return provider_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_claude_error_response(error)),
            }
        }
    }

    last_error.unwrap_or_else(|| {
        claude_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
    })
}

async fn try_execute_public_claude_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<ClaudeCallerError>> {
    let mut caller = ClaudeMessagesCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute(
            ClaudeMessagesRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn try_execute_public_claude_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<reqwest::Response>, ExecuteWithRetryError<ClaudeCallerError>> {
    let mut caller = ClaudeMessagesCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute_stream(
            ClaudeMessagesRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn try_execute_public_claude_count_tokens_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<ClaudeCallerError>> {
    let mut caller = ClaudeMessagesCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .count_tokens(
            ClaudeMessagesRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

fn find_public_claude_model(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    target: &str,
) -> Option<ModelInfo> {
    let target_key = normalize_model_identifier(target);
    if target_key.is_empty() {
        return None;
    }

    collect_public_claude_model_infos(config, snapshots)
        .into_iter()
        .find(|model| model_matches_identifier(model, &target_key))
}

fn requested_public_claude_model_candidates(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    requested_model: &str,
) -> Vec<String> {
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    push_public_claude_candidate(&mut resolved, &mut seen, requested_model);

    let current = resolved.clone();
    for snapshot in snapshots {
        if !provider_in_list(&snapshot.provider, &["claude"]) {
            continue;
        }
        let Some(prefix) = snapshot
            .prefix
            .as_deref()
            .map(normalize_model_prefix)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for candidate in &current {
            if let Some(stripped) = strip_public_claude_prefixed_model_id(candidate, &prefix) {
                push_public_claude_candidate(&mut resolved, &mut seen, &stripped);
            }
        }
    }

    let current = resolved.clone();
    for candidate in &current {
        for entry in oauth_model_alias_entries_for_provider(config, "claude") {
            if normalize_model_identifier(&entry.alias) != normalize_model_identifier(candidate) {
                continue;
            }
            push_public_claude_candidate(&mut resolved, &mut seen, &entry.name);
        }
    }

    resolved.into_iter().rev().collect()
}

fn push_public_claude_candidate(
    resolved: &mut Vec<String>,
    seen: &mut HashSet<String>,
    candidate: &str,
) {
    let trimmed = candidate.trim();
    let key = normalize_model_identifier(trimmed);
    if key.is_empty() || !seen.insert(key) {
        return;
    }
    resolved.push(trimmed.to_string());
}

fn strip_public_claude_prefixed_model_id(value: &str, prefix: &str) -> Option<String> {
    let trimmed = value.trim();
    let prefix = prefix.trim().trim_matches('/');
    let expected_prefix = format!("{prefix}/");
    trimmed
        .strip_prefix(expected_prefix.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn public_claude_error_response(error: ExecuteWithRetryError<ClaudeCallerError>) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            ClaudeCallerError::UnexpectedStatus { status, body }
            | ClaudeCallerError::RefreshRejected { status, body } => claude_error_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
            ClaudeCallerError::MissingAccessToken => {
                claude_error_response(StatusCode::UNAUTHORIZED, "Missing access token")
            }
            ClaudeCallerError::InvalidAuthFile(message) => {
                claude_error_response(StatusCode::UNAUTHORIZED, message)
            }
            ClaudeCallerError::ReadAuthFile(error) | ClaudeCallerError::WriteAuthFile(error) => {
                claude_error_response(StatusCode::UNAUTHORIZED, error.to_string())
            }
            ClaudeCallerError::Request(error) => {
                claude_error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
            }
        },
        ExecuteWithRetryError::Runtime(error) => public_claude_runtime_error_response(error),
    }
}

fn public_claude_runtime_error_response(error: RuntimeConductorError) -> Response {
    match error {
        RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
            model, until, ..
        }) => claude_error_response(
            StatusCode::TOO_MANY_REQUESTS,
            format!("model cooldown for {model} until {until}"),
        ),
        RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ) => claude_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available"),
        RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
            claude_error_response(StatusCode::BAD_REQUEST, "No provider supplied")
        }
        RuntimeConductorError::Store(error) => {
            claude_error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
        RuntimeConductorError::SelectedAuthMissing(auth_id) => claude_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("selected auth is missing from the latest snapshot set: {auth_id}"),
        ),
    }
}

fn claude_error_response(status: StatusCode, message: impl AsRef<str>) -> Response {
    (
        status,
        Json(json!({
            "type": "error",
            "error": {
                "type": claude_error_type_for_status(status),
                "message": message.as_ref(),
            }
        })),
    )
        .into_response()
}

fn claude_error_type_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid_request_error",
        StatusCode::UNAUTHORIZED => "authentication_error",
        StatusCode::FORBIDDEN => "permission_error",
        StatusCode::NOT_FOUND => "not_found_error",
        StatusCode::TOO_MANY_REQUESTS => "rate_limit_error",
        StatusCode::SERVICE_UNAVAILABLE => "overloaded_error",
        _ => "api_error",
    }
}

fn claude_provider_stream_response(response: reqwest::Response) -> Response {
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
            claude_error_response(StatusCode::BAD_GATEWAY, "Failed to build response")
        })
}

fn collect_public_claude_model_infos(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<ModelInfo> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let mut models = Vec::new();
    let mut seen = HashSet::new();

    append_public_model_infos_from_config(
        config,
        "claude-api-key",
        "claude",
        None,
        force_prefix,
        &mut seen,
        &mut models,
    );
    append_public_model_infos_from_snapshots(
        config,
        snapshots,
        &["claude"],
        force_prefix,
        &mut seen,
        &mut models,
    );

    models
}

fn claude_public_payload_from_model_info(model: &ModelInfo) -> JsonValue {
    let mut payload = serde_json::Map::new();
    payload.insert("id".to_string(), json!(model.id));
    payload.insert("object".to_string(), json!("model"));
    if let Some(created) = model.created {
        payload.insert("created_at".to_string(), json!(created));
    }
    if let Some(owned_by) = model
        .owned_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("owned_by".to_string(), json!(owned_by));
    }
    if model.model_type.is_some() {
        payload.insert("type".to_string(), json!("model"));
    }
    if let Some(display_name) = model
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("display_name".to_string(), json!(display_name));
    }
    JsonValue::Object(payload)
}

fn normalize_claude_api_key_models(
    models: Vec<ClaudeApiKeyModelEntry>,
) -> Vec<ClaudeApiKeyModelEntry> {
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

fn sanitize_loaded_claude_api_key_entries(
    entries: Vec<ClaudeApiKeyEntry>,
) -> Vec<ClaudeApiKeyEntry> {
    let mut normalized = Vec::with_capacity(entries.len());

    for mut entry in entries {
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.headers = entry.headers.and_then(normalize_headers);
        entry.excluded_models = normalize_excluded_models(entry.excluded_models);
        normalized.push(entry);
    }

    normalized
}

fn normalize_claude_api_key_entry_for_write(mut entry: ClaudeApiKeyEntry) -> ClaudeApiKeyEntry {
    entry.api_key = entry.api_key.trim().to_string();
    entry.prefix = normalize_model_prefix(&entry.prefix);
    entry.base_url = entry.base_url.trim().to_string();
    entry.proxy_url = entry.proxy_url.trim().to_string();
    entry.models = normalize_claude_api_key_models(entry.models);
    entry.headers = entry.headers.and_then(normalize_headers);
    entry.excluded_models = normalize_excluded_models(entry.excluded_models);
    entry
}

fn normalize_claude_api_key_entries_for_write(
    entries: Vec<ClaudeApiKeyEntry>,
) -> Vec<ClaudeApiKeyEntry> {
    entries
        .into_iter()
        .map(normalize_claude_api_key_entry_for_write)
        .collect()
}

fn load_claude_api_key_entries(
    state: &BackendAppState,
) -> Result<Vec<ClaudeApiKeyEntry>, ConfigError> {
    let config = load_current_config_json(state)?;
    let entries = config_json_value(&config, "claude-api-key")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<ClaudeApiKeyEntry>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(sanitize_loaded_claude_api_key_entries(entries))
}

fn persist_claude_api_key_entries(
    state: &BackendAppState,
    entries: Vec<ClaudeApiKeyEntry>,
) -> Result<Response, ConfigError> {
    if entries.is_empty() {
        persist_top_level_config_value(state, "claude-api-key", None)
    } else {
        persist_top_level_config_value(state, "claude-api-key", Some(json!(entries)))
    }
}
