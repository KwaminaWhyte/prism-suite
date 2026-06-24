//! Application **Preferences** — the data model behind Pulse's Preferences
//! dialog, plus JSON load/save to a path (After Effects' `Preferences.txt`).
//!
//! Four panes: **general** (undo levels, autosave), **display** (UI scale, hover
//! info, motion-path length), **media** (disk-cache dir, RAM reserve, conform
//! frame rate), and **previews** (preview quality, fast-draft, frozen layers).
//! Everything is plain serializable data so it round-trips deterministically;
//! `load`/`save` use `serde_json` over the workspace-pinned `serde_json` crate.

use std::path::Path;

use super::{App, Action};

/// General preferences pane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GeneralPrefs {
    /// Levels of undo to keep (AE allows 1–99).
    pub undo_levels: u32,
    /// Auto-save the project every N minutes (`0` disables).
    pub autosave_minutes: u32,
    /// Maximum number of project auto-save copies to keep.
    pub autosave_max_copies: u32,
    /// Whether tooltips are shown.
    pub show_tooltips: bool,
}

impl Default for GeneralPrefs {
    fn default() -> Self {
        Self {
            undo_levels: 32,
            autosave_minutes: 20,
            autosave_max_copies: 5,
            show_tooltips: true,
        }
    }
}

/// Display preferences pane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DisplayPrefs {
    /// UI scale factor (`1.0` = 100%).
    pub ui_scale: f32,
    /// Show info tooltips while hovering layers/keyframes.
    pub show_hover_info: bool,
    /// Number of keyframes drawn along a motion path (`0` = all).
    pub motion_path_keyframes: u32,
    /// Use a dark UI theme.
    pub dark_theme: bool,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            show_hover_info: true,
            motion_path_keyframes: 5,
            dark_theme: true,
        }
    }
}

/// Media & disk-cache preferences pane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MediaPrefs {
    /// Disk-cache directory (where pre-rendered frames live).
    pub disk_cache_dir: String,
    /// Disk-cache maximum size in gigabytes.
    pub disk_cache_max_gb: f32,
    /// Fraction of RAM reserved for other apps (0.0–1.0).
    pub ram_reserve_fraction: f32,
    /// Default conform frame rate for imported footage.
    pub conform_fps: f32,
}

impl Default for MediaPrefs {
    fn default() -> Self {
        Self {
            disk_cache_dir: "~/Library/Caches/PulseDiskCache".to_string(),
            disk_cache_max_gb: 30.0,
            ram_reserve_fraction: 0.25,
            conform_fps: 30.0,
        }
    }
}

/// Preview quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum PreviewQuality {
    Full,
    #[default]
    Half,
    Third,
    Quarter,
}

impl PreviewQuality {
    /// Resolution divisor (1, 2, 3, 4).
    pub fn divisor(self) -> u32 {
        match self {
            PreviewQuality::Full => 1,
            PreviewQuality::Half => 2,
            PreviewQuality::Third => 3,
            PreviewQuality::Quarter => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PreviewQuality::Full => "Full",
            PreviewQuality::Half => "Half",
            PreviewQuality::Third => "Third",
            PreviewQuality::Quarter => "Quarter",
        }
    }

    pub const ALL: [PreviewQuality; 4] = [
        PreviewQuality::Full,
        PreviewQuality::Half,
        PreviewQuality::Third,
        PreviewQuality::Quarter,
    ];
}

/// Previews preferences pane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PreviewPrefs {
    pub quality: PreviewQuality,
    /// Use fast-draft GPU rendering for previews.
    pub fast_draft: bool,
    /// Show frozen-layer indicators.
    pub show_frozen_layers: bool,
    /// Target preview FPS (`0` = match comp).
    pub preview_fps: f32,
}

impl Default for PreviewPrefs {
    fn default() -> Self {
        Self {
            quality: PreviewQuality::Half,
            fast_draft: true,
            show_frozen_layers: true,
            preview_fps: 0.0,
        }
    }
}

/// The full preferences document, persisted as JSON.
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Preferences {
    pub general: GeneralPrefs,
    pub display: DisplayPrefs,
    pub media: MediaPrefs,
    pub previews: PreviewPrefs,
}

