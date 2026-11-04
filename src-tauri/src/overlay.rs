#[cfg(target_os = "macos")]
use objc2::{sel, MainThreadMarker};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSRunningApplication, NSScreenSaverWindowLevel,
    NSWindow, NSWindowCollectionBehavior, NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use objc2_core_graphics::{
    CGDisplayBounds, CGEventSource, CGEventSourceStateID, CGMainDisplayID, CGMouseButton,
};
#[cfg(target_os = "macos")]
use objc2_foundation::NSNotification;
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
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
        // Capture overlays must survive a Space switch. Otherwise the frozen
        // image can remain visible on the old Space while the newly selected
        // Space has no interactive overlay window.
        let behavior = ns_window.collectionBehavior();
        ns_window.setCollectionBehavior(behavior | NSWindowCollectionBehavior::CanJoinAllSpaces);
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

#[cfg(target_os = "macos")]
fn activate_overlay_window(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ns_window = unsafe { &*(ptr.cast::<NSWindow>()) };
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    let dock_visible = window
        .app_handle()
        .state::<AppState>()
        .dock_visible
        .load(Ordering::SeqCst);
    if !dock_visible {
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }

    // A Dock-less app does not become active automatically when a global
    // shortcut orders its window front. Keep it an Accessory app and use the
    // modern activation API so no Dock icon is introduced.
    let supports_activate: bool =
        unsafe { objc2::msg_send![&*app, respondsToSelector: sel!(activate)] };
    if supports_activate {
        app.activate();
    }
    ns_window.makeKeyAndOrderFront(None);
    ns_window.orderFrontRegardless();

    // Activation is asynchronous. The first `makeKeyAndOrderFront:` above can
    // race the application becoming active and leave a visible but non-key
    // overlay; repeat it on the next event-loop turn.
    let delayed = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(180));
        let handle = delayed.app_handle().clone();
        let _ = handle.run_on_main_thread(move || {
            let Ok(ptr) = delayed.ns_window() else {
                return;
            };
            let delayed_window = unsafe { &*(ptr.cast::<NSWindow>()) };
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let delayed_app = NSApplication::sharedApplication(mtm);
            if !delayed
                .app_handle()
                .state::<AppState>()
                .dock_visible
                .load(Ordering::SeqCst)
            {
                delayed_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }
            delayed_app.activate();
            delayed_window.makeKeyAndOrderFront(None);
            let _ = delayed.set_focus();
        });
    });
}

#[cfg(target_os = "macos")]
fn request_overlay_focus(app: &AppHandle, monitor_id: u32) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(&overlay_label(monitor_id)) {
            activate_overlay_window(&window);
        }
    });
}

#[cfg(target_os = "macos")]
fn emit_native_pointer(
    window: &WebviewWindow,
    geoms: &[MonitorGeom],
    phase: &'static str,
    x: f64,
    y: f64,
) {
    let Some(monitor) = monitor_at_point(geoms, x, y) else {
        return;
    };
    let _ = window.emit(
        "overlay-native-pointer",
        NativePointerEvent {
            monitor_id: monitor.id,
            phase,
            x: x - monitor.x,
            y: y - monitor.y,
        },
    );
}

