use crate::app_identity::app_display_name;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const CONTROL_PANEL_WINDOW_WIDTH: f64 = 1116.0;
const CONTROL_PANEL_WINDOW_HEIGHT: f64 = 720.0;
const CONTROL_PANEL_MIN_WINDOW_WIDTH: f64 = 912.0;
const CONTROL_PANEL_MIN_WINDOW_HEIGHT: f64 = 624.0;

fn open_main_window_page(app: &AppHandle, page: &str, title: &str) -> Result<(), String> {
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

#[tauri::command]
pub(crate) fn open_settings_window(app: AppHandle) -> Result<(), String> {
    open_main_window_page(
        &app,
        "settings.html",
        &format!("{} Control Panel", app_display_name()),
    )
}

#[tauri::command]
pub(crate) fn open_login_window(app: AppHandle) -> Result<(), String> {
    open_main_window_page(&app, "login.html", app_display_name())
}
