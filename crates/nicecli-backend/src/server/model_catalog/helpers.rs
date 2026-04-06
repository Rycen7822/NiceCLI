use super::super::{config_json_value, json_value_to_i64, OAuthModelAliasEntry};
use super::oauth_metadata::{
    normalize_excluded_models, normalize_oauth_excluded_models_value,
    normalize_oauth_model_alias_value,
};
use super::{build_config_defined_model_infos, push_unique_model_infos};
use nicecli_models::{static_model_definitions_by_channel, ModelInfo};
use nicecli_runtime::AuthSnapshot;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashSet};

pub(in crate::server) fn normalize_model_prefix(value: &str) -> String {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty() || trimmed.contains('/') {
        String::new()
    } else {
        trimmed.to_string()
    }
}

pub(in crate::server) fn snapshot_allows_model(snapshot: &AuthSnapshot, model: &ModelInfo) -> bool {
    let lookup_keys = model_lookup_keys(model);
    if lookup_keys.is_empty() {
        return true;
    }

    if lookup_keys
        .iter()
        .any(|key| snapshot.candidate_state.excluded_models.contains(key))
    {
        return false;
    }

    if !snapshot.candidate_state.has_explicit_supported_models {
        return true;
    }

    lookup_keys
        .iter()
        .any(|key| snapshot.candidate_state.supported_models.contains(key))
}

pub(in crate::server) fn apply_excluded_model_patterns(
    models: Vec<ModelInfo>,
    patterns: &[String],
) -> Vec<ModelInfo> {
    if models.is_empty() || patterns.is_empty() {
        return models;
    }

    models
        .into_iter()
        .filter(|model| {
            !patterns
                .iter()
                .any(|pattern| model_matches_pattern(model, pattern))
        })
        .collect()
}

