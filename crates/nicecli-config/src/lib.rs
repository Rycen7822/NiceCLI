use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use serde_yaml::Value as SerdeYamlValue;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use yaml_edit::{path::YamlPath, Document as LosslessDocument, Scalar as LosslessScalar, YamlFile};

pub const DEFAULT_PORT: u16 = 8317;
pub const DEFAULT_LOCAL_HOST: &str = "127.0.0.1";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file does not exist: {path}")]
    Missing { path: String },
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("failed to parse config file {path} losslessly: {message}")]
    LosslessParse { path: String, message: String },
    #[error("failed to convert config file {path} to JSON: {source}")]
    JsonEncode {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to encode config file {path}: {source}")]
    YamlEncode {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("failed to write config file {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid config path: {path}")]
    InvalidPath { path: String },
    #[error("config root must be a YAML mapping: {path}")]
    InvalidRoot { path: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct NiceCliConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    #[serde(rename = "auth-dir")]
    pub auth_dir: Option<String>,
    #[serde(rename = "proxy-url")]
    pub proxy_url: Option<String>,
    pub debug: bool,
    #[serde(rename = "logging-to-file")]
    pub logging_to_file: bool,
    #[serde(rename = "usage-statistics-enabled")]
    pub usage_statistics_enabled: bool,
    #[serde(rename = "request-retry")]
    pub request_retry: Option<u32>,
    #[serde(rename = "max-retry-interval")]
    pub max_retry_interval: Option<u32>,
    pub routing: RoutingConfig,
    #[serde(rename = "ws-auth")]
    pub ws_auth: bool,
}

impl NiceCliConfig {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_yaml_str(path.display().to_string(), &raw)
    }

    pub fn from_yaml_str(path: impl Into<String>, raw: &str) -> Result<Self, ConfigError> {
        serde_yaml::from_str(raw).map_err(|source| ConfigError::Parse {
            path: path.into(),
            source,
        })
    }

    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(DEFAULT_PORT)
    }

    pub fn effective_host(&self) -> &str {
        self.host
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_LOCAL_HOST)
    }
}

pub fn load_config_json(path: impl AsRef<Path>) -> Result<JsonValue, ConfigError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(json!({}));
    }

    let document = read_yaml_document(path)?;
    serde_json::to_value(document).map_err(|source| ConfigError::JsonEncode {
        path: path.display().to_string(),
        source,
    })
}

