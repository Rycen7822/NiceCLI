use super::model_catalog::{
    auth_file_model_payload_from_model_info, collect_auth_file_model_infos,
};
use super::{
    auth_snapshot_store_error_response, auth_store_error_response, config_error_response,
    ensure_management_key, json_error, json_error_response, load_current_config_json,
    oauth_status_error_response, single_field_json_response, BackendAppState,
};
use axum::body::Body;
use axum::extract::{Multipart, Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use nicecli_auth::{
    delete_auth_file as delete_auth_file_from_store,
    patch_auth_file_fields as patch_auth_file_fields_in_store,
    patch_auth_file_status as patch_auth_file_status_in_store,
    read_auth_file as read_auth_file_from_store, resolve_oauth_callback_input,
    validate_oauth_state, write_auth_file as write_auth_file_to_store,
    write_oauth_callback_file_for_pending_session, AuthFileStoreError, OAuthFlowError,
    PatchAuthFileFields, PatchAuthFileStatus,
};
use nicecli_runtime::{AuthStore, FileAuthStore};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub(super) struct AuthFileNameQuery {
    name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct PatchAuthFileFieldsRequest {
    name: String,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    proxy_url: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PatchAuthFileStatusRequest {
    name: String,
    #[serde(default)]
    disabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct AuthStatusQuery {
    state: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct OAuthCallbackRequest {
    provider: String,
    redirect_url: Option<String>,
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub(super) fn route_management_auth_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route(
            "/v0/management/auth-files",
            get(list_auth_files)
                .post(upload_auth_file)
                .delete(delete_auth_file),
        )
        .route(
            "/v0/management/auth-files/download",
            get(download_auth_file),
        )
        .route(
            "/v0/management/auth-files/models",
            get(get_auth_file_models),
        )
        .route(
            "/v0/management/auth-files/fields",
            patch(patch_auth_file_fields),
        )
        .route(
            "/v0/management/auth-files/status",
            patch(patch_auth_file_status),
        )
        .route("/v0/management/oauth-callback", post(post_oauth_callback))
        .route("/v0/management/get-auth-status", get(get_auth_status))
}

pub(super) async fn list_auth_files(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let store = FileAuthStore::new(&state.auth_dir);
    match store.list_snapshots() {
        Ok(files) => Json(json!({ "files": files })).into_response(),
        Err(error) => auth_snapshot_store_error_response(error),
    }
}

pub(super) async fn download_auth_file(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<AuthFileNameQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let file_name = query.name.trim().to_string();
    let data = match read_auth_file_from_store(&state.auth_dir, &file_name) {
        Ok(data) => data,
        Err(error) => return auth_store_error_response(error),
    };

    let mut response = Response::new(Body::from(data));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        "application/json".parse().expect("valid content type"),
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        format!("attachment; filename=\"{file_name}\"")
            .parse()
            .expect("valid content disposition"),
    );
    response
}

pub(super) async fn upload_auth_file(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let mut uploaded = Vec::new();
    let mut failed = Vec::new();

    loop {
        let next_field = match multipart.next_field().await {
            Ok(field) => field,
            Err(error) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("invalid multipart form: {error}"),
                );
            }
        };

        let Some(field) = next_field else {
            break;
        };

        let original_name = field.file_name().map(str::to_string).unwrap_or_default();

        let bytes = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                failed.push(json!({
                    "name": original_name,
                    "error": format!("failed to read uploaded file: {error}")
                }));
                continue;
            }
        };

        match write_auth_file_to_store(&state.auth_dir, &original_name, &bytes) {
            Ok(file_name) => uploaded.push(file_name),
            Err(error) => failed.push(json!({
                "name": original_name,
                "error": error.to_string()
            })),
        }
    }

    if uploaded.is_empty() && failed.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "no files uploaded");
    }

    if failed.is_empty() && uploaded.len() == 1 {
        return Json(json!({ "status": "ok" })).into_response();
    }

    if failed.is_empty() {
        return Json(json!({
            "status": "ok",
            "uploaded": uploaded.len(),
            "files": uploaded,
        }))
        .into_response();
    }

    let status = if uploaded.is_empty() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::MULTI_STATUS
    };
    (
        status,
        Json(json!({
            "status": if uploaded.is_empty() { "error" } else { "partial" },
            "uploaded": uploaded.len(),
            "files": uploaded,
            "failed": failed,
        })),
    )
        .into_response()
}

pub(super) async fn delete_auth_file(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<AuthFileNameQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match delete_auth_file_from_store(&state.auth_dir, &query.name) {
        Ok(_) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => auth_store_error_response(error),
    }
}

pub(super) async fn patch_auth_file_fields(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<PatchAuthFileFieldsRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match patch_auth_file_fields_in_store(
        &state.auth_dir,
        &request.name,
        &PatchAuthFileFields {
            prefix: request.prefix,
            proxy_url: request.proxy_url,
            priority: request.priority,
            note: request.note,
        },
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => auth_store_error_response(error),
    }
}

