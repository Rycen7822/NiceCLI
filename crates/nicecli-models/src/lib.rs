use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;
use thiserror::Error;

const EMBEDDED_MODELS_JSON: &str = include_str!("../assets/models.json");
const MODEL_FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const MODEL_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/router-for-me/models/refs/heads/main/models.json",
    "https://models.router-for.me/models.json",
];

#[derive(Debug, Error)]
pub enum ModelCatalogError {
    #[error("failed to decode model catalog: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("invalid model catalog: {0}")]
    Invalid(String),
    #[error("failed to fetch model catalog: {0}")]
    Fetch(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThinkingSupport {
    pub min: Option<i64>,
    pub max: Option<i64>,
    #[serde(rename = "zero_allowed", alias = "zero-allowed")]
    pub zero_allowed: bool,
    #[serde(rename = "dynamic_allowed", alias = "dynamic-allowed")]
    pub dynamic_allowed: bool,
    pub levels: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: Option<i64>,
    #[serde(rename = "owned_by")]
    pub owned_by: Option<String>,
    #[serde(rename = "type")]
    pub model_type: Option<String>,
    #[serde(rename = "display_name", alias = "displayName")]
    pub display_name: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "inputTokenLimit", alias = "input_token_limit")]
    pub input_token_limit: Option<i64>,
    #[serde(rename = "outputTokenLimit", alias = "output_token_limit")]
    pub output_token_limit: Option<i64>,
    #[serde(
        rename = "supportedGenerationMethods",
        alias = "supported_generation_methods"
    )]
    pub supported_generation_methods: Vec<String>,
    #[serde(rename = "context_length")]
    pub context_length: Option<i64>,
    #[serde(rename = "max_completion_tokens")]
    pub max_completion_tokens: Option<i64>,
    #[serde(rename = "supported_parameters")]
    pub supported_parameters: Vec<String>,
    #[serde(
        rename = "supportedInputModalities",
        alias = "supported_input_modalities"
    )]
    pub supported_input_modalities: Vec<String>,
    #[serde(
        rename = "supportedOutputModalities",
        alias = "supported_output_modalities"
    )]
    pub supported_output_modalities: Vec<String>,
    pub thinking: Option<ThinkingSupport>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct StaticModelsJson {
    claude: Vec<ModelInfo>,
    gemini: Vec<ModelInfo>,
    vertex: Vec<ModelInfo>,
    #[serde(rename = "gemini-cli")]
    gemini_cli: Vec<ModelInfo>,
    aistudio: Vec<ModelInfo>,
    #[serde(rename = "codex-free")]
    codex_free: Vec<ModelInfo>,
    #[serde(rename = "codex-team")]
    codex_team: Vec<ModelInfo>,
    #[serde(rename = "codex-plus")]
    codex_plus: Vec<ModelInfo>,
    #[serde(rename = "codex-pro")]
    codex_pro: Vec<ModelInfo>,
    qwen: Vec<ModelInfo>,
    kimi: Vec<ModelInfo>,
    antigravity: Vec<ModelInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRefreshResult {
    pub source: String,
    pub changed_providers: Vec<String>,
}

static GLOBAL_CATALOG: LazyLock<RwLock<StaticModelsJson>> = LazyLock::new(|| {
    let parsed = parse_catalog_bytes(EMBEDDED_MODELS_JSON.as_bytes())
        .expect("embedded models.json should be valid");
    RwLock::new(parsed)
});

pub fn model_catalog_urls() -> &'static [&'static str] {
    &MODEL_URLS
}

pub fn static_model_definitions_by_channel(
    channel: &str,
    account_plan: Option<&str>,
) -> Vec<ModelInfo> {
    let guard = GLOBAL_CATALOG.read().expect("model catalog lock poisoned");
    models_for_channel(&guard, channel, account_plan)
}

pub fn lookup_static_model_info(model_id: &str, provider: Option<&str>) -> Option<ModelInfo> {
    let needle = normalize_id(model_id);
    if needle.is_empty() {
        return None;
    }

    let guard = GLOBAL_CATALOG.read().expect("model catalog lock poisoned");
    if let Some(provider) = provider {
        for model in models_for_channel(&guard, provider, None) {
            if normalize_id(&model.id) == needle {
                return Some(model);
            }
        }
    }

    for section in all_sections(&guard) {
        for model in section {
            if normalize_id(&model.id) == needle {
                return Some(model.clone());
            }
        }
    }

    None
}

pub fn replace_global_model_catalog_from_bytes(
    bytes: &[u8],
) -> Result<Vec<String>, ModelCatalogError> {
    let parsed = parse_catalog_bytes(bytes)?;
    let mut guard = GLOBAL_CATALOG.write().expect("model catalog lock poisoned");
    let changed = detect_changed_providers(&guard, &parsed);
    *guard = parsed;
    Ok(changed)
}

pub async fn refresh_global_model_catalog_from_remote(
) -> Result<Option<ModelRefreshResult>, ModelCatalogError> {
    let client = reqwest::Client::builder()
        .timeout(MODEL_FETCH_TIMEOUT)
        .build()?;

    for url in MODEL_URLS {
        let response = match client.get(url).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };
        if !response.status().is_success() {
            continue;
        }

        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        let changed = replace_global_model_catalog_from_bytes(bytes.as_ref())?;
        return Ok(Some(ModelRefreshResult {
            source: url.to_string(),
            changed_providers: changed,
        }));
    }

    Ok(None)
}

