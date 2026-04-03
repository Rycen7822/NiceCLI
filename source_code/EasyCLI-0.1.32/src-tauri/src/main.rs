// Tauri v2 backend for EasyCLI
// Ports core Electron main.js logic to Rust with a simpler API surface (KISS)

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod backend_payload;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::Rng;
use rfd::FileDialog;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::io::{self, BufRead, BufReader, Write};
#[cfg(not(target_os = "windows"))]
use std::os::unix::process::CommandExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::tray::TrayIcon;
use tauri::WindowEvent;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use thiserror::Error;

static PROCESS: Lazy<Arc<Mutex<Option<Child>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
static PROCESS_PID: Lazy<Arc<Mutex<Option<u32>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
static TRAY_ICON: Lazy<Arc<Mutex<Option<TrayIcon>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
static CALLBACK_SERVERS: Lazy<Arc<Mutex<HashMap<u16, (Arc<AtomicBool>, thread::JoinHandle<()>)>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));
// Store the runtime password used to start the embedded CLIProxyAPI.
static CLI_PROXY_PASSWORD: Lazy<Arc<Mutex<Option<String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));
static APP_VARIANT: Lazy<AppVariant> = Lazy::new(detect_app_variant);
const CONTROL_PANEL_WINDOW_WIDTH: f64 = 1116.0;
const CONTROL_PANEL_WINDOW_HEIGHT: f64 = 720.0;
const CONTROL_PANEL_MIN_WINDOW_WIDTH: f64 = 912.0;
const CONTROL_PANEL_MIN_WINDOW_HEIGHT: f64 = 624.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppVariant {
    Official,
    Dev,
}

#[derive(Error, Debug)]
enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Other: {0}")]
    Other(String),
}

fn home_dir() -> Result<PathBuf, AppError> {
    home::home_dir().ok_or_else(|| AppError::Other("Failed to resolve home directory".into()))
}

fn detect_app_variant() -> AppVariant {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
        })
        .unwrap_or_default();

    if stem.contains("nicecli") || stem.contains("easycli-dev") {
        AppVariant::Dev
    } else {
        AppVariant::Official
    }
}

fn app_variant() -> AppVariant {
    *APP_VARIANT
}

fn app_storage_dir_name() -> &'static str {
    match app_variant() {
        AppVariant::Official => "cliproxyapi",
        AppVariant::Dev => "nicecli",
    }
}

fn app_display_name() -> &'static str {
    match app_variant() {
        AppVariant::Official => "EasyCLI",
        AppVariant::Dev => "NiceCLI",
    }
}

fn auto_start_entry_name() -> &'static str {
    match app_variant() {
        AppVariant::Official => "EasyCLI",
        AppVariant::Dev => "NiceCLI",
    }
}

#[cfg(target_os = "macos")]
fn launch_agent_label() -> &'static str {
    match app_variant() {
        AppVariant::Official => "com.easycli.app",
        AppVariant::Dev => "com.nicecli.app",
    }
}

#[cfg(target_os = "macos")]
fn launch_agent_file_name() -> &'static str {
    match app_variant() {
        AppVariant::Official => "com.easycli.app.plist",
        AppVariant::Dev => "com.nicecli.app.plist",
    }
}

#[cfg(target_os = "linux")]
fn autostart_file_name() -> &'static str {
    match app_variant() {
        AppVariant::Official => "easycli.desktop",
        AppVariant::Dev => "nicecli.desktop",
    }
}

fn app_dir() -> Result<PathBuf, AppError> {
    Ok(home_dir()?.join(app_storage_dir_name()))
}
#[tauri::command]
fn read_config_yaml() -> Result<serde_json::Value, String> {
    let dir = app_dir().map_err(|e| e.to_string())?;
    let p = dir.join("config.yaml");
    if !p.exists() {
        return Ok(json!({}));
    }
    let content = fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let v: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    let json_v = serde_json::to_value(v).map_err(|e| e.to_string())?;
    Ok(json_v)
}

