use super::*;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use futures_util::StreamExt;
use nicecli_auth::CodexLoginError;
use nicecli_quota::{
    CodexQuotaSnapshotEnvelope, ListOptions, RefreshOptions, SnapshotListResponse, PROVIDER_CODEX,
};
use nicecli_runtime::{
    CodexCallerError, CodexCompactCaller, CodexCompactRequest, CodexResponsesCaller,
    CodexResponsesRequest, ExecuteWithRetryError, ExecuteWithRetryOptions, RuntimeConductorError,
    SchedulerError,
};

mod websocket;

use self::websocket::*;

const PUBLIC_CODEX_RESPONSES_PATH: &str = "/responses";
const PUBLIC_CODEX_RESPONSES_COMPACT_PATH: &str = "/responses/compact";
const DEFAULT_PUBLIC_CODEX_USER_AGENT: &str =
    "codex_cli_rs/0.116.0 (Windows NT 10.0; Win64; x64) NiceCLI";
const CODEX_API_KEY_QUOTA_SOURCE: &str = "codex_api_key";
const CODEX_API_KEY_WORKSPACE_TYPE: &str = "third_party";

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
    #[serde(skip_serializing_if = "String::is_empty")]
    label: String,
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
    label: Option<String>,
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

    let config_json = match load_current_config_json(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let snapshots = match FileAuthStore::new(&state.auth_dir).list_snapshots() {
        Ok(snapshots) => snapshots,
        Err(error) => return auth_snapshot_store_error_response(error),
    };
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
    let mut runtime_models =
        requested_public_codex_runtime_models(&config_json, &snapshots, model.as_str());
    if runtime_models.is_empty()
        && !model.trim().is_empty()
        && !model.contains('/')
        && snapshots
            .iter()
            .any(|snapshot| snapshot.provider.eq_ignore_ascii_case("codex"))
    {
        runtime_models.push(model.clone());
    }
    let api_key_targets = requested_public_codex_api_key_targets(&config_json, model.as_str());
    let prefer_api_key_targets = model.contains('/');
    let mut last_error = None;

    if stream_requested {
        if prefer_api_key_targets {
            for target in &api_key_targets {
                let patched_body = patch_public_codex_request_body(
                    &raw_body,
                    parsed_json.as_ref(),
                    target.upstream_model.as_str(),
                    compact,
                );
                match execute_public_codex_api_key_stream_request(
                    target,
                    default_proxy_url.as_deref(),
                    user_agent.as_str(),
                    patched_body,
                )
                .await
                {
                    Ok(response) => return reqwest_stream_response(response),
                    Err(error) => last_error = Some(public_codex_provider_error_response(error)),
                }
            }
        }

        for runtime_model in &runtime_models {
            let patched_body = patch_public_codex_request_body(
                &raw_body,
                parsed_json.as_ref(),
                runtime_model.as_str(),
                compact,
            );
            match try_execute_public_codex_runtime_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                runtime_model.as_str(),
                patched_body,
                ExecuteWithRetryOptions::new(chrono::Utc::now()),
            )
            .await
            {
                Ok(Some(response)) => return reqwest_stream_response(response.value),
                Ok(None) => {}
                Err(error) => last_error = Some(public_responses_error_response(error)),
            }
        }

        if !prefer_api_key_targets {
            for target in &api_key_targets {
                let patched_body = patch_public_codex_request_body(
                    &raw_body,
                    parsed_json.as_ref(),
                    target.upstream_model.as_str(),
                    compact,
                );
                match execute_public_codex_api_key_stream_request(
                    target,
                    default_proxy_url.as_deref(),
                    user_agent.as_str(),
                    patched_body,
                )
                .await
                {
                    Ok(response) => return reqwest_stream_response(response),
                    Err(error) => last_error = Some(public_codex_provider_error_response(error)),
                }
            }
        }

        return last_error.unwrap_or_else(|| {
            openai_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
        });
    }

    if prefer_api_key_targets {
        for target in &api_key_targets {
            let patched_body = patch_public_codex_request_body(
                &raw_body,
                parsed_json.as_ref(),
                target.upstream_model.as_str(),
                compact,
            );
            match execute_public_codex_api_key_http_request(
                target,
                default_proxy_url.as_deref(),
                user_agent.as_str(),
                if compact {
                    PUBLIC_CODEX_RESPONSES_COMPACT_PATH
                } else {
                    PUBLIC_CODEX_RESPONSES_PATH
                },
                patched_body,
            )
            .await
            {
                Ok(response) => return provider_http_response(response),
                Err(error) => last_error = Some(public_codex_provider_error_response(error)),
            }
        }
    }

    for runtime_model in &runtime_models {
        let patched_body = patch_public_codex_request_body(
            &raw_body,
            parsed_json.as_ref(),
            runtime_model.as_str(),
            compact,
        );
        match try_execute_public_codex_runtime_request(
            state.clone(),
            strategy,
            default_proxy_url.clone(),
            user_agent.as_str(),
            runtime_model.as_str(),
            patched_body,
            compact,
        )
        .await
        {
            Ok(Some(response)) => return provider_http_response(response),
            Ok(None) => {}
            Err(error) => last_error = Some(public_responses_error_response(error)),
        }
    }

    if !prefer_api_key_targets {
        for target in &api_key_targets {
            let patched_body = patch_public_codex_request_body(
                &raw_body,
                parsed_json.as_ref(),
                target.upstream_model.as_str(),
                compact,
            );
            match execute_public_codex_api_key_http_request(
                target,
                default_proxy_url.as_deref(),
                user_agent.as_str(),
                if compact {
                    PUBLIC_CODEX_RESPONSES_COMPACT_PATH
                } else {
                    PUBLIC_CODEX_RESPONSES_PATH
                },
                patched_body,
            )
            .await
            {
                Ok(response) => return provider_http_response(response),
                Err(error) => last_error = Some(public_codex_provider_error_response(error)),
            }
        }
    }

    last_error.unwrap_or_else(|| {
        openai_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
    })
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
        let config_json = match load_current_config_json(&state) {
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
        let snapshots = match FileAuthStore::new(&state.auth_dir).list_snapshots() {
            Ok(snapshots) => snapshots,
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
        let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
        let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
        let mut runtime_models =
            requested_public_codex_runtime_models(&config_json, &snapshots, model.as_str());
        if runtime_models.is_empty()
            && !model.trim().is_empty()
            && !model.contains('/')
            && snapshots
                .iter()
                .any(|snapshot| snapshot.provider.eq_ignore_ascii_case("codex"))
        {
            runtime_models.push(model.clone());
        }
        let api_key_targets = requested_public_codex_api_key_targets(&config_json, model.as_str());
        let prefer_api_key_targets = model.contains('/');
        let mut handled = false;

        if prefer_api_key_targets {
            for target in &api_key_targets {
                let patched_request = patch_public_codex_request_body(
                    &normalized.request,
                    Some(&parsed_request),
                    target.upstream_model.as_str(),
                    false,
                );
                match execute_public_codex_api_key_stream_request(
                    target,
                    default_proxy_url.as_deref(),
                    user_agent.as_str(),
                    patched_request,
                )
                .await
                {
                    Ok(response) => {
                        handled = true;
                        match forward_responses_websocket_stream(&mut socket, response).await {
                            Ok(output) => {
                                last_response_output = output;
                            }
                            Err(_) => break,
                        }
                        break;
                    }
                    Err(error) => {
                        let error = responses_websocket_error_from_codex_provider_error(error);
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
            if handled {
                continue;
            }
        }

        for runtime_model in &runtime_models {
            let patched_request = patch_public_codex_request_body(
                &normalized.request,
                Some(&parsed_request),
                runtime_model.as_str(),
                false,
            );
            let mut options = ExecuteWithRetryOptions::new(chrono::Utc::now());
            options.pick.prefer_websocket = true;
            if let Some(pinned_auth_id) = pinned_auth_id.as_deref() {
                options.pick.pinned_auth_id = Some(pinned_auth_id.to_string());
            }

            match try_execute_public_codex_runtime_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                runtime_model.as_str(),
                patched_request,
                options,
            )
            .await
            {
                Ok(Some(executed)) => {
                    handled = true;
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
                    break;
                }
                Ok(None) => {}
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

        if handled {
            continue;
        }

        if !prefer_api_key_targets {
            for target in &api_key_targets {
                let patched_request = patch_public_codex_request_body(
                    &normalized.request,
                    Some(&parsed_request),
                    target.upstream_model.as_str(),
                    false,
                );
                match execute_public_codex_api_key_stream_request(
                    target,
                    default_proxy_url.as_deref(),
                    user_agent.as_str(),
                    patched_request,
                )
                .await
                {
                    Ok(response) => {
                        handled = true;
                        match forward_responses_websocket_stream(&mut socket, response).await {
                            Ok(output) => {
                                last_response_output = output;
                            }
                            Err(_) => break,
                        }
                        break;
                    }
                    Err(error) => {
                        let error = responses_websocket_error_from_codex_provider_error(error);
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

        if !handled
            && write_responses_websocket_error(
                &mut socket,
                StatusCode::SERVICE_UNAVAILABLE,
                "No auth available",
                None,
            )
            .await
            .is_err()
        {
            break;
        }
    }
}

#[derive(Debug, Clone)]
struct CodexApiKeyExecutionTarget {
    entry: CodexApiKeyEntry,
    upstream_model: String,
}

fn patch_public_codex_request_body(
    raw_body: &[u8],
    parsed_json: Option<&JsonValue>,
    model: &str,
    compact: bool,
) -> Vec<u8> {
    let Some(JsonValue::Object(object)) = parsed_json else {
        return if compact {
            strip_json_object_field(raw_body, parsed_json, "stream")
        } else {
            raw_body.to_vec()
        };
    };
    let current_model = object
        .get("model")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if current_model.eq(model.trim()) && (!compact || !object.contains_key("stream")) {
        return raw_body.to_vec();
    }

    let mut next = object.clone();
    next.insert("model".to_string(), json!(model.trim()));
    if compact {
        next.remove("stream");
    }
    serde_json::to_vec(&JsonValue::Object(next)).unwrap_or_else(|_| {
        if compact {
            strip_json_object_field(raw_body, parsed_json, "stream")
        } else {
            raw_body.to_vec()
        }
    })
}

fn requested_public_codex_runtime_models(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    requested_model: &str,
) -> Vec<String> {
    let requested_key = normalize_model_identifier(requested_model);
    if requested_key.is_empty() {
        return Vec::new();
    }

    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();

    for snapshot in snapshots {
        if !snapshot.provider.eq_ignore_ascii_case("codex") {
            continue;
        }

        for upstream_model in resolve_requested_public_codex_snapshot_models(
            config,
            snapshot,
            &requested_key,
            force_prefix,
        ) {
            let key = normalize_model_identifier(&upstream_model);
            if !key.is_empty() && seen.insert(key) {
                resolved.push(upstream_model);
            }
        }
    }

    resolved
}

fn resolve_requested_public_codex_snapshot_models(
    config: &JsonValue,
    snapshot: &AuthSnapshot,
    requested_key: &str,
    force_prefix: bool,
) -> Vec<String> {
    let mut base_models = collect_auth_file_model_infos_from_config(
        config,
        "codex",
        snapshot.account_plan.as_deref(),
    );

    if snapshot.candidate_state.has_explicit_supported_models {
        base_models.retain(|model| snapshot_allows_model(snapshot, model));
        append_explicit_snapshot_models(snapshot, "codex", &mut base_models);
    } else if base_models.is_empty() {
        append_explicit_snapshot_models(snapshot, "codex", &mut base_models);
    }

    let mut excluded = oauth_excluded_models_for_provider(config, "codex");
    if !snapshot.candidate_state.excluded_models.is_empty() {
        excluded = snapshot
            .candidate_state
            .excluded_models
            .iter()
            .cloned()
            .collect::<Vec<_>>();
    }
    if !excluded.is_empty() {
        base_models = apply_excluded_model_patterns(base_models, &excluded);
    }

    let aliases = oauth_model_alias_entries_for_provider(config, "codex");
    let prefix = snapshot
        .prefix
        .as_deref()
        .map(normalize_model_prefix)
        .filter(|value| !value.is_empty());

    let mut resolved = Vec::new();
    for base_model in base_models {
        let upstream_model = base_model.id.trim().to_string();
        if upstream_model.is_empty() {
            continue;
        }

        let mut public_models = apply_public_oauth_model_alias(vec![base_model], &aliases);
        if let Some(prefix) = prefix.as_deref() {
            public_models = apply_public_model_prefixes(public_models, prefix, force_prefix);
        }

        if public_models
            .iter()
            .any(|model| model_matches_identifier(model, requested_key))
        {
            resolved.push(upstream_model);
        }
    }

    resolved
}

fn requested_public_codex_api_key_targets(
    config: &JsonValue,
    requested_model: &str,
) -> Vec<CodexApiKeyExecutionTarget> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let mut targets = Vec::new();
    let mut seen = HashSet::new();

    for entry in codex_api_key_entries_from_config_json(config) {
        let Some(upstream_model) =
            resolve_codex_api_key_entry_model(&entry, requested_model, force_prefix)
        else {
            continue;
        };
        let dedupe_key = format!(
            "{}\n{}\n{}\n{}",
            entry.base_url.trim(),
            entry.proxy_url.trim(),
            entry.api_key.trim(),
            normalize_model_identifier(&upstream_model)
        );
        if !seen.insert(dedupe_key) {
            continue;
        }
        targets.push(CodexApiKeyExecutionTarget {
            entry,
            upstream_model,
        });
    }

    targets
}

fn resolve_codex_api_key_entry_model(
    entry: &CodexApiKeyEntry,
    requested_model: &str,
    force_prefix: bool,
) -> Option<String> {
    let requested_key = normalize_model_identifier(requested_model);
    if requested_key.is_empty() {
        return None;
    }

    let prefix = normalize_model_prefix(&entry.prefix);
    if !entry.models.is_empty() {
        for model in &entry.models {
            let upstream_model = model.name.trim();
            if upstream_model.is_empty() {
                continue;
            }
            let public_model = if model.alias.trim().is_empty() {
                upstream_model
            } else {
                model.alias.trim()
            };
            if public_codex_model_variants(public_model, prefix.as_str(), force_prefix)
                .iter()
                .any(|candidate| model_matches_identifier(candidate, &requested_key))
            {
                return Some(upstream_model.to_string());
            }
        }
        return None;
    }

    let requested = requested_model.trim();
    if requested.is_empty() {
        return None;
    }

    let mut upstream_candidates = Vec::new();
    if prefix.is_empty() {
        upstream_candidates.push(requested.to_string());
    } else if let Some(stripped) = requested
        .strip_prefix(format!("{prefix}/").as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        upstream_candidates.push(stripped.to_string());
    } else if !force_prefix {
        upstream_candidates.push(requested.to_string());
    }

    upstream_candidates.into_iter().find(|candidate| {
        let candidate_key = normalize_model_identifier(candidate);
        !candidate_key.is_empty()
            && !entry
                .excluded_models
                .iter()
                .any(|model| model == &candidate_key)
    })
}

fn public_codex_model_variants(
    public_model: &str,
    prefix: &str,
    force_prefix: bool,
) -> Vec<ModelInfo> {
    let model = build_minimal_public_model_info("codex", public_model);
    if prefix.is_empty() {
        vec![model]
    } else {
        apply_public_model_prefixes(vec![model], prefix, force_prefix)
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
    if let Some(label) = value.label {
        entry.label = label.trim().to_string();
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

    let config = match load_current_config_json(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let refresh = parse_management_bool(query.refresh.as_deref());
    let auth_id = query.auth_id.unwrap_or_default();
    let workspace_id = query.workspace_id.unwrap_or_default();
    let api_key_snapshots =
        codex_api_key_quota_snapshots_from_config_json(&config, &auth_id, &workspace_id);

    match state
        .quota_service
        .list_snapshots_with_options(ListOptions {
            refresh,
            auth_id,
            workspace_id,
        })
        .await
    {
        Ok(mut snapshots) => {
            snapshots.extend(api_key_snapshots);
            Json(SnapshotListResponse::from_snapshots(snapshots)).into_response()
        }
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
    let config = match load_current_config_json(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let api_key_snapshots = codex_api_key_quota_snapshots_from_config_json(
        &config,
        &request.auth_id,
        &request.workspace_id,
    );

    match state
        .quota_service
        .refresh_with_options(RefreshOptions {
            auth_id: request.auth_id,
            workspace_id: request.workspace_id,
        })
        .await
    {
        Ok(mut snapshots) => {
            snapshots.extend(api_key_snapshots);
            Json(SnapshotListResponse::from_snapshots(snapshots)).into_response()
        }
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to refresh codex quota snapshots: {error}"),
        ),
    }
}

fn codex_api_key_quota_snapshots_from_config_json(
    config: &JsonValue,
    auth_id_filter: &str,
    workspace_id_filter: &str,
) -> Vec<CodexQuotaSnapshotEnvelope> {
    let auth_id_filter = auth_id_filter.trim();
    let workspace_id_filter = workspace_id_filter.trim();
    let fetched_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    codex_api_key_entries_from_config_json(config)
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let base_url = entry.base_url.trim().to_string();
            if base_url.is_empty() {
                return None;
            }

            let auth_id = format!("codex-api-key:{}", index + 1);
            let workspace_id = format!("{auth_id}:workspace");
            if !auth_id_filter.is_empty() && auth_id_filter != auth_id {
                return None;
            }
            if !workspace_id_filter.is_empty() && workspace_id_filter != workspace_id {
                return None;
            }

            let label = entry.label.trim().to_string();
            let display_name = if label.is_empty() {
                base_url.clone()
            } else {
                label
            };
            let auth_label = Some(display_name.clone());
            let auth_note = Some(base_url.clone());

            Some(CodexQuotaSnapshotEnvelope {
                provider: PROVIDER_CODEX.to_string(),
                auth_id,
                auth_label: auth_label.clone(),
                auth_note,
                auth_file_name: None,
                account_email: None,
                account_plan: None,
                workspace_id: Some(workspace_id),
                workspace_name: auth_label,
                workspace_type: Some(CODEX_API_KEY_WORKSPACE_TYPE.to_string()),
                snapshot: None,
                source: CODEX_API_KEY_QUOTA_SOURCE.to_string(),
                fetched_at: fetched_at.clone(),
                stale: false,
                error: None,
            })
        })
        .collect()
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

async fn try_execute_public_codex_runtime_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
    compact: bool,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<CodexCallerError>> {
    let options = ExecuteWithRetryOptions::new(chrono::Utc::now());
    if compact {
        let mut caller = CodexCompactCaller::new(&state.auth_dir, strategy)
            .with_default_proxy_url(default_proxy_url)
            .with_user_agent(user_agent.to_string());
        match caller
            .execute(
                CodexCompactRequest {
                    model: model.to_string(),
                    body,
                },
                options,
            )
            .await
        {
            Ok(response) => Ok(Some(response.value)),
            Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
                SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
            ))) => Ok(None),
            Err(error) => Err(error),
        }
    } else {
        let mut caller = CodexResponsesCaller::new(&state.auth_dir, strategy)
            .with_default_proxy_url(default_proxy_url)
            .with_user_agent(user_agent.to_string());
        match caller
            .execute(
                CodexResponsesRequest {
                    model: model.to_string(),
                    body,
                },
                options,
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
}

async fn try_execute_public_codex_runtime_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
    options: ExecuteWithRetryOptions,
) -> Result<
    Option<nicecli_runtime::Executed<reqwest::Response>>,
    ExecuteWithRetryError<CodexCallerError>,
> {
    let mut caller = CodexResponsesCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute_stream(
            CodexResponsesRequest {
                model: model.to_string(),
                body,
            },
            options,
        )
        .await
    {
        Ok(response) => Ok(Some(response)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn execute_public_codex_api_key_http_request(
    target: &CodexApiKeyExecutionTarget,
    default_proxy_url: Option<&str>,
    user_agent: &str,
    endpoint_path: &str,
    body: Vec<u8>,
) -> Result<ProviderHttpResponse, CodexCallerError> {
    let client = build_public_codex_api_key_http_client(
        trim_optional_string(Some(target.entry.proxy_url.as_str()))
            .as_deref()
            .or(default_proxy_url),
    )?;
    let response = send_public_codex_api_key_request(
        &client,
        target,
        user_agent,
        endpoint_path,
        body,
        "application/json",
    )
    .await?;

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response.bytes().await?.to_vec();
    if !(200..300).contains(&status) {
        return Err(CodexCallerError::UnexpectedStatus {
            status,
            body: public_codex_error_body_message(&body),
        });
    }

    Ok(ProviderHttpResponse {
        status,
        headers,
        body,
    })
}

async fn execute_public_codex_api_key_stream_request(
    target: &CodexApiKeyExecutionTarget,
    default_proxy_url: Option<&str>,
    user_agent: &str,
    body: Vec<u8>,
) -> Result<reqwest::Response, CodexCallerError> {
    let client = build_public_codex_api_key_http_client(
        trim_optional_string(Some(target.entry.proxy_url.as_str()))
            .as_deref()
            .or(default_proxy_url),
    )?;
    let response = send_public_codex_api_key_request(
        &client,
        target,
        user_agent,
        PUBLIC_CODEX_RESPONSES_PATH,
        body,
        "text/event-stream",
    )
    .await?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = response.bytes().await?.to_vec();
        return Err(CodexCallerError::UnexpectedStatus {
            status,
            body: public_codex_error_body_message(&body),
        });
    }

    Ok(response)
}

fn build_public_codex_api_key_http_client(
    proxy_url: Option<&str>,
) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy_url.and_then(|value| trim_optional_string(Some(value))) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

async fn send_public_codex_api_key_request(
    client: &Client,
    target: &CodexApiKeyExecutionTarget,
    user_agent: &str,
    endpoint_path: &str,
    body: Vec<u8>,
    accept: &str,
) -> Result<reqwest::Response, CodexCallerError> {
    let url = format!(
        "{}{}",
        target.entry.base_url.trim_end_matches('/'),
        endpoint_path
    );
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, accept)
        .header(
            REQWEST_USER_AGENT,
            if user_agent.trim().is_empty() {
                DEFAULT_PUBLIC_CODEX_USER_AGENT
            } else {
                user_agent.trim()
            },
        );

    if let Some(api_key) = trim_optional_string(Some(target.entry.api_key.as_str())) {
        builder = builder.header(REQWEST_AUTHORIZATION, format!("Bearer {api_key}"));
    }

    if let Some(headers) = target.entry.headers.as_ref() {
        for (name, value) in headers {
            let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) else {
                continue;
            };
            let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) else {
                continue;
            };
            builder = builder.header(header_name, header_value);
        }
    }

    builder
        .body(body)
        .send()
        .await
        .map_err(CodexCallerError::Request)
}

fn public_codex_error_body_message(body: &[u8]) -> String {
    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        "request failed".to_string()
    } else {
        trimmed
    }
}

fn public_codex_provider_error_response(error: CodexCallerError) -> Response {
    public_responses_error_response(ExecuteWithRetryError::Provider(error))
}

fn responses_websocket_error_from_codex_provider_error(
    error: CodexCallerError,
) -> ResponsesWebsocketError {
    responses_websocket_error_from_codex_error(ExecuteWithRetryError::Provider(error))
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
        entry.label = entry.label.trim().to_string();
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
    entry.label = entry.label.trim().to_string();
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

fn codex_api_key_entries_from_config_json(config: &JsonValue) -> Vec<CodexApiKeyEntry> {
    let entries = config_json_value(config, "codex-api-key")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<CodexApiKeyEntry>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    sanitize_loaded_codex_api_key_entries(entries)
}

fn load_codex_api_key_entries(
    state: &BackendAppState,
) -> Result<Vec<CodexApiKeyEntry>, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(codex_api_key_entries_from_config_json(&config))
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