impl Preferences {
    /// Serialize to a pretty JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Parse from a JSON string.
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Write the preferences to `path` as JSON, creating parent dirs as needed.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let json = self.to_json()?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Load preferences from `path`, falling back to [`Default`] if the file is
    /// missing (so a first launch always succeeds with sensible defaults).
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_json(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl App {
    pub(super) fn apply_preferences(&mut self, action: Action) {
        match action {
            Action::TogglePreferences => {
                self.preferences_open = !self.preferences_open;
            }
            Action::SetPrefUndoLevels(n) => {
                self.preferences.general.undo_levels = n.clamp(1, 99);
            }
            Action::SetPrefAutosaveMinutes(n) => {
                self.preferences.general.autosave_minutes = n.min(240);
            }
            Action::SetPrefShowTooltips(b) => {
                self.preferences.general.show_tooltips = b;
            }
            Action::SetPrefUiScale(s) => {
                self.preferences.display.ui_scale = s.clamp(0.5, 3.0);
            }
            Action::SetPrefDarkTheme(b) => {
                self.preferences.display.dark_theme = b;
            }
            Action::SetPrefMotionPathKeyframes(n) => {
                self.preferences.display.motion_path_keyframes = n.min(100);
            }
            Action::SetPrefDiskCacheDir(dir) => {
                self.preferences.media.disk_cache_dir = dir;
            }
            Action::SetPrefDiskCacheMaxGb(g) => {
                self.preferences.media.disk_cache_max_gb = g.clamp(1.0, 4096.0);
            }
            Action::SetPrefRamReserve(f) => {
                self.preferences.media.ram_reserve_fraction = f.clamp(0.0, 0.9);
            }
            Action::SetPrefConformFps(f) => {
                self.preferences.media.conform_fps = f.clamp(1.0, 240.0);
            }
            Action::SetPrefPreviewQuality(q) => {
                self.preferences.previews.quality = q;
            }
            Action::SetPrefFastDraft(b) => {
                self.preferences.previews.fast_draft = b;
            }
            Action::SavePreferences(path) => {
                self.last_prefs_save_result = Some(
                    self.preferences
                        .save(&path)
                        .map(|_| format!("Saved to {}", path.display()))
                        .unwrap_or_else(|e| format!("Save failed: {e}")),
                );
            }
            Action::LoadPreferences(path) => {
                match Preferences::load(&path) {
                    Ok(prefs) => {
                        self.preferences = prefs;
                        self.last_prefs_save_result = Some(format!("Loaded {}", path.display()));
                    }
                    Err(e) => {
                        self.last_prefs_save_result = Some(format!("Load failed: {e}"));
                    }
                }
            }
            Action::ResetPreferences => {
                self.preferences = Preferences::default();
            }
            _ => unreachable!("apply_preferences called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_are_sane() {
        let p = Preferences::default();
        assert_eq!(p.general.undo_levels, 32);
        assert_eq!(p.previews.quality, PreviewQuality::Half);
        assert_eq!(p.previews.quality.divisor(), 2);
    }

    #[test]
    fn test_json_round_trips() {
        let mut p = Preferences::default();
        p.general.undo_levels = 50;
        p.display.ui_scale = 1.5;
        p.media.disk_cache_max_gb = 64.0;
        p.previews.quality = PreviewQuality::Quarter;
        let json = p.to_json().unwrap();
        let back = Preferences::from_json(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn test_save_and_load_roundtrip_to_disk() {
        // Unique path (pid + nanos) so parallel test runs never collide on a
        // shared filename (a fixed path could read a stale default).
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir()
            .join(format!("pulse_prefs_test_{}_{}", std::process::id(), stamp));
        let path = dir.join("prefs.json");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Preferences::default();
        p.general.autosave_minutes = 7;
        p.media.disk_cache_dir = "/tmp/cache".to_string();
        p.save(&path).unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.general.autosave_minutes, 7);
        assert_eq!(loaded.media.disk_cache_dir, "/tmp/cache");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_file_falls_back_to_default() {
        let path = std::env::temp_dir().join("pulse_prefs_does_not_exist_xyz.json");
        let _ = std::fs::remove_file(&path);
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded, Preferences::default());
    }

    #[test]
    fn test_undo_levels_clamped() {
        let mut app = App::new();
        app.apply(Action::SetPrefUndoLevels(500));
        assert_eq!(app.preferences.general.undo_levels, 99);
        app.apply(Action::SetPrefUndoLevels(0));
        assert_eq!(app.preferences.general.undo_levels, 1);
    }

    #[test]
    fn test_ui_scale_clamped() {
        let mut app = App::new();
        app.apply(Action::SetPrefUiScale(10.0));
        assert!((app.preferences.display.ui_scale - 3.0).abs() < 1e-6);
        app.apply(Action::SetPrefUiScale(0.1));
        assert!((app.preferences.display.ui_scale - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_ram_reserve_clamped() {
        let mut app = App::new();
        app.apply(Action::SetPrefRamReserve(2.0));
        assert!((app.preferences.media.ram_reserve_fraction - 0.9).abs() < 1e-6);
        app.apply(Action::SetPrefRamReserve(-1.0));
        assert!((app.preferences.media.ram_reserve_fraction - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_toggle_panel_and_reset() {
        let mut app = App::new();
        assert!(!app.preferences_open);
        app.apply(Action::TogglePreferences);
        assert!(app.preferences_open);
        app.apply(Action::SetPrefUndoLevels(99));
        app.apply(Action::ResetPreferences);
        assert_eq!(app.preferences, Preferences::default());
    }

    #[test]
    fn test_save_load_actions() {
        let mut app = App::new();
        // Unique path (pid + nanos) so a stale file from a prior run can't be read.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir()
            .join(format!("pulse_prefs_action_{}_{}", std::process::id(), stamp));
        let path = dir.join("prefs.json");
        let _ = std::fs::remove_dir_all(&dir);
        app.apply(Action::SetPrefConformFps(24.0));
        app.apply(Action::SavePreferences(path.clone()));
        assert!(app.last_prefs_save_result.as_ref().unwrap().contains("Saved"));
        // Mutate then reload to confirm persistence.
        app.apply(Action::SetPrefConformFps(60.0));
        app.apply(Action::LoadPreferences(path.clone()));
        assert!((app.preferences.media.conform_fps - 24.0).abs() < 1e-6,
            "expected 24 after reload, got {}", app.preferences.media.conform_fps);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_preview_quality_set() {
        let mut app = App::new();
        app.apply(Action::SetPrefPreviewQuality(PreviewQuality::Full));
        assert_eq!(app.preferences.previews.quality, PreviewQuality::Full);
        assert_eq!(app.preferences.previews.quality.divisor(), 1);
    }
}
