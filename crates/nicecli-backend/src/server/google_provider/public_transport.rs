use super::*;

pub(super) async fn execute_public_gemini_http_request(
    proxy_url: Option<&str>,
    base_url: &str,
    method: GeminiPublicPostMethod,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
    extra_headers: Option<&BTreeMap<String, String>>,
    api_key: Option<&str>,
    bearer: Option<&str>,
) -> Result<ProviderHttpResponse, GeminiPublicCallerError> {
    let client = build_gemini_internal_http_client(proxy_url, false)
        .map_err(GeminiPublicCallerError::Request)?;
    let url = build_public_gemini_url(base_url, model, method, query);
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "application/json");
    if let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.header("X-Goog-Api-Key", api_key);
    } else if let Some(bearer) = bearer.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.header(REQWEST_AUTHORIZATION, format!("Bearer {bearer}"));
    }
    if let Some(user_agent) = trim_optional_string(Some(user_agent)) {
        builder = builder.header(REQWEST_USER_AGENT, user_agent);
    }
    builder = apply_custom_header_map(builder, extra_headers);
    let response = builder
        .body(patch_gemini_public_request_body(body, model, method))
        .send()
        .await
        .map_err(GeminiPublicCallerError::Request)?;
    provider_http_response_from_reqwest(response).await
}

pub(super) async fn execute_public_gemini_http_stream_request(
    proxy_url: Option<&str>,
    base_url: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
    extra_headers: Option<&BTreeMap<String, String>>,
    api_key: Option<&str>,
    bearer: Option<&str>,
) -> Result<PendingProviderStream, GeminiPublicCallerError> {
    let client = build_gemini_internal_http_client(proxy_url, true)
        .map_err(GeminiPublicCallerError::Request)?;
    let url = build_public_gemini_url(
        base_url,
        model,
        GeminiPublicPostMethod::StreamGenerateContent,
        query,
    );
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "text/event-stream");
    if let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.header("X-Goog-Api-Key", api_key);
    } else if let Some(bearer) = bearer.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.header(REQWEST_AUTHORIZATION, format!("Bearer {bearer}"));
    }
    if let Some(user_agent) = trim_optional_string(Some(user_agent)) {
        builder = builder.header(REQWEST_USER_AGENT, user_agent);
    }
    builder = apply_custom_header_map(builder, extra_headers);
    let response = builder
        .body(patch_gemini_public_request_body(
            body,
            model,
            GeminiPublicPostMethod::StreamGenerateContent,
        ))
        .send()
        .await
        .map_err(GeminiPublicCallerError::Request)?;
    pending_provider_stream_from_reqwest(response).await
}

pub(super) async fn execute_public_vertex_http_request(
    proxy_url: Option<&str>,
    base_url: &str,
    method: GeminiPublicPostMethod,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
    extra_headers: Option<&BTreeMap<String, String>>,
    api_key: &str,
) -> Result<ProviderHttpResponse, GeminiPublicCallerError> {
    let client = build_gemini_internal_http_client(proxy_url, false)
        .map_err(GeminiPublicCallerError::Request)?;
    let url = build_public_vertex_url(base_url, model, method, query);
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "application/json")
        .header("X-Goog-Api-Key", api_key.trim());
    if let Some(user_agent) = trim_optional_string(Some(user_agent)) {
        builder = builder.header(REQWEST_USER_AGENT, user_agent);
    }
    builder = apply_custom_header_map(builder, extra_headers);
    let response = builder
        .body(patch_gemini_public_request_body(body, model, method))
        .send()
        .await
        .map_err(GeminiPublicCallerError::Request)?;
    provider_http_response_from_reqwest(response).await
}

pub(super) async fn execute_public_vertex_http_stream_request(
    proxy_url: Option<&str>,
    base_url: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
    extra_headers: Option<&BTreeMap<String, String>>,
    api_key: &str,
) -> Result<PendingProviderStream, GeminiPublicCallerError> {
    let client = build_gemini_internal_http_client(proxy_url, true)
        .map_err(GeminiPublicCallerError::Request)?;
    let url = build_public_vertex_url(
        base_url,
        model,
        GeminiPublicPostMethod::StreamGenerateContent,
        query,
    );
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "text/event-stream")
        .header("X-Goog-Api-Key", api_key.trim());
    if let Some(user_agent) = trim_optional_string(Some(user_agent)) {
        builder = builder.header(REQWEST_USER_AGENT, user_agent);
    }
    builder = apply_custom_header_map(builder, extra_headers);
    let response = builder
        .body(patch_gemini_public_request_body(
            body,
            model,
            GeminiPublicPostMethod::StreamGenerateContent,
        ))
        .send()
        .await
        .map_err(GeminiPublicCallerError::Request)?;
    pending_provider_stream_from_reqwest(response).await
}

pub(super) async fn execute_public_vertex_service_account_http_request(
    proxy_url: Option<&str>,
    base_url: &str,
    method: GeminiPublicPostMethod,
    project_id: &str,
    location: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
    bearer: &str,
) -> Result<ProviderHttpResponse, GeminiPublicCallerError> {
    let client = build_gemini_internal_http_client(proxy_url, false)
        .map_err(GeminiPublicCallerError::Request)?;
    let url = build_public_vertex_service_account_url(
        base_url, project_id, location, model, method, query,
    );
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "application/json")
        .header(REQWEST_AUTHORIZATION, format!("Bearer {}", bearer.trim()));
    if let Some(user_agent) = trim_optional_string(Some(user_agent)) {
        builder = builder.header(REQWEST_USER_AGENT, user_agent);
    }
    let response = builder
        .body(patch_gemini_public_request_body(body, model, method))
        .send()
        .await
        .map_err(GeminiPublicCallerError::Request)?;
    provider_http_response_from_reqwest(response).await
}