/// Forward global left-button phases to the overlay.
///
/// The frozen desktop and cursor hit-testing can still render when macOS
/// refuses activation for a Dock-less app. In that state AppKit does not route
/// mouse events into the WKWebView, so polling the global button state is the
/// reliable input path.
#[cfg(target_os = "macos")]
fn start_pointer_bridge(app: &AppHandle, generation: u64, geoms: Vec<MonitorGeom>) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut was_down = CGEventSource::button_state(
            CGEventSourceStateID::CombinedSessionState,
            CGMouseButton::Left,
        );
        let mut press_monitor = None;
        while handle
            .state::<AppState>()
            .overlay_generation
            .load(Ordering::SeqCst)
            == generation
        {
            let Some((x, y)) = platform::cursor_position() else {
                std::thread::sleep(std::time::Duration::from_millis(12));
                continue;
            };
            let is_down = CGEventSource::button_state(
                CGEventSourceStateID::CombinedSessionState,
                CGMouseButton::Left,
            );
            if is_down && !was_down {
                press_monitor = monitor_at_point(&geoms, x, y).map(|monitor| monitor.id);
                if let Some(monitor_id) = press_monitor {
                    if let Some(window) = handle.get_webview_window(&overlay_label(monitor_id)) {
                        emit_native_pointer(&window, &geoms, "down", x, y);
                    }
                    if !NSRunningApplication::currentApplication().isActive() {
                        request_overlay_focus(&handle, monitor_id);
                    }
                }
            } else if is_down && was_down {
                if let Some(monitor_id) = press_monitor {
                    if let Some(window) = handle.get_webview_window(&overlay_label(monitor_id)) {
                        emit_native_pointer(&window, &geoms, "move", x, y);
                    }
                }
            } else if !is_down && was_down {
                if let Some(monitor_id) = press_monitor.take() {
                    if let Some(window) = handle.get_webview_window(&overlay_label(monitor_id)) {
                        emit_native_pointer(&window, &geoms, "up", x, y);
                    }
                }
            }
            was_down = is_down;
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn start_pointer_bridge(_app: &AppHandle, _generation: u64, _geoms: Vec<MonitorGeom>) {}

#[cfg(not(target_os = "macos"))]
fn activate_overlay_window(_window: &tauri::WebviewWindow) {}

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

#[cfg(target_os = "macos")]
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePointerEvent {
    pub monitor_id: u32,
    pub phase: &'static str,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    /// Capture session id (milliseconds since epoch when the session started).
    pub shot_id: u64,
    pub absolute_path: String,
    pub file_name: String,
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
    let state = app.state::<AppState>();
    state.overlay_generation.fetch_add(1, Ordering::SeqCst);
    {
        let mut shown = state
            .shown_overlays
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shown.clear();
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with(OVERLAY_PREFIX) {
            let _ = window.emit("overlay-reset", ());
            let _ = window.hide();
        }
    }
}

/// Show one overlay window and focus it when it belongs to the cursor monitor.
///
/// Must run on the main thread: it touches `NSWindow` on macOS.
fn show_overlay(app: &AppHandle, monitor_id: u32) {
    let Some(window) = app.get_webview_window(&overlay_label(monitor_id)) else {
        return;
    };
    let monitor = app
        .state::<AppState>()
        .pending
        .lock()
        .map(|pending| {
            pending
                .monitors
                .iter()
                .find(|m| m.id == monitor_id)
                .cloned()
        })
        .ok()
        .flatten();
    if let Some(monitor) = monitor.as_ref() {
        // Showing the window can resize the native frame again, so re-apply it.
        configure_overlay_window(&window, monitor);
    }
    let _ = window.show();
    if let Some(monitor) = monitor.as_ref() {
        configure_overlay_window(&window, monitor);
    }
    let state = app.state::<AppState>();
    let focused = state
        .focused_monitor
        .lock()
        .map(|focused| *focused)
        .unwrap_or(None);
    if focused == Some(monitor_id) {
        // Focus explicitly so pointer and keyboard commands reach the overlay.
        activate_overlay_window(&window);
        let _ = window.set_focus();
    }
    {
        let mut shown = state
            .shown_overlays
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shown.insert(monitor_id);
    }
}

/// Called by the overlay WebView once the current frame has been painted.
///
/// Overlay windows stay hidden until this arrives, so a Space switch (or a new
/// capture) can never flash the previous frame on screen.
pub fn overlay_frame_ready(app: &AppHandle, monitor_id: u32) {
    let state = app.state::<AppState>();
    let in_session = state
        .pending
        .lock()
        .map(|pending| pending.frames_by_id.contains_key(&monitor_id))
        .unwrap_or(false);
    if !in_session {
        return;
    }
    let already_shown = state
        .shown_overlays
        .lock()
        .map(|shown| shown.contains(&monitor_id))
        .unwrap_or(false);
    if already_shown {
        return;
    }
    show_overlay(app, monitor_id);
}

/// Fall back to showing every overlay if the WebView never reports a frame.
///
/// Without this an unexpected frontend error would leave an invisible capture
/// session running. The timeout is long enough for a full-resolution decode.
fn start_show_watchdog(app: &AppHandle, generation: u64, monitors: Vec<u32>) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        let state = handle.state::<AppState>();
        if state.overlay_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let missing: Vec<u32> = {
            let shown = state
                .shown_overlays
                .lock()
                .map(|shown| shown.clone())
                .unwrap_or_default();
            monitors
                .into_iter()
                .filter(|id| !shown.contains(id))
                .collect()
        };
        if missing.is_empty() {
            return;
        }
        log::warn!("overlay frames not reported in time; showing overlays anyway: {missing:?}");
        let show_handle = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            for monitor_id in missing {
                show_overlay(&show_handle, monitor_id);
            }
        });
    });
}

