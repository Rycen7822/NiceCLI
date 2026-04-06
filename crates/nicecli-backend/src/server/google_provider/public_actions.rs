use super::public_action_auth_files::{
    try_execute_public_antigravity_auth_request,
    try_execute_public_antigravity_auth_stream_request, try_execute_public_gemini_auth_request,
    try_execute_public_gemini_auth_stream_request, try_execute_public_vertex_auth_request,
    try_execute_public_vertex_auth_stream_request,
};
use super::public_action_requests::{
    read_public_gemini_request_body, requested_public_gemini_model_candidates,
};
use super::*;

pub(in crate::server) async fn execute_public_gemini_model_action(
    state: Arc<BackendAppState>,
    action: GeminiPublicAction,
    request: Request<Body>,
) -> Response {
    let config_json = match load_current_config_json(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let snapshots = match FileAuthStore::new(&state.auth_dir).list_snapshots() {
        Ok(snapshots) => snapshots,
        Err(error) => return auth_snapshot_store_error_response(error),
    };
    if find_public_gemini_model(&config_json, &snapshots, &action.model).is_none() {
        return json_error(StatusCode::NOT_FOUND, "Not Found");
    }

    let config = match load_current_config(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let request_body = match read_public_gemini_request_body(request).await {
        Ok(body) => body,
        Err(response) => return response,
    };
    let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
    let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
    let mut last_error = None;

    if matches!(action.method, GeminiPublicPostMethod::StreamGenerateContent) {
        for candidate_model in requested_public_gemini_model_candidates(&config_json, &action.model)
        {
            match try_execute_public_gemini_auth_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                &candidate_model,
                &request_body,
            )
            .await
            {
                Ok(Some(response)) => return provider_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(gemini_public_runtime_error_response(error)),
            }

            match try_execute_public_antigravity_auth_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                &candidate_model,
                &request_body,
            )
            .await
            {
                Ok(Some(response)) => return provider_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(antigravity_public_error_response(error)),
            }

            match try_execute_public_vertex_auth_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                &candidate_model,
                &request_body,
            )
            .await
            {
                Ok(Some(response)) => return provider_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(gemini_public_runtime_error_response(error)),
            }
        }

        match try_execute_public_gemini_api_key_entries_stream_request(
            &config_json,
            &action.model,
            &request_body,
        )
        .await
        {
            Ok(Some(response)) => return provider_stream_response(response),
            Ok(None) => {}
            Err(error) => last_error = Some(gemini_public_provider_error_response(error)),
        }

        match try_execute_public_vertex_api_key_entries_stream_request(
            &config_json,
            &action.model,
            &request_body,
        )
        .await
        {
            Ok(Some(response)) => return provider_stream_response(response),
            Ok(None) => {}
            Err(error) => last_error = Some(gemini_public_provider_error_response(error)),
        }
    } else {
        for candidate_model in requested_public_gemini_model_candidates(&config_json, &action.model)
        {
            match try_execute_public_gemini_auth_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                &candidate_model,
                &request_body,
                action.method,
            )
            .await
            {
                Ok(Some(response)) => return provider_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(gemini_public_runtime_error_response(error)),
            }

            if matches!(action.method, GeminiPublicPostMethod::GenerateContent) {
                match try_execute_public_antigravity_auth_request(
                    state.clone(),
                    strategy,
                    default_proxy_url.clone(),
                    &candidate_model,
                    &request_body,
                )
                .await
                {
                    Ok(Some(response)) => return provider_http_response(response),
                    Ok(None) => {}
                    Err(error) => last_error = Some(antigravity_public_error_response(error)),
                }
            }

            match try_execute_public_vertex_auth_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                &candidate_model,
                &request_body,
                action.method,
            )
            .await
            {
                Ok(Some(response)) => return provider_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(gemini_public_runtime_error_response(error)),
            }
        }

        match try_execute_public_gemini_api_key_entries_request(
            &config_json,
            &action.model,
            &request_body,
            action.method,
        )
        .await
        {
            Ok(Some(response)) => return provider_http_response(response),
            Ok(None) => {}
            Err(error) => last_error = Some(gemini_public_provider_error_response(error)),
        }

        match try_execute_public_vertex_api_key_entries_request(
            &config_json,
            &action.model,
            &request_body,
            action.method,
        )
        .await
        {
            Ok(Some(response)) => return provider_http_response(response),
            Ok(None) => {}
            Err(error) => last_error = Some(gemini_public_provider_error_response(error)),
        }
    }

    last_error.unwrap_or_else(|| json_error(StatusCode::SERVICE_UNAVAILABLE, "No auth available"))
}

