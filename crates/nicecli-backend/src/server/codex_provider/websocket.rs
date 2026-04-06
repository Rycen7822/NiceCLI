use axum::extract::ws::{Message, WebSocket};
use axum::http::StatusCode;
use futures_util::StreamExt;
use nicecli_runtime::{
    CodexCallerError, ExecuteWithRetryError, RuntimeConductorError, SchedulerError,
};
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;

#[derive(Debug)]
pub(super) struct ResponsesWebsocketNormalizedRequest {
    pub(super) request: Vec<u8>,
    pub(super) last_request: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct ResponsesWebsocketError {
    pub(super) status: StatusCode,
    pub(super) message: String,
    pub(super) headers: Option<BTreeMap<String, String>>,
}

pub(super) fn normalize_responses_websocket_request(
    raw_json: &[u8],
    last_request: Option<&[u8]>,
    last_response_output: &[u8],
    allow_incremental_input_with_previous_response_id: bool,
) -> Result<ResponsesWebsocketNormalizedRequest, ResponsesWebsocketError> {
    let parsed =
        serde_json::from_slice::<JsonValue>(raw_json).map_err(|error| ResponsesWebsocketError {
            status: StatusCode::BAD_REQUEST,
            message: format!("invalid websocket request body: {error}"),
            headers: None,
        })?;
    let request_type = parsed
        .get("type")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default();

    match request_type {
        "response.create" => {
            if last_request.is_some() {
                normalize_response_subsequent_request(
                    parsed,
                    last_request.unwrap_or_default(),
                    last_response_output,
                    allow_incremental_input_with_previous_response_id,
                )
            } else {
                normalize_response_create_request(parsed)
            }
        }
        "response.append" => normalize_response_subsequent_request(
            parsed,
            last_request.unwrap_or_default(),
            last_response_output,
            allow_incremental_input_with_previous_response_id,
        ),
        _ => Err(ResponsesWebsocketError {
            status: StatusCode::BAD_REQUEST,
            message: format!("unsupported websocket request type: {request_type}"),
            headers: None,
        }),
    }
}

fn normalize_response_create_request(
    parsed: JsonValue,
) -> Result<ResponsesWebsocketNormalizedRequest, ResponsesWebsocketError> {
    let mut object = parsed.as_object().cloned().unwrap_or_default();
    object.remove("type");
    object.insert("stream".to_string(), json!(true));
    object
        .entry("input".to_string())
        .or_insert_with(|| json!([]));

    let model = object
        .get("model")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if model.is_empty() {
        return Err(ResponsesWebsocketError {
            status: StatusCode::BAD_REQUEST,
            message: "missing model in response.create request".to_string(),
            headers: None,
        });
    }

    let request = serde_json::to_vec(&JsonValue::Object(object)).unwrap_or_default();
    Ok(ResponsesWebsocketNormalizedRequest {
        last_request: request.clone(),
        request,
    })
}

fn normalize_response_subsequent_request(
    parsed: JsonValue,
    last_request: &[u8],
    last_response_output: &[u8],
    allow_incremental_input_with_previous_response_id: bool,
) -> Result<ResponsesWebsocketNormalizedRequest, ResponsesWebsocketError> {
    if last_request.is_empty() {
        return Err(ResponsesWebsocketError {
            status: StatusCode::BAD_REQUEST,
            message: "websocket request received before response.create".to_string(),
            headers: None,
        });
    }

    let next_input = parsed.get("input").cloned();
    if !next_input
        .as_ref()
        .is_some_and(|value| matches!(value, JsonValue::Array(_)))
    {
        return Err(ResponsesWebsocketError {
            status: StatusCode::BAD_REQUEST,
            message: "websocket request requires array field: input".to_string(),
            headers: None,
        });
    }

    let last_request_json = serde_json::from_slice::<JsonValue>(last_request).unwrap_or_default();
    let mut object = parsed.as_object().cloned().unwrap_or_default();
    object.remove("type");

    if allow_incremental_input_with_previous_response_id
        && object
            .get("previous_response_id")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    {
        inherit_response_request_fields(&mut object, &last_request_json);
        object.insert("stream".to_string(), json!(true));
        let request = serde_json::to_vec(&JsonValue::Object(object)).unwrap_or_default();
        return Ok(ResponsesWebsocketNormalizedRequest {
            last_request: request.clone(),
            request,
        });
    }

    let last_input = last_request_json
        .get("input")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let merged_output = parse_json_array_or_empty(last_response_output);
    let merged_next = next_input.unwrap_or_else(|| json!([]));
    let merged_input = merge_json_arrays(&[
        last_input.as_array().cloned().unwrap_or_default(),
        merged_output,
        merged_next.as_array().cloned().unwrap_or_default(),
    ]);

    object.remove("previous_response_id");
    object.insert("input".to_string(), JsonValue::Array(merged_input));
    inherit_response_request_fields(&mut object, &last_request_json);
    object.insert("stream".to_string(), json!(true));

    let request = serde_json::to_vec(&JsonValue::Object(object)).unwrap_or_default();
    Ok(ResponsesWebsocketNormalizedRequest {
        last_request: request.clone(),
        request,
    })
}

fn inherit_response_request_fields(
    object: &mut serde_json::Map<String, JsonValue>,
    last_request_json: &JsonValue,
) {
    if !object.contains_key("model") {
        if let Some(model) = last_request_json.get("model").cloned() {
            object.insert("model".to_string(), model);
        }
    }
    if !object.contains_key("instructions") {
        if let Some(instructions) = last_request_json.get("instructions").cloned() {
            object.insert("instructions".to_string(), instructions);
        }
    }
}

fn merge_json_arrays(arrays: &[Vec<JsonValue>]) -> Vec<JsonValue> {
    let mut merged = Vec::new();
    for array in arrays {
        merged.extend(array.iter().cloned());
    }
    merged
}

fn parse_json_array_or_empty(raw: &[u8]) -> Vec<JsonValue> {
    serde_json::from_slice::<JsonValue>(raw)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
}

pub(super) fn should_handle_responses_websocket_prewarm(
    raw_json: &[u8],
    has_last_request: bool,
    allow_incremental_input_with_previous_response_id: bool,
) -> bool {
    if has_last_request || allow_incremental_input_with_previous_response_id {
        return false;
    }

    serde_json::from_slice::<JsonValue>(raw_json)
        .ok()
        .and_then(|value| {
            let request_type = value.get("type").and_then(JsonValue::as_str)?.trim();
            let generate = value.get("generate").and_then(JsonValue::as_bool)?;
            Some(request_type == "response.create" && !generate)
        })
        .unwrap_or(false)
}

pub(super) fn remove_generate_flag(payload: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<JsonValue>(payload) else {
        return payload.to_vec();
    };
    if let Some(object) = value.as_object_mut() {
        object.remove("generate");
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| payload.to_vec())
}

pub(super) fn synthetic_responses_websocket_prewarm_payloads(request_json: &[u8]) -> Vec<Vec<u8>> {
    let request = serde_json::from_slice::<JsonValue>(request_json).unwrap_or_default();
    let model = request
        .get("model")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let created_at = chrono::Utc::now().timestamp();
    let response_id = format!(
        "resp_prewarm_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default()
    );

    let mut created = json!({
        "type": "response.created",
        "sequence_number": 0,
        "response": {
            "id": response_id,
            "object": "response",
            "created_at": created_at,
            "status": "in_progress",
            "background": false,
            "error": null,
            "output": [],
        }
    });
    if !model.is_empty() {
        created["response"]["model"] = json!(model);
    }

    let mut completed = json!({
        "type": "response.completed",
        "sequence_number": 1,
        "response": {
            "id": response_id,
            "object": "response",
            "created_at": created_at,
            "status": "completed",
            "background": false,
            "error": null,
            "output": [],
            "usage": {
                "input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": 0,
            }
        }
    });
    if !model.is_empty() {
        completed["response"]["model"] = json!(model);
    }

    vec![
        serde_json::to_vec(&created).unwrap_or_default(),
        serde_json::to_vec(&completed).unwrap_or_default(),
    ]
}

pub(super) async fn forward_responses_websocket_stream(
    socket: &mut WebSocket,
    response: reqwest::Response,
) -> Result<Vec<u8>, axum::Error> {
    let mut completed = false;
    let mut completed_output = b"[]".to_vec();
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => {
                buffer.extend_from_slice(&chunk);
                while let Some(event) = pop_sse_event(&mut buffer) {
                    for payload in websocket_json_payloads_from_sse_event(&event) {
                        let event_type = serde_json::from_slice::<JsonValue>(&payload)
                            .ok()
                            .and_then(|value| {
                                value
                                    .get("type")
                                    .and_then(JsonValue::as_str)
                                    .map(str::to_string)
                            })
                            .unwrap_or_default();
                        if event_type == "response.completed" {
                            completed = true;
                            completed_output = response_completed_output_from_payload(&payload);
                        }
                        send_websocket_json(socket, &payload).await?;
                    }
                }
            }
            Err(error) => {
                let _ = write_responses_websocket_error(
                    socket,
                    StatusCode::SERVICE_UNAVAILABLE,
                    error.to_string().as_str(),
                    None,
                )
                .await?;
                return Ok(completed_output);
            }
        }
    }

    if !buffer.is_empty() {
        for payload in websocket_json_payloads_from_sse_event(&buffer) {
            let event_type = serde_json::from_slice::<JsonValue>(&payload)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(JsonValue::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            if event_type == "response.completed" {
                completed = true;
                completed_output = response_completed_output_from_payload(&payload);
            }
            send_websocket_json(socket, &payload).await?;
        }
    }

    if !completed {
        let _ = write_responses_websocket_error(
            socket,
            StatusCode::REQUEST_TIMEOUT,
            "stream closed before response.completed",
            None,
        )
        .await?;
    }

    Ok(completed_output)
}

