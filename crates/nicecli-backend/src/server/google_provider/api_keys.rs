use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct GeminiApiKeyEntry {
    #[serde(rename = "api-key")]
    pub(super) api_key: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub(super) priority: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) prefix: String,
    #[serde(rename = "base-url", skip_serializing_if = "String::is_empty")]
    pub(super) base_url: String,
    #[serde(rename = "proxy-url", skip_serializing_if = "String::is_empty")]
    pub(super) proxy_url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) models: Vec<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) headers: Option<BTreeMap<String, String>>,
    #[serde(rename = "excluded-models", skip_serializing_if = "Vec::is_empty")]
    pub(super) excluded_models: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(in crate::server) struct GeminiApiKeyPatchValue {
    #[serde(rename = "api-key")]
    api_key: Option<String>,
    prefix: Option<String>,
    #[serde(rename = "base-url")]
    base_url: Option<String>,
    #[serde(rename = "proxy-url")]
    proxy_url: Option<String>,
    headers: Option<BTreeMap<String, String>>,
    #[serde(rename = "excluded-models")]
    excluded_models: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(in crate::server) struct GeminiApiKeyPatchRequest {
    index: Option<usize>,
    #[serde(rename = "match")]
    match_value: Option<String>,
    value: Option<GeminiApiKeyPatchValue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct VertexApiKeyModelEntry {
    pub(super) name: String,
    pub(super) alias: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct VertexApiKeyEntry {
    #[serde(rename = "api-key")]
    pub(super) api_key: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub(super) priority: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) prefix: String,
    #[serde(rename = "base-url", skip_serializing_if = "String::is_empty")]
    pub(super) base_url: String,
    #[serde(rename = "proxy-url", skip_serializing_if = "String::is_empty")]
    pub(super) proxy_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) headers: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) models: Vec<VertexApiKeyModelEntry>,
    #[serde(rename = "excluded-models", skip_serializing_if = "Vec::is_empty")]
    pub(super) excluded_models: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(in crate::server) struct VertexApiKeyPatchValue {
    #[serde(rename = "api-key")]
    api_key: Option<String>,
    prefix: Option<String>,
    #[serde(rename = "base-url")]
    base_url: Option<String>,
    #[serde(rename = "proxy-url")]
    proxy_url: Option<String>,
    headers: Option<BTreeMap<String, String>>,
    models: Option<Vec<VertexApiKeyModelEntry>>,
    #[serde(rename = "excluded-models")]
    excluded_models: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(in crate::server) struct VertexApiKeyPatchRequest {
    index: Option<usize>,
    #[serde(rename = "match")]
    match_value: Option<String>,
    value: Option<VertexApiKeyPatchValue>,
}

pub(in crate::server) async fn get_gemini_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_gemini_api_key_entries(&state) {
        Ok(entries) => single_field_json_response("gemini-api-key", json!(entries)),
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn put_gemini_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: Vec<GeminiApiKeyEntry> = match parse_json_or_items_wrapper(&body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body")
        }
    };

    match persist_gemini_api_key_entries(&state, sanitize_gemini_api_key_entries(parsed)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn patch_gemini_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<GeminiApiKeyPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    let mut entries = match load_gemini_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    let target_index = request
        .index
        .filter(|index| *index < entries.len())
        .or_else(|| {
            request
                .match_value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|needle| entries.iter().position(|entry| entry.api_key == needle))
        });
    let Some(target_index) = target_index else {
        return json_error_response(StatusCode::NOT_FOUND, "item not found", "item not found");
    };

    let mut entry = entries[target_index].clone();
    if let Some(api_key) = value.api_key {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            entries.remove(target_index);
            return match persist_gemini_api_key_entries(
                &state,
                sanitize_gemini_api_key_entries(entries),
            ) {
                Ok(response) => response,
                Err(error) => config_error_response(error),
            };
        }
        entry.api_key = trimmed.to_string();
    }
    if let Some(prefix) = value.prefix {
        entry.prefix = normalize_model_prefix(&prefix);
    }
    if let Some(base_url) = value.base_url {
        entry.base_url = base_url.trim().to_string();
    }
    if let Some(proxy_url) = value.proxy_url {
        entry.proxy_url = proxy_url.trim().to_string();
    }
    if let Some(headers) = value.headers {
        entry.headers = normalize_headers(headers);
    }
    if let Some(excluded_models) = value.excluded_models {
        entry.excluded_models = normalize_excluded_models(excluded_models);
    }

    entries[target_index] = entry;
    match persist_gemini_api_key_entries(&state, sanitize_gemini_api_key_entries(entries)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn delete_gemini_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<ApiKeyIndexQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut entries = match load_gemini_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    if let Some(api_key) = query
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let original_len = entries.len();
        entries.retain(|entry| entry.api_key != api_key);
        if entries.len() == original_len {
            return json_error_response(StatusCode::NOT_FOUND, "item not found", "item not found");
        }
    } else if let Some(index) = query.index {
        if index >= entries.len() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing api-key or index",
                "missing api-key or index",
            );
        }
        entries.remove(index);
    } else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing api-key or index",
            "missing api-key or index",
        );
    }

    match persist_gemini_api_key_entries(&state, sanitize_gemini_api_key_entries(entries)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn get_vertex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_vertex_api_key_entries(&state) {
        Ok(entries) => single_field_json_response("vertex-api-key", json!(entries)),
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn put_vertex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: Vec<VertexApiKeyEntry> = match parse_json_or_items_wrapper(&body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body")
        }
    };

    let normalized = match normalize_vertex_api_key_entries_for_put(parsed) {
        Ok(entries) => entries,
        Err(message) => return json_error_response(StatusCode::BAD_REQUEST, &message, &message),
    };

    match persist_vertex_api_key_entries(&state, normalized) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn patch_vertex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<VertexApiKeyPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    let mut entries = match load_vertex_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    let target_index = request
        .index
        .filter(|index| *index < entries.len())
        .or_else(|| {
            request
                .match_value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|needle| entries.iter().position(|entry| entry.api_key == needle))
        });
    let Some(target_index) = target_index else {
        return json_error_response(StatusCode::NOT_FOUND, "item not found", "item not found");
    };

    let mut entry = entries[target_index].clone();
    if let Some(api_key) = value.api_key {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            entries.remove(target_index);
            return match persist_vertex_api_key_entries(
                &state,
                sanitize_vertex_api_key_entries(entries),
            ) {
                Ok(response) => response,
                Err(error) => config_error_response(error),
            };
        }
        entry.api_key = trimmed.to_string();
    }
    if let Some(prefix) = value.prefix {
        entry.prefix = prefix.trim().to_string();
    }
    if let Some(base_url) = value.base_url {
        let trimmed = base_url.trim();
        if trimmed.is_empty() {
            entries.remove(target_index);
            return match persist_vertex_api_key_entries(
                &state,
                sanitize_vertex_api_key_entries(entries),
            ) {
                Ok(response) => response,
                Err(error) => config_error_response(error),
            };
        }
        entry.base_url = trimmed.to_string();
    }
    if let Some(proxy_url) = value.proxy_url {
        entry.proxy_url = proxy_url.trim().to_string();
    }
    if let Some(headers) = value.headers {
        entry.headers = normalize_headers(headers);
    }
    if let Some(models) = value.models {
        entry.models = normalize_vertex_api_key_models(models);
    }
    if let Some(excluded_models) = value.excluded_models {
        entry.excluded_models = normalize_excluded_models(excluded_models);
    }

    entries[target_index] = entry;
    match persist_vertex_api_key_entries(&state, sanitize_vertex_api_key_entries(entries)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(in crate::server) async fn delete_vertex_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<ApiKeyIndexQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut entries = match load_vertex_api_key_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    if let Some(api_key) = query
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        entries.retain(|entry| entry.api_key != api_key);
    } else if let Some(index) = query.index {
        if index >= entries.len() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing api-key or index",
                "missing api-key or index",
            );
        }
        entries.remove(index);
    } else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing api-key or index",
            "missing api-key or index",
        );
    }

    match persist_vertex_api_key_entries(&state, sanitize_vertex_api_key_entries(entries)) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) fn gemini_api_key_entries_from_config_json(
    config: &JsonValue,
) -> Vec<GeminiApiKeyEntry> {
    let entries = config_json_value(config, "gemini-api-key")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<GeminiApiKeyEntry>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sanitize_gemini_api_key_entries(entries)
}

