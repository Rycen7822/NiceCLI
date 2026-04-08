// Tauri v2 backend for NiceCLI.
// Hosts the desktop shell and manages the local Rust backend runtime.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app_identity;
mod autostart;
mod backend_launch;
mod backend_payload;
mod backend_smoke;
mod callback_server;
mod desktop_host;
mod file_export;
mod local_runtime;
mod managed_host;
mod startup_preferences;
mod tray_host;
mod tray_smoke;
mod window_lifecycle;
mod windowing;

use app_identity::app_storage_dir_name;
use autostart::{check_auto_start_enabled, disable_auto_start, enable_auto_start};
use backend_smoke::maybe_run_backend_host_smoke;
use callback_server::{start_callback_server, stop_callback_server};
use desktop_host::{run_app, setup_app};
use file_export::save_files_to_directory;
use local_runtime::{restart_local_runtime, start_local_runtime};
use nicecli_backend::{
    load_state_from_bootstrap, serve_state_with_shutdown, start_model_catalog_refresh_task,
    BackendBootstrap,
};
use nicecli_config::{load_config_json, update_config_value};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::Rng;
use serde_json::json;
use startup_preferences::{read_startup_preferences, update_startup_preferences};
use std::io;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use thiserror::Error;
use tray_host::clear_tray_icon;
use window_lifecycle::handle_window_event;
use windowing::{open_login_window, open_settings_window};

static PROCESS: Lazy<Arc<Mutex<Option<Child>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
static IN_PROCESS_BACKEND: Lazy<Arc<Mutex<Option<ManagedInProcessBackend>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));
static PROCESS_PID: Lazy<Arc<Mutex<Option<u32>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
// Store the runtime password used to start the managed local backend.
static CLI_PROXY_PASSWORD: Lazy<Arc<Mutex<Option<String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));
static MANAGED_BACKEND_INFO: Lazy<Arc<Mutex<Option<ManagedBackendInfo>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

#[derive(Error, Debug)]
enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Other: {0}")]
    Other(String),
}

#[derive(Clone, Debug)]
struct ManagedBackendInfo {
    mode: String,
    version: String,
}

struct ManagedInProcessBackend {
    join_handle: Option<thread::JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    running: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
}

