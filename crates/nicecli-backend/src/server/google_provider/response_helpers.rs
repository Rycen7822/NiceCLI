use super::*;

pub(super) fn provider_stream_response(stream: PendingProviderStream) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(stream.status).unwrap_or(StatusCode::BAD_GATEWAY));
    if let Some(headers) = builder.headers_mut() {
        for (name, value) in &stream.headers {
            if name.as_str().eq_ignore_ascii_case("content-length") {
                continue;
            }
            headers.insert(name, value.clone());
        }
        if !headers.contains_key(CONTENT_TYPE) {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
        }
    }

    builder
        .body(Body::from_stream(stream.response.bytes_stream()))
        .unwrap_or_else(|_| json_error(StatusCode::BAD_GATEWAY, "Failed to build response"))
}

pub(super) fn gemini_internal_auth_failure(
    error: GeminiInternalCallerError,
) -> nicecli_runtime::ExecutionFailure<GeminiInternalCallerError> {
    nicecli_runtime::ExecutionFailure::retryable(
        error,
        nicecli_runtime::ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(nicecli_runtime::ExecutionError {
                message: "unauthorized".to_string(),
                http_status: Some(401),
            }),
        },
    )
}

pub(super) fn gemini_internal_request_failure(
    error: reqwest::Error,
) -> nicecli_runtime::ExecutionFailure<GeminiInternalCallerError> {
    let message = error.to_string();
    nicecli_runtime::ExecutionFailure::retryable(
        GeminiInternalCallerError::RefreshRequest(error),
        nicecli_runtime::ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(nicecli_runtime::ExecutionError {
                message,
                http_status: Some(503),
            }),
        },
    )
}

pub(super) fn gemini_internal_status_failure(
    status: u16,
    body: &[u8],
    model: &str,
) -> nicecli_runtime::ExecutionFailure<GeminiInternalCallerError> {
    let message = String::from_utf8_lossy(body).trim().to_string();
    let error = GeminiInternalCallerError::UnexpectedStatus {
        status,
        body: message.clone(),
    };
    let result = nicecli_runtime::ExecutionResult {
        model: Some(model.trim().to_string()),
        success: false,
        retry_after: None,
        error: Some(nicecli_runtime::ExecutionError {
            message,
            http_status: Some(status),
        }),
    };
    if matches!(status, 401 | 403 | 408 | 429 | 500 | 502 | 503 | 504) {
        nicecli_runtime::ExecutionFailure::retryable(error, result)
    } else {
        nicecli_runtime::ExecutionFailure::terminal(error, result)
    }
}

pub(super) fn gemini_internal_error_response(
    error: ExecuteWithRetryError<GeminiInternalCallerError>,
) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            GeminiInternalCallerError::UnexpectedStatus { status, body } => json_error(
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            ),
            GeminiInternalCallerError::MissingAccessToken
            | GeminiInternalCallerError::MissingProjectId
            | GeminiInternalCallerError::InvalidAuthFile(_)
            | GeminiInternalCallerError::ReadAuthFile(_) => {
                json_error(StatusCode::UNAUTHORIZED, error.to_string())
            }
            GeminiInternalCallerError::RefreshRequest(_)
            | GeminiInternalCallerError::RefreshFailed(_) => {
                json_error(StatusCode::BAD_GATEWAY, error.to_string())
            }
        },
        ExecuteWithRetryError::Runtime(error) => match error {
            RuntimeConductorError::Scheduler(SchedulerError::AuthNotFound)
            | RuntimeConductorError::Scheduler(SchedulerError::AuthUnavailable) => {
                json_error(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
            }
            RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
                json_error(StatusCode::BAD_REQUEST, "No provider supplied")
            }
            RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
                model,
                until,
                ..
            }) => json_error(
                StatusCode::TOO_MANY_REQUESTS,
                format!("model cooldown for {model} until {until}"),
            ),
            RuntimeConductorError::Store(error) => {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            RuntimeConductorError::SelectedAuthMissing(auth_id) => json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("selected auth is missing from the latest snapshot set: {auth_id}"),
            ),
        },
    }
}

pub(super) fn gemini_public_auth_failure(
    error: GeminiPublicCallerError,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    nicecli_runtime::ExecutionFailure::retryable(
        error,
        nicecli_runtime::ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(nicecli_runtime::ExecutionError {
                message: "unauthorized".to_string(),
                http_status: Some(401),
            }),
        },
    )
}

