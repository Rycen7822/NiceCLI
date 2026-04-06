use super::*;

pub(super) fn patch_gemini_internal_count_tokens_request_body(body: &[u8]) -> Vec<u8> {
    let mut parsed = match serde_json::from_slice::<JsonValue>(body) {
        Ok(parsed) => parsed,
        Err(_) => return body.to_vec(),
    };
    let Some(object) = parsed.as_object_mut() else {
        return body.to_vec();
    };
    object.remove("project");
    object.remove("model");
    serde_json::to_vec(&parsed).unwrap_or_else(|_| body.to_vec())
}

pub(super) async fn execute_gemini_internal_request_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    base_url: &str,
    method: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
) -> Result<ProviderHttpResponse, nicecli_runtime::ExecutionFailure<GeminiInternalCallerError>> {
    let credentials = load_gemini_internal_credentials(auth_dir, auth_file_name, default_proxy_url)
        .await
        .map_err(gemini_internal_auth_failure)?;
    let client = build_gemini_internal_http_client(
        credentials.proxy_url.as_deref().or(default_proxy_url),
        false,
    )
    .map_err(gemini_internal_request_failure)?;
    let url = build_gemini_internal_url(base_url, method, query, false);

    let response = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "application/json")
        .header(
            REQWEST_AUTHORIZATION,
            format!("Bearer {}", credentials.access_token),
        )
        .header(
            REQWEST_USER_AGENT,
            if user_agent.trim().is_empty() {
                gemini_cli_user_agent(model)
            } else {
                user_agent.trim().to_string()
            },
        )
        .header("X-Goog-Api-Client", GEMINI_CLI_API_CLIENT_HEADER)
        .body(patch_gemini_internal_request_body(
            body,
            model,
            &credentials.project_id,
        ))
        .send()
        .await
        .map_err(gemini_internal_request_failure)?;

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(gemini_internal_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(gemini_internal_status_failure(status, &body, model));
    }

    Ok(ProviderHttpResponse {
        status,
        headers,
        body,
    })
}

pub(super) async fn execute_gemini_internal_count_tokens_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    base_url: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
) -> Result<ProviderHttpResponse, nicecli_runtime::ExecutionFailure<GeminiPublicCallerError>> {
    let credentials = load_gemini_internal_credentials(auth_dir, auth_file_name, default_proxy_url)
        .await
        .map_err(gemini_public_auth_failure_from_internal)?;
    let client = build_gemini_internal_http_client(
        credentials.proxy_url.as_deref().or(default_proxy_url),
        false,
    )
    .map_err(gemini_public_request_failure)?;
    let url = build_gemini_internal_url(base_url, "countTokens", query, false);

    let response = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "application/json")
        .header(
            REQWEST_AUTHORIZATION,
            format!("Bearer {}", credentials.access_token),
        )
        .header(
            REQWEST_USER_AGENT,
            if user_agent.trim().is_empty() {
                gemini_cli_user_agent(model)
            } else {
                user_agent.trim().to_string()
            },
        )
        .header("X-Goog-Api-Client", GEMINI_CLI_API_CLIENT_HEADER)
        .body(patch_gemini_internal_count_tokens_request_body(body))
        .send()
        .await
        .map_err(gemini_public_request_failure)?;

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(gemini_public_request_failure)?;
    if !(200..300).contains(&status) {
        return Err(gemini_public_status_failure(status, &body, model));
    }

    Ok(ProviderHttpResponse {
        status,
        headers,
        body,
    })
}

pub(super) async fn execute_gemini_internal_stream_once(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
    base_url: &str,
    model: &str,
    body: &[u8],
    query: Option<&str>,
    user_agent: &str,
) -> Result<PendingProviderStream, nicecli_runtime::ExecutionFailure<GeminiInternalCallerError>> {
    let credentials = load_gemini_internal_credentials(auth_dir, auth_file_name, default_proxy_url)
        .await
        .map_err(gemini_internal_auth_failure)?;
    let client = build_gemini_internal_http_client(
        credentials.proxy_url.as_deref().or(default_proxy_url),
        true,
    )
    .map_err(gemini_internal_request_failure)?;
    let url = build_gemini_internal_url(base_url, "streamGenerateContent", query, true);

    let response = client
        .post(url)
        .header(REQWEST_CONTENT_TYPE, "application/json")
        .header(REQWEST_ACCEPT, "text/event-stream")
        .header(
            REQWEST_AUTHORIZATION,
            format!("Bearer {}", credentials.access_token),
        )
        .header(
            REQWEST_USER_AGENT,
            if user_agent.trim().is_empty() {
                gemini_cli_user_agent(model)
            } else {
                user_agent.trim().to_string()
            },
        )
        .header("X-Goog-Api-Client", GEMINI_CLI_API_CLIENT_HEADER)
        .body(patch_gemini_internal_request_body(
            body,
            model,
            &credentials.project_id,
        ))
        .send()
        .await
        .map_err(gemini_internal_request_failure)?;

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    if !(200..300).contains(&status) {
        let body = response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(gemini_internal_request_failure)?;
        return Err(gemini_internal_status_failure(status, &body, model));
    }

    Ok(PendingProviderStream {
        status,
        headers,
        response,
    })
}

