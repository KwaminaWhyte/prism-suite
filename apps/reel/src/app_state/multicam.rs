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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn test_add_remove_multicam_angle() {
        let mut app = App::new();
        assert_eq!(app.multicam_angles.len(), 0);
        app.apply(Action::AddMulticamAngle(MulticamAngle { label: "Cam A".to_string(), source_clip_idx: 0, sync_offset: 0.0, enabled: true }));
        app.apply(Action::AddMulticamAngle(MulticamAngle { label: "Cam B".to_string(), source_clip_idx: 1, sync_offset: 0.5, enabled: true }));
        assert_eq!(app.multicam_angles.len(), 2);
        app.apply(Action::RemoveMulticamAngle(0));
        assert_eq!(app.multicam_angles.len(), 1);
        assert_eq!(app.multicam_angles[0].label, "Cam B");
        // Out-of-bounds removal is a no-op.
        app.apply(Action::RemoveMulticamAngle(99));
        assert_eq!(app.multicam_angles.len(), 1);
    }

    #[test]
    fn test_toggle_multicam_angle() {
        let mut app = App::new();
        app.apply(Action::AddMulticamAngle(MulticamAngle::default()));
        assert!(app.multicam_angles[0].enabled);
        app.apply(Action::ToggleMulticamAngle(0));
        assert!(!app.multicam_angles[0].enabled);
        app.apply(Action::ToggleMulticamAngle(0));
        assert!(app.multicam_angles[0].enabled);
    }

    #[test]
    fn test_sync_mode_set() {
        let mut app = App::new();
        assert_eq!(app.multicam_sync_mode, MulticamSyncMode::Timecode);
        app.apply(Action::SetMulticamSyncMode(MulticamSyncMode::Waveform));
        assert_eq!(app.multicam_sync_mode, MulticamSyncMode::Waveform);
        app.apply(Action::SetMulticamDisplayMode(MulticamDisplayMode::Solo));
        assert_eq!(app.multicam_display_mode, MulticamDisplayMode::Solo);
    }

    #[test]
    fn test_flatten_multicam_clears() {
        let mut app = App::new();
        app.apply(Action::AddMulticamAngle(MulticamAngle::default()));
        app.apply(Action::AddMulticamAngle(MulticamAngle::default()));
        assert_eq!(app.multicam_angles.len(), 2);
        app.multicam_active_angle = 1;
        app.apply(Action::FlattenMulticam);
        assert_eq!(app.multicam_angles.len(), 0);
        assert_eq!(app.multicam_active_angle, 0);
    }

    #[test]
    fn test_edl_frame_rate_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEdlFrameRate(200.0));
        assert!((app.edl_config.frame_rate - 120.0).abs() < 1e-5);
        app.apply(Action::SetEdlFrameRate(0.0));
        assert!((app.edl_config.frame_rate - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_edl_format_set() {
        let mut app = App::new();
        assert_eq!(app.edl_config.format, EdlFormat::Cmx3600);
        app.apply(Action::SetEdlFormat(EdlFormat::FcpXml));
        assert_eq!(app.edl_config.format, EdlFormat::FcpXml);
        app.apply(Action::SetEdlFormat(EdlFormat::Otio));
        assert_eq!(app.edl_config.format, EdlFormat::Otio);
    }

    #[test]
    fn test_export_edl_records_path() {
        let mut app = App::new();
        assert!(app.last_edl_export_path.is_none());
        let p = std::path::PathBuf::from("/tmp/export.edl");
        app.apply(Action::ExportEdl(p.clone()));
        assert_eq!(app.last_edl_export_path, Some(p.clone()));
        let p2 = std::path::PathBuf::from("/tmp/export.xml");
        app.apply(Action::ExportFcpXml(p2.clone()));
        assert_eq!(app.last_edl_export_path, Some(p2));
        let p3 = std::path::PathBuf::from("/tmp/export.otio");
        app.apply(Action::ExportOtio(p3.clone()));
        assert_eq!(app.last_edl_export_path, Some(p3));
    }

    #[test]
    fn test_reframe_aspect_min_1() {
        let mut app = App::new();
        app.apply(Action::SetReframeAspect { w: 0, h: 0 });
        assert_eq!(app.auto_reframe_config.target_aspect_w, 1);
        assert_eq!(app.auto_reframe_config.target_aspect_h, 1);
    }

    #[test]
    fn test_analyze_reframe_adds_result() {
        let mut app = App::new();
        assert_eq!(app.reframe_results.len(), 0);
        app.apply(Action::AnalyzeReframe { clip_idx: 0 });
        assert_eq!(app.reframe_results.len(), 1);
        assert_eq!(app.reframe_results[0].0, 0);
        assert_eq!(app.reframe_results[0].1, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_clear_reframe() {
        let mut app = App::new();
        app.apply(Action::AnalyzeReframe { clip_idx: 0 });
        app.apply(Action::AnalyzeReframe { clip_idx: 1 });
        assert_eq!(app.reframe_results.len(), 2);
        app.apply(Action::ClearReframeResults);
        assert_eq!(app.reframe_results.len(), 0);
    }

    #[test]
    fn test_reframe_motion_set() {
        let mut app = App::new();
        assert_eq!(app.auto_reframe_config.motion_preset, ReframeMotion::Default);
        app.apply(Action::SetReframeMotion(ReframeMotion::SmoothFast));
        assert_eq!(app.auto_reframe_config.motion_preset, ReframeMotion::SmoothFast);
        app.apply(Action::SetReframeMotion(ReframeMotion::Slower));
        assert_eq!(app.auto_reframe_config.motion_preset, ReframeMotion::Slower);
    }
}