/// Abort a session that never became visible (capture/decoding failure).
fn abort_session(app: &AppHandle) {
    hide_overlays(app);
    if app
        .state::<AppState>()
        .restore_windows
        .swap(false, Ordering::SeqCst)
    {
        set_app_visibility(app, true);
    }
    let state = app.state::<AppState>();
    {
        let mut pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.clear();
    }
    {
        let mut focused = state
            .focused_monitor
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *focused = None;
    }
}

/// How long to wait for the macOS Space-switch animation before recapturing.
///
/// The notification is posted while the transition is still animating, so the
/// overlay must stay hidden until the new Space is fully on screen. A newer
/// notification bumps [`AppState::space_change_generation`] and supersedes
/// any wait that is already running, so the last transition always wins.
#[cfg(target_os = "macos")]
const SPACE_SETTLE_MS: u64 = 620;

/// Re-freeze every display after the active macOS Space changes.
///
/// A screenshot overlay belongs to the desktop contents that existed when the
/// session began. Keeping the old frozen image over a different Space makes
/// window highlighting and drawing appear out of sync and shows the previous
/// Space during the transition, so the overlay is hidden immediately and the
/// capture restarts once the new Space has settled.
#[cfg(target_os = "macos")]
pub fn watch_space_changes(app: &AppHandle) {
    let handle = app.clone();
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        let state = handle.state::<AppState>();
        let session_active = state
            .pending
            .lock()
            .map(|pending| pending.started_at_ms != 0)
            .unwrap_or(false);
        if !session_active {
            return;
        }
        if now_ms().saturating_sub(state.last_space_recapture_ms.load(Ordering::SeqCst)) < 1_200 {
            return;
        }
        let generation = state.space_change_generation.fetch_add(1, Ordering::SeqCst) + 1;

        // Immediately put the overlays back into their empty "nothing selected"
        // state: the frozen frame of the previous Space must not travel with the
        // animation. The windows stay visible, so the user keeps seeing the
        // dimmed capture mask on top of the system's Space animation instead of
        // the overlay disappearing. Bumping the overlay generation also stops
        // the synthetic pointer bridge for the duration of the transition.
        state.overlay_generation.fetch_add(1, Ordering::SeqCst);
        for (label, window) in handle.webview_windows() {
            if label.starts_with(OVERLAY_PREFIX) {
                let _ = window.emit("overlay-space-changing", ());
            }
        }

        let recapture = handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(SPACE_SETTLE_MS));
            let state = recapture.state::<AppState>();
            // A newer Space transition or a cancelled session supersedes this one.
            if state.space_change_generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let still_active = state
                .pending
                .lock()
                .map(|pending| pending.started_at_ms != 0)
                .unwrap_or(false);
            if !still_active {
                return;
            }
            state
                .last_space_recapture_ms
                .store(now_ms(), Ordering::SeqCst);
            let recapture_handle = recapture.clone();
            // Runs on the main thread: it reads `NSWindow.windowNumber`.
            let _ = recapture.run_on_main_thread(move || {
                let state = recapture_handle.state::<AppState>();
                if state.space_change_generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                recapture_space(&recapture_handle, generation);
            });
        });
    });
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceActiveSpaceDidChangeNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(block);
    std::mem::forget(observer);
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
        .visible_on_all_workspaces(true)
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
///
/// The blocking screen grab runs on a worker thread and the window/AppKit work
/// is posted back to the main thread, so the hotkey path never freezes the UI
/// for the duration of a multi-display capture.
///
/// Must be called from the main thread (it hides the application windows).
pub fn begin(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.capture_lock.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        // Cancel any pending Space recapture: this session is the newest one.
        state.space_change_generation.fetch_add(1, Ordering::SeqCst);
    }

    hide_overlays(app);
    // Remember whether the app windows were on screen so a cancelled session can
    // restore them without popping the editor open on a normal tray-only capture.
    let windows_visible = app.webview_windows().iter().any(|(label, window)| {
        matches!(label.as_str(), "main" | "settings") && window.is_visible().unwrap_or(false)
    });
    app.state::<AppState>()
        .restore_windows
        .store(windows_visible, Ordering::SeqCst);
    // Hide Snaply windows before capturing.
    set_app_visibility(app, false);
    #[cfg(target_os = "macos")]
    crate::macos_windows::clear_content_bounds_cache();

    let handle = app.clone();
    std::thread::spawn(move || {
        // Give the compositor a moment to remove the windows and any previous
        // overlay before grabbing the screen, otherwise they end up in the shot.
        std::thread::sleep(std::time::Duration::from_millis(60));
        // Worker threads need their own autorelease pool on macOS: the capture
        // path allocates CoreGraphics/Foundation objects.
        #[cfg(target_os = "macos")]
        let captured = objc2::rc::autoreleasepool(|_pool| capture_frames());
        #[cfg(not(target_os = "macos"))]
        let captured = capture_frames();
        let main_handle = handle.clone();
        let posted = handle.run_on_main_thread(move || {
            let state = main_handle.state::<AppState>();
            if let Err(error) = begin_ui(&main_handle, captured) {
                log::error!("capture session failed: {error}");
                abort_session(&main_handle);
            }
            state.capture_lock.store(false, Ordering::SeqCst);
        });
        if posted.is_err() {
            // The app is shutting down; release the lock so a restarted app can capture.
            handle
                .state::<AppState>()
                .capture_lock
                .store(false, Ordering::SeqCst);
        }
    });
    Ok(())
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

