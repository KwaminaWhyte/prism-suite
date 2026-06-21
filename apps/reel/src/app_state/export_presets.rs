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