pub(in crate::server) fn apply_public_oauth_model_alias(
    models: Vec<ModelInfo>,
    aliases: &[OAuthModelAliasEntry],
) -> Vec<ModelInfo> {
    if models.is_empty() || aliases.is_empty() {
        return models;
    }

    #[derive(Debug, Clone)]
    struct AliasMapping {
        alias: String,
        fork: bool,
    }

    let mut forward: BTreeMap<String, Vec<AliasMapping>> = BTreeMap::new();
    for entry in aliases {
        let name = normalize_model_identifier(&entry.name);
        let alias = entry.alias.trim().to_string();
        if name.is_empty() || alias.is_empty() || entry.name.eq_ignore_ascii_case(&entry.alias) {
            continue;
        }
        forward.entry(name).or_default().push(AliasMapping {
            alias,
            fork: entry.fork,
        });
    }
    if forward.is_empty() {
        return models;
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for model in models {
        let key = normalize_model_identifier(&model.id);
        let Some(entries) = forward.get(&key) else {
            if !key.is_empty() && seen.insert(key) {
                out.push(model);
            }
            continue;
        };

        if entries.iter().any(|entry| entry.fork) && !key.is_empty() && seen.insert(key.clone()) {
            out.push(model.clone());
        }

        for entry in entries {
            let alias_key = normalize_model_identifier(&entry.alias);
            if alias_key.is_empty() || !seen.insert(alias_key) {
                continue;
            }
            let mut clone = model.clone();
            clone.id = entry.alias.trim().to_string();
            if let Some(name) = clone.name.as_deref() {
                clone.name = Some(rewrite_model_info_name(name, &model.id, &clone.id));
            }
            out.push(clone);
        }
    }

    out
}

pub(in crate::server) fn apply_public_model_prefixes(
    models: Vec<ModelInfo>,
    prefix: &str,
    force_model_prefix: bool,
) -> Vec<ModelInfo> {
    let trimmed_prefix = prefix.trim();
    if trimmed_prefix.is_empty() || models.is_empty() {
        return models;
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for model in models {
        let base_id = model.id.trim();
        if base_id.is_empty() {
            continue;
        }

        if !force_model_prefix || trimmed_prefix.eq_ignore_ascii_case(base_id) {
            let key = normalize_model_identifier(base_id);
            if !key.is_empty() && seen.insert(key) {
                out.push(model.clone());
            }
        }

        let mut clone = model.clone();
        clone.id = format!("{trimmed_prefix}/{base_id}");
        if let Some(name) = clone.name.as_deref() {
            clone.name = Some(rewrite_model_info_name(name, base_id, &clone.id));
        }

        let key = normalize_model_identifier(&clone.id);
        if !key.is_empty() && seen.insert(key) {
            out.push(clone);
        }
    }

    out
}

pub(in crate::server) fn oauth_model_alias_entries_for_provider(
    config: &JsonValue,
    provider: &str,
) -> Vec<OAuthModelAliasEntry> {
    let Some(channel) = oauth_model_alias_channel_for_provider(provider) else {
        return Vec::new();
    };
    let normalized =
        normalize_oauth_model_alias_value(config_json_value(config, "oauth-model-alias"));
    normalized
        .as_object()
        .and_then(|object| object.get(channel))
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    serde_json::from_value::<OAuthModelAliasEntry>(item.clone()).ok()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(in crate::server) fn oauth_excluded_models_for_provider(
    config: &JsonValue,
    provider: &str,
) -> Vec<String> {
    let Some(channel) = oauth_model_alias_channel_for_provider(provider) else {
        return Vec::new();
    };
    let normalized =
        normalize_oauth_excluded_models_value(config_json_value(config, "oauth-excluded-models"));
    normalized
        .as_object()
        .and_then(|object| object.get(channel))
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .map(normalize_excluded_models)
        .unwrap_or_default()
}

pub(in crate::server) fn oauth_model_alias_channel_for_provider(
    provider: &str,
) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "vertex" => Some("vertex"),
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini-cli" => Some("gemini-cli"),
        "aistudio" => Some("aistudio"),
        "antigravity" => Some("antigravity"),
        "qwen" => Some("qwen"),
        "kimi" => Some("kimi"),
        _ => None,
    }
}

pub(in crate::server) fn model_matches_identifier(model: &ModelInfo, target_key: &str) -> bool {
    model_lookup_keys(model)
        .iter()
        .any(|candidate| candidate == target_key)
}

pub(in crate::server) fn model_matches_pattern(model: &ModelInfo, pattern: &str) -> bool {
    let normalized_pattern = pattern.trim().to_ascii_lowercase();
    !normalized_pattern.is_empty()
        && model_lookup_keys(model)
            .iter()
            .any(|candidate| wildcard_match(&normalized_pattern, candidate))
}

pub(in crate::server) fn model_lookup_keys(model: &ModelInfo) -> Vec<String> {
    let mut values = Vec::new();
    let mut seen = HashSet::new();
    for candidate in [
        Some(model.id.as_str()),
        model.name.as_deref(),
        model
            .name
            .as_deref()
            .and_then(|name| name.strip_prefix("models/")),
        model.id.strip_prefix("models/"),
    ] {
        let Some(candidate) = candidate else {
            continue;
        };
        let normalized = normalize_model_identifier(candidate);
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        values.push(normalized);
    }
    values
}

pub(in crate::server) fn normalize_model_patterns_from_json_values(
    value: Option<&JsonValue>,
) -> Vec<String> {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .map(normalize_excluded_models)
        .unwrap_or_default()
}

pub(in crate::server) fn normalize_model_identifier(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(in crate::server) fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }

    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut remaining = value;

    if let Some(prefix) = parts.first().filter(|segment| !segment.is_empty()) {
        if !remaining.starts_with(prefix) {
            return false;
        }
        remaining = &remaining[prefix.len()..];
    }

    if let Some(suffix) = parts.last().filter(|segment| !segment.is_empty()) {
        if !remaining.ends_with(suffix) {
            return false;
        }
        remaining = &remaining[..remaining.len().saturating_sub(suffix.len())];
    }

    for segment in parts
        .iter()
        .skip(1)
        .take(parts.len().saturating_sub(2))
        .filter(|segment| !segment.is_empty())
    {
        let Some(index) = remaining.find(segment) else {
            return false;
        };
        remaining = &remaining[index + segment.len()..];
    }

    true
}