pub(super) fn vertex_api_key_entries_from_config_json(
    config: &JsonValue,
) -> Vec<VertexApiKeyEntry> {
    let entries = config_json_value(config, "vertex-api-key")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<VertexApiKeyEntry>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sanitize_vertex_api_key_entries(entries)
}

fn sanitize_gemini_api_key_entries(entries: Vec<GeminiApiKeyEntry>) -> Vec<GeminiApiKeyEntry> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for mut entry in entries {
        entry.api_key = entry.api_key.trim().to_string();
        if entry.api_key.is_empty() || !seen.insert(entry.api_key.clone()) {
            continue;
        }

        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().to_string();
        entry.proxy_url = entry.proxy_url.trim().to_string();
        entry.headers = entry.headers.and_then(normalize_headers);
        entry.excluded_models = normalize_excluded_models(entry.excluded_models);
        normalized.push(entry);
    }

    normalized
}

fn load_gemini_api_key_entries(
    state: &BackendAppState,
) -> Result<Vec<GeminiApiKeyEntry>, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(gemini_api_key_entries_from_config_json(&config))
}

fn persist_gemini_api_key_entries(
    state: &BackendAppState,
    entries: Vec<GeminiApiKeyEntry>,
) -> Result<Response, ConfigError> {
    if entries.is_empty() {
        persist_top_level_config_value(state, "gemini-api-key", None)
    } else {
        persist_top_level_config_value(state, "gemini-api-key", Some(json!(entries)))
    }
}

