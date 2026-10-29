use tauri::AppHandle;
use tauri_plugin_global_shortcut::ShortcutState;

use crate::settings;

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let shortcut = settings::load(app).shortcut;
    let capture = MenuItem::with_id(app, "capture", "截图", true, Some(shortcut.as_str()))?;
    let settings_item = MenuItem::with_id(app, "settings", "设置…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "退出 Snaply", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&capture, &settings_item, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id("snaply-tray")
        .tooltip("Snaply — 截图")
        .menu(&menu)
        // Left click captures; right click opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "capture" => {
                let _ = crate::overlay::begin(app);
            }
            "settings" => {
                let _ = crate::settings_window::open(app);
            }
            "quit" => crate::exit_application(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = crate::overlay::begin(tray.app_handle());
            }
        });

    // Use a monochrome 22pt tray icon with a 44px Retina variant.
    if let Ok(img) = ::image::load_from_memory_with_format(
        include_bytes!("../icons/tray@2x.png"),
        ::image::ImageFormat::Png,
    ) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        builder = builder.icon(tauri::image::Image::new_owned(rgba.into_raw(), w, h));
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }
    builder.build(app)?;
    Ok(())
}

/// Register the global capture shortcut.
pub fn register_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| e.to_string())?;
    log::info!("registered global shortcut {shortcut}");
    Ok(())
}

/// Handle all registered global shortcut events.
pub fn handle_hotkey(
    app: &AppHandle,
    shortcut: &tauri_plugin_global_shortcut::Shortcut,
    state: ShortcutState,
) {
    // Snaply owns the shortcut registration, so any press starts a capture.
    let _ = shortcut;
    if state == ShortcutState::Pressed {
        let _ = crate::overlay::begin(app);
    }
}
