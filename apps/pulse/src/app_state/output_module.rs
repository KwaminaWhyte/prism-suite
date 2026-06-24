//! Output Module configuration for render-queue items (After Effects' *Output
//! Module Settings*).
//!
//! Each render-queue item can carry one [`OutputModule`]: the container format
//! (MP4/MOV/PNG-seq/EXR-seq), the codec, the colour **depth** (8/16/32 bpc), an
//! output resolution/scale, the frame range to render, and whether audio is
//! muxed. This is a pure data model — the actual encoding lives in `render.rs` —
//! so it stays deterministic and testable. The defaults mirror AE's "Lossless"
//! preset adapted to a sensible H.264 starting point.

use super::{App, Action};

/// The container/file format an output module writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum OutputModuleFormat {
    /// H.264/H.265 in an `.mp4` container — the default delivery format.
    #[default]
    Mp4,
    /// QuickTime `.mov` (ProRes / DNxHD-friendly).
    Mov,
    /// A `.png` image sequence (one file per frame).
    PngSequence,
    /// An OpenEXR `.exr` image sequence (linear, high-dynamic-range).
    ExrSequence,
}

impl OutputModuleFormat {
    /// Human label for the format picker.
    pub fn label(self) -> &'static str {
        match self {
            OutputModuleFormat::Mp4 => "MP4",
            OutputModuleFormat::Mov => "QuickTime MOV",
            OutputModuleFormat::PngSequence => "PNG Sequence",
            OutputModuleFormat::ExrSequence => "EXR Sequence",
        }
    }

    /// File extension (per-frame for sequences).
    pub fn extension(self) -> &'static str {
        match self {
            OutputModuleFormat::Mp4 => "mp4",
            OutputModuleFormat::Mov => "mov",
            OutputModuleFormat::PngSequence => "png",
            OutputModuleFormat::ExrSequence => "exr",
        }
    }

    /// Whether this format is an image sequence (one file per frame) rather than
    /// a single movie container.
    pub fn is_sequence(self) -> bool {
        matches!(
            self,
            OutputModuleFormat::PngSequence | OutputModuleFormat::ExrSequence
        )
    }

    /// All formats, for UI pickers.
    pub const ALL: [OutputModuleFormat; 4] = [
        OutputModuleFormat::Mp4,
        OutputModuleFormat::Mov,
        OutputModuleFormat::PngSequence,
        OutputModuleFormat::ExrSequence,
    ];
}

/// Colour bit depth (bits per channel) of the rendered output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ColorDepth {
    /// 8 bits per channel (256 levels) — standard delivery.
    #[default]
    Eight,
    /// 16 bits per channel — banding-free grading.
    Sixteen,
    /// 32-bit float per channel — linear HDR (EXR).
    ThirtyTwo,
}

impl ColorDepth {
    /// Bits per channel as an integer.
    pub fn bits(self) -> u32 {
        match self {
            ColorDepth::Eight => 8,
            ColorDepth::Sixteen => 16,
            ColorDepth::ThirtyTwo => 32,
        }
    }

    /// Label for the depth picker.
    pub fn label(self) -> &'static str {
        match self {
            ColorDepth::Eight => "8 bpc",
            ColorDepth::Sixteen => "16 bpc",
            ColorDepth::ThirtyTwo => "32 bpc (float)",
        }
    }

    pub const ALL: [ColorDepth; 3] = [ColorDepth::Eight, ColorDepth::Sixteen, ColorDepth::ThirtyTwo];
}

/// The codec used inside the container (only meaningful for movie formats).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum OutputCodec {
    #[default]
    H264,
    H265,
    ProRes422,
    ProRes4444,
    DnxHd,
    /// No codec (image sequences).
    None,
}

impl OutputCodec {
    pub fn label(self) -> &'static str {
        match self {
            OutputCodec::H264 => "H.264",
            OutputCodec::H265 => "H.265 (HEVC)",
            OutputCodec::ProRes422 => "ProRes 422",
            OutputCodec::ProRes4444 => "ProRes 4444",
            OutputCodec::DnxHd => "DNxHD",
            OutputCodec::None => "None",
        }
    }
}

