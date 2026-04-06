use super::public_action_requests::patch_public_gemini_request_for_antigravity;
use super::*;

pub(super) async fn try_execute_public_gemini_auth_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    model: &str,
    request: &GeminiPublicRequestBody,
    method: GeminiPublicPostMethod,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<GeminiPublicCallerError>> {
    let mut conductor = RuntimeConductor::new(&state.auth_dir, strategy);
    let auth_dir = state.auth_dir.clone();
    let request = request.clone();
    let selection_model = model.trim().to_string();
    let execution_model = selection_model.clone();
    let execution = conductor
        .execute_single_with_retry(
            "gemini",
            &selection_model,
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
            move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let auth_file_name = selection.snapshot.name.clone();
                let model = execution_model.clone();
                async move {
                    execute_public_gemini_auth_request_once(
                        &auth_dir,
                        auth_file_name.as_str(),
                        default_proxy_url.as_deref(),
                        method,
                        &model,
                        &request,
                    )
                    .await
                }
            },
        )
        .await;

    match execution {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn try_execute_public_antigravity_auth_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<AntigravityCallerError>> {
    let mut caller = AntigravityGenerateContentCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(request.user_agent.clone());
    let body = patch_public_gemini_request_for_antigravity(&request.body);
    match caller
        .execute(
            AntigravityGenerateContentRequest {
                model: model.trim().to_string(),
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

pub(super) async fn try_execute_public_antigravity_auth_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<Option<PendingProviderStream>, ExecuteWithRetryError<AntigravityCallerError>> {
    let mut caller = AntigravityGenerateContentCaller::new(&state.auth_dir, strategy)
        .with_default_proxy_url(default_proxy_url)
        .with_user_agent(request.user_agent.clone());
    let body = patch_public_gemini_request_for_antigravity(&request.body);
    match caller
        .execute_stream(
            AntigravityGenerateContentRequest {
                model: model.trim().to_string(),
                body,
            },
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
        )
        .await
    {
        Ok(response) => {
            let response = response.value;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            Ok(Some(PendingProviderStream {
                status,
                headers,
                response,
            }))
        }
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn execute_public_gemini_auth_request_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    method: GeminiPublicPostMethod,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<ProviderHttpResponse, nicecli_runtime::ExecutionFailure<GeminiPublicCallerError>> {
    let raw = read_auth_file_from_store(auth_dir, auth_file_name).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::ReadAuthFile(error))
    })?;
    let value: JsonValue = serde_json::from_slice(&raw).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(error.to_string()))
    })?;
    let root = value.as_object().ok_or_else(|| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(
            "root must be an object".to_string(),
        ))
    })?;
    let auth_base_url = json_string_path(root, &[&["base_url"]]);

    if json_string_path(root, &[&["project_id"], &["metadata", "project_id"]]).is_some() {
        let internal_base_url = auth_base_url
            .as_deref()
            .unwrap_or(DEFAULT_GEMINI_INTERNAL_BASE_URL);
        return match method {
            GeminiPublicPostMethod::GenerateContent => execute_gemini_internal_request_once(
                auth_dir,
                auth_file_name,
                default_proxy_url,
                internal_base_url,
                "generateContent",
                model,
                &request.body,
                request.query.as_deref(),
                &request.user_agent,
            )
            .await
            .map_err(gemini_public_failure_from_internal),
            GeminiPublicPostMethod::CountTokens => {
                execute_gemini_internal_count_tokens_once(
                    auth_dir,
                    auth_file_name,
                    default_proxy_url,
                    internal_base_url,
                    model,
                    &request.body,
                    request.query.as_deref(),
                    &request.user_agent,
                )
                .await
            }
            GeminiPublicPostMethod::StreamGenerateContent => Err(gemini_public_auth_failure(
                GeminiPublicCallerError::UnsupportedAuthFile,
            )),
        };
    }

    if root
        .get("auth_mode")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("web_cookies"))
    {
        return Err(gemini_public_auth_failure(
            GeminiPublicCallerError::UnsupportedAuthFile,
        ));
    }

    let proxy_url = json_string_path(root, &[&["proxy_url"]])
        .or_else(|| trim_optional_string(default_proxy_url));
    let base_url = auth_base_url.unwrap_or_else(|| DEFAULT_GEMINI_PUBLIC_BASE_URL.to_string());
    if let Some(api_key) = json_string_path(root, &[&["api_key"]]) {
        return execute_public_gemini_http_request(
            proxy_url.as_deref().or(default_proxy_url),
            base_url.as_str(),
            method,
            model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            None,
            Some(api_key.as_str()),
            None,
        )
        .await
        .map_err(gemini_public_request_failure_with_model(model));
    }

    let access_token = load_gemini_internal_access_token(root, proxy_url.as_deref())
        .await
        .map_err(gemini_public_auth_failure_from_internal)?;
    execute_public_gemini_http_request(
        proxy_url.as_deref().or(default_proxy_url),
        base_url.as_str(),
        method,
        model,
        &request.body,
        request.query.as_deref(),
        &request.user_agent,
        None,
        None,
        Some(access_token.as_str()),
    )
    .await
    .map_err(gemini_public_request_failure_with_model(model))
}

