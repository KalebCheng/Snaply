#[cfg(target_os = "macos")]
use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use std::collections::HashMap;
use std::sync::{atomic::Ordering, Arc};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
#[cfg(not(target_os = "macos"))]
use tauri::{LogicalPosition, LogicalSize};
use xcap::Monitor;

use crate::platform::{self, MonitorGeom, WindowHit};
use crate::state::{AppState, CapturedFrame};

pub const OVERLAY_PREFIX: &str = "overlay-";

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
    for label in ["main", "settings"] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
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

fn overlay_label(monitor_id: u32) -> String {
    format!("{OVERLAY_PREFIX}{monitor_id}")
}

pub(crate) fn monitor_id_from_label(label: &str) -> Option<u32> {
    label.strip_prefix(OVERLAY_PREFIX)?.parse().ok()
}

pub fn hide_overlays(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(OVERLAY_PREFIX) {
            let _ = window.emit("overlay-reset", ());
            let _ = window.hide();
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
            .name()
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

fn create_overlay_window(app: &AppHandle, monitor: &MonitorInfo) -> Result<WebviewWindow, String> {
    let label = overlay_label(monitor.id);
    let url = WebviewUrl::App(format!("overlay.html?m={}&prewarm=1", monitor.id).into());
    let builder = WebviewWindowBuilder::new(app, &label, url)
        .title("Snaply")
        .inner_size(monitor.width.max(1) as f64, monitor.height.max(1) as f64)
        .position(monitor.x as f64, monitor.y as f64)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .background_color(tauri::window::Color(0, 0, 0, 0));
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Transparent)
        .hidden_title(true);
    let window = builder.build().map_err(|error| error.to_string())?;
    configure_overlay_window(&window, monitor);
    Ok(window)
}

fn ensure_overlay_window(
    app: &AppHandle,
    monitor: &MonitorInfo,
) -> Result<(WebviewWindow, bool), String> {
    if let Some(window) = app.get_webview_window(&overlay_label(monitor.id)) {
        configure_overlay_window(&window, monitor);
        return Ok((window, false));
    }
    create_overlay_window(app, monitor).map(|window| (window, true))
}

fn sync_overlay_windows(app: &AppHandle, monitors: &[MonitorInfo]) -> Result<(), String> {
    let active_labels: std::collections::HashSet<String> =
        monitors.iter().map(|m| overlay_label(m.id)).collect();
    for (label, window) in app.webview_windows() {
        if !label.starts_with(OVERLAY_PREFIX) || active_labels.contains(&label) {
            continue;
        }
        if let Some(monitor_id) = monitor_id_from_label(&label) {
            app.state::<AppState>()
                .ready_overlays
                .lock()
                .unwrap()
                .remove(&monitor_id);
        }
        let _ = window.destroy();
    }

    for monitor in monitors {
        let (window, _) = ensure_overlay_window(app, monitor)?;
        let _ = window.hide();
    }
    Ok(())
}

/// Create and load hidden overlay WebViews before the first capture.
///
/// Overlay WebView startup costs roughly 1-2 seconds on macOS. Keeping the
/// windows warm removes that cost from the screenshot hotkey path.
pub fn prewarm(app: &AppHandle) -> Result<(), String> {
    let monitors = Monitor::all().map_err(platform::describe_capture_error)?;
    if monitors.is_empty() {
        return Ok(());
    }
    let geoms = platform::monitor_geoms_for(&monitors);
    let infos: Vec<MonitorInfo> = monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| to_info(monitor, &geoms, index))
        .collect();
    sync_overlay_windows(app, &infos)
}

