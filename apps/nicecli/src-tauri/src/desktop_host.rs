use crate::managed_host::stop_process_internal;
use crate::tray_smoke::maybe_start_tray_host_smoke;

pub(crate) fn setup_app(
    app: &mut tauri::App<tauri::Wry>,
) -> Result<(), Box<dyn std::error::Error>> {
    maybe_start_tray_host_smoke(app)
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
