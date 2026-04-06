use super::config_json_value;
use nicecli_models::{lookup_static_model_info, static_model_definitions_by_channel, ModelInfo};
use nicecli_runtime::AuthSnapshot;
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;

mod helpers;
mod oauth_metadata;

pub(in crate::server) use helpers::*;
pub(in crate::server) use oauth_metadata::*;

pub(super) fn collect_public_openai_models(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<JsonValue> {
    collect_public_openai_model_infos(config, snapshots)
        .into_iter()
        .map(|model| openai_public_payload_from_model_info(&model))
        .collect()
}

pub(super) fn collect_auth_file_model_infos(
    config: &JsonValue,
    snapshot: Option<&AuthSnapshot>,
    provider: Option<&str>,
) -> Vec<ModelInfo> {
    let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };

    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let account_plan = snapshot.and_then(|snapshot| snapshot.account_plan.as_deref());
    let mut models = collect_auth_file_model_infos_from_config(config, provider, account_plan);

    let Some(snapshot) = snapshot else {
        return models;
    };

    if snapshot.candidate_state.has_explicit_supported_models {
        models.retain(|model| snapshot_allows_model(snapshot, model));
        append_explicit_snapshot_models(snapshot, provider, &mut models);
    } else if models.is_empty() {
        append_explicit_snapshot_models(snapshot, provider, &mut models);
    }

    let mut excluded = oauth_excluded_models_for_provider(config, provider);
    if !snapshot.candidate_state.excluded_models.is_empty() {
        excluded = snapshot
            .candidate_state
            .excluded_models
            .iter()
            .cloned()
            .collect();
    }
    if !excluded.is_empty() {
        models = apply_excluded_model_patterns(models, &excluded);
    }

    models = apply_public_oauth_model_alias(
        models,
        &oauth_model_alias_entries_for_provider(config, provider),
    );

    if let Some(prefix) = snapshot
        .prefix
        .as_deref()
        .map(normalize_model_prefix)
        .filter(|value| !value.is_empty())
    {
        models = apply_public_model_prefixes(models, &prefix, force_prefix);
    }

    models
}

pub(super) fn auth_file_model_payload_from_model_info(model: &ModelInfo) -> JsonValue {
    let mut payload = serde_json::Map::new();
    payload.insert("id".to_string(), json!(model.id));
    if let Some(display_name) = model
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("display_name".to_string(), json!(display_name));
    }
    if let Some(model_type) = model
        .model_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("type".to_string(), json!(model_type));
    }
    if let Some(owned_by) = model
        .owned_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("owned_by".to_string(), json!(owned_by));
    }
    JsonValue::Object(payload)
}

pub(in crate::server) fn collect_public_openai_model_infos(
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
        "codex-api-key",
        "codex",
        None,
        force_prefix,
        &mut seen,
        &mut models,
    );
    append_public_model_infos_from_openai_compat_config(
        config,
        force_prefix,
        &mut seen,
        &mut models,
    );
    append_public_model_infos_from_snapshots(
        config,
        snapshots,
        &[
            "codex",
            "openai-compatibility",
            "openai_compatibility",
            "qwen",
            "kimi",
        ],
        force_prefix,
        &mut seen,
        &mut models,
    );

    models
}

pub(in crate::server) fn append_public_model_infos_from_config(
    config: &JsonValue,
    config_path: &str,
    provider: &str,
    account_plan: Option<&str>,
    force_prefix: bool,
    seen: &mut HashSet<String>,
    models: &mut Vec<ModelInfo>,
) {
    let Some(entries) = config_json_value(config, config_path).and_then(JsonValue::as_array) else {
        return;
    };

    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let resolved = resolve_public_model_infos_from_config_entry(
            object,
            provider,
            account_plan,
            force_prefix,
        );
        push_unique_model_infos(seen, models, resolved);
    }
}

pub(in crate::server) fn append_public_model_infos_from_openai_compat_config(
    config: &JsonValue,
    force_prefix: bool,
    seen: &mut HashSet<String>,
    models: &mut Vec<ModelInfo>,
) {
    let Some(entries) =
        config_json_value(config, "openai-compatibility").and_then(JsonValue::as_array)
    else {
        return;
    };

    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let resolved = resolve_public_model_infos_from_config_entry(
            object,
            "openai-compatibility",
            None,
            force_prefix,
        );
        push_unique_model_infos(seen, models, resolved);
    }
}

