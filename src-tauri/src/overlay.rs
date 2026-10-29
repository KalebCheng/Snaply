#[cfg(target_os = "macos")]
use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
#[cfg(not(target_os = "macos"))]
use tauri::{LogicalPosition, LogicalSize};
use xcap::Monitor;

use crate::platform::{self, MonitorGeom, WindowHit};
use crate::state::AppState;

pub const OVERLAY_PREFIX: &str = "overlay-";
static OVERLAY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "macos")]
fn appkit_overlay_frame(monitor: &MonitorInfo, main_height: f64) -> CGRect {
    CGRect::new(
        CGPoint::new(
            monitor.x as f64,
            main_height - monitor.y as f64 - monitor.height as f64,
        ),
        CGSize::new(monitor.width.max(1) as f64, monitor.height.max(1) as f64),
    )
}

/// Keep the overlay above the Dock, menu bar, and status bar.
///
/// Without this level adjustment the frozen screenshot and the live system UI
/// render on top of each other, producing visible ghosting.
#[cfg(target_os = "macos")]
fn configure_overlay_window(window: &tauri::WebviewWindow, monitor: &MonitorInfo) {
    if let Ok(ptr) = window.ns_window() {
        let ns_window = unsafe { &*(ptr.cast::<NSWindow>()) };
        ns_window.setLevel(NSScreenSaverWindowLevel);
        // Non-key overlay windows still need mouse-move events for hover
        // detection and the magnifier when the cursor crosses displays.
        ns_window.setAcceptsMouseMovedEvents(true);

        // Tauri 的逻辑坐标定位会按窗口当前所在显示器的缩放比再次换算，
        // 覆盖层跨到不同 DPI 的副屏时会整体偏移。AppKit 接受全局屏幕坐标，
        // 因此这里直接通过 NSWindow 设置原生 frame。
        let main_height = CGDisplayBounds(CGMainDisplayID()).size.height;
        ns_window.setFrame_display(appkit_overlay_frame(monitor, main_height), true);
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_overlay_window(window: &tauri::WebviewWindow, monitor: &MonitorInfo) {
    let _ = window.set_position(LogicalPosition::new(monitor.x, monitor.y));
    let _ = window.set_size(LogicalSize::new(
        monitor.width.max(1),
        monitor.height.max(1),
    ));
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    /// Logical position of the overlay window.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Physical pixel dimensions of the captured monitor.
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub primary: bool,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySnapshot {
    pub monitor: MonitorInfo,
    pub started_at_ms: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorLocation {
    pub monitor_id: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    pub absolute_path: String,
    pub file_name: String,
    /// Cropped PNG in physical pixels, returned directly to the frontend.
    pub png: Vec<u8>,
    pub mode: String,
}

/// Timestamp used in file names, in UTC.
pub fn timestamp() -> String {
    let secs = (now_ms() / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Convert Unix days to a civil calendar date.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Show or hide all application windows.
pub fn set_app_visibility(app: &AppHandle, visible: bool) {
    for window in app.webview_windows().values() {
        let _ = if visible {
            window.show()
        } else {
            window.hide()
        };
    }
    if visible {
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.set_focus();
        }
    }
}

pub fn close_overlays(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(OVERLAY_PREFIX) {
            let _ = window.destroy();
        }
    }
}

fn monitor_at_point(geoms: &[MonitorGeom], x: f64, y: f64) -> Option<&MonitorGeom> {
    geoms
        .iter()
        .find(|g| x >= g.x && y >= g.y && x < g.x + g.width && y < g.y + g.height)
}

/// 返回光标所在显示器及其本地逻辑坐标。
///
/// 覆盖层直接从各自的 WebView 轮询这里，而不是依赖全局事件广播：
/// 未获得焦点的副屏窗口也一定能收到当前位置。
pub fn cursor_location(app: &AppHandle) -> Option<CursorLocation> {
    let (cx, cy) = platform::cursor_position()?;
    let geoms = app
        .state::<AppState>()
        .pending
        .lock()
        .unwrap()
        .geoms
        .clone();
    cursor_location_for(&geoms, cx, cy)
}

fn cursor_location_for(geoms: &[MonitorGeom], cx: f64, cy: f64) -> Option<CursorLocation> {
    let monitor = monitor_at_point(geoms, cx, cy)?;
    Some(CursorLocation {
        monitor_id: monitor.id,
        x: cx - monitor.x,
        y: cy - monitor.y,
    })
}

fn to_info(m: &Monitor, geoms: &[MonitorGeom], index: usize) -> MonitorInfo {
    let g = geoms.get(index);
    let width = m.width().unwrap_or(0);
    let height = m.height().unwrap_or(0);
    let scale = g.map(|g| g.scale).unwrap_or(1.0);
    MonitorInfo {
        id: m.id().unwrap_or(index as u32),
        name: m
            .friendly_name()
            .or_else(|_| m.name())
            .unwrap_or_else(|_| format!("Display {}", index + 1)),
        x: m.x().unwrap_or(0),
        y: m.y().unwrap_or(0),
        width,
        height,
        pixel_width: (width as f64 * scale).round() as u32,
        pixel_height: (height as f64 * scale).round() as u32,
        primary: m.is_primary().unwrap_or(index == 0),
    }
}

/// Start a capture session and open one overlay per monitor.
pub fn begin(app: &AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        if state.capture_lock.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
    }
    let result = begin_inner(app);
    app.state::<AppState>()
        .capture_lock
        .store(false, Ordering::SeqCst);
    result
}

/// Captured monitor metadata, PNG data, and geometry.
type CapturedFrames = (Vec<MonitorInfo>, HashMap<u32, Vec<u8>>, Vec<MonitorGeom>);

/// Capture every monitor on the current thread because xcap is not `Send`.
fn capture_frames() -> Result<CapturedFrames, String> {
    let monitors = Monitor::all().map_err(platform::describe_capture_error)?;
    let geoms = platform::monitor_geoms()?;
    let mut infos = Vec::with_capacity(monitors.len());
    let mut pngs = HashMap::new();
    for (i, m) in monitors.iter().enumerate() {
        let img = m.capture_image().map_err(|e| {
            format!(
                "无法抓取显示器「{}」：{}",
                m.friendly_name().unwrap_or_default(),
                platform::describe_capture_error(e)
            )
        })?;
        let info = to_info(m, &geoms, i);
        pngs.insert(info.id, platform::encode_png(&img)?);
        infos.push(info);
    }
    Ok((infos, pngs, geoms))
}

fn begin_inner(app: &AppHandle) -> Result<(), String> {
    // Hide Snaply windows before capturing.
    set_app_visibility(app, false);
    #[cfg(target_os = "macos")]
    crate::macos_windows::clear_content_bounds_cache();
    let started_at_ms = now_ms();

    // Capture frames and monitor geometry.
    let (infos, pngs, geoms) = match capture_frames() {
        Ok(frames) => frames,
        Err(e) => {
            platform::permission_hint(app, &e);
            set_app_visibility(app, true);
            return Err(e);
        }
    };

    // Focus the monitor containing the cursor.
    let cursor = platform::cursor_position();
    let focused_id = cursor
        .and_then(|(cx, cy)| monitor_at_point(&geoms, cx, cy).map(|g| g.id))
        .or_else(|| infos.iter().find(|m| m.primary).map(|m| m.id))
        .or_else(|| infos.first().map(|m| m.id));

    // Cache the snapshot before creating overlay windows.
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending.lock().unwrap();
        pending.png_by_id = pngs;
        pending.monitors = infos.clone();
        pending.geoms = geoms;
        pending.started_at_ms = started_at_ms;
    }

    // Create one overlay per monitor with a unique window label.
    let generation = OVERLAY_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    for m in infos.iter() {
        let label = format!("{OVERLAY_PREFIX}{generation}-{}", m.id);
        let url = WebviewUrl::App(format!("overlay.html?m={}", m.id).into());
        let should_focus = Some(m.id) == focused_id;
        let mut builder = WebviewWindowBuilder::new(app, &label, url)
            .title("Snaply")
            .inner_size(m.width.max(1) as f64, m.height.max(1) as f64)
            .position(m.x as f64, m.y as f64)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .visible(false)
            .focused(should_focus)
            .accept_first_mouse(true)
            .background_color(tauri::window::Color(0, 0, 0, 0));
        if cfg!(target_os = "macos") {
            builder = builder
                .title_bar_style(tauri::TitleBarStyle::Transparent)
                .hidden_title(true);
        }
        let window = builder.build().map_err(|e| e.to_string())?;
        configure_overlay_window(&window, m);
        let _ = window.show();
        // 显示窗口和挂载 WebView 都可能再次调整原生窗口尺寸，
        // 因此显示后需要重新应用最终 frame。
        configure_overlay_window(&window, m);
        if should_focus {
            // Focus explicitly so keyboard commands reach the overlay.
            let _ = window.set_focus();
        }
    }
    Ok(())
}

pub fn overlay_snapshot(app: &AppHandle, monitor_id: u32) -> Result<OverlaySnapshot, String> {
    let state = app.state::<AppState>();
    let pending = state.pending.lock().unwrap();
    let monitor = pending
        .monitors
        .iter()
        .find(|m| m.id == monitor_id)
        .cloned()
        .ok_or("截图会话已失效，请重新截图")?;
    Ok(OverlaySnapshot {
        monitor,
        started_at_ms: pending.started_at_ms,
    })
}

pub fn overlay_image(app: &AppHandle, monitor_id: u32) -> Result<Vec<u8>, String> {
    let state = app.state::<AppState>();
    let pending = state.pending.lock().unwrap();
    pending
        .png_by_id
        .get(&monitor_id)
        .cloned()
        .ok_or_else(|| "截图会话已失效，请重新截图".to_string())
}

/// Find the window under the cursor in monitor-local logical coordinates.
pub fn windows_under_cursor(app: &AppHandle, monitor_id: u32, x: f64, y: f64) -> Vec<WindowHit> {
    let state = app.state::<AppState>();
    let geoms = state.pending.lock().unwrap().geoms.clone();
    let Some(index) = geoms.iter().position(|g| g.id == monitor_id) else {
        return Vec::new();
    };
    let _ = index;
    // Window discovery is an enhancement and must never block capture.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        platform::windows_under_cursor(&geoms, monitor_id, x, y)
    }))
    .unwrap_or_else(|_| {
        log::warn!("window identification failed; continuing without window snapping");
        Vec::new()
    })
}

