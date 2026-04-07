use super::config_json_value;
use nicecli_models::{
    lookup_static_model_info, static_model_definitions_by_channel, ModelInfo, ThinkingSupport,
};
use nicecli_runtime::AuthSnapshot;
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;

mod helpers;
mod oauth_metadata;

pub(in crate::server) use helpers::*;
pub(in crate::server) use oauth_metadata::*;

const CODEX_BUNDLED_MODEL_SLUGS: &[&str] = &[
    "gpt-5.3-codex",
    "gpt-5.4",
    "gpt-5.2-codex",
    "gpt-5.1-codex-max",
    "gpt-5.2",
    "gpt-5.1-codex-mini",
];
const CODEX_COMPAT_BASE_INSTRUCTIONS: &str = concat!(
    "You are Codex, based on GPT-5. You are running as a coding agent in the Codex CLI on a user's computer.\n\n",
    "When working:\n",
    "- Solve the user's task end-to-end when practical.\n",
    "- Search the codebase before making assumptions.\n",
    "- Prefer `rg` and focused, minimal edits.\n",
    "- Preserve user changes and avoid unrelated refactors.\n",
    "- Do not use destructive git commands unless explicitly requested.\n",
    "- Keep progress updates concise and informative.\n",
    "- In review mode, prioritize bugs, regressions, risks, and missing tests.\n",
    "- Final answers should be concise, actionable, and reference the files you changed.\n",
);
const CODEX_REMOTE_PRIORITY_START: i32 = 100;

pub(super) fn collect_public_openai_models(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<JsonValue> {
    collect_public_openai_model_infos(config, snapshots)
        .into_iter()
        .map(|model| openai_public_payload_from_model_info(&model))
        .collect()
}

pub(super) fn collect_public_codex_models(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<JsonValue> {
    collect_public_codex_model_infos(config, snapshots)
        .into_iter()
        .filter(|model| codex_public_model_needs_overlay(&model.id))
        .enumerate()
        .map(|(index, model)| codex_public_payload_from_model_info(&model, index))
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

pub(in crate::server) fn collect_public_codex_model_infos(
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
    append_public_model_infos_from_snapshots(
        config,
        snapshots,
        &["codex"],
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

fn codex_public_model_needs_overlay(model_id: &str) -> bool {
    let normalized = normalize_model_identifier(model_id);
    !CODEX_BUNDLED_MODEL_SLUGS
        .iter()
        .any(|slug| normalized == normalize_model_identifier(slug))
}

fn codex_public_payload_from_model_info(model: &ModelInfo, index: usize) -> JsonValue {
    let reasoning_levels = codex_reasoning_levels_from_model_info(model);
    let default_reasoning_level =
        codex_default_reasoning_level_from_supported(&reasoning_levels).map(JsonValue::String);
    let supports_reasoning_summaries =
        !reasoning_levels.is_empty() && codex_model_supports_rich_tools(model);
    let support_verbosity = codex_model_supports_rich_tools(model);

    json!({
        "slug": &model.id,
        "display_name": &model.id,
        "description": model.description.clone(),
        "default_reasoning_level": default_reasoning_level,
        "supported_reasoning_levels": reasoning_levels,
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": CODEX_REMOTE_PRIORITY_START + i32::try_from(index).unwrap_or(i32::MAX),
        "availability_nux": JsonValue::Null,
        "upgrade": JsonValue::Null,
        "base_instructions": CODEX_COMPAT_BASE_INSTRUCTIONS,
        "model_messages": JsonValue::Null,
        "supports_reasoning_summaries": supports_reasoning_summaries,
        "default_reasoning_summary": if supports_reasoning_summaries { "none" } else { "auto" },
        "support_verbosity": support_verbosity,
        "default_verbosity": if support_verbosity { json!("low") } else { JsonValue::Null },
        "apply_patch_tool_type": "freeform",
        "web_search_tool_type": "text",
        "truncation_policy": {
            "mode": "tokens",
            "limit": 10_000,
        },
        "supports_parallel_tool_calls": codex_model_supports_rich_tools(model),
        "supports_image_detail_original": false,
        "context_window": model.context_length,
        "auto_compact_token_limit": JsonValue::Null,
        "effective_context_window_percent": 95,
        "experimental_supported_tools": [],
        "input_modalities": codex_input_modalities_from_model_info(model),
        "supports_search_tool": false,
    })
}

fn codex_reasoning_levels_from_model_info(model: &ModelInfo) -> Vec<JsonValue> {
    codex_reasoning_level_names(model.thinking.as_ref())
        .into_iter()
        .map(|level| {
            json!({
                "effort": level,
                "description": codex_reasoning_level_description(&level),
            })
        })
        .collect()
}

fn codex_reasoning_level_names(thinking: Option<&ThinkingSupport>) -> Vec<String> {
    let Some(thinking) = thinking else {
        return Vec::new();
    };

    if !thinking.levels.is_empty() {
        let mut levels = thinking
            .levels
            .iter()
            .filter_map(|level| codex_reasoning_level_name(level))
            .collect::<Vec<_>>();
        levels.dedup();
        return levels;
    }

    if thinking.max.is_some() || thinking.min.is_some() {
        return vec!["low".to_string(), "medium".to_string(), "high".to_string()];
    }

    Vec::new()
}

fn codex_reasoning_level_name(level: &str) -> Option<String> {
    match normalize_model_identifier(level).as_str() {
        "none" => Some("none".to_string()),
        "minimal" => Some("minimal".to_string()),
        "low" => Some("low".to_string()),
        "medium" => Some("medium".to_string()),
        "high" => Some("high".to_string()),
        "xhigh" | "max" => Some("xhigh".to_string()),
        _ => None,
    }
}

fn codex_reasoning_level_description(level: &str) -> &'static str {
    match level {
        "none" => "Uses no additional deliberate reasoning",
        "minimal" => "Fast responses with minimal reasoning",
        "low" => "Fast responses with lighter reasoning",
        "medium" => "Balances speed and reasoning depth for everyday tasks",
        "high" => "Greater reasoning depth for complex problems",
        "xhigh" => "Extra high reasoning depth for complex problems",
        _ => "Reasoning mode",
    }
}

fn codex_default_reasoning_level_from_supported(levels: &[JsonValue]) -> Option<String> {
    let supported = levels
        .iter()
        .filter_map(|level| level.get("effort"))
        .filter_map(JsonValue::as_str)
        .collect::<Vec<_>>();

    for preferred in ["medium", "low", "minimal", "high", "xhigh", "none"] {
        if supported.contains(&preferred) {
            return Some(preferred.to_string());
        }
    }

    supported.first().map(|value| (*value).to_string())
}

fn codex_model_supports_rich_tools(model: &ModelInfo) -> bool {
    model
        .supported_parameters
        .iter()
        .any(|parameter| normalize_model_identifier(parameter) == "tools")
        || model
            .owned_by
            .as_deref()
            .is_some_and(|owner| normalize_model_identifier(owner) == "openai")
        || normalize_model_identifier(&model.id).starts_with("gpt-")
}

fn codex_input_modalities_from_model_info(model: &ModelInfo) -> Vec<String> {
    let mapped = model
        .supported_input_modalities
        .iter()
        .filter_map(
            |modality| match normalize_model_identifier(modality).as_str() {
                "text" => Some("text".to_string()),
                "image" => Some("image".to_string()),
                _ => None,
            },
        )
        .collect::<Vec<_>>();

    if mapped.is_empty() {
        vec!["text".to_string(), "image".to_string()]
    } else {
        mapped
    }
}
