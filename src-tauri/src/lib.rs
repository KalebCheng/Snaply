use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::state::AppState;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub shortcut: String,
    pub screen_permission: bool,
    pub platform: &'static str,
}

mod commands;
mod image;
#[cfg(target_os = "macos")]
mod macos_windows;
mod main_window;
mod overlay;
mod platform;
mod settings;
mod settings_window;
mod state;
mod tray;
mod updates;

/// Quit explicitly so the tray exit guard does not prevent application shutdown.
pub fn exit_application(app: &AppHandle) {
    app.state::<AppState>()
        .allow_exit
        .store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    #[cfg(windows)]
    enable_per_monitor_dpi();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    tray::handle_hotkey(app, shortcut, event.state())
                })
                .build(),
        )
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_capture,
            commands::cancel_capture,
            commands::overlay_snapshot,
            commands::overlay_ready,
            commands::overlay_frame_ready,
            commands::overlay_image,
            commands::cursor_location,
            commands::windows_under_cursor,
            commands::overlay_commit,
            commands::get_settings,
            commands::save_settings,
            commands::reset_shortcut,
            commands::pick_save_dir,
            commands::open_settings,
            commands::check_screen_permission,
            commands::open_screen_permission_settings,
            commands::open_external_url,
            updates::check_for_updates,
            image::image_info,
            image::image_rgba,
            app_info,
            save_image_target,
            save_image,
            copy_image,
            copy_text,
            reveal_in_folder,
            open_folder,
            quit_app,
            hide_main,
            close_main,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Start in the tray without opening a window.
            if let Err(e) = tray::build(&handle) {
                log::error!("tray init failed: {e}");
            }
            #[cfg(target_os = "macos")]
            overlay::watch_space_changes(&handle);
            let saved = settings::load(&handle);
            *app.state::<AppState>().settings.lock().unwrap() = saved.clone();
            if let Err(error) = settings_window::refresh_dock_visibility(&handle) {
                log::warn!("failed to apply Dock visibility: {error}");
            }
            settings_window::register_on_start(&handle);
            #[cfg(target_os = "macos")]
            if !platform::has_screen_permission() {
                let permission_handle = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    let _ = permission_handle.run_on_main_thread(|| {
                        if !platform::has_screen_permission() {
                            platform::request_screen_permission();
                        }
                    });
                });
            }
            let prewarm_handle = handle.clone();
            std::thread::spawn(move || {
                // Warm the WebViews after startup without blocking app launch.
                std::thread::sleep(std::time::Duration::from_secs(1));
                let main_handle = prewarm_handle.clone();
                let _ = prewarm_handle.run_on_main_thread(move || {
                    if let Err(error) = overlay::prewarm(&main_handle) {
                        log::warn!("overlay prewarm failed: {error}");
                    }
                });
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let WindowEvent::Destroyed = event else {
                return;
            };
            if window.label().starts_with(overlay::OVERLAY_PREFIX) {
                if let Some(monitor_id) = overlay::monitor_id_from_label(window.label()) {
                    window
                        .app_handle()
                        .state::<AppState>()
                        .ready_overlays
                        .lock()
                        .unwrap()
                        .remove(&monitor_id);
                }
            }
            if matches!(window.label(), "main" | "settings") {
                let _ = settings_window::refresh_dock_visibility(window.app_handle());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Snaply")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                let explicit = app
                    .state::<AppState>()
                    .allow_exit
                    .load(std::sync::atomic::Ordering::SeqCst);
                if !explicit && code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        shortcut: settings_window::current_shortcut(&app),
        screen_permission: platform::has_screen_permission(),
        platform: std::env::consts::OS,
    }
}

/// Maximum accepted PNG size for clipboard writes (guards against hostile input).
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// Read a raw binary body from an IPC request.
fn raw_body<'a>(request: &'a tauri::ipc::Request<'_>) -> Result<&'a [u8], String> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => Ok(bytes.as_slice()),
        tauri::ipc::InvokeBody::Json(_) => Err("该命令只接受二进制请求体".into()),
    }
}