/// Install a fresh session on the main thread.
///
/// Overlay windows are kept hidden and are only shown after their WebView has
/// painted the frame (`overlay_frame_ready`), which is what prevents the
/// previous Space's frozen image from flashing on screen.
fn begin_ui(app: &AppHandle, captured: Result<CapturedFrames, String>) -> Result<(), String> {
    let (infos, frames, geoms) = match captured {
        Ok(captured) => captured,
        Err(error) => {
            platform::permission_hint(app, &error);
            return Err(error);
        }
    };
    if infos.is_empty() {
        return Err("未检测到可截图的显示器".into());
    }

    let started_at_ms = now_ms();
    // capture_frames puts the monitor under the cursor first.
    let focused_id = infos.first().map(|monitor| monitor.id);

    // Cache the snapshot before touching overlay windows.
    {
        let state = app.state::<AppState>();
        let mut pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.frames_by_id = frames;
        pending.monitors = infos.clone();
        pending.geoms = geoms.clone();
        pending.started_at_ms = started_at_ms;
        drop(pending);
        {
            let mut focused = state
                .focused_monitor
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *focused = focused_id;
        }
        {
            let mut shown = state
                .shown_overlays
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            shown.clear();
        }
    }

    sync_overlay_windows(app, &infos)?;

    // Reuse the prewarmed overlay WebViews and ask them for the new raw frame.
    for monitor in infos.iter() {
        let (window, _) = ensure_overlay_window(app, monitor)?;
        configure_overlay_window(&window, monitor);
        // Stay hidden until the new frame is painted; see `overlay_frame_ready`.
        let _ = window.hide();
        let ready = app
            .state::<AppState>()
            .ready_overlays
            .lock()
            .map(|ready| ready.contains(&monitor.id))
            .unwrap_or(false);
        if ready {
            let _ = window.emit("overlay-refresh", started_at_ms);
        }
    }

    let generation = app
        .state::<AppState>()
        .overlay_generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    start_pointer_bridge(app, generation, geoms);
    start_show_watchdog(
        app,
        generation,
        infos.iter().map(|monitor| monitor.id).collect(),
    );
    Ok(())
}