pub(super) async fn load_gemini_internal_credentials(
    auth_dir: &Path,
    auth_file_name: &str,
    default_proxy_url: Option<&str>,
) -> Result<GeminiInternalCredentials, GeminiInternalCallerError> {
    let raw = read_auth_file_from_store(auth_dir, auth_file_name)?;
    let value: JsonValue = serde_json::from_slice(&raw)
        .map_err(|error| GeminiInternalCallerError::InvalidAuthFile(error.to_string()))?;
    let root = value.as_object().ok_or_else(|| {
        GeminiInternalCallerError::InvalidAuthFile("root must be an object".to_string())
    })?;

    let project_id = json_string_path(root, &[&["project_id"], &["metadata", "project_id"]])
        .ok_or(GeminiInternalCallerError::MissingProjectId)?;
    let proxy_url = json_string_path(root, &[&["proxy_url"]])
        .or_else(|| trim_optional_string(default_proxy_url));
    let access_token = load_gemini_internal_access_token(root, proxy_url.as_deref()).await?;

    Ok(GeminiInternalCredentials {
        access_token,
        project_id,
        proxy_url,
    })
}

pub(super) async fn load_gemini_internal_access_token(
    root: &serde_json::Map<String, JsonValue>,
    proxy_url: Option<&str>,
) -> Result<String, GeminiInternalCallerError> {
    let current_access_token = json_string_path(
        root,
        &[
            &["access_token"],
            &["token", "access_token"],
            &["token", "accessToken"],
        ],
    );
    if current_access_token
        .as_deref()
        .filter(|_| !gemini_internal_token_expired(root))
        .is_some()
    {
        return Ok(current_access_token.expect("token presence already checked"));
    }

    let refresh_token = json_string_path(root, &[&["refresh_token"], &["token", "refresh_token"]]);
    let token_uri = json_string_path(root, &[&["token", "token_uri"]])
        .unwrap_or_else(|| DEFAULT_GEMINI_INTERNAL_TOKEN_URL.to_string());
    let client_id = json_string_path(root, &[&["token", "client_id"]]);
    let client_secret = json_string_path(root, &[&["token", "client_secret"]]);

    if let (Some(refresh_token), Some(client_id), Some(client_secret)) =
        (refresh_token, client_id, client_secret)
    {
        return refresh_gemini_internal_access_token(
            &token_uri,
            &refresh_token,
            &client_id,
            &client_secret,
            proxy_url,
        )
        .await;
    }

    current_access_token.ok_or(GeminiInternalCallerError::MissingAccessToken)
}

pub(super) async fn load_vertex_service_account_credentials(
    root: &serde_json::Map<String, JsonValue>,
    default_proxy_url: Option<&str>,
) -> Result<VertexServiceAccountCredentials, GeminiPublicCallerError> {
    let project_id = json_string_path(
        root,
        &[
            &["project_id"],
            &["project"],
            &["service_account", "project_id"],
        ],
    )
    .ok_or_else(|| {
        GeminiPublicCallerError::InvalidAuthFile("vertex auth is missing project_id".to_string())
    })?;
    let location =
        json_string_path(root, &[&["location"]]).unwrap_or_else(|| "us-central1".to_string());
    let proxy_url = json_string_path(root, &[&["proxy_url"]])
        .or_else(|| trim_optional_string(default_proxy_url));
    let client_email =
        json_string_path(root, &[&["service_account", "client_email"]]).ok_or_else(|| {
            GeminiPublicCallerError::InvalidAuthFile(
                "vertex auth is missing service_account.client_email".to_string(),
            )
        })?;
    let private_key =
        json_string_path(root, &[&["service_account", "private_key"]]).ok_or_else(|| {
            GeminiPublicCallerError::InvalidAuthFile(
                "vertex auth is missing service_account.private_key".to_string(),
            )
        })?;
    let token_uri = json_string_path(root, &[&["service_account", "token_uri"]])
        .unwrap_or_else(|| DEFAULT_GEMINI_INTERNAL_TOKEN_URL.to_string());
    let access_token = refresh_vertex_service_account_access_token(
        &token_uri,
        &client_email,
        &private_key,
        proxy_url.as_deref(),
    )
    .await?;

    Ok(VertexServiceAccountCredentials {
        access_token,
        project_id,
        location,
        proxy_url,
    })
}

