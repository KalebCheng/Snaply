use image::RgbaImage;
use xcap::Monitor;
#[cfg(not(target_os = "macos"))]
use xcap::Window;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorGeom {
    pub id: u32,
    /// Global logical origin of this monitor in the virtual desktop.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

fn geom(m: &Monitor, index: usize) -> MonitorGeom {
    let width = m.width().unwrap_or(1) as f64;
    let height = m.height().unwrap_or(1) as f64;
    let capture_scale = m
        .capture_image()
        .map(|img| img.width() as f64 / width)
        .unwrap_or(1.0)
        .max(0.1);
    MonitorGeom {
        id: m.id().unwrap_or(index as u32),
        x: m.x().unwrap_or(0) as f64,
        y: m.y().unwrap_or(0) as f64,
        width,
        height,
        scale: capture_scale,
    }
}

pub fn monitor_geoms() -> Result<Vec<MonitorGeom>, String> {
    let monitors = Monitor::all().map_err(describe_capture_error)?;
    Ok(monitors
        .iter()
        .enumerate()
        .map(|(i, m)| geom(m, i))
        .collect())
}

pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Translate low-level capture failures into actionable messages.
#[cfg(target_os = "macos")]
pub fn describe_capture_error<E: std::fmt::Display>(err: E) -> String {
    let text = err.to_string();
    if !has_screen_permission() {
        return "Snaply 需要「屏幕录制」权限才能截图。请在「系统设置 › 隐私与安全性 › 屏幕录制」中勾选 Snaply，然后重新启动应用。".into();
    }
    text
}

#[cfg(not(target_os = "macos"))]
pub fn describe_capture_error<E: std::fmt::Display>(err: E) -> String {
    let text = err.to_string();
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none() {
            return format!(
                "当前是纯 Wayland 会话，暂不支持直接截图。请改用 X11 / XWayland 会话。({text})"
            );
        }
    }
    text
}

#[cfg(target_os = "macos")]
pub fn has_screen_permission() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

/// Request screen recording access. The first call opens the system prompt;
/// the application must be restarted after permission is granted.
#[cfg(target_os = "macos")]
pub fn request_screen_permission() -> bool {
    if has_screen_permission() {
        return true;
    }
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

#[cfg(not(target_os = "macos"))]
pub fn has_screen_permission() -> bool {
    true
}

/// Current global cursor position in logical points.
#[cfg(target_os = "macos")]
pub fn cursor_position() -> Option<(f64, f64)> {
    // CGEventGetLocation 需要真实的 CGEvent；传 NULL 会得到 (0, 0)。
    // CGEventCreate(NULL) 返回携带当前鼠标位置的系统事件。
    let event = objc2_core_graphics::CGEvent::new(None)?;
    let p = objc2_core_graphics::CGEvent::location(Some(&event));
    Some((p.x, p.y))
}

/// Current global cursor position in platform-native display coordinates.
#[cfg(windows)]
pub fn cursor_position() -> Option<(f64, f64)> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut p) } == 0 {
        return None;
    }
    Some((p.x as f64, p.y as f64))
}

#[cfg(target_os = "linux")]
pub fn cursor_position() -> Option<(f64, f64)> {
    None
}

pub fn permission_hint(app: &tauri::AppHandle, message: &str) {
    #[cfg(target_os = "macos")]
    if !has_screen_permission() {
        open_permission_settings(app);
    }
    let _ = app;
    log::warn!("{message}");
}

/// Open the macOS Screen Recording privacy pane.
pub fn open_permission_settings(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        let _ = app.opener().open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            None::<String>,
        );
    }
    let _ = app;
}

// Window hit testing

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowHit {
    pub title: String,
    pub app_name: String,
    /// Logical coordinates relative to the monitor's top-left corner.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Zero marks non-standard surfaces such as menus and sheets.
    pub layer: i64,
}

/// Return windows under the cursor, ordered from top to bottom.
/// `cx/cy` are logical coordinates relative to the monitor origin.
pub fn windows_under_cursor(
    monitors: &[MonitorGeom],
    monitor_id: u32,
    cx: f64,
    cy: f64,
) -> Vec<WindowHit> {
    let Some(monitor) = monitors.iter().find(|m| m.id == monitor_id) else {
        return Vec::new();
    };

    #[cfg(target_os = "macos")]
    {
        // Query CoreGraphics directly for accurate z-order and occlusion.
        crate::macos_windows::windows_at_point(monitor, cx, cy)
    }

    #[cfg(not(target_os = "macos"))]
    windows_under_cursor_generic(monitor, cx, cy)
}

/// Approximate hit testing on other platforms using geometry and area.
#[cfg(not(target_os = "macos"))]
fn windows_under_cursor_generic(monitor: &MonitorGeom, cx: f64, cy: f64) -> Vec<WindowHit> {
    let Ok(windows) = Window::all() else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for w in windows.iter() {
        if w.is_minimized().unwrap_or(false) {
            continue;
        }
        let layer = w.z().unwrap_or(0);
        if layer == 0 {
            continue;
        }
        let (wx, wy) = (w.x().unwrap_or(0) as f64, w.y().unwrap_or(0) as f64);
        let (ww, wh) = (
            w.width().unwrap_or(0) as f64,
            w.height().unwrap_or(0) as f64,
        );
        if ww < 8.0 || wh < 8.0 {
            continue;
        }
        if let Ok(m) = w.current_monitor() {
            if m.id().ok() != Some(monitor.id) {
                continue;
            }
        }
        let x = (wx - monitor.x) / monitor.scale;
        let y = (wy - monitor.y) / monitor.scale;
        let width = ww / monitor.scale;
        let height = wh / monitor.scale;
        if cx < x || cy < y || cx >= x + width || cy >= y + height {
            continue;
        }
        hits.push(WindowHit {
            title: w.title().unwrap_or_default(),
            app_name: w.app_name().unwrap_or_default(),
            x,
            y,
            width,
            height,
            layer: layer as i64,
        });
    }
    hits.sort_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)));
    hits
}