/// Return a monitor overlay's CoreGraphics window number.
///
/// Must run on the main thread (Cocoa window object).
#[cfg(target_os = "macos")]
fn overlay_window_number(app: &AppHandle, monitor_id: u32) -> Option<u32> {
    let window = app.get_webview_window(&overlay_label(monitor_id))?;
    let pointer = window.ns_window().ok()?;
    if pointer.is_null() {
        return None;
    }
    // SAFETY: `ns_window()` hands out the live `NSWindow` owned by the WebView,
    // and the window is guaranteed to outlive this call because we hold the
    // `WebviewWindow` handle. Reading the window number does not retain it.
    let ns_window = unsafe { &*(pointer.cast::<NSWindow>()) };
    u32::try_from(ns_window.windowNumber()).ok()
}

/// Re-freeze every display after a Space switch without hiding the overlay.
///
/// The overlay keeps showing its dimmed "nothing selected" state while the new
/// Space is captured; because the capture excludes this app's own windows, the
/// mask is never baked into the frozen frame and there is no visible gap.
/// Falls back to the hide-and-recapture path when the excluded capture is not
/// available (e.g. the overlay window is gone).
#[cfg(target_os = "macos")]
fn recapture_space(app: &AppHandle, generation: u64) {
    let state = app.state::<AppState>();
    let (monitors, geoms) = {
        let pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (pending.monitors.clone(), pending.geoms.clone())
    };
    if monitors.is_empty() {
        return;
    }

    let targets: Vec<(u32, u32)> = monitors
        .iter()
        .map(|monitor| overlay_window_number(app, monitor.id).map(|number| (monitor.id, number)))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    if targets.len() != monitors.len() {
        log::warn!("overlay window ids unavailable; falling back to a hidden recapture");
        let fallback = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = begin(&fallback);
        });
        return;
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        let mut frames = HashMap::with_capacity(targets.len());
        for (monitor_id, number) in targets {
            let Some(geom) = geoms.iter().find(|geom| geom.id == monitor_id) else {
                continue;
            };
            let rect = CGRect::new(
                CGPoint::new(geom.x, geom.y),
                CGSize::new(geom.width.max(1.0), geom.height.max(1.0)),
            );
            let captured = objc2::rc::autoreleasepool(|_pool| {
                crate::macos_windows::capture_display_excluding_window(rect, number)
            });
            let Some(image) = captured else {
                log::warn!("capture below overlay failed; falling back to a hidden recapture");
                let fallback = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let _ = begin(&fallback);
                });
                return;
            };
            let expected_width = (geom.width * geom.scale).round() as u32;
            let expected_height = (geom.height * geom.scale).round() as u32;
            if image.width().abs_diff(expected_width) > 2
                || image.height().abs_diff(expected_height) > 2
            {
                log::warn!(
                    "capture below overlay returned {}x{} (expected {}x{}); falling back",
                    image.width(),
                    image.height(),
                    expected_width,
                    expected_height
                );
                let fallback = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let _ = begin(&fallback);
                });
                return;
            }
            let (width, height) = image.dimensions();
            frames.insert(
                monitor_id,
                CapturedFrame {
                    width,
                    height,
                    rgba: Arc::new(image.into_raw()),
                },
            );
        }
        let install_handle = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            install_space_frames(&install_handle, frames, generation);
        });
    });
}