fn pop_sse_event(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let lf_boundary = buffer.windows(2).position(|window| window == b"\n\n");
    let crlf_boundary = buffer.windows(4).position(|window| window == b"\r\n\r\n");
    let boundary = match (lf_boundary, crlf_boundary) {
        (Some(lf), Some(crlf)) => Some(lf.min(crlf)),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    }?;

    let separator_len = if buffer[boundary..].starts_with(b"\r\n\r\n") {
        4
    } else {
        2
    };
    Some(buffer.drain(..boundary + separator_len).collect())
}

fn websocket_json_payloads_from_sse_event(event: &[u8]) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    for line in event.split(|byte| *byte == b'\n') {
        let mut line = line;
        if line.ends_with(b"\r") {
            line = &line[..line.len().saturating_sub(1)];
        }
        let trimmed = line
            .iter()
            .copied()
            .skip_while(|byte| byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        if trimmed.is_empty() || trimmed.starts_with(b"event:") {
            continue;
        }

        let trimmed = if trimmed.starts_with(b"data:") {
            trimmed[5..].to_vec()
        } else {
            trimmed
        };
        let trimmed = trimmed
            .into_iter()
            .skip_while(|byte| byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        if trimmed.is_empty() || trimmed == b"[DONE]" {
            continue;
        }
        if serde_json::from_slice::<JsonValue>(&trimmed).is_ok() {
            payloads.push(trimmed);
        }
    }
    payloads
}

fn response_completed_output_from_payload(payload: &[u8]) -> Vec<u8> {
    serde_json::from_slice::<JsonValue>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("response")
                .and_then(|response| response.get("output"))
                .cloned()
        })
        .and_then(|value| serde_json::to_vec(&value).ok())
        .unwrap_or_else(|| b"[]".to_vec())
}