pub fn update_config_value(
    path: impl AsRef<Path>,
    endpoint: &str,
    value: &JsonValue,
    is_delete: bool,
) -> Result<(), ConfigError> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let normalized_endpoint = normalize_endpoint_path(endpoint)?;
    let path_label = path.display().to_string();

    if !is_delete && normalized_endpoint.contains('.') {
        let config_json = serde_json::to_value(read_yaml_document(path)?).map_err(|source| {
            ConfigError::JsonEncode {
                path: path_label.clone(),
                source,
            }
        })?;
        let root_key = normalized_endpoint
            .split('.')
            .next()
            .expect("normalized endpoint should have at least one segment");
        if config_json.get(root_key).is_none() {
            let mut rewritten_json = config_json;
            set_json_value_at_path(
                &mut rewritten_json,
                &normalized_endpoint,
                value.clone(),
                &path_label,
            )?;
            let root_value = rewritten_json
                .get(root_key)
                .cloned()
                .unwrap_or_else(|| JsonValue::Object(serde_json::Map::new()));
            let rewritten =
                replace_top_level_complex_value(&raw, root_key, &path_label, &root_value)?;
            return fs::write(path, rewritten).map_err(|source| ConfigError::Write {
                path: path_label,
                source,
            });
        }
    }

    if !is_delete
        && !normalized_endpoint.contains('.')
        && matches!(value, JsonValue::Array(_) | JsonValue::Object(_))
    {
        let rewritten =
            replace_top_level_complex_value(&raw, &normalized_endpoint, &path_label, value)?;
        return fs::write(path, rewritten).map_err(|source| ConfigError::Write {
            path: path_label,
            source,
        });
    }

    let yaml = parse_lossless_yaml(path, &raw)?;
    let document = yaml.ensure_document();
    if document.as_mapping().is_none() {
        return Err(ConfigError::InvalidRoot {
            path: path.display().to_string(),
        });
    }

    if is_delete {
        document.remove_path(&normalized_endpoint);
    } else {
        let scalar = parse_lossless_scalar(value, &path_label)?;
        document.set_path(&normalized_endpoint, scalar);
    }

    let rewritten = yaml.to_string();
    if !is_delete && normalized_endpoint.contains('.') {
        let maybe_json = serde_yaml::from_str::<SerdeYamlValue>(&rewritten)
            .ok()
            .and_then(|value| serde_json::to_value(value).ok());
        let write_scalar_fallback = maybe_json
            .as_ref()
            .and_then(|json| config_json_value(json, &normalized_endpoint))
            .is_none();

        if write_scalar_fallback {
            let current_json =
                serde_json::to_value(read_yaml_document(path)?).map_err(|source| {
                    ConfigError::JsonEncode {
                        path: path_label.clone(),
                        source,
                    }
                })?;
            let fallback = rewrite_nested_scalar_via_top_level_replace(
                &raw,
                current_json,
                &normalized_endpoint,
                value.clone(),
                &path_label,
            )?;
            return fs::write(path, fallback).map_err(|source| ConfigError::Write {
                path: path_label,
                source,
            });
        }
    }

    fs::write(path, rewritten).map_err(|source| ConfigError::Write {
        path: path_label,
        source,
    })
}

pub fn set_proxy_url_override(
    path: impl AsRef<Path>,
    proxy_url: Option<&str>,
) -> Result<(), ConfigError> {
    let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };

    update_config_value(
        path,
        "proxy-url",
        &JsonValue::String(proxy_url.to_string()),
        false,
    )
}

fn read_yaml_document(path: &Path) -> Result<SerdeYamlValue, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::Missing {
            path: path.display().to_string(),
        });
    }

    let raw = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;

    serde_yaml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.display().to_string(),
        source,
    })
}