/// Publish freshly captured Space frames while the overlays stay visible.
#[cfg(target_os = "macos")]
fn install_space_frames(app: &AppHandle, frames: HashMap<u32, CapturedFrame>, generation: u64) {
    let state = app.state::<AppState>();
    if state.space_change_generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let started_at_ms = now_ms();
    let geoms = {
        let mut pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // The session was cancelled or replaced while the capture was running.
        if pending.started_at_ms == 0 || pending.monitors.is_empty() {
            return;
        }
        pending.frames_by_id = frames;
        pending.started_at_ms = started_at_ms;
        pending.geoms.clone()
    };

    for (label, window) in app.webview_windows() {
        let Some(monitor_id) = monitor_id_from_label(&label) else {
            continue;
        };
        let ready = state
            .ready_overlays
            .lock()
            .map(|ready| ready.contains(&monitor_id))
            .unwrap_or(false);
        if ready {
            let _ = window.emit("overlay-refresh", started_at_ms);
        }
    }

    // Switching Spaces can deactivate the app; make the capture surface key
    // again so pointer and keyboard keep working without hiding anything.
    if let Some(focused) = state
        .focused_monitor
        .lock()
        .map(|focused| *focused)
        .unwrap_or(None)
    {
        show_overlay(app, focused);
    }

    let generation = state.overlay_generation.fetch_add(1, Ordering::SeqCst) + 1;
    start_pointer_bridge(app, generation, geoms);
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

/// Everything the CPU-heavy half of a commit needs, captured on the main thread.
pub struct CommitPlan {
    pub frame: CapturedFrame,
    pub scale: f64,
    pub shot_id: u64,
    pub dir: std::path::PathBuf,
    pub copy_only: bool,
}

/// Snapshot the session state needed to commit, on the main thread.
///
/// Resolving the capture directory here keeps path lookups (which may touch
/// platform APIs) off the blocking worker thread.
pub fn commit_prepare(app: &AppHandle, monitor_id: u32) -> Result<CommitPlan, String> {
    let state = app.state::<AppState>();
    let (frame, scale, shot_id) = {
        let pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let geom = pending
            .geoms
            .iter()
            .find(|geom| geom.id == monitor_id)
            .ok_or("找不到对应显示器")?;
        let frame = pending
            .frames_by_id
            .get(&monitor_id)
            .cloned()
            .ok_or("截图缓存已失效，请重新截图")?;
        (frame, geom.scale, pending.started_at_ms)
    };
    let settings = crate::settings::load(app);
    let dir = crate::settings::capture_dir(app, &settings)?;
    Ok(CommitPlan {
        frame,
        scale,
        shot_id,
        dir,
        copy_only: settings.copy_after_capture,
    })
}

/// Result of the CPU-heavy half of a commit.
pub struct CommitOutcome {
    pub result: CreateResult,
    pub shot: crate::state::ShotImage,
    /// Resolved file path, used by `save_image` later on.
    pub path: String,
    /// True when the result should not bring the editor forward.
    pub copy_only: bool,
}

/// Crop the selected region, encode the PNG, and write it to disk.
///
/// Runs on a blocking worker thread: this is the expensive part of a commit.
#[allow(clippy::too_many_arguments)]
pub fn commit_execute(
    plan: &CommitPlan,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    mode: &str,
) -> Result<CommitOutcome, String> {
    let frame = &plan.frame;
    let (sx, sy, sw, sh) = clamp_crop(x, y, w, h, plan.scale, frame.width, frame.height);
    let crop = image::RgbaImage::from_raw(
        sw,
        sh,
        crop_rgba(&frame.rgba, frame.width, frame.height, sx, sy, sw, sh)?,
    )
    .ok_or("无法裁切截图")?;

    let dir = &plan.dir;
    let path = unique_capture_path(dir);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("Snaply-{}.png", timestamp()));
    crop.save(&path).map_err(|e| e.to_string())?;

    let (width, height) = crop.dimensions();
    let copy_only = mode == "copy" || plan.copy_only;
    Ok(CommitOutcome {
        shot: crate::state::ShotImage {
            width,
            height,
            rgba: crop.into_raw(),
        },
        path: path.to_string_lossy().into_owned(),
        result: CreateResult {
            shot_id: plan.shot_id,
            absolute_path: path.to_string_lossy().into_owned(),
            file_name,
            mode: if copy_only {
                "copy".into()
            } else {
                "edit".into()
            },
        },
        copy_only,
    })
}

