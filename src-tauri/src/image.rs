use tauri::ipc::Response;
use tauri::Manager;

use crate::state::AppState;

/// Raw RGBA pixels for the pending editor image.
#[tauri::command]
pub fn image_info(state: tauri::State<'_, AppState>) -> Option<(u32, u32)> {
    state
        .last_shot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|shot| (shot.width, shot.height))
}

/// Raw RGBA payload for the pending editor image.
///
/// A full-screen 5K frame is ~56 MB, so the copy happens on a worker thread and
/// the bytes travel over IPC as a raw body (`Response`), never as a JSON array.
#[tauri::command]
pub async fn image_rgba(app: tauri::AppHandle) -> Response {
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .last_shot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|shot| shot.rgba.clone())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Response::new(bytes)
}