fn parse_lossless_yaml(path: &Path, raw: &str) -> Result<YamlFile, ConfigError> {
    if raw.trim().is_empty() {
        return Ok(YamlFile::new());
    }

    YamlFile::from_str(raw).map_err(|error| ConfigError::LosslessParse {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

fn normalize_endpoint_path(endpoint: &str) -> Result<String, ConfigError> {
    let parts: Vec<&str> = endpoint
        .split(['.', '/'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();

    if parts.is_empty() {
        return Err(ConfigError::InvalidPath {
            path: endpoint.to_string(),
        });
    }

    Ok(parts.join("."))
}

fn set_json_value_at_path(
    root: &mut JsonValue,
    endpoint: &str,
    value: JsonValue,
    path_label: &str,
) -> Result<(), ConfigError> {
    let mut segments = endpoint.split('.').peekable();
    let Some(first_segment) = segments.next() else {
        return Err(ConfigError::InvalidPath {
            path: endpoint.to_string(),
        });
    };

    let mut current = root
        .as_object_mut()
        .ok_or_else(|| ConfigError::InvalidRoot {
            path: path_label.to_string(),
        })?;

    let mut segment = first_segment;
    for next_segment in segments {
        let entry = current
            .entry(segment.to_string())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
        if !entry.is_object() {
            *entry = JsonValue::Object(serde_json::Map::new());
        }
        current = entry.as_object_mut().expect("entry forced to object");
        segment = next_segment;
    }

    current.insert(segment.to_string(), value);
    Ok(())
}

fn json_to_lossless_yaml_document(
    path_label: &str,
    value: &JsonValue,
) -> Result<LosslessDocument, ConfigError> {
    let raw = serde_yaml::to_string(value).map_err(|source| ConfigError::YamlEncode {
        path: path_label.to_string(),
        source,
    })?;

    LosslessDocument::from_str(&raw).map_err(|error| ConfigError::LosslessParse {
        path: path_label.to_string(),
        message: error.to_string(),
    })
}

fn parse_lossless_scalar(
    value: &JsonValue,
    path_label: &str,
) -> Result<LosslessScalar, ConfigError> {
    let document = json_to_lossless_yaml_document(path_label, value)?;
    document
        .as_scalar()
        .ok_or_else(|| ConfigError::LosslessParse {
            path: path_label.to_string(),
            message: "generated YAML value did not produce a scalar root".to_string(),
        })
}

fn replace_top_level_complex_value(
    raw: &str,
    key: &str,
    path_label: &str,
    value: &JsonValue,
) -> Result<String, ConfigError> {
    let replacement = render_top_level_complex_value(key, path_label, value)?;
    if let Some((start, end)) = find_top_level_key_block(raw, key) {
        let mut rewritten = String::with_capacity(raw.len() - (end - start) + replacement.len());
        rewritten.push_str(&raw[..start]);
        rewritten.push_str(&replacement);
        rewritten.push_str(&raw[end..]);
        return Ok(rewritten);
    }

    if raw.is_empty() {
        return Ok(replacement);
    }

    let mut rewritten = raw.to_string();
    if !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    rewritten.push_str(&replacement);
    Ok(rewritten)
}

fn render_top_level_complex_value(
    key: &str,
    path_label: &str,
    value: &JsonValue,
) -> Result<String, ConfigError> {
    let raw = serde_yaml::to_string(value).map_err(|source| ConfigError::YamlEncode {
        path: path_label.to_string(),
        source,
    })?;
    let rendered_value = strip_yaml_document_marker(&raw).trim_end();

    if !matches!(value, JsonValue::Array(_) | JsonValue::Object(_))
        && !rendered_value.contains('\n')
    {
        return Ok(format!("{key}: {rendered_value}\n"));
    }

    let mut rendered = String::new();
    rendered.push_str(key);
    rendered.push_str(":\n");
    for line in rendered_value.lines() {
        rendered.push_str("  ");
        rendered.push_str(line);
        rendered.push('\n');
    }
    Ok(rendered)
}

fn rewrite_nested_scalar_via_top_level_replace(
    raw: &str,
    mut config_json: JsonValue,
    endpoint: &str,
    value: JsonValue,
    path_label: &str,
) -> Result<String, ConfigError> {
    set_json_value_at_path(&mut config_json, endpoint, value, path_label)?;
    let root_key = endpoint
        .split('.')
        .next()
        .ok_or_else(|| ConfigError::InvalidPath {
            path: endpoint.to_string(),
        })?;
    let root_value = config_json
        .get(root_key)
        .cloned()
        .unwrap_or_else(|| JsonValue::Object(serde_json::Map::new()));
    replace_top_level_complex_value(raw, root_key, path_label, &root_value)
}

fn strip_yaml_document_marker(raw: &str) -> &str {
    raw.strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
        .unwrap_or(raw)
}

fn find_top_level_key_block(raw: &str, key: &str) -> Option<(usize, usize)> {
    let mut line_start = 0usize;
    let mut block_start = None;
    let mut block_end = raw.len();

    while line_start < raw.len() {
        let line_end = raw[line_start..]
            .find('\n')
            .map(|offset| line_start + offset + 1)
            .unwrap_or(raw.len());
        let line = raw[line_start..line_end]
            .trim_end_matches('\n')
            .trim_end_matches('\r');

        if block_start.is_none() {
            if is_top_level_key_line(line, key) {
                block_start = Some(line_start);
            }
        } else if is_new_top_level_section(line) {
            block_end = line_start;
            break;
        }

        line_start = line_end;
    }

    block_start.map(|start| (start, block_end))
}

fn config_json_value<'a>(value: &'a JsonValue, config_path: &str) -> Option<&'a JsonValue> {
    let mut current = value;
    for segment in config_path
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        current = current.get(segment)?;
    }
    Some(current)
}

fn is_top_level_key_line(line: &str, key: &str) -> bool {
    let trimmed = line.trim_end();
    trimmed
        .strip_prefix(key)
        .is_some_and(|rest| rest.starts_with(':'))
        && !line.starts_with(' ')
        && !line.starts_with('\t')
}

fn is_new_top_level_section(line: &str) -> bool {
    let trimmed = line.trim_end();
    !trimmed.is_empty() && !line.starts_with(' ') && !line.starts_with('\t')
}

#[cfg(test)]
mod tests {
    use super::{
        load_config_json, set_proxy_url_override, update_config_value, NiceCliConfig,
        DEFAULT_LOCAL_HOST, DEFAULT_PORT,
    };
    use serde_json::{json, Value as JsonValue};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parses_current_yaml_keys() {
        let config = NiceCliConfig::from_yaml_str(
            "inline",
            r#"
port: 9000
auth-dir: C:/auth
proxy-url: http://127.0.0.1:7890
debug: true
logging-to-file: true
usage-statistics-enabled: true
request-retry: 5
max-retry-interval: 45
routing:
  strategy: fill-first
ws-auth: true
"#,
        )
        .expect("config should parse");

        assert_eq!(config.port, Some(9000));
        assert_eq!(config.auth_dir.as_deref(), Some("C:/auth"));
        assert_eq!(config.proxy_url.as_deref(), Some("http://127.0.0.1:7890"));
        assert!(config.debug);
        assert!(config.logging_to_file);
        assert!(config.usage_statistics_enabled);
        assert_eq!(config.request_retry, Some(5));
        assert_eq!(config.max_retry_interval, Some(45));
        assert_eq!(config.routing.strategy.as_deref(), Some("fill-first"));
        assert!(config.ws_auth);
    }

    #[test]
    fn exposes_effective_defaults() {
        let config = NiceCliConfig::default();
        assert_eq!(config.effective_port(), DEFAULT_PORT);
        assert_eq!(config.effective_host(), DEFAULT_LOCAL_HOST);
    }

    #[test]
    fn loads_config_as_json() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"
port: 8317
routing:
  strategy: round-robin
"#,
        )
        .expect("write config");

        let json = load_config_json(&path).expect("json config");
        assert_eq!(json["port"], 8317);
        assert_eq!(json["routing"]["strategy"], "round-robin");
    }

    #[test]
    fn updates_nested_yaml_value() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"
port: 8317
routing:
  strategy: round-robin
"#,
        )
        .expect("write config");

        update_config_value(&path, "routing.strategy", &json!("fill-first"), false)
            .expect("update config");

        let config = NiceCliConfig::load_from_path(&path).expect("reload config");
        assert_eq!(config.routing.strategy.as_deref(), Some("fill-first"));
    }

    #[test]
    fn deletes_existing_yaml_value() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"
