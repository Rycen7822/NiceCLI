use super::super::{
    config_json_value, load_current_config_json, BackendAppState, OAuthModelAliasEntry,
};
use nicecli_config::ConfigError;
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet};

pub(in crate::server) fn normalize_excluded_models(models: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for model in models {
        let trimmed = model.trim().to_ascii_lowercase();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        normalized.push(trimmed);
    }
    normalized
}

pub(in crate::server) fn normalize_oauth_excluded_models_map(
    entries: HashMap<String, Vec<String>>,
) -> Option<JsonValue> {
    let mut normalized = serde_json::Map::new();
    for (provider, models) in entries {
        let key = provider.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        let models = normalize_excluded_models(models);
        if models.is_empty() {
            continue;
        }
        normalized.insert(key, json!(models));
    }
    if normalized.is_empty() {
        None
    } else {
        Some(JsonValue::Object(normalized))
    }
}

pub(in crate::server) fn normalize_oauth_excluded_models_value(
    value: Option<&JsonValue>,
) -> JsonValue {
    let mut normalized = serde_json::Map::new();
    let Some(object) = value.and_then(JsonValue::as_object) else {
        return JsonValue::Null;
    };

    for (provider, models) in object {
        let key = provider.trim().to_ascii_lowercase();
        let normalized_models = models
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .map(normalize_excluded_models)
            .unwrap_or_default();
        if key.is_empty() || normalized_models.is_empty() {
            continue;
        }
        normalized.insert(key, json!(normalized_models));
    }

    if normalized.is_empty() {
        JsonValue::Null
    } else {
        JsonValue::Object(normalized)
    }
}

pub(in crate::server) fn load_oauth_excluded_models_map(
    state: &BackendAppState,
) -> Result<HashMap<String, Vec<String>>, ConfigError> {
    let config = load_current_config_json(state)?;
    let normalized =
        normalize_oauth_excluded_models_value(config_json_value(&config, "oauth-excluded-models"));

    Ok(normalized
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(provider, models)| {
                    let items = models
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>();
                    (provider.clone(), items)
                })
                .collect()
        })
        .unwrap_or_default())
}

pub(in crate::server) fn normalize_oauth_model_alias_entries(
    entries: Vec<OAuthModelAliasEntry>,
) -> Vec<OAuthModelAliasEntry> {
    let mut normalized = Vec::new();
    let mut seen_aliases = HashSet::new();
    for entry in entries {
        let name = entry.name.trim();
        let alias = entry.alias.trim();
        if name.is_empty() || alias.is_empty() || name.eq_ignore_ascii_case(alias) {
            continue;
        }
        let alias_key = alias.to_ascii_lowercase();
        if !seen_aliases.insert(alias_key) {
            continue;
        }
        normalized.push(OAuthModelAliasEntry {
            name: name.to_string(),
            alias: alias.to_string(),
            fork: entry.fork,
        });
    }
    normalized
}

pub(in crate::server) fn normalize_oauth_model_alias_map(
    entries: HashMap<String, Vec<OAuthModelAliasEntry>>,
) -> Option<JsonValue> {
    let mut normalized = serde_json::Map::new();
    for (channel, aliases) in entries {
        let key = channel.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        let aliases = normalize_oauth_model_alias_entries(aliases);
        if aliases.is_empty() {
            continue;
        }
        normalized.insert(key, json!(aliases));
    }
    if normalized.is_empty() {
        None
    } else {
        Some(JsonValue::Object(normalized))
    }
}

pub(in crate::server) fn normalize_oauth_model_alias_value(value: Option<&JsonValue>) -> JsonValue {
    let mut normalized = serde_json::Map::new();
    let Some(object) = value.and_then(JsonValue::as_object) else {
        return JsonValue::Null;
    };

    for (channel, aliases) in object {
        let key = channel.trim().to_ascii_lowercase();
        let parsed_aliases = aliases
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        serde_json::from_value::<OAuthModelAliasEntry>(item.clone()).ok()
                    })
                    .collect::<Vec<_>>()
            })
            .map(normalize_oauth_model_alias_entries)
            .unwrap_or_default();
        if key.is_empty() || parsed_aliases.is_empty() {
            continue;
        }
        normalized.insert(key, json!(parsed_aliases));
    }

    if normalized.is_empty() {
        JsonValue::Null
    } else {
        JsonValue::Object(normalized)
    }
}

pub(in crate::server) fn load_oauth_model_alias_map(
    state: &BackendAppState,
) -> Result<HashMap<String, Vec<OAuthModelAliasEntry>>, ConfigError> {
    let config = load_current_config_json(state)?;
    let normalized =
        normalize_oauth_model_alias_value(config_json_value(&config, "oauth-model-alias"));

    Ok(normalized
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(channel, aliases)| {
                    let items = aliases
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|item| {
                            serde_json::from_value::<OAuthModelAliasEntry>(item.clone()).ok()
                        })
                        .collect::<Vec<_>>();
                    (channel.clone(), items)
                })
                .collect()
        })
        .unwrap_or_default())
}