pub(super) async fn refresh_vertex_service_account_access_token(
    token_url: &str,
    client_email: &str,
    private_key: &str,
    proxy_url: Option<&str>,
) -> Result<String, GeminiPublicCallerError> {
    #[derive(Debug, Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        scope: &'a str,
        aud: &'a str,
        exp: i64,
        iat: i64,
    }

    #[derive(Debug, Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
    }

    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        iss: client_email.trim(),
        scope: GOOGLE_CLOUD_PLATFORM_SCOPE,
        aud: token_url.trim(),
        iat: now,
        exp: now + 3500,
    };
    let assertion = encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(private_key.as_bytes())
            .map_err(|error| GeminiPublicCallerError::InvalidAuthFile(error.to_string()))?,
    )
    .map_err(|error| GeminiPublicCallerError::InvalidAuthFile(error.to_string()))?;

    let client = build_gemini_internal_http_client(proxy_url, false)
        .map_err(GeminiPublicCallerError::Request)?;
    let response = client
        .post(token_url.trim())
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await
        .map_err(GeminiPublicCallerError::RefreshRequest)?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(GeminiPublicCallerError::RefreshRequest)?;
    if !status.is_success() {
        return Err(GeminiPublicCallerError::RefreshFailed(
            String::from_utf8_lossy(&body).trim().to_string(),
        ));
    }

    let parsed: TokenResponse = serde_json::from_slice(&body)
        .map_err(|error| GeminiPublicCallerError::RefreshFailed(error.to_string()))?;
    parsed
        .access_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(GeminiPublicCallerError::MissingAccessToken)
}

pub(super) fn gemini_internal_token_expired(root: &serde_json::Map<String, JsonValue>) -> bool {
    let Some(expiry) = json_string_path(
        root,
        &[&["expiry"], &["token", "expiry"], &["token", "Expiry"]],
    ) else {
        return false;
    };

    chrono::DateTime::parse_from_rfc3339(&expiry)
        .ok()
        .map(|value| {
            value.with_timezone(&chrono::Utc) <= chrono::Utc::now() + chrono::Duration::seconds(60)
        })
        .unwrap_or(false)
}

pub(super) async fn refresh_gemini_internal_access_token(
    token_url: &str,
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
    proxy_url: Option<&str>,
) -> Result<String, GeminiInternalCallerError> {
    #[derive(Debug, Deserialize)]
    struct RefreshResponse {
        access_token: Option<String>,
    }

    let client = build_gemini_internal_http_client(proxy_url, false)?;
    let response = client
        .post(token_url.trim())
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.trim()),
            ("client_id", client_id.trim()),
            ("client_secret", client_secret.trim()),
        ])
        .send()
        .await?;
    let status = response.status();
    let body = response.bytes().await?;
    if !status.is_success() {
        return Err(GeminiInternalCallerError::RefreshFailed(
            String::from_utf8_lossy(&body).trim().to_string(),
        ));
    }

    let parsed: RefreshResponse = serde_json::from_slice(&body)
        .map_err(|error| GeminiInternalCallerError::RefreshFailed(error.to_string()))?;
    parsed
        .access_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(GeminiInternalCallerError::MissingAccessToken)
}

pub(super) fn build_gemini_internal_http_client(
    proxy_url: Option<&str>,
    streaming: bool,
) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder();
    if !streaming {
        builder = builder.timeout(Duration::from_secs(60));
    }
    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build()
}

pub(super) fn build_gemini_internal_url(
    base_url: &str,
    method: &str,
    query: Option<&str>,
    streaming: bool,
) -> String {
    let mut url = format!(
        "{}/v1internal:{}",
        base_url.trim_end_matches('/'),
        method.trim()
    );
    let query = query.map(str::trim).filter(|value| !value.is_empty());
    if let Some(query) = query {
        url.push('?');
        url.push_str(query);
    } else if streaming {
        url.push_str("?alt=sse");
    }
    url
}

pub(super) fn patch_gemini_internal_request_body(
    body: &[u8],
    model: &str,
    project_id: &str,
) -> Vec<u8> {
    let mut parsed = match serde_json::from_slice::<JsonValue>(body) {
        Ok(parsed) => parsed,
        Err(_) => return body.to_vec(),
    };
    let Some(object) = parsed.as_object_mut() else {
        return body.to_vec();
    };
    object.insert("model".to_string(), json!(model.trim()));
    object.insert("project".to_string(), json!(project_id.trim()));
    serde_json::to_vec(&parsed).unwrap_or_else(|_| body.to_vec())
}

pub(super) fn json_string_path(
    object: &serde_json::Map<String, JsonValue>,
    paths: &[&[&str]],
) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = JsonValue::Object(object.clone());
        for segment in *path {
            current = current.get(*segment)?.clone();
        }
        current
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}
