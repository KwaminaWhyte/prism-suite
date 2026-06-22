use super::{App, Action};

/// Video codec options for MP4 export.
#[derive(Clone, Debug, PartialEq)]
pub enum VideoCodecKind {
    H264,
    H265,
    Vp9,
    Av1,
}

/// Image format for spritesheet export.
#[derive(Clone, Debug, PartialEq)]
pub enum SpriteFormat {
    Png,
    Jpg,
    Webp,
}

/// All supported publish targets.
#[derive(Clone, Debug, PartialEq)]
pub enum PublishTarget {
    Html5Canvas,
    WebGl,
    Svg,
    AnimatedGif { colors: u16, dither: bool },
    VideoMp4 { codec: VideoCodecKind, bitrate_kbps: u32 },
    SpriteSheet { columns: u32, padding: u32, format: SpriteFormat },
    ApngSequence,
    JsonAnimation,
}

/// Configuration for a publish / export operation.
#[derive(Clone, Debug)]
pub struct PublishConfig {
    pub target: PublishTarget,
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
    pub loop_playback: bool,
    /// Quality 1–100 for lossy formats.
    pub quality: u8,
    pub include_hidden: bool,
    pub flatten_to_single_layer: bool,
    /// RGBA background color.
    pub background_color: [u8; 4],
    pub transparent_background: bool,
}

impl PublishConfig {
    pub fn new() -> Self {
        Self {
            target: PublishTarget::Html5Canvas,
            output_path: String::new(),
            width: 1920,
            height: 1080,
            frame_rate: 24.0,
            loop_playback: false,
            quality: 85,
            include_hidden: false,
            flatten_to_single_layer: false,
            background_color: [0, 0, 0, 255],
            transparent_background: false,
        }
    }
}

/// Status of a queued export job.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportStatus {
    Queued,
    Running,
    Done,
    Failed(String),
}

/// A single export job in the render queue.
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub id: usize,
    pub name: String,
    pub config: PublishConfig,
    pub status: ExportStatus,
    /// Progress 0.0..=1.0.
    pub progress: f32,
}

/// The export / render queue.
#[derive(Clone, Debug)]
pub struct ExportQueue {
    pub jobs: Vec<ExportJob>,
    pub next_job_id: usize,
}

impl ExportQueue {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            next_job_id: 0,
        }
    }
}

