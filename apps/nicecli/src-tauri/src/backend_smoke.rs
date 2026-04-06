use crate::backend_launch::{prepare_backend_launch, wait_for_listen_port};
use nicecli_config::update_config_value;
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Child;

#[derive(Debug, Serialize)]
struct BackendHostSmokeResult {
    success: bool,
    backend_mode: String,
    version: String,
    port: u16,
    keep_alive_status: u16,
    auth_files_status: u16,
    app_dir: String,
}

fn backend_host_smoke_requested(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--smoke-backend-host")
}

fn run_smoke_request(
    port: u16,
    path: &str,
    management_key: Option<&str>,
) -> Result<(u16, String), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| e.to_string())?;
        let url = format!("http://127.0.0.1:{port}{path}");
        let mut request = client.get(url);
        if let Some(key) = management_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(|e| e.to_string())?;
        Ok((status, body))
    })
}

fn reserve_local_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    Ok(port)
}

fn run_backend_host_smoke() -> Result<BackendHostSmokeResult, String> {
    struct SmokeGuard {
        child: Option<Child>,
        in_process: Option<crate::ManagedInProcessBackend>,
        app_dir: PathBuf,
    }

    impl Drop for SmokeGuard {
        fn drop(&mut self) {
            if let Some(backend) = self.in_process.take() {
                backend.stop();
            }
            if let Some(child) = self.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = fs::remove_dir_all(&self.app_dir);
        }
    }

    let app_dir = std::env::temp_dir().join(format!(
        "nicecli-host-smoke-{}",
        crate::generate_random_password()
    ));
    fs::create_dir_all(app_dir.join("auth")).map_err(|e| e.to_string())?;

    let launch = prepare_backend_launch(&app_dir, None)?;
    let port = reserve_local_port()?;
    update_config_value(launch.config_path(), "port", &json!(port), false)
        .map_err(|e| e.to_string())?;
    let password = crate::generate_random_password();
    let mut guard = SmokeGuard {
        child: None,
        in_process: None,
        app_dir: app_dir.clone(),
    };

    match &launch {
        crate::BackendLaunch::InProcess { config_path, .. } => {
            guard.in_process = Some(crate::start_in_process_backend(
                None,
                config_path,
                &password,
            )?);
        }
        crate::BackendLaunch::Child { .. } => {
            guard.child = Some(crate::spawn_backend_process(&launch, &password)?);
        }
    }

    if !wait_for_listen_port(port, 12_000) {
        return Err("backend smoke process did not become ready in time".into());
    }

    let (keep_alive_status, keep_alive_body) =
        run_smoke_request(port, "/keep-alive", Some(&password))?;
    if keep_alive_status != 200 || !keep_alive_body.contains("ok") {
        return Err(format!(
            "keep-alive smoke request failed with status {keep_alive_status}: {keep_alive_body}"
        ));
    }

    let (auth_files_status, auth_files_body) =
        run_smoke_request(port, "/v0/management/auth-files", Some(&password))?;
    if auth_files_status != 200 {
        return Err(format!(
            "auth-files smoke request failed with status {auth_files_status}: {auth_files_body}"
        ));
    }

    if let Some(backend) = guard.in_process.take() {
        backend.stop();
    }
    if let Some(child) = guard.child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    guard.child = None;

    Ok(BackendHostSmokeResult {
        success: true,
        backend_mode: launch.info().mode.clone(),
        version: launch.info().version.clone(),
        port,
        keep_alive_status,
        auth_files_status,
        app_dir: app_dir.display().to_string(),
    })
}

pub(crate) fn maybe_run_backend_host_smoke() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().collect();
    if !backend_host_smoke_requested(&args) {
        return Ok(false);
    }

    let result = run_backend_host_smoke()?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::backend_host_smoke_requested;

    #[test]
    fn backend_smoke_flag_detection_matches_expected_arg() {
        assert!(backend_host_smoke_requested(&[
            "nicecli".to_string(),
            "--smoke-backend-host".to_string(),
        ]));
        assert!(!backend_host_smoke_requested(&[
            "nicecli".to_string(),
            "--smoke-tray-host".to_string(),
        ]));
    }
}
