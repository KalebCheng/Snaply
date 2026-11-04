use tauri::AppHandle;
use tauri_plugin_global_shortcut::ShortcutState;

const TRAY_ID: &str = "snaply-tray";

fn tray_labels(
    language: crate::settings::Language,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match language {
        crate::settings::Language::SimplifiedChinese => {
            ("截图", "设置…", "退出 Snaply", "Snaply — 截图")
        }
        crate::settings::Language::English => {
            ("Capture", "Settings…", "Quit Snaply", "Snaply — Capture")
        }
        crate::settings::Language::Russian => {
            ("Снимок", "Настройки…", "Выйти из Snaply", "Snaply — Снимок")
        }
    }
}

/// Build the tray menu. Rebuilt whenever the capture shortcut changes so the
/// accelerator shown next to "capture" is never stale.
fn tray_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};

    let shortcut = crate::settings_window::current_shortcut(app);
    let (capture_label, settings_label, quit_label, _) =
        tray_labels(crate::settings::load(app).language);
    let capture = MenuItem::with_id(app, "capture", capture_label, true, Some(shortcut.as_str()))?;
    let settings_item =
        MenuItem::with_id(app, "settings", settings_label, true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&capture, &settings_item, &sep, &quit])
}

/// Refresh the accelerator shown in the tray menu after a shortcut rebind.
pub fn refresh_menu(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (_, _, _, tooltip) = tray_labels(crate::settings::load(app).language);
    let _ = tray.set_tooltip(Some(tooltip));
    match tray_menu(app) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                log::warn!("failed to refresh tray menu: {error}");
            }
        }
        Err(error) => log::warn!("failed to rebuild tray menu: {error}"),
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let menu = tray_menu(app)?;
    let (_, _, _, tooltip) = tray_labels(crate::settings::load(app).language);

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip)
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
