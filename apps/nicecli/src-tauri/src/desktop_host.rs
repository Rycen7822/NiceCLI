use crate::local_runtime::start_local_runtime_internal;
use crate::managed_host::{stop_managed_process, stop_process_internal};
use crate::startup_preferences::StartupPreferences;
use crate::tray_host::clear_tray_icon;
use crate::tray_smoke::maybe_start_tray_host_smoke;
use crate::windowing::{open_login_window, open_settings_window, write_local_runtime_session};
use tauri::Manager;

fn initialize_main_window(
    app: &tauri::App<tauri::Wry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.handle().clone();
    let startup_preferences = match crate::app_dir()
        .map_err(|error| error.to_string())
        .and_then(|app_dir| StartupPreferences::load(&app_dir))
    {
        Ok(preferences) => preferences,
        Err(error) => {
            eprintln!("[NiceCLI][WARN] failed to load startup preferences: {error}");
            StartupPreferences::default()
        }
    };

    if startup_preferences.auto_login {
        match start_local_runtime_internal(app_handle.clone(), None) {
            Ok(response) => {
                let startup_result = response
                    .password
                    .as_deref()
                    .ok_or_else(|| "local runtime started without management key".to_string())
                    .and_then(|password| write_local_runtime_session(&app_handle, password))
                    .and_then(|_| {
                        if startup_preferences.silent_startup {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.hide();
                            }
                            Ok(())
                        } else {
                            open_settings_window(app_handle.clone())
                        }
                    });

                if startup_result.is_ok() {
                    return Ok(());
                }

                if let Err(error) = startup_result {
                    eprintln!("[NiceCLI][WARN] auto login startup fallback: {error}");
                    stop_managed_process();
                    clear_tray_icon();
                }
            }
            Err(error) => {
                eprintln!("[NiceCLI][WARN] auto login failed to start local runtime: {error}");
            }
        }
    }

    open_login_window(app_handle)
        .map_err(|error| Box::<dyn std::error::Error>::from(std::io::Error::other(error)))
}

pub(crate) fn setup_app(
    app: &mut tauri::App<tauri::Wry>,
) -> Result<(), Box<dyn std::error::Error>> {
    maybe_start_tray_host_smoke(app)?;
    initialize_main_window(app)
}

pub(crate) fn run_app(app: tauri::App<tauri::Wry>) {
    app.run(|_, event| {
        if matches!(
            event,
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
        ) {
            stop_process_internal();
        }
    });
}
