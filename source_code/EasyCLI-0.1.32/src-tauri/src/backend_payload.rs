use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded_backend.rs"));

pub struct BackendPayloadPaths {
    pub exec_path: PathBuf,
}

pub fn version() -> &'static str {
    EMBEDDED_BACKEND_VERSION
}

pub fn ensure_backend_payload(app_dir: &Path) -> io::Result<BackendPayloadPaths> {
    let runtime_root = app_dir.join("runtime").join("cliproxyapi");
    let version_file = runtime_root.join("version.txt");
    let version_dir = runtime_root.join(EMBEDDED_BACKEND_VERSION);
    let exec_path = version_dir.join(EMBEDDED_BACKEND_FILENAME);
    let config_example_path = version_dir.join("config.example.yaml");

    let current_version = fs::read_to_string(&version_file).ok();
    let needs_refresh = current_version
        .as_deref()
        .map(|value| value.trim() != EMBEDDED_BACKEND_VERSION)
        .unwrap_or(true)
        || !exec_path.exists()
        || !config_example_path.exists();

    if needs_refresh {
        if runtime_root.exists() {
            fs::remove_dir_all(&runtime_root)?;
        }
        fs::create_dir_all(&version_dir)?;
        fs::write(&exec_path, EMBEDDED_BACKEND_BYTES)?;
        fs::write(&config_example_path, EMBEDDED_CONFIG_EXAMPLE)?;
        fs::write(&version_file, EMBEDDED_BACKEND_VERSION)?;

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&exec_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&exec_path, perms)?;
        }
    }

    Ok(BackendPayloadPaths { exec_path })
}

pub fn ensure_default_config(app_dir: &Path) -> io::Result<PathBuf> {
    let config_path = app_dir.join("config.yaml");
    if config_path.exists() {
        return Ok(config_path);
    }

    fs::create_dir_all(app_dir)?;

    let auth_dir = normalize_yaml_path(&app_dir.join("auth"));
    let config_text = EMBEDDED_CONFIG_EXAMPLE.replace(
        "auth-dir: \"~/.cli-proxy-api\"",
        &format!("auth-dir: \"{auth_dir}\""),
    );
    fs::write(&config_path, config_text)?;
    Ok(config_path)
}

fn normalize_yaml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
