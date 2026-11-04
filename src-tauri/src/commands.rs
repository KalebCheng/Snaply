use tauri::{AppHandle, Manager};

use crate::overlay::{CreateResult, CursorLocation, OverlaySnapshot};
use crate::platform::WindowHit;
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
pub fn overlay_ready(app: AppHandle, monitor_id: u32) {
    crate::overlay::overlay_ready(&app, monitor_id);
}

/// Show the overlay once its WebView has painted the current frame.
#[tauri::command]
pub fn overlay_frame_ready(app: AppHandle, monitor_id: u32) {
    crate::overlay::overlay_frame_ready(&app, monitor_id);
}

#[tauri::command]
pub fn overlay_image(app: AppHandle, monitor_id: u32) -> Result<tauri::ipc::Response, String> {
    crate::overlay::overlay_image(&app, monitor_id).map(tauri::ipc::Response::new)
}

/// Window hit testing enumerates CoreGraphics/CoreGraphics-equivalent state and
/// must not run on the UI thread: it is polled while the user hovers the overlay.
#[tauri::command]
pub async fn windows_under_cursor(
    app: AppHandle,
    monitor_id: u32,
    x: f64,
    y: f64,
) -> Vec<WindowHit> {
    crate::overlay::windows_under_cursor(&app, monitor_id, x, y)
}

/// Polled ~60 times per second by every overlay; keep it off the UI thread.
#[tauri::command]
pub async fn cursor_location(app: AppHandle) -> Option<CursorLocation> {
    crate::overlay::cursor_location(&app)
}

/// Crop, encode, and persist the selection without blocking the UI thread.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn overlay_commit(
    app: AppHandle,
    monitor_id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    mode: Option<String>,
) -> Result<CreateResult, String> {
    let mode = mode.unwrap_or_else(|| "edit".into());
    let plan = crate::overlay::commit_prepare(&app, monitor_id)?;

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        crate::overlay::commit_execute(&plan, x, y, width, height, &mode)
    })
    .await
    .map_err(|error| error.to_string())??;

    // Closing the overlay and opening the editor touch AppKit: hop to the main thread.
    let result = outcome.result.clone();
    let (tx, mut rx) = tauri::async_runtime::channel::<Result<(), String>>(1);
    let finish_handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.try_send(crate::overlay::commit_finish(&finish_handle, outcome));
    })
    .map_err(|error| error.to_string())?;

    match rx.recv().await {
        Some(Ok(())) => Ok(result),
        Some(Err(error)) => Err(error),
        None => Err("截图提交没有返回结果".into()),
    }
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
    *app.state::<AppState>()
        .settings
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
    if dock_visibility_changed {
        settings_window::refresh_dock_visibility(&app)?;
    }
    // The tray menu shows the accelerator of the capture shortcut; rebuild it.
    crate::tray::refresh_menu(&app);
    Ok(get_settings(app))
}

#[tauri::command]
pub fn reset_shortcut(app: AppHandle) -> Result<SettingsPayload, String> {
    let mut current = settings::load(&app);
    current.shortcut = settings::DEFAULT_SHORTCUT.into();
    save_settings(app, current)
}

#[tauri::command]
pub async fn pick_save_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    // 文件选择器必须在主线程非阻塞地创建；同步版本会阻塞 Tauri 事件循环。
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title("选择截图保存目录")
        .pick_folder(move |picked| {
            let _ = tx.try_send(picked.map(|path| path.to_string()));
        });
    rx.recv()
        .await
        .ok_or_else(|| "保存目录选择器意外关闭".to_string())
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

/// Open a link in the system browser, restricted to this project's pages.
#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    if !is_allowed_external_url(&url) {
        return Err("Blocked external URL".into());
    }

    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| error.to_string())
}

/// Only https URLs on github.com for the Snaply repository (or the maintainer
/// profile) may be opened. Parsing the URL prevents prefix tricks such as
/// `https://github.com/KalebCheng/Snaply-evil`.
fn is_allowed_external_url(raw: &str) -> bool {
    let Ok(parsed) = tauri::Url::parse(raw) else {
        return false;
    };
    if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
        return false;
    }
    let path = parsed.path();
    path == "/KalebCheng" || path == "/KalebCheng/Snaply" || path.starts_with("/KalebCheng/Snaply/")
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
        assert_eq!(value["settings"]["dockVisibility"], "hide");
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
        let decoded: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded["settings"]["language"], "en");
        assert_eq!(decoded["settings"]["dockVisibility"], "hide");
        assert_eq!(decoded["effectiveShortcut"], settings::DEFAULT_SHORTCUT);
    }

    #[test]
    fn external_urls_are_restricted_to_the_project() {
        assert!(is_allowed_external_url("https://github.com/KalebCheng"));
        assert!(is_allowed_external_url(
            "https://github.com/KalebCheng/Snaply"
        ));
        assert!(is_allowed_external_url(
            "https://github.com/KalebCheng/Snaply/releases/latest"
        ));
        // Prefix tricks and foreign hosts must not pass.
        assert!(!is_allowed_external_url(
            "https://github.com/KalebCheng/Snaply-evil"
        ));
        assert!(!is_allowed_external_url(
            "https://evil.com/KalebCheng/Snaply"
        ));
        assert!(!is_allowed_external_url(
            "http://github.com/KalebCheng/Snaply"
        ));
        assert!(!is_allowed_external_url("file:///etc/passwd"));
        assert!(!is_allowed_external_url("javascript:alert(1)"));
        assert!(!is_allowed_external_url(""));
    }
}