pub(in crate::server) fn rewrite_model_info_name(name: &str, old_id: &str, new_id: &str) -> String {
    let trimmed = name.trim();
    let old_id = old_id.trim();
    let new_id = new_id.trim();
    if trimmed.is_empty() || old_id.is_empty() || new_id.is_empty() {
        return name.to_string();
    }
    if trimmed.eq_ignore_ascii_case(old_id) {
        return new_id.to_string();
    }
    if let Some(prefix) = trimmed.strip_suffix(old_id) {
        if prefix.ends_with('/') {
            return format!("{prefix}{new_id}");
        }
    }
    if trimmed.eq_ignore_ascii_case(&format!("models/{old_id}")) {
        return format!("models/{new_id}");
    }
    name.to_string()
}

pub(in crate::server) fn pretty_model_label(model_id: &str) -> String {
    model_id
        .trim()
        .strip_prefix("models/")
        .unwrap_or(model_id.trim())
        .to_string()
}

pub(in crate::server) fn default_model_type_for_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "claude" => Some("claude"),
        "codex" => Some("openai"),
        "openai-compatibility" | "openai_compatibility" => Some("openai-compatibility"),
        "gemini" => Some("gemini"),
        "gemini-cli" => Some("gemini-cli"),
        "aistudio" => Some("aistudio"),
        "vertex" => Some("vertex"),
        "qwen" => Some("qwen"),
        "kimi" => Some("kimi"),
        "antigravity" => Some("antigravity"),
        _ => None,
    }
}

pub(in crate::server) fn collect_auth_file_model_infos_from_config(
    config: &JsonValue,
    provider: &str,
    account_plan: Option<&str>,
) -> Vec<ModelInfo> {
    let mut models = Vec::new();
    let mut seen_model_ids = HashSet::new();

    for config_path in auth_file_model_config_paths(provider) {
        let Some(entries) = config_json_value(config, config_path).and_then(JsonValue::as_array)
        else {
            continue;
        };

        for entry in entries {
            let Some(config_entry) = entry.as_object() else {
                continue;
            };
            push_unique_model_infos(
                &mut seen_model_ids,
                &mut models,
                build_config_defined_model_infos(config_entry, provider),
            );
        }
    }

    if models.is_empty() {
        models = static_model_definitions_by_channel(provider, account_plan);
    }

    models
}

pub(in crate::server) fn auth_file_model_config_paths(provider: &str) -> &'static [&'static str] {
    match provider.trim().to_ascii_lowercase().as_str() {
        "codex" => &["codex-api-key"],
        "claude" => &["claude-api-key"],
        "gemini" | "gemini-cli" | "aistudio" => &["gemini-api-key"],
        "vertex" => &["vertex-api-key"],
        "openai-compatibility" | "openai_compatibility" => &["openai-compatibility"],
        _ => &[],
    }
}

pub(in crate::server) fn provider_in_list(provider: &str, providers: &[&str]) -> bool {
    let provider = provider.trim();
    !provider.is_empty()
        && providers
            .iter()
            .any(|candidate| provider.eq_ignore_ascii_case(candidate))
}

pub(in crate::server) fn default_owned_by_for_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "claude" => Some("anthropic"),
        "codex" | "openai-compatibility" | "openai_compatibility" => Some("openai"),
        "antigravity" => Some("antigravity"),
        "qwen" => Some("qwen"),
        "kimi" => Some("kimi"),
        _ => None,
    }
}

pub(in crate::server) fn first_non_empty_string(
    object: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

pub(in crate::server) fn first_non_empty_i64(
    object: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
) -> Option<i64> {
    keys.iter()
        .filter_map(|key| object.get(*key))
        .find_map(json_value_to_i64)
}

pub(in crate::server) fn first_non_empty_string_list(
    object: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
) -> Option<Vec<String>> {
    for key in keys {
        let Some(items) = object.get(*key).and_then(JsonValue::as_array) else {
            continue;
        };

        let mut values = Vec::new();
        let mut seen = HashSet::new();
        for item in items {
            let Some(value) = item
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let dedupe_key = value.to_ascii_lowercase();
            if seen.insert(dedupe_key) {
                values.push(value.to_string());
            }
        }

        if !values.is_empty() {
            return Some(values);
        }
    }

    None
}
