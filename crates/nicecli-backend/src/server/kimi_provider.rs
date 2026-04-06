use super::*;

pub(super) async fn get_kimi_auth_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match state.kimi_login_service.start_login().await {
        Ok(started) => {
            let auth_dir = state.auth_dir.clone();
            let kimi_login_service = state.kimi_login_service.clone();
            let state_value = started.state.clone();
            tokio::spawn(async move {
                let _ = kimi_login_service
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
            format!("failed to generate kimi authentication url: {error}"),
        ),
    }
}