#[tauri::command]
fn update_config_yaml(
    endpoint: String,
    value: serde_json::Value,
    is_delete: Option<bool>,
) -> Result<serde_json::Value, String> {
    let dir = app_dir().map_err(|e| e.to_string())?;
    let p = dir.join("config.yaml");
    if !p.exists() {
        return Err("Configuration file does not exist".into());
    }
    let content = fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let mut conf: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    let parts: Vec<&str> = endpoint.split('.').collect();
    // Descend mapping
    let mut current = conf.as_mapping_mut().ok_or("Invalid config structure")?;
    for (i, part) in parts.iter().enumerate() {
        let key = serde_yaml::Value::from(*part);
        if i == parts.len() - 1 {
            if is_delete.unwrap_or(false) {
                current.remove(&key);
            } else {
                current.insert(
                    key,
                    serde_yaml::to_value(&value).map_err(|e| e.to_string())?,
                );
            }
        } else {
            let entry = current
                .entry(key)
                .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
            if let Some(map) = entry.as_mapping_mut() {
                current = map;
            } else {
                return Err("Invalid nested config path".into());
            }
        }
    }
    let out = serde_yaml::to_string(&conf).map_err(|e| e.to_string())?;
    fs::write(&p, out).map_err(|e| e.to_string())?;
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

fn write_proxy_url_override(config_path: &Path, proxy_url: Option<&str>) -> Result<(), String> {
    let Some(proxy_url) = proxy_url.map(str::trim) else {
        return Ok(());
    };

    if proxy_url.is_empty() {
        return Ok(());
    }

    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut conf: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    let mapping = conf.as_mapping_mut().ok_or("Invalid config structure")?;
    mapping.insert(
        serde_yaml::Value::from("proxy-url"),
        serde_yaml::Value::from(proxy_url),
    );
    let updated = serde_yaml::to_string(&conf).map_err(|e| e.to_string())?;
    fs::write(config_path, updated).map_err(|e| e.to_string())
}

fn wait_for_listen_port(port: u16, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(150));
    }
    false
}

fn stop_managed_process() {
    let mut child = PROCESS.lock().take();
    if let Some(child) = child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    *PROCESS_PID.lock() = None;
    *CLI_PROXY_PASSWORD.lock() = None;
}

fn managed_process_running() -> bool {
    let mut guard = PROCESS.lock();
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => {
                *PROCESS_PID.lock() = Some(child.id());
                return true;
            }
            Ok(Some(_)) | Err(_) => {
                *guard = None;
                *PROCESS_PID.lock() = None;
            }
        }
    }
    false
}

fn start_monitor(app: tauri::AppHandle) {
    let proc_ref = Arc::clone(&PROCESS);
    thread::spawn(move || {
        loop {
            let mut remove = false;
            let mut exit_code: Option<i32> = None;
            {
                let mut guard = proc_ref.lock();
                if let Some(child) = guard.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            exit_code = status.code();
                            remove = true;
                        }
                        Ok(None) => {
                            // Still running
                        }
                        Err(_) => {
                            // Treat as closed
                            remove = true;
                        }
                    }
                } else {
                    // No process
                    break;
                }
            }
            if remove {
                // Clear stored process
                *proc_ref.lock() = None;
                // Emit event
                if let Some(code) = exit_code {
                    println!("[CLIProxyAPI][EXIT] process exited with code {}", code);
                } else {
                    println!("[CLIProxyAPI][EXIT] process closed (no exit code)");
                }
                if let Some(code) = exit_code {
                    let _ = app.emit("process-exit-error", json!({"code": code}));
                } else {
                    let _ = app.emit(
                        "process-closed",
                        json!({"message": "CLIProxyAPI process has closed"}),
                    );
                }
                // Remove tray icon when process exits
                let _ = TRAY_ICON.lock().take();
                break;
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });
}

