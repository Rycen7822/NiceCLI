use super::{
    attachment_response, get_bool_config_field_response, get_config_bool_value,
    get_int_config_field_response, json_error_response, put_bool_config_field_response,
    put_int_config_field_response, BackendAppState, ConfigBoolValueRequest, ConfigIntValueRequest,
};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct LogsQuery {
    after: Option<i64>,
    limit: Option<usize>,
}

pub(super) fn route_management_logs_routes(
    router: Router<Arc<BackendAppState>>,
) -> Router<Arc<BackendAppState>> {
    router
        .route(
            "/v0/management/logging-to-file",
            get(get_logging_to_file)
                .put(put_logging_to_file)
                .patch(put_logging_to_file),
        )
        .route(
            "/v0/management/logs-max-total-size-mb",
            get(get_logs_max_total_size_mb)
                .put(put_logs_max_total_size_mb)
                .patch(put_logs_max_total_size_mb),
        )
        .route(
            "/v0/management/error-logs-max-files",
            get(get_error_logs_max_files)
                .put(put_error_logs_max_files)
                .patch(put_error_logs_max_files),
        )
        .route(
            "/v0/management/request-log",
            get(get_request_log)
                .put(put_request_log)
                .patch(put_request_log),
        )
        .route("/v0/management/logs", get(get_logs))
        .route(
            "/v0/management/request-error-logs",
            get(get_request_error_logs),
        )
        .route(
            "/v0/management/request-log-by-id/:id",
            get(get_request_log_by_id),
        )
}

pub(super) async fn get_logging_to_file(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(&state, &headers, "logging-to-file", "logging-to-file")
}

pub(super) async fn put_logging_to_file(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "logging-to-file")
}

pub(super) async fn get_logs_max_total_size_mb(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_int_config_field_response(
        &state,
        &headers,
        "logs-max-total-size-mb",
        "logs-max-total-size-mb",
    )
}

pub(super) async fn put_logs_max_total_size_mb(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigIntValueRequest>,
) -> Response {
    put_int_config_field_response(
        &state,
        &headers,
        request,
        "logs-max-total-size-mb",
        normalize_non_negative_i64,
    )
}

pub(super) async fn get_error_logs_max_files(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_int_config_field_response(
        &state,
        &headers,
        "error-logs-max-files",
        "error-logs-max-files",
    )
}

pub(super) async fn put_error_logs_max_files(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigIntValueRequest>,
) -> Response {
    put_int_config_field_response(
        &state,
        &headers,
        request,
        "error-logs-max-files",
        normalize_error_logs_max_files,
    )
}

pub(super) async fn get_request_log(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    get_bool_config_field_response(&state, &headers, "request-log", "request-log")
}

pub(super) async fn put_request_log(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Json(request): Json<ConfigBoolValueRequest>,
) -> Response {
    put_bool_config_field_response(&state, &headers, request, "request-log")
}

pub(super) async fn get_logs(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    Query(query): Query<LogsQuery>,
) -> Response {
    if let Err(response) = super::ensure_management_key(&headers, &state) {
        return response;
    }

    let cutoff = query.after.unwrap_or_default().max(0);
    let limit = match query.limit {
        Some(0) => {
            return json_error_response(
                StatusCode::BAD_REQUEST,
                "invalid limit",
                "must be greater than zero",
            );
        }
        Some(limit) => limit,
        None => 0,
    };

    let log_dir = resolve_log_directory(&state);
    let files = match collect_log_files(&log_dir) {
        Ok(files) => files,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Json(json!({
                "lines": [],
                "line-count": 0,
                "latest-timestamp": cutoff,
            }))
            .into_response();
        }
        Err(error) => {
            return json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_failed",
                &format!("failed to list log files: {error}"),
            );
        }
    };

    let mut accumulator = LogAccumulator::new(cutoff, limit);
    for path in files {
        if let Err(error) = accumulator.consume_file(&path) {
            return json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_failed",
                &format!("failed to read log file {}: {error}", path.display()),
            );
        }
    }

    let (lines, total, latest) = accumulator.finish();
    Json(json!({
        "lines": lines,
        "line-count": total,
        "latest-timestamp": latest.max(cutoff),
    }))
    .into_response()
}

pub(super) async fn get_request_error_logs(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = super::ensure_management_key(&headers, &state) {
        return response;
    }

    if get_config_bool_value(&state, "request-log").unwrap_or(false) {
        return Json(json!({ "files": [] })).into_response();
    }

    let log_dir = resolve_log_directory(&state);
    let entries = match fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Json(json!({ "files": [] })).into_response();
        }
        Err(error) => {
            return json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_failed",
                &format!("failed to list request error logs: {error}"),
            );
        }
    };

    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("error-") || !name.ends_with(".log") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs() as i64)
            .unwrap_or_default();
        files.push(json!({
            "name": name,
            "size": metadata.len(),
            "modified": modified,
        }));
    }
    files.sort_by(|left, right| {
        right["modified"]
            .as_i64()
            .cmp(&left["modified"].as_i64())
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });

    Json(json!({ "files": files })).into_response()
}