proxy-url: http://127.0.0.1:7890
"#,
        )
        .expect("write config");

        update_config_value(&path, "proxy-url", &JsonValue::Null, true).expect("delete config");

        let json = load_config_json(&path).expect("json config");
        assert!(json.get("proxy-url").is_none());
    }

    #[test]
    fn applies_proxy_url_override() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(&path, "port: 8317\n").expect("write config");

        set_proxy_url_override(&path, Some(" http://127.0.0.1:7890 ")).expect("set proxy override");

        let config = NiceCliConfig::load_from_path(&path).expect("reload config");
        assert_eq!(config.proxy_url.as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn preserves_comments_and_unknown_fields_when_updating_nested_value() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"# keep-this-comment
proxy-url: http://127.0.0.1:7890
routing:
  # keep-routing-comment
  strategy: round-robin
unknown-key:
  child: 1
"#,
        )
        .expect("write config");

        update_config_value(&path, "routing.strategy", &json!("fill-first"), false)
            .expect("update config");

        let rewritten = fs::read_to_string(&path).expect("read rewritten config");
        assert!(rewritten.contains("# keep-this-comment"));
        assert!(rewritten.contains("# keep-routing-comment"));
        assert!(rewritten.contains("unknown-key:"));
        assert!(rewritten.contains("child: 1"));
        assert!(rewritten.contains("strategy: fill-first"));
    }

    #[test]
    fn normalizes_slash_delimited_paths_to_nested_yaml_keys() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"# keep-header
