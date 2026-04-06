use super::*;
use axum::body::Bytes;
use futures_util::{stream::try_unfold, StreamExt};
use serde_json::{json, Value as JsonValue};
use std::io;

pub(super) fn patch_public_openai_completions_request_body(
    raw_body: &[u8],
    parsed_json: Option<&JsonValue>,
    model: &str,
) -> Vec<u8> {
    let Some(JsonValue::Object(object)) = parsed_json else {
        return raw_body.to_vec();
    };

    let mut next = serde_json::Map::new();
    next.insert("model".to_string(), json!(model.trim()));
    next.insert(
        "messages".to_string(),
        json!([{
            "role": "user",
            "content": completions_prompt_text(object.get("prompt")),
        }]),
    );

    for field in [
        "max_tokens",
        "temperature",
        "top_p",
        "frequency_penalty",
        "presence_penalty",
        "stop",
        "stream",
        "logprobs",
        "top_logprobs",
        "echo",
    ] {
        copy_json_object_field(object, &mut next, field);
    }

    serde_json::to_vec(&JsonValue::Object(next)).unwrap_or_else(|_| raw_body.to_vec())
}

pub(super) fn public_openai_completions_http_response(
    mut response: ProviderHttpResponse,
) -> Response {
    response.body = convert_chat_completions_response_to_completions(&response.body);
    provider_http_response(response)
}

pub(super) fn public_openai_completions_stream_response(response: reqwest::Response) -> Response {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = response.headers().clone();
    let stream = try_unfold(
        (Box::pin(response.bytes_stream()), Vec::<u8>::new(), false),
        |(mut upstream, mut buffer, finished)| async move {
            if finished {
                return Ok::<_, io::Error>(None);
            }

            loop {
                match upstream.as_mut().next().await {
                    Some(Ok(chunk)) => {
                        buffer.extend_from_slice(&chunk);
                        let next = drain_chat_completion_sse_events(&mut buffer, false);
                        if !next.is_empty() {
                            return Ok(Some((Bytes::from(next), (upstream, buffer, false))));
                        }
                    }
                    Some(Err(error)) => return Err(io::Error::other(error)),
                    None => {
                        let next = drain_chat_completion_sse_events(&mut buffer, true);
                        if next.is_empty() {
                            return Ok(None);
                        }
                        return Ok(Some((Bytes::from(next), (upstream, Vec::new(), true))));
                    }
                }
            }
        },
    );

    let mut builder = Response::builder().status(status);
    if let Some(next_headers) = builder.headers_mut() {
        for (name, value) in &headers {
            if name.as_str().eq_ignore_ascii_case("content-length")
                || name.as_str().eq_ignore_ascii_case("transfer-encoding")
                || name.as_str().eq_ignore_ascii_case("connection")
            {
                continue;
            }
            next_headers.insert(name.clone(), value.clone());
        }
        next_headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    }

    builder.body(Body::from_stream(stream)).unwrap_or_else(|_| {
        openai_error_response(StatusCode::BAD_GATEWAY, "Failed to build response")
    })
}

fn copy_json_object_field(
    source: &serde_json::Map<String, JsonValue>,
    target: &mut serde_json::Map<String, JsonValue>,
    field: &str,
) {
    if let Some(value) = source.get(field) {
        target.insert(field.to_string(), value.clone());
    }
}

fn completions_prompt_text(prompt: Option<&JsonValue>) -> String {
    match prompt {
        Some(JsonValue::String(value)) if !value.is_empty() => value.clone(),
        Some(JsonValue::String(_)) | None | Some(JsonValue::Null) => "Complete this:".to_string(),
        Some(value) => {
            let text = json_value_to_string(value);
            if text.is_empty() {
                "Complete this:".to_string()
            } else {
                text
            }
        }
    }
}

fn json_value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => String::new(),
        JsonValue::String(value) => value.clone(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::Array(_) | JsonValue::Object(_) => value.to_string(),
    }
}