// Kill any process using the specified port
fn kill_process_on_port(port: u16) -> Result<(), String> {
    println!("[PORT_CLEANUP] Checking port {}", port);

    #[cfg(target_os = "macos")]
    {
        // Use lsof to find the process
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
        // Use fuser to kill the process
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
        // Use netstat to find the PID, then taskkill to kill it
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
                    // Extract PID from the last column
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

#[tauri::command]
fn start_cliproxyapi(
    app: tauri::AppHandle,
    proxy_url: Option<String>,
) -> Result<serde_json::Value, String> {
    if managed_process_running() {
        return Ok(json!({
            "success": true,
            "message": "already running",
            "password": CLI_PROXY_PASSWORD.lock().clone(),
            "version": backend_payload::version(),
        }));
    }

    let dir = app_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let payload = backend_payload::ensure_backend_payload(&dir).map_err(|e| e.to_string())?;
    let config_path = backend_payload::ensure_default_config(&dir).map_err(|e| e.to_string())?;
    write_proxy_url_override(&config_path, proxy_url.as_deref())?;

    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let conf: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    let port = conf.get("port").and_then(|v| v.as_u64()).unwrap_or(8317) as u16;

    if let Err(e) = kill_process_on_port(port) {
        eprintln!("[PORT_CLEANUP] Warning: {}", e);
    }

    let password = generate_random_password();
    *CLI_PROXY_PASSWORD.lock() = Some(password.clone());

    let mut cmd = std::process::Command::new(&payload.exec_path);
    cmd.args([
        "-config",
        config_path.to_string_lossy().as_ref(),
        "--password",
        &password,
    ]);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().map_err(|e| {
        eprintln!("[CLIProxyAPI][ERROR] failed to start process: {}", e);
        e.to_string()
    })?;
    let pid = child.id();
    *PROCESS_PID.lock() = Some(pid);
    *PROCESS.lock() = Some(child);
    start_monitor(app.clone());

    if !wait_for_listen_port(port, 12_000) {
        stop_managed_process();
        return Err("CLIProxyAPI did not become ready in time".into());
    }

    let _ = create_tray(&app);

    Ok(json!({
        "success": true,
        "password": password,
        "version": backend_payload::version(),
    }))
}

#[tauri::command]
fn restart_cliproxyapi(app: tauri::AppHandle) -> Result<(), String> {
    stop_managed_process();
    let result = start_cliproxyapi(app.clone(), None)?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("cliproxyapi-restarted", result.clone());
    }
    Ok(())
}

fn stop_process_internal() {
    stop_managed_process();
    println!("[CLIProxyAPI][INFO] app closing - CLIProxyAPI stopped");
}

fn create_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::TrayIconBuilder,
    };
    let mut guard = TRAY_ICON.lock();
    if guard.is_some() {
        return Ok(());
    }

    let open_settings = MenuItemBuilder::with_id("open_settings", "Open Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open_settings, &quit])
        .build()?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip(app_display_name())
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_settings" => {
                let _ = open_settings_window(app.clone());
            }
            "quit" => {
                stop_process_internal();
                let _ = TRAY_ICON.lock().take();
                let _ = app.exit(0);
            }
            _ => {}
        });
    // Platform-specific tray icon
    #[cfg(target_os = "linux")]
    {
        const ICON_PNG: &[u8] = include_bytes!("../../images/icon.png");
        if let Ok(img) = image::load_from_memory(ICON_PNG) {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
            builder = builder.icon(icon);
        }
    }
    #[cfg(target_os = "windows")]
    {
        const ICON_ICO: &[u8] = include_bytes!("../../images/icon.ico");
        if let Ok(dir) = ico::IconDir::read(Cursor::new(ICON_ICO)) {
            if let Some(entry) = dir.entries().iter().max_by_key(|e| e.width()) {
                if let Ok(img) = entry.decode() {
                    let w = img.width();
                    let h = img.height();
                    let rgba = img.rgba_data().to_vec();
                    let icon = tauri::image::Image::new_owned(rgba, w, h);
                    builder = builder.icon(icon);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Try decode ICNS and convert to PNG buffer; fallback to PNG if needed.
        const ICON_ICNS: &[u8] = include_bytes!("../../images/icon.icns");
        let mut set = false;
        if let Ok(fam) = icns::IconFamily::read(Cursor::new(ICON_ICNS)) {
            use icns::IconType;
            let prefs = [
                IconType::RGBA32_512x512,
                IconType::RGBA32_256x256,
                IconType::RGBA32_128x128,
                IconType::RGBA32_64x64,
                IconType::RGBA32_32x32,
                IconType::RGBA32_16x16,
            ];
            for ty in prefs.iter() {
                if let Ok(icon_img) = fam.get_icon_with_type(*ty) {
                    let mut png_buf: Vec<u8> = Vec::new();
                    if icon_img.write_png(&mut png_buf).is_ok() {
                        if let Ok(img) = image::load_from_memory(&png_buf) {
                            let rgba = img.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                            builder = builder.icon(icon);
                            set = true;
                            break;
                        }
                    }
                }
            }
        }
        if !set {
            const ICON_PNG: &[u8] = include_bytes!("../../images/icon.png");
            if let Ok(img) = image::load_from_memory(ICON_PNG) {
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                builder = builder.icon(icon);
            }
        }
    }
    let tray = builder.build(app)?;
    *guard = Some(tray);
    Ok(())
}

fn open_main_window_page(app: &tauri::AppHandle, page: &str, title: &str) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let script = format!("window.location.replace({page:?});");
        win.eval(script).map_err(|e| e.to_string())?;
        let _ = win.set_title(title);
        let _ = win.show();
        let _ = win.set_focus();
        #[cfg(target_os = "macos")]
        {
            let _ = app.show();
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
            let _ = app.set_dock_visibility(true);
        }
        return Ok(());
    }

    let url = WebviewUrl::App(page.into());
    let win = WebviewWindowBuilder::new(app, "main", url)
        .title(title)
        .inner_size(CONTROL_PANEL_WINDOW_WIDTH, CONTROL_PANEL_WINDOW_HEIGHT)
        .min_inner_size(
            CONTROL_PANEL_MIN_WINDOW_WIDTH,
            CONTROL_PANEL_MIN_WINDOW_HEIGHT,
        )
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.show();
    let _ = win.set_focus();
    #[cfg(target_os = "macos")]
    {
        let _ = app.show();
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        let _ = app.set_dock_visibility(true);
    }
    Ok(())
}