pub(in crate::server) fn append_public_model_infos_from_snapshots(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    providers: &[&str],
    force_prefix: bool,
    seen: &mut HashSet<String>,
    models: &mut Vec<ModelInfo>,
) {
    for snapshot in snapshots {
        if !provider_in_list(&snapshot.provider, providers) {
            continue;
        }
        let resolved = resolve_public_model_infos_from_snapshot(config, snapshot, force_prefix);
        push_unique_model_infos(seen, models, resolved);
    }
}

pub(in crate::server) fn push_unique_model_infos(
    seen: &mut HashSet<String>,
    models: &mut Vec<ModelInfo>,
    resolved: Vec<ModelInfo>,
) {
    for model in resolved {
        let key = normalize_model_identifier(&model.id);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        models.push(model);
    }
}

pub(in crate::server) fn resolve_public_model_infos_from_config_entry(
    entry: &serde_json::Map<String, JsonValue>,
    provider: &str,
    account_plan: Option<&str>,
    force_prefix: bool,
) -> Vec<ModelInfo> {
    let prefix = entry
        .get("prefix")
        .and_then(JsonValue::as_str)
        .map(normalize_model_prefix)
        .filter(|value| !value.is_empty());
    let excluded = normalize_model_patterns_from_json_values(
        entry
            .get("excluded-models")
            .or_else(|| entry.get("excluded_models")),
    );

    let mut models = if provider.eq_ignore_ascii_case("openai-compatibility")
        || provider.eq_ignore_ascii_case("openai_compatibility")
        || entry
            .get("models")
            .and_then(JsonValue::as_array)
            .is_some_and(|items| !items.is_empty())
    {
        build_config_defined_model_infos(entry, provider)
    } else {
        static_model_definitions_by_channel(provider, account_plan)
    };

    if !excluded.is_empty() {
        models = apply_excluded_model_patterns(models, &excluded);
    }
    if let Some(prefix) = prefix.as_deref() {
        models = apply_public_model_prefixes(models, prefix, force_prefix);
    }

    models
}

pub(in crate::server) fn resolve_public_model_infos_from_snapshot(
    config: &JsonValue,
    snapshot: &AuthSnapshot,
    force_prefix: bool,
) -> Vec<ModelInfo> {
    let provider = snapshot.provider.trim();
    if provider.is_empty() {
        return Vec::new();
    }

    let mut models =
        static_model_definitions_by_channel(provider, snapshot.account_plan.as_deref());
    if snapshot.candidate_state.has_explicit_supported_models {
        models.retain(|model| snapshot_allows_model(snapshot, model));
        append_explicit_snapshot_models(snapshot, provider, &mut models);
    } else if models.is_empty() {
        append_explicit_snapshot_models(snapshot, provider, &mut models);
    }

    let mut excluded = oauth_excluded_models_for_provider(config, provider);
    if !snapshot.candidate_state.excluded_models.is_empty() {
        excluded = snapshot
            .candidate_state
            .excluded_models
            .iter()
            .cloned()
            .collect();
    }
    if !excluded.is_empty() {
        models = apply_excluded_model_patterns(models, &excluded);
    }

    models = apply_public_oauth_model_alias(
        models,
        &oauth_model_alias_entries_for_provider(config, provider),
    );

    if let Some(prefix) = snapshot
        .prefix
        .as_deref()
        .map(normalize_model_prefix)
        .filter(|value| !value.is_empty())
    {
        models = apply_public_model_prefixes(models, &prefix, force_prefix);
    }

    models
}

pub(in crate::server) fn append_explicit_snapshot_models(
    snapshot: &AuthSnapshot,
    provider: &str,
    models: &mut Vec<ModelInfo>,
) {
    let mut seen = models
        .iter()
        .map(|model| normalize_model_identifier(&model.id))
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();

    for model_id in snapshot.candidate_state.explicit_model_ids() {
        let mut model = build_minimal_public_model_info(provider, &model_id);
        let key = normalize_model_identifier(&model.id);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        if model.display_name.is_none() {
            model.display_name = Some(pretty_model_label(&model.id));
        }
        models.push(model);
    }
}

pub(in crate::server) fn build_config_defined_model_infos(
    entry: &serde_json::Map<String, JsonValue>,
    provider: &str,
) -> Vec<ModelInfo> {
    let Some(items) = entry.get("models").and_then(JsonValue::as_array) else {
        return Vec::new();
    };

    let compat_owner = if provider.eq_ignore_ascii_case("openai-compatibility")
        || provider.eq_ignore_ascii_case("openai_compatibility")
    {
        first_non_empty_string(entry, &["name"])
    } else {
        None
    };

    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for model in items {
        let Some(object) = model.as_object() else {
            continue;
        };
        let Some(info) = build_config_defined_model_info(object, provider, compat_owner.as_deref())
        else {
            continue;
        };
        let key = normalize_model_identifier(&info.id);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        models.push(info);
    }

    models
}

