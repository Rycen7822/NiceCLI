use once_cell::sync::Lazy;

pub(crate) const TRAY_MENU_OPEN_SETTINGS_ID: &str = "open_settings";
pub(crate) const TRAY_MENU_QUIT_ID: &str = "quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppVariant {
    Official,
    Dev,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseRequestAction {
    HideToTray,
    ExitApp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayMenuAction {
    OpenSettings,
    Quit,
    Ignore,
}

static APP_VARIANT: Lazy<AppVariant> = Lazy::new(detect_app_variant);

fn detect_app_variant() -> AppVariant {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
        })
        .unwrap_or_default();

    if stem.contains("nicecli") || stem.contains("easycli-dev") {
        AppVariant::Dev
    } else {
        AppVariant::Official
    }
}

fn app_variant() -> AppVariant {
    *APP_VARIANT
}

pub(crate) fn app_storage_dir_name_for_variant(variant: AppVariant) -> &'static str {
    match variant {
        // Keep the historical official directory name for data compatibility.
        AppVariant::Official => "cliproxyapi",
        AppVariant::Dev => "nicecli",
    }
}

pub(crate) fn app_storage_dir_name() -> &'static str {
    app_storage_dir_name_for_variant(app_variant())
}

pub(crate) fn app_display_name_for_variant(variant: AppVariant) -> &'static str {
    match variant {
        AppVariant::Official => "EasyCLI",
        AppVariant::Dev => "NiceCLI",
    }
}

pub(crate) fn app_display_name() -> &'static str {
    app_display_name_for_variant(app_variant())
}

pub(crate) fn auto_start_entry_name_for_variant(variant: AppVariant) -> &'static str {
    match variant {
        AppVariant::Official => "EasyCLI",
        AppVariant::Dev => "NiceCLI",
    }
}

pub(crate) fn auto_start_entry_name() -> &'static str {
    auto_start_entry_name_for_variant(app_variant())
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_agent_label() -> &'static str {
    match app_variant() {
        AppVariant::Official => "com.easycli.app",
        AppVariant::Dev => "com.nicecli.app",
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_agent_file_name() -> &'static str {
    match app_variant() {
        AppVariant::Official => "com.easycli.app.plist",
        AppVariant::Dev => "com.nicecli.app.plist",
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn autostart_file_name_for_variant(variant: AppVariant) -> &'static str {
    match variant {
        AppVariant::Official => "easycli.desktop",
        AppVariant::Dev => "nicecli.desktop",
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn autostart_file_name() -> &'static str {
    autostart_file_name_for_variant(app_variant())
}

pub(crate) fn close_request_action(has_tray: bool) -> CloseRequestAction {
    if has_tray {
        CloseRequestAction::HideToTray
    } else {
        CloseRequestAction::ExitApp
    }
}

pub(crate) fn tray_menu_action(menu_id: &str) -> TrayMenuAction {
    match menu_id {
        TRAY_MENU_OPEN_SETTINGS_ID => TrayMenuAction::OpenSettings,
        TRAY_MENU_QUIT_ID => TrayMenuAction::Quit,
        _ => TrayMenuAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        app_display_name_for_variant, app_storage_dir_name_for_variant,
        auto_start_entry_name_for_variant, close_request_action, tray_menu_action, AppVariant,
        CloseRequestAction, TrayMenuAction,
    };

    #[test]
    fn dev_variant_keeps_nicecli_identity() {
        assert_eq!(app_storage_dir_name_for_variant(AppVariant::Dev), "nicecli");
        assert_eq!(app_display_name_for_variant(AppVariant::Dev), "NiceCLI");
        assert_eq!(
            auto_start_entry_name_for_variant(AppVariant::Dev),
            "NiceCLI"
        );
    }

    #[test]
    fn official_variant_keeps_easycli_identity() {
        assert_eq!(
            app_storage_dir_name_for_variant(AppVariant::Official),
            "cliproxyapi"
        );
        assert_eq!(
            app_display_name_for_variant(AppVariant::Official),
            "EasyCLI"
        );
        assert_eq!(
            auto_start_entry_name_for_variant(AppVariant::Official),
            "EasyCLI"
        );
    }

    #[test]
    fn close_request_hides_to_tray_when_icon_exists() {
        assert_eq!(close_request_action(true), CloseRequestAction::HideToTray);
    }

    #[test]
    fn close_request_exits_without_tray_icon() {
        assert_eq!(close_request_action(false), CloseRequestAction::ExitApp);
    }

    #[test]
    fn tray_menu_routes_open_settings_action() {
        assert_eq!(
            tray_menu_action("open_settings"),
            TrayMenuAction::OpenSettings
        );
    }

    #[test]
    fn tray_menu_routes_quit_action() {
        assert_eq!(tray_menu_action("quit"), TrayMenuAction::Quit);
    }

    #[test]
    fn tray_menu_ignores_unknown_action() {
        assert_eq!(tray_menu_action("unknown"), TrayMenuAction::Ignore);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dev_variant_uses_nicecli_autostart_file() {
        assert_eq!(
            super::autostart_file_name_for_variant(AppVariant::Dev),
            "nicecli.desktop"
        );
        assert_eq!(
            super::autostart_file_name_for_variant(AppVariant::Official),
            "easycli.desktop"
        );
    }
}