async fn try_execute_public_gemini_api_key_entries_request(
    config: &JsonValue,
    requested_public_model: &str,
    request: &GeminiPublicRequestBody,
    method: GeminiPublicPostMethod,
) -> Result<Option<ProviderHttpResponse>, GeminiPublicCallerError> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let entries = gemini_api_key_entries_from_config_json(config);
    let mut matched = false;
    let mut last_error = None;

    for entry in entries {
        let Some(upstream_model) =
            resolve_gemini_api_key_entry_model(&entry, requested_public_model, force_prefix)
        else {
            continue;
        };
        matched = true;
        let proxy_url = trim_optional_string(Some(entry.proxy_url.as_str()));
        let base_url = if entry.base_url.trim().is_empty() {
            DEFAULT_GEMINI_PUBLIC_BASE_URL.to_string()
        } else {
            entry.base_url.clone()
        };
        match execute_public_gemini_http_request(
            proxy_url.as_deref(),
            base_url.as_str(),
            method,
            &upstream_model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            entry.headers.as_ref(),
            Some(entry.api_key.as_str()),
            None,
        )
        .await
        {
            Ok(response) => return Ok(Some(response)),
            Err(error) => last_error = Some(error),
        }
    }

    if matched {
        Err(last_error.unwrap_or(GeminiPublicCallerError::UnsupportedAuthFile))
    } else {
        Ok(None)
    }
}

async fn try_execute_public_gemini_api_key_entries_stream_request(
    config: &JsonValue,
    requested_public_model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<Option<PendingProviderStream>, GeminiPublicCallerError> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let entries = gemini_api_key_entries_from_config_json(config);
    let mut matched = false;
    let mut last_error = None;

    for entry in entries {
        let Some(upstream_model) =
            resolve_gemini_api_key_entry_model(&entry, requested_public_model, force_prefix)
        else {
            continue;
        };
        matched = true;
        let proxy_url = trim_optional_string(Some(entry.proxy_url.as_str()));
        let base_url = if entry.base_url.trim().is_empty() {
            DEFAULT_GEMINI_PUBLIC_BASE_URL.to_string()
        } else {
            entry.base_url.clone()
        };
        match execute_public_gemini_http_stream_request(
            proxy_url.as_deref(),
            base_url.as_str(),
            &upstream_model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            entry.headers.as_ref(),
            Some(entry.api_key.as_str()),
            None,
        )
        .await
        {
            Ok(response) => return Ok(Some(response)),
            Err(error) => last_error = Some(error),
        }
    }

    if matched {
        Err(last_error.unwrap_or(GeminiPublicCallerError::UnsupportedAuthFile))
    } else {
        Ok(None)
    }
}

async fn try_execute_public_vertex_api_key_entries_request(
    config: &JsonValue,
    requested_public_model: &str,
    request: &GeminiPublicRequestBody,
    method: GeminiPublicPostMethod,
) -> Result<Option<ProviderHttpResponse>, GeminiPublicCallerError> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let entries = vertex_api_key_entries_from_config_json(config);
    let mut matched = false;
    let mut last_error = None;

    for entry in entries {
        let Some(upstream_model) =
            resolve_vertex_api_key_entry_model(&entry, requested_public_model, force_prefix)
        else {
            continue;
        };
        matched = true;
        let proxy_url = trim_optional_string(Some(entry.proxy_url.as_str()));
        let base_url = if entry.base_url.trim().is_empty() {
            DEFAULT_VERTEX_PUBLIC_BASE_URL.to_string()
        } else {
            entry.base_url.clone()
        };
        match execute_public_vertex_http_request(
            proxy_url.as_deref(),
            base_url.as_str(),
            method,
            &upstream_model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            entry.headers.as_ref(),
            entry.api_key.as_str(),
        )
        .await
        {
            Ok(response) => return Ok(Some(response)),
            Err(error) => last_error = Some(error),
        }
    }

    if matched {
        Err(last_error.unwrap_or(GeminiPublicCallerError::UnsupportedAuthFile))
    } else {
        Ok(None)
    }
}

async fn try_execute_public_vertex_api_key_entries_stream_request(
    config: &JsonValue,
    requested_public_model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<Option<PendingProviderStream>, GeminiPublicCallerError> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let entries = vertex_api_key_entries_from_config_json(config);
    let mut matched = false;
    let mut last_error = None;

    for entry in entries {
        let Some(upstream_model) =
            resolve_vertex_api_key_entry_model(&entry, requested_public_model, force_prefix)
        else {
            continue;
        };
        matched = true;
        let proxy_url = trim_optional_string(Some(entry.proxy_url.as_str()));
        let base_url = if entry.base_url.trim().is_empty() {
            DEFAULT_VERTEX_PUBLIC_BASE_URL.to_string()
        } else {
            entry.base_url.clone()
        };
        match execute_public_vertex_http_stream_request(
            proxy_url.as_deref(),
            base_url.as_str(),
            &upstream_model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            entry.headers.as_ref(),
            entry.api_key.as_str(),
        )
        .await
        {
            Ok(response) => return Ok(Some(response)),
            Err(error) => last_error = Some(error),
        }
    }

    if matched {
        Err(last_error.unwrap_or(GeminiPublicCallerError::UnsupportedAuthFile))
    } else {
        Ok(None)
    }
}