fn normalize_vertex_api_key_models(
    models: Vec<VertexApiKeyModelEntry>,
) -> Vec<VertexApiKeyModelEntry> {
    let mut normalized = Vec::new();
    for mut model in models {
        model.name = model.name.trim().to_string();
        model.alias = model.alias.trim().to_string();
        if model.name.is_empty() || model.alias.is_empty() {
            continue;
        }
        normalized.push(model);
    }
    normalized
}

fn sanitize_vertex_api_key_entries(entries: Vec<VertexApiKeyEntry>) -> Vec<VertexApiKeyEntry> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for mut entry in entries {
        entry.api_key = entry.api_key.trim().to_string();
        if entry.api_key.is_empty() {
            continue;
        }

        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().to_string();
        entry.proxy_url = entry.proxy_url.trim().to_string();
        entry.headers = entry.headers.and_then(normalize_headers);
        entry.models = normalize_vertex_api_key_models(entry.models);
        entry.excluded_models = normalize_excluded_models(entry.excluded_models);

        let unique_key = format!("{}|{}", entry.api_key, entry.base_url);
        if !seen.insert(unique_key) {
            continue;
        }
        normalized.push(entry);
    }

    normalized
}

fn normalize_vertex_api_key_entries_for_put(
    entries: Vec<VertexApiKeyEntry>,
) -> Result<Vec<VertexApiKeyEntry>, String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for (index, mut entry) in entries.into_iter().enumerate() {
        entry.api_key = entry.api_key.trim().to_string();
        entry.prefix = entry.prefix.trim().to_string();
        entry.base_url = entry.base_url.trim().to_string();
        entry.proxy_url = entry.proxy_url.trim().to_string();
        entry.headers = entry.headers.and_then(normalize_headers);
        entry.models = normalize_vertex_api_key_models(entry.models);
        entry.excluded_models = normalize_excluded_models(entry.excluded_models);

        if entry.api_key.is_empty() {
            return Err(format!("vertex-api-key[{index}].api-key is required"));
        }

        entry.prefix = normalize_model_prefix(&entry.prefix);
        let unique_key = format!("{}|{}", entry.api_key, entry.base_url);
        if !seen.insert(unique_key) {
            continue;
        }
        normalized.push(entry);
    }

    Ok(normalized)
}

fn load_vertex_api_key_entries(
    state: &BackendAppState,
) -> Result<Vec<VertexApiKeyEntry>, ConfigError> {
    let config = load_current_config_json(state)?;
    Ok(vertex_api_key_entries_from_config_json(&config))
}

