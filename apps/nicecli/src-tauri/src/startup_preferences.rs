use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const STARTUP_PREFERENCES_FILE_NAME: &str = "startup-preferences.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct StartupPreferences {
    pub auto_login: bool,
    pub silent_startup: bool,
}

impl StartupPreferences {
    fn file_path(app_dir: &Path) -> PathBuf {
        app_dir.join(STARTUP_PREFERENCES_FILE_NAME)
    }

    pub(crate) fn load(app_dir: &Path) -> Result<Self, String> {
        let file_path = Self::file_path(app_dir);
        if !file_path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&file_path).map_err(|error| {
            format!(
                "Failed to read startup preferences {}: {}",
                file_path.display(),
                error
            )
        })?;

        serde_json::from_str(&raw).map_err(|error| {
            format!(
                "Failed to parse startup preferences {}: {}",
                file_path.display(),
                error
            )
        })
    }

    pub(crate) fn save(&self, app_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(app_dir).map_err(|error| {
            format!(
                "Failed to create startup preferences directory {}: {}",
                app_dir.display(),
                error
            )
        })?;

        let file_path = Self::file_path(app_dir);
        let raw = serde_json::to_string_pretty(self).map_err(|error| {
            format!(
                "Failed to encode startup preferences {}: {}",
                file_path.display(),
                error
            )
        })?;

        fs::write(&file_path, raw).map_err(|error| {
            format!(
                "Failed to write startup preferences {}: {}",
                file_path.display(),
                error
            )
        })
    }
}

#[tauri::command]
pub(crate) fn read_startup_preferences() -> Result<StartupPreferences, String> {
    let app_dir = crate::app_dir().map_err(|error| error.to_string())?;
    StartupPreferences::load(&app_dir)
}

#[tauri::command]
pub(crate) fn update_startup_preferences(
    preferences: StartupPreferences,
) -> Result<StartupPreferences, String> {
    let app_dir = crate::app_dir().map_err(|error| error.to_string())?;
    preferences.save(&app_dir)?;
    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::StartupPreferences;
    use std::fs;
    use std::path::PathBuf;

    fn test_app_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nicecli-startup-preferences-{}-{}-{}",
            name,
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_returns_default_when_file_is_missing() {
        let dir = test_app_dir("missing");
        let prefs = StartupPreferences::load(&dir).unwrap();
        assert_eq!(prefs, StartupPreferences::default());
        cleanup(&dir);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = test_app_dir("round-trip");
        let prefs = StartupPreferences {
            auto_login: true,
            silent_startup: true,
        };

        prefs.save(&dir).unwrap();

        let loaded = StartupPreferences::load(&dir).unwrap();
        assert_eq!(loaded, prefs);
        cleanup(&dir);
    }

    #[test]
    fn load_fails_for_invalid_json() {
        let dir = test_app_dir("invalid-json");
        let file_path = dir.join("startup-preferences.json");
        fs::write(&file_path, "{invalid json").unwrap();

        let error = StartupPreferences::load(&dir).unwrap_err();
        assert!(error.contains("Failed to parse startup preferences"));
        cleanup(&dir);
    }
}
