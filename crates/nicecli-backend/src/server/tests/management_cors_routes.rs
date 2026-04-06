use super::*;

fn header_text(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn codex_auth_url_preflight_allows_management_headers() {
    let temp_dir = TempDir::new().expect("temp dir");
    let response = build_router(load_fixture_state(&temp_dir))
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/v0/management/codex-auth-url")
                .header("Origin", "http://tauri.localhost")
                .header("Access-Control-Request-Method", "GET")
                .header(
                    "Access-Control-Request-Headers",
                    "x-management-key,content-type",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert!(response.status().is_success());
    let allow_origin = header_text(response.headers(), "access-control-allow-origin");
    assert_eq!(allow_origin, "*");
    let allow_headers = header_text(response.headers(), "access-control-allow-headers");
    assert!(allow_headers
        .to_ascii_lowercase()
        .contains("x-management-key"));
    assert!(allow_headers.to_ascii_lowercase().contains("content-type"));
}

#[tokio::test]
async fn auth_file_upload_preflight_allows_management_headers() {
    let temp_dir = TempDir::new().expect("temp dir");
    let response = build_router(load_fixture_state(&temp_dir))
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/v0/management/auth-files")
                .header("Origin", "http://tauri.localhost")
                .header("Access-Control-Request-Method", "POST")
                .header(
                    "Access-Control-Request-Headers",
                    "x-management-key,content-type",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert!(response.status().is_success());
    let allow_origin = header_text(response.headers(), "access-control-allow-origin");
    assert_eq!(allow_origin, "*");
    let allow_headers = header_text(response.headers(), "access-control-allow-headers");
    assert!(allow_headers
        .to_ascii_lowercase()
        .contains("x-management-key"));
    assert!(allow_headers.to_ascii_lowercase().contains("content-type"));
}

#[tokio::test]
async fn auth_preflight_allows_private_network_requests() {
    let temp_dir = TempDir::new().expect("temp dir");
    let response = build_router(load_fixture_state(&temp_dir))
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/v0/management/claude-auth-url")
                .header("Origin", "http://tauri.localhost")
                .header("Access-Control-Request-Method", "GET")
                .header(
                    "Access-Control-Request-Headers",
                    "x-management-key,content-type",
                )
                .header("Access-Control-Request-Private-Network", "true")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert!(response.status().is_success());
    let allow_private_network =
        header_text(response.headers(), "access-control-allow-private-network");
    assert_eq!(allow_private_network, "true");
}
