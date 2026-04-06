use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CONFIG_TEMPLATE_TEXT: &str = include_str!("../resources/default-config.yaml");
const AUTH_DIR_PLACEHOLDER: &str = "__NICECLI_AUTH_DIR__";
const DEFAULT_AUTH_DIR: &str = "~/.cli-proxy-api";

pub fn ensure_default_config(app_dir: &Path) -> io::Result<PathBuf> {
    let config_path = app_dir.join("config.yaml");
    if config_path.exists() {
        return Ok(config_path);
    }

    fs::create_dir_all(app_dir)?;

    let config_text = CONFIG_TEMPLATE_TEXT.replace(AUTH_DIR_PLACEHOLDER, DEFAULT_AUTH_DIR);
    fs::write(&config_path, config_text)?;
    Ok(config_path)
}

#[cfg(test)]
mod tests {
    use super::{ensure_default_config, AUTH_DIR_PLACEHOLDER, DEFAULT_AUTH_DIR};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("nicecli-config-test-{suffix}"))
    }

    #[test]
    fn ensure_default_config_uses_user_home_auth_dir_mapping() {
        let app_dir = unique_temp_dir();
        let config_path = ensure_default_config(&app_dir).expect("config created");
        let config_text = fs::read_to_string(&config_path).expect("config text");

        assert!(config_path.ends_with("config.yaml"));
        assert!(config_text.contains("host: \"127.0.0.1\""));
        assert!(config_text.contains("auth-dir:"));
        assert!(config_text.contains(DEFAULT_AUTH_DIR));
        assert!(!config_text.contains(AUTH_DIR_PLACEHOLDER));
        let _ = fs::remove_dir_all(&app_dir);
    }
}
