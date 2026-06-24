//! Rich application preferences (Photoshop *Edit > Preferences* parity).
//!
//! `AppPrefs` in `mod.rs` is the *minimal* persisted state (window size + recent
//! files). This module adds a richer [`PigmentPreferences`] model covering the
//! Performance / Color / Interface / File-Handling preference panes, with its own
//! JSON load/save to a configurable path. It is intentionally decoupled from
//! `AppPrefs` so the small launch-critical prefs stay cheap to read while the
//! full pane state lives in its own file (`pigment_preferences.json`).
//!
//! Everything here is app-local and deterministic — no GPU, no I/O at
//! construction. `load`/`save` are explicit and tolerant: a missing or corrupt
//! file falls back to defaults so the editor always starts.

use serde::{Deserialize, Serialize};

use super::{Action, App};

/// How much RAM Pigment may use for the tile/history cache, as a percentage of
/// physical memory. Mirrors Photoshop's Performance > Memory Usage slider.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformancePrefs {
    /// Fraction of physical RAM the app may use (0.10..=0.90).
    pub memory_usage_fraction: f32,
    /// Number of history states kept for undo (1..=1000).
    pub history_states: u32,
    /// Number of cache levels for the image pyramid (1..=8).
    pub cache_levels: u8,
    /// Tile cache size in MiB (16..=8192).
    pub cache_tile_mib: u32,
    /// Use the GPU compositor when an adapter is available.
    pub use_gpu: bool,
    /// Number of worker threads for CPU filters (0 = auto = num_cpus).
    pub worker_threads: u8,
}

impl Default for PerformancePrefs {
    fn default() -> Self {
        Self {
            memory_usage_fraction: 0.70,
            history_states: 50,
            cache_levels: 4,
            cache_tile_mib: 1024,
            use_gpu: true,
            worker_threads: 0,
        }
    }
}

impl PerformancePrefs {
    /// Clamp every field to its documented valid range.
    pub fn clamp(&mut self) {
        self.memory_usage_fraction = self.memory_usage_fraction.clamp(0.10, 0.90);
        self.history_states = self.history_states.clamp(1, 1000);
        self.cache_levels = self.cache_levels.clamp(1, 8);
        self.cache_tile_mib = self.cache_tile_mib.clamp(16, 8192);
    }
}

/// Working-space / color-management preferences (Color Settings pane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorPrefs {
    /// RGB working space name (e.g. "sRGB IEC61966-2.1").
    pub working_rgb: String,
    /// CMYK working space name.
    pub working_cmyk: String,
    /// Gray working space (gamma) name.
    pub working_gray: String,
    /// Default rendering intent label ("Perceptual" / "Relative Colorimetric" / …).
    pub rendering_intent: String,
    /// Use black-point compensation during conversions.
    pub black_point_compensation: bool,
    /// Use dithering when converting between 8-bit spaces.
    pub use_dither: bool,
}

impl Default for ColorPrefs {
    fn default() -> Self {
        Self {
            working_rgb: "sRGB IEC61966-2.1".into(),
            working_cmyk: "U.S. Web Coated (SWOP) v2".into(),
            working_gray: "Gray Gamma 2.2".into(),
            rendering_intent: "Relative Colorimetric".into(),
            black_point_compensation: true,
            use_dither: true,
        }
    }
}

/// Interface / appearance preferences (Interface pane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfacePrefs {
    /// UI theme: "Dark", "Medium Dark", "Medium Light", "Light".
    pub theme: String,
    /// UI scale factor (0.5..=4.0).
    pub ui_scale: f32,
    /// Show tool tips on hover.
    pub show_tooltips: bool,
    /// Show channels in their own color rather than grayscale.
    pub show_channels_in_color: bool,
    /// Highlight color for the canvas pasteboard ("Default", "Black", "Light Gray", …).
    pub pasteboard_color: String,
    /// Dynamic (animated) color sliders.
    pub dynamic_color_sliders: bool,
}

impl Default for InterfacePrefs {
    fn default() -> Self {
        Self {
            theme: "Dark".into(),
            ui_scale: 1.0,
            show_tooltips: true,
            show_channels_in_color: false,
            pasteboard_color: "Default".into(),
            dynamic_color_sliders: true,
        }
    }
}

