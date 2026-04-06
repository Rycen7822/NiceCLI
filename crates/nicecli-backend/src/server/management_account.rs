use super::{
    get_anthropic_auth_url, get_antigravity_auth_url, get_codex_auth_url,
    get_codex_quota_snapshots, get_gemini_cli_auth_url, get_kimi_auth_url, get_qwen_auth_url,
    import_vertex_credential, refresh_codex_quota_snapshots, save_gemini_web_tokens,
    BackendAppState,
};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

pub(super) fn route_management_account_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route(
            "/v0/management/codex/quota-snapshots",
            get(get_codex_quota_snapshots),
        )
        .route(
            "/v0/management/codex/quota-snapshots/refresh",
            post(refresh_codex_quota_snapshots),
        )
        .route(
            "/v0/management/antigravity-auth-url",
            get(get_antigravity_auth_url),
        )
        .route(
            "/v0/management/anthropic-auth-url",
            get(get_anthropic_auth_url),
        )
        .route("/v0/management/codex-auth-url", get(get_codex_auth_url))
        .route(
            "/v0/management/gemini-cli-auth-url",
            get(get_gemini_cli_auth_url),
        )
        .route(
            "/v0/management/gemini-web-token",
            post(save_gemini_web_tokens),
        )
        .route("/v0/management/kimi-auth-url", get(get_kimi_auth_url))
        .route("/v0/management/qwen-auth-url", get(get_qwen_auth_url))
        .route(
            "/v0/management/vertex/import",
            post(import_vertex_credential),
        )
}