pub(super) fn gemini_public_auth_failure_from_internal(
    error: GeminiInternalCallerError,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    gemini_public_auth_failure(map_gemini_internal_error_to_public(error))
}

pub(super) fn gemini_public_request_failure(
    error: reqwest::Error,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    gemini_public_request_failure_for_model(error, "")
}

pub(super) fn gemini_public_request_failure_with_model<'a>(
    model: &'a str,
) -> impl Fn(GeminiPublicCallerError) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> + 'a
{
    move |error| gemini_public_execution_failure(error, model)
}

pub(super) fn gemini_public_request_failure_for_model(
    error: reqwest::Error,
    model: &str,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    let message = error.to_string();
    nicecli_runtime::ExecutionFailure::retryable(
        GeminiPublicCallerError::Request(error),
        nicecli_runtime::ExecutionResult {
            model: if model.trim().is_empty() {
                None
            } else {
                Some(model.trim().to_string())
            },
            success: false,
            retry_after: None,
            error: Some(nicecli_runtime::ExecutionError {
                message,
                http_status: Some(503),
            }),
        },
    )
}

pub(super) fn gemini_public_status_failure(
    status: u16,
    body: &[u8],
    model: &str,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    let message = String::from_utf8_lossy(body).trim().to_string();
    let error = GeminiPublicCallerError::UnexpectedStatus {
        status,
        body: message.clone(),
    };
    let result = nicecli_runtime::ExecutionResult {
        model: Some(model.trim().to_string()),
        success: false,
        retry_after: None,
        error: Some(nicecli_runtime::ExecutionError {
            message,
            http_status: Some(status),
        }),
    };
    if matches!(status, 401 | 403 | 408 | 429 | 500 | 502 | 503 | 504) {
        nicecli_runtime::ExecutionFailure::retryable(error, result)
    } else {
        nicecli_runtime::ExecutionFailure::terminal(error, result)
    }
}

pub(super) fn gemini_public_execution_failure(
    error: GeminiPublicCallerError,
    model: &str,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    match error {
        GeminiPublicCallerError::UnexpectedStatus { status, body } => {
            gemini_public_status_failure(status, body.as_bytes(), model)
        }
        GeminiPublicCallerError::Request(error) => {
            gemini_public_request_failure_for_model(error, model)
        }
        GeminiPublicCallerError::RefreshRequest(error) => {
            nicecli_runtime::ExecutionFailure::retryable(
                GeminiPublicCallerError::RefreshRequest(error),
                nicecli_runtime::ExecutionResult {
                    model: Some(model.trim().to_string()),
                    success: false,
                    retry_after: None,
                    error: Some(nicecli_runtime::ExecutionError {
                        message: "token refresh request failed".to_string(),
                        http_status: Some(503),
                    }),
                },
            )
        }
        GeminiPublicCallerError::RefreshFailed(message) => {
            nicecli_runtime::ExecutionFailure::retryable(
                GeminiPublicCallerError::RefreshFailed(message.clone()),
                nicecli_runtime::ExecutionResult {
                    model: Some(model.trim().to_string()),
                    success: false,
                    retry_after: None,
                    error: Some(nicecli_runtime::ExecutionError {
                        message,
                        http_status: Some(503),
                    }),
                },
            )
        }
        GeminiPublicCallerError::ReadAuthFile(_)
        | GeminiPublicCallerError::InvalidAuthFile(_)
        | GeminiPublicCallerError::MissingAccessToken
        | GeminiPublicCallerError::UnsupportedAuthFile => gemini_public_auth_failure(error),
    }
}

pub(super) fn gemini_public_failure_from_internal(
    failure: nicecli_runtime::ExecutionFailure<GeminiInternalCallerError>,
) -> nicecli_runtime::ExecutionFailure<GeminiPublicCallerError> {
    if failure.retryable {
        nicecli_runtime::ExecutionFailure::retryable(
            map_gemini_internal_error_to_public(failure.error),
            failure.result,
        )
    } else {
        nicecli_runtime::ExecutionFailure::terminal(
            map_gemini_internal_error_to_public(failure.error),
            failure.result,
        )
    }
}

