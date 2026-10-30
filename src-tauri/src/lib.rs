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
        shortcut: settings::load(&app).shortcut,
        screen_permission: platform::has_screen_permission(),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
fn save_image(app: AppHandle, path: String, bytes: Vec<u8>) -> Result<(), String> {
    let target = std::path::PathBuf::from(&path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
    *app.state::<AppState>().last_edit_path.lock().unwrap() = Some(path);
    Ok(())
}

#[tauri::command]
fn copy_image(app: AppHandle, bytes: Vec<u8>) -> Result<(), String> {
    let img = ::image::load_from_memory(&bytes)
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

#[tauri::command]
fn reveal_in_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(std::path::Path::new(&path))
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
