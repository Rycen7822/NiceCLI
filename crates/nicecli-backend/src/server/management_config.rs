use super::{
    config_error_response, delete_config_field_response, delete_string_list_config_field_response,
    ensure_management_key, get_bool_config_field_response,
    get_bool_config_field_response_with_default, get_config_string_list_value,
    get_int_config_field_response, get_string_config_field_response, json_error_response,
    load_current_config, load_current_config_json, patch_string_list_config_field_response,
    put_bool_config_field_response, put_int_config_field_response,
    put_string_config_field_response, put_string_list_config_field_response,
    single_field_json_response, BackendAppState, ConfigBoolValueRequest, ConfigIntValueRequest,
    ConfigStringValueRequest, DeleteStringListQuery, PatchStringListRequest,
};
use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use nicecli_config::NiceCliConfig;
use nicecli_runtime::RoutingStrategy;
use serde_json::{json, Value as JsonValue};
use std::fs;
use std::sync::Arc;

pub(super) fn route_management_config_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route("/v0/management/config", get(get_config))
        .route(
            "/v0/management/config.yaml",
            get(get_config_yaml).put(put_config_yaml),
        )
        .route(
            "/v0/management/debug",
            get(get_debug).put(put_debug).patch(put_debug),
        )
        .route(
            "/v0/management/usage-statistics-enabled",
            get(get_usage_statistics_enabled)
                .put(put_usage_statistics_enabled)
                .patch(put_usage_statistics_enabled),
        )
        .route(
            "/v0/management/proxy-url",
            get(get_proxy_url)
                .put(put_proxy_url)
                .patch(put_proxy_url)
                .delete(delete_proxy_url),
        )
        .route(
            "/v0/management/quota-exceeded/switch-project",
            get(get_quota_switch_project)
                .put(put_quota_switch_project)
                .patch(put_quota_switch_project),
        )
        .route(
            "/v0/management/quota-exceeded/switch-preview-model",
            get(get_quota_switch_preview_model)
                .put(put_quota_switch_preview_model)
                .patch(put_quota_switch_preview_model),
        )
        .route(
            "/v0/management/api-keys",
            get(get_api_keys)
                .put(put_api_keys)
                .patch(patch_api_keys)
                .delete(delete_api_keys),
        )
        .route(
            "/v0/management/ws-auth",
            get(get_ws_auth).put(put_ws_auth).patch(put_ws_auth),
        )
        .route(
            "/v0/management/ampcode/upstream-url",
            get(get_amp_upstream_url)
                .put(put_amp_upstream_url)
                .patch(put_amp_upstream_url)
                .delete(delete_amp_upstream_url),
        )
        .route(
            "/v0/management/ampcode/upstream-api-key",
            get(get_amp_upstream_api_key)
                .put(put_amp_upstream_api_key)
                .patch(put_amp_upstream_api_key)
                .delete(delete_amp_upstream_api_key),
        )
        .route(
            "/v0/management/ampcode/restrict-management-to-localhost",
            get(get_amp_restrict_management_to_localhost)
                .put(put_amp_restrict_management_to_localhost)
                .patch(put_amp_restrict_management_to_localhost),
        )
        .route(
            "/v0/management/request-retry",
            get(get_request_retry)
                .put(put_request_retry)
                .patch(put_request_retry),
        )
        .route(
            "/v0/management/max-retry-interval",
            get(get_max_retry_interval)
                .put(put_max_retry_interval)
                .patch(put_max_retry_interval),
        )
        .route(
            "/v0/management/force-model-prefix",
            get(get_force_model_prefix)
                .put(put_force_model_prefix)
                .patch(put_force_model_prefix),
        )
        .route(
            "/v0/management/routing/strategy",
            get(get_routing_strategy)
                .put(put_routing_strategy)
                .patch(put_routing_strategy),
        )
        .route("/v0/management/usage", get(get_usage_statistics))
}

pub(super) async fn get_config(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_current_config_json(&state) {
        Ok(config) => Json(config).into_response(),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn get_config_yaml(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match fs::read(state.bootstrap.config_path()) {
        Ok(body) => {
            let mut response_headers = HeaderMap::new();
            response_headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/yaml; charset=utf-8"),
            );
            response_headers.insert("Cache-Control", HeaderValue::from_static("no-store"));
            response_headers.insert(
                "X-Content-Type-Options",
                HeaderValue::from_static("nosniff"),
            );
            (response_headers, body).into_response()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            json_error_response(StatusCode::NOT_FOUND, "not_found", "config file not found")
        }
        Err(error) => json_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_failed",
            &error.to_string(),
        ),
    }
}

pub(super) async fn put_config_yaml(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    if let Err(error) =
        NiceCliConfig::from_yaml_str(state.bootstrap.config_path().display().to_string(), &body)
    {
        return json_error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_config",
            &error.to_string(),
        );
    }

    match fs::write(state.bootstrap.config_path(), body) {
        Ok(()) => Json(json!({ "ok": true, "changed": ["config"] })).into_response(),
        Err(error) => json_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "write_failed",
            &error.to_string(),
        ),
    }
}

pub(super) async fn get_debug(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(&state, &headers, "debug", "debug")
}

pub(super) async fn put_debug(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "debug")
}

pub(super) async fn get_usage_statistics_enabled(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(
        &state,
        &headers,
        "usage-statistics-enabled",
        "usage-statistics-enabled",
    )
}

pub(super) async fn put_usage_statistics_enabled(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "usage-statistics-enabled")
}