fn persist_vertex_api_key_entries(
    state: &BackendAppState,
    entries: Vec<VertexApiKeyEntry>,
) -> Result<Response, ConfigError> {
    if entries.is_empty() {
        persist_top_level_config_value(state, "vertex-api-key", None)
    } else {
        persist_top_level_config_value(state, "vertex-api-key", Some(json!(entries)))
    }
}

pub(super) fn resolve_gemini_api_key_entry_model(
    entry: &GeminiApiKeyEntry,
    requested_public_model: &str,
    force_prefix: bool,
) -> Option<String> {
    if entry.models.is_empty() {
        return static_model_definitions_by_channel("gemini", None)
            .into_iter()
            .find(|model| {
                config_public_model_matches(
                    requested_public_model,
                    &model.id,
                    &entry.prefix,
                    force_prefix,
                ) && !config_model_matches_excluded_patterns(model, &entry.excluded_models)
            })
            .map(|model| model.id);
    }

    for raw_model in &entry.models {
        let Some(object) = raw_model.as_object() else {
            continue;
        };
        let Some(upstream_model) = first_non_empty_string(object, &["name", "id"]) else {
            continue;
        };
        let alias = first_non_empty_string(object, &["alias"]);
        let public_model = alias.as_deref().unwrap_or(upstream_model.as_str());
        if !config_public_model_matches(
            requested_public_model,
            public_model,
            &entry.prefix,
            force_prefix,
        ) {
            continue;
        }
        if explicit_config_model_matches_excluded_patterns(
            "gemini",
            &upstream_model,
            alias.as_deref(),
            &entry.excluded_models,
        ) {
            continue;
        }
        return Some(upstream_model);
    }

    None
}

pub(super) fn resolve_vertex_api_key_entry_model(
    entry: &VertexApiKeyEntry,
    requested_public_model: &str,
    force_prefix: bool,
) -> Option<String> {
    if entry.models.is_empty() {
        return static_model_definitions_by_channel("vertex", None)
            .into_iter()
            .find(|model| {
                config_public_model_matches(
                    requested_public_model,
                    &model.id,
                    &entry.prefix,
                    force_prefix,
                ) && !config_model_matches_excluded_patterns(model, &entry.excluded_models)
            })
            .map(|model| model.id);
    }

    for model in &entry.models {
        let upstream_model = model.name.trim();
        if upstream_model.is_empty() {
            continue;
        }
        let alias = model.alias.trim();
        let public_model = if alias.is_empty() {
            upstream_model
        } else {
            alias
        };
        if !config_public_model_matches(
            requested_public_model,
            public_model,
            &entry.prefix,
            force_prefix,
        ) {
            continue;
        }
        if explicit_config_model_matches_excluded_patterns(
            "vertex",
            upstream_model,
            (!alias.is_empty()).then_some(alias),
            &entry.excluded_models,
        ) {
            continue;
        }
        return Some(upstream_model.to_string());
    }

    None
}

fn config_public_model_matches(
    requested_public_model: &str,
    public_model: &str,
    prefix: &str,
    force_prefix: bool,
) -> bool {
    let requested = normalize_model_identifier(requested_public_model);
    let public_model = public_model.trim();
    if requested.is_empty() || public_model.is_empty() {
        return false;
    }

    let direct = normalize_model_identifier(public_model);
    let normalized_prefix = normalize_model_prefix(prefix);
    if normalized_prefix.is_empty() {
        return requested == direct;
    }

    let prefixed = normalize_model_identifier(&format!("{normalized_prefix}/{public_model}"));
    if force_prefix {
        requested == prefixed
    } else {
        requested == direct || requested == prefixed
    }
}

fn config_model_matches_excluded_patterns(model: &ModelInfo, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| model_matches_pattern(model, pattern))
}

fn explicit_config_model_matches_excluded_patterns(
    provider: &str,
    upstream_model: &str,
    alias: Option<&str>,
    patterns: &[String],
) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let mut model = build_minimal_public_model_info(provider, upstream_model);
    if let Some(alias) = alias.map(str::trim).filter(|value| !value.is_empty()) {
        model.id = alias.to_string();
        model.name = None;
    }
    config_model_matches_excluded_patterns(&model, patterns)
}
