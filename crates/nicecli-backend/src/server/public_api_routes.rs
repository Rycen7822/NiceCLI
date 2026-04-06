use super::{
    execute_public_claude_count_tokens_request, execute_public_claude_messages_request,
    execute_public_codex_request, execute_public_openai_chat_request,
    execute_public_openai_completions_request, get_public_codex_responses_websocket,
    BackendAppState, PublicApiAuthQuery,
};
use axum::body::Body;
use axum::extract::{ws::WebSocketUpgrade, Query, State};
use axum::http::{HeaderMap, Request};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

pub(super) fn route_public_api_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route("/v1/chat/completions", post(post_v1_chat_completions))
        .route("/v1/completions", post(post_v1_completions))
        .route("/v1/messages", post(post_v1_messages))
        .route(
            "/v1/messages/count_tokens",
            post(post_v1_messages_count_tokens),
        )
        .route(
            "/v1/responses",
            get(get_v1_responses).post(post_v1_responses),
        )
        .route("/v1/responses/compact", post(post_v1_responses_compact))
}

async fn post_v1_responses(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    request: Request<Body>,
) -> Response {
    execute_public_codex_request(state, headers, query, request, false).await
}

async fn get_v1_responses(
    state: State<Arc<BackendAppState>>,
    headers: HeaderMap,
    query: Query<PublicApiAuthQuery>,
    websocket: WebSocketUpgrade,
) -> Response {
    get_public_codex_responses_websocket(state, headers, query, websocket).await
}

async fn post_v1_chat_completions(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    request: Request<Body>,
) -> Response {
    execute_public_openai_chat_request(state, headers, query, request).await
}

async fn post_v1_completions(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    request: Request<Body>,
) -> Response {
    execute_public_openai_completions_request(state, headers, query, request).await
}

async fn post_v1_messages(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    request: Request<Body>,
) -> Response {
    execute_public_claude_messages_request(state, headers, query, request).await
}

async fn post_v1_messages_count_tokens(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    request: Request<Body>,
) -> Response {
    execute_public_claude_count_tokens_request(state, headers, query, request).await
}

async fn post_v1_responses_compact(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<PublicApiAuthQuery>,
    request: Request<Body>,
) -> Response {
    execute_public_codex_request(state, headers, query, request, true).await
}
