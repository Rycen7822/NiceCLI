use super::*;

pub(super) async fn read_public_gemini_request_body(
    request: Request<Body>,
) -> Result<GeminiPublicRequestBody, Response> {
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
    serde_json::from_slice::<JsonValue>(&body)
        .map_err(|_| json_error(StatusCode::BAD_REQUEST, "invalid body"))?;
    Ok(GeminiPublicRequestBody {
        body,
        user_agent,
        query,
    })
}

pub(super) fn requested_public_gemini_model_candidates(
    config: &JsonValue,
    requested_model: &str,
) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let requested = requested_model.trim().trim_start_matches("models/").trim();
    if !requested.is_empty() && seen.insert(requested.to_ascii_lowercase()) {
        candidates.push(requested.to_string());
    }
    if let Some(last_segment) = requested
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let key = last_segment.to_ascii_lowercase();
        if seen.insert(key) {
            candidates.push(last_segment.to_string());
        }
    }

    let mut index = 0;
    while index < candidates.len() {
        let current = candidates[index].clone();
        for upstream_model in reverse_public_gemini_model_aliases(config, &current) {
            let key = upstream_model.to_ascii_lowercase();
            if seen.insert(key) {
                candidates.push(upstream_model);
            }
        }
        index += 1;
    }

    candidates
}

pub(in crate::server) fn parse_gemini_public_action(action: &str) -> Option<GeminiPublicAction> {
    let action = action.trim().trim_start_matches('/');
    let (model, method) = action.split_once(':')?;
    let model = model.strip_prefix("models/").unwrap_or(model).trim();
    if model.is_empty() {
        return None;
    }

    let method = match method.trim() {
        "generateContent" => GeminiPublicPostMethod::GenerateContent,
        "streamGenerateContent" => GeminiPublicPostMethod::StreamGenerateContent,
        "countTokens" => GeminiPublicPostMethod::CountTokens,
        _ => return None,
    };

    Some(GeminiPublicAction {
        model: model.to_string(),
        method,
    })
}

pub(super) fn patch_public_gemini_request_for_antigravity(body: &[u8]) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<JsonValue>(body) else {
        return body.to_vec();
    };
    let Some(object) = value.as_object() else {
        return body.to_vec();
    };
    if object.contains_key("request") {
        return body.to_vec();
    }

    serde_json::to_vec(&json!({
        "request": JsonValue::Object(object.clone()),
    }))
    .unwrap_or_else(|_| body.to_vec())
}

fn reverse_public_gemini_model_aliases(config: &JsonValue, requested_model: &str) -> Vec<String> {
    let target = normalize_model_identifier(requested_model);
    if target.is_empty() {
        return Vec::new();
    }

    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    for provider in ["gemini-cli", "aistudio", "vertex", "antigravity"] {
        for entry in oauth_model_alias_entries_for_provider(config, provider) {
            if normalize_model_identifier(&entry.alias) != target {
                continue;
            }
            let upstream = entry.name.trim();
            if upstream.is_empty() {
                continue;
            }
            let key = upstream.to_ascii_lowercase();
            if seen.insert(key) {
                resolved.push(upstream.to_string());
            }
        }
    }

    for upstream in reverse_config_entry_model_aliases(config, "gemini-api-key", &target) {
        let key = upstream.to_ascii_lowercase();
        if seen.insert(key) {
            resolved.push(upstream);
        }
    }
    for upstream in reverse_config_entry_model_aliases(config, "vertex-api-key", &target) {
        let key = upstream.to_ascii_lowercase();
        if seen.insert(key) {
            resolved.push(upstream);
        }
    }

    resolved
}

fn reverse_config_entry_model_aliases(
    config: &JsonValue,
    config_path: &str,
    target_alias: &str,
) -> Vec<String> {
    let Some(entries) = config_json_value(config, config_path).and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    let mut resolved = Vec::new();
    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let Some(models) = object.get("models").and_then(JsonValue::as_array) else {
            continue;
        };
        for model in models {
            let Some(object) = model.as_object() else {
                continue;
            };
            let Some(alias) = first_non_empty_string(object, &["alias"]) else {
                continue;
            };
            if normalize_model_identifier(&alias) != target_alias {
                continue;
            }
            let Some(upstream) = first_non_empty_string(object, &["name", "id"]) else {
                continue;
            };
            resolved.push(upstream);
        }
    }
    resolved
}
