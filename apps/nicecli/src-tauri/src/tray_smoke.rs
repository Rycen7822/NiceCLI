use crate::tray_host::{create_tray, tray_icon_initialized};
use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Serialize)]
struct TrayHostSmokeResult {
    success: bool,
    tray_initialized: bool,
    window_exists_after_close: bool,
    window_visible_after_close: bool,
    window_hidden_to_tray: bool,
}

fn tray_host_smoke_requested() -> bool {
    std::env::args().any(|arg| arg == "--smoke-tray-host")
}

pub(crate) fn maybe_start_tray_host_smoke(
    app: &tauri::App,
) -> Result<(), Box<dyn std::error::Error>> {
    if !tray_host_smoke_requested() {
        return Ok(());
    }

    create_tray(app.handle())
        .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

    let app_handle = app.handle().clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<TrayHostSmokeResult, String> {
            std::thread::sleep(std::time::Duration::from_millis(750));

            let tray_initialized = tray_icon_initialized();
            let window = app_handle
                .get_webview_window("main")
                .ok_or_else(|| "main window not found during tray smoke".to_string())?;
            window.close().map_err(|e| e.to_string())?;

            std::thread::sleep(std::time::Duration::from_millis(750));

            let (window_exists_after_close, window_visible_after_close) =
                match app_handle.get_webview_window("main") {
                    Some(window) => (true, window.is_visible().map_err(|e| e.to_string())?),
                    None => (false, false),
                };
            let window_hidden_to_tray = window_exists_after_close && !window_visible_after_close;

            Ok(TrayHostSmokeResult {
                success: tray_initialized && window_hidden_to_tray,
                tray_initialized,
                window_exists_after_close,
                window_visible_after_close,
                window_hidden_to_tray,
            })
        })();

        let exit_code = match &result {
            Ok(result) => {
                let payload = serde_json::to_string(result).unwrap_or_else(|_| {
                    "{\"success\":false,\"error\":\"failed to serialize tray host smoke result\"}"
                        .to_string()
                });
                if result.success {
                    println!("{payload}");
                    0
                } else {
                    eprintln!("{payload}");
                    1
                }
            }
            Err(error) => {
                eprintln!("tray host smoke failed: {error}");
                1
            }
        };

        app_handle.exit(exit_code);
    });

    Ok(())
}