fn parse_catalog_bytes(bytes: &[u8]) -> Result<StaticModelsJson, ModelCatalogError> {
    let parsed = serde_json::from_slice::<StaticModelsJson>(bytes)?;
    validate_catalog(&parsed)?;
    Ok(parsed)
}

fn validate_catalog(catalog: &StaticModelsJson) -> Result<(), ModelCatalogError> {
    for (name, models) in [
        ("claude", &catalog.claude),
        ("gemini", &catalog.gemini),
        ("vertex", &catalog.vertex),
        ("gemini-cli", &catalog.gemini_cli),
        ("aistudio", &catalog.aistudio),
        ("codex-free", &catalog.codex_free),
        ("codex-team", &catalog.codex_team),
        ("codex-plus", &catalog.codex_plus),
        ("codex-pro", &catalog.codex_pro),
        ("qwen", &catalog.qwen),
        ("kimi", &catalog.kimi),
        ("antigravity", &catalog.antigravity),
    ] {
        if models.is_empty() {
            return Err(ModelCatalogError::Invalid(format!(
                "{name} section is empty"
            )));
        }
        let mut seen = std::collections::HashSet::with_capacity(models.len());
        for (index, model) in models.iter().enumerate() {
            let id = normalize_id(&model.id);
            if id.is_empty() {
                return Err(ModelCatalogError::Invalid(format!(
                    "{name}[{index}] has empty id"
                )));
            }
            if !seen.insert(id.clone()) {
                return Err(ModelCatalogError::Invalid(format!(
                    "{name} contains duplicate model id {id}"
                )));
            }
        }
    }
    Ok(())
}

fn models_for_channel(
    catalog: &StaticModelsJson,
    channel: &str,
    account_plan: Option<&str>,
) -> Vec<ModelInfo> {
    match normalize_id(channel).as_str() {
        "claude" => catalog.claude.clone(),
        "gemini" => catalog.gemini.clone(),
        "vertex" => catalog.vertex.clone(),
        "gemini-cli" => catalog.gemini_cli.clone(),
        "aistudio" => catalog.aistudio.clone(),
        "codex" => match normalize_codex_plan(account_plan) {
            "free" => catalog.codex_free.clone(),
            "team" => catalog.codex_team.clone(),
            "plus" => catalog.codex_plus.clone(),
            _ => catalog.codex_pro.clone(),
        },
        "qwen" => catalog.qwen.clone(),
        "kimi" => catalog.kimi.clone(),
        "antigravity" => catalog.antigravity.clone(),
        _ => Vec::new(),
    }
}

fn normalize_codex_plan(plan: Option<&str>) -> &'static str {
    match normalize_id(plan.unwrap_or_default()).as_str() {
        "free" => "free",
        "team" | "business" | "go" => "team",
        "plus" => "plus",
        "pro" => "pro",
        _ => "pro",
    }
}

fn detect_changed_providers(old: &StaticModelsJson, new: &StaticModelsJson) -> Vec<String> {
    let mut changed = Vec::new();

    for (provider, old_models, new_models) in [
        ("claude", &old.claude, &new.claude),
        ("gemini", &old.gemini, &new.gemini),
        ("vertex", &old.vertex, &new.vertex),
        ("gemini-cli", &old.gemini_cli, &new.gemini_cli),
        ("aistudio", &old.aistudio, &new.aistudio),
        ("qwen", &old.qwen, &new.qwen),
        ("kimi", &old.kimi, &new.kimi),
        ("antigravity", &old.antigravity, &new.antigravity),
    ] {
        if old_models != new_models {
            changed.push(provider.to_string());
        }
    }

    if old.codex_free != new.codex_free
        || old.codex_team != new.codex_team
        || old.codex_plus != new.codex_plus
        || old.codex_pro != new.codex_pro
    {
        changed.push("codex".to_string());
    }

    changed
}

fn all_sections(catalog: &StaticModelsJson) -> [&Vec<ModelInfo>; 12] {
    [
        &catalog.claude,
        &catalog.gemini,
        &catalog.vertex,
        &catalog.gemini_cli,
        &catalog.aistudio,
        &catalog.codex_free,
        &catalog.codex_team,
        &catalog.codex_plus,
        &catalog.codex_pro,
        &catalog.qwen,
        &catalog.kimi,
        &catalog.antigravity,
    ]
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        detect_changed_providers, models_for_channel, parse_catalog_bytes, StaticModelsJson,
    };

    #[test]
    fn loads_embedded_catalog_and_picks_codex_plan_sections() {
        let catalog =
            parse_catalog_bytes(include_bytes!("../assets/models.json")).expect("catalog");

        let team_models = models_for_channel(&catalog, "codex", Some("team"));
        let plus_models = models_for_channel(&catalog, "codex", Some("plus"));
        let pro_models = models_for_channel(&catalog, "codex", Some("pro"));

        assert!(!team_models.is_empty());
        assert!(!plus_models.is_empty());
        assert!(!pro_models.is_empty());
        assert_ne!(team_models, plus_models);
    }

    #[test]
    fn groups_codex_changes_under_single_provider_name() {
        let mut old = StaticModelsJson::default();
        old.codex_pro.push(super::ModelInfo {
            id: "gpt-5".to_string(),
            object: "model".to_string(),
            ..super::ModelInfo::default()
        });

        let mut new = old.clone();
        new.codex_plus.push(super::ModelInfo {
            id: "gpt-5-mini".to_string(),
            object: "model".to_string(),
            ..super::ModelInfo::default()
        });

        let changed = detect_changed_providers(&old, &new);
        assert_eq!(changed, vec!["codex".to_string()]);
    }
}