pub(super) async fn try_execute_public_gemini_auth_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<Option<PendingProviderStream>, ExecuteWithRetryError<GeminiPublicCallerError>> {
    let mut conductor = RuntimeConductor::new(&state.auth_dir, strategy);
    let auth_dir = state.auth_dir.clone();
    let request = request.clone();
    let selection_model = model.trim().to_string();
    let execution_model = selection_model.clone();
    let execution = conductor
        .execute_single_with_retry(
            "gemini",
            &selection_model,
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
            move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let auth_file_name = selection.snapshot.name.clone();
                let model = execution_model.clone();
                async move {
                    execute_public_gemini_auth_stream_request_once(
                        &auth_dir,
                        auth_file_name.as_str(),
                        default_proxy_url.as_deref(),
                        &model,
                        &request,
                    )
                    .await
                }
            },
        )
        .await;

    match execution {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn execute_public_gemini_auth_stream_request_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<PendingProviderStream, nicecli_runtime::ExecutionFailure<GeminiPublicCallerError>> {
    let raw = read_auth_file_from_store(auth_dir, auth_file_name).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::ReadAuthFile(error))
    })?;
    let value: JsonValue = serde_json::from_slice(&raw).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(error.to_string()))
    })?;
    let root = value.as_object().ok_or_else(|| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(
            "root must be an object".to_string(),
        ))
    })?;
    let auth_base_url = json_string_path(root, &[&["base_url"]]);

    if json_string_path(root, &[&["project_id"], &["metadata", "project_id"]]).is_some() {
        let internal_base_url = auth_base_url
            .as_deref()
            .unwrap_or(DEFAULT_GEMINI_INTERNAL_BASE_URL);
        return execute_gemini_internal_stream_once(
            auth_dir,
            auth_file_name,
            default_proxy_url,
            internal_base_url,
            model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
        )
        .await
        .map_err(gemini_public_failure_from_internal);
    }

    if root
        .get("auth_mode")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("web_cookies"))
    {
        return Err(gemini_public_auth_failure(
            GeminiPublicCallerError::UnsupportedAuthFile,
        ));
    }

    let proxy_url = json_string_path(root, &[&["proxy_url"]])
        .or_else(|| trim_optional_string(default_proxy_url));
    let base_url = auth_base_url.unwrap_or_else(|| DEFAULT_GEMINI_PUBLIC_BASE_URL.to_string());
    if let Some(api_key) = json_string_path(root, &[&["api_key"]]) {
        return execute_public_gemini_http_stream_request(
            proxy_url.as_deref().or(default_proxy_url),
            base_url.as_str(),
            model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            None,
            Some(api_key.as_str()),
            None,
        )
        .await
        .map_err(gemini_public_request_failure_with_model(model));
    }

    let access_token = load_gemini_internal_access_token(root, proxy_url.as_deref())
        .await
        .map_err(gemini_public_auth_failure_from_internal)?;
    execute_public_gemini_http_stream_request(
        proxy_url.as_deref().or(default_proxy_url),
        base_url.as_str(),
        model,
        &request.body,
        request.query.as_deref(),
        &request.user_agent,
        None,
        None,
        Some(access_token.as_str()),
    )
    .await
    .map_err(gemini_public_request_failure_with_model(model))
}