/// One Output Module: everything needed to turn a comp into a deliverable file.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OutputModule {
    /// Display name (AE's named output-module templates).
    pub name: String,
    pub format: OutputModuleFormat,
    pub codec: OutputCodec,
    pub depth: ColorDepth,
    /// Output scale as a fraction of comp resolution (`1.0` = full, `0.5` = half).
    pub scale: f32,
    /// Explicit output pixel size; `None` = derive from comp size × `scale`.
    pub resolution: Option<(u32, u32)>,
    /// Inclusive first frame to render.
    pub start_frame: u32,
    /// Exclusive last frame to render (so `[start, end)`).
    pub end_frame: u32,
    /// Whether audio is muxed into the output (no-op for image sequences).
    pub audio_enabled: bool,
}

impl Default for OutputModule {
    fn default() -> Self {
        Self {
            name: "H.264 - Match Render Settings".to_string(),
            format: OutputModuleFormat::Mp4,
            codec: OutputCodec::H264,
            depth: ColorDepth::Eight,
            scale: 1.0,
            resolution: None,
            start_frame: 0,
            end_frame: 0,
            audio_enabled: true,
        }
    }
}

impl OutputModule {
    /// A new module with the given name and format, codec inferred from format.
    pub fn new(name: impl Into<String>, format: OutputModuleFormat) -> Self {
        let codec = match format {
            OutputModuleFormat::Mp4 => OutputCodec::H264,
            OutputModuleFormat::Mov => OutputCodec::ProRes422,
            OutputModuleFormat::PngSequence | OutputModuleFormat::ExrSequence => OutputCodec::None,
        };
        let depth = match format {
            OutputModuleFormat::ExrSequence => ColorDepth::ThirtyTwo,
            _ => ColorDepth::Eight,
        };
        Self {
            name: name.into(),
            format,
            codec,
            depth,
            ..Default::default()
        }
    }

    /// Resolve the actual output pixel dimensions for a comp of `comp_w × comp_h`.
    /// An explicit `resolution` wins; otherwise the comp size is multiplied by
    /// `scale` (clamped to at least 1×1).
    pub fn output_size(&self, comp_w: u32, comp_h: u32) -> (u32, u32) {
        if let Some((w, h)) = self.resolution {
            return (w.max(1), h.max(1));
        }
        let s = self.scale.clamp(0.01, 8.0);
        (
            ((comp_w as f32 * s).round() as u32).max(1),
            ((comp_h as f32 * s).round() as u32).max(1),
        )
    }

    /// Number of frames this module renders (`end - start`, clamped to ≥0).
    pub fn frame_count(&self) -> u32 {
        self.end_frame.saturating_sub(self.start_frame)
    }
}