fn callback_path_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "/anthropic/callback",
        "codex" => "/codex/callback",
        "google" => "/google/callback",
        "iflow" => "/iflow/callback",
        "antigravity" => "/antigravity/callback",
        _ => "/callback",
    }
}

fn build_redirect_url(
    mode: &str,
    provider: &str,
    base_url: Option<String>,
    local_port: Option<u16>,
    query: &str,
) -> String {
    let cb = callback_path_for(provider);
    let base = if mode == "local" {
        let port = local_port.unwrap_or(8317);
        format!("http://127.0.0.1:{}{}", port, cb)
    } else {
        let bu = base_url.unwrap_or_else(|| "http://127.0.0.1:8317".to_string());
        // ensure single slash
        if bu.ends_with('/') {
            format!("{}{}", bu, cb.trim_start_matches('/'))
        } else {
            format!("{}/{}", bu, cb.trim_start_matches('/'))
        }
    };
    if query.is_empty() {
        base
    } else {
        format!("{}?{}", base, query)
    }
}

fn run_callback_server(
    stop: Arc<AtomicBool>,
    listen_port: u16,
    mode: String,
    provider: String,
    base_url: Option<String>,
    local_port: Option<u16>,
) {
    let addr = format!("127.0.0.1:{}", listen_port);
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[CALLBACK] failed to bind {}: {}", addr, e);
            return;
        }
    };
    if let Err(e) = listener.set_nonblocking(false) {
        eprintln!("[CALLBACK] set_nonblocking failed: {}", e);
    }
    println!("[CALLBACK] listening on {} for provider {}", addr, provider);
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // read request line
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut req_line = String::new();
                if reader.read_line(&mut req_line).is_ok() {
                    let pathq = req_line.split_whitespace().nth(1).unwrap_or("/");
                    let query = pathq.splitn(2, '?').nth(1).unwrap_or("");
                    let loc =
                        build_redirect_url(&mode, &provider, base_url.clone(), local_port, query);
                    let resp = format!(
                        "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        loc
                    );
                    let _ = stream.write_all(resp.as_bytes());
                }
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
            Err(e) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                eprintln!("[CALLBACK] accept error: {}", e);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    println!("[CALLBACK] server on {} stopped", addr);
}

