use super::{
    ensure_management_key, handle_anthropic_callback, handle_antigravity_callback,
    handle_codex_callback, handle_google_callback, handle_v1internal_method, BackendAppState,
};
use axum::body::Body;
use axum::extract::{ConnectInfo, Path as AxumPath, State};
use axum::http::{HeaderMap, Request};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

pub(super) fn route_entry_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route("/", get(get_root))
        .route("/anthropic/callback", get(handle_anthropic_callback))
        .route("/antigravity/callback", get(handle_antigravity_callback))
        .route("/codex/callback", get(handle_codex_callback))
        .route("/google/callback", get(handle_google_callback))
        .route("/keep-alive", get(handle_keep_alive))
        .route("/v1internal:method", post(post_v1internal_method))
}

async fn handle_keep_alive(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    match ensure_management_key(&headers, &state) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(response) => response,
    }
}

async fn post_v1internal_method(
    State(state): State<Arc<BackendAppState>>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    AxumPath(method): AxumPath<String>,
    request: Request<Body>,
) -> Response {
    handle_v1internal_method(state, connect_info, method, request, None).await
}

async fn get_root() -> Response {
    Json(json!({
        "message": "CLI Proxy API Server",
        "endpoints": [
            "POST /v1/chat/completions",
            "POST /v1/completions",
            "GET /v1/models"
        ]
    }))
    .into_response()
}
