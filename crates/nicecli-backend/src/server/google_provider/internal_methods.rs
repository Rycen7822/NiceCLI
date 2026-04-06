use super::*;

pub(in crate::server) async fn handle_v1internal_method(
    state: Arc<BackendAppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    method: String,
    request: Request<Body>,
    base_url_override: Option<&str>,
) -> Response {
    if let Err(response) = ensure_local_v1internal_request(connect_info) {
        return response;
    }

    match parse_gemini_internal_method(&method) {
        GeminiInternalMethod::GenerateContent => {
            execute_gemini_internal_generate_content(state, request, base_url_override).await
        }
        GeminiInternalMethod::StreamGenerateContent => {
            execute_gemini_internal_stream_generate_content(state, request, base_url_override).await
        }
        GeminiInternalMethod::Other => {
            pass_through_v1internal_request(state, method, request, base_url_override).await
        }
    }
}

fn parse_gemini_internal_method(method: &str) -> GeminiInternalMethod {
    match method.trim() {
        "generateContent" => GeminiInternalMethod::GenerateContent,
        "streamGenerateContent" => GeminiInternalMethod::StreamGenerateContent,
        _ => GeminiInternalMethod::Other,
    }
}

fn ensure_local_v1internal_request(
    connect_info: Option<ConnectInfo<SocketAddr>>,
) -> Result<(), Response> {
    let Some(ConnectInfo(address)) = connect_info else {
        return Ok(());
    };

    if address.ip().is_loopback() {
        return Ok(());
    }

    Err(json_error(
        StatusCode::FORBIDDEN,
        "CLI reply only allow local access",
    ))
}

async fn execute_gemini_internal_generate_content(
    state: Arc<BackendAppState>,
    request: Request<Body>,
    base_url_override: Option<&str>,
) -> Response {
    let request_body = match read_json_request_body(request).await {
        Ok(request_body) => request_body,
        Err(response) => return response,
    };

    let config = match load_current_config(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
    let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
    let base_url = base_url_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_GEMINI_INTERNAL_BASE_URL)
        .to_string();
    let user_agent = if request_body.user_agent.trim().is_empty() {
        gemini_cli_user_agent(&request_body.model)
    } else {
        request_body.user_agent.clone()
    };
    let selection_model = request_body.model.clone();

    let mut conductor = RuntimeConductor::new(&state.auth_dir, strategy);
    let auth_dir = state.auth_dir.clone();
    let model = request_body.model.clone();
    let body = request_body.body.clone();
    let query = request_body.query.clone();
    let execution = conductor
        .execute_single_with_retry(
            "gemini",
            &selection_model,
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
            move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let base_url = base_url.clone();
                let user_agent = user_agent.clone();
                let auth_file_name = selection.snapshot.name.clone();
                let model = model.clone();
                let body = body.clone();
                let query = query.clone();
                async move {
                    execute_gemini_internal_request_once(
                        &auth_dir,
                        auth_file_name.as_str(),
                        default_proxy_url.as_deref(),
                        &base_url,
                        "generateContent",
                        &model,
                        &body,
                        query.as_deref(),
                        &user_agent,
                    )
                    .await
                }
            },
        )
        .await;

    match execution {
        Ok(response) => provider_http_response(response.value),
        Err(error) => gemini_internal_error_response(error),
    }
}

