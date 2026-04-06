use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct OpenAICompatibilityApiKeyEntry {
    #[serde(rename = "api-key")]
    api_key: String,
    #[serde(rename = "proxy-url", skip_serializing_if = "String::is_empty")]
    proxy_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct OpenAICompatibilityModelEntry {
    name: String,
    alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<JsonValue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct OpenAICompatibilityEntry {
    name: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    priority: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    prefix: String,
    #[serde(rename = "base-url")]
    base_url: String,
    #[serde(rename = "api-key-entries", skip_serializing_if = "Vec::is_empty")]
    api_key_entries: Vec<OpenAICompatibilityApiKeyEntry>,
    models: Vec<OpenAICompatibilityModelEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct OpenAICompatibilityPatchValue {
    name: Option<String>,
    prefix: Option<String>,
    #[serde(rename = "base-url")]
    base_url: Option<String>,
    #[serde(rename = "api-key-entries")]
    api_key_entries: Option<Vec<OpenAICompatibilityApiKeyEntry>>,
    models: Option<Vec<OpenAICompatibilityModelEntry>>,
    headers: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct OpenAICompatibilityPatchRequest {
    name: Option<String>,
    index: Option<usize>,
    value: Option<OpenAICompatibilityPatchValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct NameIndexQuery {
    name: Option<String>,
    index: Option<usize>,
}

pub(super) async fn get_openai_compatibility(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_openai_compatibility_entries(&state) {
        Ok(entries) => single_field_json_response("openai-compatibility", json!(entries)),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_openai_compatibility(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let parsed: Vec<OpenAICompatibilityEntry> = match parse_json_or_items_wrapper(&body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body")
        }
    };

    match persist_openai_compatibility_entries(
        &state,
        sanitize_loaded_openai_compatibility_entries(parsed),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn patch_openai_compatibility(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<OpenAICompatibilityPatchRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(value) = request.value else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };

    let mut entries = match load_openai_compatibility_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    let target_index = request
        .index
        .filter(|index| *index < entries.len())
        .or_else(|| {
            request.name.as_deref().map(str::trim).and_then(|needle| {
                entries
                    .iter()
                    .position(|entry| !needle.is_empty() && entry.name == needle)
            })
        });
    let Some(target_index) = target_index else {
        return json_error_response(StatusCode::NOT_FOUND, "item not found", "item not found");
    };

    let mut entry = entries[target_index].clone();
    if let Some(name) = value.name {
        entry.name = name.trim().to_string();
    }
    if let Some(prefix) = value.prefix {
        entry.prefix = prefix.trim().to_string();
    }
    if let Some(base_url) = value.base_url {
        let trimmed = base_url.trim();
        if trimmed.is_empty() {
            entries.remove(target_index);
            return match persist_openai_compatibility_entries(
                &state,
                sanitize_loaded_openai_compatibility_entries(entries),
            ) {
                Ok(response) => response,
                Err(error) => config_error_response(error),
            };
        }
        entry.base_url = trimmed.to_string();
    }
    if let Some(api_key_entries) = value.api_key_entries {
        entry.api_key_entries = normalize_openai_compatibility_api_key_entries(api_key_entries);
    }
    if let Some(models) = value.models {
        entry.models = models;
    }
    if let Some(headers) = value.headers {
        entry.headers = normalize_headers(headers);
    }

    entries[target_index] = normalize_openai_compatibility_entry_for_write(entry);
    match persist_openai_compatibility_entries(
        &state,
        sanitize_loaded_openai_compatibility_entries(entries),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn delete_openai_compatibility(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<NameIndexQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut entries = match load_openai_compatibility_entries(&state) {
        Ok(entries) => entries,
        Err(error) => return config_error_response(error),
    };

    if let Some(name) = query.name {
        if name.is_empty() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing name or index",
                "missing name or index",
            );
        }
        entries.retain(|entry| entry.name != name);
    } else if let Some(index) = query.index {
        if index >= entries.len() {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "missing name or index",
                "missing name or index",
            );
        }
        entries.remove(index);
    } else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "missing name or index",
            "missing name or index",
        );
    }

    match persist_openai_compatibility_entries(
        &state,
        sanitize_loaded_openai_compatibility_entries(entries),
    ) {
        Ok(response) => response,
        Err(error) => config_error_response(error),
    }
}

fn normalize_openai_compatibility_api_key_entries(
    entries: Vec<OpenAICompatibilityApiKeyEntry>,
) -> Vec<OpenAICompatibilityApiKeyEntry> {
    entries
        .into_iter()
        .map(|mut entry| {
            entry.api_key = entry.api_key.trim().to_string();
            entry
        })
        .collect()
}

fn sanitize_loaded_openai_compatibility_entries(
    entries: Vec<OpenAICompatibilityEntry>,
) -> Vec<OpenAICompatibilityEntry> {
    let mut normalized = Vec::new();

    for mut entry in entries {
        entry.name = entry.name.trim().to_string();
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().to_string();
        entry.api_key_entries =
            normalize_openai_compatibility_api_key_entries(entry.api_key_entries);
        entry.headers = entry.headers.and_then(normalize_headers);
        if entry.base_url.is_empty() {
            continue;
        }
        normalized.push(entry);
    }

    normalized
}

fn normalize_openai_compatibility_entry_for_write(
    mut entry: OpenAICompatibilityEntry,
) -> OpenAICompatibilityEntry {
    entry.name = entry.name.trim().to_string();
    entry.prefix = normalize_model_prefix(&entry.prefix);
    entry.base_url = entry.base_url.trim().to_string();
    entry.api_key_entries = normalize_openai_compatibility_api_key_entries(entry.api_key_entries);
    entry.headers = entry.headers.and_then(normalize_headers);
    entry
}

fn load_openai_compatibility_entries(
    state: &BackendAppState,
) -> Result<Vec<OpenAICompatibilityEntry>, ConfigError> {
    let config = load_current_config_json(state)?;
    let entries = config_json_value(&config, "openai-compatibility")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    serde_json::from_value::<OpenAICompatibilityEntry>(item.clone()).ok()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(sanitize_loaded_openai_compatibility_entries(entries))
}

fn persist_openai_compatibility_entries(
    state: &BackendAppState,
    entries: Vec<OpenAICompatibilityEntry>,
) -> Result<Response, ConfigError> {
    if entries.is_empty() {
        persist_top_level_config_value(state, "openai-compatibility", None)
    } else {
        persist_top_level_config_value(state, "openai-compatibility", Some(json!(entries)))
    }
}