#[tauri::command]
fn start_callback_server(
    provider: String,
    listen_port: u16,
    mode: String,
    base_url: Option<String>,
    local_port: Option<u16>,
) -> Result<serde_json::Value, String> {
    let mut map = CALLBACK_SERVERS.lock();
    if let Some((flag, handle)) = map.remove(&listen_port) {
        flag.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(("127.0.0.1", listen_port));
        let _ = handle.join();
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let handle = thread::spawn(move || {
        run_callback_server(
            stop_clone,
            listen_port,
            mode,
            provider,
            base_url,
            local_port,
        )
    });
    map.insert(listen_port, (stop, handle));
    Ok(json!({"success": true}))
}

#[tauri::command]
fn stop_callback_server(listen_port: u16) -> Result<serde_json::Value, String> {
    // Take the server handle out of the map so it won't be stopped twice
    let opt = CALLBACK_SERVERS.lock().remove(&listen_port);
    if let Some((flag, handle)) = opt {
        // Signal stop and nudge the listener, then detach-join in background
        flag.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(("127.0.0.1", listen_port));
        std::thread::spawn(move || {
            let _ = handle.join();
        });
        Ok(json!({"success": true}))
    } else {
        Ok(json!({"success": false, "error": "not running"}))
    }
}

#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    open_main_window_page(
        &app,
        "settings.html",
        &format!("{} Control Panel", app_display_name()),
    )
}

#[tauri::command]
fn open_login_window(app: tauri::AppHandle) -> Result<(), String> {
    open_main_window_page(&app, "login.html", app_display_name())
}

// Auto-start functionality

#[cfg(target_os = "macos")]
fn get_launch_agent_path() -> Result<PathBuf, AppError> {
    let home = home_dir()?;
    Ok(home
        .join("Library/LaunchAgents")
        .join(launch_agent_file_name()))
}

#[cfg(target_os = "linux")]
fn get_autostart_path() -> Result<PathBuf, AppError> {
    let home = home_dir()?;
    Ok(home.join(".config/autostart").join(autostart_file_name()))
}

#[cfg(target_os = "macos")]
fn get_app_path() -> Result<String, AppError> {
    // Get the path to the current executable
    let exe = std::env::current_exe()?;

    // Navigate up from the executable to find the .app bundle
    // Typical path: /Applications/EasyCLI.app/Contents/MacOS/EasyCLI
    let mut path = exe.as_path();

    // Go up directories until we find the .app bundle
    while let Some(parent) = path.parent() {
        if let Some(file_name) = parent.file_name() {
            if file_name.to_string_lossy().ends_with(".app") {
                return Ok(parent.to_string_lossy().to_string());
            }
        }
        path = parent;
    }

    // Fallback: return the executable path
    Ok(exe.to_string_lossy().to_string())
}

#[cfg(target_os = "linux")]
fn get_app_path() -> Result<String, AppError> {
    let exe = std::env::current_exe()?;
    Ok(exe.to_string_lossy().to_string())
}

#[cfg(target_os = "windows")]
fn get_app_path() -> Result<String, AppError> {
    let exe = std::env::current_exe()?;
    Ok(exe.to_string_lossy().to_string())
}

#[tauri::command]
fn check_auto_start_enabled() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launch_agent_path().map_err(|e| e.to_string())?;
        Ok(json!({"enabled": plist_path.exists()}))
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_path = get_autostart_path().map_err(|e| e.to_string())?;
        Ok(json!({"enabled": desktop_path.exists()}))
    }

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

        match run_key {
            Ok(key) => match key.get_value::<String, _>(auto_start_entry_name()) {
                Ok(_) => Ok(json!({"enabled": true})),
                Err(_) => Ok(json!({"enabled": false})),
            },
            Err(_) => Ok(json!({"enabled": false})),
        }
    }
}

