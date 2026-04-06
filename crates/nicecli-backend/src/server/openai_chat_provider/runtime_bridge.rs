use super::*;

pub(super) async fn try_execute_public_qwen_chat_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<QwenCallerError>> {
    let mut caller = QwenChatCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute(
            QwenChatCompletionsRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn try_execute_public_qwen_chat_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<reqwest::Response>, ExecuteWithRetryError<QwenCallerError>> {
    let mut caller = QwenChatCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute_stream(
            QwenChatCompletionsRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn try_execute_public_kimi_chat_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<KimiCallerError>> {
    let mut caller = KimiChatCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute(
            KimiChatCompletionsRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn try_execute_public_kimi_chat_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    user_agent: &str,
    model: &str,
    body: Vec<u8>,
) -> Result<Option<reqwest::Response>, ExecuteWithRetryError<KimiCallerError>> {
    let mut caller = KimiChatCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(user_agent.to_string());
    match caller
        .execute_stream(
            KimiChatCompletionsRequest {
                model: model.to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}