pub(super) async fn patch_auth_file_status(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<PatchAuthFileStatusRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let name = request.name.trim();
    if name.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "name is required");
    }

    let Some(disabled) = request.disabled else {
        return json_error(StatusCode::BAD_REQUEST, "disabled is required");
    };

    match patch_auth_file_status_in_store(&state.auth_dir, name, PatchAuthFileStatus { disabled }) {
        Ok(()) => Json(json!({ "status": "ok", "disabled": disabled })).into_response(),
        Err(AuthFileStoreError::NotFound) => {
            json_error(StatusCode::NOT_FOUND, "auth file not found")
        }
        Err(error) => auth_store_error_response(error),
    }
}

pub(super) async fn get_auth_file_models(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<AuthFileNameQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let auth_name = query.name.trim();
    if auth_name.is_empty() {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "name is required",
            "name is required",
        );
    }

    let store = FileAuthStore::new(&state.auth_dir);
    let auth_snapshot = match store.find_snapshot(auth_name) {
        Ok(snapshot) => snapshot,
        Err(error) => return auth_snapshot_store_error_response(error),
    };
    let auth_provider = auth_snapshot
        .as_ref()
        .map(|snapshot| snapshot.provider.as_str());

    match load_current_config_json(&state) {
        Ok(config) => {
            let models =
                collect_auth_file_model_infos(&config, auth_snapshot.as_ref(), auth_provider);
            single_field_json_response(
                "models",
                JsonValue::Array(
                    models
                        .iter()
                        .map(auth_file_model_payload_from_model_info)
                        .collect(),
                ),
            )
        }
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn get_auth_status(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<AuthStatusQuery>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(raw_state) = query
        .state
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Json(json!({ "status": "ok" })).into_response();
    };

    if validate_oauth_state(raw_state).is_err() {
        return oauth_status_error_response(StatusCode::BAD_REQUEST, "invalid state");
    }

    match state.oauth_sessions.get(raw_state) {
        Ok(Some(session)) if !session.status.is_empty() => {
            Json(json!({ "status": "error", "error": session.status })).into_response()
        }
        Ok(Some(_)) => Json(json!({ "status": "wait" })).into_response(),
        Ok(None) => Json(json!({ "status": "ok" })).into_response(),
        Err(OAuthFlowError::InvalidState(_)) => {
            oauth_status_error_response(StatusCode::BAD_REQUEST, "invalid state")
        }
        Err(error) => oauth_status_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read oauth session: {error}"),
        ),
    }
}

pub(super) async fn post_oauth_callback(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<OAuthCallbackRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let callback = match resolve_oauth_callback_input(
        &request.provider,
        request.redirect_url.as_deref(),
        request.state.as_deref(),
        request.code.as_deref(),
        request.error.as_deref(),
    ) {
        Ok(callback) => callback,
        Err(OAuthFlowError::UnsupportedProvider) => {
            return oauth_status_error_response(StatusCode::BAD_REQUEST, "unsupported provider")
        }
        Err(OAuthFlowError::InvalidRedirectUrl) => {
            return oauth_status_error_response(StatusCode::BAD_REQUEST, "invalid redirect_url")
        }
        Err(OAuthFlowError::MissingState) => {
            return oauth_status_error_response(StatusCode::BAD_REQUEST, "state is required")
        }
        Err(OAuthFlowError::InvalidState(_)) => {
            return oauth_status_error_response(StatusCode::BAD_REQUEST, "invalid state")
        }
        Err(OAuthFlowError::MissingCodeOrError) => {
            return oauth_status_error_response(
                StatusCode::BAD_REQUEST,
                "code or error is required",
            )
        }
        Err(error) => {
            return oauth_status_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to resolve oauth callback: {error}"),
            )
        }
    };

    let session = match state.oauth_sessions.get(&callback.state) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return oauth_status_error_response(StatusCode::NOT_FOUND, "unknown or expired state")
        }
        Err(OAuthFlowError::InvalidState(_)) => {
            return oauth_status_error_response(StatusCode::BAD_REQUEST, "invalid state")
        }
        Err(error) => {
            return oauth_status_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load oauth session: {error}"),
            )
        }
    };

    if !session.status.is_empty() {
        return oauth_status_error_response(StatusCode::CONFLICT, "oauth flow is not pending");
    }

    if !session.provider.eq_ignore_ascii_case(&callback.provider) {
        return oauth_status_error_response(
            StatusCode::BAD_REQUEST,
            "provider does not match state",
        );
    }

    match write_oauth_callback_file_for_pending_session(
        &state.auth_dir,
        state.oauth_sessions.as_ref(),
        &callback.provider,
        &callback.state,
        &callback.code,
        &callback.error,
    ) {
        Ok(_) => Json(json!({ "status": "ok" })).into_response(),
        Err(OAuthFlowError::SessionNotPending) => {
            oauth_status_error_response(StatusCode::CONFLICT, "oauth flow is not pending")
        }
        Err(
            OAuthFlowError::UnsupportedProvider
            | OAuthFlowError::InvalidRedirectUrl
            | OAuthFlowError::MissingState
            | OAuthFlowError::MissingCodeOrError
            | OAuthFlowError::InvalidState(_),
        ) => oauth_status_error_response(StatusCode::BAD_REQUEST, "invalid oauth callback"),
        Err(error) => oauth_status_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist oauth callback: {error}"),
        ),
    }
}
