#[cfg(target_os = "linux")]
use crate::app_identity::app_display_name;
#[cfg(target_os = "windows")]
use crate::app_identity::auto_start_entry_name;
#[cfg(target_os = "linux")]
use crate::app_identity::autostart_file_name;
#[cfg(target_os = "macos")]
use crate::app_identity::{launch_agent_file_name, launch_agent_label};
use serde_json::{json, Value};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::fs;
use std::io;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

#[cfg(target_os = "windows")]
const WINDOWS_RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn home_dir() -> Result<PathBuf, String> {
    home::home_dir().ok_or_else(|| "Failed to resolve home directory".to_string())
}

#[cfg(target_os = "macos")]
fn get_launch_agent_path() -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join("Library/LaunchAgents")
        .join(launch_agent_file_name()))
}

#[cfg(target_os = "linux")]
fn get_autostart_path() -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join(".config/autostart")
        .join(autostart_file_name()))
}

#[cfg(target_os = "macos")]
fn get_app_path() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut path = exe.as_path();

    while let Some(parent) = path.parent() {
        if let Some(file_name) = parent.file_name() {
            if file_name.to_string_lossy().ends_with(".app") {
                return Ok(parent.to_string_lossy().to_string());
            }
        }
        path = parent;
    }

    Ok(exe.to_string_lossy().to_string())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn get_app_path() -> Result<String, String> {
    std::env::current_exe()
        .map(|exe| exe.to_string_lossy().to_string())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn format_windows_auto_start_value(app_path: &str) -> String {
    if app_path.starts_with('"') && app_path.ends_with('"') {
        return app_path.to_string();
    }

    if app_path.contains([' ', '\t']) {
        format!("\"{app_path}\"")
    } else {
        app_path.to_string()
    }
}

#[cfg(target_os = "windows")]
fn windows_autostart_enabled_in_subkey(
    run_key_path: &str,
    entry_name: &str,
) -> Result<bool, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(run_key_path) {
        Ok(key) => match key.get_value::<String, _>(entry_name) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string()),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(target_os = "windows")]
fn set_windows_autostart_entry_in_subkey(
    run_key_path: &str,
    entry_name: &str,
    value: &str,
) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(run_key_path)
        .map_err(|error| error.to_string())?;
    key.set_value(entry_name, &value)
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn delete_windows_autostart_entry_in_subkey(
    run_key_path: &str,
    entry_name: &str,
) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(run_key_path, KEY_WRITE) {
        Ok(key) => match key.delete_value(entry_name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub(crate) fn check_auto_start_enabled() -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launch_agent_path()?;
        return Ok(json!({"enabled": plist_path.exists()}));
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_path = get_autostart_path()?;
        return Ok(json!({"enabled": desktop_path.exists()}));
    }

    #[cfg(target_os = "windows")]
    {
        let enabled =
            windows_autostart_enabled_in_subkey(WINDOWS_RUN_SUBKEY, auto_start_entry_name())?;
        return Ok(json!({"enabled": enabled}));
    }

    #[allow(unreachable_code)]
    Ok(json!({"enabled": false}))
}

#[tauri::command]
pub(crate) fn enable_auto_start() -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launch_agent_path()?;
        let app_path = get_app_path()?;

        if let Some(parent) = plist_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

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

        fs::write(&plist_path, plist_content).map_err(|error| error.to_string())?;
        return Ok(json!({"success": true}));
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_path = get_autostart_path()?;
        let app_path = get_app_path()?;

        if let Some(parent) = desktop_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

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

        fs::write(&desktop_path, desktop_content).map_err(|error| error.to_string())?;
        return Ok(json!({"success": true}));
    }

    #[cfg(target_os = "windows")]
    {
        let app_path = format_windows_auto_start_value(&get_app_path()?);
        set_windows_autostart_entry_in_subkey(
            WINDOWS_RUN_SUBKEY,
            auto_start_entry_name(),
            &app_path,
        )?;
        return Ok(json!({"success": true}));
    }

    #[allow(unreachable_code)]
    Ok(json!({"success": false, "error": "unsupported platform"}))
}