pub(in crate::server) fn build_config_defined_model_info(
    object: &serde_json::Map<String, JsonValue>,
    provider: &str,
    owner_override: Option<&str>,
) -> Option<ModelInfo> {
    let upstream_name = first_non_empty_string(object, &["name", "id"]);
    let alias = first_non_empty_string(object, &["alias"]);
    let public_id = alias.clone().or_else(|| upstream_name.clone())?;
    let public_id = public_id.trim().to_string();
    if public_id.is_empty() {
        return None;
    }

    Some(ModelInfo {
        id: public_id.clone(),
        object: "model".to_string(),
        created: first_non_empty_i64(object, &["created", "created_at"]),
        owned_by: owner_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| first_non_empty_string(object, &["owned_by", "ownedBy"]))
            .or_else(|| default_owned_by_for_provider(provider).map(str::to_string)),
        model_type: first_non_empty_string(object, &["type"])
            .or_else(|| default_model_type_for_provider(provider).map(str::to_string)),
        display_name: first_non_empty_string(object, &["display_name", "displayName"])
            .or_else(|| upstream_name.clone())
            .or_else(|| Some(public_id.clone())),
        name: None,
        version: first_non_empty_string(object, &["version"]),
        description: first_non_empty_string(object, &["description"]),
        input_token_limit: first_non_empty_i64(object, &["inputTokenLimit", "input_token_limit"]),
        output_token_limit: first_non_empty_i64(
            object,
            &["outputTokenLimit", "output_token_limit"],
        ),
        supported_generation_methods: first_non_empty_string_list(
            object,
            &["supportedGenerationMethods", "supported_generation_methods"],
        )
        .unwrap_or_default(),
        context_length: first_non_empty_i64(object, &["context_length"]),
        max_completion_tokens: first_non_empty_i64(object, &["max_completion_tokens"]),
        supported_parameters: first_non_empty_string_list(object, &["supported_parameters"])
            .unwrap_or_default(),
        supported_input_modalities: first_non_empty_string_list(
            object,
            &["supportedInputModalities", "supported_input_modalities"],
        )
        .unwrap_or_default(),
        supported_output_modalities: first_non_empty_string_list(
            object,
            &["supportedOutputModalities", "supported_output_modalities"],
        )
        .unwrap_or_default(),
        thinking: None,
    })
}

pub(in crate::server) fn build_minimal_public_model_info(
    provider: &str,
    model_id: &str,
) -> ModelInfo {
    let normalized = model_id.trim();
    if let Some(mut info) = lookup_static_model_info(normalized, Some(provider)) {
        if info.display_name.is_none() {
            info.display_name = Some(pretty_model_label(&info.id));
        }
        return info;
    }

    let bare_name = normalized
        .strip_prefix("models/")
        .unwrap_or(normalized)
        .trim();
    let mut info = ModelInfo {
        id: bare_name.to_string(),
        object: "model".to_string(),
        created: None,
        owned_by: default_owned_by_for_provider(provider).map(str::to_string),
        model_type: default_model_type_for_provider(provider).map(str::to_string),
        display_name: Some(pretty_model_label(bare_name)),
        name: None,
        version: None,
        description: None,
        input_token_limit: None,
        output_token_limit: None,
        supported_generation_methods: Vec::new(),
        context_length: None,
        max_completion_tokens: None,
        supported_parameters: Vec::new(),
        supported_input_modalities: Vec::new(),
        supported_output_modalities: Vec::new(),
        thinking: None,
    };

    if provider_in_list(
        provider,
        &["gemini", "gemini-cli", "aistudio", "vertex", "antigravity"],
    ) {
        info.name = Some(format!("models/{bare_name}"));
        info.supported_generation_methods = vec!["generateContent".to_string()];
    }

    info
}

pub(in crate::server) fn openai_public_payload_from_model_info(model: &ModelInfo) -> JsonValue {
    let mut payload = serde_json::Map::new();
    payload.insert("id".to_string(), json!(model.id));
    payload.insert("object".to_string(), json!("model"));
    if let Some(created) = model.created {
        payload.insert("created".to_string(), json!(created));
    }
    if let Some(owned_by) = model
        .owned_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("owned_by".to_string(), json!(owned_by));
    }
    JsonValue::Object(payload)
}
