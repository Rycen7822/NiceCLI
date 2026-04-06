use super::*;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use std::path::Path;

pub(super) async fn send_antigravity_request(
    client: &Client,
    auth: &AntigravityAuthState,
    default_user_agent: &str,
    body: &[u8],
    path: &str,
    accept: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let url = format!("{}{}", auth.base_url.trim_end_matches('/'), path);
    client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, accept)
        .header(
            USER_AGENT,
            auth.user_agent
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if default_user_agent.trim().is_empty() {
                        DEFAULT_ANTIGRAVITY_USER_AGENT
                    } else {
                        default_user_agent.trim()
                    }
                }),
        )
        .header(
            AUTHORIZATION,
            format!(
                "Bearer {}",
                auth.access_token.as_deref().unwrap_or_default().trim()
            ),
        )
        .body(body.to_vec())
        .send()
        .await
}

pub(super) async fn refresh_antigravity_auth(
    auth_dir: &Path,
    auth_file_name: &str,
    mut auth: AntigravityAuthState,
    default_proxy_url: Option<&str>,
    endpoints: &AntigravityCallerEndpoints,
    now: DateTime<Utc>,
) -> Result<AntigravityAuthState, AntigravityCallerError> {
    let refresh_token = auth
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AntigravityCallerError::MissingAccessToken)?;
    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))?;
    let response = client
        .post(&endpoints.token_url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, DEFAULT_ANTIGRAVITY_REFRESH_USER_AGENT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", endpoints.client_id.as_str()),
            ("client_secret", endpoints.client_secret.as_str()),
        ])
        .send()
        .await?;

    let status = response.status().as_u16();
    let body = response.bytes().await?.to_vec();
    if !(200..300).contains(&status) {
        return Err(AntigravityCallerError::RefreshRejected {
            status,
            body: response_body_message(&body),
        });
    }

    let refreshed = parse_refresh_token_response(&body, now)?;
    apply_refreshed_auth_state(&mut auth, &refreshed, now);
    persist_antigravity_auth_state(auth_dir, auth_file_name, &auth)?;
    Ok(auth)
}

pub(super) fn antigravity_auth_failure(
    error: AntigravityCallerError,
) -> ExecutionFailure<AntigravityCallerError> {
    ExecutionFailure::retryable(error, unauthorized_result())
}

pub(super) fn antigravity_local_failure(
    error: AntigravityCallerError,
) -> ExecutionFailure<AntigravityCallerError> {
    ExecutionFailure::retryable(error, unauthorized_result())
}

pub(super) fn antigravity_request_failure(
    error: reqwest::Error,
) -> ExecutionFailure<AntigravityCallerError> {
    let message = error.to_string();
    ExecutionFailure::retryable(
        AntigravityCallerError::Request(error),
        ExecutionResult {
            model: None,
            success: false,
            retry_after: None,
            error: Some(ExecutionError {
                message,
                http_status: Some(503),
            }),
        },
    )
}

pub(super) fn normalized_model(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn unauthorized_result() -> ExecutionResult {
    ExecutionResult {
        model: None,
        success: false,
        retry_after: None,
        error: Some(ExecutionError {
            message: "unauthorized".to_string(),
            http_status: Some(401),
        }),
    }
}