pub(super) async fn get_proxy_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_current_config(&state) {
        Ok(config) => Json(json!({
            "proxy-url": config.proxy_url.unwrap_or_default()
        }))
        .into_response(),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_proxy_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigStringValueRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let value = request.value.or(request.proxy_url).unwrap_or_default();
    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        "proxy-url",
        &json!(value),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn delete_proxy_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        "proxy-url",
        &serde_json::Value::Null,
        true,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn get_quota_switch_project(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(
        &state,
        &headers,
        "quota-exceeded/switch-project",
        "switch-project",
    )
}

pub(super) async fn put_quota_switch_project(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "quota-exceeded/switch-project")
}

pub(super) async fn get_quota_switch_preview_model(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(
        &state,
        &headers,
        "quota-exceeded/switch-preview-model",
        "switch-preview-model",
    )
}

pub(super) async fn put_quota_switch_preview_model(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(
        &state,
        &headers,
        request,
        "quota-exceeded/switch-preview-model",
    )
}

pub(super) async fn get_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match get_config_string_list_value(&state, "api-keys") {
        Ok(values) => single_field_json_response("api-keys", json!(values)),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    put_string_list_config_field_response(&state, &headers, body, "api-keys")
}

pub(super) async fn patch_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<PatchStringListRequest>,
) -> Response {
    patch_string_list_config_field_response(&state, &headers, request, "api-keys")
}

pub(super) async fn delete_api_keys(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<DeleteStringListQuery>,
) -> Response {
    delete_string_list_config_field_response(&state, &headers, query, "api-keys")
}

pub(super) async fn get_ws_auth(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(&state, &headers, "ws-auth", "ws-auth")
}

pub(super) async fn put_ws_auth(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "ws-auth")
}

pub(super) async fn get_amp_upstream_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_string_config_field_response(&state, &headers, "ampcode/upstream-url", "upstream-url", "")
}

pub(super) async fn put_amp_upstream_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigStringValueRequest>,
) -> Response {
    put_string_config_field_response(&state, &headers, request, "ampcode/upstream-url")
}

pub(super) async fn delete_amp_upstream_url(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    delete_config_field_response(&state, &headers, "ampcode/upstream-url")
}

pub(super) async fn get_amp_upstream_api_key(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_string_config_field_response(
        &state,
        &headers,
        "ampcode/upstream-api-key",
        "upstream-api-key",
        "",
    )
}

pub(super) async fn put_amp_upstream_api_key(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigStringValueRequest>,
) -> Response {
    put_string_config_field_response(&state, &headers, request, "ampcode/upstream-api-key")
}

pub(super) async fn delete_amp_upstream_api_key(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    delete_config_field_response(&state, &headers, "ampcode/upstream-api-key")
}

pub(super) async fn get_amp_restrict_management_to_localhost(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response_with_default(
        &state,
        &headers,
        "ampcode/restrict-management-to-localhost",
        "restrict-management-to-localhost",
        false,
    )
}

pub(super) async fn put_amp_restrict_management_to_localhost(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(
        &state,
        &headers,
        request,
        "ampcode/restrict-management-to-localhost",
    )
}

pub(super) async fn get_request_retry(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_int_config_field_response(&state, &headers, "request-retry", "request-retry")
}

pub(super) async fn put_request_retry(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigIntValueRequest>,
) -> Response {
    put_int_config_field_response(&state, &headers, request, "request-retry", identity_i64)
}

pub(super) async fn get_max_retry_interval(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_int_config_field_response(&state, &headers, "max-retry-interval", "max-retry-interval")
}

pub(super) async fn put_max_retry_interval(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigIntValueRequest>,
) -> Response {
    put_int_config_field_response(
        &state,
        &headers,
        request,
        "max-retry-interval",
        identity_i64,
    )
}

pub(super) async fn get_force_model_prefix(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(&state, &headers, "force-model-prefix", "force-model-prefix")
}

pub(super) async fn put_force_model_prefix(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "force-model-prefix")
}

pub(super) async fn get_routing_strategy(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    match load_current_config(&state) {
        Ok(config) => {
            let strategy = RoutingStrategy::from_config_value(config.routing.strategy.as_deref());
            Json(json!({ "strategy": strategy.as_str() })).into_response()
        }
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn put_routing_strategy(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigStringValueRequest>,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    let Some(value) = request.value.as_deref() else {
        return json_error_response(StatusCode::BAD_REQUEST, "invalid body", "invalid body");
    };
    let Some(strategy) = RoutingStrategy::parse(value) else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "invalid strategy",
            "invalid strategy",
        );
    };

    match nicecli_config::update_config_value(
        state.bootstrap.config_path(),
        "routing.strategy",
        &json!(strategy.as_str()),
        false,
    ) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(error) => config_error_response(error),
    }
}

pub(super) async fn get_usage_statistics(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = ensure_management_key(&headers, &state) {
        return response;
    }

    Json(json!({
        "usage": empty_usage_statistics_snapshot(),
        "failed_requests": 0
    }))
    .into_response()
}

fn identity_i64(value: i64) -> i64 {
    value
}

fn empty_usage_statistics_snapshot() -> JsonValue {
    json!({
        "total_requests": 0,
        "success_count": 0,
        "failure_count": 0,
        "total_tokens": 0,
        "apis": {},
        "requests_by_day": {},
        "requests_by_hour": {},
        "tokens_by_day": {},
        "tokens_by_hour": {},
    })
}
