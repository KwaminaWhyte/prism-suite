use super::{App, Action};

/// Export format (original model).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat { H264Mp4, ProResProxy, ProRes422, Gif }

impl Default for ExportFormat { fn default() -> Self { ExportFormat::H264Mp4 } }

/// A named export preset (original model).
#[derive(Clone, Debug)]
pub struct ExportPreset {
    pub name: String,
    pub format: ExportFormat,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bitrate_kbps: u32,
}

/// Export format (Batch 10 model).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ExportFormatB10 {
    #[default] H264, H265, ProRes, DnxHd, Av1, Mp3, Aac, Wav,
}

/// Export resolution (Batch 10 model).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ExportResolution {
    FourK,
    #[default] FullHd1080, Hd720, Sd480,
}

/// Export preset (Batch 10 model).
#[derive(Debug, Clone)]
pub struct ExportPresetB10 {
    pub name: String,
    pub format: ExportFormatB10,
    pub resolution: ExportResolution,
    pub frame_rate: f32,
    pub bitrate_mbps: f32,
    pub two_pass: bool,
    pub audio_bitrate_kbps: u32,
    pub use_max_render_quality: bool,
    pub use_frame_blend: bool,
    pub export_video: bool,
    pub export_audio: bool,
}

impl Default for ExportPresetB10 {
    fn default() -> Self {
        Self {
            name: String::new(),
            format: ExportFormatB10::default(),
            resolution: ExportResolution::default(),
            frame_rate: 29.97,
            bitrate_mbps: 20.0,
            two_pass: false,
            audio_bitrate_kbps: 320,
            use_max_render_quality: false,
            use_frame_blend: false,
            export_video: true,
            export_audio: true,
        }
    }
}

impl ExportPresetB10 {
    pub fn h264_1080p() -> Self {
        Self {
            name: "H.264 1080p".to_string(),
            format: ExportFormatB10::H264,
            resolution: ExportResolution::FullHd1080,
            frame_rate: 29.97,
            bitrate_mbps: 20.0,
            audio_bitrate_kbps: 320,
            export_video: true,
            export_audio: true,
            ..Default::default()
        }
    }
}

pub trait AppExportPresetsExt {
    fn apply_export_presets(&mut self, action: Action);
}

impl AppExportPresetsExt for App {
    fn apply_export_presets(&mut self, action: Action) {
        match action {
            Action::AddExportPreset(preset) => { self.export_preset_list.push(preset); }
            Action::DeleteExportPresetNew(idx) => {
                if idx < self.export_preset_list.len() { self.export_preset_list.remove(idx); }
            }
            Action::ExportWithPreset(idx) => {
                if let Some(preset) = self.export_preset_list.get(idx).cloned() {
                    self.project.width = preset.width;
                    self.project.height = preset.height;
                    self.project.fps = preset.fps as f32;
                    self.export_format = preset.format;
                    self.host.mark_dirty();
                }
            }
            Action::ToggleExportPresets => { self.show_export_presets = !self.show_export_presets; }
            Action::ExportProRes(_path) | Action::ExportDNxHD(_path) => {
                log::info!("reel-gpui: ProRes/DNxHD export requested (stub — wire to start_export_codec)");
            }
            Action::ExportProResProxy(path) => { log::info!("reel-gpui: ProRes Proxy export: {}", path.display()); }
            Action::ExportProRes422(path) => { log::info!("reel-gpui: ProRes 422 export: {}", path.display()); }
            Action::ExportGif(path) => { log::info!("reel-gpui: GIF export: {}", path.display()); }
            Action::SetExportFormat(fmt) => { self.export_format = fmt; }
            Action::ToggleExportPanel => { self.export_panel_open = !self.export_panel_open; }
            Action::AddExportPresetB10(p) => { self.export_presets_b10.push(p); }
            Action::RemoveExportPresetB10(idx) => {
                if self.export_presets_b10.len() > 1 && idx < self.export_presets_b10.len() {
                    self.export_presets_b10.remove(idx);
                    if self.active_export_preset >= self.export_presets_b10.len() {
                        self.active_export_preset = self.export_presets_b10.len() - 1;
                    }
                }
            }
            Action::SetActiveExportPreset(idx) => {
                self.active_export_preset = idx.min(self.export_presets_b10.len().saturating_sub(1));
            }
            Action::SetExportFormatB10(f) => {
                if let Some(p) = self.export_presets_b10.get_mut(self.active_export_preset) { p.format = f; }
            }
            Action::SetExportResolution(r) => {
                if let Some(p) = self.export_presets_b10.get_mut(self.active_export_preset) { p.resolution = r; }
            }
            Action::SetExportFrameRate(v) => {
                if let Some(p) = self.export_presets_b10.get_mut(self.active_export_preset) {
                    p.frame_rate = v.clamp(1.0, 120.0);
                }
            }
            Action::SetExportBitrate(v) => {
                if let Some(p) = self.export_presets_b10.get_mut(self.active_export_preset) {
                    p.bitrate_mbps = v.clamp(0.1, 800.0);
                }
            }
            Action::SetExportAudioBitrate(v) => {
                if let Some(p) = self.export_presets_b10.get_mut(self.active_export_preset) {
                    p.audio_bitrate_kbps = v.clamp(64, 1536);
                }
            }
            Action::SetExportTwoPass(v) => {
                if let Some(p) = self.export_presets_b10.get_mut(self.active_export_preset) { p.two_pass = v; }
            }
            Action::SetExportPath(path) => { self.last_export_path = Some(path); }
            Action::StartExport => {
                if self.last_export_path.is_none() {
                    self.last_export_path = Some(std::path::PathBuf::from("export_output"));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn test_export_frame_rate_clamp() {
        let mut app = App::new();
        // Below minimum → clamped to 1.
        app.apply(Action::SetExportFrameRate(0.0));
        assert_eq!(app.export_presets_b10[0].frame_rate, 1.0);
        // Above maximum → clamped to 120.
        app.apply(Action::SetExportFrameRate(200.0));
        assert_eq!(app.export_presets_b10[0].frame_rate, 120.0);
    }

    #[test]
    fn test_export_bitrate_clamp() {
        let mut app = App::new();
        // Below minimum → clamped to 0.1.
        app.apply(Action::SetExportBitrate(0.0));
        assert_eq!(app.export_presets_b10[0].bitrate_mbps, 0.1);
    }

    #[test]
    fn test_export_audio_bitrate_clamp() {
        let mut app = App::new();
        // Below minimum → clamped to 64.
        app.apply(Action::SetExportAudioBitrate(10));
        assert_eq!(app.export_presets_b10[0].audio_bitrate_kbps, 64);
    }

    #[test]
    fn test_remove_last_preset_blocked() {
        let mut app = App::new();
        // Only one preset exists; removing it should be a no-op.
        assert_eq!(app.export_presets_b10.len(), 1);
        app.apply(Action::RemoveExportPresetB10(0));
        assert_eq!(app.export_presets_b10.len(), 1);
    }

    #[test]
    fn test_add_preset_and_set_active() {
        let mut app = App::new();
        app.apply(Action::AddExportPresetB10(ExportPresetB10 {
            name: "4K ProRes".to_string(),
            format: ExportFormatB10::ProRes,
            resolution: ExportResolution::FourK,
            ..Default::default()
        }));
        assert_eq!(app.export_presets_b10.len(), 2);
        app.apply(Action::SetActiveExportPreset(1));
        assert_eq!(app.active_export_preset, 1);
        // Clamp out-of-bounds index.
        app.apply(Action::SetActiveExportPreset(99));
        assert_eq!(app.active_export_preset, 1);
    }
}