/// Mark the React overlay as ready. If capture data is already cached, request
/// an immediate refresh instead of relying on an event that could have raced
/// with the WebView bootstrap.
pub fn overlay_ready(app: &AppHandle, monitor_id: u32) {
    let state = app.state::<AppState>();
    let newly_ready = state.ready_overlays.lock().unwrap().insert(monitor_id);
    if !newly_ready {
        return;
    }
    let session = {
        let pending = state.pending.lock().unwrap();
        pending
            .frames_by_id
            .contains_key(&monitor_id)
            .then_some(pending.started_at_ms)
    };
    if let Some(session) = session {
        if let Some(window) = app.get_webview_window(&overlay_label(monitor_id)) {
            let _ = window.emit("overlay-refresh", session);
        }
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

/// Captured monitor metadata, raw pixels, and geometry.
type CapturedFrames = (
    Vec<MonitorInfo>,
    HashMap<u32, CapturedFrame>,
    Vec<MonitorGeom>,
);

/// Capture every monitor on the current thread because xcap is not `Send`.
fn capture_frames() -> Result<CapturedFrames, String> {
    let monitors = Monitor::all().map_err(platform::describe_capture_error)?;
    let geoms = platform::monitor_geoms_for(&monitors);
    let focused_index = platform::cursor_position()
        .and_then(|(x, y)| {
            geoms
                .iter()
                .position(|g| x >= g.x && y >= g.y && x < g.x + g.width && y < g.y + g.height)
        })
        .or_else(|| {
            monitors
                .iter()
                .position(|m| m.is_primary().unwrap_or(false))
        })
        .unwrap_or(0);
    let mut capture_order: Vec<usize> = (0..monitors.len()).collect();
    capture_order.sort_by_key(|index| *index != focused_index);
    let mut infos = Vec::with_capacity(monitors.len());
    let mut frames = HashMap::new();
    for i in capture_order {
        let m = &monitors[i];
        let img = m.capture_image().map_err(|e| {
            format!(
                "无法抓取显示器「{}」：{}",
                m.name().unwrap_or_default(),
                platform::describe_capture_error(e)
            )
        })?;
        let info = to_info(m, &geoms, i);
        let (width, height) = img.dimensions();
        frames.insert(
            info.id,
            CapturedFrame {
                width,
                height,
                rgba: Arc::new(img.into_raw()),
            },
        );
        infos.push(info);
    }
    Ok((infos, frames, geoms))
}

fn begin_inner(app: &AppHandle) -> Result<(), String> {
    hide_overlays(app);
    // Hide Snaply windows before capturing.
    set_app_visibility(app, false);
    #[cfg(target_os = "macos")]
    crate::macos_windows::clear_content_bounds_cache();
    let started_at_ms = now_ms();

    // Capture frames and monitor geometry.
    let (infos, frames, geoms) = match capture_frames() {
        Ok(frames) => frames,
        Err(e) => {
            platform::permission_hint(app, &e);
            set_app_visibility(app, true);
            return Err(e);
        }
    };

    // capture_frames puts the monitor under the cursor first.
    let focused_id = infos.first().map(|monitor| monitor.id);

    // Cache the snapshot before creating overlay windows.
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending.lock().unwrap();
        pending.frames_by_id = frames;
        pending.monitors = infos.clone();
        pending.geoms = geoms;
        pending.started_at_ms = started_at_ms;
    }

    sync_overlay_windows(app, &infos)?;

    // Reuse the prewarmed overlay WebViews and request the new raw frame.
    for m in infos.iter() {
        let (window, _) = ensure_overlay_window(app, m)?;
        let should_focus = Some(m.id) == focused_id;
        let _ = window.show();
        // 显示窗口和挂载 WebView 都可能再次调整原生窗口尺寸，
        // 因此显示后需要重新应用最终 frame。
        configure_overlay_window(&window, m);
        let ready = app
            .state::<AppState>()
            .ready_overlays
            .lock()
            .unwrap()
            .contains(&m.id);
        if ready {
            let _ = window.emit("overlay-refresh", started_at_ms);
        }
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

/// Return raw RGBA pixels prefixed with little-endian width and height.
///
/// Sending raw pixels avoids spending hundreds of milliseconds encoding a
/// full-screen PNG only for the overlay to immediately decode it again.
pub fn overlay_image(app: &AppHandle, monitor_id: u32) -> Result<Vec<u8>, String> {
    let frame = {
        let state = app.state::<AppState>();
        let pending = state.pending.lock().unwrap();
        pending
            .frames_by_id
            .get(&monitor_id)
            .cloned()
            .ok_or_else(|| "截图会话已失效，请重新截图".to_string())?
    };

    Ok(overlay_payload(&frame))
}

fn overlay_payload(frame: &CapturedFrame) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8 + frame.rgba.len());
    payload.extend_from_slice(&frame.width.to_le_bytes());
    payload.extend_from_slice(&frame.height.to_le_bytes());
    payload.extend_from_slice(frame.rgba.as_slice());
    payload
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
    let (frame, scale) = {
        let pending = state.pending.lock().unwrap();
        let g = pending
            .geoms
            .iter()
            .find(|g| g.id == monitor_id)
            .ok_or("找不到对应显示器")?;
        let frame = pending
            .frames_by_id
            .get(&monitor_id)
            .cloned()
            .ok_or("截图缓存已失效，请重新截图")?;
        (frame, g.scale)
    };

    let sx = ((x.max(0.0) * scale).round() as u32).min(frame.width.saturating_sub(1));
    let sy = ((y.max(0.0) * scale).round() as u32).min(frame.height.saturating_sub(1));
    let sw = ((w.max(1.0) * scale).round() as u32)
        .min(frame.width.saturating_sub(sx))
        .max(1);
    let sh = ((h.max(1.0) * scale).round() as u32)
        .min(frame.height.saturating_sub(sy))
        .max(1);
    let crop = image::RgbaImage::from_raw(
        sw,
        sh,
        crop_rgba(&frame.rgba, frame.width, frame.height, sx, sy, sw, sh)?,
    )
    .ok_or("无法裁切截图")?;
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

    hide_overlays(app);
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

fn crop_rgba(
    rgba: &[u8],
    image_width: u32,
    image_height: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    if x >= image_width
        || y >= image_height
        || width == 0
        || height == 0
        || x.checked_add(width)
            .map_or(true, |right| right > image_width)
        || y.checked_add(height)
            .map_or(true, |bottom| bottom > image_height)
    {
        return Err("截图裁切区域超出图像边界".into());
    }

    let row_bytes = width as usize * 4;
    let source_stride = image_width as usize * 4;
    let mut cropped = Vec::with_capacity(row_bytes * height as usize);
    for row in y..y + height {
        let start = row as usize * source_stride + x as usize * 4;
        cropped.extend_from_slice(&rgba[start..start + row_bytes]);
    }
    Ok(cropped)
}

pub fn cancel(app: &AppHandle) -> Result<(), String> {
    hide_overlays(app);
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

    #[test]
    fn crops_raw_rgba_without_loading_a_png() {
        let rgba: Vec<u8> = (0..4 * 3 * 4).map(|value| value as u8).collect();
        let cropped = crop_rgba(&rgba, 4, 3, 1, 1, 2, 2).unwrap();
        let expected: Vec<u8> = (1..3)
            .flat_map(|row| {
                (1..3).flat_map(move |column| {
                    (0..4).map(move |channel| ((row * 4 + column) * 4 + channel) as u8)
                })
            })
            .collect();
        assert_eq!(cropped, expected);
    }

    #[test]
    fn rejects_out_of_bounds_raw_crop() {
        let rgba = vec![0; 4 * 3 * 4];
        assert!(crop_rgba(&rgba, 4, 3, 3, 2, 2, 2).is_err());
    }

    #[test]
    fn overlay_payload_contains_dimensions_and_raw_pixels() {
        let frame = CapturedFrame {
            width: 2,
            height: 1,
            rgba: Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8]),
        };
        let payload = overlay_payload(&frame);

        assert_eq!(&payload[..4], &2u32.to_le_bytes());
        assert_eq!(&payload[4..8], &1u32.to_le_bytes());
        assert_eq!(&payload[8..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