pub(super) async fn execute_public_vertex_service_account_http_stream_request(
    proxy_url: Option<&str>,
    base_url: &str,
    project_id: &str,
    location: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
    bearer: &str,
) -> Result<PendingProviderStream, GeminiPublicCallerError> {
    let client = build_gemini_internal_http_client(proxy_url, true)
        .map_err(GeminiPublicCallerError::Request)?;
    let url = build_public_vertex_service_account_url(
        base_url,
        project_id,
        location,
        model,
        GeminiPublicPostMethod::StreamGenerateContent,
        query,
    );
    let mut builder = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "text/event-stream")
        .header(REQWEST_AUTHORIZATION, format!("Bearer {}", bearer.trim()));
    if let Some(user_agent) = trim_optional_string(Some(user_agent)) {
        builder = builder.header(REQWEST_USER_AGENT, user_agent);
    }
    let response = builder
        .body(patch_gemini_public_request_body(
            body,
            model,
            GeminiPublicPostMethod::StreamGenerateContent,
        ))
        .send()
        .await
        .map_err(GeminiPublicCallerError::Request)?;
    pending_provider_stream_from_reqwest(response).await
}

pub(super) fn build_public_gemini_url(
    base_url: &str,
    model: &str,
    method: GeminiPublicPostMethod,
    query: Option<&str>,
) -> String {
    let mut url = format!(
        "{}/v1beta/models/{}:{}",
        base_url.trim_end_matches('/'),
        model.trim(),
        method.as_str()
    );
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        url.push('?');
        url.push_str(query);
    } else if matches!(method, GeminiPublicPostMethod::StreamGenerateContent) {
        url.push_str("?alt=sse");
    }
    url
}

pub(super) fn build_public_vertex_url(
    base_url: &str,
    model: &str,
    method: GeminiPublicPostMethod,
    query: Option<&str>,
) -> String {
    let mut url = format!(
        "{}/v1/publishers/google/models/{}:{}",
        base_url.trim_end_matches('/'),
        model.trim(),
        method.as_str()
    );
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        url.push('?');
        url.push_str(query);
    } else if matches!(method, GeminiPublicPostMethod::StreamGenerateContent) {
        url.push_str("?alt=sse");
    }
    url
}

pub(super) fn build_public_vertex_service_account_url(
    base_url: &str,
    project_id: &str,
    location: &str,
    model: &str,
    method: GeminiPublicPostMethod,
    query: Option<&str>,
) -> String {
    let mut url = format!(
        "{}/v1/projects/{}/locations/{}/publishers/google/models/{}:{}",
        base_url.trim_end_matches('/'),
        project_id.trim(),
        normalize_vertex_location(location),
        model.trim(),
        method.as_str()
    );
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        url.push('?');
        url.push_str(query);
    } else if matches!(method, GeminiPublicPostMethod::StreamGenerateContent) {
        url.push_str("?alt=sse");
    }
    url
}

pub(super) fn default_vertex_service_account_base_url(location: &str) -> String {
    let location = normalize_vertex_location(location);
    if location.eq_ignore_ascii_case("global") {
        DEFAULT_VERTEX_PUBLIC_BASE_URL.to_string()
    } else {
        format!("https://{}-aiplatform.googleapis.com", location)
    }
}

pub(super) fn normalize_vertex_location(location: &str) -> String {
    let trimmed = location.trim();
    if trimmed.is_empty() {
        "us-central1".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn patch_gemini_public_request_body(
    body: &[u8],
    model: &str,
    method: GeminiPublicPostMethod,
) -> Vec<u8> {
    let mut parsed = match serde_json::from_slice::<JsonValue>(body) {
        Ok(parsed) => parsed,
        Err(_) => return body.to_vec(),
    };
    let Some(object) = parsed.as_object_mut() else {
        return body.to_vec();
    };
    object.insert("model".to_string(), json!(model.trim()));
    if matches!(method, GeminiPublicPostMethod::CountTokens) {
        object.remove("tools");
        object.remove("generationConfig");
        object.remove("safetySettings");
    }
    serde_json::to_vec(&parsed).unwrap_or_else(|_| body.to_vec())
}

fn apply_custom_header_map(
    mut builder: reqwest::RequestBuilder,
    headers: Option<&BTreeMap<String, String>>,
) -> reqwest::RequestBuilder {
    let Some(headers) = headers else {
        return builder;
    };
    for (name, value) in headers {
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
}

async fn provider_http_response_from_reqwest(
    response: reqwest::Response,
) -> Result<ProviderHttpResponse, GeminiPublicCallerError> {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(GeminiPublicCallerError::Request)?;
    if !(200..300).contains(&status) {
        return Err(GeminiPublicCallerError::UnexpectedStatus {
            status,
            body: String::from_utf8_lossy(&body).trim().to_string(),
        });
    }
    Ok(ProviderHttpResponse {
        status,
        headers,
        body,
    })
}

async fn pending_provider_stream_from_reqwest(
    response: reqwest::Response,
) -> Result<PendingProviderStream, GeminiPublicCallerError> {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    if !(200..300).contains(&status) {
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(GeminiPublicCallerError::Request)?;
        return Err(GeminiPublicCallerError::UnexpectedStatus {
            status,
            body: String::from_utf8_lossy(&body).trim().to_string(),
        });
    }

    Ok(PendingProviderStream {
        status,
        headers,
        response,
    })
}
