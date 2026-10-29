use std::sync::{atomic::AtomicBool, Mutex};

use crate::overlay::MonitorInfo;
use crate::platform::MonitorGeom;
use crate::settings::Settings;

pub struct PendingCapture {
    pub png_by_id: std::collections::HashMap<u32, Vec<u8>>,
    pub monitors: Vec<MonitorInfo>,
    pub geoms: Vec<MonitorGeom>,
    pub started_at_ms: u64,
}

impl PendingCapture {
    pub fn new() -> Self {
        Self {
            png_by_id: Default::default(),
            monitors: Vec::new(),
            geoms: Vec::new(),
            started_at_ms: 0,
        }
    }
    pub fn clear(&mut self) {
        self.png_by_id.clear();
        self.monitors.clear();
        self.geoms.clear();
        self.started_at_ms = 0;
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
    pub pending: Mutex<PendingCapture>,
    /// Path to the most recent editor image for follow-up actions.
    pub last_edit_path: Mutex<Option<String>>,
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
            pending: Mutex::new(PendingCapture::new()),
            last_edit_path: Mutex::new(None),
            allow_exit: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            dock_visible: AtomicBool::new(false),
        }
    }
}

impl Default for PendingCapture {
    fn default() -> Self {
        Self::new()
    }
}