pub(super) async fn get_request_log_by_id(
    State(state): State<Arc<BackendAppState>>,
    headers: HeaderMap,
    AxumPath(request_id): AxumPath<String>,
) -> Response {
    if let Err(response) = super::ensure_management_key(&headers, &state) {
        return response;
    }

    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.contains('/') || request_id.contains('\\') {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "invalid request ID",
            "invalid request ID",
        );
    }

    let log_dir = resolve_log_directory(&state);
    let entries = match fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "log directory not found",
            );
        }
        Err(error) => {
            return json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_failed",
                &format!("failed to list log directory: {error}"),
            );
        }
    };

    let suffix = format!("-{request_id}.log");
    let matched = entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file()
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.ends_with(&suffix))
    });

    let Some(path) = matched else {
        return json_error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            "log file not found for the given request ID",
        );
    };

    match fs::read(&path) {
        Ok(body) => attachment_response(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("request.log"),
            body,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            json_error_response(StatusCode::NOT_FOUND, "not_found", "log file not found")
        }
        Err(error) => json_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_failed",
            &format!("failed to read log file: {error}"),
        ),
    }
}

fn normalize_non_negative_i64(value: i64) -> i64 {
    value.max(0)
}

fn normalize_error_logs_max_files(value: i64) -> i64 {
    if value < 0 {
        10
    } else {
        value
    }
}

fn resolve_log_directory(state: &BackendAppState) -> PathBuf {
    if let Some(base) = resolve_writable_path() {
        return base.join("logs");
    }

    let auth_logs = state.auth_dir.join("logs");
    if auth_logs.exists() {
        return auth_logs;
    }

    let local_logs = PathBuf::from("logs");
    if local_logs.exists() {
        return local_logs;
    }

    auth_logs
}

fn resolve_writable_path() -> Option<PathBuf> {
    ["WRITABLE_PATH", "writable_path"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn collect_log_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let entries = fs::read_dir(dir)?;
    let mut candidates = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name == "main.log" {
            candidates.push((0_i64, path));
            continue;
        }
        if let Some(order) = rotation_order(name) {
            candidates.push((order, path));
        }
    }

    candidates.sort_by_key(|(order, _)| *order);
    candidates.reverse();
    Ok(candidates.into_iter().map(|(_, path)| path).collect())
}

#[derive(Debug)]
struct LogAccumulator {
    cutoff: i64,
    limit: usize,
    lines: Vec<String>,
    total: usize,
    latest: i64,
    include: bool,
}

impl LogAccumulator {
    fn new(cutoff: i64, limit: usize) -> Self {
        let capacity = match limit {
            0 => 256,
            value => value.min(256),
        };
        Self {
            cutoff,
            limit,
            lines: Vec::with_capacity(capacity),
            total: 0,
            latest: 0,
            include: false,
        }
    }

    fn consume_file(&mut self, path: &Path) -> Result<(), std::io::Error> {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };

        for line in raw.lines() {
            self.add_line(line.trim_end_matches('\r'));
        }
        Ok(())
    }

    fn add_line(&mut self, line: &str) {
        self.total += 1;
        let timestamp = parse_log_timestamp(line);
        if timestamp > self.latest {
            self.latest = timestamp;
        }

        if timestamp > 0 {
            self.include = self.cutoff == 0 || timestamp > self.cutoff;
            if self.include {
                self.push_line(line);
            }
            return;
        }

        if self.cutoff == 0 || self.include {
            self.push_line(line);
        }
    }

    fn push_line(&mut self, line: &str) {
        self.lines.push(line.to_string());
        if self.limit > 0 && self.lines.len() > self.limit {
            let overflow = self.lines.len() - self.limit;
            self.lines.drain(..overflow);
        }
    }

    fn finish(self) -> (Vec<String>, usize, i64) {
        (self.lines, self.total, self.latest)
    }
}

fn parse_log_timestamp(line: &str) -> i64 {
    let trimmed = line.strip_prefix('[').unwrap_or(line);
    if trimmed.len() < 19 {
        return 0;
    }

    let candidate = &trimmed[..19];
    chrono::NaiveDateTime::parse_from_str(candidate, "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|value| value.and_local_timezone(chrono::Local).single())
        .map(|value| value.timestamp())
        .unwrap_or_default()
}

fn rotation_order(name: &str) -> Option<i64> {
    numeric_rotation_order(name).or_else(|| timestamp_rotation_order(name))
}

fn numeric_rotation_order(name: &str) -> Option<i64> {
    name.strip_prefix("main.log.")
        .and_then(|suffix| suffix.parse::<i64>().ok())
}

fn timestamp_rotation_order(name: &str) -> Option<i64> {
    let prefix = "main-";
    if !name.starts_with(prefix) {
        return None;
    }

    let mut clean = name.trim_start_matches(prefix).to_string();
    if clean.ends_with(".gz") {
        clean.truncate(clean.len().saturating_sub(3));
    }
    if clean.ends_with(".log") {
        clean.truncate(clean.len().saturating_sub(4));
    } else {
        return None;
    }
    if let Some((value, _)) = clean.split_once('.') {
        clean = value.to_string();
    }

    chrono::NaiveDateTime::parse_from_str(&clean, "%Y-%m-%dT%H-%M-%S")
        .ok()
        .and_then(|value| value.and_local_timezone(chrono::Local).single())
        .map(|value| i64::MAX - value.timestamp())
}
