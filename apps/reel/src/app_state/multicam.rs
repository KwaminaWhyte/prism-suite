use super::{App, Action};

/// A single multicam angle (Batch 8).
#[derive(Debug, Clone)]
pub struct MulticamAngle {
    pub label: String,
    pub source_clip_idx: usize,
    pub sync_offset: f32,
    pub enabled: bool,
}

impl Default for MulticamAngle {
    fn default() -> Self {
        Self { label: "Angle".to_string(), source_clip_idx: 0, sync_offset: 0.0, enabled: true }
    }
}

/// How multicam clips are synchronized.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MulticamSyncMode {
    #[default] Timecode, Waveform, InPoint, Manual,
}

/// How the multicam viewer is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MulticamDisplayMode {
    #[default] Grid, Solo, PiP,
}

/// EDL interchange format.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EdlFormat {
    #[default] Cmx3600, FcpXml, Aaf, Otio,
}

/// EDL export configuration.
#[derive(Debug, Clone)]
pub struct EdlConfig {
    pub format: EdlFormat,
    pub frame_rate: f32,
    pub reel_name: String,
    pub include_audio: bool,
    pub include_video: bool,
}

impl Default for EdlConfig {
    fn default() -> Self {
        Self {
            format: EdlFormat::Cmx3600,
            frame_rate: 24.0,
            reel_name: "REEL001".to_string(),
            include_audio: true,
            include_video: true,
        }
    }
}

/// Auto-reframe motion preset.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ReframeMotion {
    #[default] Default, Slower, Faster, SmoothFast, SmoothSlow,
}

/// Auto-reframe configuration.
#[derive(Debug, Clone)]
pub struct AutoReframeConfig {
    pub target_aspect_w: u32,
    pub target_aspect_h: u32,
    pub motion_preset: ReframeMotion,
    pub keep_scale: bool,
    pub analyze_on_import: bool,
}

impl Default for AutoReframeConfig {
    fn default() -> Self {
        Self {
            target_aspect_w: 9,
            target_aspect_h: 16,
            motion_preset: ReframeMotion::Default,
            keep_scale: true,
            analyze_on_import: false,
        }
    }
}

pub trait AppMulticamExt {
    fn apply_multicam(&mut self, action: Action);
}

impl AppMulticamExt for App {
    fn apply_multicam(&mut self, action: Action) {
        match action {
            Action::AddMulticamAngle(a) => { self.multicam_angles.push(a); }
            Action::RemoveMulticamAngle(i) => {
                if i < self.multicam_angles.len() { self.multicam_angles.remove(i); }
            }
            Action::SetMulticamAngleLabel { idx, label } => {
                if let Some(a) = self.multicam_angles.get_mut(idx) { a.label = label; }
            }
            Action::SetMulticamAngleSyncOffset { idx, offset } => {
                if let Some(a) = self.multicam_angles.get_mut(idx) { a.sync_offset = offset; }
            }
            Action::ToggleMulticamAngle(i) => {
                if let Some(a) = self.multicam_angles.get_mut(i) { a.enabled = !a.enabled; }
            }
            Action::SetMulticamSyncMode(m) => { self.multicam_sync_mode = m; }
            Action::SetMulticamDisplayMode(m) => { self.multicam_display_mode = m; }
            Action::FlattenMulticam => {
                self.multicam_angles.clear();
                self.multicam_active_angle = 0;
            }
            Action::SetEdlFormat(f) => { self.edl_config.format = f; }
            Action::SetEdlFrameRate(r) => { self.edl_config.frame_rate = r.clamp(1.0, 120.0); }
            Action::SetEdlReelName(n) => { self.edl_config.reel_name = n; }
            Action::SetEdlIncludeAudio(b) => { self.edl_config.include_audio = b; }
            Action::SetEdlIncludeVideo(b) => { self.edl_config.include_video = b; }
            Action::ExportEdl(p) => { self.last_edl_export_path = Some(p); }
            Action::ImportEdl(_p) => { self.last_import_clip_count = 0; }
            Action::ExportFcpXml(p) => { self.last_edl_export_path = Some(p); }
            Action::ImportFcpXml(_p) => { self.last_import_clip_count = 0; }
            Action::ExportOtio(p) => { self.last_edl_export_path = Some(p); }
            Action::ToggleAutoReframePanel => { self.auto_reframe_panel_open = !self.auto_reframe_panel_open; }
            Action::SetReframeAspect { w, h } => {
                self.auto_reframe_config.target_aspect_w = w.max(1);
                self.auto_reframe_config.target_aspect_h = h.max(1);
            }
            Action::SetReframeMotion(m) => { self.auto_reframe_config.motion_preset = m; }
            Action::SetReframeKeepScale(b) => { self.auto_reframe_config.keep_scale = b; }
            Action::SetReframeAnalyzeOnImport(b) => { self.auto_reframe_config.analyze_on_import = b; }
            Action::AnalyzeReframe { clip_idx } => {
                self.reframe_results.push((clip_idx, vec![0.0, 0.5, 1.0]));
            }
            Action::ApplyReframe { clip_idx } => {
                let _result = self.reframe_results.iter().find(|(ci, _)| *ci == clip_idx);
            }
            Action::ClearReframeResults => { self.reframe_results.clear(); }
            _ => {}
        }
    }
}
