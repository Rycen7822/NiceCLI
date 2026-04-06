use super::*;

pub(super) async fn handle_antigravity_callback(
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
        "antigravity",
        &state_value,
        &code,
        &callback_error,
    ) {
        Ok(_) => Html(OAUTH_CALLBACK_SUCCESS_HTML).into_response(),
        Err(error) => oauth_callback_file_page_response(error),
    }
}

pub(super) async fn get_antigravity_auth_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match state.antigravity_login_service.start_login() {
        Ok(started) => {
            let auth_dir = state.auth_dir.clone();
            let antigravity_login_service = state.antigravity_login_service.clone();
            let state_value = started.state.clone();
            tokio::spawn(async move {
                let _ = antigravity_login_service
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
            format!("failed to generate antigravity authentication url: {error}"),
        ),
    }
}
