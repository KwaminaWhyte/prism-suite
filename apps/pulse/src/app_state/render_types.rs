//! Render-queue / export **status + config types** extracted from
//! `app_state/render.rs` (workspace size rule): the queue/job status enums
//! ([`RenderJobStatus`], [`RenderStatus`], [`PreRenderStatus`]), the queue/job
//! items ([`RenderJob`], [`RenderQueueItem`]), the output-format enums
//! ([`RenderFormat`], [`RenderOutputFormat`]), the audio-spectrum visualiser
//! config, the Brainstorm panel state, and the precompose [`SubComp`] / output
//! preset. Plain data — the `apply_render` dispatcher in `render.rs` and the
//! panels consume these exactly as before; re-exported from `render` so existing
//! `crate::app_state::{…}` paths keep resolving.

use super::*;

/// Status of a job in the render queue.
#[derive(Clone, Debug)]
pub enum RenderJobStatus {
    Pending,
    Rendering(f32),
    Done,
    Failed(String),
}

/// Output format for a render queue item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderFormat {
    /// H.264 MP4 — broadly compatible, smallest file (default).
    Mp4H264,
    /// H.265/HEVC MP4 — better quality/size ratio; requires hardware support.
    Mp4H265,
    /// Apple ProRes 422 Proxy — lossless-ish, large; requires FFmpeg.
    ProResProxy,
    /// Animated GIF — legacy web format, palette-quantized.
    Gif,
}

impl RenderFormat {
    pub fn label(self) -> &'static str {
        match self {
            RenderFormat::Mp4H264 => "H.264 MP4",
            RenderFormat::Mp4H265 => "H.265 MP4",
            RenderFormat::ProResProxy => "ProRes Proxy",
            RenderFormat::Gif => "Animated GIF",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            RenderFormat::Mp4H264 | RenderFormat::Mp4H265 => "mp4",
            RenderFormat::ProResProxy => "mov",
            RenderFormat::Gif => "gif",
        }
    }

    /// All supported render formats, for UI pickers.
    pub const ALL: [RenderFormat; 4] = [
        RenderFormat::Mp4H264,
        RenderFormat::Mp4H265,
        RenderFormat::ProResProxy,
        RenderFormat::Gif,
    ];
}

/// One export job in the render queue.
#[derive(Clone, Debug)]
pub struct RenderJob {
    pub comp_name: String,
    pub output_path: PathBuf,
    pub format: RenderFormat,
    pub status: RenderJobStatus,
}

/// A saved output module preset (Wave 14).
#[derive(Clone, Debug)]
pub struct OutputPreset {
    pub name: String,
    pub format: export::OutputFormat,
}

/// A captured group of layers extracted by pre-compose (Wave 11).
#[derive(Clone, Debug)]
pub struct SubComp {
    pub name: String,
    pub layers: Vec<crate::comp::PulseLayer>,
}


/// One randomised keyframe-variation preview in the Brainstorm panel.
#[derive(Clone, Debug)]
pub struct BrainstormVariation {
    pub label: String,
    pub overrides: Vec<(usize, crate::comp::Prop, f32)>,
    pub selected: bool,
}

/// State for the Brainstorm panel (generate + pick random comp variations).
#[derive(Clone, Debug, Default)]
pub struct BrainstormState {
    pub open: bool,
    pub variations: Vec<BrainstormVariation>,
    pub grid_cols: u32,
    pub grid_rows: u32,
}

impl BrainstormState {
    pub fn new() -> Self {
        Self { open: false, variations: Vec::new(), grid_cols: 2, grid_rows: 3 }
    }
}

/// Status of the pre-render cache.
#[derive(Clone, Debug, PartialEq)]
pub enum PreRenderStatus {
    NotStarted,
    Rendering { frames_done: u32, total: u32 },
    Done { frame_count: u32, cache_dir: std::path::PathBuf },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum AudioVisMode {
    #[default]
    Spectrum,
    Waveform,
    Bars,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum AudioVisSide {
    #[default]
    Both,
    Left,
    Right,
    All,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioSpectrumConfig {
    pub mode: AudioVisMode,
    pub audio_layer: Option<usize>,
    pub start_freq: f32,
    pub end_freq: f32,
    pub max_height: f32,
    pub audio_duration: f32,
    pub side: AudioVisSide,
    pub softness: f32,
    pub inside_color: [f32; 4],
    pub outside_color: [f32; 4],
    pub mirror: bool,
    pub displayed_samples: u32,
    pub digital: bool,
    pub frequency_bands: u32,
    pub thickness: f32,
}

impl Default for AudioSpectrumConfig {
    fn default() -> Self {
        Self {
            mode: AudioVisMode::Spectrum,
            audio_layer: None,
            start_freq: 20.0,
            end_freq: 20000.0,
            max_height: 500.0,
            audio_duration: 0.0,
            side: AudioVisSide::Both,
            softness: 0.0,
            inside_color: [1.0, 1.0, 1.0, 1.0],
            outside_color: [0.0, 0.0, 0.0, 0.0],
            mirror: false,
            displayed_samples: 512,
            digital: false,
            frequency_bands: 64,
            thickness: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum RenderStatus {
    #[default]
    Queued,
    Rendering,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum RenderOutputFormat {
    #[default]
    H264Mp4,
    ProResHq,
    DnxHd,
    Exr,
    Tiff,
    Png,
    Wav,
    Aiff,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderQueueItem {
    pub comp_name: String,
    pub output_path: std::path::PathBuf,
    pub format: RenderOutputFormat,
    pub status: RenderStatus,
    pub progress: f32,
    pub start_frame: u32,
    pub end_frame: u32,
    pub use_proxy: bool,
}

impl Default for RenderQueueItem {
    fn default() -> Self {
        Self {
            comp_name: "Comp 1".to_string(),
            output_path: std::path::PathBuf::from("output.mp4"),
            format: RenderOutputFormat::H264Mp4,
            status: RenderStatus::Queued,
            progress: 0.0,
            start_frame: 0,
            end_frame: 100,
            use_proxy: false,
        }
    }
}
