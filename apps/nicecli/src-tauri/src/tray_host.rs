use crate::app_identity::{
    app_display_name, tray_menu_action, TrayMenuAction, TRAY_MENU_OPEN_SETTINGS_ID,
    TRAY_MENU_QUIT_ID,
};
use crate::windowing::open_settings_window;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::io::Cursor;
use std::sync::Arc;
use tauri::tray::TrayIcon;

static TRAY_ICON: Lazy<Arc<Mutex<Option<TrayIcon>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

pub(crate) fn clear_tray_icon() {
    let _ = TRAY_ICON.lock().take();
}

pub(crate) fn tray_icon_initialized() -> bool {
    TRAY_ICON.lock().is_some()
}

pub(crate) fn create_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::TrayIconBuilder,
    };

    let mut guard = TRAY_ICON.lock();
    if guard.is_some() {
        return Ok(());
    }

    let open_settings =
        MenuItemBuilder::with_id(TRAY_MENU_OPEN_SETTINGS_ID, "Open Settings").build(app)?;
    let quit = MenuItemBuilder::with_id(TRAY_MENU_QUIT_ID, "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open_settings, &quit])
        .build()?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip(app_display_name())
        .on_menu_event(|app, event| match tray_menu_action(event.id().as_ref()) {
            TrayMenuAction::OpenSettings => {
                let _ = open_settings_window(app.clone());
            }
            TrayMenuAction::Quit => {
                crate::managed_host::stop_process_internal();
                clear_tray_icon();
                app.exit(0);
            }
            TrayMenuAction::Ignore => {}
        });

    #[cfg(target_os = "linux")]
    {
        const ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
        if let Ok(img) = image::load_from_memory(ICON_PNG) {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
            builder = builder.icon(icon);
        }
    }

    #[cfg(target_os = "windows")]
    {
        const ICON_ICO: &[u8] = include_bytes!("../icons/icon.ico");
        if let Ok(dir) = ico::IconDir::read(Cursor::new(ICON_ICO)) {
            if let Some(entry) = dir.entries().iter().max_by_key(|entry| entry.width()) {
                if let Ok(img) = entry.decode() {
                    let w = img.width();
                    let h = img.height();
                    let rgba = img.rgba_data().to_vec();
                    let icon = tauri::image::Image::new_owned(rgba, w, h);
                    builder = builder.icon(icon);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        const ICON_ICNS: &[u8] = include_bytes!("../icons/icon.icns");
        let mut set = false;
        if let Ok(fam) = icns::IconFamily::read(Cursor::new(ICON_ICNS)) {
            use icns::IconType;
            let prefs = [
                IconType::RGBA32_512x512,
                IconType::RGBA32_256x256,
                IconType::RGBA32_128x128,
                IconType::RGBA32_64x64,
                IconType::RGBA32_32x32,
                IconType::RGBA32_16x16,
            ];
            for ty in prefs.iter() {
                if let Ok(icon_img) = fam.get_icon_with_type(*ty) {
                    let mut png_buf: Vec<u8> = Vec::new();
                    if icon_img.write_png(&mut png_buf).is_ok() {
                        if let Ok(img) = image::load_from_memory(&png_buf) {
                            let rgba = img.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                            builder = builder.icon(icon);
                            set = true;
                            break;
                        }
                    }
                }
            }
        }
        if !set {
            const ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
            if let Ok(img) = image::load_from_memory(ICON_PNG) {
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                builder = builder.icon(icon);
            }
        }
    }

    let tray = builder.build(app)?;
    *guard = Some(tray);
    Ok(())
}