/// Crop the selected region from the physical frame, save it, and open the editor.
pub fn commit(
    app: &AppHandle,
    monitor_id: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    mode: &str,
) -> Result<CreateResult, String> {
    let state = app.state::<AppState>();
    let (png, scale) = {
        let pending = state.pending.lock().unwrap();
        let g = pending
            .geoms
            .iter()
            .find(|g| g.id == monitor_id)
            .ok_or("找不到对应显示器")?;
        let png = pending
            .png_by_id
            .get(&monitor_id)
            .cloned()
            .ok_or("截图缓存已失效，请重新截图")?;
        (png, g.scale)
    };

    let full = image::load_from_memory(&png).map_err(|e| e.to_string())?;
    let sx = (x.max(0.0) * scale).round() as u32;
    let sy = (y.max(0.0) * scale).round() as u32;
    let sw = ((w.max(1.0) * scale).round() as u32)
        .min(full.width().saturating_sub(sx))
        .max(1);
    let sh = ((h.max(1.0) * scale).round() as u32)
        .min(full.height().saturating_sub(sy))
        .max(1);
    let crop = image::imageops::crop_imm(&full, sx, sy, sw, sh).to_image();
    let png = platform::encode_png(&crop)?;

    let settings = crate::settings::load(app);
    let dir = crate::settings::capture_dir(app, &settings)?;
    let file_name = format!("Snaply-{}.png", timestamp());
    let path = dir.join(&file_name);
    crop.save(&path).map_err(|e| e.to_string())?;

    // Cache RGBA so the frontend can draw without another decode.
    {
        let (width, height) = crop.dimensions();
        *state.last_shot.lock().unwrap() = Some(crate::state::ShotImage {
            width,
            height,
            rgba: crop.into_raw(),
        });
    }

    close_overlays(app);
    state.pending.lock().unwrap().clear();
    // Copy mode stays in the background; edit mode brings the main window forward.
    if !(mode == "copy" || settings.copy_after_capture) {
        show_main_window(app)?;
    }
    Ok(CreateResult {
        absolute_path: path.to_string_lossy().into_owned(),
        file_name,
        png,
        mode: if mode == "copy" || settings.copy_after_capture {
            "copy".into()
        } else {
            "edit".into()
        },
    })
}