/// Finish a commit on the main thread: publish the shot, close the overlay, and
/// bring the editor forward when needed.
pub fn commit_finish(app: &AppHandle, outcome: CommitOutcome) -> Result<(), String> {
    let state = app.state::<AppState>();
    *state
        .last_shot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(outcome.shot);
    *state
        .last_edit_path
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(outcome.path.clone());

    hide_overlays(app);
    state.restore_windows.store(false, Ordering::SeqCst);
    {
        let mut pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.clear();
    }
    {
        let mut focused = state
            .focused_monitor
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *focused = None;
    }
    // Copy mode stays in the background; edit mode brings the main window forward.
    if !outcome.copy_only {
        show_main_window(app)?;
    }
    Ok(())
}

/// Clamp a logical selection to physical pixel bounds.
///
/// Exposed for unit tests: negative/NaN/oversized selections must never produce
/// an out-of-bounds crop.
fn clamp_crop(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    scale: f64,
    width: u32,
    height: u32,
) -> (u32, u32, u32, u32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let sx = ((x.max(0.0) * scale).round() as u32).min(width.saturating_sub(1));
    let sy = ((y.max(0.0) * scale).round() as u32).min(height.saturating_sub(1));
    let sw = ((w.max(1.0) * scale).round() as u32)
        .min(width.saturating_sub(sx))
        .max(1);
    let sh = ((h.max(1.0) * scale).round() as u32)
        .min(height.saturating_sub(sy))
        .max(1);
    (sx, sy, sw, sh)
}

/// Build a file path that does not clobber an existing capture.
///
/// Two captures within the same second used to overwrite each other silently.
pub fn unique_capture_path(dir: &std::path::Path) -> std::path::PathBuf {
    let stamp = timestamp();
    let candidate = dir.join(format!("Snaply-{stamp}.png"));
    if !candidate.exists() {
        return candidate;
    }
    for index in 1..1_000u32 {
        let candidate = dir.join(format!("Snaply-{stamp}-{index}.png"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("Snaply-{stamp}-{}.png", now_ms()))
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
        || x.checked_add(width).is_none_or(|right| right > image_width)
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > image_height)
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
    let state = app.state::<AppState>();
    if state.restore_windows.swap(false, Ordering::SeqCst) {
        set_app_visibility(app, true);
    }
    {
        let mut pending = state
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.clear();
    }
    {
        let mut focused = state
            .focused_monitor
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *focused = None;
    }
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
    #[test]
    fn clamps_negative_nan_and_oversized_crops() {
        // Negative origin is pulled back to zero.
        assert_eq!(
            clamp_crop(-10.0, -5.0, 20.0, 20.0, 2.0, 100, 100),
            (0, 0, 40, 40)
        );
        // Huge origin/selection never exceeds the frame.
        assert_eq!(
            clamp_crop(1e9, 1e9, 1e9, 1e9, 2.0, 100, 100),
            (99, 99, 1, 1)
        );
        // NaN is treated as zero and a zero selection still produces one pixel.
        assert_eq!(
            clamp_crop(f64::NAN, f64::NAN, 1.0, 1.0, 2.0, 100, 100),
            (0, 0, 2, 2)
        );
        // Full selection maps to the whole frame.
        assert_eq!(
            clamp_crop(0.0, 0.0, 1000.0, 1000.0, 1.0, 100, 100),
            (0, 0, 100, 100)
        );
        // A bogus scale factor must not produce a zero-sized crop.
        assert_eq!(
            clamp_crop(0.0, 0.0, 10.0, 10.0, 0.0, 100, 100),
            (0, 0, 10, 10)
        );
    }

    #[test]
    fn unique_capture_path_never_reuses_a_name() {
        let dir = std::env::temp_dir().join(format!("snaply-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_capture_path(&dir);
        std::fs::write(&first, b"x").unwrap();
        let second = unique_capture_path(&dir);
        assert_ne!(first, second, "second capture must not overwrite the first");
        assert_eq!(second.parent(), Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