#[tauri::command]
pub(crate) fn disable_auto_start() -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launch_agent_path()?;
        if plist_path.exists() {
            fs::remove_file(&plist_path).map_err(|error| error.to_string())?;
        }
        return Ok(json!({"success": true}));
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_path = get_autostart_path()?;
        if desktop_path.exists() {
            fs::remove_file(&desktop_path).map_err(|error| error.to_string())?;
        }
        return Ok(json!({"success": true}));
    }

    #[cfg(target_os = "windows")]
    {
        delete_windows_autostart_entry_in_subkey(WINDOWS_RUN_SUBKEY, auto_start_entry_name())?;
        return Ok(json!({"success": true}));
    }

    #[allow(unreachable_code)]
    Ok(json!({"success": false, "error": "unsupported platform"}))
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use super::{
        delete_windows_autostart_entry_in_subkey, format_windows_auto_start_value,
        set_windows_autostart_entry_in_subkey, windows_autostart_enabled_in_subkey,
    };
    #[cfg(target_os = "windows")]
    use winreg::enums::HKEY_CURRENT_USER;
    #[cfg(target_os = "windows")]
    use winreg::RegKey;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_autostart_value_quotes_spaced_paths() {
        assert_eq!(
            format_windows_auto_start_value(r"C:\Program Files\NiceCLI\nicecli.exe"),
            r#""C:\Program Files\NiceCLI\nicecli.exe""#
        );
        assert_eq!(
            format_windows_auto_start_value(r"C:\NiceCLI\nicecli.exe"),
            r"C:\NiceCLI\nicecli.exe"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_autostart_value_keeps_existing_quotes() {
        assert_eq!(
            format_windows_auto_start_value(r#""C:\Program Files\NiceCLI\nicecli.exe""#),
            r#""C:\Program Files\NiceCLI\nicecli.exe""#
        );
    }

    #[cfg(target_os = "windows")]
    fn windows_test_run_subkey() -> String {
        format!(
            "Software\\NiceCLI\\Tests\\Autostart\\{}_{}",
            std::process::id(),
            rand::random::<u64>()
        )
    }

    #[cfg(target_os = "windows")]
    fn cleanup_windows_test_subkey(path: &str) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_autostart_registry_helpers_round_trip_entry() {
        let subkey = windows_test_run_subkey();
        let entry_name = "NiceCLI Test";
        let entry_value = format_windows_auto_start_value(r"C:\Program Files\NiceCLI\nicecli.exe");

        cleanup_windows_test_subkey(&subkey);

        assert!(!windows_autostart_enabled_in_subkey(&subkey, entry_name).unwrap());

        set_windows_autostart_entry_in_subkey(&subkey, entry_name, &entry_value).unwrap();
        assert!(windows_autostart_enabled_in_subkey(&subkey, entry_name).unwrap());

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey(&subkey).unwrap();
        let stored_value: String = key.get_value(entry_name).unwrap();
        assert_eq!(stored_value, entry_value);

        delete_windows_autostart_entry_in_subkey(&subkey, entry_name).unwrap();
        assert!(!windows_autostart_enabled_in_subkey(&subkey, entry_name).unwrap());

        cleanup_windows_test_subkey(&subkey);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_autostart_registry_delete_is_idempotent_for_missing_entry() {
        let subkey = windows_test_run_subkey();
        let entry_name = "NiceCLI Missing";

        cleanup_windows_test_subkey(&subkey);
        delete_windows_autostart_entry_in_subkey(&subkey, entry_name).unwrap();
        assert!(!windows_autostart_enabled_in_subkey(&subkey, entry_name).unwrap());
        cleanup_windows_test_subkey(&subkey);
    }
}
