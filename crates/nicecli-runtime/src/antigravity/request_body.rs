use super::*;
use serde_json::{Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn normalize_request_body(
    body: &[u8],
    model: &str,
    project_id: Option<&str>,
) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(root) = value.as_object_mut() else {
        return body.to_vec();
    };

    let trimmed_model = model.trim();
    let is_image_model = trimmed_model.to_ascii_lowercase().contains("image");
    root.insert(
        "model".to_string(),
        Value::String(trimmed_model.to_string()),
    );
    root.insert(
        "userAgent".to_string(),
        Value::String(DEFAULT_ANTIGRAVITY_BODY_USER_AGENT.to_string()),
    );
    root.insert(
        "requestType".to_string(),
        Value::String(if is_image_model { "image_gen" } else { "agent" }.to_string()),
    );
    root.insert(
        "project".to_string(),
        Value::String(
            project_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(generate_project_id),
        ),
    );
    root.insert(
        "requestId".to_string(),
        Value::String(if is_image_model {
            generate_image_request_id()
        } else {
            generate_request_id()
        }),
    );

    let generated_session_id = (!is_image_model).then(|| generate_session_id_from_body(body));
    let tool_config = root.remove("toolConfig");
    let request = ensure_object(root, "request");
    request.remove("safetySettings");
    if request.get("toolConfig").is_none() {
        if let Some(tool_config) = tool_config {
            request.insert("toolConfig".to_string(), tool_config);
        }
    }

    if !is_image_model && missing_session_id(request) {
        request.insert(
            "sessionId".to_string(),
            Value::String(generated_session_id.unwrap_or_else(|| format!("-{}", unique_suffix()))),
        );
    }

    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn ensure_object<'a>(root: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let should_replace = !matches!(root.get(key), Some(Value::Object(_)));
    if should_replace {
        root.insert(key.to_string(), Value::Object(Map::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("request object")
}

fn missing_session_id(request: &Map<String, Value>) -> bool {
    match request.get("sessionId") {
        Some(Value::String(value)) => value.trim().is_empty(),
        Some(_) => false,
        None => true,
    }
}

fn generate_session_id_from_body(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(text) = first_user_text(&value) {
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            return format!("-{}", hasher.finish());
        }
    }
    format!("-{}", unique_suffix())
}

fn first_user_text(value: &Value) -> Option<String> {
    value
        .get("request")
        .and_then(|value| value.get("contents"))
        .and_then(Value::as_array)
        .and_then(|contents| {
            contents.iter().find_map(|content| {
                if !content
                    .get("role")
                    .and_then(Value::as_str)
                    .is_some_and(|role| role.eq_ignore_ascii_case("user"))
                {
                    return None;
                }
                content
                    .get("parts")
                    .and_then(Value::as_array)
                    .and_then(|parts| parts.first())
                    .and_then(|part| part.get("text"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
        })
}

fn generate_request_id() -> String {
    format!("agent-{}", unique_suffix())
}

fn generate_image_request_id() -> String {
    format!(
        "image_gen/{}/{}",
        Utc::now().timestamp_millis(),
        unique_suffix()
    )
}

fn generate_project_id() -> String {
    format!("project-{}", unique_suffix())
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{counter:x}")
}
