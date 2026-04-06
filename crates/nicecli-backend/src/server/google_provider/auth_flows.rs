use super::*;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(in crate::server) struct GeminiCliAuthUrlQuery {
    project_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(in crate::server) struct GeminiWebTokenRequest {
    secure_1psid: String,
    secure_1psidts: String,
    label: Option<String>,
    email: Option<String>,
}

pub(in crate::server) async fn handle_google_callback(
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
        "gemini",
        &state_value,
        &code,
        &callback_error,
    ) {
        Ok(_) => Html(OAUTH_CALLBACK_SUCCESS_HTML).into_response(),
        Err(error) => oauth_callback_file_page_response(error),
    }
}

pub(in crate::server) async fn get_gemini_cli_auth_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<GeminiCliAuthUrlQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match state
        .gemini_cli_login_service
        .start_login(query.project_id.as_deref())
    {
        Ok(started) => {
            let auth_dir = state.auth_dir.clone();
            let gemini_cli_login_service = state.gemini_cli_login_service.clone();
            let state_value = started.state.clone();
            tokio::spawn(async move {
                let _ = gemini_cli_login_service
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
            format!("failed to generate gemini authentication url: {error}"),
        ),
    }
}

pub(in crate::server) async fn save_gemini_web_tokens(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<GeminiWebTokenRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let email = request
        .email
        .as_deref()
        .or(request.label.as_deref())
        .unwrap_or_default();

    match save_gemini_web_tokens_to_store(
        &state.auth_dir,
        email,
        &request.secure_1psid,
        &request.secure_1psidts,
    ) {
        Ok(saved) => Json(json!({
            "status": "ok",
            "file": saved.file_name,
            "email": saved.email,
        }))
        .into_response(),
        Err(error) => gemini_web_token_error_response(error),
    }
}

pub(in crate::server) async fn import_vertex_credential(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut file_bytes = None;
    let mut location = None;

    loop {
        let next_field = match multipart.next_field().await {
            Ok(field) => field,
            Err(error) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("invalid multipart form: {error}"),
                );
            }
        };

        let Some(field) = next_field else {
            break;
        };

        match field.name() {
            Some("location") => match field.text().await {
                Ok(value) => location = Some(value),
                Err(error) => {
                    return json_error(
                        StatusCode::BAD_REQUEST,
                        format!("failed to read location field: {error}"),
                    );
                }
            },
            Some("file") => match field.bytes().await {
                Ok(value) => file_bytes = Some(value.to_vec()),
                Err(error) => {
                    return json_error(
                        StatusCode::BAD_REQUEST,
                        format!("failed to read uploaded file: {error}"),
                    );
                }
            },
            _ => {}
        }
    }

    let Some(file_bytes) = file_bytes else {
        return json_error(StatusCode::BAD_REQUEST, "file required");
    };

    match import_vertex_credential_to_store(&state.auth_dir, &file_bytes, location.as_deref()) {
        Ok(imported) => Json(json!({
            "status": "ok",
            "auth-file": imported.file_path.display().to_string(),
            "file": imported.file_name,
            "project_id": imported.project_id,
            "email": imported.email,
            "location": imported.location,
        }))
        .into_response(),
        Err(error) => vertex_credential_import_error_response(error),
    }
}

fn gemini_web_token_error_response(error: GeminiWebTokenError) -> Response {
    let status = match error {
        GeminiWebTokenError::MissingEmail
        | GeminiWebTokenError::MissingSecure1Psid
        | GeminiWebTokenError::MissingSecure1PsidTs => StatusCode::BAD_REQUEST,
        GeminiWebTokenError::Encode(_) | GeminiWebTokenError::Write(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    json_error(status, error.to_string())
}

fn vertex_credential_import_error_response(error: VertexCredentialImportError) -> Response {
    let status = match error {
        VertexCredentialImportError::InvalidJson(_)
        | VertexCredentialImportError::InvalidRoot
        | VertexCredentialImportError::MissingPrivateKey
        | VertexCredentialImportError::MissingProjectId => StatusCode::BAD_REQUEST,
        VertexCredentialImportError::Encode(_) | VertexCredentialImportError::Write(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    json_error(status, error.to_string())
}
