use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};

/// Create the compact main window with native macOS window controls.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Snaply")
        .inner_size(880.0, 600.0)
        .min_inner_size(640.0, 420.0)
        .center()
        .focused(true)
        .visible(true)
        .transparent(true)
        .decorations(false)
        .shadow(true)
        .hidden_title(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .background_color(tauri::window::Color(0, 0, 0, 0));
    if cfg!(target_os = "macos") {
        // Keep native shadows while allowing the frontend to paint theme-aware colors.
        builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay);
    }
    let window = builder.build()?;
    let _ = window.set_focus();
    let _ = crate::settings_window::refresh_dock_visibility(app);
    Ok(())
}
