use super::*;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::{Client, Proxy};
use std::path::Path;

pub(super) async fn send_kimi_request(
    client: &Client,
    auth: &KimiAuthState,
    user_agent: &str,
    body: &[u8],
    accept: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let url = format!(
        "{}{}",
        auth.base_url.trim_end_matches('/'),
        KIMI_CHAT_COMPLETIONS_PATH
    );
    client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, accept)
        .header(
            USER_AGENT,
            if user_agent.trim().is_empty() {
                DEFAULT_KIMI_USER_AGENT
            } else {
                user_agent.trim()
            },
        )
        .header("X-Msh-Platform", DEFAULT_KIMI_PLATFORM)
        .header("X-Msh-Version", DEFAULT_KIMI_VERSION)
        .header("X-Msh-Device-Name", resolve_device_name())
        .header("X-Msh-Device-Model", resolve_device_model())
        .header("X-Msh-Device-Id", auth.device_id.as_str())
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

pub(super) async fn refresh_kimi_auth(
    auth_dir: &Path,
    auth_file_name: &str,
    mut auth: KimiAuthState,
    default_proxy_url: Option<&str>,
    endpoints: &KimiCallerEndpoints,
) -> Result<KimiAuthState, KimiCallerError> {
    let refresh_token = auth
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(KimiCallerError::MissingAccessToken)?;
    let client = build_http_client(auth.proxy_url.as_deref().or(default_proxy_url))?;
    let response = client
        .post(&endpoints.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .header("X-Msh-Platform", DEFAULT_KIMI_PLATFORM)
        .header("X-Msh-Version", DEFAULT_KIMI_VERSION)
        .header("X-Msh-Device-Name", resolve_device_name())
        .header("X-Msh-Device-Model", resolve_device_model())
        .header("X-Msh-Device-Id", auth.device_id.as_str())
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", endpoints.client_id.as_str()),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?;

    let status = response.status().as_u16();
    let body = response.bytes().await?.to_vec();
    if !(200..300).contains(&status) {
        return Err(KimiCallerError::RefreshRejected {
            status,
            body: response_body_message(&body),
        });
    }

    let refreshed = parse_refresh_token_response(&body)?;
    apply_refreshed_auth_state(&mut auth, &refreshed);
    persist_kimi_auth_state(auth_dir, auth_file_name, &auth)?;
    Ok(auth)
}

pub(super) fn build_http_client(proxy_url: Option<&str>) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy_url.and_then(|value| trimmed(Some(value.to_string()))) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

pub(super) fn refresh_due(auth: &KimiAuthState) -> bool {
    if auth
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return auth.refresh_token.is_some();
    }

    auth.expired_at
        .map(|expired_at| expired_at <= Utc::now() + Duration::seconds(KIMI_REFRESH_LEAD_SECS))
        .unwrap_or(false)
}

pub(super) fn kimi_auth_failure(error: KimiCallerError) -> ExecutionFailure<KimiCallerError> {
    ExecutionFailure::retryable(error, unauthorized_result())
}

pub(super) fn kimi_local_failure(error: KimiCallerError) -> ExecutionFailure<KimiCallerError> {
    ExecutionFailure::retryable(error, unauthorized_result())
}

pub(super) fn kimi_request_failure(error: reqwest::Error) -> ExecutionFailure<KimiCallerError> {
    let message = error.to_string();
    ExecutionFailure::retryable(
        KimiCallerError::Request(error),
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

pub(super) fn response_body_message(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(message) = first_non_empty([
            value_string_path(&value, &["error", "message"]),
            value_string_path(&value, &["message"]),
            value_string_path(&value, &["error_description"]),
            value_string_path(&value, &["error"]),
        ]) {
            return message;
        }
    }

    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        "request failed".to_string()
    } else {
        trimmed
    }
}

pub(super) fn normalized_model(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn resolve_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn resolve_device_model() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
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
