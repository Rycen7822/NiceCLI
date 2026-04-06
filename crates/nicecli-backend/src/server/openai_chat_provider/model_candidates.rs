use super::*;

pub(super) fn find_public_openai_chat_model(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    target: &str,
) -> Option<ModelInfo> {
    let target_key = normalize_model_identifier(target);
    if target_key.is_empty() {
        return None;
    }

    collect_public_openai_chat_model_infos(config, snapshots)
        .into_iter()
        .find(|model| model_matches_identifier(model, &target_key))
}

pub(super) fn requested_public_openai_chat_model_candidates(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
    requested_model: &str,
) -> Vec<String> {
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    push_public_openai_chat_candidate(&mut resolved, &mut seen, requested_model);

    let current = resolved.clone();
    for snapshot in snapshots {
        if !provider_in_list(&snapshot.provider, &["qwen", "kimi"]) {
            continue;
        }
        let Some(prefix) = snapshot
            .prefix
            .as_deref()
            .map(normalize_model_prefix)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        for candidate in &current {
            if let Some(stripped) = strip_prefixed_model_id(candidate, &prefix) {
                push_public_openai_chat_candidate(&mut resolved, &mut seen, &stripped);
            }
        }
    }

    let current = resolved.clone();
    for candidate in &current {
        for provider in ["qwen", "kimi"] {
            for entry in oauth_model_alias_entries_for_provider(config, provider) {
                if normalize_model_identifier(&entry.alias) != normalize_model_identifier(candidate)
                {
                    continue;
                }
                push_public_openai_chat_candidate(&mut resolved, &mut seen, &entry.name);
            }
        }
    }

    resolved.into_iter().rev().collect()
}

fn collect_public_openai_chat_model_infos(
    config: &JsonValue,
    snapshots: &[AuthSnapshot],
) -> Vec<ModelInfo> {
    let force_prefix = config_json_value(config, "force-model-prefix")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    append_public_model_infos_from_snapshots(
        config,
        snapshots,
        &["qwen", "kimi"],
        force_prefix,
        &mut seen,
        &mut models,
    );
    models
}

fn push_public_openai_chat_candidate(
    resolved: &mut Vec<String>,
    seen: &mut HashSet<String>,
    candidate: &str,
) {
    let trimmed = candidate.trim();
    let key = normalize_model_identifier(trimmed);
    if key.is_empty() || !seen.insert(key) {
        return;
    }
    resolved.push(trimmed.to_string());
}

fn strip_prefixed_model_id(value: &str, prefix: &str) -> Option<String> {
    let trimmed = value.trim();
    let prefix = prefix.trim().trim_matches('/');
    let expected_prefix = format!("{prefix}/");
    trimmed
        .strip_prefix(expected_prefix.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
