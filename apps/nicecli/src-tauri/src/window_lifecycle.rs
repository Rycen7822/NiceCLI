use crate::app_identity::{close_request_action, CloseRequestAction};
use crate::tray_host::tray_icon_initialized;
use tauri::{Window, WindowEvent};

pub(crate) fn handle_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        match close_request_action(tray_icon_initialized()) {
            CloseRequestAction::HideToTray => {
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
                    "[NiceCLI][INFO] {} window hidden - app remains in tray",
                    window.label()
                );
            }
            CloseRequestAction::ExitApp => {
                println!(
                    "[NiceCLI][INFO] {} window closed before tray initialization - exiting app",
                    window.label()
                );
            }
        }
    }
}