pub(super) async fn send_websocket_json(
    socket: &mut WebSocket,
    payload: &[u8],
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(String::from_utf8_lossy(payload).into_owned()))
        .await
}

pub(super) async fn write_responses_websocket_error(
    socket: &mut WebSocket,
    status: StatusCode,
    message: &str,
    headers: Option<&BTreeMap<String, String>>,
) -> Result<Vec<u8>, axum::Error> {
    let payload = build_responses_websocket_error_payload(status, message, headers);
    send_websocket_json(socket, &payload).await?;
    Ok(payload)
}

fn build_responses_websocket_error_payload(
    status: StatusCode,
    message: &str,
    headers: Option<&BTreeMap<String, String>>,
) -> Vec<u8> {
    let trimmed = message.trim();
    let mut payload = json!({
        "type": "error",
        "status": status.as_u16(),
    });

    if let Some(headers) = headers.filter(|headers| !headers.is_empty()) {
        payload["headers"] = json!(headers);
    }

    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) {
            if let Some(error) = value.get("error").cloned() {
                payload["error"] = error;
                return serde_json::to_vec(&payload).unwrap_or_default();
            }
            payload["error"] = value;
            return serde_json::to_vec(&payload).unwrap_or_default();
        }
    }

    let (error_type, code) = match status {
        StatusCode::UNAUTHORIZED => ("authentication_error", Some("invalid_api_key")),
        StatusCode::FORBIDDEN => ("permission_error", Some("insufficient_quota")),
        StatusCode::TOO_MANY_REQUESTS => ("rate_limit_error", Some("rate_limit_exceeded")),
        StatusCode::NOT_FOUND => ("invalid_request_error", Some("model_not_found")),
        _ if status.is_server_error() => ("server_error", Some("internal_server_error")),
        _ => ("invalid_request_error", None),
    };

    let mut error = serde_json::Map::new();
    error.insert(
        "message".to_string(),
        json!(if trimmed.is_empty() {
            status.canonical_reason().unwrap_or("error")
        } else {
            trimmed
        }),
    );
    error.insert("type".to_string(), json!(error_type));
    if let Some(code) = code {
        error.insert("code".to_string(), json!(code));
    }
    payload["error"] = JsonValue::Object(error);
    serde_json::to_vec(&payload).unwrap_or_default()
}