/// Persist the annotated PNG next to the capture it belongs to.
///
/// The clipboard/save path used to accept an arbitrary `path` argument, which
/// was an arbitrary-file-write primitive for anything running in the WebView.
/// The target is now derived from the capture directory (or the current
/// capture session), so the renderer can only overwrite its own screenshots.
#[tauri::command]
fn save_image(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let bytes = raw_body(&request)?;
    if bytes.is_empty() {
        return Err("图片数据为空".into());
    }
    let target = resolve_save_target(&app)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Write through a temporary file so a crash cannot truncate an existing capture.
    let temp = target.with_extension("png.tmp");
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &target).map_err(|e| e.to_string())?;
    let saved = target.to_string_lossy().into_owned();
    *app.state::<AppState>()
        .last_edit_path
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(saved.clone());
    Ok(saved)
}

/// Register the file a following raw-body `save_image` should overwrite.
///
/// The raw request body cannot carry a second argument, and the editor may be
/// saving an older capture than the most recent one, so the target is validated
/// here and consumed by `save_image`.
#[tauri::command]
fn save_image_target(app: AppHandle, path: String) -> Result<(), String> {
    let dir = settings::capture_dir(&app, &settings::load(&app))?;
    let target = std::path::PathBuf::from(&path);
    let is_png = target
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if target.parent() != Some(dir.as_path()) || !is_png {
        return Err("只允许写入截图目录中的 PNG 文件".into());
    }
    *app.state::<AppState>()
        .pending_save_path
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(target);
    Ok(())
}

/// Resolve where `save_image` may write.
fn resolve_save_target(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let state = app.state::<AppState>();
    if let Some(pending) = state
        .pending_save_path
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        return Ok(pending);
    }
    let dir = settings::capture_dir(app, &settings::load(app))?;
    let current = state
        .last_edit_path
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(current) = current {
        let candidate = std::path::PathBuf::from(current);
        if candidate.parent() == Some(dir.as_path())
            && candidate.extension().is_some_and(|ext| ext == "png")
        {
            return Ok(candidate);
        }
    }
    Ok(crate::overlay::unique_capture_path(&dir))
}

/// Copy an annotated PNG to the clipboard without re-encoding it in the frontend.
#[tauri::command]
fn copy_image(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let bytes = raw_body(&request)?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("图片太大，无法复制到剪贴板".into());
    }
    let img = ::image::load_from_memory(bytes)
        .map_err(|e| format!("无法解析图片：{e}"))?
        .to_rgba8();
    let (width, height) = img.dimensions();
    let tauri_image = tauri::image::Image::new_owned(img.into_raw(), width, height);
    app.clipboard()
        .write_image(&tauri_image)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Reveal a capture in the file manager. Only paths inside the capture
/// directory (or the current session file) are accepted.
#[tauri::command]
fn reveal_in_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let target = std::path::PathBuf::from(&path);
    let dir = settings::capture_dir(&app, &settings::load(&app))?;
    let allowed = target.parent() == Some(dir.as_path())
        || app
            .state::<AppState>()
            .last_edit_path
            .lock()
            .map(|current| current.as_deref() == Some(path.as_str()))
            .unwrap_or(false);
    if !allowed {
        return Err("只允许在截图目录中定位文件".into());
    }
    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_folder(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = settings::capture_dir(&app, &settings::load(&app))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<String>)
        .map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    exit_application(&app);
}

#[tauri::command]
fn hide_main(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = settings_window::refresh_dock_visibility(&app);
    }
}

#[tauri::command]
fn close_main(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = settings_window::refresh_dock_visibility(&app);
    }
}

#[cfg(windows)]
fn enable_per_monitor_dpi() {
    use windows_sys::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