impl App {
    pub(super) fn apply_output_module(&mut self, action: Action) {
        match action {
            Action::AddOutputModule(module) => {
                self.output_modules.push(module);
            }
            Action::RemoveOutputModule(idx) => {
                if idx < self.output_modules.len() {
                    self.output_modules.remove(idx);
                }
            }
            Action::SetOutputModuleFormat { idx, format } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    m.format = format;
                    // Keep codec/depth coherent with the new format.
                    m.codec = match format {
                        OutputModuleFormat::Mp4 => OutputCodec::H264,
                        OutputModuleFormat::Mov => OutputCodec::ProRes422,
                        OutputModuleFormat::PngSequence | OutputModuleFormat::ExrSequence => {
                            OutputCodec::None
                        }
                    };
                    if format == OutputModuleFormat::ExrSequence {
                        m.depth = ColorDepth::ThirtyTwo;
                    }
                }
            }
            Action::SetOutputModuleCodec { idx, codec } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    m.codec = codec;
                }
            }
            Action::SetOutputModuleDepth { idx, depth } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    m.depth = depth;
                }
            }
            Action::SetOutputModuleScale { idx, scale } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    m.scale = scale.clamp(0.01, 8.0);
                    // An explicit scale overrides any pinned resolution.
                    m.resolution = None;
                }
            }
            Action::SetOutputModuleResolution { idx, width, height } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    m.resolution = Some((width.max(1), height.max(1)));
                }
            }
            Action::SetOutputModuleRange { idx, start, end } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
                    m.start_frame = lo;
                    m.end_frame = hi;
                }
            }
            Action::SetOutputModuleAudio { idx, enabled } => {
                if let Some(m) = self.output_modules.get_mut(idx) {
                    m.audio_enabled = enabled;
                }
            }
            _ => unreachable!("apply_output_module called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_output_module() {
        let m = OutputModule::default();
        assert_eq!(m.format, OutputModuleFormat::Mp4);
        assert_eq!(m.codec, OutputCodec::H264);
        assert_eq!(m.depth, ColorDepth::Eight);
        assert!((m.scale - 1.0).abs() < 1e-6);
        assert!(m.audio_enabled);
    }

    #[test]
    fn test_new_infers_codec_and_depth() {
        let mov = OutputModule::new("ProRes", OutputModuleFormat::Mov);
        assert_eq!(mov.codec, OutputCodec::ProRes422);
        let exr = OutputModule::new("EXR", OutputModuleFormat::ExrSequence);
        assert_eq!(exr.codec, OutputCodec::None);
        assert_eq!(exr.depth, ColorDepth::ThirtyTwo);
        assert!(exr.format.is_sequence());
    }

    #[test]
    fn test_output_size_from_scale() {
        let mut m = OutputModule::default();
        m.scale = 0.5;
        assert_eq!(m.output_size(1920, 1080), (960, 540));
        m.scale = 1.0;
        assert_eq!(m.output_size(1920, 1080), (1920, 1080));
    }

    #[test]
    fn test_output_size_explicit_resolution_wins() {
        let mut m = OutputModule::default();
        m.scale = 0.5;
        m.resolution = Some((1280, 720));
        assert_eq!(m.output_size(1920, 1080), (1280, 720));
    }

    #[test]
    fn test_frame_count() {
        let mut m = OutputModule::default();
        m.start_frame = 10;
        m.end_frame = 40;
        assert_eq!(m.frame_count(), 30);
        // Underflow guarded.
        m.start_frame = 50;
        assert_eq!(m.frame_count(), 0);
    }

    #[test]
    fn test_add_remove_module() {
        let mut app = App::new();
        assert!(app.output_modules.is_empty());
        app.apply(Action::AddOutputModule(OutputModule::default()));
        assert_eq!(app.output_modules.len(), 1);
        app.apply(Action::RemoveOutputModule(0));
        assert!(app.output_modules.is_empty());
    }

    #[test]
    fn test_set_format_keeps_codec_coherent() {
        let mut app = App::new();
        app.apply(Action::AddOutputModule(OutputModule::default()));
        app.apply(Action::SetOutputModuleFormat {
            idx: 0,
            format: OutputModuleFormat::ExrSequence,
        });
        assert_eq!(app.output_modules[0].format, OutputModuleFormat::ExrSequence);
        assert_eq!(app.output_modules[0].codec, OutputCodec::None);
        assert_eq!(app.output_modules[0].depth, ColorDepth::ThirtyTwo);
    }

    #[test]
    fn test_set_scale_clears_resolution_and_clamps() {
        let mut app = App::new();
        let mut m = OutputModule::default();
        m.resolution = Some((100, 100));
        app.apply(Action::AddOutputModule(m));
        app.apply(Action::SetOutputModuleScale { idx: 0, scale: 100.0 });
        assert!(app.output_modules[0].resolution.is_none());
        assert!((app.output_modules[0].scale - 8.0).abs() < 1e-6);
    }

    #[test]
    fn test_set_range_orders_endpoints() {
        let mut app = App::new();
        app.apply(Action::AddOutputModule(OutputModule::default()));
        app.apply(Action::SetOutputModuleRange { idx: 0, start: 90, end: 10 });
        assert_eq!(app.output_modules[0].start_frame, 10);
        assert_eq!(app.output_modules[0].end_frame, 90);
    }

    #[test]
    fn test_set_resolution_and_audio() {
        let mut app = App::new();
        app.apply(Action::AddOutputModule(OutputModule::default()));
        app.apply(Action::SetOutputModuleResolution { idx: 0, width: 3840, height: 2160 });
        assert_eq!(app.output_modules[0].resolution, Some((3840, 2160)));
        app.apply(Action::SetOutputModuleAudio { idx: 0, enabled: false });
        assert!(!app.output_modules[0].audio_enabled);
    }

    #[test]
    fn test_set_codec_and_depth() {
        let mut app = App::new();
        app.apply(Action::AddOutputModule(OutputModule::default()));
        app.apply(Action::SetOutputModuleCodec { idx: 0, codec: OutputCodec::ProRes4444 });
        assert_eq!(app.output_modules[0].codec, OutputCodec::ProRes4444);
        app.apply(Action::SetOutputModuleDepth { idx: 0, depth: ColorDepth::Sixteen });
        assert_eq!(app.output_modules[0].depth, ColorDepth::Sixteen);
        assert_eq!(app.output_modules[0].depth.bits(), 16);
    }
}
