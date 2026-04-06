use nicecli_backend::{AuthFileEntry, SnapshotListResponse};
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

#[derive(Debug, Deserialize)]
struct AuthFilesResponse {
    files: Vec<AuthFileEntry>,
}

pub(crate) fn normalize_oauth_auth_url_response(
    payload: Value,
    dynamic_query_keys: &[&str],
) -> Value {
    let url = payload["url"].as_str().expect("auth url");
    let parsed = Url::parse(url).expect("valid auth url");
    let mut stable_query = serde_json::Map::new();
    let mut dynamic_keys_present = Vec::new();

    for (key, value) in parsed.query_pairs() {
        if dynamic_query_keys.iter().any(|candidate| *candidate == key) {
            dynamic_keys_present.push(key.into_owned());
        } else {
            stable_query.insert(key.into_owned(), Value::String(value.into_owned()));
        }
    }

    dynamic_keys_present.sort();

    json!({
        "status": payload["status"],
        "state_present": payload["state"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
        "url": {
            "scheme": parsed.scheme(),
            "host": parsed.host_str().unwrap_or_default(),
            "path": parsed.path(),
            "stable_query": stable_query,
            "dynamic_query_keys": dynamic_keys_present,
        }
    })
}

pub(crate) fn normalize_auth_files_response(payload: Value) -> Value {
    let payload: AuthFilesResponse = serde_json::from_value(payload).expect("auth files");
    json!({
        "files": payload.files.into_iter().map(|file| json!({
            "id": file.id,
            "name": file.name,
            "type": file.provider_type,
            "provider": file.provider,
            "source": file.source,
            "email": file.email,
            "note": file.note,
            "priority": file.priority
        })).collect::<Vec<_>>()
    })
}

pub(crate) fn normalize_codex_auth_file(payload: Value) -> Value {
    json!({
        "provider": payload["provider"],
        "type": payload["type"],
        "email": payload["email"],
        "account_id": payload["account_id"],
        "account_plan": payload["account_plan"],
        "access_token": payload["access_token"],
        "refresh_token": payload["refresh_token"],
        "id_token_present": payload["id_token"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
        "expired_present": payload["expired"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
        "note": payload["note"],
    })
}

pub(crate) fn normalize_qwen_auth_file(file_name: &str, payload: Value) -> Value {
    let email = payload["email"].as_str().unwrap_or_default();
    json!({
        "file_name_prefix_ok": file_name.starts_with("qwen-"),
        "file_name_suffix_ok": file_name.ends_with(".json"),
        "id_matches_file_name": payload["id"].as_str() == Some(file_name),
        "provider": payload["provider"],
        "type": payload["type"],
        "email_present": !email.trim().is_empty(),
        "email_is_digits": email.chars().all(|ch| ch.is_ascii_digit()),
        "access_token": payload["access_token"],
        "refresh_token": payload["refresh_token"],
        "resource_url": payload["resource_url"],
        "last_refresh_present": payload["last_refresh"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
        "expired_present": payload["expired"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
    })
}

pub(crate) fn normalize_anthropic_auth_file(file_name: &str, payload: Value) -> Value {
    json!({
        "file_name": file_name,
        "id_matches_file_name": payload["id"].as_str() == Some(file_name),
        "provider": payload["provider"],
        "type": payload["type"],
        "email": payload["email"],
        "access_token": payload["access_token"],
        "refresh_token": payload["refresh_token"],
        "account_uuid": payload["account_uuid"],
        "organization_uuid": payload["organization_uuid"],
        "organization_name": payload["organization_name"],
        "last_refresh_present": payload["last_refresh"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
        "expired_present": payload["expired"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
    })
}

pub(crate) fn normalize_gemini_cli_auth_file(file_name: &str, payload: Value) -> Value {
    let token = payload["token"].as_object();
    json!({
        "file_name": file_name,
        "id_matches_file_name": payload["id"].as_str() == Some(file_name),
        "provider": payload["provider"],
        "type": payload["type"],
        "email": payload["email"],
        "project_id": payload["project_id"],
        "auto": payload["auto"],
        "checked": payload["checked"],
        "access_token": payload["access_token"],
        "refresh_token": payload["refresh_token"],
        "token_type": payload["token_type"],
        "token_present": token.is_some(),
        "token_refresh_token_matches": token.and_then(|value| value.get("refresh_token")) == Some(&payload["refresh_token"]),
        "token_client_id_present": token.and_then(|value| value.get("client_id")).and_then(Value::as_str).map(|value| !value.trim().is_empty()).unwrap_or(false),
        "token_expiry_present": token.and_then(|value| value.get("expiry")).and_then(Value::as_str).map(|value| !value.trim().is_empty()).unwrap_or(false),
        "expiry_present": payload["expiry"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
    })
}

pub(crate) fn normalize_antigravity_auth_file(file_name: &str, payload: Value) -> Value {
    json!({
        "file_name": file_name,
        "id_matches_file_name": payload["id"].as_str() == Some(file_name),
        "provider": payload["provider"],
        "type": payload["type"],
        "email": payload["email"],
        "project_id": payload["project_id"],
        "access_token": payload["access_token"],
        "refresh_token": payload["refresh_token"],
        "expires_in": payload["expires_in"],
        "timestamp_present": payload["timestamp"].as_i64().map(|value| value > 0).unwrap_or(false),
        "expired_present": payload["expired"].as_str().map(|value| !value.trim().is_empty()).unwrap_or(false),
    })
}

pub(crate) fn normalize_gemini_web_auth_file(file_name: &str, payload: Value) -> Value {
    json!({
        "file_name": file_name,
        "id_matches_file_name": payload["id"].as_str() == Some(file_name),
        "provider": payload["provider"],
        "type": payload["type"],
        "auth_mode": payload["auth_mode"],
        "email": payload["email"],
        "label": payload["label"],
        "cookie": payload["cookie"],
        "secure_1psid": payload["secure_1psid"],
        "secure_1psidts": payload["secure_1psidts"],
    })
}

pub(crate) fn normalize_vertex_auth_file(file_name: &str, payload: Value) -> Value {
    let service_account = payload["service_account"].as_object();
    json!({
        "file_name": file_name,
        "id_matches_file_name": payload["id"].as_str() == Some(file_name),
        "provider": payload["provider"],
        "type": payload["type"],
        "project_id": payload["project_id"],
        "email": payload["email"],
        "location": payload["location"],
        "label": payload["label"],
        "service_account_project_id": service_account.and_then(|value| value.get("project_id")),
        "service_account_client_email": service_account.and_then(|value| value.get("client_email")),
        "service_account_private_key": service_account.and_then(|value| value.get("private_key")),
    })
}

pub(crate) fn normalize_quota_response(payload: Value) -> Value {
    let payload: SnapshotListResponse = serde_json::from_value(payload).expect("quota payload");
    json!({
        "provider": payload.provider,
        "snapshots": payload.snapshots.into_iter().map(|entry| {
            let snapshot = entry.snapshot;
            json!({
                "provider": entry.provider,
                "auth_id": entry.auth_id,
                "auth_file_name": entry.auth_file_name,
                "account_email": entry.account_email,
                "account_plan": entry.account_plan,
                "source": entry.source,
                "stale": entry.stale,
                "error": entry.error,
                "snapshot": snapshot.as_ref().map(|snapshot| json!({
                    "plan_type": snapshot.plan_type,
                    "primary": snapshot.primary.as_ref().map(|window| json!({
                        "used_percent": window.used_percent,
                        "window_minutes": window.window_minutes,
                        "resets_at": window.resets_at
                    })),
                    "credits": snapshot.credits.as_ref().map(|credits| json!({
                        "has_credits": credits.has_credits,
                        "unlimited": credits.unlimited,
                        "balance": credits.balance
                    }))
                }))
            })
        }).collect::<Vec<_>>()
    })
}

pub(crate) fn normalize_quota_filtered_response(payload: Value) -> Value {
    let payload: SnapshotListResponse = serde_json::from_value(payload).expect("quota payload");
    json!({
        "provider": payload.provider,
        "snapshots": payload.snapshots.into_iter().map(|entry| {
            let snapshot = entry.snapshot;
            json!({
                "provider": entry.provider,
                "auth_id": entry.auth_id,
                "auth_file_name": entry.auth_file_name,
                "account_email": entry.account_email,
                "account_plan": entry.account_plan,
                "workspace_id": entry.workspace_id,
                "workspace_name": entry.workspace_name,
                "workspace_type": entry.workspace_type,
                "source": entry.source,
                "stale": entry.stale,
                "error": entry.error,
                "snapshot": snapshot.as_ref().map(|snapshot| json!({
                    "plan_type": snapshot.plan_type,
                    "primary": snapshot.primary.as_ref().map(|window| json!({
                        "used_percent": window.used_percent,
                        "window_minutes": window.window_minutes,
                        "resets_at": window.resets_at
                    })),
                    "credits": snapshot.credits.as_ref().map(|credits| json!({
                        "has_credits": credits.has_credits,
                        "unlimited": credits.unlimited,
                        "balance": credits.balance
                    }))
                }))
            })
        }).collect::<Vec<_>>()
    })
}

pub(crate) fn normalize_quota_metadata_response(payload: Value) -> Value {
    let payload: SnapshotListResponse = serde_json::from_value(payload).expect("quota payload");
    json!({
        "provider": payload.provider,
        "snapshots": payload.snapshots.into_iter().map(|entry| {
            let snapshot = entry.snapshot;
            json!({
                "auth_id": entry.auth_id,
                "auth_label": entry.auth_label,
                "auth_note": entry.auth_note,
                "auth_file_name": entry.auth_file_name,
                "account_email": entry.account_email,
                "account_plan": entry.account_plan,
                "workspace_id": entry.workspace_id,
                "workspace_name": entry.workspace_name,
                "workspace_type": entry.workspace_type,
                "stale": entry.stale,
                "error": entry.error,
                "snapshot": snapshot.as_ref().map(|snapshot| json!({
                    "plan_type": snapshot.plan_type,
                    "primary": snapshot.primary.as_ref().map(|window| json!({
                        "used_percent": window.used_percent,
                        "window_minutes": window.window_minutes,
                        "resets_at": window.resets_at
                    }))
                }))
            })
        }).collect::<Vec<_>>()
    })
}

pub(crate) fn normalize_quota_go_common_response(payload: Value) -> Value {
    let payload: SnapshotListResponse = serde_json::from_value(payload).expect("quota payload");
    strip_null_fields(json!({
        "provider": payload.provider,
        "snapshots": payload.snapshots.into_iter().map(|entry| {
            let snapshot = entry.snapshot;
            json!({
                "provider": entry.provider,
                "auth_id": entry.auth_id,
                "auth_label": entry.auth_label,
                "auth_note": entry.auth_note,
                "account_email": entry.account_email,
                "workspace_id": entry.workspace_id,
                "workspace_name": entry.workspace_name,
                "workspace_type": entry.workspace_type,
                "source": entry.source,
                "stale": entry.stale,
                "error": entry.error,
                "snapshot": snapshot.as_ref().map(|snapshot| json!({
                    "plan_type": snapshot.plan_type,
                    "primary": snapshot.primary.as_ref().map(|window| json!({
                        "used_percent": normalize_go_number(window.used_percent),
                        "window_minutes": window.window_minutes,
                        "resets_at": window.resets_at
                    }))
                }))
            })
        }).collect::<Vec<_>>()
    }))
}

fn normalize_go_number(value: f64) -> Value {
    if value.fract() == 0.0 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn strip_null_fields(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    let value = strip_null_fields(value);
                    if value.is_null() {
                        None
                    } else {
                        Some((key, value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(strip_null_fields).collect()),
        other => other,
    }
}