/// File-handling preferences (File Handling pane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileHandlingPrefs {
    /// Maximize PSD/PSB compatibility ("Always", "Ask", "Never").
    pub maximize_psd_compatibility: String,
    /// Auto-save recovery interval in minutes (0 = off; else 1..=60).
    pub autosave_minutes: u32,
    /// Number of recent files to remember (0..=100).
    pub recent_file_count: u32,
    /// Append a copy suffix when saving over a file in a foreign format.
    pub append_copy_suffix: bool,
    /// Embed the document color profile on save.
    pub embed_color_profile: bool,
    /// Save thumbnails (image previews) with documents.
    pub save_thumbnails: bool,
}

impl Default for FileHandlingPrefs {
    fn default() -> Self {
        Self {
            maximize_psd_compatibility: "Ask".into(),
            autosave_minutes: 10,
            recent_file_count: 20,
            append_copy_suffix: true,
            embed_color_profile: true,
            save_thumbnails: true,
        }
    }
}

/// The full preferences model spanning every pane. Persisted as one JSON file.
/// `#[serde(default)]` on every field keeps old/partial files loadable.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PigmentPreferences {
    #[serde(default)]
    pub performance: PerformancePrefs,
    #[serde(default)]
    pub color: ColorPrefs,
    #[serde(default)]
    pub interface: InterfacePrefs,
    #[serde(default)]
    pub file_handling: FileHandlingPrefs,
}

impl PigmentPreferences {
    /// Default path: alongside the minimal `pigment_prefs.json`, namely
    /// `<config>/prism/pigment_preferences.json`.
    pub fn default_path() -> std::path::PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".config")
        });
        base.join("prism").join("pigment_preferences.json")
    }

    /// Serialize to a pretty JSON string (never fails for this plain struct).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Parse from a JSON string, falling back to defaults for absent fields and
    /// to a full default on a hard parse error.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }

    /// Load from `path`, returning defaults on any error (missing file, parse
    /// failure). Always succeeds.
    pub fn load_from(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_json(&s),
            Err(_) => Self::default(),
        }
    }

    /// Save to `path`, creating parent directories. Returns the error string on
    /// failure (callers may log it; a prefs write must never crash the app).
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, self.to_json()).map_err(|e| e.to_string())
    }
}