quota-exceeded:
  # keep-switch-comment
  switch-project: false
"#,
        )
        .expect("write config");

        update_config_value(&path, "quota-exceeded/switch-project", &json!(true), false)
            .expect("update config");

        let json = load_config_json(&path).expect("json config");
        assert_eq!(json["quota-exceeded"]["switch-project"], true);

        let rewritten = fs::read_to_string(&path).expect("read rewritten config");
        assert!(rewritten.contains("# keep-header"));
        assert!(rewritten.contains("# keep-switch-comment"));
        assert!(!rewritten.contains("quota-exceeded/switch-project"));
    }

    #[test]
    fn creates_missing_top_level_mapping_for_nested_scalar_updates() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"# keep-header
host: 127.0.0.1
port: 8317
"#,
        )
        .expect("write config");

        update_config_value(
            &path,
            "ampcode/upstream-url",
            &json!("https://amp.example.com"),
            false,
        )
        .expect("update config");

        let json = load_config_json(&path).expect("json config");
        assert_eq!(json["ampcode"]["upstream-url"], "https://amp.example.com");

        let rewritten = fs::read_to_string(&path).expect("read rewritten config");
        assert!(rewritten.contains("# keep-header"));
        assert!(rewritten.contains("ampcode:"));
        assert!(rewritten.contains("upstream-url: https://amp.example.com"));
    }

    #[test]
    fn deletes_yaml_key_without_dropping_neighbor_comments() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"# keep-header
proxy-url: http://127.0.0.1:7890
# keep-port-comment
port: 8317
"#,
        )
        .expect("write config");

        update_config_value(&path, "proxy-url", &JsonValue::Null, true).expect("delete config");

        let json = load_config_json(&path).expect("json config");
        assert!(json.get("proxy-url").is_none());
        assert_eq!(json["port"], 8317);

        let rewritten = fs::read_to_string(&path).expect("read rewritten config");
        assert!(rewritten.contains("# keep-header"));
        assert!(rewritten.contains("# keep-port-comment"));
    }

    #[test]
    fn updates_sequence_of_mappings_without_dropping_top_level_comments() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"# keep-openai-comment
openai-compatibility:
  - name: demo
    base-url: https://example.com
    api-key-entries:
      - api-key: old-key
"#,
        )
        .expect("write config");

        update_config_value(
            &path,
            "openai-compatibility",
            &json!([
                {
                    "name": "demo",
                    "base-url": "https://example.com",
                    "api-key-entries": [
                        { "api-key": "new-key", "proxy-url": "http://127.0.0.1:7890" }
                    ],
                    "models": [
                        { "name": "gpt-5", "alias": "demo-gpt-5" }
                    ]
                }
            ]),
            false,
        )
        .expect("update config");

        let json = load_config_json(&path).expect("json config");
        assert_eq!(json["openai-compatibility"][0]["name"], "demo");
        assert_eq!(
            json["openai-compatibility"][0]["api-key-entries"][0]["api-key"],
            "new-key"
        );
        assert_eq!(
            json["openai-compatibility"][0]["api-key-entries"][0]["proxy-url"],
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            json["openai-compatibility"][0]["models"][0]["alias"],
            "demo-gpt-5"
        );

        let rewritten = fs::read_to_string(&path).expect("read rewritten config");
        assert!(rewritten.contains("# keep-openai-comment"));
    }
}