pub(super) fn map_gemini_internal_error_to_public(
    error: GeminiInternalCallerError,
) -> GeminiPublicCallerError {
    match error {
        GeminiInternalCallerError::ReadAuthFile(error) => {
            GeminiPublicCallerError::ReadAuthFile(error)
        }
        GeminiInternalCallerError::InvalidAuthFile(message) => {
            GeminiPublicCallerError::InvalidAuthFile(message)
        }
        GeminiInternalCallerError::MissingAccessToken => {
            GeminiPublicCallerError::MissingAccessToken
        }
        GeminiInternalCallerError::MissingProjectId => GeminiPublicCallerError::UnsupportedAuthFile,
        GeminiInternalCallerError::RefreshRequest(error) => {
            GeminiPublicCallerError::RefreshRequest(error)
        }
        GeminiInternalCallerError::RefreshFailed(message) => {
            GeminiPublicCallerError::RefreshFailed(message)
        }
        GeminiInternalCallerError::UnexpectedStatus { status, body } => {
            GeminiPublicCallerError::UnexpectedStatus { status, body }
        }
    }
}

pub(super) fn gemini_public_runtime_error_response(
    error: ExecuteWithRetryError<GeminiPublicCallerError>,
) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => gemini_public_provider_error_response(error),
        ExecuteWithRetryError::Runtime(error) => match error {
            RuntimeConductorError::Scheduler(SchedulerError::AuthNotFound)
            | RuntimeConductorError::Scheduler(SchedulerError::AuthUnavailable) => {
                json_error(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
            }
            RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
                json_error(StatusCode::BAD_REQUEST, "No provider supplied")
            }
            RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
                model,
                until,
                ..
            }) => json_error(
                StatusCode::TOO_MANY_REQUESTS,
                format!("model cooldown for {model} until {until}"),
            ),
            RuntimeConductorError::Store(error) => {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            RuntimeConductorError::SelectedAuthMissing(auth_id) => json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("selected auth is missing from the latest snapshot set: {auth_id}"),
            ),
        },
    }
}

pub(super) fn gemini_public_provider_error_response(error: GeminiPublicCallerError) -> Response {
    match error {
        GeminiPublicCallerError::UnexpectedStatus { status, body } => upstream_json_error_response(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            &body,
        ),
        GeminiPublicCallerError::MissingAccessToken
        | GeminiPublicCallerError::InvalidAuthFile(_)
        | GeminiPublicCallerError::ReadAuthFile(_)
        | GeminiPublicCallerError::UnsupportedAuthFile => {
            json_error(StatusCode::UNAUTHORIZED, error.to_string())
        }
        GeminiPublicCallerError::RefreshRequest(_)
        | GeminiPublicCallerError::RefreshFailed(_)
        | GeminiPublicCallerError::Request(_) => {
            json_error(StatusCode::BAD_GATEWAY, error.to_string())
        }
    }
}

pub(super) fn antigravity_public_error_response(
    error: ExecuteWithRetryError<AntigravityCallerError>,
) -> Response {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            AntigravityCallerError::UnexpectedStatus { status, body }
            | AntigravityCallerError::RefreshRejected { status, body } => {
                upstream_json_error_response(
                    StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                    &body,
                )
            }
            AntigravityCallerError::MissingAccessToken
            | AntigravityCallerError::InvalidAuthFile(_)
            | AntigravityCallerError::ReadAuthFile(_)
            | AntigravityCallerError::WriteAuthFile(_) => {
                json_error(StatusCode::UNAUTHORIZED, error.to_string())
            }
            AntigravityCallerError::Request(_) => {
                json_error(StatusCode::BAD_GATEWAY, error.to_string())
            }
        },
        ExecuteWithRetryError::Runtime(error) => match error {
            RuntimeConductorError::Scheduler(SchedulerError::AuthNotFound)
            | RuntimeConductorError::Scheduler(SchedulerError::AuthUnavailable) => {
                json_error(StatusCode::SERVICE_UNAVAILABLE, "No auth available")
            }
            RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
                json_error(StatusCode::BAD_REQUEST, "No provider supplied")
            }
            RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
                model,
                until,
                ..
            }) => json_error(
                StatusCode::TOO_MANY_REQUESTS,
                format!("model cooldown for {model} until {until}"),
            ),
            RuntimeConductorError::Store(error) => {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            RuntimeConductorError::SelectedAuthMissing(auth_id) => json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("selected auth is missing from the latest snapshot set: {auth_id}"),
            ),
        },
    }
}

pub(super) fn upstream_json_error_response(status: StatusCode, body: &str) -> Response {
    let trimmed = body.trim();
    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) {
            return (status, Json(value)).into_response();
        }
    }
    json_error(
        status,
        if trimmed.is_empty() {
            status.canonical_reason().unwrap_or("error").to_string()
        } else {
            trimmed.to_string()
        },
    )
}
