use super::{App, Action};

/// Supported export formats.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportFormat {
    Mp4,
    Gif,
    WebM,
    LottieJson,
    Apng,
    Spritesheet,
    Png,
}

/// Configuration for a single export job.
#[derive(Clone, Debug)]
pub struct ExportConfig {
    pub format: ExportFormat,
    pub fps: f32,
    pub scale: f32,
    pub quality: u8,
    pub transparent: bool,
    pub start_frame: usize,
    pub end_frame: usize,
    pub output_path: String,
}

impl ExportConfig {
    pub fn new() -> Self {
        Self {
            format: ExportFormat::Mp4,
            fps: 24.0,
            scale: 1.0,
            quality: 85,
            transparent: false,
            start_frame: 0,
            end_frame: 0,
            output_path: String::new(),
        }
    }
}

impl App {
    pub fn apply_export(&mut self, action: Action) {
        match action {
            Action::SetExportFormat(fmt) => {
                self.export_config.format = fmt;
            }
            Action::SetExportFps(fps) => {
                self.export_config.fps = fps;
            }
            Action::SetExportScale(scale) => {
                self.export_config.scale = scale.clamp(0.1, 4.0);
            }
            Action::SetExportQuality(q) => {
                self.export_config.quality = q.min(100);
            }
            Action::SetExportTransparent(t) => {
                self.export_config.transparent = t;
            }
            Action::SetExportStartFrame(f) => {
                self.export_config.start_frame = f;
            }
            Action::SetExportEndFrame(f) => {
                self.export_config.end_frame = f;
            }
            Action::SetExportPath(path) => {
                self.export_config.output_path = path;
            }
            Action::StartExport => {
                self.export_in_progress = true;
            }
            Action::CancelExport => {
                self.export_in_progress = false;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::ExportFormat;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_set_export_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::Gif));
        assert_eq!(a.export_config.format, ExportFormat::Gif);
    }

    #[test]
    fn test_set_export_fps() {
        let mut a = app();
        a.apply(Action::SetExportFps(60.0));
        assert_eq!(a.export_config.fps, 60.0);
    }

    #[test]
    fn test_set_export_scale_clamp() {
        let mut a = app();
        a.apply(Action::SetExportScale(10.0));
        assert_eq!(a.export_config.scale, 4.0);
        a.apply(Action::SetExportScale(0.0));
        assert_eq!(a.export_config.scale, 0.1);
    }

    #[test]
    fn test_set_export_quality_clamp() {
        let mut a = app();
        a.apply(Action::SetExportQuality(200));
        assert_eq!(a.export_config.quality, 100);
    }

    #[test]
    fn test_set_export_transparent() {
        let mut a = app();
        a.apply(Action::SetExportTransparent(true));
        assert!(a.export_config.transparent);
    }

    #[test]
    fn test_set_export_path() {
        let mut a = app();
        a.apply(Action::SetExportPath("/output/animation.mp4".to_string()));
        assert_eq!(a.export_config.output_path, "/output/animation.mp4");
    }

    #[test]
    fn test_start_cancel_export() {
        let mut a = app();
        a.apply(Action::StartExport);
        assert!(a.export_in_progress);
        a.apply(Action::CancelExport);
        assert!(!a.export_in_progress);
    }

    #[test]
    fn test_set_export_start_end_frame() {
        let mut a = app();
        a.apply(Action::SetExportStartFrame(5));
        a.apply(Action::SetExportEndFrame(120));
        assert_eq!(a.export_config.start_frame, 5);
        assert_eq!(a.export_config.end_frame, 120);
    }

    #[test]
    fn test_export_lottie_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::LottieJson));
        assert_eq!(a.export_config.format, ExportFormat::LottieJson);
    }

    #[test]
    fn test_export_webm_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::WebM));
        assert_eq!(a.export_config.format, ExportFormat::WebM);
    }

    #[test]
    fn test_export_spritesheet_format() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::Spritesheet));
        assert_eq!(a.export_config.format, ExportFormat::Spritesheet);
    }

    #[test]
    fn test_export_default_config() {
        let a = app();
        assert_eq!(a.export_config.format, ExportFormat::Mp4);
        assert_eq!(a.export_config.fps, 24.0);
        assert_eq!(a.export_config.scale, 1.0);
        assert_eq!(a.export_config.quality, 85);
        assert!(!a.export_config.transparent);
    }
}