pub fn cancel(app: &AppHandle) -> Result<(), String> {
    close_overlays(app);
    app.state::<AppState>().pending.lock().unwrap().clear();
    Ok(())
}

/// Show and focus the editor window, creating it when necessary.
pub fn show_main_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        let _ = crate::settings_window::refresh_dock_visibility(app);
        return Ok(());
    }
    crate::main_window::create(app).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_negative_secondary_display_coordinates() {
        let geoms = vec![
            MonitorGeom {
                id: 1,
                x: 0.0,
                y: 0.0,
                width: 1800.0,
                height: 1169.0,
                scale: 2.0,
            },
            MonitorGeom {
                id: 2,
                x: 876.0,
                y: -1440.0,
                width: 2560.0,
                height: 1440.0,
                scale: 1.0,
            },
        ];

        assert_eq!(monitor_at_point(&geoms, 900.0, -1400.0).unwrap().id, 2);
        assert_eq!(monitor_at_point(&geoms, 100.0, 100.0).unwrap().id, 1);
        assert!(monitor_at_point(&geoms, 3500.0, -1400.0).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn converts_secondary_display_to_appkit_coordinates() {
        let monitor = MonitorInfo {
            id: 3,
            name: "副屏".into(),
            x: 876,
            y: -1440,
            width: 2560,
            height: 1440,
            pixel_width: 2560,
            pixel_height: 1440,
            primary: false,
        };

        let frame = appkit_overlay_frame(&monitor, 1169.0);
        assert_eq!(frame.origin.x, 876.0);
        assert_eq!(frame.origin.y, 1169.0);
        assert_eq!(frame.size.width, 2560.0);
        assert_eq!(frame.size.height, 1440.0);
    }

    #[test]
    fn reports_local_cursor_on_secondary_display() {
        let geoms = vec![
            MonitorGeom {
                id: 1,
                x: 0.0,
                y: 0.0,
                width: 1800.0,
                height: 1169.0,
                scale: 2.0,
            },
            MonitorGeom {
                id: 3,
                x: 876.0,
                y: -1440.0,
                width: 2560.0,
                height: 1440.0,
                scale: 1.0,
            },
        ];

        let pointer = cursor_location_for(&geoms, 1200.0, -400.0).unwrap();
        assert_eq!(pointer.monitor_id, 3);
        assert_eq!(pointer.x, 324.0);
        assert_eq!(pointer.y, 1040.0);
    }
}
