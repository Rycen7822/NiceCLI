use super::*;

pub(super) fn public_qwen_chat_error_response(
    error: ExecuteWithRetryError<QwenCallerError>,
) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            QwenCallerError::UnexpectedStatus { status, body } => openai_error_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
            QwenCallerError::MissingAccessToken => {
                openai_error_response(StatusCode::UNAUTHORIZED, "Missing access token")
            }
            QwenCallerError::InvalidAuthFile(message) => {
                openai_error_response(StatusCode::UNAUTHORIZED, message)
            }
            QwenCallerError::ReadAuthFile(error) | QwenCallerError::WriteAuthFile(error) => {
                openai_error_response(StatusCode::UNAUTHORIZED, error.to_string())
            }
            QwenCallerError::Request(error) => {
                openai_error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
            }
            QwenCallerError::RefreshRejected { status, body } => openai_error_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
        },
        ExecuteWithRetryError::Runtime(error) => public_openai_chat_runtime_error_response(error),
    }
}

pub(super) fn public_kimi_chat_error_response(
    error: ExecuteWithRetryError<KimiCallerError>,
) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            KimiCallerError::UnexpectedStatus { status, body } => openai_error_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
            KimiCallerError::MissingAccessToken => {
                openai_error_response(StatusCode::UNAUTHORIZED, "Missing access token")
            }
            KimiCallerError::InvalidAuthFile(message) => {
                openai_error_response(StatusCode::UNAUTHORIZED, message)
            }
            KimiCallerError::ReadAuthFile(error) | KimiCallerError::WriteAuthFile(error) => {
                openai_error_response(StatusCode::UNAUTHORIZED, error.to_string())
            }
            KimiCallerError::Request(error) => {
                openai_error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
            }
            KimiCallerError::RefreshRejected { status, body } => openai_error_response(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
        },
        ExecuteWithRetryError::Runtime(error) => public_openai_chat_runtime_error_response(error),
    }
}

fn public_openai_chat_runtime_error_response(error: RuntimeConductorError) -> Response {
    match error {
        RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
            model, until, ..
        }) => openai_error_response(
            StatusCode::TOO_MANY_REQUESTS,
            format!("model cooldown for {model} until {until}"),
        ),
        RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ) => openai_error_response(StatusCode::SERVICE_UNAVAILABLE, "No auth available"),
        RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
            openai_error_response(StatusCode::BAD_REQUEST, "No provider supplied")
        }
        RuntimeConductorError::Store(error) => {
            openai_error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
        RuntimeConductorError::SelectedAuthMissing(auth_id) => openai_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("selected auth is missing from the latest snapshot set: {auth_id}"),
        ),
    }
}

pub(super) fn provider_stream_response(response: reqwest::Response) -> Response {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = response.headers().clone();
    let mut builder = Response::builder().status(status);
    if let Some(next_headers) = builder.headers_mut() {
        for (name, value) in &headers {
            if name.as_str().eq_ignore_ascii_case("content-length")
                || name.as_str().eq_ignore_ascii_case("transfer-encoding")
                || name.as_str().eq_ignore_ascii_case("connection")
            {
                continue;
            }
            next_headers.insert(name.clone(), value.clone());
        }
        if !next_headers.contains_key(CONTENT_TYPE) {
            next_headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
        }
    }

    builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap_or_else(|_| {
            openai_error_response(StatusCode::BAD_GATEWAY, "Failed to build response")
        })
}
