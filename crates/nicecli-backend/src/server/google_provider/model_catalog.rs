use super::*;

pub(in crate::server) fn collect_public_gemini_models(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<JsonValue> {
    collect_public_gemini_model_infos(config, snapshots)
        .into_iter()
        .map(|model| gemini_public_list_payload_from_model_info(&model))
        .collect()
}

pub(in crate::server) fn find_public_gemini_model(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    target: &str,
) -> Option<JsonValue> {
    let target_key = normalize_model_identifier(target);
    if target_key.is_empty() {
        return None;
    }

    collect_public_gemini_model_infos(config, snapshots)
        .into_iter()
        .find(|model| model_matches_identifier(model, &target_key))
        .map(|model| gemini_public_detail_payload_from_model_info(&model))
}

fn collect_public_gemini_model_infos(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<ModelInfo> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let mut models = Vec::new();
    let mut seen = HashSet::new();

    append_public_model_infos_from_config(
        config,
        "gemini-api-key",
        "gemini",
        None,
        force_prefix,
        &mut seen,
        &mut models,
    );
    append_public_model_infos_from_config(
        config,
        "vertex-api-key",
        "vertex",
        None,
        force_prefix,
        &mut seen,
        &mut models,
    );
    append_public_model_infos_from_snapshots(
        config,
        snapshots,
        &["gemini", "gemini-cli", "aistudio", "vertex", "antigravity"],
        force_prefix,
        &mut seen,
        &mut models,
    );

    models
}

fn gemini_public_list_payload_from_model_info(model: &ModelInfo) -> JsonValue {
    let mut payload = gemini_public_payload_from_model_info(model);
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };

    let raw_name = object
        .get("name")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();
    if !raw_name.is_empty() {
        let bare_name = raw_name
            .strip_prefix("models/")
            .unwrap_or(raw_name.as_str())
            .trim();
        object.insert("name".to_string(), json!(format!("models/{bare_name}")));
        if !object.contains_key("displayName") {
            object.insert("displayName".to_string(), json!(bare_name));
        }
        if !object.contains_key("description") {
            object.insert("description".to_string(), json!(bare_name));
        }
    }
    if !object.contains_key("supportedGenerationMethods") {
        object.insert(
            "supportedGenerationMethods".to_string(),
            json!(vec!["generateContent"]),
        );
    }

    payload
}

fn gemini_public_detail_payload_from_model_info(model: &ModelInfo) -> JsonValue {
    let mut payload = gemini_public_payload_from_model_info(model);
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };

    let Some(name) = object
        .get("name")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return payload;
    };

    if !name.starts_with("models/") {
        object.insert("name".to_string(), json!(format!("models/{name}")));
    }

    payload
}

fn gemini_public_payload_from_model_info(model: &ModelInfo) -> JsonValue {
    let mut payload = serde_json::Map::new();
    let model_name = model
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| model.id.trim().to_string());
    if !model_name.is_empty() {
        payload.insert("name".to_string(), json!(model_name));
    }
    if let Some(version) = model
        .version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("version".to_string(), json!(version));
    }
    if let Some(display_name) = model
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("displayName".to_string(), json!(display_name));
    }
    if let Some(description) = model
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("description".to_string(), json!(description));
    }
    if let Some(limit) = model.input_token_limit {
        payload.insert("inputTokenLimit".to_string(), json!(limit));
    }
    if let Some(limit) = model.output_token_limit {
        payload.insert("outputTokenLimit".to_string(), json!(limit));
    }
    if !model.supported_generation_methods.is_empty() {
        payload.insert(
            "supportedGenerationMethods".to_string(),
            json!(model.supported_generation_methods),
        );
    }
    if !model.supported_input_modalities.is_empty() {
        payload.insert(
            "supportedInputModalities".to_string(),
            json!(model.supported_input_modalities),
        );
    }
    if !model.supported_output_modalities.is_empty() {
        payload.insert(
            "supportedOutputModalities".to_string(),
            json!(model.supported_output_modalities),
        );
    }

    JsonValue::Object(payload)
}
