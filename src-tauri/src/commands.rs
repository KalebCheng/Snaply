use tauri::{AppHandle, Manager};

use crate::overlay::{CreateResult, CursorLocation, OverlaySnapshot};
use crate::platform::{MonitorGeom, WindowHit};
use crate::settings::{self, Settings};
use crate::settings_window;
use crate::state::AppState;

#[tauri::command]
pub fn start_capture(app: AppHandle) -> Result<(), String> {
    crate::overlay::begin(&app)
}

#[tauri::command]
pub fn cancel_capture(app: AppHandle) -> Result<(), String> {
    crate::overlay::cancel(&app)
}

#[tauri::command]
pub fn overlay_snapshot(app: AppHandle, monitor_id: u32) -> Result<OverlaySnapshot, String> {
    crate::overlay::overlay_snapshot(&app, monitor_id)
}

#[tauri::command]
pub fn overlay_image(app: AppHandle, monitor_id: u32) -> Vec<u8> {
    crate::overlay::overlay_image(&app, monitor_id).unwrap_or_default()
}

#[tauri::command]
pub fn windows_under_cursor(app: AppHandle, monitor_id: u32, x: f64, y: f64) -> Vec<WindowHit> {
    crate::overlay::windows_under_cursor(&app, monitor_id, x, y)
}

#[tauri::command]
pub fn cursor_location(app: AppHandle) -> Option<CursorLocation> {
    crate::overlay::cursor_location(&app)
}

#[tauri::command]
pub fn overlay_commit(
    app: AppHandle,
    monitor_id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    mode: Option<String>,
) -> Result<CreateResult, String> {
    let mode = mode.unwrap_or_else(|| "edit".into());
    let result = crate::overlay::commit(&app, monitor_id, x, y, width, height, &mode)?;
    *app.state::<AppState>().last_edit_path.lock().unwrap() = Some(result.absolute_path.clone());
    Ok(result)
}

#[tauri::command]
pub fn _monitor_geoms(app: AppHandle) -> Vec<MonitorGeom> {
    app.state::<AppState>()
        .pending
        .lock()
        .unwrap()
        .geoms
        .clone()
}

// Settings

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    settings: Settings,
    default_save_dir: String,
    effective_shortcut: String,
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> SettingsPayload {
    let settings = settings::load(&app);
    let default_save_dir = settings::capture_dir(&app, &Settings::default())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    SettingsPayload {
        effective_shortcut: settings_window::current_shortcut(&app),
        default_save_dir,
        settings,
    }
}

#[tauri::command]
pub fn save_settings(app: AppHandle, next: Settings) -> Result<SettingsPayload, String> {
    // Apply shortcut changes first; if rebinding fails, keep the previous settings.
    if next.shortcut != settings_window::current_shortcut(&app) {
        settings_window::rebind(&app, &next.shortcut)?;
    }
    let previous = settings::load(&app);
    let dock_visibility_changed = previous.dock_visibility != next.dock_visibility;
    settings::save(&app, &next)?;
    *app.state::<AppState>().settings.lock().unwrap() = next;
    if dock_visibility_changed {
        settings_window::refresh_dock_visibility(&app)?;
    }
    Ok(get_settings(app))
}

#[tauri::command]
pub fn reset_shortcut(app: AppHandle) -> Result<SettingsPayload, String> {
    let mut current = settings::load(&app);
    current.shortcut = settings::DEFAULT_SHORTCUT.into();
    save_settings(app, current)
}

#[tauri::command]
pub fn pick_save_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("选择截图保存目录")
        .blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    settings_window::open(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_screen_permission() -> bool {
    crate::platform::has_screen_permission()
}

#[tauri::command]
pub fn open_screen_permission_settings(app: AppHandle) {
    // Trigger the system prompt first, then open the privacy settings page.
    #[cfg(target_os = "macos")]
    crate::platform::request_screen_permission();
    crate::platform::open_permission_settings(&app);
}

#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let allowed = url == "https://github.com/KalebCheng"
        || url.starts_with("https://github.com/KalebCheng/Snaply");
    if !allowed {
        return Err("Blocked external URL".into());
    }

    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{DockVisibility, Language};

    #[test]
    fn settings_payload_keeps_settings_nested() {
        let payload = SettingsPayload {
            settings: Settings::default(),
            default_save_dir: "/tmp/Snaply".into(),
            effective_shortcut: settings::DEFAULT_SHORTCUT.into(),
        };
        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["settings"]["language"], "zh-CN");
        assert_eq!(value["settings"]["dockVisibility"], "followWindow");
        assert_eq!(value["defaultSaveDir"], "/tmp/Snaply");
        assert_eq!(value["effectiveShortcut"], settings::DEFAULT_SHORTCUT);
        assert!(value.get("shortcut").is_none());
    }

    #[test]
    fn settings_payload_round_trips_supported_values() {
        let payload = SettingsPayload {
            settings: Settings {
                language: Language::English,
                dock_visibility: DockVisibility::Hide,
                ..Settings::default()
            },
            default_save_dir: String::new(),
            effective_shortcut: settings::DEFAULT_SHORTCUT.into(),
        };
        let json = serde_json::to_string(&payload).unwrap();

        assert!(json.contains(r#""settings":{"#));
        assert!(json.contains(r#""language":"en""#));
        assert!(json.contains(r#""dockVisibility":"hide""#));
    }
}
