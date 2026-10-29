use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::settings::{self, DockVisibility};
use crate::state::AppState;
use crate::tray;

const WINDOW_WIDTH: f64 = 980.0;
const WINDOW_HEIGHT: f64 = 760.0;

/// Open the settings window with native window controls.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        window.set_focus()?;
        let _ = refresh_dock_visibility(app);
        return Ok(());
    }
    let mut builder =
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
            .title("Snaply Settings")
            .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
            .min_inner_size(820.0, 660.0)
            .center()
            .focused(true)
            .transparent(true)
            .decorations(true)
            .shadow(true)
            .hidden_title(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .traffic_light_position(LogicalPosition::new(18.0, 20.0))
            .background_color(tauri::window::Color(0, 0, 0, 0));
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon)?;
    }
    let window = builder.build()?;
    #[cfg(target_os = "macos")]
    {
        // Resize once after creation to avoid a transparent first paint on macOS.
        let _ = window.set_size(tauri::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
        let _ = window.center();
    }
    let _ = window.set_focus();
    let _ = refresh_dock_visibility(app);
    Ok(())
}

/// Recompute Dock visibility from the current preference and window state.
#[cfg(target_os = "macos")]
pub fn refresh_dock_visibility(app: &AppHandle) -> Result<(), String> {
    let preference = settings::load(app).dock_visibility;
    let visible = match preference {
        DockVisibility::Hide => false,
        DockVisibility::Show => true,
        DockVisibility::FollowWindow => has_visible_app_window(app),
    };
    let state = app.state::<AppState>();
    if state.dock_visible.load(std::sync::atomic::Ordering::SeqCst) == visible {
        return Ok(());
    }

    app.set_dock_visibility(visible)
        .map_err(|error| error.to_string())?;
    state
        .dock_visible
        .store(visible, std::sync::atomic::Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn refresh_dock_visibility(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn has_visible_app_window(app: &AppHandle) -> bool {
    ["main", "settings"].iter().any(|label| {
        app.get_webview_window(label)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
    })
}

/// Return the currently registered shortcut for settings display.
pub fn current_shortcut(app: &AppHandle) -> String {
    crate::settings::load(app).shortcut
}

/// Register the saved shortcut during startup.
pub fn register_on_start(app: &AppHandle) {
    let shortcut = current_shortcut(app);
    if let Err(e) = tray::register_shortcut(app, &shortcut) {
        log::warn!("hotkey '{e}' failed, fallback to default");
        let _ = tray::register_shortcut(app, crate::settings::DEFAULT_SHORTCUT);
    }
}

/// Rebind the global shortcut and roll back if registration fails.
pub fn rebind(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let manager = app.global_shortcut();
    if manager.is_registered(shortcut) {
        return Err(format!("{shortcut} 已被占用"));
    }
    let previous = current_shortcut(app);
    let _ = manager.unregister(previous.as_str());
    if let Err(e) = manager.register(shortcut) {
        // Roll back so the previous shortcut remains usable.
        let _ = manager.register(previous.as_str());
        return Err(format!("无法注册 {shortcut}：{e}"));
    }
    Ok(())
}
