use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Default shortcut: Cmd+Shift+1 on macOS and Ctrl+Shift+1 elsewhere.
pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+1";
const LEGACY_DEFAULT_SHORTCUT: &str = "Alt+Shift+S";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "zh-CN")]
    #[default]
    SimplifiedChinese,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockVisibility {
    #[default]
    #[serde(rename = "hide")]
    Hide,
    #[serde(rename = "followWindow")]
    FollowWindow,
    #[serde(rename = "show")]
    Show,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Global capture shortcut using Tauri accelerator syntax.
    pub shortcut: String,
    /// Custom output directory. An empty value uses the system Pictures folder.
    pub save_dir: Option<String>,
    /// Copy captures directly to the clipboard without opening the editor.
    pub copy_after_capture: bool,
    /// Language used by user-facing settings and application chrome.
    pub language: Language,
    /// Controls when Snaply appears in the macOS Dock.
    pub dock_visibility: DockVisibility,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            save_dir: None,
            copy_after_capture: false,
            language: Language::default(),
            dock_visibility: DockVisibility::Hide,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    let Ok(path) = settings_path(app) else {
        return Settings::default();
    };
    // Missing or malformed files fall back to defaults so users are never blocked.
    let saved: Settings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    // Older builds persisted the previous default shortcut; migrate it.
    if saved.shortcut == LEGACY_DEFAULT_SHORTCUT {
        return Settings {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            ..saved
        };
    }
    saved
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Resolve the capture directory, preferring a custom path.
pub fn capture_dir(app: &AppHandle, settings: &Settings) -> Result<PathBuf, String> {
    let dir = match settings.save_dir.as_deref().filter(|s| !s.is_empty()) {
        Some(custom) => PathBuf::from(custom),
        None => {
            let base = app
                .path()
                .picture_dir()
                .or_else(|_| app.path().app_data_dir())
                .map_err(|e| e.to_string())?;
            base.join("Snaply")
        }
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_use_default_language() {
        let settings: Settings = serde_json::from_str(
            r#"{"shortcut":"CommandOrControl+Shift+1","saveDir":null,"copyAfterCapture":false}"#,
        )
        .unwrap();
        assert_eq!(settings.language, Language::SimplifiedChinese);
        assert_eq!(settings.dock_visibility, DockVisibility::Hide);
    }

    #[test]
    fn language_round_trips_with_frontend_codes() {
        let settings = Settings {
            language: Language::English,
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""language":"en""#));
        assert!(json.contains(r#""dockVisibility":"hide""#));
        let decoded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.language, Language::English);
        assert_eq!(decoded.dock_visibility, DockVisibility::Hide);
    }
}
