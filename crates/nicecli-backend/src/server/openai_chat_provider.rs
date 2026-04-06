use super::*;
use nicecli_runtime::{
    ExecuteWithRetryError, KimiCallerError, KimiChatCaller, KimiChatCompletionsRequest,
    QwenCallerError, QwenChatCaller, QwenChatCompletionsRequest,
};

mod completions;
mod model_candidates;
mod responses;
mod runtime_bridge;

use self::completions::*;
use self::model_candidates::*;
use self::responses::*;
use self::runtime_bridge::*;

pub(super) async fn execute_public_openai_chat_request(
    state: Arc<BackendAppState>,
    headers: HeaderMap,
    query: PublicApiAuthQuery,
    request: Request<Body>,
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
        Err(_) => return openai_error_response(StatusCode::BAD_REQUEST, "Invalid request"),
    };
    let parsed_json = serde_json::from_slice::<JsonValue>(&raw_body).ok();
    let stream_requested = parsed_json
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
        return openai_error_response(StatusCode::BAD_REQUEST, "Model is required");
    }

    if find_public_openai_chat_model(&config_json, &snapshots, &requested_model).is_none() {
        return openai_error_response(StatusCode::NOT_FOUND, "Model not found");
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
        requested_public_openai_chat_model_candidates(&config_json, &snapshots, &requested_model)
    {
        let upstream_body = patch_public_openai_chat_request_body(
            &raw_body,
            parsed_json.as_ref(),
            candidate_model.as_str(),
        );

        if stream_requested {
            match try_execute_public_qwen_chat_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return provider_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_qwen_chat_error_response(error)),
            }

            match try_execute_public_kimi_chat_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body,
            )
            .await
            {
                Ok(Some(response)) => return provider_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_kimi_chat_error_response(error)),
            }
        } else {
            match try_execute_public_qwen_chat_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return provider_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_qwen_chat_error_response(error)),
            }

            match try_execute_public_kimi_chat_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body,
            )
            .await
            {
                Ok(Some(response)) => return provider_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_kimi_chat_error_response(error)),
            }
        }
    }

    last_error.unwrap_or_else(|| {
        openai_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
    })
}

pub(super) async fn execute_public_openai_completions_request(
    state: Arc<BackendAppState>,
    headers: HeaderMap,
    query: PublicApiAuthQuery,
    request: Request<Body>,
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
        Err(_) => return openai_error_response(StatusCode::BAD_REQUEST, "Invalid request"),
    };
    let parsed_json = serde_json::from_slice::<JsonValue>(&raw_body).ok();
    let stream_requested = parsed_json
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
        return openai_error_response(StatusCode::BAD_REQUEST, "Model is required");
    }

    if find_public_openai_chat_model(&config_json, &snapshots, &requested_model).is_none() {
        return openai_error_response(StatusCode::NOT_FOUND, "Model not found");
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
        requested_public_openai_chat_model_candidates(&config_json, &snapshots, &requested_model)
    {
        let upstream_body = patch_public_openai_completions_request_body(
            &raw_body,
            parsed_json.as_ref(),
            candidate_model.as_str(),
        );

        if stream_requested {
            match try_execute_public_qwen_chat_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return public_openai_completions_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_qwen_chat_error_response(error)),
            }

            match try_execute_public_kimi_chat_stream_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body,
            )
            .await
            {
                Ok(Some(response)) => return public_openai_completions_stream_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_kimi_chat_error_response(error)),
            }
        } else {
            match try_execute_public_qwen_chat_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body.clone(),
            )
            .await
            {
                Ok(Some(response)) => return public_openai_completions_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_qwen_chat_error_response(error)),
            }

            match try_execute_public_kimi_chat_request(
                state.clone(),
                strategy,
                default_proxy_url.clone(),
                user_agent.as_str(),
                candidate_model.as_str(),
                upstream_body,
            )
            .await
            {
                Ok(Some(response)) => return public_openai_completions_http_response(response),
                Ok(None) => {}
                Err(error) => last_error = Some(public_kimi_chat_error_response(error)),
            }
        }
    }

    last_error.unwrap_or_else(|| {
        openai_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
    })
}

fn patch_public_openai_chat_request_body(
    raw_body: &[u8],
    parsed_json: Option<&JsonValue>,
    model: &str,
) -> Vec<u8> {
    let Some(JsonValue::Object(object)) = parsed_json else {
        return raw_body.to_vec();
    };
    let mut next = object.clone();
    next.insert("model".to_string(), json!(model.trim()));
    serde_json::to_vec(&JsonValue::Object(next)).unwrap_or_else(|_| raw_body.to_vec())
}