fn convert_chat_completions_response_to_completions(raw_json: &[u8]) -> Vec<u8> {
    let Ok(root) = serde_json::from_slice::<JsonValue>(raw_json) else {
        return raw_json.to_vec();
    };

    let choices = root
        .get("choices")
        .and_then(JsonValue::as_array)
        .map(|choices| {
            choices
                .iter()
                .map(convert_chat_completion_choice_to_completions)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut payload = serde_json::Map::new();
    payload.insert(
        "id".to_string(),
        json!(root.get("id").map(json_value_to_string).unwrap_or_default()),
    );
    payload.insert("object".to_string(), json!("text_completion"));
    payload.insert(
        "created".to_string(),
        root.get("created").cloned().unwrap_or_else(|| json!(0)),
    );
    payload.insert(
        "model".to_string(),
        json!(root
            .get("model")
            .map(json_value_to_string)
            .unwrap_or_default()),
    );
    payload.insert("choices".to_string(), JsonValue::Array(choices));
    if let Some(usage) = root.get("usage") {
        payload.insert("usage".to_string(), usage.clone());
    }

    serde_json::to_vec(&JsonValue::Object(payload)).unwrap_or_else(|_| raw_json.to_vec())
}

fn convert_chat_completion_choice_to_completions(choice: &JsonValue) -> JsonValue {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "index".to_string(),
        choice.get("index").cloned().unwrap_or_else(|| json!(0)),
    );
    payload.insert(
        "text".to_string(),
        json!(extract_chat_completion_text(choice).unwrap_or_default()),
    );
    if let Some(finish_reason) = choice.get("finish_reason") {
        payload.insert(
            "finish_reason".to_string(),
            json!(json_value_to_string(finish_reason)),
        );
    }
    if let Some(logprobs) = choice.get("logprobs") {
        payload.insert("logprobs".to_string(), logprobs.clone());
    }
    JsonValue::Object(payload)
}

fn extract_chat_completion_text(choice: &JsonValue) -> Option<String> {
    if let Some(message) = choice.get("message").and_then(JsonValue::as_object) {
        if let Some(content) = message.get("content") {
            return Some(json_value_to_string(content));
        }
    }

    choice
        .get("delta")
        .and_then(JsonValue::as_object)
        .and_then(|delta| delta.get("content"))
        .map(json_value_to_string)
}

fn drain_chat_completion_sse_events(buffer: &mut Vec<u8>, flush: bool) -> Vec<u8> {
    let mut output = Vec::new();

    while let Some((event_end, separator_len)) = find_sse_event_separator(buffer) {
        let event = buffer[..event_end].to_vec();
        buffer.drain(..event_end + separator_len);
        if let Some(transformed) = transform_chat_completion_sse_event_to_completions(&event) {
            output.extend_from_slice(&transformed);
        }
    }

    if flush && !buffer.is_empty() {
        let event = std::mem::take(buffer);
        if let Some(transformed) = transform_chat_completion_sse_event_to_completions(&event) {
            output.extend_from_slice(&transformed);
        }
    }

    output
}

fn find_sse_event_separator(buffer: &[u8]) -> Option<(usize, usize)> {
    let newline = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));

    match (newline, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn transform_chat_completion_sse_event_to_completions(event: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(event).ok()?;
    let mut lines = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        if let Some(data) = line.strip_prefix("data:") {
            let payload = data.trim_start();
            if payload == "[DONE]" {
                lines.push("data: [DONE]".to_string());
                continue;
            }

            let converted =
                convert_chat_completions_stream_chunk_to_completions(payload.as_bytes())?;
            lines.push(format!("data: {}", String::from_utf8_lossy(&converted)));
            continue;
        }

        lines.push(line.to_string());
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("{}\n\n", lines.join("\n")).into_bytes())
    }
}

fn convert_chat_completions_stream_chunk_to_completions(chunk_data: &[u8]) -> Option<Vec<u8>> {
    let root = serde_json::from_slice::<JsonValue>(chunk_data).ok()?;
    let choices = root.get("choices").and_then(JsonValue::as_array)?;
    let has_usage = root.get("usage").is_some();
    let has_content = choices
        .iter()
        .any(chat_completion_chunk_has_meaningful_content);

    if !has_content && !has_usage {
        return None;
    }

    let completions_choices = choices
        .iter()
        .map(convert_chat_completion_stream_choice_to_completions)
        .collect::<Vec<_>>();

    let mut payload = serde_json::Map::new();
    payload.insert(
        "id".to_string(),
        json!(root.get("id").map(json_value_to_string).unwrap_or_default()),
    );
    payload.insert("object".to_string(), json!("text_completion"));
    payload.insert(
        "created".to_string(),
        root.get("created").cloned().unwrap_or_else(|| json!(0)),
    );
    payload.insert(
        "model".to_string(),
        json!(root
            .get("model")
            .map(json_value_to_string)
            .unwrap_or_default()),
    );
    payload.insert("choices".to_string(), JsonValue::Array(completions_choices));
    if let Some(usage) = root.get("usage") {
        payload.insert("usage".to_string(), usage.clone());
    }

    serde_json::to_vec(&JsonValue::Object(payload)).ok()
}

fn chat_completion_chunk_has_meaningful_content(choice: &JsonValue) -> bool {
    if choice
        .get("delta")
        .and_then(JsonValue::as_object)
        .and_then(|delta| delta.get("content"))
        .map(json_value_to_string)
        .is_some_and(|content| !content.is_empty())
    {
        return true;
    }

    choice
        .get("finish_reason")
        .map(json_value_to_string)
        .is_some_and(|finish_reason| !finish_reason.is_empty())
}

fn convert_chat_completion_stream_choice_to_completions(choice: &JsonValue) -> JsonValue {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "index".to_string(),
        choice.get("index").cloned().unwrap_or_else(|| json!(0)),
    );
    payload.insert(
        "text".to_string(),
        json!(choice
            .get("delta")
            .and_then(JsonValue::as_object)
            .and_then(|delta| delta.get("content"))
            .map(json_value_to_string)
            .unwrap_or_default()),
    );
    if let Some(finish_reason) = choice.get("finish_reason") {
        payload.insert(
            "finish_reason".to_string(),
            json!(json_value_to_string(finish_reason)),
        );
    }
    if let Some(logprobs) = choice.get("logprobs") {
        payload.insert("logprobs".to_string(), logprobs.clone());
    }
    JsonValue::Object(payload)
}
