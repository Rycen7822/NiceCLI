use crate::{BackendLaunch, BackendLaunchMode, ManagedBackendInfo};
use nicecli_config::{set_proxy_url_override as persist_proxy_url_override, NiceCliConfig};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const BACKEND_MODE_ENV: &str = "NICECLI_BACKEND_MODE";
const RUST_BACKEND_BIN_ENV: &str = "NICECLI_RUST_BACKEND_BIN";

fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn detect_backend_launch_mode() -> Result<BackendLaunchMode, String> {
    let explicit_binary = env_trimmed(RUST_BACKEND_BIN_ENV).map(PathBuf::from);
    let requested_mode = env_trimmed(BACKEND_MODE_ENV)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if explicit_binary.is_none() && requested_mode.is_empty() {
        return Ok(BackendLaunchMode::RustInProcess);
    }

    if explicit_binary.is_none()
        && requested_mode != "rust"
        && requested_mode != "rust-backend"
        && requested_mode != "rust-external"
    {
        return Err(format!(
            "Unsupported {} value: {}",
            BACKEND_MODE_ENV, requested_mode
        ));
    }

    let exec_path = explicit_binary.or_else(discover_rust_backend_executable).ok_or_else(|| {
        format!(
            "Rust backend requested but executable was not found. Build crates/nicecli-backend first or set {}.",
            RUST_BACKEND_BIN_ENV
        )
    })?;

    if !exec_path.exists() {
        return Err(format!(
            "Rust backend executable not found: {}",
            exec_path.display()
        ));
    }

    Ok(BackendLaunchMode::RustExternal { exec_path })
}

fn discover_rust_backend_executable() -> Option<PathBuf> {
    let workspace_root = std::env::current_dir()
        .ok()
        .and_then(|path| find_workspace_root(&path))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| find_workspace_root(&path))
        })?;

    let executable_name = rust_backend_executable_name();
    let profiles = if cfg!(debug_assertions) {
        ["debug", "release"]
    } else {
        ["release", "debug"]
    };

    profiles
        .iter()
        .map(|profile| {
            workspace_root
                .join("target")
                .join(profile)
                .join(executable_name)
        })
        .find(|path| path.exists())
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    };

    while let Some(path) = current {
        if path
            .join("crates")
            .join("nicecli-backend")
            .join("Cargo.toml")
            .exists()
        {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }

    None
}

#[cfg(target_os = "windows")]
fn rust_backend_executable_name() -> &'static str {
    "nicecli-backend.exe"
}

#[cfg(not(target_os = "windows"))]
fn rust_backend_executable_name() -> &'static str {
    "nicecli-backend"
}

pub(crate) fn prepare_backend_launch(
    app_dir: &Path,
    proxy_url: Option<&str>,
) -> Result<BackendLaunch, String> {
    let config_path =
        crate::backend_payload::ensure_default_config(app_dir).map_err(|e| e.to_string())?;
    persist_proxy_url_override(&config_path, proxy_url).map_err(|e| e.to_string())?;

    match detect_backend_launch_mode()? {
        BackendLaunchMode::RustInProcess => Ok(BackendLaunch::InProcess {
            config_path,
            info: ManagedBackendInfo {
                mode: "rust-in-process".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }),
        BackendLaunchMode::RustExternal { exec_path } => Ok(BackendLaunch::Child {
            exec_path,
            config_path,
            info: ManagedBackendInfo {
                mode: "rust-external".to_string(),
                version: "nicecli-backend-dev".to_string(),
            },
        }),
    }
}

pub(crate) fn wait_for_listen_port(port: u16, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(150));
    }
    false
}

pub(crate) fn load_backend_port(config_path: &Path) -> Result<u16, String> {
    NiceCliConfig::load_from_path(config_path)
        .map(|config| config.effective_port())
        .map_err(|e| e.to_string())
}
