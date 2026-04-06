use super::*;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use std::path::Path;

pub(super) async fn send_qwen_request(
    client: &Client,
    auth: &QwenAuthState,
    user_agent: &str,
    body: &[u8],
    accept: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let url = format!(
        "{}{}",
        auth.base_url.trim_end_matches('/'),
        QWEN_CHAT_COMPLETIONS_PATH
    );
    client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, accept)
        .header(
            USER_AGENT,
            if user_agent.trim().is_empty() {
                DEFAULT_QWEN_USER_AGENT
            } else {
                user_agent.trim()
            },
        )
        .header(
            AUTHORIZATION,
            format!(
                "Bearer {}",
                auth.access_token.as_deref().unwrap_or_default().trim()
            ),
        )
        .header("X-Dashscope-Useragent", DEFAULT_QWEN_USER_AGENT)
        .header("X-Stainless-Runtime-Version", "v22.17.0")
        .header("Sec-Fetch-Mode", "cors")
        .header("X-Stainless-Lang", "js")
        .header("X-Stainless-Arch", "arm64")
        .header("X-Stainless-Package-Version", "5.11.0")
        .header("X-Dashscope-Cachecontrol", "enable")
        .header("X-Stainless-Retry-Count", "0")
        .header("X-Stainless-Os", "MacOS")
        .header("X-Dashscope-Authtype", "qwen-oauth")
        .header("X-Stainless-Runtime", "node")
        .body(body.to_vec())
        .send()
        .await
}

pub(super) async fn refresh_qwen_auth(
    auth_dir: &Path,
    auth_file_name: &str,
    mut auth: QwenAuthState,
    default_proxy_url: Option<&str>,
    endpoints: &QwenCallerEndpoints,
    now: DateTime<Utc>,
) -> Result<QwenAuthState, QwenCallerError> {
    let refresh_token = auth
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(QwenCallerError::MissingAccessToken)?;
    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))?;
    let response = client
        .post(&endpoints.token_url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", endpoints.client_id.as_str()),
        ])
        .send()
        .await?;

    let status = response.status().as_u16();
    let body = response.bytes().await?.to_vec();
    if !(200..300).contains(&status) {
        return Err(QwenCallerError::RefreshRejected {
            status,
            body: response_body_message(&body),
        });
    }

    let refreshed = parse_refresh_token_response(&body, now)?;
    apply_refreshed_auth_state(&mut auth, &refreshed, now);
    persist_qwen_auth_state(auth_dir, auth_file_name, &auth)?;
    Ok(auth)
}

pub(super) fn qwen_auth_failure(error: QwenCallerError) -> ExecutionFailure<QwenCallerError> {
    ExecutionFailure::retryable(error, unauthorized_result())
}

pub(super) fn qwen_local_failure(error: QwenCallerError) -> ExecutionFailure<QwenCallerError> {
    ExecutionFailure::retryable(error, unauthorized_result())
}

pub(super) fn qwen_request_failure(error: reqwest::Error) -> ExecutionFailure<QwenCallerError> {
    let message = error.to_string();
    ExecutionFailure::retryable(
        QwenCallerError::Request(error),
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
