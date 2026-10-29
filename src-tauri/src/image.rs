use tauri::ipc::Response;

use crate::state::AppState;

/// Raw RGBA pixels for the pending editor image.
#[tauri::command]
pub fn image_info(state: tauri::State<'_, AppState>) -> Option<(u32, u32)> {
    state
        .last_shot
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| (s.width, s.height))
}

#[tauri::command]
pub fn image_rgba(state: tauri::State<'_, AppState>) -> Response {
    let guard = state.last_shot.lock().unwrap();
    let bytes = guard.as_ref().map(|s| s.rgba.clone()).unwrap_or_default();
    Response::new(bytes)
}
