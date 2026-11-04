use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, AtomicU64},
    Arc, Mutex,
};

use crate::overlay::MonitorInfo;
use crate::platform::MonitorGeom;
use crate::settings::Settings;

pub struct PendingCapture {
    pub frames_by_id: std::collections::HashMap<u32, CapturedFrame>,
    pub monitors: Vec<MonitorInfo>,
    pub geoms: Vec<MonitorGeom>,
    pub started_at_ms: u64,
    pub session_id: u64,
}

#[derive(Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

impl PendingCapture {
    pub fn new() -> Self {
        Self {
            frames_by_id: Default::default(),
            monitors: Vec::new(),
            geoms: Vec::new(),
            started_at_ms: 0,
            session_id: 0,
        }
    }
    pub fn clear(&mut self) {
        self.frames_by_id.clear();
        self.monitors.clear();
        self.geoms.clear();
        self.started_at_ms = 0;
        self.session_id = 0;
    }
}

/// Captured image waiting for annotation, stored as raw RGBA pixels.
pub struct ShotImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct AppState {
    /// Active settings cache. Updates are persisted immediately.
    pub settings: Mutex<Settings>,
    pub last_shot: Mutex<Option<ShotImage>>,
    /// Prevent repeated hotkey presses from stacking overlay sessions.
    pub capture_lock: AtomicBool,
    /// Invalidates late results from a superseded capture session.
    pub capture_session: AtomicU64,
    /// Invalidates native pointer pollers when an overlay session closes.
    pub overlay_generation: AtomicU64,
    /// Coalesces repeated active-Space notifications into one recapture.
    #[cfg(target_os = "macos")]
    pub space_change_generation: AtomicU64,
    /// Prevents activation feedback from immediately recapturing again.
    #[cfg(target_os = "macos")]
    pub last_space_recapture_ms: AtomicU64,
    pub pending: Mutex<PendingCapture>,
    /// Monitor ids whose prewarmed overlay WebView has finished loading.
    pub ready_overlays: Mutex<HashSet<u32>>,
    /// Monitor ids whose overlay window is currently visible for this session.
    ///
    /// Overlay windows stay hidden until the WebView has painted the current
    /// frame, so the user never sees a stale screenshot from a previous
    /// session or a previous Space.
    pub shown_overlays: Mutex<HashSet<u32>>,
    /// Monitor under the cursor when the session started; it receives focus.
    pub focused_monitor: Mutex<Option<u32>>,
    /// Path to the most recent editor image for follow-up actions.
    pub last_edit_path: Mutex<Option<String>>,
    /// Shortcut that is actually registered with the OS; may differ from the
    /// persisted value when the saved accelerator was already taken.
    pub registered_shortcut: Mutex<String>,
    /// Whether the app windows were visible before the capture hid them, so a
    /// cancelled session can restore exactly the previous state.
    pub restore_windows: AtomicBool,
    /// Validated target for the next raw-body `save_image` call.
    pub pending_save_path: Mutex<Option<std::path::PathBuf>>,
    /// Explicit quit requests bypass the tray application exit guard.
    pub allow_exit: AtomicBool,
    /// Last Dock visibility applied on macOS.
    #[cfg(target_os = "macos")]
    pub dock_visible: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            settings: Mutex::new(Settings::default()),
            last_shot: Mutex::new(None),
            capture_lock: AtomicBool::new(false),
            capture_session: AtomicU64::new(0),
            overlay_generation: AtomicU64::new(0),
            #[cfg(target_os = "macos")]
            space_change_generation: AtomicU64::new(0),
            #[cfg(target_os = "macos")]
            last_space_recapture_ms: AtomicU64::new(0),
            pending: Mutex::new(PendingCapture::new()),
            ready_overlays: Mutex::new(HashSet::new()),
            shown_overlays: Mutex::new(HashSet::new()),
            focused_monitor: Mutex::new(None),
            last_edit_path: Mutex::new(None),
            registered_shortcut: Mutex::new(Settings::default().shortcut),
            restore_windows: AtomicBool::new(false),
            pending_save_path: Mutex::new(None),
            allow_exit: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            // macOS starts as a regular foreground app with a visible Dock icon.
            dock_visible: AtomicBool::new(true),
        }
    }
}

impl Default for PendingCapture {
    fn default() -> Self {
        Self::new()
    }
}
