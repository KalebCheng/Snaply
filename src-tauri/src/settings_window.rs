use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(target_os = "macos")]
use crate::settings;
#[cfg(target_os = "macos")]
use crate::settings::DockVisibility as MacDockVisibility;
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
    let builder =
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
            .title("Snaply Settings")
            .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
            .min_inner_size(820.0, 660.0)
            .center()
            .focused(true)
            .transparent(true)
            .decorations(true)
            .shadow(true)
            .background_color(tauri::window::Color(0, 0, 0, 0));
    #[cfg(target_os = "macos")]
    let builder = builder
        .hidden_title(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .traffic_light_position(tauri::LogicalPosition::new(18.0, 20.0));
    let builder = if let Some(icon) = app.default_window_icon().cloned() {
        builder.icon(icon)?
    } else {
        builder
    };
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
        MacDockVisibility::Hide => false,
        MacDockVisibility::Show => true,
        MacDockVisibility::FollowWindow => has_visible_app_window(app),
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

/// Return the shortcut that is actually registered with the OS.
///
/// The persisted value can differ from the registered one (for example when the
/// saved accelerator was already taken at startup), and the settings UI must
/// show what really works instead of a value that was never bound.
pub fn current_shortcut(app: &AppHandle) -> String {
    app.state::<AppState>()
        .registered_shortcut
        .lock()
        .map(|shortcut| shortcut.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
}

fn set_registered_shortcut(app: &AppHandle, shortcut: &str) {
    let state = app.state::<AppState>();
    let mut current = state
        .registered_shortcut
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *current = shortcut.to_string();
}

/// Register the saved shortcut during startup.
///
/// When the saved accelerator is taken we fall back to the default one and
/// persist that choice, so the settings window and the tray menu can never show
/// a shortcut that is not registered.
pub fn register_on_start(app: &AppHandle) {
    let shortcut = crate::settings::load(app).shortcut;
    match tray::register_shortcut(app, &shortcut) {
        Ok(()) => set_registered_shortcut(app, &shortcut),
        Err(error) => {
            log::warn!("hotkey '{error}' failed, falling back to the default");
            match tray::register_shortcut(app, crate::settings::DEFAULT_SHORTCUT) {
                Ok(()) => {
                    set_registered_shortcut(app, crate::settings::DEFAULT_SHORTCUT);
                    let mut settings = crate::settings::load(app);
                    settings.shortcut = crate::settings::DEFAULT_SHORTCUT.into();
                    if let Err(save_error) = crate::settings::save(app, &settings) {
                        log::warn!("failed to persist fallback shortcut: {save_error}");
                    }
                }
                Err(fallback_error) => {
                    log::error!("failed to register any shortcut: {fallback_error}");
                }
            }
        }
    }
}

/// Rebind the global shortcut and roll back if registration fails.
///
/// Idempotent: asking for the shortcut that is already registered succeeds
/// instead of reporting "already in use", which used to make "restore default"
/// fail forever after a startup fallback.
pub fn rebind(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let manager = app.global_shortcut();
    let previous = current_shortcut(app);
    if shortcut == previous {
        return Ok(());
    }
    if manager.is_registered(shortcut) {
        return Err(format!("{shortcut} 已被占用"));
    }
    let _ = manager.unregister(previous.as_str());
    if let Err(error) = manager.register(shortcut) {
        // Roll back so the previous shortcut remains usable.
        let _ = manager.register(previous.as_str());
        return Err(format!("无法注册 {shortcut}：{error}"));
    }
    set_registered_shortcut(app, shortcut);
    Ok(())
}