impl App {
    pub(super) fn apply_prefs(&mut self, action: Action) {
        match action {
            Action::SetPrefMemoryFraction(f) => {
                self.preferences.performance.memory_usage_fraction = f;
                self.preferences.performance.clamp();
            }
            Action::SetPrefHistoryStates(n) => {
                self.preferences.performance.history_states = n;
                self.preferences.performance.clamp();
            }
            Action::SetPrefUseGpu(b) => {
                self.preferences.performance.use_gpu = b;
            }
            Action::SetPrefWorkingRgb(name) => {
                self.preferences.color.working_rgb = name;
            }
            Action::SetPrefRenderingIntent(name) => {
                self.preferences.color.rendering_intent = name;
            }
            Action::SetPrefTheme(name) => {
                self.preferences.interface.theme = name;
            }
            Action::SetPrefUiScale(s) => {
                self.preferences.interface.ui_scale = s.clamp(0.5, 4.0);
            }
            Action::SetPrefAutosaveMinutes(m) => {
                self.preferences.file_handling.autosave_minutes = m.min(60);
            }
            Action::SetPrefRecentFileCount(n) => {
                self.preferences.file_handling.recent_file_count = n.min(100);
            }
            Action::ResetPreferences => {
                self.preferences = PigmentPreferences::default();
            }
            Action::SavePreferences => {
                let path = self
                    .preferences_path
                    .clone()
                    .unwrap_or_else(PigmentPreferences::default_path);
                match self.preferences.save_to(&path) {
                    Ok(()) => self.status_message = Some("Preferences saved".into()),
                    Err(e) => {
                        log::warn!("preferences save failed: {e}");
                        self.status_message = Some(format!("Preferences save failed: {e}"));
                    }
                }
            }
            Action::LoadPreferences => {
                let path = self
                    .preferences_path
                    .clone()
                    .unwrap_or_else(PigmentPreferences::default_path);
                self.preferences = PigmentPreferences::load_from(&path);
                self.status_message = Some("Preferences loaded".into());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let p = PigmentPreferences::default();
        assert_eq!(p.performance.history_states, 50);
        assert!(p.performance.use_gpu);
        assert_eq!(p.color.working_rgb, "sRGB IEC61966-2.1");
        assert_eq!(p.interface.theme, "Dark");
        assert_eq!(p.file_handling.recent_file_count, 20);
    }

    #[test]
    fn performance_clamp_bounds() {
        let mut perf = PerformancePrefs {
            memory_usage_fraction: 5.0,
            history_states: 99_999,
            cache_levels: 99,
            cache_tile_mib: 1,
            ..PerformancePrefs::default()
        };
        perf.clamp();
        assert_eq!(perf.memory_usage_fraction, 0.90);
        assert_eq!(perf.history_states, 1000);
        assert_eq!(perf.cache_levels, 8);
        assert_eq!(perf.cache_tile_mib, 16);
    }

    #[test]
    fn json_round_trips() {
        let mut p = PigmentPreferences::default();
        p.interface.theme = "Light".into();
        p.performance.history_states = 123;
        let json = p.to_json();
        let back = PigmentPreferences::from_json(&json);
        assert_eq!(back, p);
    }

    #[test]
    fn partial_json_uses_defaults() {
        // Only the interface theme is present; everything else defaults.
        let json = r#"{ "interface": { "theme": "Medium Light" } }"#;
        let p = PigmentPreferences::from_json(json);
        assert_eq!(p.interface.theme, "Medium Light");
        // ui_scale absent → default; whole pane reconstructed from #[serde(default)].
        assert_eq!(p.interface.ui_scale, 1.0);
        assert_eq!(p.performance.history_states, 50);
    }

    #[test]
    fn corrupt_json_falls_back() {
        let p = PigmentPreferences::from_json("not valid json {{{");
        assert_eq!(p, PigmentPreferences::default());
    }

    #[test]
    fn save_and_load_from_disk() {
        let dir = std::env::temp_dir().join(format!("pigment-prefs-test-{}", std::process::id()));
        let path = dir.join("prefs.json");
        let _ = std::fs::remove_file(&path);
        let mut p = PigmentPreferences::default();
        p.color.rendering_intent = "Perceptual".into();
        p.file_handling.autosave_minutes = 5;
        p.save_to(&path).expect("save");
        let back = PigmentPreferences::load_from(&path);
        assert_eq!(back, p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_is_default() {
        let path = std::path::Path::new("/nonexistent/dir/should/not/exist/prefs.json");
        assert_eq!(PigmentPreferences::load_from(path), PigmentPreferences::default());
    }

    #[test]
    fn action_set_theme_and_scale() {
        let mut app = App::new();
        app.apply(Action::SetPrefTheme("Light".into()));
        assert_eq!(app.preferences.interface.theme, "Light");
        app.apply(Action::SetPrefUiScale(10.0));
        assert_eq!(app.preferences.interface.ui_scale, 4.0); // clamped
    }

    #[test]
    fn action_history_states_clamped() {
        let mut app = App::new();
        app.apply(Action::SetPrefHistoryStates(5000));
        assert_eq!(app.preferences.performance.history_states, 1000);
    }

    #[test]
    fn action_reset_restores_defaults() {
        let mut app = App::new();
        app.apply(Action::SetPrefTheme("Light".into()));
        app.apply(Action::SetPrefUseGpu(false));
        app.apply(Action::ResetPreferences);
        assert_eq!(app.preferences, PigmentPreferences::default());
    }

    #[test]
    fn action_save_and_load_via_path() {
        let dir = std::env::temp_dir().join(format!("pigment-prefs-act-{}", std::process::id()));
        let path = dir.join("p.json");
        let _ = std::fs::remove_file(&path);
        let mut app = App::new();
        app.preferences_path = Some(path.clone());
        app.apply(Action::SetPrefRenderingIntent("Saturation".into()));
        app.apply(Action::SavePreferences);
        // Mutate in memory, then load back from disk to prove persistence.
        app.apply(Action::SetPrefRenderingIntent("Perceptual".into()));
        app.apply(Action::LoadPreferences);
        assert_eq!(app.preferences.color.rendering_intent, "Saturation");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