async fn execute_gemini_internal_stream_generate_content(
    state: Arc<BackendAppState>,
    request: Request<Body>,
    base_url_override: Option<&str>,
) -> Response {
    let request_body = match read_json_request_body(request).await {
        Ok(request_body) => request_body,
        Err(response) => return response,
    };

    let config = match load_current_config(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
    let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
    let base_url = base_url_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_GEMINI_INTERNAL_BASE_URL)
        .to_string();
    let user_agent = if request_body.user_agent.trim().is_empty() {
        gemini_cli_user_agent(&request_body.model)
    } else {
        request_body.user_agent.clone()
    };
    let selection_model = request_body.model.clone();

    let mut conductor = RuntimeConductor::new(&state.auth_dir, strategy);
    let auth_dir = state.auth_dir.clone();
    let model = request_body.model.clone();
    let body = request_body.body.clone();
    let query = request_body.query.clone();
    let execution = conductor
        .execute_single_with_retry(
            "gemini",
            &selection_model,
            nicecli_runtime::ExecuteWithRetryOptions::new(chrono::Utc::now()),
            move |selection| {
                let auth_dir = auth_dir.clone();
                let default_proxy_url = default_proxy_url.clone();
                let base_url = base_url.clone();
                let user_agent = user_agent.clone();
                let auth_file_name = selection.snapshot.name.clone();
                let model = model.clone();
                let body = body.clone();
                let query = query.clone();
                async move {
                    execute_gemini_internal_stream_once(
                        &auth_dir,
                        auth_file_name.as_str(),
                        default_proxy_url.as_deref(),
                        &base_url,
                        &model,
                        &body,
                        query.as_deref(),
                        &user_agent,
                    )
                    .await
                }
            },
        )
        .await;

    match execution {
        Ok(response) => provider_stream_response(response.value),
        Err(error) => gemini_internal_error_response(error),
    }
}

async fn pass_through_v1internal_request(
    state: Arc<BackendAppState>,
    _method: String,
    request: Request<Body>,
    base_url_override: Option<&str>,
) -> Response {
    let config = match load_current_config(&state) {
        Ok(config) => config,
        Err(error) => return config_error_response(error),
    };
    let default_proxy_url = trim_optional_string(config.proxy_url.as_deref());
    let base_url = base_url_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_GEMINI_INTERNAL_BASE_URL);
    let query_suffix = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(request.uri().path());
    let url = format!("{}{}", base_url.trim_end_matches('/'), query_suffix);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), usize::MAX).await {
        Ok(body) => body.to_vec(),
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid body"),
    };

    let client = match build_gemini_internal_http_client(default_proxy_url.as_deref(), false) {
        Ok(client) => client,
        Err(error) => return json_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };

    let mut builder = client.post(url);
    builder = forward_request_headers(builder, &headers);
    if !headers.contains_key(CONTENT_TYPE) {
        builder = builder.header(REQWEST_CONTENT_TYPE, "application/json");
    }

    let response = match builder.body(body).send().await {
        Ok(response) => response,
        Err(error) => return json_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = match response.bytes().await {
        Ok(body) => body.to_vec(),
        Err(error) => return json_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };

    if !(200..300).contains(&status) {
        return json_error(StatusCode::BAD_REQUEST, String::from_utf8_lossy(&body));
    }

    provider_http_response(ProviderHttpResponse {
        status,
        headers,
        body,
    })
}

#[derive(Debug, Clone)]
struct GeminiInternalRequestBody {
    body: Vec<u8>,
    model: String,
    user_agent: String,
    query: Option<String>,
}

async fn read_json_request_body(
    request: Request<Body>,
) -> Result<GeminiInternalRequestBody, Response> {
    let user_agent =
        extract_trimmed_header_value(request.headers(), "User-Agent").unwrap_or_default();
    let query = request
        .uri()
        .query()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|_| json_error(StatusCode::BAD_REQUEST, "invalid body"))?
        .to_vec();
    let parsed = serde_json::from_slice::<JsonValue>(&body)
        .map_err(|_| json_error(StatusCode::BAD_REQUEST, "invalid body"))?;
    let model = parsed
        .get("model")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "model is required"))?
        .to_string();

    Ok(GeminiInternalRequestBody {
        body,
        model,
        user_agent,
        query,
    })
}

fn forward_request_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if matches!(
            name.as_str().to_ascii_lowercase().as_str(),
            "host" | "content-length"
        ) {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    builder
}