#[tauri::command]
fn enable_auto_start() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launch_agent_path().map_err(|e| e.to_string())?;
        let app_path = get_app_path().map_err(|e| e.to_string())?;

        // Create LaunchAgents directory if it doesn't exist
        if let Some(parent) = plist_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // Create plist content
        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/open</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
            launch_agent_label(),
            app_path
        );

        fs::write(&plist_path, plist_content).map_err(|e| e.to_string())?;
        Ok(json!({"success": true}))
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_path = get_autostart_path().map_err(|e| e.to_string())?;
        let app_path = get_app_path().map_err(|e| e.to_string())?;

        // Create autostart directory if it doesn't exist
        if let Some(parent) = desktop_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // Create .desktop file content
        let desktop_content = format!(
            r#"[Desktop Entry]
Type=Application
Name={}
Exec={}
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
Comment={} - API Proxy Management Tool"#,
            app_display_name(),
            app_path,
            app_display_name()
        );

        fs::write(&desktop_path, desktop_content).map_err(|e| e.to_string())?;
        Ok(json!({"success": true}))
    }

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let app_path = get_app_path().map_err(|e| e.to_string())?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                KEY_WRITE,
            )
            .map_err(|e| e.to_string())?;

        run_key
            .set_value(auto_start_entry_name(), &app_path)
            .map_err(|e| e.to_string())?;
        Ok(json!({"success": true}))
    }
}

#[tauri::command]
fn disable_auto_start() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launch_agent_path().map_err(|e| e.to_string())?;
        if plist_path.exists() {
            fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
        }
        Ok(json!({"success": true}))
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_path = get_autostart_path().map_err(|e| e.to_string())?;
        if desktop_path.exists() {
            fs::remove_file(&desktop_path).map_err(|e| e.to_string())?;
        }
        Ok(json!({"success": true}))
    }

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu.open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            KEY_WRITE,
        );

        if let Ok(key) = run_key {
            let _ = key.delete_value(auto_start_entry_name());
        }
        Ok(json!({"success": true}))
    }
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let has_tray = TRAY_ICON.lock().is_some();
                if has_tray {
                    api.prevent_close();
                    let _ = window.hide();
                    #[cfg(target_os = "macos")]
                    {
                        let _ = window
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                        let _ = window.app_handle().set_dock_visibility(false);
                    }
                    println!(
                        "[CLIProxyAPI][INFO] {} window hidden - app remains in tray",
                        window.label()
                    );
                    return;
                }
                // No tray icon yet (e.g., app closed before starting CLIProxyAPI) - allow default shutdown.
                println!(
                    "[CLIProxyAPI][INFO] {} window closed before tray initialization - exiting app",
                    window.label()
                );
            }
        })
        .invoke_handler(tauri::generate_handler![
            read_config_yaml,
            update_config_yaml,
            restart_cliproxyapi,
            start_cliproxyapi,
            open_settings_window,
            open_login_window,
            start_callback_server,
            stop_callback_server,
            save_files_to_directory,
            check_auto_start_enabled,
            enable_auto_start,
            disable_auto_start
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_, event| {
        if matches!(
            event,
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
        ) {
            stop_process_internal();
        }
    });
}

#[derive(Deserialize)]
struct SaveFile {
    name: String,
    content: String,
}

#[tauri::command]
fn save_files_to_directory(files: Vec<SaveFile>) -> Result<serde_json::Value, String> {
    if files.is_empty() {
        return Ok(json!({"success": false, "error": "No files to save"}));
    }
    // Show a system directory picker to choose the destination folder
    let folder = FileDialog::new()
        .set_title("Choose save directory")
        .pick_folder()
        .ok_or_else(|| "User cancelled directory selection".to_string())?;

    // Write each file into the chosen directory
    let mut success: usize = 0;
    let mut error_count: usize = 0;
    let mut errors: Vec<String> = Vec::new();
    for f in files {
        let path = folder.join(&f.name);
        match fs::write(&path, f.content.as_bytes()) {
            Ok(_) => success += 1,
            Err(e) => {
                error_count += 1;
                errors.push(format!("{}: {}", f.name, e));
            }
        }
    }

    Ok(json!({
        "success": success > 0,
        "successCount": success,
        "errorCount": error_count,
        "errors": if errors.is_empty() { serde_json::Value::Null } else { json!(errors) }
    }))
}
