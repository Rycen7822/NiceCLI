use super::model_catalog::{collect_public_codex_models, collect_public_openai_models};
use super::{
    auth_snapshot_store_error_response, collect_public_gemini_models, config_error_response,
    ensure_public_api_key, execute_public_gemini_model_action, find_public_gemini_model,
    json_error, load_current_config_json, maybe_public_claude_models_response,
    parse_gemini_public_action, BackendAppState, PublicApiAuthQuery,
};
use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use nicecli_runtime::{AuthSnapshot, AuthStore, FileAuthStore};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

pub(super) fn route_public_model_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route("/v1/models", get(get_v1_models))
        .route("/v1beta/models", get(get_v1beta_models))
        .route(
            "/v1beta/models/*action",
            get(get_v1beta_model).post(post_v1beta_model_action),
        )
}

async fn get_v1_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let (config, snapshots) = match load_public_model_context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };

    if let Some(response) = maybe_public_codex_models_response(&query, &config, &snapshots) {
        return response;
    }

    if let Some(response) = maybe_public_claude_models_response(&headers, &config, &snapshots) {
        return response;
    }

    Json(json!({
        "object": "list",
        "data": collect_public_openai_models(&config, &snapshots),
    }))
    .into_response()
}

async fn get_v1beta_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let (config, snapshots) = match load_public_model_context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };

    Json(json!({
        "models": collect_public_gemini_models(&config, &snapshots),
    }))
    .into_response()
}

async fn get_v1beta_model(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    AxumPath(action): AxumPath<String>,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let (config, snapshots) = match load_public_model_context(&state) {
        Ok(context) => context,
        Err(response) => return response,
    };

    let action = action.trim().trim_start_matches('/');
    if action.is_empty() {
        return json_error(StatusCode::NOT_FOUND, "Not Found");
    }

    let target = action
        .strip_prefix("models/")
        .unwrap_or(action)
        .trim()
        .to_ascii_lowercase();
    if let Some(model) = find_public_gemini_model(&config, &snapshots, &target) {
        return Json(model).into_response();
    }

    json_error(StatusCode::NOT_FOUND, "Not Found")
}

async fn post_v1beta_model_action(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    AxumPath(action): AxumPath<String>,
    request: Request<Body>,
) -> Response {
    if let Err(response) = ensure_public_api_key(&headers, &query, &state) {
        return response;
    }

    let Some(action) = parse_gemini_public_action(&action) else {
        return json_error(StatusCode::NOT_FOUND, "Not Found");
    };

    execute_public_gemini_model_action(state, action, request).await
}

fn load_public_model_context(
    state: &Arc<BackendAppState>,
) -> Result<(JsonValue, Vec<AuthSnapshot>), Response> {
    let config = load_current_config_json(state).map_err(config_error_response)?;
    let snapshots = FileAuthStore::new(&state.auth_dir)
        .list_snapshots()
        .map_err(auth_snapshot_store_error_response)?;
    Ok((config, snapshots))
}

fn maybe_public_codex_models_response(
    query: &PublicApiAuthQuery,
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Option<Response> {
    let client_version = query
        .client_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let _ = client_version;
    Some(
        Json(json!({
            "models": collect_public_codex_models(config, snapshots),
        }))
        .into_response(),
    )
}