pub(super) fn responses_websocket_error_from_codex_error(
    error: ExecuteWithRetryError<CodexCallerError>,
) -> ResponsesWebsocketError {
    match error {
        ExecuteWithRetryError::Provider(error) => match error {
            CodexCallerError::UnexpectedStatus { status, body } => ResponsesWebsocketError {
                status: StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                message: body,
                headers: None,
            },
            CodexCallerError::MissingAccessToken => ResponsesWebsocketError {
                status: StatusCode::UNAUTHORIZED,
                message: "Missing access token".to_string(),
                headers: None,
            },
            CodexCallerError::InvalidAuthFile(message) => ResponsesWebsocketError {
                status: StatusCode::UNAUTHORIZED,
                message,
                headers: None,
            },
            CodexCallerError::ReadAuthFile(error) => ResponsesWebsocketError {
                status: StatusCode::UNAUTHORIZED,
                message: error.to_string(),
                headers: None,
            },
            CodexCallerError::Request(error) => ResponsesWebsocketError {
                status: StatusCode::SERVICE_UNAVAILABLE,
                message: error.to_string(),
                headers: None,
            },
        },
        ExecuteWithRetryError::Runtime(error) => match error {
            RuntimeConductorError::Scheduler(SchedulerError::ModelCooldown {
                model,
                until,
                ..
            }) => {
                let retry_after = (until - chrono::Utc::now())
                    .num_seconds()
                    .max(0)
                    .to_string();
                let mut headers = BTreeMap::new();
                headers.insert("Retry-After".to_string(), retry_after);
                ResponsesWebsocketError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    message: format!("model cooldown for {model} until {until}"),
                    headers: Some(headers),
                }
            }
            RuntimeConductorError::Scheduler(
                SchedulerError::AuthNotFound | SchedulerError::AuthUnavailable,
            ) => ResponsesWebsocketError {
                status: StatusCode::SERVICE_UNAVAILABLE,
                message: "No auth available".to_string(),
                headers: None,
            },
            RuntimeConductorError::Scheduler(SchedulerError::NoProvider) => {
                ResponsesWebsocketError {
                    status: StatusCode::BAD_REQUEST,
                    message: "No provider supplied".to_string(),
                    headers: None,
                }
            }
            RuntimeConductorError::Store(error) => ResponsesWebsocketError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: error.to_string(),
                headers: None,
            },
            RuntimeConductorError::SelectedAuthMissing(auth_id) => ResponsesWebsocketError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!(
                    "selected auth is missing from the latest snapshot set: {auth_id}"
                ),
                headers: None,
            },
        },
    }
}