pub(super) async fn try_execute_public_vertex_auth_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    model: &str,
    request: &GeminiPublicRequestBody,
    method: GeminiPublicPostMethod,
) -> Result<Option<ProviderHttpResponse>, ExecuteWithRetryError<GeminiPublicCallerError>> {
    let mut conductor = RuntimeConductor::new(&state.auth_dir, strategy);
    let auth_dir = state.auth_dir.clone();
    let request = request.clone();
    let selection_model = model.trim().to_string();
    let execution_model = selection_model.clone();
    let execution = conductor
        .execute_single_with_retry(
            "vertex",
            &selection_model,
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
            move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let auth_file_name = selection.snapshot.name.clone();
                let model = execution_model.clone();
                async move {
                    execute_public_vertex_auth_request_once(
                        &auth_dir,
                        auth_file_name.as_str(),
                        default_proxy_url.as_deref(),
                        method,
                        &model,
                        &request,
                    )
                    .await
                }
            },
        )
        .await;

    match execution {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) async fn try_execute_public_vertex_auth_stream_request(
    state: Arc<BackendAppState>,
    strategy: RoutingStrategy,
    default_proxy_url: Option<String>,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<Option<PendingProviderStream>, ExecuteWithRetryError<GeminiPublicCallerError>> {
    let mut conductor = RuntimeConductor::new(&state.auth_dir, strategy);
    let auth_dir = state.auth_dir.clone();
    let request = request.clone();
    let selection_model = model.trim().to_string();
    let execution_model = selection_model.clone();
    let execution = conductor
        .execute_single_with_retry(
            "vertex",
            &selection_model,
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
            move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let request = request.clone();
                let auth_file_name = selection.snapshot.name.clone();
                let model = execution_model.clone();
                async move {
                    execute_public_vertex_auth_stream_request_once(
                        &auth_dir,
                        auth_file_name.as_str(),
                        default_proxy_url.as_deref(),
                        &model,
                        &request,
                    )
                    .await
                }
            },
        )
        .await;

    match execution {
        Ok(response) => Ok(Some(response.value)),
        Err(ExecuteWithRetryError::Runtime(RuntimeConductorError::Scheduler(
            SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
        ))) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn execute_public_vertex_auth_request_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    method: GeminiPublicPostMethod,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<ProviderHttpResponse, nicecli_runtime::ExecutionFailure<GeminiPublicCallerError>> {
    let raw = read_auth_file_from_store(auth_dir, auth_file_name).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::ReadAuthFile(error))
    })?;
    let value: JsonValue = serde_json::from_slice(&raw).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(error.to_string()))
    })?;
    let root = value.as_object().ok_or_else(|| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(
            "root must be an object".to_string(),
        ))
    })?;

    let proxy_url = json_string_path(root, &[&["proxy_url"]])
        .or_else(|| trim_optional_string(default_proxy_url));
    let location =
        json_string_path(root, &[&["location"]]).unwrap_or_else(|| "us-central1".to_string());
    let base_url = json_string_path(root, &[&["base_url"]])
        .unwrap_or_else(|| default_vertex_service_account_base_url(&location));

    if let Some(api_key) = json_string_path(root, &[&["api_key"]]) {
        return execute_public_vertex_http_request(
            proxy_url.as_deref().or(default_proxy_url),
            base_url.as_str(),
            method,
            model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            None,
            api_key.as_str(),
        )
        .await
        .map_err(gemini_public_request_failure_with_model(model));
    }

    let credentials = load_vertex_service_account_credentials(root, proxy_url.as_deref())
        .await
        .map_err(gemini_public_auth_failure)?;
    execute_public_vertex_service_account_http_request(
        credentials.proxy_url.as_deref().or(default_proxy_url),
        base_url.as_str(),
        method,
        &credentials.project_id,
        &credentials.location,
        model,
        &request.body,
        request.query.as_deref(),
        &request.user_agent,
        credentials.access_token.as_str(),
    )
    .await
    .map_err(gemini_public_request_failure_with_model(model))
}

async fn execute_public_vertex_auth_stream_request_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    model: &str,
    request: &GeminiPublicRequestBody,
) -> Result<PendingProviderStream, nicecli_runtime::ExecutionFailure<GeminiPublicCallerError>> {
    let raw = read_auth_file_from_store(auth_dir, auth_file_name).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::ReadAuthFile(error))
    })?;
    let value: JsonValue = serde_json::from_slice(&raw).map_err(|error| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(error.to_string()))
    })?;
    let root = value.as_object().ok_or_else(|| {
        gemini_public_auth_failure(GeminiPublicCallerError::InvalidAuthFile(
            "root must be an object".to_string(),
        ))
    })?;

    let proxy_url = json_string_path(root, &[&["proxy_url"]])
        .or_else(|| trim_optional_string(default_proxy_url));
    let location =
        json_string_path(root, &[&["location"]]).unwrap_or_else(|| "us-central1".to_string());
    let base_url = json_string_path(root, &[&["base_url"]])
        .unwrap_or_else(|| default_vertex_service_account_base_url(&location));

    if let Some(api_key) = json_string_path(root, &[&["api_key"]]) {
        return execute_public_vertex_http_stream_request(
            proxy_url.as_deref().or(default_proxy_url),
            base_url.as_str(),
            model,
            &request.body,
            request.query.as_deref(),
            &request.user_agent,
            None,
            api_key.as_str(),
        )
        .await
        .map_err(gemini_public_request_failure_with_model(model));
    }

    let credentials = load_vertex_service_account_credentials(root, proxy_url.as_deref())
        .await
        .map_err(gemini_public_auth_failure)?;
    execute_public_vertex_service_account_http_stream_request(
        credentials.proxy_url.as_deref().or(default_proxy_url),
        base_url.as_str(),
        &credentials.project_id,
        &credentials.location,
        model,
        &request.body,
        request.query.as_deref(),
        &request.user_agent,
        credentials.access_token.as_str(),
    )
    .await
    .map_err(gemini_public_request_failure_with_model(model))
}
