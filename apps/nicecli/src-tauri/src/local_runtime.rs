use crate::backend_launch::{load_backend_port, prepare_backend_launch, wait_for_listen_port};
use crate::managed_host::{managed_process_running, start_monitor, stop_managed_process};
use crate::tray_host::create_tray;
use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalRuntimeStartResponse {
    pub(crate) success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) password: Option<String>,
    pub(crate) version: String,
    pub(crate) backend_mode: String,
}

fn kill_process_on_port(port: u16) -> Result<(), String> {
    println!("[PORT_CLEANUP] Checking port {}", port);

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("lsof")
            .args(["-ti", &format!(":{}", port)])
            .output()
            .map_err(|e| format!("Failed to run lsof: {}", e))?;

        if output.status.success() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid_str in pids.lines() {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    println!("[PORT_CLEANUP] Killing PID {} on port {}", pid, port);
                    if let Err(e) = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .output()
                    {
                        eprintln!("[PORT_CLEANUP] Failed to run kill for PID {}: {}", pid, e);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("fuser")
            .args(["-k", "-9", &format!("{}/tcp", port)])
            .output()
            .map_err(|e| format!("Failed to run fuser: {}", e))?;

        if output.status.success() {
            println!("[PORT_CLEANUP] Killed processes on port {}", port);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("netstat")
            .args(["-ano"])
            .output()
            .map_err(|e| format!("Failed to run netstat: {}", e))?;

        if output.status.success() {
            let netstat_output = String::from_utf8_lossy(&output.stdout);
            let port_pattern = format!(":{}", port);

            for line in netstat_output.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() > 2
                    && parts[1].ends_with(&port_pattern)
                    && line.contains("LISTENING")
                {
                    if let Some(pid_str) = parts.last() {
                        if let Ok(pid) = pid_str.parse::<i32>() {
                            println!("[PORT_CLEANUP] Killing PID {} on port {}", pid, port);
                            if let Err(e) = std::process::Command::new("taskkill")
                                .args(["/F", "/PID", &pid.to_string()])
                                .output()
                            {
                                eprintln!(
                                    "[PORT_CLEANUP] Failed to run taskkill for PID {}: {}",
                                    pid, e
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn start_local_runtime_internal(
    app: tauri::AppHandle,
    proxy_url: Option<&str>,
) -> Result<LocalRuntimeStartResponse, String> {
    if managed_process_running() {
        let info =
            crate::MANAGED_BACKEND_INFO
                .lock()
                .clone()
                .unwrap_or(crate::ManagedBackendInfo {
                    mode: "rust-in-process".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                });
        return Ok(LocalRuntimeStartResponse {
            success: true,
            message: Some("already running".to_string()),
            password: crate::CLI_PROXY_PASSWORD.lock().clone(),
            version: info.version,
            backend_mode: info.mode,
        });
    }

    let dir = crate::app_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let launch = prepare_backend_launch(&dir, proxy_url)?;
    let port = load_backend_port(launch.config_path())?;

    if let Err(error) = kill_process_on_port(port) {
        eprintln!("[PORT_CLEANUP] Warning: {}", error);
    }

    let password = crate::generate_random_password();
    *crate::CLI_PROXY_PASSWORD.lock() = Some(password.clone());

    let info = launch.info().clone();
    match &launch {
        crate::BackendLaunch::InProcess { config_path, .. } => {
            let backend =
                crate::start_in_process_backend(Some(app.clone()), config_path, &password)?;
            println!(
                "[NiceCLI][INFO] starting local runtime mode={} config={}",
                info.mode,
                config_path.display()
            );
            *crate::IN_PROCESS_BACKEND.lock() = Some(backend);
            *crate::PROCESS_PID.lock() = None;
        }
        crate::BackendLaunch::Child { exec_path, .. } => {
            let child = crate::spawn_backend_process(&launch, &password)?;
            let pid = child.id();
            println!(
                "[NiceCLI][INFO] starting local runtime mode={} executable={}",
                info.mode,
                exec_path.display()
            );
            *crate::PROCESS_PID.lock() = Some(pid);
            *crate::PROCESS.lock() = Some(child);
            start_monitor(app.clone());
        }
    }
    *crate::MANAGED_BACKEND_INFO.lock() = Some(info.clone());

    if !wait_for_listen_port(port, 12_000) {
        stop_managed_process();
        return Err("NiceCLI local runtime did not become ready in time".into());
    }

    let _ = create_tray(&app);

    Ok(LocalRuntimeStartResponse {
        success: true,
        message: None,
        password: Some(password),
        version: info.version,
        backend_mode: info.mode,
    })
}

#[tauri::command]
pub(crate) fn start_local_runtime(
    app: tauri::AppHandle,
    proxy_url: Option<String>,
) -> Result<LocalRuntimeStartResponse, String> {
    start_local_runtime_internal(app, proxy_url.as_deref())
}

#[tauri::command]
pub(crate) fn restart_local_runtime(app: tauri::AppHandle) -> Result<(), String> {
    stop_managed_process();
    let result = start_local_runtime_internal(app.clone(), None)?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("local-runtime-restarted", result.clone());
    }
    Ok(())
}
