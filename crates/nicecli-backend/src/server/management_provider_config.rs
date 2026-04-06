use super::model_catalog::{
    load_oauth_excluded_models_map, load_oauth_model_alias_map, normalize_excluded_models,
    normalize_oauth_excluded_models_map, normalize_oauth_excluded_models_value,
    normalize_oauth_model_alias_entries, normalize_oauth_model_alias_map,
    normalize_oauth_model_alias_value,
};
use super::{
    config_error_response, config_json_value, delete_claude_api_keys, delete_codex_api_keys,
    delete_gemini_api_keys, delete_openai_compatibility, delete_vertex_api_keys,
    ensure_management_key, get_claude_api_keys, get_codex_api_keys, get_gemini_api_keys,
    get_openai_compatibility, get_vertex_api_keys, json_error_response, load_current_config_json,
    patch_claude_api_keys, patch_codex_api_keys, patch_gemini_api_keys, patch_openai_compatibility,
    patch_vertex_api_keys, persist_top_level_config_value, put_claude_api_keys, put_codex_api_keys,
    put_gemini_api_keys, put_openai_compatibility, put_vertex_api_keys, BackendAppState,
    OAuthExcludedModelsPatchRequest, OAuthModelAliasEntry, OAuthModelAliasPatchRequest,
    ProviderQuery,
};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

pub(super) fn route_management_provider_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route(
            "/v0/management/gemini-api-key",
            get(get_gemini_api_keys)
                .put(put_gemini_api_keys)
                .patch(patch_gemini_api_keys)
                .delete(delete_gemini_api_keys),
        )
        .route(
            "/v0/management/claude-api-key",
            get(get_claude_api_keys)
                .put(put_claude_api_keys)
                .patch(patch_claude_api_keys)
                .delete(delete_claude_api_keys),
        )
        .route(
            "/v0/management/codex-api-key",
            get(get_codex_api_keys)
                .put(put_codex_api_keys)
                .patch(patch_codex_api_keys)
                .delete(delete_codex_api_keys),
        )
        .route(
            "/v0/management/openai-compatibility",
            get(get_openai_compatibility)
                .put(put_openai_compatibility)
                .patch(patch_openai_compatibility)
                .delete(delete_openai_compatibility),
        )
        .route(
            "/v0/management/vertex-api-key",
            get(get_vertex_api_keys)
                .put(put_vertex_api_keys)
                .patch(patch_vertex_api_keys)
                .delete(delete_vertex_api_keys),
        )
        .route(
            "/v0/management/oauth-excluded-models",
            get(get_oauth_excluded_models)
                .put(put_oauth_excluded_models)
                .patch(patch_oauth_excluded_models)
                .delete(delete_oauth_excluded_models),
        )
        .route(
            "/v0/management/oauth-model-alias",
            get(get_oauth_model_alias)
                .put(put_oauth_model_alias)
                .patch(patch_oauth_model_alias)
                .delete(delete_oauth_model_alias),
        )
}

pub(super) async fn get_oauth_excluded_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_current_config_json(&state) {
        Ok(config) => super::single_field_json_response(
            "oauth-excluded-models",
            normalize_oauth_excluded_models_value(config_json_value(
                &config,
                "oauth-excluded-models",
            )),
        ),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_oauth_excluded_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: std::collections::HashMap<String, Vec<String>> =
        match super::parse_json_or_items_wrapper(&body) {
            Ok(parsed) => parsed,
            Err(_) => {
                return json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid body",
                    "invalid body",
                );
            }
        };

    match persist_top_level_config_value(
        &state,
        "oauth-excluded-models",
        normalize_oauth_excluded_models_map(parsed),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn patch_oauth_excluded_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<OAuthExcludedModelsPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(provider) = request
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
    else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    let normalized_models = normalize_excluded_models(request.models);
    let mut current = match load_oauth_excluded_models_map(&state) {
        Ok(current) => current,
        Err(error) => return config_error_response(error),
    };

    if normalized_models.is_empty() {
        if current.remove(&provider).is_none() {
            return json_error_response(
                StatusCode::NOT_FOUND,
                "provider not found",
                "provider not found",
            );
        }
    } else {
        current.insert(provider, normalized_models);
    }

    match persist_top_level_config_value(
        &state,
        "oauth-excluded-models",
        normalize_oauth_excluded_models_map(current),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn delete_oauth_excluded_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<ProviderQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(provider) = query
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
    else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing provider",
            "missing provider",
        );
    };

    let mut current = match load_oauth_excluded_models_map(&state) {
        Ok(current) => current,
        Err(error) => return config_error_response(error),
    };
    if current.remove(&provider).is_none() {
        return json_error_response(
            StatusCode::NOT_FOUND,
            "provider not found",
            "provider not found",
        );
    }

    match persist_top_level_config_value(
        &state,
        "oauth-excluded-models",
        normalize_oauth_excluded_models_map(current),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn get_oauth_model_alias(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_current_config_json(&state) {
        Ok(config) => super::single_field_json_response(
            "oauth-model-alias",
            normalize_oauth_model_alias_value(config_json_value(&config, "oauth-model-alias")),
        ),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_oauth_model_alias(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: std::collections::HashMap<String, Vec<OAuthModelAliasEntry>> =
        match super::parse_json_or_items_wrapper(&body) {
            Ok(parsed) => parsed,
            Err(_) => {
                return json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid body",
                    "invalid body",
                );
            }
        };

    match persist_top_level_config_value(
        &state,
        "oauth-model-alias",
        normalize_oauth_model_alias_map(parsed),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn patch_oauth_model_alias(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<OAuthModelAliasPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let raw_channel = request
        .channel
        .as_deref()
        .or(request.provider.as_deref())
        .map(str::trim)
        .unwrap_or_default();
    if raw_channel.is_empty() {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "invalid channel",
            "invalid channel",
        );
    }
    let channel = raw_channel.to_ascii_lowercase();

    let normalized_aliases = normalize_oauth_model_alias_entries(request.aliases);
    let mut current = match load_oauth_model_alias_map(&state) {
        Ok(current) => current,
        Err(error) => return config_error_response(error),
    };

    if normalized_aliases.is_empty() {
        if current.remove(&channel).is_none() {
            return json_error_response(
                StatusCode::NOT_FOUND,
                "channel not found",
                "channel not found",
            );
        }
    } else {
        current.insert(channel, normalized_aliases);
    }

    match persist_top_level_config_value(
        &state,
        "oauth-model-alias",
        normalize_oauth_model_alias_map(current),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn delete_oauth_model_alias(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<ProviderQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(channel) = query
        .channel
        .as_deref()
        .or(query.provider.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
    else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing channel",
            "missing channel",
        );
    };

    let mut current = match load_oauth_model_alias_map(&state) {
        Ok(current) => current,
        Err(error) => return config_error_response(error),
    };
    if current.remove(&channel).is_none() {
        return json_error_response(
            StatusCode::NOT_FOUND,
            "channel not found",
            "channel not found",
        );
    }

    match persist_top_level_config_value(
        &state,
        "oauth-model-alias",
        normalize_oauth_model_alias_map(current),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}