impl ManagedInProcessBackend {
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop(mut self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[derive(Debug)]
enum BackendLaunchMode {
    RustInProcess,
    RustExternal { exec_path: PathBuf },
}

#[derive(Debug)]
enum BackendLaunch {
    InProcess {
        config_path: PathBuf,
        info: ManagedBackendInfo,
    },
    Child {
        exec_path: PathBuf,
        config_path: PathBuf,
        info: ManagedBackendInfo,
    },
}

impl BackendLaunch {
    fn config_path(&self) -> &Path {
        match self {
            Self::InProcess { config_path, .. } | Self::Child { config_path, .. } => config_path,
        }
    }

    fn info(&self) -> &ManagedBackendInfo {
        match self {
            Self::InProcess { info, .. } | Self::Child { info, .. } => info,
        }
    }
}

fn home_dir() -> Result<PathBuf, AppError> {
    home::home_dir().ok_or_else(|| AppError::Other("Failed to resolve home directory".into()))
}

fn app_dir() -> Result<PathBuf, AppError> {
    Ok(home_dir()?.join(app_storage_dir_name()))
}
#[tauri::command]
fn read_config_yaml() -> Result<serde_json::Value, String> {
    let dir = app_dir().map_err(|e| e.to_string())?;
    let p = dir.join("config.yaml");
    load_config_json(&p).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_config_yaml(
    endpoint: String,
    value: serde_json::Value,
    is_delete: Option<bool>,
) -> Result<serde_json::Value, String> {
    let dir = app_dir().map_err(|e| e.to_string())?;
    let p = dir.join("config.yaml");
    update_config_value(&p, &endpoint, &value, is_delete.unwrap_or(false))
        .map_err(|e| e.to_string())?;
    Ok(json!({"success": true}))
}

fn generate_random_password() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

fn spawn_backend_process(launch: &BackendLaunch, password: &str) -> Result<Child, String> {
    let (exec_path, config_path) = match launch {
        BackendLaunch::Child {
            exec_path,
            config_path,
            ..
        } => (exec_path, config_path),
        BackendLaunch::InProcess { .. } => {
            return Err("cannot spawn a child process for rust-in-process backend".into())
        }
    };

    let mut cmd = std::process::Command::new(exec_path);
    cmd.args([
        "--config",
        config_path.to_string_lossy().as_ref(),
        "--password",
        password,
    ]);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn().map_err(|e| {
        eprintln!(
            "[NiceCLI][ERROR] failed to start local runtime process: {}",
            e
        );
        e.to_string()
    })
}

fn start_in_process_backend(
    app: Option<tauri::AppHandle>,
    config_path: &Path,
    password: &str,
) -> Result<ManagedInProcessBackend, String> {
    let config_path = config_path.to_path_buf();
    let password = password.to_string();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (started_tx, started_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let running = Arc::new(AtomicBool::new(false));
    let running_flag = Arc::clone(&running);
    let running_after = Arc::clone(&running);
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_requested_flag = Arc::clone(&stop_requested);
    let join_handle = thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = started_tx.send(Err(error.to_string()));
                return;
            }
        };

        let result = runtime.block_on(async move {
            let mut started_tx = Some(started_tx);
            let bootstrap =
                BackendBootstrap::new(config_path).with_local_management_password(password);
            let state = match load_state_from_bootstrap(bootstrap) {
                Ok(state) => state,
                Err(error) => {
                    let message = error.to_string();
                    if let Some(tx) = started_tx.take() {
                        let _ = tx.send(Err(message.clone()));
                    }
                    return Err(message);
                }
            };
            let bind_addr = format!(
                "{}:{}",
                state.config.effective_host(),
                state.config.effective_port()
            );
            let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
                Ok(listener) => listener,
                Err(error) => {
                    let message = format!("failed to bind {bind_addr}: {error}");
                    if let Some(tx) = started_tx.take() {
                        let _ = tx.send(Err(message.clone()));
                    }
                    return Err(message);
                }
            };
            running_flag.store(true, Ordering::SeqCst);
            if let Some(tx) = started_tx.take() {
                let _ = tx.send(Ok(()));
            }
            let model_catalog_cache_path = state
                .bootstrap
                .config_path()
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(".nicecli-model-catalog.json");
            start_model_catalog_refresh_task(model_catalog_cache_path);
            serve_state_with_shutdown(state, listener, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .map_err(|error| error.to_string())
        });

        running_after.store(false, Ordering::SeqCst);
        if stop_requested_flag.load(Ordering::SeqCst) {
            return;
        }

        let Some(app) = app else {
            return;
        };

        match result {
            Ok(()) => {
                println!("[NiceCLI][EXIT] in-process backend closed");
                let _ = app.emit(
                    "process-closed",
                    json!({"message": "NiceCLI local runtime has closed"}),
                );
            }
            Err(error) => {
                eprintln!("[NiceCLI][EXIT] in-process backend exited with error: {error}");
                let _ = app.emit("process-exit-error", json!({"code": 1, "error": error}));
            }
        }
        clear_tray_icon();
    });

    match started_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(())) => Ok(ManagedInProcessBackend {
            join_handle: Some(join_handle),
            shutdown_tx: Some(shutdown_tx),
            running,
            stop_requested,
        }),
        Ok(Err(error)) => {
            stop_requested.store(true, Ordering::SeqCst);
            let _ = shutdown_tx.send(());
            let _ = join_handle.join();
            Err(error)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            stop_requested.store(true, Ordering::SeqCst);
            let _ = shutdown_tx.send(());
            let _ = join_handle.join();
            Err("rust-in-process backend startup timed out".into())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            stop_requested.store(true, Ordering::SeqCst);
            let _ = shutdown_tx.send(());
            let _ = join_handle.join();
            Err("rust-in-process backend exited before reporting startup status".into())
        }
    }
}

fn main() {
    match maybe_run_backend_host_smoke() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("backend host smoke failed: {error}");
            std::process::exit(1);
        }
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(setup_app)
        .on_window_event(handle_window_event)
        .invoke_handler(tauri::generate_handler![
            read_config_yaml,
            update_config_yaml,
            restart_local_runtime,
            start_local_runtime,
            open_settings_window,
            open_login_window,
            start_callback_server,
            stop_callback_server,
            save_files_to_directory,
            check_auto_start_enabled,
            enable_auto_start,
            disable_auto_start,
            read_startup_preferences,
            update_startup_preferences
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    run_app(app);
}