impl App {
    pub fn apply_publish(&mut self, action: Action) {
        match action {
            Action::SetPublishTarget(target) => {
                self.publish_config.target = target;
            }
            Action::SetPublishOutputPath(path) => {
                self.publish_config.output_path = path;
            }
            Action::SetPublishDimensions { width, height } => {
                self.publish_config.width = width.max(1);
                self.publish_config.height = height.max(1);
            }
            Action::SetPublishFrameRate(fps) => {
                self.publish_config.frame_rate = fps.clamp(1.0, 120.0);
            }
            Action::SetPublishQuality(q) => {
                self.publish_config.quality = q.clamp(1, 100);
            }
            Action::SetPublishLoop(v) => {
                self.publish_config.loop_playback = v;
            }
            Action::SetPublishTransparentBg(v) => {
                self.publish_config.transparent_background = v;
            }
            Action::AddToExportQueue { name, config } => {
                let id = self.export_queue.next_job_id;
                self.export_queue.next_job_id += 1;
                self.export_queue.jobs.push(ExportJob {
                    id,
                    name,
                    config,
                    status: ExportStatus::Queued,
                    progress: 0.0,
                });
            }
            Action::RemoveFromExportQueue { job_id } => {
                self.export_queue.jobs.retain(|j| j.id != job_id);
            }
            Action::ClearExportQueue => {
                self.export_queue.jobs.clear();
            }
            Action::SetExportJobStatus { job_id, status } => {
                if let Some(job) = self.export_queue.jobs.iter_mut().find(|j| j.id == job_id) {
                    job.status = status;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{PublishConfig, PublishTarget, VideoCodecKind, SpriteFormat, ExportStatus};

    fn app() -> App {
        App::new()
    }

    fn default_config() -> PublishConfig {
        PublishConfig::new()
    }

    #[test]
    fn test_publish_config_defaults() {
        let a = app();
        assert_eq!(a.publish_config.target, PublishTarget::Html5Canvas);
        assert_eq!(a.publish_config.width, 1920);
        assert_eq!(a.publish_config.quality, 85);
        assert!(!a.publish_config.transparent_background);
    }

    #[test]
    fn test_set_publish_target_webgl() {
        let mut a = app();
        a.apply(Action::SetPublishTarget(PublishTarget::WebGl));
        assert_eq!(a.publish_config.target, PublishTarget::WebGl);
    }

    #[test]
    fn test_set_publish_target_gif() {
        let mut a = app();
        a.apply(Action::SetPublishTarget(PublishTarget::AnimatedGif { colors: 256, dither: true }));
        assert_eq!(a.publish_config.target, PublishTarget::AnimatedGif { colors: 256, dither: true });
    }

    #[test]
    fn test_set_publish_target_mp4() {
        let mut a = app();
        a.apply(Action::SetPublishTarget(PublishTarget::VideoMp4 { codec: VideoCodecKind::H264, bitrate_kbps: 8000 }));
        assert_eq!(a.publish_config.target, PublishTarget::VideoMp4 { codec: VideoCodecKind::H264, bitrate_kbps: 8000 });
    }

    #[test]
    fn test_set_publish_output_path() {
        let mut a = app();
        a.apply(Action::SetPublishOutputPath("/out/anim.html".to_string()));
        assert_eq!(a.publish_config.output_path, "/out/anim.html");
    }

    #[test]
    fn test_set_publish_dimensions() {
        let mut a = app();
        a.apply(Action::SetPublishDimensions { width: 800, height: 600 });
        assert_eq!(a.publish_config.width, 800);
        assert_eq!(a.publish_config.height, 600);
    }

    #[test]
    fn test_set_publish_dimensions_min_one() {
        let mut a = app();
        a.apply(Action::SetPublishDimensions { width: 0, height: 0 });
        assert_eq!(a.publish_config.width, 1);
        assert_eq!(a.publish_config.height, 1);
    }

    #[test]
    fn test_set_publish_frame_rate_clamped() {
        let mut a = app();
        a.apply(Action::SetPublishFrameRate(999.0));
        assert_eq!(a.publish_config.frame_rate, 120.0);
        a.apply(Action::SetPublishFrameRate(0.0));
        assert_eq!(a.publish_config.frame_rate, 1.0);
    }

    #[test]
    fn test_set_publish_quality_clamped() {
        let mut a = app();
        a.apply(Action::SetPublishQuality(200));
        assert_eq!(a.publish_config.quality, 100);
        a.apply(Action::SetPublishQuality(0));
        assert_eq!(a.publish_config.quality, 1);
    }

    #[test]
    fn test_set_publish_loop() {
        let mut a = app();
        a.apply(Action::SetPublishLoop(true));
        assert!(a.publish_config.loop_playback);
    }

    #[test]
    fn test_set_publish_transparent_bg() {
        let mut a = app();
        a.apply(Action::SetPublishTransparentBg(true));
        assert!(a.publish_config.transparent_background);
    }

    #[test]
    fn test_add_to_export_queue() {
        let mut a = app();
        a.apply(Action::AddToExportQueue { name: "Main export".to_string(), config: default_config() });
        assert_eq!(a.export_queue.jobs.len(), 1);
        assert_eq!(a.export_queue.jobs[0].name, "Main export");
        assert_eq!(a.export_queue.jobs[0].status, ExportStatus::Queued);
        assert_eq!(a.export_queue.jobs[0].progress, 0.0);
    }

    #[test]
    fn test_add_multiple_export_jobs_unique_ids() {
        let mut a = app();
        a.apply(Action::AddToExportQueue { name: "A".to_string(), config: default_config() });
        a.apply(Action::AddToExportQueue { name: "B".to_string(), config: default_config() });
        assert_ne!(a.export_queue.jobs[0].id, a.export_queue.jobs[1].id);
    }

    #[test]
    fn test_remove_from_export_queue() {
        let mut a = app();
        a.apply(Action::AddToExportQueue { name: "Job".to_string(), config: default_config() });
        let job_id = a.export_queue.jobs[0].id;
        a.apply(Action::RemoveFromExportQueue { job_id });
        assert!(a.export_queue.jobs.is_empty());
    }

    #[test]
    fn test_clear_export_queue() {
        let mut a = app();
        a.apply(Action::AddToExportQueue { name: "A".to_string(), config: default_config() });
        a.apply(Action::AddToExportQueue { name: "B".to_string(), config: default_config() });
        a.apply(Action::ClearExportQueue);
        assert!(a.export_queue.jobs.is_empty());
    }

    #[test]
    fn test_set_export_job_status() {
        let mut a = app();
        a.apply(Action::AddToExportQueue { name: "Job".to_string(), config: default_config() });
        let job_id = a.export_queue.jobs[0].id;
        a.apply(Action::SetExportJobStatus { job_id, status: ExportStatus::Running });
        assert_eq!(a.export_queue.jobs[0].status, ExportStatus::Running);
        a.apply(Action::SetExportJobStatus { job_id, status: ExportStatus::Done });
        assert_eq!(a.export_queue.jobs[0].status, ExportStatus::Done);
    }

    #[test]
    fn test_set_export_job_status_failed() {
        let mut a = app();
        a.apply(Action::AddToExportQueue { name: "Job".to_string(), config: default_config() });
        let job_id = a.export_queue.jobs[0].id;
        a.apply(Action::SetExportJobStatus { job_id, status: ExportStatus::Failed("out of disk".to_string()) });
        assert!(matches!(a.export_queue.jobs[0].status, ExportStatus::Failed(_)));
    }

    #[test]
    fn test_spritesheet_publish_target() {
        let mut a = app();
        a.apply(Action::SetPublishTarget(PublishTarget::SpriteSheet { columns: 8, padding: 2, format: SpriteFormat::Png }));
        assert_eq!(a.publish_config.target, PublishTarget::SpriteSheet { columns: 8, padding: 2, format: SpriteFormat::Png });
    }

    #[test]
    fn test_json_animation_target() {
        let mut a = app();
        a.apply(Action::SetPublishTarget(PublishTarget::JsonAnimation));
        assert_eq!(a.publish_config.target, PublishTarget::JsonAnimation);
    }
}
