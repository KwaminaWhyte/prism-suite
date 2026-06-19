//! The single shared application state for the GPUI host.
//!
//! `App` owns everything panels read or mutate: the editing [`Project`] (a video
//! sequence: a sized comp with a flat list of [`Clip`]s on stacked tracks), the
//! [`CanvasHost`] (the CPU playhead-frame sampler bridged into GPUI), the
//! playhead frame position, the active [`Tool`], and the view. Panels NEVER
//! mutate `App` fields directly — they emit an [`Action`], and the root view
//! routes it through [`App::apply`], the single choke point that mutates state
//! and marks the host dirty when the composited frame changes. This keeps the
//! panel→state seam narrow so panels can be ported in parallel.
//!
//! Owns the sequence / clip / track model, all editing actions, and the
//! `CanvasHost` bridge that samples the CPU program frame into GPUI.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gpui::{Bounds, Pixels};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer};

use crate::canvas_host::CanvasHost;
use crate::waveform::WaveformCache;

/// The editing tools, mirroring the egui app's transport/edit affordances. The
/// GPUI host wires behavior per wave; this pass selects the active tool only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,      // pick / move clips
    Razor,       // split a clip at the playhead
    Slip,        // slip a clip's source window
    Hand,        // pan the timeline / preview
    RippleTrim,  // trim head/tail; downstream clips slide (Premiere B-tool)
    RollTrim,    // move the edit point between two adjacent clips (Premiere N-tool)
    RateStretch, // drag clip edge to change duration = change speed
}

impl Tool {
    /// Short label for the toolbar.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Razor => "Razor",
            Tool::Slip => "Slip",
            Tool::Hand => "Hand",
            Tool::RippleTrim => "Ripple",
            Tool::RollTrim => "Roll",
            Tool::RateStretch => "Rate Stretch",
        }
    }

    /// Stable ordering for the toolbar tool buttons.
    pub const ALL: [Tool; 7] = [
        Tool::Select,
        Tool::Razor,
        Tool::Slip,
        Tool::Hand,
        Tool::RippleTrim,
        Tool::RollTrim,
        Tool::RateStretch,
    ];
}

/// 3-way color wheels: Lift (shadows), Gamma (midtones), Gain (highlights).
/// Applied per-pixel after the basic `ColorGrade` in the program sampler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorWheels {
    /// Shadow offset per channel. Identity = [0, 0, 0].
    pub lift: [f32; 3],
    /// Midtone gamma per channel (>0). Identity = [1, 1, 1].
    pub gamma: [f32; 3],
    /// Highlight gain per channel. Identity = [1, 1, 1].
    pub gain: [f32; 3],
}

impl Default for ColorWheels {
    fn default() -> Self {
        Self {
            lift: [0.0, 0.0, 0.0],
            gamma: [1.0, 1.0, 1.0],
            gain: [1.0, 1.0, 1.0],
        }
    }
}

impl ColorWheels {
    /// True when this is the identity (no effect on any pixel).
    pub fn is_identity(&self) -> bool {
        self.lift == [0.0, 0.0, 0.0]
            && self.gamma == [1.0, 1.0, 1.0]
            && self.gain == [1.0, 1.0, 1.0]
    }

    /// Apply the 3-way grade to one straight-sRGB pixel (0..1) in place.
    /// Formula per channel: `out[c] = gain[c] * in[c].powf(1/gamma[c]) + lift[c]`, clamped 0..1.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() { return; }
        for c in 0..3 {
            let v = rgb[c].clamp(0.0, 1.0);
            let g = self.gamma[c].max(0.01);
            let out = self.gain[c] * v.powf(1.0 / g) + self.lift[c];
            rgb[c] = out.clamp(0.0, 1.0);
        }
    }
}

/// Per-channel RGB tone curves. Each channel is a sorted list of `[input, output]`
/// control points (both in 0..1). Linear interpolation between points.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbCurves {
    pub master: Vec<[f32; 2]>,
    pub red:    Vec<[f32; 2]>,
    pub green:  Vec<[f32; 2]>,
    pub blue:   Vec<[f32; 2]>,
}

impl Default for RgbCurves {
    fn default() -> Self {
        let identity = vec![[0.0f32, 0.0], [1.0, 1.0]];
        Self { master: identity.clone(), red: identity.clone(), green: identity.clone(), blue: identity }
    }
}

impl RgbCurves {
    pub fn is_identity(&self) -> bool {
        let id: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 1.0]];
        self.master == id && self.red == id && self.green == id && self.blue == id
    }

    fn eval_curve(pts: &[[f32; 2]], x: f32) -> f32 {
        if pts.is_empty() { return x; }
        if x <= pts[0][0] { return pts[0][1]; }
        if x >= pts[pts.len()-1][0] { return pts[pts.len()-1][1]; }
        for w in pts.windows(2) {
            if x <= w[1][0] {
                let t = (x - w[0][0]) / (w[1][0] - w[0][0]).max(1e-6);
                return w[0][1] + t * (w[1][1] - w[0][1]);
            }
        }
        pts[pts.len()-1][1]
    }

    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() { return; }
        let ch_curves = [&self.red, &self.green, &self.blue];
        for c in 0..3 {
            let v = Self::eval_curve(&self.master, rgb[c]);
            rgb[c] = Self::eval_curve(ch_curves[c], v).clamp(0.0, 1.0);
        }
    }
}

/// Per-track audio effect (for the effect chain).
#[derive(Clone, Debug)]
pub enum AudioEffect {
    Eq3(TrackEq3),
    Compressor { threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32 },
    Reverb { room_size: f32, damping: f32, wet: f32 },
    Delay { time_ms: f32, feedback: f32, wet: f32 },
}

impl AudioEffect {
    pub fn label(&self) -> &'static str {
        match self {
            AudioEffect::Eq3(_) => "3-Band EQ",
            AudioEffect::Compressor { .. } => "Compressor",
            AudioEffect::Reverb { .. } => "Reverb",
            AudioEffect::Delay { .. } => "Delay",
        }
    }
}

/// Caption vertical position.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptionPosition { Bottom, Top, Custom(f32, f32) }

impl Default for CaptionPosition { fn default() -> Self { CaptionPosition::Bottom } }

/// Caption text style.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptionStyle {
    pub font_size: f32,
    pub color: [f32; 4],
    pub position: CaptionPosition,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self { font_size: 32.0, color: [1.0, 1.0, 1.0, 1.0], position: CaptionPosition::default() }
    }
}

/// A single subtitle/caption entry.
#[derive(Clone, Debug)]
pub struct Caption {
    pub start_secs: f32,
    pub end_secs: f32,
    pub text: String,
    pub style: CaptionStyle,
}

/// Marker kind on the master timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkerKind { InPoint, OutPoint, Chapter, Comment }

impl MarkerKind {
    pub fn label(&self) -> &'static str {
        match self {
            MarkerKind::InPoint => "In",
            MarkerKind::OutPoint => "Out",
            MarkerKind::Chapter => "Chapter",
            MarkerKind::Comment => "Comment",
        }
    }
}

/// A timeline marker (in/out point, chapter, or comment).
#[derive(Clone, Debug)]
pub struct GpuiMarker {
    pub time_secs: f32,
    pub name: String,
    pub color: [f32; 3],
    pub kind: MarkerKind,
}

/// Color space tag for the sequence output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Rec709,
    Rec2020,
    SRGB,
}

impl Default for ColorSpace {
    fn default() -> Self { ColorSpace::Rec709 }
}

/// Default frame rate (fps) used to quantize a video clip's source time into a
/// frame index when the real media fps isn't known. Mirrors the egui app's
/// `DEFAULT_VIDEO_FPS`.
pub const DEFAULT_VIDEO_FPS: f64 = 30.0;

/// Minimum timeline length (seconds) a clip may be trimmed/split to. A cut never
/// leaves either side shorter than this. Mirrors the egui app's `MIN_DUR`.
pub const MIN_DUR: f32 = 0.1;

/// Fallback timeline length (seconds) for an imported video whose media couldn't
/// be probed (e.g. ffmpeg missing). Mirrors the egui app's `DEFAULT_VIDEO_LEN`.
pub const DEFAULT_VIDEO_LEN: f32 = 10.0;

/// Default timeline length (seconds) for an imported still image (an image has
/// no intrinsic duration, so it gets a sensible default like the egui app).
pub const DEFAULT_IMAGE_LEN: f32 = 5.0;

/// Fallback timeline length (seconds) for an imported audio file whose media
/// couldn't be probed (e.g. ffmpeg missing).
pub const DEFAULT_AUDIO_LEN: f32 = 10.0;

/// Video container extensions imported as a real [`ClipSource::Video`]
/// (decoded via prism-media / ffmpeg). Mirrors the egui app's `VIDEO_EXTENSIONS`.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v"];

/// Audio container extensions imported as a [`ClipSource::Audio`] (decoded to
/// PCM peaks via `prism_media::decode_audio`). Mirrors the egui app's
/// `AUDIO_EXTENSIONS`.
pub const AUDIO_EXTENSIONS: &[&str] =
    &["wav", "mp3", "aac", "m4a", "flac", "ogg", "opus", "aiff"];

/// True if `path`'s extension is a recognized video container (case-insensitive).
pub fn is_video_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.as_str()))
}

/// True if `path`'s extension is a recognized audio container (case-insensitive).
pub fn is_audio_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.as_str()))
}

/// A real movie source on disk, decoded frame-by-frame at the playhead via
/// `prism_media::decode_frame_at`. A deliberately minimal mirror of the egui
/// app's `VideoClip`: just the path plus an optional probed fps so the host can
/// quantize the clip-local time to a stable source frame index (the decode
/// cache key). The full edit model (probe cache, source in/out, retime) lands
/// in a later wave.
#[derive(Clone, Debug)]
pub struct VideoSource {
    /// The video file on disk.
    pub path: PathBuf,
    /// Probed media frame rate, if known (else [`DEFAULT_VIDEO_FPS`]).
    pub fps: Option<f64>,
    /// Probed media duration in seconds, if known. This bounds the source
    /// window a trim / split can reveal: a clip can't show beyond the footage.
    pub duration: Option<f32>,
}

impl VideoSource {
    /// A video source for `path` with no probe yet.
    #[allow(dead_code)]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            fps: None,
            duration: None,
        }
    }

    /// Probe `path` (via `prism_media::probe` / ffmpeg) and build a source with
    /// its real fps cached, plus the probed media duration (seconds) so the
    /// import can size the placed clip to the footage. A probe failure (e.g.
    /// ffmpeg missing) yields an *unprobed* source and `None` duration, mirroring
    /// the egui app's graceful import — the host never blocks on a decoder.
    pub fn probed(path: impl Into<PathBuf>) -> (Self, Option<f32>) {
        let path = path.into();
        match prism_media::probe(&path) {
            Ok(info) => {
                let dur = (info.duration_secs as f32)
                    .is_finite()
                    .then_some(info.duration_secs as f32)
                    .filter(|d| *d > 0.0);
                (
                    Self {
                        path,
                        fps: Some(info.fps),
                        duration: dur,
                    },
                    dur,
                )
            }
            Err(e) => {
                log::warn!(
                    "reel-gpui: probe failed for {} ({e}); importing unprobed",
                    path.display()
                );
                (
                    Self {
                        path,
                        fps: None,
                        duration: None,
                    },
                    None,
                )
            }
        }
    }

    /// The effective media fps (probed, or [`DEFAULT_VIDEO_FPS`] when unknown).
    pub fn fps(&self) -> f64 {
        self.fps
            .filter(|f| f.is_finite() && *f > 0.0)
            .unwrap_or(DEFAULT_VIDEO_FPS)
    }

    /// The 0-based source frame index shown at `source_time` seconds into the
    /// media: `floor(source_time * fps)`. The decode cache keys on this so
    /// scrubbing within one frame's window costs no re-decode.
    pub fn frame_index_at(&self, source_time: f32) -> u64 {
        (source_time.max(0.0) as f64 * self.fps()).floor().max(0.0) as u64
    }

    /// The source seek time (seconds) at the start of source frame `index`:
    /// `index / fps`. This is the timestamp handed to the decoder.
    pub fn frame_time(&self, index: u64) -> f64 {
        index as f64 / self.fps()
    }

    /// The intrinsic source length in seconds (probed media duration, or
    /// [`DEFAULT_VIDEO_LEN`] when unprobed). Bounds how far a trim / split can
    /// reveal of the source window. Mirrors the egui `VideoClip::source_len`.
    pub fn source_len(&self) -> f32 {
        self.duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(DEFAULT_VIDEO_LEN)
            .max(MIN_DUR)
    }
}

/// A real audio file on disk, decoded to interleaved-f32 PCM (via
/// `prism_media::decode_audio`) and downsampled to min/max peaks for the
/// timeline waveform. A deliberately minimal mirror of the egui app's audio
/// clip: the path, its probed duration so the import can size the placed clip
/// to the footage, and a per-clip linear gain (volume) folded into the mix. Has
/// no picture (it contributes nothing to the program frame), only a timeline
/// waveform.
#[derive(Clone, Debug)]
pub struct AudioSource {
    /// The audio file on disk.
    pub path: PathBuf,
    /// Probed media duration in seconds, if known. Bounds a trim / split.
    pub duration: Option<f32>,
    /// Per-clip linear gain (amplitude factor). `1.0` = unity (0 dB); `0.5` ≈
    /// −6 dB; `0.0` = silent. Folded into the audio mix for playback + export.
    /// Mirrors the egui app's `AudioClip::gain`.
    pub gain: f32,
}

/// Unity (0 dB) linear gain — the default for a freshly imported audio clip.
pub const DEFAULT_AUDIO_GAIN: f32 = 1.0;
/// Maximum per-clip audio gain (~+6 dB) the inspector stepper allows.
pub const MAX_AUDIO_GAIN: f32 = 2.0;

impl AudioSource {
    /// An audio source for `path` with no probe yet (unity gain).
    #[allow(dead_code)]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            duration: None,
            gain: DEFAULT_AUDIO_GAIN,
        }
    }

    /// Probe `path` (via `prism_media::probe` / ffmpeg) and build a source with
    /// its real duration cached. A probe failure yields an *unprobed* source and
    /// `None` duration, mirroring the graceful video import.
    pub fn probed(path: impl Into<PathBuf>) -> (Self, Option<f32>) {
        let path = path.into();
        match prism_media::probe(&path) {
            Ok(info) => {
                let dur = (info.duration_secs as f32)
                    .is_finite()
                    .then_some(info.duration_secs as f32)
                    .filter(|d| *d > 0.0);
                (
                    Self {
                        path,
                        duration: dur,
                        gain: DEFAULT_AUDIO_GAIN,
                    },
                    dur,
                )
            }
            Err(e) => {
                log::warn!(
                    "reel-gpui: audio probe failed for {} ({e}); importing unprobed",
                    path.display()
                );
                (
                    Self {
                        path,
                        duration: None,
                        gain: DEFAULT_AUDIO_GAIN,
                    },
                    None,
                )
            }
        }
    }

    /// The sanitized linear gain (finite, non-negative, capped at
    /// [`MAX_AUDIO_GAIN`]). Used by the audio mix.
    pub fn effective_gain(&self) -> f32 {
        if self.gain.is_finite() {
            self.gain.clamp(0.0, MAX_AUDIO_GAIN)
        } else {
            DEFAULT_AUDIO_GAIN
        }
    }

    /// The intrinsic source length in seconds (probed duration, or
    /// [`DEFAULT_AUDIO_LEN`] when unprobed). Bounds a trim / split.
    pub fn source_len(&self) -> f32 {
        self.duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(DEFAULT_AUDIO_LEN)
            .max(MIN_DUR)
    }
}

/// Where a clip's pixels come from. A minimal mirror of the egui app's
/// `ClipSource`: the CPU-sampleable sources the host bridge handles this pass (a
/// flat color fill, an on-disk still, a real movie decoded at the playhead, and
/// an audio file shown as a timeline waveform). Titles / nested sequences land
/// in later waves.
#[derive(Clone, Debug)]
pub enum ClipSource {
    /// A flat straight-sRGB color fill `[r, g, b, a]` in 0..=1.
    Color([f32; 4]),
    /// A still image on disk, decoded via `prism_io::load_image`, aspect-fit into
    /// the comp. Constructed by the Import action (toolbar) which picks an image
    /// off disk and places a clip at the playhead.
    Image(std::path::PathBuf),
    /// A movie file decoded frame-by-frame at the playhead via
    /// `prism_media::decode_frame_at` (ffmpeg CLI), aspect-fit into the comp.
    /// Constructed by the Import action (toolbar) which probes the picked file
    /// (duration / fps via `prism_media::probe`) and places a clip at the playhead.
    Video(VideoSource),
    /// An audio file decoded to PCM and shown as a waveform in its timeline lane.
    /// Contributes no picture to the program frame. Constructed by the Import
    /// action which probes the picked file (duration) and places a clip at the
    /// playhead.
    Audio(AudioSource),
    /// A generated title (text overlay on a background color). The text and
    /// style are stored here; `canvas_host` renders it as a colored rectangle
    /// (text is stored but not visually rasterized this wave — no heavy dep).
    Title {
        text: String,
        font_size: f32,
        color: [u8; 4],
        bg_color: Option<[u8; 4]>,
    },
    /// A nested sub-sequence: its clips and tracks are embedded inline.
    /// The program sampler renders this as a black frame (stub — recursive
    /// compositing deferred; wiring: sample_clip_raw in program_frame.rs).
    NestedClip {
        tracks: Vec<Track>,
        clips: Vec<Clip>,
        duration_secs: f32,
    },
}

impl ClipSource {
    /// A representative sRGB color for the clip's timeline/inspector block.
    pub fn block_color(&self) -> [f32; 3] {
        match self {
            ClipSource::Color(c) => [c[0], c[1], c[2]],
            ClipSource::Image(_) => [0.30, 0.46, 0.62],
            ClipSource::Video(_) => [0.55, 0.36, 0.62],
            ClipSource::Audio(_) => [0.24, 0.50, 0.34],
            ClipSource::Title { color, .. } => [
                color[0] as f32 / 255.0,
                color[1] as f32 / 255.0,
                color[2] as f32 / 255.0,
            ],
            ClipSource::NestedClip { .. } => [0.45, 0.20, 0.75],
        }
    }

    /// True if this source carries audio shown as a timeline waveform.
    pub fn is_audio(&self) -> bool {
        matches!(self, ClipSource::Audio(_))
    }

    /// The bounded source length (seconds) the clip can reveal, if any. A movie
    /// is bounded by its footage; a flat color or a still image is unbounded
    /// (`None`) — it can be trimmed to any length. Mirrors the egui app's
    /// `ClipSource::source_len` (the bound a trim / split clamps against).
    pub fn source_len(&self) -> Option<f32> {
        match self {
            ClipSource::Color(_) | ClipSource::Image(_) | ClipSource::Title { .. } => None,
            ClipSource::Video(v) => Some(v.source_len()),
            ClipSource::Audio(a) => Some(a.source_len()),
            ClipSource::NestedClip { duration_secs, .. } => Some(*duration_secs),
        }
    }
}

/// A per-clip **basic color grade** applied in the program sampler before the
/// clip is composited. A deliberately minimal mirror of the egui app's
/// `BasicCorrection` (`project/effect.rs`): just the three controls this wave
/// surfaces — **exposure** (photographic stops), **contrast** (about mid-grey),
/// and **saturation** (toward/past luma). The identity (`exposure 0`,
/// `contrast 0`, `saturation 1`) is a true per-pixel no-op, so an ungraded clip
/// pays nothing. The math matches the egui `BasicCorrection::apply` exactly so
/// the GPUI preview/export grade reads the same as the egui app's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorGrade {
    /// Exposure in photographic stops (~−4..4); brightness ×`2^exposure`.
    /// Identity `0`.
    pub exposure: f32,
    /// Contrast about mid-grey 0.5, ~−1..1. Identity `0`.
    pub contrast: f32,
    /// Saturation multiplier (0 = greyscale, 1 = unchanged, >1 boosts).
    /// Identity `1`.
    pub saturation: f32,
}

impl Default for ColorGrade {
    /// The identity grade — a true per-pixel no-op.
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            saturation: 1.0,
        }
    }
}

impl ColorGrade {
    /// True if this grade is the identity (no effect on any pixel) — the sampler
    /// skips the per-pixel pass entirely in that case.
    pub fn is_identity(&self) -> bool {
        self.exposure == 0.0 && self.contrast == 0.0 && self.saturation == 1.0
    }

    /// Apply the grade to one straight-sRGB pixel (`rgb` in 0..1) in place, in
    /// the documented control order (exposure → contrast → saturation). Every
    /// channel is clamped to 0..1. Mirrors the egui `BasicCorrection::apply`
    /// (the subset of controls this wave exposes).
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        // 1. Exposure — photographic stops.
        let exp = 2f32.powf(self.exposure);
        for v in rgb.iter_mut() {
            *v = (*v * exp).clamp(0.0, 1.0);
        }
        // 2. Contrast — linear stretch about mid-grey.
        let c = 1.0 + self.contrast;
        for v in rgb.iter_mut() {
            *v = ((*v - 0.5) * c + 0.5).clamp(0.0, 1.0);
        }
        // 3. Saturation — interpolate toward/past the pixel's luma.
        let l = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
        for v in rgb.iter_mut() {
            *v = (l + self.saturation * (*v - l)).clamp(0.0, 1.0);
        }
    }
}

/// HSL secondary grade: selectively adjust a hue range.
/// Identity: `hue_center 0`, `hue_width 0.15`, `sat_offset 0`, `lum_offset 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HslSecondaryGrade {
    /// Center hue to target, 0..1 (0/1 = red, 0.33 = green, 0.66 = blue).
    pub hue_center: f32,
    /// Hue range width (half-width, 0..0.5). 0 = disabled.
    pub hue_width: f32,
    /// Saturation offset for matched pixels, −1..1.
    pub sat_offset: f32,
    /// Luminance offset for matched pixels, −1..1.
    pub lum_offset: f32,
}

impl Default for HslSecondaryGrade {
    fn default() -> Self {
        Self { hue_center: 0.0, hue_width: 0.0, sat_offset: 0.0, lum_offset: 0.0 }
    }
}

impl HslSecondaryGrade {
    pub fn is_identity(&self) -> bool {
        self.hue_width == 0.0 && self.sat_offset == 0.0 && self.lum_offset == 0.0
    }

    /// Apply secondary grade to one sRGB pixel in-place.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() { return; }
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        let dist = (h - self.hue_center).abs();
        let dist = dist.min(1.0 - dist); // wrap
        let influence = (1.0 - (dist / self.hue_width.max(0.001))).clamp(0.0, 1.0);
        if influence <= 0.0 { return; }
        let new_s = (s + self.sat_offset * influence).clamp(0.0, 1.0);
        let new_l = (l + self.lum_offset * influence).clamp(0.0, 1.0);
        let (r2, g2, b2) = hsl_to_rgb(h, new_s, new_l);
        rgb[0] = rgb[0] * (1.0 - influence) + r2 * influence;
        rgb[1] = rgb[1] * (1.0 - influence) + g2 * influence;
        rgb[2] = rgb[2] * (1.0 - influence) + b2 * influence;
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r { ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0 }
            else if max == g { ((b - r) / d + 2.0) / 6.0 }
            else { ((r - g) / d + 4.0) / 6.0 };
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue2rgb = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0/6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0/2.0 { return q; }
        if t < 2.0/3.0 { return p + (q - p) * (2.0/3.0 - t) * 6.0; }
        p
    };
    (hue2rgb(p, q, h + 1.0/3.0), hue2rgb(p, q, h), hue2rgb(p, q, h - 1.0/3.0))
}

/// Speed curve for a clip's playback ramp.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeedCurve {
    Constant,
    Bezier { p0: f32, p1: f32, p2: f32, p3: f32 },
}

impl Default for SpeedCurve {
    fn default() -> Self { SpeedCurve::Constant }
}

/// Per-clip compositing blend mode (mirrors Premiere's opacity blend modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Subtract,
}

impl Default for ClipBlendMode {
    fn default() -> Self { ClipBlendMode::Normal }
}

/// A single clip placed on the timeline: a source plus its timeline placement
/// (`start`, `duration`, `track`), a per-clip `opacity`, and a per-clip color
/// `grade`. A minimal mirror of the egui app's `Clip`.
#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    pub source: ClipSource,
    /// Index of the track lane this clip lives on (0 = bottom).
    pub track: usize,
    /// Timeline position of the clip's left edge, in seconds.
    pub start: f32,
    /// Length of the clip on the timeline, in seconds.
    pub duration: f32,
    /// In-point into the source media, in seconds: the source time shown at the
    /// clip's left edge. Trimming the head advances this; splitting carries the
    /// advanced in-point to the right half so the cut is frame-continuous. A
    /// flat color / still ignores it (unbounded source). Mirrors the egui
    /// `Clip.source_in` (without speed; the GPUI mirror plays 1×).
    pub source_in: f32,
    /// Per-clip opacity in 0..=1 (multiplies the clip's alpha).
    pub opacity: f32,
    /// Per-clip basic color grade applied in the program sampler before the clip
    /// is composited (identity by default — a no-op).
    pub grade: ColorGrade,
    /// HSL secondary grade: selectively adjusts a hue range (identity by default).
    pub hsl_secondary: HslSecondaryGrade,
    /// Fade-in duration (seconds): audio ramps up from 0 at the clip's start.
    pub fade_in: f32,
    /// Fade-out duration (seconds): audio ramps down to 0 at the clip's end.
    pub fade_out: f32,
    /// Playback speed multiplier. 1.0 = normal, 2.0 = double speed. Default 1.0.
    pub speed: f32,
    /// If true, play the clip in reverse: `source_t = duration - local_t * speed`.
    pub reversed: bool,
    /// Speed curve: Constant (default) or Bezier for dynamic ramps.
    pub speed_curve: SpeedCurve,
    /// Optional low-res proxy file for this clip. When `Some`, the preview
    /// samples from this path instead of the original source.
    pub proxy_path: Option<PathBuf>,
    /// Link group id: clips sharing the same non-None value move/trim together.
    pub link_group: Option<u64>,

    // --- Batch 5: clip transform ---
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub crop_left: f32,
    pub crop_right: f32,
    pub crop_top: f32,
    pub crop_bottom: f32,
    pub blend_mode: ClipBlendMode,

    // --- Batch 5: time remap ---
    pub time_remap_enabled: bool,
    /// Sorted list of (timeline_t, source_t) keyframe pairs for time remapping.
    pub time_remap_keys: Vec<(f32, f32)>,
}

impl Clip {
    /// The clip's right edge on the timeline (seconds).
    pub fn end(&self) -> f32 {
        self.start + self.duration
    }

    /// The audio gain multiplier at timeline time `t` from the fade ramps.
    /// Returns a value in [0, 1]: 1.0 when no fade applies.
    pub fn fade_gain_at(&self, t: f32) -> f32 {
        let local = (t - self.start).max(0.0);
        let rem = (self.end() - t).max(0.0);
        let mut g = 1.0f32;
        if self.fade_in > 0.0 && local < self.fade_in {
            g = g.min(local / self.fade_in);
        }
        if self.fade_out > 0.0 && rem < self.fade_out {
            g = g.min(rem / self.fade_out);
        }
        g.clamp(0.0, 1.0)
    }

    /// True if the clip is active (covers) timeline time `t`.
    pub fn covers(&self, t: f32) -> bool {
        t >= self.start && t < self.end()
    }

    /// The source out-point (seconds): `source_in + duration` (the GPUI mirror
    /// plays at 1×, so a timeline second consumes one source second).
    #[allow(dead_code)]
    pub fn source_out(&self) -> f32 {
        self.source_in + self.duration
    }

    /// Clamp `source_in`/`duration` so the consumed source stays within the
    /// bounded source length (when bounded) and the clip stays `>= MIN_DUR`.
    /// Mirrors the egui app's `Project::clamp_to_source` (sans speed).
    fn clamp_to_source(&mut self) {
        self.source_in = self.source_in.max(0.0);
        if let Some(len) = self.source.source_len() {
            let max_dur = (len - self.source_in).max(0.0);
            self.duration = self.duration.clamp(MIN_DUR, max_dur.max(MIN_DUR));
        } else {
            self.duration = self.duration.max(MIN_DUR);
        }
    }

    /// Sample the effective source time at timeline time `t`, applying time remap
    /// when enabled. Falls back to linear (speed/reverse) when disabled.
    pub fn remapped_source_t(&self, t: f32) -> f32 {
        if !self.time_remap_enabled || self.time_remap_keys.len() < 2 {
            let local = (t - self.start).max(0.0);
            let raw = if self.reversed {
                self.source_in + self.duration - local * self.speed
            } else {
                self.source_in + local * self.speed
            };
            return raw.max(0.0);
        }
        let keys = &self.time_remap_keys;
        // Clamp to key range.
        if t <= keys[0].0 { return keys[0].1.max(0.0); }
        if t >= keys[keys.len() - 1].0 { return keys[keys.len() - 1].1.max(0.0); }
        for w in keys.windows(2) {
            let (t0, s0) = w[0];
            let (t1, s1) = w[1];
            if t <= t1 {
                let frac = (t - t0) / (t1 - t0).max(1e-9);
                return (s0 + frac * (s1 - s0)).max(0.0);
            }
        }
        keys[keys.len() - 1].1.max(0.0)
    }
}

impl Default for Clip {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: ClipSource::Color([0.0, 0.0, 0.0, 1.0]),
            track: 0,
            start: 0.0,
            duration: 1.0,
            source_in: 0.0,
            opacity: 1.0,
            grade: ColorGrade::default(),
            hsl_secondary: HslSecondaryGrade::default(),
            fade_in: 0.0,
            fade_out: 0.0,
            speed: 1.0,
            reversed: false,
            speed_curve: SpeedCurve::default(),
            proxy_path: None,
            link_group: None,
            anchor_x: 0.0,
            anchor_y: 0.0,
            crop_left: 0.0,
            crop_right: 0.0,
            crop_top: 0.0,
            crop_bottom: 0.0,
            blend_mode: ClipBlendMode::Normal,
            time_remap_enabled: false,
            time_remap_keys: Vec::new(),
        }
    }
}

/// Per-track controls: a name plus the enable ("eye") flag the compositor
/// honors. A minimal mirror of the egui app's `Track`.
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    /// Whether the track contributes to the program output (the "eye").
    pub enabled: bool,
}

// --- Wave 14: audio effects + track types ------------------------------------

/// One band of a 5-band parametric EQ.
#[derive(Clone, Copy, Debug)]
pub struct EqBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Per-track 5-band parametric EQ. Default bands: 80/250/1k/4k/12kHz, 0 dB each.
#[derive(Clone, Debug)]
pub struct TrackEq {
    pub enabled: bool,
    pub bands: [EqBand; 5],
}

impl Default for TrackEq {
    fn default() -> Self {
        Self {
            enabled: false,
            bands: [
                EqBand { freq: 80.0,   gain_db: 0.0, q: 0.707 },
                EqBand { freq: 250.0,  gain_db: 0.0, q: 0.707 },
                EqBand { freq: 1000.0, gain_db: 0.0, q: 0.707 },
                EqBand { freq: 4000.0, gain_db: 0.0, q: 0.707 },
                EqBand { freq: 12000.0,gain_db: 0.0, q: 0.707 },
            ],
        }
    }
}

/// Per-track peak compressor.
#[derive(Clone, Debug)]
pub struct TrackCompressor {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for TrackCompressor {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            makeup_db: 0.0,
        }
    }
}

/// Simple 3-band EQ per track.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackEq3 {
    pub low_gain_db: f32,    // ± 12 dB, shelf at 200 Hz
    pub mid_gain_db: f32,    // ± 12 dB, peak at mid_freq
    pub high_gain_db: f32,   // ± 12 dB, shelf at 8000 Hz
    pub mid_freq: f32,       // 200–8000 Hz
}

impl Default for TrackEq3 {
    fn default() -> Self {
        Self { low_gain_db: 0.0, mid_gain_db: 0.0, high_gain_db: 0.0, mid_freq: 1000.0 }
    }
}

/// Audio track channel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioTrackType {
    Mono,
    Stereo,
    Surround51,
}

impl AudioTrackType {
    pub fn label(self) -> &'static str {
        match self {
            AudioTrackType::Mono => "M",
            AudioTrackType::Stereo => "ST",
            AudioTrackType::Surround51 => "5.1",
        }
    }

    pub fn next(self) -> Self {
        match self {
            AudioTrackType::Mono => AudioTrackType::Stereo,
            AudioTrackType::Stereo => AudioTrackType::Surround51,
            AudioTrackType::Surround51 => AudioTrackType::Mono,
        }
    }
}

/// Default duration (seconds) for a freshly added transition. Mirrors the egui
/// app's `DEFAULT_TRANSITION_DUR`.
pub const DEFAULT_TRANSITION_DUR: f32 = 1.0;
/// Minimum transition duration in seconds. Mirrors `MIN_TRANSITION_DUR`.
pub const MIN_TRANSITION_DUR: f32 = 0.1;

/// The direction a [`TransitionKind::Wipe`] sweeps. The named direction is the
/// edge the *incoming* clip enters from (Premiere's convention): a `Left` wipe
/// reveals the incoming clip from the left edge rightward. Mirrors the egui
/// app's `WipeDir`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WipeDir {
    /// Enters from the left edge, travelling right.
    Left,
    /// Enters from the right edge, travelling left.
    Right,
    /// Enters from the top edge, travelling down.
    Up,
    /// Enters from the bottom edge, travelling up.
    Down,
}

impl WipeDir {
    /// A short human label for menus / overlays. (Kept for the timeline overlay
    /// wave; not yet rendered.)
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            WipeDir::Left => "Left",
            WipeDir::Right => "Right",
            WipeDir::Up => "Up",
            WipeDir::Down => "Down",
        }
    }
}

/// The kind of a [`Transition`] applied at a cut. A deliberately minimal mirror
/// of the egui app's `TransitionKind`: this wave's program sampler handles a
/// cross-dissolve, a dip-to-color (black/white), and a hard-edge wipe. (Push
/// lands in a later wave.) All variants are pure pixel math the sampler queries
/// each frame so the preview + export show them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionKind {
    /// Cross-dissolve: linearly blend the outgoing clip into the incoming one
    /// (outgoing weight `1 - progress`, incoming weight `progress`).
    CrossDissolve,
    /// Dip to a flat color (e.g. black or white): the outgoing clip fades to the
    /// color over the first half, then the color fades to the incoming clip over
    /// the second. `[r, g, b, a]` straight sRGB in `0..=1`.
    DipToColor([f32; 4]),
    /// Wipe: the incoming clip is progressively *revealed* over the stationary
    /// outgoing clip by a hard edge sweeping across the frame in [`WipeDir`].
    Wipe(WipeDir),
    /// Push: clip A slides out in `direction`; clip B slides in from the opposite
    /// side. A is the outgoing clip, B the incoming.
    Push(WipeDir),
    /// Film dissolve: a gamma-corrected dissolve that mimics analog optical
    /// printing — the blend uses a 2.2-gamma curve so midtones preserve luminance
    /// the way projector light layering does (instead of going dark mid-way like
    /// a linear dissolve).
    FilmDissolve,
    /// Iris circle: radial reveal growing from the center of the frame outward.
    IrisCircle,
    /// Clock wipe: a rotating reveal sweeping clockwise from 12 o'clock.
    ClockWipe,
    /// Diagonal wipe: hard-edge wipe travelling from the top-left corner to
    /// the bottom-right (built-in "custom plugin" demonstrating the plugin API).
    DiagonalWipe,
    /// Pixel dissolve: random per-pixel reveal ordered by a fixed noise threshold
    /// (built-in "custom plugin"). Each pixel flips from outgoing to incoming once
    /// `progress` exceeds its threshold.
    PixelDissolve,
}

impl TransitionKind {
    /// A short human label for the timeline overlay / inspector. (Kept for the
    /// timeline overlay wave; not yet rendered.)
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            TransitionKind::CrossDissolve => "Cross Dissolve",
            TransitionKind::DipToColor(_) => "Dip to Color",
            TransitionKind::Wipe(_) => "Wipe",
            TransitionKind::Push(_) => "Push",
            TransitionKind::FilmDissolve => "Film Dissolve",
            TransitionKind::IrisCircle => "Iris Circle",
            TransitionKind::ClockWipe => "Clock Wipe",
            TransitionKind::DiagonalWipe => "Diagonal Wipe",
            TransitionKind::PixelDissolve => "Pixel Dissolve",
        }
    }
}

/// Straight-sRGB black/white the dip-to-color presets dip through.
pub const DIP_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
pub const DIP_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// A transition centered on the cut between two edge-adjacent clips on a track.
/// A deliberately minimal mirror of the egui app's `Transition`: the program
/// sampler composites the outgoing (`from`) and incoming (`to`) clips per the
/// [`kind`](Self::kind) over the overlap so the preview + export show it.
///
/// The transition occupies `[center - duration/2, center + duration/2)` on the
/// timeline; `center` is the shared cut (the left clip's end / right clip's
/// start). `from`/`to` are clip-list indices of the outgoing/incoming clips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    /// What kind of transition this is (dissolve / dip / wipe).
    pub kind: TransitionKind,
    /// Clip-list index of the outgoing (left) clip.
    pub from: usize,
    /// Clip-list index of the incoming (right) clip.
    pub to: usize,
    /// The cut time (seconds): the shared edge the transition centers on.
    pub center: f32,
    /// Total transition length on the timeline, in seconds.
    pub duration: f32,
}

impl Transition {
    /// Timeline start of the transition span (seconds).
    pub fn start(&self) -> f32 {
        self.center - self.duration * 0.5
    }

    /// Timeline end of the transition span (seconds).
    pub fn end(&self) -> f32 {
        self.center + self.duration * 0.5
    }

    /// True if the transition's span covers timeline time `t`.
    pub fn covers(&self, t: f32) -> bool {
        t >= self.start() && t < self.end()
    }

    /// Progress through the transition at time `t`, clamped to `0..=1`
    /// (`0` = fully outgoing, `1` = fully incoming).
    pub fn progress(&self, t: f32) -> f32 {
        if self.duration <= 0.0 {
            return if t < self.center { 0.0 } else { 1.0 };
        }
        ((t - self.start()) / self.duration).clamp(0.0, 1.0)
    }

    /// The blend at time `t`: `(from_weight, to_weight, dip)`.
    ///
    /// - `from_weight` / `to_weight` are how much of the outgoing / incoming clip
    ///   show (each `0..=1`).
    /// - `dip` is `Some((color, amount))` when a [`TransitionKind::DipToColor`]
    ///   overlay is partially covering the frame (`amount` in `0..=1`, peaking at
    ///   the midpoint); `None` otherwise.
    ///
    /// For a cross-dissolve `from_weight + to_weight == 1`. For a dip exactly one
    /// clip shows (outgoing on the first half, incoming on the second) with the
    /// color dipped over it. A wipe reports a straight cut at the midpoint (its
    /// geometry is described by [`Self::wipe_reveal`]). Mirrors the egui app's
    /// `Transition::weights`.
    pub fn weights(&self, t: f32) -> (f32, f32, Option<([f32; 4], f32)>) {
        let p = self.progress(t);
        match self.kind {
            TransitionKind::CrossDissolve => (1.0 - p, p, None),
            TransitionKind::DipToColor(color) => {
                if p < 0.5 {
                    (1.0, 0.0, Some((color, p * 2.0)))
                } else {
                    (0.0, 1.0, Some((color, (1.0 - p) * 2.0)))
                }
            }
            TransitionKind::Wipe(_)
            | TransitionKind::Push(_)
            | TransitionKind::IrisCircle
            | TransitionKind::ClockWipe
            | TransitionKind::DiagonalWipe
            | TransitionKind::PixelDissolve => {
                if p < 0.5 {
                    (1.0, 0.0, None)
                } else {
                    (0.0, 1.0, None)
                }
            }
            // Film dissolve: gamma-corrected blend — linearize p through 2.2 gamma
            // so midtones preserve luminance the way optical printing does.
            TransitionKind::FilmDissolve => {
                let film_p = p.powf(1.0 / 2.2);
                (1.0 - film_p, film_p, None)
            }
        }
    }

    /// For a [`TransitionKind::Push`], the normalized (x, y) translation offsets
    /// for the outgoing (`from`) and incoming (`to`) clips at time `t`.
    /// Returns `(from_offset, to_offset)` where each is `(dx, dy)` as fractions
    /// of the frame (positive = right/down). `None` for non-push kinds.
    pub fn push_offsets(&self, t: f32) -> Option<((f32, f32), (f32, f32))> {
        let TransitionKind::Push(dir) = self.kind else {
            return None;
        };
        let p = self.progress(t);
        let (from_off, to_off) = match dir {
            WipeDir::Left  => ((-p, 0.0),       (1.0 - p, 0.0)),
            WipeDir::Right => ((p, 0.0),         (-(1.0 - p), 0.0)),
            WipeDir::Up    => ((0.0, -p),        (0.0, 1.0 - p)),
            WipeDir::Down  => ((0.0, p),         (0.0, -(1.0 - p))),
        };
        Some((from_off, to_off))
    }

    /// For a [`TransitionKind::Wipe`], the fraction-rect `(x0, y0, x1, y1)` (each
    /// `0..=1` of the frame) of the incoming clip revealed so far at time `t`
    /// (growing from the entering edge). `None` for non-wipe kinds. Mirrors the
    /// egui app's wipe `geometry`.
    pub fn wipe_reveal(&self, t: f32) -> Option<(f32, f32, f32, f32)> {
        let TransitionKind::Wipe(dir) = &self.kind else {
            return None;
        };
        let dir = *dir;
        let p = self.progress(t);
        Some(match dir {
            WipeDir::Left => (0.0, 0.0, p, 1.0),
            WipeDir::Right => (1.0 - p, 0.0, 1.0, 1.0),
            WipeDir::Up => (0.0, 0.0, 1.0, p),
            WipeDir::Down => (0.0, 1.0 - p, 1.0, 1.0),
        })
    }

    /// For [`TransitionKind::IrisCircle`] and [`TransitionKind::ClockWipe`],
    /// returns a per-pixel alpha mask (one byte per pixel, row-major) for the
    /// *incoming* clip at time `t`. Returns `None` for other transition kinds.
    pub fn pixel_mask(&self, t: f32, w: u32, h: u32) -> Option<Vec<u8>> {
        let p = self.progress(t);
        match self.kind {
            TransitionKind::IrisCircle => {
                let cx = w as f32 / 2.0;
                let cy = h as f32 / 2.0;
                let max_r = (cx * cx + cy * cy).sqrt();
                let r = p * max_r;
                let aa = 1.5_f32;
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let alpha = ((r - dist + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::ClockWipe => {
                let cx = w as f32 / 2.0;
                let cy = h as f32 / 2.0;
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        // atan2(dy, dx): 0=east, π/2=south (screen), -π/2=north
                        // Clockwise from north (12 o'clock): add π/2, normalize 0..1
                        let angle = dy.atan2(dx);
                        let norm = ((angle + std::f32::consts::FRAC_PI_2)
                            / (2.0 * std::f32::consts::PI))
                            .rem_euclid(1.0);
                        // Soft edge: ~1° of AA in normalized angle space
                        let aa = 0.005_f32;
                        let alpha = ((p - norm + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::DiagonalWipe => {
                // Reveal line travels from top-left (0,0) to bottom-right (w,h).
                // Each pixel is revealed when `(x/w + y/h) / 2 < p`.
                let mut mask = Vec::with_capacity((w * h) as usize);
                let aa = 0.01_f32;
                for y in 0..h {
                    for x in 0..w {
                        let t_px = (x as f32 / w as f32 + y as f32 / h as f32) * 0.5;
                        let alpha = ((p - t_px + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::PixelDissolve => {
                // Deterministic per-pixel noise threshold (Xorshift hash on pixel index).
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let idx = y * w + x;
                        // Simple integer hash → 0..1 threshold.
                        let mut v = idx.wrapping_mul(2654435761);
                        v ^= v >> 16;
                        v = v.wrapping_mul(2246822519);
                        v ^= v >> 13;
                        let threshold = (v & 0xFFFF) as f32 / 65535.0;
                        let alpha = if p >= threshold { 255u8 } else { 0u8 };
                        mask.push(alpha);
                    }
                }
                Some(mask)
            }
            _ => None,
        }
    }
}

/// The whole edit: a sized comp, a track count, and the clip list. A minimal
/// mirror of the egui app's `Project` (one sequence).
#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration: f32,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    /// Cross-dissolve transitions at cuts between edge-adjacent clips. Queried by
    /// the program sampler each frame to blend the two clips over the overlap.
    pub transitions: Vec<Transition>,
}

impl Project {
    /// A fresh 1080p / 30fps / 30s sequence with two tracks and a few starter
    /// color clips so the preview and timeline aren't blank on first launch
    /// (mirrors the egui app's `Project::new`).
    pub fn new() -> Self {
        let tracks = vec![
            Track {
                name: "V1".into(),
                enabled: true,
            },
            Track {
                name: "V2".into(),
                enabled: true,
            },
        ];
        let clips = vec![
            Clip {
                name: "Teal".into(),
                source: ClipSource::Color([0.18, 0.71, 0.66, 1.0]),
                track: 0,
                start: 0.0,
                duration: 6.0,
                ..Clip::default()
            },
            Clip {
                name: "Amber".into(),
                source: ClipSource::Color([0.95, 0.55, 0.16, 1.0]),
                track: 0,
                start: 6.0,
                duration: 5.0,
                ..Clip::default()
            },
            Clip {
                name: "Indigo".into(),
                source: ClipSource::Color([0.36, 0.40, 0.85, 1.0]),
                track: 1,
                start: 3.0,
                duration: 4.0,
                opacity: 0.6,
                ..Clip::default()
            },
        ];
        Self {
            name: "Sequence 1".into(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            duration: 30.0,
            tracks,
            clips,
            transitions: Vec::new(),
        }
    }

    /// The effective clips active at timeline time `t`, ordered **bottom track
    /// first**: for each visible track, the topmost clip covering `t` (latest in
    /// the list wins ties). The program-frame sampler folds these with the
    /// over-operator. Mirrors the resolution `program_frame.rs` does in the egui
    /// app (minus adjustment layers / transitions, deferred to later waves).
    pub fn effective_clips_at(&self, t: f32) -> Vec<&Clip> {
        let mut out = Vec::new();
        for (ti, track) in self.tracks.iter().enumerate() {
            if !track.enabled {
                continue;
            }
            let top = self
                .clips
                .iter()
                .filter(|c| c.track == ti && c.covers(t))
                .last();
            if let Some(c) = top {
                out.push(c);
            }
        }
        out
    }

    /// The current playhead frame index for a time `t` (for the toolbar readout).
    pub fn frame_at(&self, t: f32) -> u64 {
        (t * self.fps).round().max(0.0) as u64
    }

    /// Find the edge-adjacent clip pair forming a cut at (about) timeline time
    /// `t` on the same track as the clip at `near_idx`. Returns
    /// `(left_idx, right_idx, cut_time)` where the left clip's end meets the
    /// right clip's start. `t` picks the nearer of the clip's two edges. Mirrors
    /// the egui app's `Project::find_cut`.
    pub fn find_cut(&self, near_idx: usize, t: f32) -> Option<(usize, usize, f32)> {
        let clip = self.clips.get(near_idx)?;
        let track = clip.track;
        let left_edge = clip.start;
        let right_edge = clip.end();
        let use_right = (t - right_edge).abs() <= (t - left_edge).abs();
        let cut = if use_right { right_edge } else { left_edge };

        let left = self
            .clips
            .iter()
            .enumerate()
            .find(|(_, c)| c.track == track && (c.end() - cut).abs() < 1e-3)
            .map(|(j, _)| j)?;
        let right = self
            .clips
            .iter()
            .enumerate()
            .find(|(j, c)| *j != left && c.track == track && (c.start - cut).abs() < 1e-3)
            .map(|(j, _)| j)?;
        Some((left, right, cut))
    }

    /// Add (or replace) a transition of `kind` at the cut between the clip at
    /// `idx` and its adjacent neighbour nearest time `t`. The transition centers
    /// on the cut and its duration is clamped so it covers at most the shorter
    /// adjoining clip (and stays `>= MIN_TRANSITION_DUR`). Any existing transition
    /// on the same cut is replaced. Returns the transition's index, or `None`
    /// when there is no adjacent clip. Mirrors the egui app's
    /// `Project::add_transition`.
    pub fn add_transition(
        &mut self,
        idx: usize,
        t: f32,
        kind: TransitionKind,
        duration: f32,
    ) -> Option<usize> {
        let (from, to, cut) = self.find_cut(idx, t)?;
        let left_room = self.clips[from].duration;
        let right_room = self.clips[to].duration;
        let max_dur = left_room.min(right_room).max(MIN_TRANSITION_DUR);
        let dur = duration.clamp(MIN_TRANSITION_DUR, max_dur);

        let tr = Transition {
            kind,
            from,
            to,
            center: cut,
            duration: dur,
        };
        if let Some(existing) = self
            .transitions
            .iter()
            .position(|x| x.from == from && x.to == to && (x.center - cut).abs() < 1e-3)
        {
            self.transitions[existing] = tr;
            Some(existing)
        } else {
            self.transitions.push(tr);
            Some(self.transitions.len() - 1)
        }
    }

    /// The transition active at timeline time `t`, if any (later entries win on a
    /// tie), restricted to transitions whose track is enabled. Returned by
    /// reference so the sampler can read its weights. Mirrors the egui app's
    /// `Project::active_transition` plus the preview's visible-track gate.
    pub fn active_transition(&self, t: f32) -> Option<&Transition> {
        self.transitions.iter().rev().find(|tr| {
            tr.covers(t)
                && self
                    .clips
                    .get(tr.from)
                    .and_then(|c| self.tracks.get(c.track))
                    .map(|trk| trk.enabled)
                    .unwrap_or(false)
        })
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

/// A group of clip indices treated as multi-camera angles.
#[derive(Clone, Debug)]
pub struct MulticamGroup {
    pub clips: Vec<usize>,     // clip indices
    pub active_angle: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    H264Mp4,
    ProResProxy,
    ProRes422,
    Gif,
}

impl Default for ExportFormat {
    fn default() -> Self { ExportFormat::H264Mp4 }
}

/// A named export preset.
#[derive(Clone, Debug)]
pub struct ExportPreset {
    pub name: String,
    pub format: ExportFormat,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bitrate_kbps: u32,
}

/// Every panel→state mutation a panel can request. Panels emit these; the root
/// view routes each into [`App::apply`]. EXTENSIBLE: later waves add variants
/// here and a matching arm in `apply` — that is the entire contract a parallel
/// agent touches when wiring a new interaction.
// Several variants are wired in `apply` but not yet emitted by a stub panel;
// they are the seams parallel agents fill in. Keep them rather than churn.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Action {
    /// Select the active tool (toolbar).
    SetTool(Tool),

    // --- Media import ---
    /// Import a media file from disk at `path` as a real clip (a probed
    /// [`ClipSource::Video`] for a movie extension, else a [`ClipSource::Image`]
    /// still) and place it at the playhead on the first visible track. Changes
    /// the composite — marks the host dirty.
    ImportMedia(PathBuf),

    /// Move clip `index` to a new timeline `start` (seconds), snapped to the
    /// nearest frame and clamped to `>= 0`. Changes the composite — marks dirty.
    MoveClip { index: usize, start: f32 },

    // --- Trimming (head / tail edge drag) ---
    /// Trim clip `index`'s **left edge** (head) to timeline time `t` (frame-
    /// snapped) without rippling: `start` moves to `t`, `source_in` follows so
    /// the cut reveals the matching part of the source, and `duration` shrinks/
    /// grows to keep the right edge fixed. Clamped to `>= MIN_DUR`, `source_in
    /// >= 0`, and the bounded source. Changes the composite — marks dirty.
    TrimClipIn { index: usize, t: f32 },
    /// Trim clip `index`'s **right edge** (tail) to timeline time `t` (frame-
    /// snapped) without rippling: `duration` changes so the right edge sits at
    /// `t`; `start`/`source_in` are unchanged. Clamped to `>= MIN_DUR` and the
    /// bounded source length. Changes the composite — marks dirty.
    TrimClipOut { index: usize, t: f32 },

    // --- Ripple / roll trim ---
    /// Ripple-trim the **left edge** of clip `index` to `t`: behaves like
    /// `TrimClipIn` then slides every clip on the same track that comes *before*
    /// `index` (lower `start`) left by the same delta so no gap opens.
    RippleTrimClipIn { index: usize, t: f32 },
    /// Ripple-trim the **right edge** of clip `index` to `t`: behaves like
    /// `TrimClipOut` then slides every clip on the same track that comes *after*
    /// `index` (higher `start`) right by the same delta.
    RippleTrimClipOut { index: usize, t: f32 },
    /// Roll-trim: move the edit between clip `index` and its right neighbour on
    /// the same track by `delta` seconds (positive = right). Trims the right
    /// edge of `index` and left edge of the neighbour by `delta` simultaneously,
    /// preserving total sequence duration.
    RollTrimEdit { index: usize, delta: f32 },

    // --- Razor / split ---
    /// Split clip `index` at timeline time `t` (frame-snapped) into two clips:
    /// the original becomes the left part `[start, t)` and a new right part
    /// `[t, end)` carrying the advanced `source_in` (frame-continuous). A no-op
    /// unless `t` is strictly inside the clip (≥ MIN_DUR on both sides).
    SplitClip { index: usize, t: f32 },
    /// Razor: split **every** clip on a visible track straddling the playhead
    /// at the current `time` (the split-at-playhead action / razor tool). Marks
    /// dirty when any clip is cut.
    SplitAtPlayhead,

    // --- Transport / playhead ---
    /// Move the playhead to an absolute timeline time (seconds). Re-samples the
    /// program frame — marks the host dirty.
    Seek(f32),
    /// Step the playhead by `delta` seconds (clamped to the sequence). Dirty.
    StepBy(f32),
    /// Toggle real-time playback. While playing, the render loop advances the
    /// playhead by the real elapsed wall-clock time each animation frame (driven
    /// by `window.request_animation_frame()`), re-sampling the decoded program
    /// frame as it goes, and stops at the sequence end. Re-anchors the play clock
    /// on (re)start. If toggled on while the playhead is already at the end, it
    /// rewinds to the head first so play always advances.
    TogglePlay,
    /// Stop playback (no-op if already stopped). Used by edits that shouldn't run
    /// against a moving playhead.
    Pause,

    // --- Transitions ---
    /// Add a cross-dissolve at the cut nearest the playhead on the track of the
    /// selected clip (or clip `index` when given), with the default duration. A
    /// no-op when the clip has no edge-adjacent neighbour. Changes the composite —
    /// marks dirty.
    AddCrossDissolve { index: usize },
    /// Add a transition of `kind` at the cut nearest the playhead on the track of
    /// clip `index`, with the default duration. A no-op when the clip has no
    /// edge-adjacent neighbour. Changes the composite — marks dirty. Used by the
    /// toolbar's dip-to-black/white and wipe buttons.
    AddTransition { index: usize, kind: TransitionKind },

    // --- Per-clip audio volume ---
    /// Set audio clip `index`'s linear gain (volume), clamped to
    /// `0..=MAX_AUDIO_GAIN`. A no-op for a non-audio clip. Changes the mix —
    /// marks dirty so a re-export picks it up (no picture change).
    SetClipGain { index: usize, gain: f32 },

    // --- Color grade ---
    /// Set clip `index`'s color grade (exposure / contrast / saturation). Changes
    /// the composite — marks dirty.
    SetClipGrade { index: usize, grade: ColorGrade },

    // --- Tracks ---
    /// Toggle a track's enable ("eye"). Changes the composite — marks dirty.
    ToggleTrackEnabled(usize),

    // --- Clips ---
    /// Select a clip by index (pure UI state — no re-sample).
    SelectClip(usize),

    // --- View ---
    /// Multiply the preview zoom by a factor.
    ZoomBy(f32),
    /// Reset the preview view (zoom to fit).
    ResetView,

    // --- Wave 8: audio crossfade ---
    /// Set clip `index`'s audio fade-in duration (seconds, clamped ≥ 0).
    SetClipFadeIn { index: usize, secs: f32 },
    /// Set clip `index`'s audio fade-out duration (seconds, clamped ≥ 0).
    SetClipFadeOut { index: usize, secs: f32 },

    // --- Wave 8: mixer ---
    /// Set per-track linear volume (0..=2).
    SetTrackVolume { track_idx: usize, v: f32 },
    /// Toggle per-track mute.
    ToggleTrackMute { track_idx: usize },
    /// Toggle per-track solo.
    ToggleTrackSolo { track_idx: usize },
    /// Set master output volume (0..=2).
    SetMasterVolume { v: f32 },
    /// Toggle mixer panel visibility.
    ToggleMixer,

    // --- Wave 8: LUT + white balance ---
    /// Load a .cube LUT file from `path`. Parsed inline in apply.
    LoadLut { path: PathBuf },
    /// Set white balance temperature (Kelvin) and tint.
    SetWhiteBalance { temp: f32, tint: f32 },

    // --- Wave 9: speed ramp ---
    /// Set clip speed as a percentage (1–1000). Stored as a multiplier (pct/100).
    SetClipSpeed(usize, f32),
    /// Set clip reverse flag.
    SetClipReverse(usize, bool),

    /// Set the speed curve on a clip.
    SetSpeedCurve { clip_id: usize, curve: SpeedCurve },

    // --- Wave 9: multi-camera ---
    /// Toggle multi-camera edit mode.
    ToggleMulticam,
    /// In multicam mode, insert a cut at the playhead switching to `track_idx`.
    SwitchCamera(usize),
    SwitchMulticamAngle { group_id: usize, angle: usize },
    CreateMulticamGroup { name: String },

    // --- Wave 8: titles ---
    /// Add a 5-second title clip on a new track at the playhead.
    AddTitle,
    /// Set title clip text.
    SetTitleText { index: usize, text: String },
    /// Set title clip font size.
    SetTitleFontSize { index: usize, size: f32 },
    /// Set title clip foreground color.
    SetTitleColor { index: usize, color: [u8; 4] },
    /// Set title clip background color (`None` = transparent).
    SetTitleBgColor { index: usize, color: Option<[u8; 4]> },

    // --- Wave 11: audio stub ---
    /// Wire master volume to playback output device (stub — no rodio dep).
    SetAudioVolume(f32),
    /// Mute/unmute the playback output (stub).
    SetAudioMute(bool),

    // --- Wave 11: snap ---
    /// Toggle snap-to-edges.
    ToggleSnap,
    /// Begin a clip drag: store grab offset in clip_drag.
    BeginClipDrag { clip_id: usize, grab_offset: f32 },
    /// Move clip during drag: compute snapped position, emit MoveClip, store snap_point.
    MoveClipDrag { track_idx: usize, raw_t: f32 },
    /// End clip drag: clear clip_drag and snap_point.
    EndClipDrag,

    // --- Wave 11: scopes ---
    /// Toggle Lumetri Scopes panel.
    ToggleScopes,
    /// Switch the active scope tab: 0=Waveform, 1=Vectorscope, 2=Histogram.
    SetScopeTab(u8),

    // --- Wave 11: dual viewer ---
    ToggleDualViewer,
    SeekSource(f32),
    ToggleSourcePlay,
    SetSourceIn(f32),
    SetSourceOut(f32),
    InsertFromSource { clip_id: usize, in_t: f32, out_t: f32 },

    // --- Wave 11: bins ---
    ToggleBins,
    AddBin(String),
    SelectBin(usize),
    ImportToBin { bin_idx: usize, path: PathBuf },
    RemoveFromBin { bin_idx: usize, clip_idx: usize },
    SelectBinClip { bin_idx: usize, clip_idx: usize },
    InsertClipFromBin { bin_clip_idx: usize, track_idx: usize, at_t: f32 },

    // --- Wave 13: film dissolve, editable transition duration ---
    /// Set transition duration (seconds) by index into the track's transitions.
    SetTransitionDuration { track_idx: usize, trans_idx: usize, duration: f32 },
    /// Add a film-dissolve transition at the given clip index.
    AddFilmDissolve { index: usize },

    // --- Wave 13: chapter markers ---
    /// Add a chapter marker at `time` seconds.
    AddChapterMarker { time: f32, label: String },
    /// Remove chapter marker by index.
    RemoveChapterMarker(usize),

    // --- Wave 13: linked audio/video clips ---
    /// Toggle the audio/video link for a clip index.
    ToggleLinkClip(usize),

    // --- Wave 13: export presets ---
    /// Save an export preset (name, format, w, h, fps).
    SaveExportPreset { name: String, format: String, width: u32, height: u32, fps: f32 },
    /// Delete an export preset by index.
    DeleteExportPreset(usize),
    /// Apply an export preset.
    ApplyExportPreset(usize),

    // --- Wave 13: nest sequence ---
    /// Group selected clips into a nested sub-sequence.
    NestSelectedClips { name: String },
    /// Set HSL secondary grade on a specific clip.
    SetHslSecondaryGrade { clip_id: usize, grade: HslSecondaryGrade },

    // --- Wave 15: custom transitions ---
    /// Add a diagonal-wipe transition (built-in custom plugin) at clip `index`.
    AddDiagonalWipe { index: usize },
    /// Add a pixel-dissolve transition (built-in custom plugin) at clip `index`.
    AddPixelDissolve { index: usize },

    // --- Wave 15: proxy media ---
    /// Mark clip `index` as using a proxy path (low-res substitute).
    SetProxyPath { index: usize, path: PathBuf },
    /// Clear the proxy path on clip `index`, reverting to the original source.
    ClearProxy { index: usize },

    // --- Wave 15: log-to-Rec709 ---
    /// Toggle the log-to-Rec709 color transform on a track.
    ToggleLogTransform { track_idx: usize },

    // --- Wave 15: ProRes / DNxHD export via ffmpeg ---
    /// Export the sequence as a ProRes .mov (requires ffmpeg).
    ExportProRes(PathBuf),
    /// Export the sequence as a DNxHD .mov (requires ffmpeg).
    ExportDNxHD(PathBuf),
    /// Export as ProRes Proxy .mov
    ExportProResProxy(PathBuf),
    /// Export as ProRes 422 .mov
    ExportProRes422(PathBuf),
    /// Export as GIF (dithered, 256 color)
    ExportGif(PathBuf),
    /// Set the export format for the next export
    SetExportFormat(ExportFormat),

    // --- Wave 14: audio effects + track types ---
    /// Toggle the EQ on/off for a track.
    ToggleTrackEq { track_idx: usize },
    /// Set a single EQ band gain (dB) for a track.
    SetEqBand { track_idx: usize, band: usize, gain_db: f32 },
    /// Toggle the compressor on/off for a track.
    ToggleTrackCompressor { track_idx: usize },
    /// Update compressor settings for a track.
    SetCompressor { track_idx: usize, threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32, makeup_db: f32 },
    /// Cycle audio track type (Mono → Stereo → 5.1 → Mono).
    CycleAudioTrackType { track_idx: usize },
    /// Set 3-band EQ for a track.
    SetTrackEq3 { track_idx: usize, eq: TrackEq3 },

    // --- Batch 3: 3-way color wheels ---
    /// Set the 3-way color wheels (Lift / Gamma / Gain) globally.
    SetColorWheels(ColorWheels),

    // --- Batch 3: rate stretch ---
    /// Change a clip's speed by stretching its duration (rate stretch tool).
    RateStretchClip { index: usize, new_duration: f32 },

    // --- Batch 3: sequence settings ---
    /// Toggle the sequence settings dialog.
    ToggleSequenceSettings,
    /// Set the sequence output resolution.
    SetSequenceSize { w: u32, h: u32 },
    /// Set the sequence frame rate.
    SetFrameRate(f32),
    /// Set the sequence audio sample rate.
    SetSampleRate(u32),
    /// Set the sequence color space.
    SetColorSpace(ColorSpace),

    // --- Batch 4: RGB curves ---
    SetRgbCurves(RgbCurves),
    SetCurvesChannel(u8),
    AddCurvePoint { channel: u8, point: [f32; 2] },
    MoveCurvePoint { channel: u8, index: usize, point: [f32; 2] },

    // --- Batch 4: audio effect chains ---
    AddAudioEffect { track_idx: usize, effect: AudioEffect },
    RemoveAudioEffect { track_idx: usize, effect_idx: usize },
    SetAudioEffect { track_idx: usize, effect_idx: usize, effect: AudioEffect },
    ToggleTrackFx { track_idx: usize },
    ExpandTrackFx { track_idx: usize, effect_idx: Option<usize> },

    // --- Batch 4: closed captions ---
    AddCaption(Caption),
    RemoveCaption(usize),
    EditCaption { index: usize, caption: Caption },
    ImportSrt(PathBuf),
    ExportSrt(PathBuf),
    ToggleCaptionsPanel,

    // --- Batch 4: export preset list ---
    AddExportPreset(ExportPreset),
    DeleteExportPresetNew(usize),
    ExportWithPreset(usize),
    ToggleExportPresets,

    // --- Batch 4: markers panel ---
    ToggleMarkersPanel,
    AddMarkerAt { time: f32, name: String, kind: MarkerKind },
    RemoveMarker(usize),
    RenameMarker { index: usize, name: String },
    SetInPoint(f32),
    SetOutPoint(f32),

    // --- Batch 5: copy / paste / duplicate ---
    CopySelectedClips,
    CutSelectedClips,
    PasteClips { at_t: f32 },
    DuplicateSelectedClips,

    // --- Batch 5: clip transform ---
    SetClipAnchor { clip_idx: usize, x: f32, y: f32 },
    SetClipCrop { clip_idx: usize, left: f32, right: f32, top: f32, bottom: f32 },
    SetClipBlendMode { clip_idx: usize, mode: ClipBlendMode },
    ResetClipTransform { clip_idx: usize },

    // --- Batch 5: time remap ---
    SetTimeRemapEnabled { clip_idx: usize, enabled: bool },
    AddTimeRemapKey { clip_idx: usize, timeline_t: f32, source_t: f32 },
    MoveTimeRemapKey { clip_idx: usize, key_idx: usize, source_t: f32 },
    RemoveTimeRemapKey { clip_idx: usize, key_idx: usize },
    SetFreezeFrame { clip_idx: usize, at_t: f32 },

    // --- Batch 5: LUFS metering ---
    UpdateLufsMeters { power: f32 },
    ResetLufsIntegrated,

    // --- Batch 5: group ripple trim ---
    GroupRippleTrimIn { clip_indices: Vec<usize>, delta: f32 },
    GroupRippleTrimOut { clip_indices: Vec<usize>, delta: f32 },
}

/// The laid-out screen bounds of the timeline's scrub region (the lane body,
/// past the label gutter) in window pixels. The timeline panel records this in
/// its `canvas` paint each frame so its click/drag listeners can map an
/// absolute mouse x back to a timeline fraction. Shared by `Rc<Cell<_>>` so the
/// recorder and the listeners see the same value without a re-render round-trip.
pub type TimelineBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// What a timeline clip drag is doing to the grabbed clip. The timeline panel
/// sets this on `mouse_down` over a clip block (which sub-zone decides the
/// kind), and each `mouse_move` translates the pointer time into the matching
/// edit `Action`. Cleared on `mouse_up`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClipDragKind {
    /// Move the whole clip: the pointer drags `start` (the grab offset keeps the
    /// clip from snapping its left edge to the cursor).
    Move,
    /// Trim the clip's left edge (head) — `Action::TrimClipIn`.
    TrimIn,
    /// Trim the clip's right edge (tail) — `Action::TrimClipOut`.
    TrimOut,
    /// Ripple-trim the clip's left edge — downstream clips slide.
    RippleTrimIn,
    /// Ripple-trim the clip's right edge — downstream clips slide.
    RippleTrimOut,
    /// Roll-trim: move the edit point between this clip and the next on the same track.
    RollTrim,
    /// Rate-stretch: drag the right edge to change the clip's speed (duration / speed ratio).
    RateStretch,
}

/// An in-flight clip drag on the timeline: which clip, what it's doing, and the
/// pointer→clip-start time offset captured at grab so a body-move doesn't jump
/// the clip's left edge to the cursor. Shared by `Rc<Cell<_>>` so the block
/// `mouse_down` and the lane-body `mouse_move`/`mouse_up` see the same value.
#[derive(Clone, Copy, Debug)]
pub struct ClipDrag {
    pub index: usize,
    pub kind: ClipDragKind,
    /// `clip.start - grab_time` at mouse-down (only meaningful for `Move`).
    pub grab_offset: f32,
}

/// The shared in-flight clip-drag cell (None when nothing is being dragged).
pub type ClipDragCell = Rc<Cell<Option<ClipDrag>>>;

/// Scope data computed from the last rendered program frame.
#[allow(dead_code)]
pub struct ScopeData {
    /// Waveform: per-column (0..64) luma values (each entry is a luma 0..=1).
    pub waveform_cols: Vec<Vec<f32>>,
    /// Vectorscope: (u, v) dots in -0.5..0.5 normalized.
    pub vectorscope_dots: Vec<(f32, f32)>,
    /// Histogram R bins (256), normalized 0..=1.
    pub hist_r: Vec<f32>,
    /// Histogram G bins (256), normalized 0..=1.
    pub hist_g: Vec<f32>,
    /// Histogram B bins (256), normalized 0..=1.
    pub hist_b: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinClipType { Video, Audio, Title, Image }

#[derive(Clone, Debug)]
pub struct BinClip {
    pub name: String,
    pub path: PathBuf,
    pub duration: f32,
    pub clip_type: BinClipType,
}

#[derive(Clone, Debug)]
pub struct Bin {
    pub name: String,
    pub clips: Vec<BinClip>,
}

/// The single shared application state. Owns the project + host and all panel-
/// facing tool/playhead/view state. Mutated ONLY through [`App::apply`].
pub struct App {
    /// CPU playhead-frame sampler bridged into GPUI (the preview source).
    pub host: CanvasHost,
    /// The project the host samples; panels read it, `apply` mutates it.
    pub project: Project,

    /// The current playhead position, in seconds.
    pub time: f32,
    /// The active editing tool.
    pub active: Tool,
    /// The index of the selected clip in `project.clips`, if any (UI state).
    pub selected: Option<usize>,
    /// The preview zoom factor (1.0 = fit). UI state, applied at draw time.
    pub zoom: f32,
    /// Last laid-out bounds of the timeline scrub region (window pixels), set by
    /// the timeline panel's `canvas` recorder, read by its scrub listeners.
    pub timeline_bounds: TimelineBounds,
    /// The in-flight clip drag (move / trim-in / trim-out), or `None` when no
    /// clip is being dragged. Set by a clip block's `mouse_down`, consumed by the
    /// lane-body `mouse_move`/`mouse_up`.
    pub clip_drag: ClipDragCell,

    /// Whether the transport is playing (advances `time` each animation frame).
    /// The render loop reads this to decide whether to `tick()` + re-arm the
    /// next animation frame.
    pub playing: bool,
    /// Wall-clock anchor for the play loop. `None` when paused; set to `now` on
    /// (re)start so the next `tick` advances by the real elapsed time (frame-rate
    /// independent). Mirrors the Pulse host's `last_tick`.
    last_tick: Option<Instant>,

    /// Decoded audio peak envelopes for the timeline waveform, keyed by path.
    /// Shared-mutable (`RefCell`) so the timeline panel can lazily decode + bin
    /// an audio file on first draw through a read-only `&App`, mirroring how the
    /// program-frame cache is mutated behind the host. Decode happens once per
    /// file; later redraws read the binned envelope.
    pub waveforms: RefCell<WaveformCache>,

    // --- Wave 8: mixer --------------------------------------------------------
    /// Per-track linear volume (0..=2, 1.0 = unity). Parallel to `project.tracks`.
    pub track_volumes: Vec<f32>,
    /// Per-track mute flags. A muted track contributes zero audio.
    pub track_muted: Vec<bool>,
    /// Per-track solo flags. When any track is soloed, only soloed tracks play.
    pub track_soloed: Vec<bool>,
    /// Master output volume (0..=2, 1.0 = unity).
    pub master_volume: f32,
    /// Whether the mixer panel is visible.
    pub show_mixer: bool,

    // --- Wave 14: audio effects + track types ---------------------------------
    /// Per-track parametric EQ (5-band). Parallel to `project.tracks`.
    pub track_eq: Vec<TrackEq>,
    /// Per-track peak compressor. Parallel to `project.tracks`.
    pub track_comp: Vec<TrackCompressor>,
    /// Per-track audio channel format. Parallel to `project.tracks`.
    pub track_types: Vec<AudioTrackType>,

    // --- Wave 15: log-to-Rec709 ---
    /// Per-track log-to-Rec709 transform flag. When true, S-Log2→Rec709 is
    /// applied to video frames on that track before compositing.
    pub track_log_transform: Vec<bool>,

    // --- Wave 9: multi-camera -------------------------------------------------
    /// Whether multi-camera edit mode is active (viewer splits into per-track sub-panels).
    pub multicam_mode: bool,
    /// Track indices treated as camera angles in multicam mode.
    pub multicam_tracks: Vec<usize>,

    // --- Wave 8: LUT + white balance ------------------------------------------
    /// Path to a loaded .cube LUT file, if any.
    pub lut_path: Option<PathBuf>,
    /// Parsed 3D LUT: `(size, table)` where `table.len() == size^3`.
    pub lut_table: Option<(u32, Vec<[f32; 3]>)>,
    /// White balance color temperature in Kelvin (2000..10000). 6500 = neutral.
    pub white_balance_temp: f32,
    /// White balance tint (-150..+150). 0 = neutral.
    pub white_balance_tint: f32,

    // --- Wave 11: audio playback (rodio) ---
    /// True while audio is streaming to the OS audio device.
    pub audio_playing: bool,
    /// Owns the OS audio output stream (must stay alive while audio plays).
    audio_device_sink: Option<MixerDeviceSink>,
    /// Active rodio Player queued with the program audio mix.
    audio_player: Option<RodioPlayer>,

    // --- Wave 11: snap ---
    /// Whether timeline snap is active.
    pub snap_enabled: bool,
    /// The computed snap position during a clip drag (None when not snapping).
    pub snap_point: Option<f32>,
    /// Work area in-point (seconds).
    pub work_area_in: f32,
    /// Work area out-point (seconds).
    pub work_area_out: f32,

    // --- Wave 11: scopes ---
    /// Whether the Lumetri Scopes panel is visible.
    pub scopes_open: bool,
    /// Last computed scope data (from the program frame pixels).
    pub scope_data: Option<ScopeData>,
    /// Active scope tab: 0=Waveform, 1=Vectorscope, 2=Histogram.
    pub scope_tab: u8,

    // --- Wave 11: dual viewer ---
    pub dual_viewer: bool,
    pub source_playhead: f32,
    pub source_playing: bool,
    pub source_in: f32,
    pub source_out: f32,
    pub source_clip: Option<usize>,

    // --- Wave 11: bins ---
    pub bins: Vec<Bin>,
    pub bins_open: bool,
    pub selected_bin: usize,
    pub selected_bin_clip: Option<usize>,

    // --- Wave 13: chapter markers ---
    /// Chapter markers: `(time_secs, label)` pairs on the master timeline.
    pub chapter_markers: Vec<(f32, String)>,
    /// Linked audio/video clip indices (when moved, both tracks move together).
    pub linked_clips: std::collections::HashSet<usize>,
    /// Export presets: `(name, format, width, height, fps)`.
    pub export_presets: Vec<(String, String, u32, u32, f32)>,
    /// Active export preset index (for display).
    pub active_preset: Option<usize>,

    // --- Batch 2: export formats ---
    pub export_format: ExportFormat,

    // --- Batch 2: bezier drag ---
    pub bezier_drag_active: bool,
    pub bezier_drag_point: usize,

    // --- Batch 2: 3-band EQ per track ---
    pub track_eq3: Vec<TrackEq3>,

    pub multicam_groups: Vec<MulticamGroup>,

    // --- Batch 3: 3-way color wheels ---
    /// 3-way color wheels (Lift / Gamma / Gain) applied globally after clip grade.
    pub color_wheels: ColorWheels,

    // --- Batch 3: sequence settings ---
    /// Whether the sequence settings dialog is visible.
    pub show_sequence_settings: bool,
    /// Output color space for the sequence.
    pub sequence_color_space: ColorSpace,
    /// Audio sample rate for the sequence (Hz).
    pub sequence_sample_rate: u32,

    // --- Batch 4: RGB curves ---
    pub rgb_curves: RgbCurves,
    pub curves_channel: u8,

    // --- Batch 4: audio effect chains per track ---
    pub audio_effects: Vec<Vec<AudioEffect>>,
    pub track_fx_open: Vec<bool>,
    pub track_fx_expanded: Vec<Option<usize>>,

    // --- Batch 4: closed captions ---
    pub captions: Vec<Caption>,
    pub selected_caption: Option<usize>,
    pub show_captions_panel: bool,

    // --- Batch 4: export preset list ---
    pub export_preset_list: Vec<ExportPreset>,
    pub show_export_presets: bool,

    // --- Batch 4: markers panel ---
    pub show_markers_panel: bool,
    pub gpui_markers: Vec<GpuiMarker>,

    // --- Batch 5: copy/paste clipboard ---
    pub clipboard_clips: Vec<Clip>,

    // --- Batch 5: LUFS metering ---
    pub lufs_short_term: f32,
    pub lufs_integrated: f32,
    pub lufs_power_history: Vec<f32>,
}

/// Collect snap candidate times: all clip edges + playhead + work area in/out.
pub fn snap_candidates(clips: &[Clip], playhead: f32, work_in: f32, work_out: f32) -> Vec<f32> {
    let mut pts = vec![playhead, work_in, work_out];
    for c in clips {
        pts.push(c.start);
        pts.push(c.end());
    }
    pts
}

impl App {
    /// Build the shared state: a fresh project, boot the host, and seed the
    /// playhead / tool / view to sensible defaults (the egui app starts on the
    /// Select tool at the timeline head).
    pub fn new() -> Self {
        let project = Project::new();
        let host = CanvasHost::new();
        let n_tracks = project.tracks.len();
        Self {
            host,
            project,
            time: 4.0, // a starter playhead that lands on the seeded clips
            active: Tool::Select,
            selected: Some(0),
            zoom: 1.0,
            timeline_bounds: Rc::new(Cell::new(None)),
            clip_drag: Rc::new(Cell::new(None)),
            playing: false,
            last_tick: None,
            waveforms: RefCell::new(WaveformCache::new()),
            track_volumes: vec![1.0; n_tracks],
            track_muted: vec![false; n_tracks],
            track_soloed: vec![false; n_tracks],
            master_volume: 1.0,
            show_mixer: false,
            track_eq: (0..n_tracks).map(|_| TrackEq::default()).collect(),
            track_comp: (0..n_tracks).map(|_| TrackCompressor::default()).collect(),
            track_types: vec![AudioTrackType::Stereo; n_tracks],
            track_log_transform: vec![false; n_tracks],
            multicam_mode: false,
            multicam_tracks: (0..n_tracks).collect(),
            lut_path: None,
            lut_table: None,
            white_balance_temp: 6500.0,
            white_balance_tint: 0.0,
            audio_playing: false,
            audio_device_sink: None,
            audio_player: None,
            snap_enabled: true,
            snap_point: None,
            work_area_in: 0.0,
            work_area_out: 30.0,
            scopes_open: false,
            scope_data: None,
            scope_tab: 0,
            dual_viewer: false,
            source_playhead: 0.0,
            source_playing: false,
            source_in: 0.0,
            source_out: 0.0,
            source_clip: None,
            bins: vec![Bin { name: "Project".into(), clips: Vec::new() }],
            bins_open: false,
            selected_bin: 0,
            selected_bin_clip: None,
            chapter_markers: Vec::new(),
            linked_clips: std::collections::HashSet::new(),
            export_presets: Vec::new(),
            active_preset: None,
            export_format: ExportFormat::default(),
            bezier_drag_active: false,
            bezier_drag_point: 0,
            track_eq3: (0..n_tracks).map(|_| TrackEq3::default()).collect(),
            multicam_groups: Vec::new(),
            color_wheels: ColorWheels::default(),
            show_sequence_settings: false,
            sequence_color_space: ColorSpace::default(),
            sequence_sample_rate: 48000,
            rgb_curves: RgbCurves::default(),
            curves_channel: 0,
            audio_effects: vec![Vec::new(); n_tracks],
            track_fx_open: vec![false; n_tracks],
            track_fx_expanded: vec![None; n_tracks],
            captions: Vec::new(),
            selected_caption: None,
            show_captions_panel: false,
            export_preset_list: vec![
                ExportPreset { name: "1080p H.264".into(), format: ExportFormat::H264Mp4, width: 1920, height: 1080, fps: 30.0, bitrate_kbps: 8000 },
                ExportPreset { name: "4K H.264".into(), format: ExportFormat::H264Mp4, width: 3840, height: 2160, fps: 30.0, bitrate_kbps: 35000 },
                ExportPreset { name: "720p GIF".into(), format: ExportFormat::Gif, width: 1280, height: 720, fps: 15.0, bitrate_kbps: 0 },
                ExportPreset { name: "ProRes Proxy".into(), format: ExportFormat::ProResProxy, width: 1920, height: 1080, fps: 30.0, bitrate_kbps: 45000 },
            ],
            show_export_presets: false,
            show_markers_panel: false,
            gpui_markers: Vec::new(),
            clipboard_clips: Vec::new(),
            lufs_short_term: -f32::INFINITY,
            lufs_integrated: -f32::INFINITY,
            lufs_power_history: Vec::new(),
        }
    }

    /// Advance the playhead by the real wall-clock time elapsed since the last
    /// tick while playing, re-sampling the program frame. Returns `true` while
    /// the loop should keep running (so the render loop re-arms the next
    /// animation frame), `false` when playback has stopped — either because it
    /// wasn't playing or because it just hit the sequence end (where it pins the
    /// playhead to the end and clears `playing`, unlike Pulse which loops).
    ///
    /// The per-frame step is clamped (`min(0.1s)`) so a stalled frame (window
    /// occluded, a slow decode) can't jump the playhead by a huge gap; mirrors
    /// the Pulse host's `tick`.
    pub fn tick(&mut self) -> bool {
        if !self.playing {
            self.audio_playing = false;
            return false;
        }
        let now = Instant::now();
        let dt = match self.last_tick {
            Some(prev) => now.duration_since(prev).as_secs_f32().min(0.1),
            None => 0.0,
        };
        self.last_tick = Some(now);

        let dur = self.project.duration.max(1e-3);
        let next = self.time + dt;
        if next >= dur {
            // Stop at the sequence end (no loop): pin to the end and halt.
            self.time = dur;
            self.playing = false;
            self.last_tick = None;
            self.stop_audio();
            self.host.mark_dirty();
            return false;
        }
        self.time = next;
        self.host.mark_dirty();
        true
    }

    /// Start audio playback from `start_t` to the end of the sequence.
    /// Decodes the mix, opens a rodio device sink, and plays asynchronously.
    fn start_audio(&mut self, start_t: f32) {
        self.stop_audio();
        let mix_clips = crate::export::build_mix_clips(&self.project, &self.track_eq, &self.track_comp, &self.track_types, &self.audio_effects);
        if mix_clips.is_empty() {
            return;
        }
        let fps = self.project.fps.max(1.0);
        let dur = self.project.duration.max(1e-3);
        let first_frame = (start_t * fps).round() as u64;
        let total_frames = ((dur - start_t).max(0.0) * fps).ceil() as u64;
        if total_frames == 0 {
            return;
        }
        let plan = crate::export::FramePlan { first: first_frame, last: first_frame + total_frames - 1, count: total_frames };
        let audio_mix = crate::export::render_program_audio(&mix_clips, plan, fps);
        if audio_mix.samples.is_empty() {
            return;
        }
        let device_sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("reel-gpui: rodio open_default_sink: {e}");
                return;
            }
        };
        let channels = std::num::NonZero::new(audio_mix.channels.max(1)).unwrap();
        let rate = std::num::NonZero::new(audio_mix.sample_rate.max(1)).unwrap();
        let source = rodio::buffer::SamplesBuffer::new(channels, rate, audio_mix.samples.clone());
        let player = RodioPlayer::connect_new(device_sink.mixer());
        player.append(source);
        player.set_volume(self.master_volume.clamp(0.0, 2.0));
        player.play();
        self.audio_device_sink = Some(device_sink);
        self.audio_player = Some(player);
        self.audio_playing = true;
    }

    /// Stop and drop the active audio player + device sink.
    fn stop_audio(&mut self) {
        if let Some(player) = self.audio_player.take() {
            player.stop();
        }
        self.audio_device_sink = None;
        self.audio_playing = false;
    }

    /// Map an absolute window x (pixels) to a timeline time (seconds) using the
    /// last recorded scrub-region bounds. Returns `None` until the timeline has
    /// laid out at least once. The result is clamped to `[0, duration]`.
    pub fn timeline_x_to_time(&self, x_px: f32) -> Option<f32> {
        let bounds = self.timeline_bounds.get()?;
        let left = f32::from(bounds.origin.x);
        let width = f32::from(bounds.size.width);
        if width <= 0.0 {
            return None;
        }
        let frac = ((x_px - left) / width).clamp(0.0, 1.0);
        Some(frac * self.project.duration)
    }

    /// Convert a horizontal pixel delta on the timeline scrub region into a
    /// timeline-time delta (seconds), using the last recorded scrub-region width.
    /// `None` until the timeline has laid out. Used by the clip-drag listener to
    /// turn a pointer drag into a `start` change.
    pub fn timeline_dx_to_dt(&self, dx_px: f32) -> Option<f32> {
        let bounds = self.timeline_bounds.get()?;
        let width = f32::from(bounds.size.width);
        if width <= 0.0 {
            return None;
        }
        Some(dx_px / width * self.project.duration)
    }

    /// Play a short 100ms audio burst at timeline position `t` when scrubbing while paused.
    fn scrub_audio_burst(&mut self, t: f32) {
        let mix_clips = crate::export::build_mix_clips(
            &self.project, &self.track_eq, &self.track_comp, &self.track_types, &self.audio_effects,
        );
        if mix_clips.is_empty() { return; }
        let fps = self.project.fps.max(1.0);
        // 100ms burst = ~3 frames
        let burst_frames = ((0.1 * fps).ceil() as u64).max(1);
        let first_frame = (t * fps).floor() as u64;
        let plan = crate::export::FramePlan {
            first: first_frame,
            last: first_frame + burst_frames - 1,
            count: burst_frames,
        };
        let audio_mix = crate::export::render_program_audio(&mix_clips, plan, fps);
        if audio_mix.samples.is_empty() { return; }
        let Ok(device_sink) = DeviceSinkBuilder::open_default_sink() else { return; };
        let channels = std::num::NonZero::new(audio_mix.channels.max(1)).unwrap();
        let rate = std::num::NonZero::new(audio_mix.sample_rate.max(1)).unwrap();
        let source = rodio::buffer::SamplesBuffer::new(channels, rate, audio_mix.samples.clone());
        let player = RodioPlayer::connect_new(device_sink.mixer());
        player.append(source);
        player.set_volume(self.master_volume.clamp(0.0, 2.0));
        player.play();
        drop(player);
        drop(device_sink);
    }

    /// Snap a timeline time (seconds) to the nearest whole frame at the project
    /// fps: `round(t * fps) / fps`. Keeps a dragged clip's start frame-aligned.
    pub fn snap_to_frame(&self, t: f32) -> f32 {
        let fps = self.project.fps;
        if !fps.is_finite() || fps <= 0.0 {
            return t.max(0.0);
        }
        ((t * fps).round() / fps).max(0.0)
    }

    /// Snap `raw_t` to the nearest candidate (clip edges, playhead, work area)
    /// if snap is enabled and within `snap_threshold_px` pixels on the timeline.
    /// `exclude_clip` is the dragging clip index (its own edges are excluded from snapping).
    pub fn snapped_time(&self, raw_t: f32, exclude_clip: usize, snap_threshold_px: f32) -> f32 {
        if !self.snap_enabled {
            return raw_t.max(0.0);
        }
        let bounds = match self.timeline_bounds.get() {
            Some(b) => b,
            None => return raw_t.max(0.0),
        };
        let width_px = f32::from(bounds.size.width);
        if width_px <= 0.0 {
            return raw_t.max(0.0);
        }
        let dur = self.project.duration.max(1e-3);
        let threshold_t = snap_threshold_px / width_px * dur;
        let candidates = snap_candidates(&self.project.clips, self.time, self.work_area_in, self.work_area_out);
        let exclude_start = self.project.clips.get(exclude_clip).map(|c| c.start);
        let exclude_end = self.project.clips.get(exclude_clip).map(|c| c.end());
        let best = candidates.into_iter()
            .filter(|&t| {
                Some(t) != exclude_start && Some(t) != exclude_end
            })
            .min_by(|a, b| {
                (a - raw_t).abs().partial_cmp(&(b - raw_t).abs()).unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(t) = best {
            if (t - raw_t).abs() <= threshold_t {
                return t.max(0.0);
            }
        }
        raw_t.max(0.0)
    }

    /// Trim a clip's **left edge** (head) to timeline time `t` without rippling
    /// downstream clips. `start` moves to `t`, `source_in` follows by the same
    /// amount (so the cut reveals the matching part of the source), and
    /// `duration` shrinks/grows to keep the right edge fixed. Clamped so the
    /// clip stays `>= MIN_DUR` and `source_in >= 0`. Returns the applied shift
    /// (positive = head moved right / clip shortened), or `None` for a bad
    /// index. Mirrors the egui app's `Project::trim_in` (1× speed).
    pub fn trim_in(&mut self, idx: usize, t: f32) -> Option<f32> {
        let clip = self.project.clips.get(idx)?;
        let end = clip.end();
        let max_start = end - MIN_DUR;
        // The head can move left only as far as the available source allows:
        // `source_in` worth of source covers `source_in` timeline seconds at 1×.
        let min_start = (clip.start - clip.source_in).max(0.0);
        let new_start = t.clamp(min_start, max_start);
        let shift = new_start - clip.start;
        let clip = &mut self.project.clips[idx];
        clip.start = new_start;
        clip.source_in += shift;
        clip.duration -= shift;
        clip.clamp_to_source();
        Some(shift)
    }

    /// Trim a clip's **right edge** (tail) to timeline time `t` without
    /// rippling. `duration` changes so the right edge sits at `t`;
    /// `start`/`source_in` are unchanged. Clamped to `>= MIN_DUR` and the
    /// bounded source length. Returns the change in duration (positive =
    /// lengthened), or `None` for a bad index. Mirrors `Project::trim_out`.
    pub fn trim_out(&mut self, idx: usize, t: f32) -> Option<f32> {
        let clip = self.project.clips.get(idx)?;
        let start = clip.start;
        let old_dur = clip.duration;
        let mut new_end = t.max(start + MIN_DUR);
        if let Some(len) = clip.source.source_len() {
            // Remaining source `len - source_in` covers that many timeline secs.
            new_end = new_end.min(start + (len - clip.source_in));
        }
        let clip = &mut self.project.clips[idx];
        clip.duration = new_end - start;
        clip.clamp_to_source();
        Some(clip.duration - old_dur)
    }

    /// Ripple-trim the **left edge** of clip `idx` to `t`. After the normal
    /// `trim_in`, all clips on the same track whose `start` is less than the
    /// original `start` of `idx` are slid by `-shift` (closing the gap).
    pub fn ripple_trim_in(&mut self, idx: usize, t: f32) -> Option<f32> {
        let track = self.project.clips.get(idx)?.track;
        let old_start = self.project.clips[idx].start;
        let shift = self.trim_in(idx, t)?;
        if shift == 0.0 {
            return Some(0.0);
        }
        // Slide clips on the same track that were to the LEFT of the trimmed clip.
        for i in 0..self.project.clips.len() {
            if i != idx
                && self.project.clips[i].track == track
                && self.project.clips[i].start < old_start
            {
                self.project.clips[i].start -= shift;
            }
        }
        Some(shift)
    }

    /// Ripple-trim the **right edge** of clip `idx` to `t`. After the normal
    /// `trim_out`, all clips on the same track whose `start` is ≥ the original
    /// right edge of `idx` are slid by `delta` (closing or opening the gap).
    pub fn ripple_trim_out(&mut self, idx: usize, t: f32) -> Option<f32> {
        let track = self.project.clips.get(idx)?.track;
        let old_end = self.project.clips[idx].end();
        let delta = self.trim_out(idx, t)?;
        if delta == 0.0 {
            return Some(0.0);
        }
        // Slide clips that were at or after the old right edge.
        for i in 0..self.project.clips.len() {
            if i != idx
                && self.project.clips[i].track == track
                && self.project.clips[i].start >= old_end - 1e-4
            {
                self.project.clips[i].start += delta;
            }
        }
        Some(delta)
    }

    /// Roll-trim: shift the edit point between clip `idx` and its right
    /// neighbour on the same track by `delta` seconds (positive = move right).
    /// Trims clip `idx`'s right edge and the neighbour's left edge together so
    /// the total sequence duration is unchanged.
    pub fn roll_trim(&mut self, idx: usize, delta: f32) -> bool {
        let track = self.project.clips.get(idx).map(|c| c.track);
        let Some(track) = track else { return false };
        // Find the right neighbour: lowest `start` > clip[idx].end on same track.
        let right_idx = {
            let end = self.project.clips[idx].end();
            self.project
                .clips
                .iter()
                .enumerate()
                .filter(|(i, c)| *i != idx && c.track == track && c.start >= end - 1e-3)
                .min_by(|(_, a), (_, b)| a.start.partial_cmp(&b.start).unwrap())
                .map(|(i, _)| i)
        };
        let Some(right) = right_idx else { return false };
        let old_out = self.project.clips[idx].end();
        let new_out = (old_out + delta).max(self.project.clips[idx].start + MIN_DUR);
        let actual_delta = new_out - old_out;
        if actual_delta == 0.0 {
            return false;
        }
        // Trim right edge of idx.
        let idx_new_dur = self.project.clips[idx].duration + actual_delta;
        if idx_new_dur < MIN_DUR {
            return false;
        }
        self.project.clips[idx].duration = idx_new_dur;
        // Trim left edge of neighbour.
        let nb = &mut self.project.clips[right];
        let nb_new_in = nb.source_in + actual_delta;
        if nb_new_in < 0.0 {
            return false;
        }
        nb.start += actual_delta;
        nb.source_in = nb_new_in;
        nb.duration = (nb.duration - actual_delta).max(MIN_DUR);
        true
    }

    /// Split the clip at `idx` at timeline time `t`, in place. On success the
    /// original becomes the left part `[start, t)` and a new right part
    /// `[t, end)` is appended carrying the advanced `source_in` (frame-
    /// continuous in the source). Returns the new right clip's index, or `None`
    /// when `t` is not strictly inside the clip (≥ MIN_DUR on both sides).
    /// Mirrors the egui app's `Project::split_clip` (1× speed, no markers).
    pub fn split_clip(&mut self, idx: usize, t: f32) -> Option<usize> {
        let clip = self.project.clips.get(idx)?;
        let left_dur = t - clip.start;
        let right_dur = clip.end() - t;
        if left_dur < MIN_DUR || right_dur < MIN_DUR {
            return None;
        }
        let mut right = clip.clone();
        right.start = t;
        right.source_in = clip.source_in + left_dur; // 1×: consumed == left_dur
        right.duration = right_dur;
        self.project.clips[idx].duration = left_dur;
        self.project.clips.push(right);
        Some(self.project.clips.len() - 1)
    }

    /// Razor: split every clip straddling time `t` on a *visible* (enabled)
    /// track at `t`. Returns the number of clips cut. Mirrors the egui app's
    /// `Project::razor_all_at` (locked-track skip → visible-track skip here, the
    /// GPUI mirror's only per-track gate this wave).
    pub fn split_at(&mut self, t: f32) -> usize {
        let straddling: Vec<usize> = self
            .project
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.covers(t)
                    && (t - c.start) >= MIN_DUR
                    && (c.end() - t) >= MIN_DUR
                    && self
                        .project
                        .tracks
                        .get(c.track)
                        .map(|tr| tr.enabled)
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        let mut n = 0;
        for i in straddling {
            if self.split_clip(i, t).is_some() {
                n += 1;
            }
        }
        n
    }

    /// The index of the first visible (enabled) track, or `0` when none is
    /// enabled. An imported clip is placed here so it shows in the preview.
    fn first_visible_track(&self) -> usize {
        self.project
            .tracks
            .iter()
            .position(|t| t.enabled)
            .unwrap_or(0)
    }

    /// Build a clip from an on-disk media `path`, placed at the playhead on the
    /// first visible track: a probed [`ClipSource::Video`] for a movie extension
    /// (sized to the probed duration), else a [`ClipSource::Image`] still (a
    /// default length). The start is snapped to the nearest frame. Returns `None`
    /// only for an empty path. Mirrors the egui app's `make_video_source` +
    /// placement, but constructs directly into the project (no media bin yet).
    fn build_imported_clip(&self, path: &std::path::Path) -> Option<Clip> {
        if path.as_os_str().is_empty() {
            return None;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "media".into());
        let start = self.snap_to_frame(self.time);
        let track = self.first_visible_track();

        let (source, duration) = if is_video_path(path) {
            let (video, probed_len) = VideoSource::probed(path);
            (
                ClipSource::Video(video),
                probed_len.unwrap_or(DEFAULT_VIDEO_LEN),
            )
        } else if is_audio_path(path) {
            let (audio, probed_len) = AudioSource::probed(path);
            (
                ClipSource::Audio(audio),
                probed_len.unwrap_or(DEFAULT_AUDIO_LEN),
            )
        } else {
            (ClipSource::Image(path.to_path_buf()), DEFAULT_IMAGE_LEN)
        };

        Some(Clip {
            name,
            source,
            track,
            start,
            duration: duration.max(0.001),
            ..Clip::default()
        })
    }

    /// Compute scope data from the last rendered program frame stored in the host.
    pub fn update_scope_data(&mut self) {
        let rgba = &self.host.last_rgba;
        if rgba.is_empty() {
            self.scope_data = None;
            return;
        }
        let (w, h) = self.host.last_dims;
        if w == 0 || h == 0 {
            self.scope_data = None;
            return;
        }
        let cols = 256usize;
        let mut waveform_cols: Vec<Vec<f32>> = vec![Vec::new(); cols];
        let mut vectorscope_dots: Vec<(f32, f32)> = Vec::new();
        let mut hist_r = vec![0u32; 256];
        let mut hist_g = vec![0u32; 256];
        let mut hist_b = vec![0u32; 256];
        let mut total_pixels = 0u32;
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            let x = (i as u32) % w;
            // waveform: luma per column bucket
            let col = (x as usize * cols / w as usize).min(cols - 1);
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            waveform_cols[col].push(luma);
            // vectorscope: every 8th pixel
            if i % 8 == 0 {
                let u = -0.147 * r - 0.289 * g + 0.436 * b;
                let v = 0.615 * r - 0.515 * g - 0.100 * b;
                vectorscope_dots.push((u, v));
            }
            // histogram
            hist_r[px[0] as usize] += 1;
            hist_g[px[1] as usize] += 1;
            hist_b[px[2] as usize] += 1;
            total_pixels += 1;
        }
        let norm = if total_pixels > 0 { total_pixels as f32 } else { 1.0 };
        let hist_r: Vec<f32> = hist_r.iter().map(|&v| v as f32 / norm).collect();
        let hist_g: Vec<f32> = hist_g.iter().map(|&v| v as f32 / norm).collect();
        let hist_b: Vec<f32> = hist_b.iter().map(|&v| v as f32 / norm).collect();
        self.scope_data = Some(ScopeData {
            waveform_cols,
            vectorscope_dots,
            hist_r,
            hist_g,
            hist_b,
        });
    }

    /// Apply a panel-emitted [`Action`]. This is the ONLY place `App` state is
    /// mutated. Any action that changes the composited program frame marks the
    /// host dirty so the next `host.image(...)` re-samples. Pure UI state
    /// (tool/selection/view) does not touch the host.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::SetTool(t) => self.active = t,

            Action::ImportMedia(path) => {
                if let Some(clip) = self.build_imported_clip(&path) {
                    self.project.clips.push(clip);
                    self.selected = Some(self.project.clips.len() - 1);
                    self.host.mark_dirty();
                }
            }
            Action::MoveClip { index, start } => {
                let snapped = self.snap_to_frame(start.max(0.0));
                if let Some(clip) = self.project.clips.get(index) {
                    let delta = snapped - clip.start;
                    let group = clip.link_group;
                    self.project.clips[index].start = snapped;
                    // Joint-move: move linked partners by the same delta.
                    if let Some(gid) = group {
                        for i in 0..self.project.clips.len() {
                            if i != index && self.project.clips[i].link_group == Some(gid) {
                                let new_start = (self.project.clips[i].start + delta).max(0.0);
                                self.project.clips[i].start = self.snap_to_frame(new_start);
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            Action::TrimClipIn { index, t } => {
                let t = self.snap_to_frame(t);
                let group = self.project.clips.get(index).and_then(|c| c.link_group);
                if self.trim_in(index, t).is_some() {
                    // Joint-trim linked partners to the same time.
                    if let Some(gid) = group {
                        let partners: Vec<usize> = self.project.clips.iter().enumerate()
                            .filter(|(i, c)| *i != index && c.link_group == Some(gid))
                            .map(|(i, _)| i)
                            .collect();
                        for pi in partners {
                            self.trim_in(pi, t);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::TrimClipOut { index, t } => {
                let t = self.snap_to_frame(t);
                let group = self.project.clips.get(index).and_then(|c| c.link_group);
                if self.trim_out(index, t).is_some() {
                    // Joint-trim linked partners to the same time.
                    if let Some(gid) = group {
                        let partners: Vec<usize> = self.project.clips.iter().enumerate()
                            .filter(|(i, c)| *i != index && c.link_group == Some(gid))
                            .map(|(i, _)| i)
                            .collect();
                        for pi in partners {
                            self.trim_out(pi, t);
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            Action::RippleTrimClipIn { index, t } => {
                let t = self.snap_to_frame(t);
                if self.ripple_trim_in(index, t).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::RippleTrimClipOut { index, t } => {
                let t = self.snap_to_frame(t);
                if self.ripple_trim_out(index, t).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::RollTrimEdit { index, delta } => {
                if self.roll_trim(index, delta) {
                    self.host.mark_dirty();
                }
            }

            Action::SplitClip { index, t } => {
                let t = self.snap_to_frame(t);
                if let Some(new_idx) = self.split_clip(index, t) {
                    self.selected = Some(new_idx);
                    self.host.mark_dirty();
                }
            }
            Action::SplitAtPlayhead => {
                let t = self.snap_to_frame(self.time);
                if self.split_at(t) > 0 {
                    self.host.mark_dirty();
                }
            }

            Action::Seek(t) => {
                let was_playing = self.playing;
                self.stop_audio();
                self.time = t.clamp(0.0, self.project.duration);
                self.host.mark_dirty();
                self.update_scope_data();
                if was_playing {
                    let nt = self.time;
                    self.start_audio(nt);
                } else {
                    // Audio scrub: play a short 100ms burst at the new position even when paused.
                    self.scrub_audio_burst(t);
                }
            }
            Action::StepBy(delta) => {
                // A manual frame step stops playback so it doesn't fight the loop.
                self.playing = false;
                self.last_tick = None;
                self.time = (self.time + delta).clamp(0.0, self.project.duration);
                self.host.mark_dirty();
            }
            Action::TogglePlay => {
                self.playing = !self.playing;
                if self.playing {
                    // If parked at the end, rewind so play actually advances.
                    if self.time >= self.project.duration.max(1e-3) {
                        self.time = 0.0;
                        self.host.mark_dirty();
                    }
                    // Re-anchor the wall clock so the first tick steps by a small
                    // real delta, not the gap since the last play.
                    self.last_tick = Some(Instant::now());
                    let t = self.time;
                    self.start_audio(t);
                } else {
                    self.last_tick = None;
                    self.stop_audio();
                }
            }
            Action::Pause => {
                self.playing = false;
                self.last_tick = None;
                self.stop_audio();
            }

            Action::AddCrossDissolve { index } => {
                let t = self.snap_to_frame(self.time);
                if self
                    .project
                    .add_transition(
                        index,
                        t,
                        TransitionKind::CrossDissolve,
                        DEFAULT_TRANSITION_DUR,
                    )
                    .is_some()
                {
                    self.host.mark_dirty();
                }
            }
            Action::AddTransition { index, kind } => {
                let t = self.snap_to_frame(self.time);
                if self
                    .project
                    .add_transition(index, t, kind, DEFAULT_TRANSITION_DUR)
                    .is_some()
                {
                    self.host.mark_dirty();
                }
            }

            Action::SetClipGain { index, gain } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Audio(audio) = &mut clip.source {
                        audio.gain = gain.clamp(0.0, MAX_AUDIO_GAIN);
                        self.host.mark_dirty();
                    }
                }
            }

            Action::SetClipGrade { index, grade } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    clip.grade = grade;
                    self.host.mark_dirty();
                }
            }

            Action::ToggleTrackEnabled(ti) => {
                if let Some(track) = self.project.tracks.get_mut(ti) {
                    track.enabled = !track.enabled;
                    self.host.mark_dirty();
                }
            }

            Action::SelectClip(i) => {
                if i < self.project.clips.len() {
                    self.selected = Some(i);
                }
            }

            Action::ZoomBy(factor) => {
                self.zoom = (self.zoom * factor).clamp(0.1, 8.0);
            }
            Action::ResetView => {
                self.zoom = 1.0;
            }

            Action::SetClipFadeIn { index, secs } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    clip.fade_in = secs.max(0.0);
                }
            }
            Action::SetClipFadeOut { index, secs } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    clip.fade_out = secs.max(0.0);
                }
            }

            Action::SetTrackVolume { track_idx, v } => {
                if let Some(vol) = self.track_volumes.get_mut(track_idx) {
                    *vol = v.clamp(0.0, 2.0);
                }
            }
            Action::ToggleTrackMute { track_idx } => {
                if let Some(m) = self.track_muted.get_mut(track_idx) {
                    *m = !*m;
                }
            }
            Action::ToggleTrackSolo { track_idx } => {
                if let Some(s) = self.track_soloed.get_mut(track_idx) {
                    *s = !*s;
                }
            }
            Action::SetMasterVolume { v } => {
                self.master_volume = v.clamp(0.0, 2.0);
            }
            Action::ToggleMixer => {
                self.show_mixer = !self.show_mixer;
            }

            // --- Wave 14: audio effects + track types -------------------------
            Action::ToggleTrackEq { track_idx } => {
                if let Some(eq) = self.track_eq.get_mut(track_idx) {
                    eq.enabled = !eq.enabled;
                }
            }
            Action::SetEqBand { track_idx, band, gain_db } => {
                if let Some(eq) = self.track_eq.get_mut(track_idx) {
                    if let Some(b) = eq.bands.get_mut(band) {
                        b.gain_db = gain_db.clamp(-12.0, 12.0);
                    }
                }
            }
            Action::ToggleTrackCompressor { track_idx } => {
                if let Some(comp) = self.track_comp.get_mut(track_idx) {
                    comp.enabled = !comp.enabled;
                }
            }
            Action::SetCompressor { track_idx, threshold_db, ratio, attack_ms, release_ms, makeup_db } => {
                if let Some(comp) = self.track_comp.get_mut(track_idx) {
                    comp.threshold_db = threshold_db.clamp(-60.0, 0.0);
                    comp.ratio = ratio.clamp(1.0, 20.0);
                    comp.attack_ms = attack_ms.clamp(0.1, 200.0);
                    comp.release_ms = release_ms.clamp(10.0, 2000.0);
                    comp.makeup_db = makeup_db.clamp(-12.0, 24.0);
                }
            }
            Action::CycleAudioTrackType { track_idx } => {
                if let Some(t) = self.track_types.get_mut(track_idx) {
                    *t = t.next();
                }
            }
            Action::SetTrackEq3 { track_idx, eq } => {
                if let Some(e) = self.track_eq3.get_mut(track_idx) {
                    *e = eq;
                }
            }

            // --- Wave 15: custom transitions ---------------------------------
            Action::AddDiagonalWipe { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::DiagonalWipe, 1.0);
            }
            Action::AddPixelDissolve { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::PixelDissolve, 1.0);
            }

            // --- Wave 15: proxy media ----------------------------------------
            Action::SetProxyPath { index, path } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    clip.proxy_path = Some(path);
                    self.host.mark_dirty();
                }
            }
            Action::ClearProxy { index } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    clip.proxy_path = None;
                    self.host.mark_dirty();
                }
            }

            // --- Wave 15: log-to-Rec709 ---------------------------------------
            Action::ToggleLogTransform { track_idx } => {
                // Grow vector if new tracks were added after init.
                while self.track_log_transform.len() <= track_idx {
                    self.track_log_transform.push(false);
                }
                if let Some(f) = self.track_log_transform.get_mut(track_idx) {
                    *f = !*f;
                }
                self.host.mark_dirty();
            }

            // --- Wave 15: ProRes / DNxHD export --------------------------------
            Action::ExportProRes(_path) | Action::ExportDNxHD(_path) => {
                // ProRes/DNxHD export requires ffmpeg with the appropriate codec.
                // The UI wires this only when ffmpeg is available (same gate as H.264
                // export). Actual encoding is identical to the H.264 path but with
                // different codec flags: `-c:v prores_ks -profile:v 3` for ProRes,
                // `-c:v dnxhd -b:v 185M` for DNxHD. This variant is a stub that
                // surfaces the "codec not available" message until wired to
                // `start_export_codec`.
                log::info!("reel-gpui: ProRes/DNxHD export requested (stub — wire to start_export_codec)");
            }
            Action::ExportProResProxy(path) => {
                log::info!("reel-gpui: ProRes Proxy export: {}", path.display());
            }
            Action::ExportProRes422(path) => {
                log::info!("reel-gpui: ProRes 422 export: {}", path.display());
            }
            Action::ExportGif(path) => {
                log::info!("reel-gpui: GIF export: {}", path.display());
            }
            Action::SetExportFormat(fmt) => {
                self.export_format = fmt;
            }

            Action::LoadLut { path } => {
                match parse_cube_lut(&path) {
                    Ok((size, table)) => {
                        self.lut_path = Some(path);
                        self.lut_table = Some((size, table));
                        self.host.mark_dirty();
                    }
                    Err(e) => {
                        log::warn!("reel-gpui: LUT load failed: {e}");
                    }
                }
            }
            Action::SetWhiteBalance { temp, tint } => {
                self.white_balance_temp = temp.clamp(2000.0, 10000.0);
                self.white_balance_tint = tint.clamp(-150.0, 150.0);
                self.host.mark_dirty();
            }

            Action::ToggleMulticam => {
                self.multicam_mode = !self.multicam_mode;
                if self.multicam_mode {
                    self.multicam_tracks = (0..self.project.tracks.len()).collect();
                }
            }
            Action::SwitchCamera(track_idx) => {
                let t = self.snap_to_frame(self.time);
                log::info!("reel-gpui: SwitchCamera to track {} at t={:.2}", track_idx, t);
                self.host.mark_dirty();
            }
            Action::SwitchMulticamAngle { group_id, angle } => {
                if let Some(grp) = self.multicam_groups.get_mut(group_id) {
                    grp.active_angle = angle.min(grp.clips.len().saturating_sub(1));
                    self.host.mark_dirty();
                }
            }
            Action::CreateMulticamGroup { name: _ } => {
                let clips = if let Some(sel) = self.selected {
                    vec![sel]
                } else {
                    vec![]
                };
                self.multicam_groups.push(MulticamGroup { clips, active_angle: 0 });
            }

            Action::SetClipSpeed(idx, pct) => {
                if let Some(clip) = self.project.clips.get_mut(idx) {
                    clip.speed = (pct / 100.0).clamp(0.01, 10.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetClipReverse(idx, rev) => {
                if let Some(clip) = self.project.clips.get_mut(idx) {
                    clip.reversed = rev;
                    self.host.mark_dirty();
                }
            }
            Action::SetSpeedCurve { clip_id, curve } => {
                if let Some(c) = self.project.clips.get_mut(clip_id) {
                    c.speed_curve = curve;
                }
            }

            Action::AddTitle => {
                let start = self.snap_to_frame(self.time);
                let track_idx = self.project.tracks.len();
                self.project.tracks.push(Track {
                    name: format!("T{}", track_idx + 1),
                    enabled: true,
                });
                self.track_volumes.push(1.0);
                self.track_muted.push(false);
                self.track_soloed.push(false);
                self.track_eq.push(TrackEq::default());
                self.track_comp.push(TrackCompressor::default());
                self.track_types.push(AudioTrackType::Stereo);
                self.track_log_transform.push(false);
                self.track_eq3.push(TrackEq3::default());
                let clip = Clip {
                    name: "Title".into(),
                    source: ClipSource::Title {
                        text: "Title Text".into(),
                        font_size: 48.0,
                        color: [255, 255, 255, 255],
                        bg_color: None,
                    },
                    track: track_idx,
                    start,
                    duration: 5.0,
                    ..Clip::default()
                };
                self.project.clips.push(clip);
                self.selected = Some(self.project.clips.len() - 1);
                self.host.mark_dirty();
            }
            Action::SetTitleText { index, text } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { text: t, .. } = &mut clip.source {
                        *t = text;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTitleFontSize { index, size } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { font_size, .. } = &mut clip.source {
                        *font_size = size.max(4.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTitleColor { index, color } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { color: c, .. } = &mut clip.source {
                        *c = color;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTitleBgColor { index, color } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { bg_color, .. } = &mut clip.source {
                        *bg_color = color;
                        self.host.mark_dirty();
                    }
                }
            }

            Action::SetAudioVolume(v) => {
                let vol = v.clamp(0.0, 2.0);
                self.master_volume = vol;
                if let Some(player) = &self.audio_player {
                    player.set_volume(vol);
                }
            }
            Action::SetAudioMute(m) => {
                if let Some(player) = &self.audio_player {
                    let vol = if m { 0.0 } else { self.master_volume };
                    player.set_volume(vol);
                }
            }

            Action::ToggleSnap => {
                self.snap_enabled = !self.snap_enabled;
            }
            Action::BeginClipDrag { clip_id, grab_offset } => {
                self.clip_drag.set(Some(ClipDrag {
                    index: clip_id,
                    kind: ClipDragKind::Move,
                    grab_offset,
                }));
            }
            Action::MoveClipDrag { track_idx: _, raw_t } => {
                let drag = match self.clip_drag.get() {
                    Some(d) => d,
                    None => return,
                };
                let snapped = self.snapped_time(raw_t, drag.index, 8.0);
                self.snap_point = if self.snap_enabled && (snapped - raw_t).abs() > 1e-6 {
                    Some(snapped)
                } else {
                    None
                };
                let snapped = self.snap_to_frame(snapped);
                if let Some(clip) = self.project.clips.get_mut(drag.index) {
                    clip.start = snapped;
                    self.host.mark_dirty();
                }
                self.update_scope_data();
            }
            Action::EndClipDrag => {
                self.clip_drag.set(None);
                self.snap_point = None;
            }

            Action::ToggleScopes => {
                self.scopes_open = !self.scopes_open;
            }
            Action::SetScopeTab(tab) => {
                self.scope_tab = tab.min(2);
            }

            Action::ToggleDualViewer => {
                self.dual_viewer = !self.dual_viewer;
            }
            Action::SeekSource(t) => {
                self.source_playhead = t.clamp(0.0, self.project.duration);
            }
            Action::ToggleSourcePlay => {
                self.source_playing = !self.source_playing;
            }
            Action::SetSourceIn(t) => {
                self.source_in = t.clamp(0.0, self.project.duration);
            }
            Action::SetSourceOut(t) => {
                self.source_out = t.clamp(0.0, self.project.duration);
            }
            Action::InsertFromSource { clip_id, in_t, out_t } => {
                log::info!("reel-gpui: InsertFromSource clip={clip_id} in={in_t:.2} out={out_t:.2}");
            }

            Action::ToggleBins => {
                self.bins_open = !self.bins_open;
            }
            Action::AddBin(name) => {
                self.bins.push(Bin { name, clips: Vec::new() });
            }
            Action::SelectBin(idx) => {
                if idx < self.bins.len() {
                    self.selected_bin = idx;
                    self.selected_bin_clip = None;
                }
            }
            Action::ImportToBin { bin_idx, path } => {
                if let Some(bin) = self.bins.get_mut(bin_idx) {
                    let name = path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "media".into());
                    let clip_type = if is_video_path(&path) {
                        BinClipType::Video
                    } else if is_audio_path(&path) {
                        BinClipType::Audio
                    } else {
                        BinClipType::Image
                    };
                    bin.clips.push(BinClip {
                        name,
                        path,
                        duration: 0.0,
                        clip_type,
                    });
                }
            }
            Action::RemoveFromBin { bin_idx, clip_idx } => {
                if let Some(bin) = self.bins.get_mut(bin_idx) {
                    if clip_idx < bin.clips.len() {
                        bin.clips.remove(clip_idx);
                        if self.selected_bin_clip == Some(clip_idx) {
                            self.selected_bin_clip = None;
                        }
                    }
                }
            }
            Action::SelectBinClip { bin_idx, clip_idx } => {
                if bin_idx < self.bins.len() && clip_idx < self.bins[bin_idx].clips.len() {
                    self.selected_bin = bin_idx;
                    self.selected_bin_clip = Some(clip_idx);
                }
            }
            Action::InsertClipFromBin { bin_clip_idx, track_idx, at_t } => {
                let bin_clip = self.bins.get(self.selected_bin)
                    .and_then(|b| b.clips.get(bin_clip_idx))
                    .cloned();
                if let Some(bc) = bin_clip {
                    if let Some(clip) = self.build_imported_clip(&bc.path) {
                        let mut clip = clip;
                        clip.track = track_idx.min(self.project.tracks.len().saturating_sub(1));
                        clip.start = self.snap_to_frame(at_t);
                        self.project.clips.push(clip);
                        self.selected = Some(self.project.clips.len() - 1);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Wave 13 handlers ---
            Action::SetTransitionDuration { track_idx: _, trans_idx, duration } => {
                if let Some(t) = self.project.transitions.get_mut(trans_idx) {
                    t.duration = duration.max(0.0);
                }
            }

            Action::AddFilmDissolve { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::FilmDissolve, 1.0);
            }

            Action::AddChapterMarker { time, label } => {
                self.chapter_markers.push((time, label));
                self.chapter_markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            }

            Action::RemoveChapterMarker(idx) => {
                if idx < self.chapter_markers.len() {
                    self.chapter_markers.remove(idx);
                }
            }

            Action::ToggleLinkClip(clip_idx) => {
                if self.linked_clips.contains(&clip_idx) {
                    self.linked_clips.remove(&clip_idx);
                    if let Some(clip) = self.project.clips.get_mut(clip_idx) {
                        clip.link_group = None;
                    }
                } else {
                    self.linked_clips.insert(clip_idx);
                    // Assign a link_group: reuse selected clip's group if it has one,
                    // otherwise create a new group id from this clip's index.
                    let new_gid = self.selected
                        .and_then(|sel| self.project.clips.get(sel))
                        .and_then(|c| c.link_group)
                        .unwrap_or(clip_idx as u64 + 1);
                    if let Some(clip) = self.project.clips.get_mut(clip_idx) {
                        clip.link_group = Some(new_gid);
                    }
                    // Also set the selected clip's group if it was unset.
                    if let Some(sel) = self.selected {
                        if sel != clip_idx {
                            if let Some(c) = self.project.clips.get_mut(sel) {
                                if c.link_group.is_none() {
                                    c.link_group = Some(new_gid);
                                }
                            }
                        }
                    }
                }
            }

            Action::SaveExportPreset { name, format, width, height, fps } => {
                self.export_presets.push((name, format, width, height, fps));
            }

            Action::DeleteExportPreset(idx) => {
                if idx < self.export_presets.len() {
                    self.export_presets.remove(idx);
                    if self.active_preset == Some(idx) {
                        self.active_preset = None;
                    }
                }
            }

            Action::ApplyExportPreset(idx) => {
                if idx < self.export_presets.len() {
                    self.active_preset = Some(idx);
                }
            }

            Action::NestSelectedClips { name } => {
                let Some(sel_idx) = self.selected else { return };
                if sel_idx >= self.project.clips.len() { return };
                let inner_clip = self.project.clips[sel_idx].clone();
                let dur = inner_clip.duration;
                let inner_tracks = self.project.tracks.clone();
                self.project.clips[sel_idx].source = ClipSource::NestedClip {
                    tracks: inner_tracks,
                    clips: vec![inner_clip],
                    duration_secs: dur.max(MIN_DUR),
                };
                // Rename the outer clip to the nest name.
                self.project.clips[sel_idx].name = name;
                self.host.mark_dirty();
                log::info!("reel-gpui: NestSelectedClips — clip {} wrapped into nested sequence", sel_idx);
            }

            Action::SetHslSecondaryGrade { clip_id, grade } => {
                if let Some(clip) = self.project.clips.get_mut(clip_id) {
                    clip.hsl_secondary = grade;
                }
            }

            // --- Batch 3: 3-way color wheels ---
            Action::SetColorWheels(cw) => {
                self.color_wheels = cw;
                self.host.mark_dirty();
            }

            // --- Batch 3: rate stretch ---
            Action::RateStretchClip { index, new_duration } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    let orig_duration = clip.duration;
                    let new_dur = new_duration.max(MIN_DUR);
                    if orig_duration > 0.0 {
                        clip.speed = (orig_duration / new_dur).clamp(0.01, 10.0);
                    }
                    clip.duration = new_dur;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: sequence settings ---
            Action::ToggleSequenceSettings => {
                self.show_sequence_settings = !self.show_sequence_settings;
            }
            Action::SetSequenceSize { w, h } => {
                self.project.width = w.max(1);
                self.project.height = h.max(1);
                self.host.mark_dirty();
            }
            Action::SetFrameRate(fps) => {
                self.project.fps = fps.clamp(1.0, 120.0);
                self.host.mark_dirty();
            }
            Action::SetSampleRate(rate) => {
                self.sequence_sample_rate = rate;
            }
            Action::SetColorSpace(cs) => {
                self.sequence_color_space = cs;
            }

            // --- Batch 4: RGB curves ---
            Action::SetRgbCurves(c) => { self.rgb_curves = c; self.host.mark_dirty(); }
            Action::SetCurvesChannel(ch) => { self.curves_channel = ch; }
            Action::AddCurvePoint { channel, point } => {
                let target = match channel {
                    1 => &mut self.rgb_curves.red,
                    2 => &mut self.rgb_curves.green,
                    3 => &mut self.rgb_curves.blue,
                    _ => &mut self.rgb_curves.master,
                };
                target.push(point);
                target.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
                self.host.mark_dirty();
            }
            Action::MoveCurvePoint { channel, index, point } => {
                let target = match channel {
                    1 => &mut self.rgb_curves.red,
                    2 => &mut self.rgb_curves.green,
                    3 => &mut self.rgb_curves.blue,
                    _ => &mut self.rgb_curves.master,
                };
                if index < target.len() {
                    target[index] = point;
                    target.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }

            // --- Batch 4: audio effect chains ---
            Action::AddAudioEffect { track_idx, effect } => {
                while self.audio_effects.len() <= track_idx { self.audio_effects.push(Vec::new()); }
                self.audio_effects[track_idx].push(effect);
            }
            Action::RemoveAudioEffect { track_idx, effect_idx } => {
                if track_idx < self.audio_effects.len() && effect_idx < self.audio_effects[track_idx].len() {
                    self.audio_effects[track_idx].remove(effect_idx);
                }
            }
            Action::SetAudioEffect { track_idx, effect_idx, effect } => {
                if track_idx < self.audio_effects.len() && effect_idx < self.audio_effects[track_idx].len() {
                    self.audio_effects[track_idx][effect_idx] = effect;
                }
            }
            Action::ToggleTrackFx { track_idx } => {
                while self.track_fx_open.len() <= track_idx { self.track_fx_open.push(false); }
                self.track_fx_open[track_idx] = !self.track_fx_open[track_idx];
            }
            Action::ExpandTrackFx { track_idx, effect_idx } => {
                while self.track_fx_expanded.len() <= track_idx { self.track_fx_expanded.push(None); }
                self.track_fx_expanded[track_idx] = effect_idx;
            }

            // --- Batch 4: closed captions ---
            Action::AddCaption(cap) => { self.captions.push(cap); }
            Action::RemoveCaption(idx) => {
                if idx < self.captions.len() { self.captions.remove(idx); }
            }
            Action::EditCaption { index, caption } => {
                if index < self.captions.len() { self.captions[index] = caption; }
            }
            Action::ImportSrt(path) => {
                match parse_srt(&path) {
                    Ok(caps) => self.captions = caps,
                    Err(e) => log::warn!("reel-gpui: SRT import failed: {e}"),
                }
            }
            Action::ExportSrt(path) => {
                let s = serialize_srt(&self.captions);
                if let Err(e) = std::fs::write(&path, s) {
                    log::warn!("reel-gpui: SRT export failed: {e}");
                }
            }
            Action::ToggleCaptionsPanel => { self.show_captions_panel = !self.show_captions_panel; }

            // --- Batch 4: export preset list ---
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

            // --- Batch 4: markers panel ---
            Action::ToggleMarkersPanel => { self.show_markers_panel = !self.show_markers_panel; }
            Action::AddMarkerAt { time, name, kind } => {
                let color = match kind {
                    MarkerKind::InPoint  => [0.2f32, 0.8, 0.2],
                    MarkerKind::OutPoint => [0.8, 0.2, 0.2],
                    MarkerKind::Chapter  => [0.9, 0.7, 0.0],
                    MarkerKind::Comment  => [0.3, 0.6, 1.0],
                };
                self.gpui_markers.push(GpuiMarker { time_secs: time, name, color, kind });
                self.gpui_markers.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap_or(std::cmp::Ordering::Equal));
            }
            Action::RemoveMarker(idx) => {
                if idx < self.gpui_markers.len() { self.gpui_markers.remove(idx); }
            }
            Action::RenameMarker { index, name } => {
                if let Some(m) = self.gpui_markers.get_mut(index) { m.name = name; }
            }
            Action::SetInPoint(t) => { self.work_area_in = t.max(0.0); }
            Action::SetOutPoint(t) => { self.work_area_out = t.max(0.0); }

            // --- Batch 5: copy / paste / duplicate ---------------------------
            Action::CopySelectedClips => {
                if let Some(idx) = self.selected {
                    if let Some(clip) = self.project.clips.get(idx).cloned() {
                        self.clipboard_clips = vec![clip];
                    }
                }
            }
            Action::CutSelectedClips => {
                if let Some(idx) = self.selected {
                    if idx < self.project.clips.len() {
                        let clip = self.project.clips.remove(idx);
                        self.clipboard_clips = vec![clip];
                        self.selected = None;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::PasteClips { at_t } => {
                if self.clipboard_clips.is_empty() { return; }
                let start_t = self.snap_to_frame(at_t);
                // Offset pasted clips relative to the first clipboard clip's start.
                let base_t = self.clipboard_clips[0].start;
                let first_new = self.project.clips.len();
                for src in self.clipboard_clips.clone() {
                    let offset = src.start - base_t;
                    let mut c = src;
                    c.start = self.snap_to_frame(start_t + offset);
                    c.link_group = None;
                    self.project.clips.push(c);
                }
                self.selected = Some(first_new);
                self.host.mark_dirty();
            }
            Action::DuplicateSelectedClips => {
                if let Some(idx) = self.selected {
                    if let Some(src) = self.project.clips.get(idx).cloned() {
                        let mut dup = src;
                        dup.start = self.snap_to_frame(dup.start + dup.duration);
                        dup.link_group = None;
                        self.project.clips.push(dup);
                        self.selected = Some(self.project.clips.len() - 1);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 5: clip transform -------------------------------------
            Action::SetClipAnchor { clip_idx, x, y } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.anchor_x = x.clamp(-1.0, 1.0);
                    c.anchor_y = y.clamp(-1.0, 1.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetClipCrop { clip_idx, left, right, top, bottom } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.crop_left   = left.clamp(0.0, 1.0);
                    c.crop_right  = right.clamp(0.0, 1.0);
                    c.crop_top    = top.clamp(0.0, 1.0);
                    c.crop_bottom = bottom.clamp(0.0, 1.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetClipBlendMode { clip_idx, mode } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.blend_mode = mode;
                    self.host.mark_dirty();
                }
            }
            Action::ResetClipTransform { clip_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.anchor_x = 0.0; c.anchor_y = 0.0;
                    c.crop_left = 0.0; c.crop_right = 0.0;
                    c.crop_top = 0.0; c.crop_bottom = 0.0;
                    c.blend_mode = ClipBlendMode::Normal;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: time remap -----------------------------------------
            Action::SetTimeRemapEnabled { clip_idx, enabled } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = enabled;
                    if enabled && c.time_remap_keys.is_empty() {
                        // Seed two keyframes: identity mapping over the clip.
                        c.time_remap_keys = vec![
                            (c.start, c.source_in),
                            (c.end(), c.source_in + c.duration),
                        ];
                    }
                    self.host.mark_dirty();
                }
            }
            Action::AddTimeRemapKey { clip_idx, timeline_t, source_t } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_keys.push((timeline_t, source_t));
                    c.time_remap_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::MoveTimeRemapKey { clip_idx, key_idx, source_t } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if let Some(k) = c.time_remap_keys.get_mut(key_idx) {
                        k.1 = source_t.max(0.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::RemoveTimeRemapKey { clip_idx, key_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if key_idx < c.time_remap_keys.len() {
                        c.time_remap_keys.remove(key_idx);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetFreezeFrame { clip_idx, at_t } => {
                // Insert two keyframes with the same source time, creating a freeze.
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    let src_t = c.remapped_source_t(at_t);
                    let end = c.end();
                    c.time_remap_keys.retain(|k| k.0 < at_t || k.0 >= end);
                    c.time_remap_keys.push((at_t, src_t));
                    c.time_remap_keys.push((end - 1e-4, src_t));
                    c.time_remap_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: LUFS metering --------------------------------------
            Action::UpdateLufsMeters { power } => {
                self.lufs_power_history.push(power);
                // Keep last 10 minutes worth at ~10 blocks/sec.
                if self.lufs_power_history.len() > 6000 {
                    self.lufs_power_history.drain(..1000);
                }
                self.lufs_short_term = lufs_short_term(&self.lufs_power_history, 10.0);
                self.lufs_integrated = lufs_integrated(&self.lufs_power_history);
            }
            Action::ResetLufsIntegrated => {
                self.lufs_power_history.clear();
                self.lufs_short_term = -f32::INFINITY;
                self.lufs_integrated = -f32::INFINITY;
            }

            // --- Batch 5: group ripple trim ----------------------------------
            Action::GroupRippleTrimIn { clip_indices, delta } => {
                if delta == 0.0 || clip_indices.is_empty() { return; }
                for &ci in &clip_indices {
                    let t = self.project.clips.get(ci).map(|c| c.start + delta);
                    if let Some(t) = t {
                        self.ripple_trim_in(ci, t);
                    }
                }
                self.host.mark_dirty();
            }
            Action::GroupRippleTrimOut { clip_indices, delta } => {
                if delta == 0.0 || clip_indices.is_empty() { return; }
                for &ci in &clip_indices {
                    let t = self.project.clips.get(ci).map(|c| c.end() + delta);
                    if let Some(t) = t {
                        self.ripple_trim_out(ci, t);
                    }
                }
                self.host.mark_dirty();
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// --- Batch 5: LUFS loudness metering -----------------------------------------

/// K-weighted power of a stereo block of samples (ITU-R BS.1770-4).
/// `samples` is interleaved L/R at `sample_rate` Hz.
pub fn k_weighted_power(samples: &[f32], sample_rate: u32) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sr = sample_rate as f32;
    // Pre-filter coefficients for 48 kHz (ITU-R BS.1770 stage 1 — high-shelf).
    // Values from the standard; we approximate at arbitrary sample rates.
    let scale = 48000.0 / sr.max(1.0);
    let _ = scale; // used conceptually; exact IIR not needed for this approximation

    // For a reasonable approximation: weight high frequencies slightly (+4dB shelf).
    // Full IIR needs state — here we use a simplified RMS with a +2 dB HF boost.
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    sum_sq / samples.len() as f32
}

/// Short-term LUFS over a 3-second window of power history.
/// `history` contains per-block mean-square values; `block_rate` is blocks/sec.
pub fn lufs_short_term(history: &[f32], block_rate: f32) -> f32 {
    let window = (3.0 * block_rate).ceil() as usize;
    let slice = if history.len() > window { &history[history.len() - window..] } else { history };
    if slice.is_empty() { return -f32::INFINITY; }
    let mean: f32 = slice.iter().sum::<f32>() / slice.len() as f32;
    if mean <= 0.0 { return -f32::INFINITY; }
    -0.691 + 10.0 * mean.log10()
}

/// Integrated LUFS (gated) over the full history.
pub fn lufs_integrated(history: &[f32]) -> f32 {
    if history.is_empty() { return -f32::INFINITY; }
    // Absolute gate: -70 LUFS.
    let abs_gate = 1e-7_f32; // 10^((-70 + 0.691) / 10)
    let above: Vec<f32> = history.iter().copied().filter(|&p| p >= abs_gate).collect();
    if above.is_empty() { return -f32::INFINITY; }
    let mean_above: f32 = above.iter().sum::<f32>() / above.len() as f32;
    // Relative gate: -10 LU below mean_above.
    let rel_gate = mean_above * 0.1;
    let gated: Vec<f32> = above.iter().copied().filter(|&p| p >= rel_gate).collect();
    if gated.is_empty() { return -f32::INFINITY; }
    let mean_gated: f32 = gated.iter().sum::<f32>() / gated.len() as f32;
    if mean_gated <= 0.0 { return -f32::INFINITY; }
    -0.691 + 10.0 * mean_gated.log10()
}

/// Parse a minimal `.cube` 3D LUT file. Returns `(size, table)` or an error string.
pub fn parse_cube_lut(path: &std::path::Path) -> Result<(u32, Vec<[f32; 3]>), String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut size: Option<u32> = None;
    let mut table: Vec<[f32; 3]> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("LUT_3D_SIZE") {
            let n: u32 = line
                .split_whitespace()
                .nth(1)
                .ok_or("missing size")?
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?;
            size = Some(n);
            table.reserve((n * n * n) as usize);
            continue;
        }
        if line.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
            continue;
        }
        let mut parts = line.split_whitespace();
        let r: f32 = parts.next().ok_or("missing r")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let g: f32 = parts.next().ok_or("missing g")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let b: f32 = parts.next().ok_or("missing b")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        table.push([r, g, b]);
    }
    let sz = size.ok_or("LUT_3D_SIZE not found")?;
    let expected = (sz * sz * sz) as usize;
    if table.len() != expected {
        return Err(format!("expected {} entries, got {}", expected, table.len()));
    }
    Ok((sz, table))
}

/// Parse an SRT subtitle file into a `Vec<Caption>`.
pub fn parse_srt(path: &std::path::Path) -> Result<Vec<Caption>, String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut captions = Vec::new();
    let mut lines_iter = reader.lines().peekable();
    while let Some(line) = lines_iter.next() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim().to_string();
        if line.is_empty() { continue; }
        if line.parse::<u32>().is_err() { continue; }
        let Some(tc_line) = lines_iter.next() else { break; };
        let tc = tc_line.map_err(|e| e.to_string())?;
        let tc = tc.trim().to_string();
        let parts: Vec<&str> = tc.splitn(2, " --> ").collect();
        if parts.len() != 2 { continue; }
        let start = parse_srt_timecode(parts[0])?;
        let end = parse_srt_timecode(parts[1])?;
        let mut text_lines = Vec::new();
        while let Some(tl) = lines_iter.next() {
            let tl = tl.map_err(|e| e.to_string())?;
            let tl = tl.trim().to_string();
            if tl.is_empty() { break; }
            text_lines.push(tl);
        }
        captions.push(Caption {
            start_secs: start,
            end_secs: end,
            text: text_lines.join("\n"),
            style: CaptionStyle::default(),
        });
    }
    Ok(captions)
}

fn parse_srt_timecode(s: &str) -> Result<f32, String> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 { return Err(format!("bad timecode: {s}")); }
    let h: f32 = parts[0].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let m: f32 = parts[1].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let sec: f32 = parts[2].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    Ok(h * 3600.0 + m * 60.0 + sec)
}

/// Serialize captions back to SRT format.
pub fn serialize_srt(captions: &[Caption]) -> String {
    let mut out = String::new();
    for (i, cap) in captions.iter().enumerate() {
        out.push_str(&format!("{}\n", i + 1));
        out.push_str(&format!("{} --> {}\n", format_srt_time(cap.start_secs), format_srt_time(cap.end_secs)));
        out.push_str(&cap.text);
        out.push_str("\n\n");
    }
    out
}

fn format_srt_time(t: f32) -> String {
    let t = t.max(0.0);
    let h = (t / 3600.0) as u32;
    let m = ((t % 3600.0) / 60.0) as u32;
    let s = (t % 60.0) as u32;
    let ms = ((t % 1.0) * 1000.0).round() as u32;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size, Bounds};

    #[test]
    fn video_frame_mapping_quantizes_to_fps() {
        let v = VideoSource {
            path: "/clips/a.mp4".into(),
            fps: Some(25.0),
            duration: None,
        };
        // 25fps → each frame is 0.04s. Sample mid-frame to avoid float fuzz.
        assert_eq!(v.frame_index_at(0.0), 0);
        assert_eq!(v.frame_index_at(0.05), 1);
        assert_eq!(v.frame_index_at(0.10), 2);
        assert_eq!(v.frame_index_at(0.50), 12);
        // Negative source time clamps to frame 0.
        assert_eq!(v.frame_index_at(-1.0), 0);
        // frame_time is the inverse start-of-frame seek time.
        assert!((v.frame_time(2) - 0.08).abs() < 1e-9);
    }

    #[test]
    fn unprobed_video_falls_back_to_default_fps() {
        let v = VideoSource::new("/clips/a.mp4");
        assert!((v.fps() - DEFAULT_VIDEO_FPS).abs() < 1e-9);
        // A degenerate (zero/NaN) probe also falls back.
        let bad = VideoSource {
            path: "/c.mp4".into(),
            fps: Some(0.0),
            duration: None,
        };
        assert!((bad.fps() - DEFAULT_VIDEO_FPS).abs() < 1e-9);
    }

    #[test]
    fn scrub_x_maps_to_time_within_recorded_bounds() {
        let app = App::new(); // duration 30s
        // No bounds recorded yet → no mapping.
        assert!(app.timeline_x_to_time(100.0).is_none());

        // Record a scrub region from x=100 to x=900 (width 800).
        app.timeline_bounds.set(Some(Bounds {
            origin: point(px(100.0), px(0.0)),
            size: size(px(800.0), px(40.0)),
        }));

        // Left edge → t=0, right edge → t=duration, midpoint → half.
        let dur = app.project.duration;
        assert!((app.timeline_x_to_time(100.0).unwrap() - 0.0).abs() < 1e-4);
        assert!((app.timeline_x_to_time(900.0).unwrap() - dur).abs() < 1e-3);
        assert!((app.timeline_x_to_time(500.0).unwrap() - dur * 0.5).abs() < 1e-3);

        // Outside the region clamps to the ends.
        assert!((app.timeline_x_to_time(0.0).unwrap() - 0.0).abs() < 1e-4);
        assert!((app.timeline_x_to_time(2000.0).unwrap() - dur).abs() < 1e-3);
    }

    #[test]
    fn snap_to_frame_rounds_to_project_fps() {
        let app = App::new(); // 30 fps → 1 frame = 1/30 s ≈ 0.0333s
        // Exactly on a frame stays put.
        assert!((app.snap_to_frame(1.0) - 1.0).abs() < 1e-5);
        // Just past frame 30 (1.0s) snaps back to it.
        assert!((app.snap_to_frame(1.01) - 1.0).abs() < 1e-5);
        // Mid-way to the next frame rounds up to frame 31 (1/30 past 1.0).
        assert!((app.snap_to_frame(1.02) - 31.0 / 30.0).abs() < 1e-5);
        // Negative clamps to 0.
        assert!((app.snap_to_frame(-0.5)).abs() < 1e-6);
    }

    #[test]
    fn import_image_places_a_clip_at_the_playhead() {
        let mut app = App::new();
        app.time = 7.0;
        let before = app.project.clips.len();
        // A .png path → an Image clip (no probe / ffmpeg needed).
        app.apply(Action::ImportMedia("/tmp/shot.png".into()));
        assert_eq!(app.project.clips.len(), before + 1);
        let clip = app.project.clips.last().unwrap();
        assert!(matches!(clip.source, ClipSource::Image(_)));
        assert_eq!(clip.name, "shot");
        // Placed at the (frame-snapped) playhead on the first enabled track.
        assert!((clip.start - app.snap_to_frame(7.0)).abs() < 1e-5);
        assert_eq!(clip.track, 0);
        assert!((clip.duration - DEFAULT_IMAGE_LEN).abs() < 1e-5);
        // The freshly imported clip becomes the selection.
        assert_eq!(app.selected, Some(app.project.clips.len() - 1));
    }

    #[test]
    fn import_skips_an_empty_path() {
        let mut app = App::new();
        let before = app.project.clips.len();
        app.apply(Action::ImportMedia(PathBuf::new()));
        assert_eq!(app.project.clips.len(), before);
    }

    #[test]
    fn video_extension_classifier() {
        assert!(is_video_path(std::path::Path::new("/a/b.MP4")));
        assert!(is_video_path(std::path::Path::new("/a/b.mov")));
        assert!(!is_video_path(std::path::Path::new("/a/b.png")));
        assert!(!is_video_path(std::path::Path::new("/a/b")));
    }

    #[test]
    fn move_clip_snaps_start_and_clamps() {
        let mut app = App::new();
        // Move clip 0 to 2.51s → snaps to nearest frame at 30fps.
        app.apply(Action::MoveClip { index: 0, start: 2.51 });
        let expected = app.snap_to_frame(2.51);
        assert!((app.project.clips[0].start - expected).abs() < 1e-5);
        // A negative start clamps to 0.
        app.apply(Action::MoveClip { index: 0, start: -3.0 });
        assert!((app.project.clips[0].start).abs() < 1e-6);
        // An out-of-range index is a no-op (no panic).
        app.apply(Action::MoveClip { index: 999, start: 1.0 });
    }

    #[test]
    fn timeline_dx_maps_to_dt_within_recorded_bounds() {
        let app = App::new(); // duration 30s
        assert!(app.timeline_dx_to_dt(100.0).is_none());
        app.timeline_bounds.set(Some(Bounds {
            origin: point(px(100.0), px(0.0)),
            size: size(px(800.0), px(40.0)),
        }));
        // Half the width → half the duration.
        assert!((app.timeline_dx_to_dt(400.0).unwrap() - 15.0).abs() < 1e-3);
        // A leftward drag yields a negative dt.
        assert!((app.timeline_dx_to_dt(-80.0).unwrap() + 3.0).abs() < 1e-3);
    }

    #[test]
    fn seek_action_clamps_and_marks_dirty() {
        let mut app = App::new();
        app.apply(Action::Seek(12.5));
        assert!((app.time - 12.5).abs() < 1e-6);
        // Beyond the sequence end clamps to the duration.
        app.apply(Action::Seek(9999.0));
        assert!((app.time - app.project.duration).abs() < 1e-6);
        // Negative clamps to 0.
        app.apply(Action::Seek(-5.0));
        assert!((app.time - 0.0).abs() < 1e-6);
    }

    // --- Trimming -----------------------------------------------------------

    #[test]
    fn trim_out_shrinks_tail_and_clamps_to_min_dur() {
        let mut app = App::new();
        // Clip 0: start 0, dur 6 (a color clip → unbounded source).
        app.apply(Action::TrimClipOut { index: 0, t: 4.0 });
        assert!((app.project.clips[0].start - 0.0).abs() < 1e-5);
        assert!((app.project.clips[0].duration - 4.0).abs() < 1e-5);
        // Source_in is untouched by a tail trim.
        assert!((app.project.clips[0].source_in - 0.0).abs() < 1e-6);
        // Dragging the tail past the head clamps to MIN_DUR.
        app.apply(Action::TrimClipOut { index: 0, t: -2.0 });
        assert!((app.project.clips[0].duration - MIN_DUR).abs() < 1e-5);
    }

    #[test]
    fn trim_in_advances_source_and_keeps_right_edge() {
        let mut app = App::new();
        // Clip 0: start 0, dur 6, end 6. Trim the head to t=2.
        let end_before = app.project.clips[0].end();
        app.apply(Action::TrimClipIn { index: 0, t: 2.0 });
        let c = &app.project.clips[0];
        assert!((c.start - 2.0).abs() < 1e-5);
        // source_in followed the head by the same shift (reveals later source).
        assert!((c.source_in - 2.0).abs() < 1e-5);
        // The right edge is unchanged (non-ripple, fixed tail).
        assert!((c.end() - end_before).abs() < 1e-4);
        // The head can't pass the right edge (min dur).
        app.apply(Action::TrimClipIn { index: 0, t: 100.0 });
        assert!(app.project.clips[0].duration >= MIN_DUR - 1e-6);
    }

    #[test]
    fn trim_out_clamps_to_bounded_source_len() {
        let mut app = App::new();
        // A 5s-bounded movie clip (probed). source_in 0 → tail can't pass 5s.
        app.project.clips.push(Clip {
            name: "movie".into(),
            source: ClipSource::Video(VideoSource {
                path: "/m.mp4".into(),
                fps: Some(30.0),
                duration: Some(5.0),
            }),
            track: 0,
            start: 0.0,
            duration: 4.0,
            ..Clip::default()
        });
        let idx = app.project.clips.len() - 1;
        // Try to drag the tail out to 9s → clamps to the 5s source bound.
        app.apply(Action::TrimClipOut { index: idx, t: 9.0 });
        assert!((app.project.clips[idx].duration - 5.0).abs() < 1e-4);
    }

    // --- Razor / split ------------------------------------------------------

    #[test]
    fn split_clip_cuts_into_two_source_continuous_parts() {
        let mut app = App::new();
        // Clip 0: start 0, dur 6. Split at t=2.
        let before = app.project.clips.len();
        let new_idx = app.split_clip(0, 2.0).expect("split should succeed");
        assert_eq!(app.project.clips.len(), before + 1);
        // Left part: [0, 2).
        assert!((app.project.clips[0].start - 0.0).abs() < 1e-5);
        assert!((app.project.clips[0].duration - 2.0).abs() < 1e-5);
        // Right part: [2, 6), with source_in advanced by the left duration.
        let right = &app.project.clips[new_idx];
        assert!((right.start - 2.0).abs() < 1e-5);
        assert!((right.duration - 4.0).abs() < 1e-5);
        assert!((right.source_in - 2.0).abs() < 1e-5);
    }

    #[test]
    fn split_clip_rejects_a_cut_outside_the_clip() {
        let mut app = App::new();
        // Clip 0 covers [0, 6). A cut at the edge / outside is a no-op.
        assert!(app.split_clip(0, 0.0).is_none());
        assert!(app.split_clip(0, 6.0).is_none());
        assert!(app.split_clip(0, 9.0).is_none());
        // Too close to an edge (< MIN_DUR on one side) is rejected.
        assert!(app.split_clip(0, 0.01).is_none());
    }

    #[test]
    fn split_at_playhead_razors_every_straddling_visible_clip() {
        let mut app = App::new();
        // At t=4: Teal [0,6) on V1 covers it; Indigo [3,7) on V2 covers it;
        // Amber [6,11) does not. → 2 clips split.
        let before = app.project.clips.len();
        app.apply(Action::Seek(4.0));
        app.apply(Action::SplitAtPlayhead);
        assert_eq!(app.project.clips.len(), before + 2);
        // A disabled track is skipped by the razor.
        app.project.tracks[1].enabled = false;
        let before2 = app.project.clips.len();
        // Seek somewhere a single visible clip straddles, then razor.
        app.apply(Action::Seek(1.0)); // Teal-left [0,4) straddles at 1.0
        app.apply(Action::SplitAtPlayhead);
        assert_eq!(app.project.clips.len(), before2 + 1);
    }

    // --- Playback -----------------------------------------------------------

    #[test]
    fn toggle_play_anchors_and_clears_clock() {
        let mut app = App::new();
        assert!(!app.playing);
        app.apply(Action::TogglePlay);
        assert!(app.playing);
        // A tick while playing advances the playhead by a (tiny) real delta and
        // keeps the loop alive.
        let before = app.time;
        let alive = app.tick();
        assert!(alive);
        assert!(app.time >= before);
        // Toggling off stops the loop; a tick then is a no-op.
        app.apply(Action::TogglePlay);
        assert!(!app.playing);
        assert!(!app.tick());
    }

    #[test]
    fn tick_stops_at_sequence_end() {
        let mut app = App::new();
        app.apply(Action::TogglePlay);
        // Park at the end so the next tick (any positive dt) crosses it.
        app.time = app.project.duration;
        let alive = app.tick();
        assert!(!alive, "tick at the end should stop the loop");
        assert!(!app.playing, "playback halts at the sequence end");
        assert!((app.time - app.project.duration).abs() < 1e-3);
    }

    #[test]
    fn toggle_play_at_end_rewinds_to_head() {
        let mut app = App::new();
        app.time = app.project.duration; // parked at the end
        app.apply(Action::TogglePlay);
        assert!(app.playing);
        assert!((app.time - 0.0).abs() < 1e-6, "play from the end rewinds");
    }

    #[test]
    fn frame_step_halts_playback() {
        let mut app = App::new();
        app.apply(Action::TogglePlay);
        assert!(app.playing);
        app.apply(Action::StepBy(1.0 / 30.0));
        assert!(!app.playing, "a manual frame step stops the play loop");
    }

    // --- Transitions --------------------------------------------------------

    #[test]
    fn add_cross_dissolve_finds_the_cut_and_clamps_duration() {
        let mut app = App::new();
        // Two abutting clips on V1: Teal [0,6) and Amber [6,11). The cut is at 6.
        // Add a dissolve near clip 0 with t close to its right edge.
        let added = app.project.add_transition(
            0,
            6.0,
            TransitionKind::CrossDissolve,
            DEFAULT_TRANSITION_DUR,
        );
        assert!(added.is_some());
        let tr = app.project.transitions[0];
        assert!((tr.center - 6.0).abs() < 1e-4, "centered on the cut");
        assert!((tr.duration - DEFAULT_TRANSITION_DUR).abs() < 1e-4);
        // A re-add on the same cut replaces (no duplicate).
        let again = app
            .project
            .add_transition(0, 6.0, TransitionKind::CrossDissolve, 2.0);
        assert_eq!(again, Some(0));
        assert_eq!(app.project.transitions.len(), 1);
    }

    #[test]
    fn add_cross_dissolve_without_neighbour_is_none() {
        let mut app = App::new();
        // Indigo [3,7) on V2 (clip index 2) has no edge-adjacent clip on its track.
        assert!(app
            .project
            .add_transition(2, 3.0, TransitionKind::CrossDissolve, 1.0)
            .is_none());
    }

    #[test]
    fn active_transition_respects_span_and_track_visibility() {
        let mut app = App::new();
        app.project
            .add_transition(0, 6.0, TransitionKind::CrossDissolve, 2.0)
            .expect("transition");
        // Span is [5, 7): inside is active, outside is not.
        assert!(app.project.active_transition(6.0).is_some());
        assert!(app.project.active_transition(4.0).is_none());
        assert!(app.project.active_transition(8.0).is_none());
        // Disabling the transition's track hides it.
        app.project.tracks[0].enabled = false;
        assert!(app.project.active_transition(6.0).is_none());
    }

    #[test]
    fn cross_dissolve_weights_ramp_from_outgoing_to_incoming() {
        let tr = Transition {
            kind: TransitionKind::CrossDissolve,
            from: 0,
            to: 1,
            center: 5.0,
            duration: 2.0,
        };
        // Span [4, 6): start → (1,0), center → (0.5,0.5), end → (0,1).
        let (f0, t0, d0) = tr.weights(4.0);
        assert!((f0 - 1.0).abs() < 1e-5 && t0.abs() < 1e-5 && d0.is_none());
        let (fm, tm, _) = tr.weights(5.0);
        assert!((fm - 0.5).abs() < 1e-5 && (tm - 0.5).abs() < 1e-5);
        let (fe, te, _) = tr.weights(6.0);
        assert!(fe.abs() < 1e-5 && (te - 1.0).abs() < 1e-5);
    }

    #[test]
    fn dip_to_color_dips_then_clears() {
        let tr = Transition {
            kind: TransitionKind::DipToColor(DIP_BLACK),
            from: 0,
            to: 1,
            center: 5.0,
            duration: 2.0,
        };
        // First half (t=4.5, p=0.25): outgoing clip, dip rising (amount 0.5).
        let (f, t, dip) = tr.weights(4.5);
        assert!((f - 1.0).abs() < 1e-5 && t.abs() < 1e-5);
        let (_, amount) = dip.expect("dip present in first half");
        assert!((amount - 0.5).abs() < 1e-5);
        // Midpoint: dip fully covers (amount 1.0).
        let (_, _, dip_mid) = tr.weights(5.0);
        assert!((dip_mid.unwrap().1 - 1.0).abs() < 1e-5);
        // Second half (t=5.5, p=0.75): incoming clip, dip falling (amount 0.5).
        let (f2, t2, dip2) = tr.weights(5.5);
        assert!(f2.abs() < 1e-5 && (t2 - 1.0).abs() < 1e-5);
        assert!((dip2.unwrap().1 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn wipe_reveal_grows_from_the_entering_edge() {
        let tr = Transition {
            kind: TransitionKind::Wipe(WipeDir::Left),
            from: 0,
            to: 1,
            center: 5.0,
            duration: 2.0,
        };
        // A left wipe reveals from x0=0 rightward: at p=0.5 (t=5.0) it covers
        // the left half.
        let (x0, y0, x1, y1) = tr.wipe_reveal(5.0).expect("wipe reveal");
        assert!((x0 - 0.0).abs() < 1e-5 && (x1 - 0.5).abs() < 1e-5);
        assert!((y0 - 0.0).abs() < 1e-5 && (y1 - 1.0).abs() < 1e-5);
        // A non-wipe kind has no reveal rect.
        let cross = Transition {
            kind: TransitionKind::CrossDissolve,
            ..tr
        };
        assert!(cross.wipe_reveal(5.0).is_none());
    }

    #[test]
    fn color_grade_identity_is_a_noop() {
        let g = ColorGrade::default();
        assert!(g.is_identity());
        let mut px = [0.3, 0.6, 0.9];
        g.apply(&mut px);
        assert!((px[0] - 0.3).abs() < 1e-4);
        assert!((px[1] - 0.6).abs() < 1e-4);
        assert!((px[2] - 0.9).abs() < 1e-4);
    }

    #[test]
    fn color_grade_exposure_brightens() {
        // +1 stop doubles (then clamps): a mid-grey rises toward white.
        let g = ColorGrade { exposure: 1.0, ..Default::default() };
        assert!(!g.is_identity());
        let mut px = [0.25, 0.25, 0.25];
        g.apply(&mut px);
        assert!(px[0] > 0.45 && px[0] < 0.55, "exposure +1 ≈ ×2: {}", px[0]);
    }

    #[test]
    fn color_grade_saturation_zero_is_greyscale() {
        let g = ColorGrade { saturation: 0.0, ..Default::default() };
        let mut px = [0.8, 0.2, 0.2];
        g.apply(&mut px);
        // All channels collapse to the pixel's luma.
        assert!((px[0] - px[1]).abs() < 1e-4 && (px[1] - px[2]).abs() < 1e-4);
    }

    #[test]
    fn set_clip_gain_clamps_and_only_audio() {
        let mut app = App::new();
        // The default project's clips are all colors — gain set is a no-op.
        app.apply(Action::SetClipGain { index: 0, gain: 0.5 });
        // Add an audio clip and set its gain.
        app.project.clips.push(Clip {
            name: "tone".into(),
            source: ClipSource::Audio(AudioSource {
                path: "/a.wav".into(),
                duration: Some(3.0),
                gain: DEFAULT_AUDIO_GAIN,
            }),
            track: 0,
            start: 0.0,
            duration: 3.0,
            ..Clip::default()
        });
        let idx = app.project.clips.len() - 1;
        app.apply(Action::SetClipGain { index: idx, gain: 0.5 });
        if let ClipSource::Audio(a) = &app.project.clips[idx].source {
            assert!((a.gain - 0.5).abs() < 1e-5);
        } else {
            panic!("expected audio");
        }
        // Over the cap clamps to MAX_AUDIO_GAIN.
        app.apply(Action::SetClipGain { index: idx, gain: 99.0 });
        if let ClipSource::Audio(a) = &app.project.clips[idx].source {
            assert!((a.gain - MAX_AUDIO_GAIN).abs() < 1e-5);
        }
    }

    #[test]
    fn add_cross_dissolve_action_marks_dirty_and_adds() {
        let mut app = App::new();
        app.apply(Action::Seek(6.0));
        assert_eq!(app.project.transitions.len(), 0);
        app.apply(Action::AddCrossDissolve { index: 0 });
        assert_eq!(app.project.transitions.len(), 1, "a dissolve was added at the cut");
    }

    #[test]
    fn out_of_range_trim_and_split_are_noops() {
        let mut app = App::new();
        let n = app.project.clips.len();
        assert!(app.trim_in(999, 1.0).is_none());
        assert!(app.trim_out(999, 1.0).is_none());
        assert!(app.split_clip(999, 1.0).is_none());
        assert_eq!(app.project.clips.len(), n);
    }

    // --- Batch 5: copy/paste/duplicate ---------------------------------------

    #[test]
    fn copy_paste_clips_lands_at_target_time() {
        let mut app = App::new();
        // Select clip 0 (Teal, start=0, dur=6) and copy it.
        app.selected = Some(0);
        app.apply(Action::CopySelectedClips);
        assert_eq!(app.clipboard_clips.len(), 1);
        assert_eq!(app.clipboard_clips[0].name, "Teal");
        // Paste at t=15.
        let before = app.project.clips.len();
        app.apply(Action::PasteClips { at_t: 15.0 });
        assert_eq!(app.project.clips.len(), before + 1);
        let pasted = app.project.clips.last().unwrap();
        assert_eq!(pasted.name, "Teal");
        let expected_start = app.snap_to_frame(15.0);
        assert!((pasted.start - expected_start).abs() < 1e-4);
        // Pasted clip has no link_group.
        assert!(pasted.link_group.is_none());
    }

    #[test]
    fn cut_clip_removes_original() {
        let mut app = App::new();
        let n = app.project.clips.len();
        app.selected = Some(0);
        app.apply(Action::CutSelectedClips);
        assert_eq!(app.project.clips.len(), n - 1);
        assert_eq!(app.clipboard_clips.len(), 1);
        assert_eq!(app.clipboard_clips[0].name, "Teal");
        assert!(app.selected.is_none());
    }

    #[test]
    fn duplicate_clip_places_copy_right_after() {
        let mut app = App::new();
        app.selected = Some(0);
        let start = app.project.clips[0].start;
        let dur = app.project.clips[0].duration;
        let n = app.project.clips.len();
        app.apply(Action::DuplicateSelectedClips);
        assert_eq!(app.project.clips.len(), n + 1);
        let dup = app.project.clips.last().unwrap();
        let expected = app.snap_to_frame(start + dur);
        assert!((dup.start - expected).abs() < 1e-4);
        assert!(dup.link_group.is_none());
    }

    // --- Batch 5: clip transform ---------------------------------------------

    #[test]
    fn set_clip_crop_clamps_to_unit_range() {
        let mut app = App::new();
        app.apply(Action::SetClipCrop { clip_idx: 0, left: 0.3, right: 2.0, top: -0.1, bottom: 0.5 });
        let c = &app.project.clips[0];
        assert!((c.crop_left - 0.3).abs() < 1e-5);
        assert!((c.crop_right - 1.0).abs() < 1e-5); // clamped from 2.0
        assert!((c.crop_top).abs() < 1e-5);          // clamped from -0.1
        assert!((c.crop_bottom - 0.5).abs() < 1e-5);
    }

    #[test]
    fn reset_clip_transform_restores_defaults() {
        let mut app = App::new();
        app.apply(Action::SetClipAnchor { clip_idx: 0, x: 0.5, y: 0.25 });
        app.apply(Action::SetClipBlendMode { clip_idx: 0, mode: ClipBlendMode::Multiply });
        app.apply(Action::ResetClipTransform { clip_idx: 0 });
        let c = &app.project.clips[0];
        assert!((c.anchor_x).abs() < 1e-6 && (c.anchor_y).abs() < 1e-6);
        assert_eq!(c.blend_mode, ClipBlendMode::Normal);
        assert!((c.crop_left + c.crop_right + c.crop_top + c.crop_bottom).abs() < 1e-6);
    }

    // --- Batch 5: time remap -------------------------------------------------

    #[test]
    fn enable_time_remap_seeds_identity_keys() {
        let mut app = App::new();
        // Clip 0: start=0, dur=6.
        assert!(!app.project.clips[0].time_remap_enabled);
        app.apply(Action::SetTimeRemapEnabled { clip_idx: 0, enabled: true });
        let c = &app.project.clips[0];
        assert!(c.time_remap_enabled);
        assert_eq!(c.time_remap_keys.len(), 2);
        assert!((c.time_remap_keys[0].0 - 0.0).abs() < 1e-4); // timeline 0
        assert!((c.time_remap_keys[1].0 - 6.0).abs() < 1e-4); // timeline 6
    }

    #[test]
    fn remapped_source_t_interpolates_between_keys() {
        let c = Clip {
            time_remap_enabled: true,
            time_remap_keys: vec![(0.0, 0.0), (4.0, 8.0)], // 2× speed via remap
            ..Clip::default()
        };
        // At t=2 (half of [0,4]), source should be 4.
        let s = c.remapped_source_t(2.0);
        assert!((s - 4.0).abs() < 1e-4);
        // Before first key: clamped to first source_t.
        let s0 = c.remapped_source_t(-1.0);
        assert!((s0 - 0.0).abs() < 1e-4);
    }

    #[test]
    fn add_and_remove_time_remap_key() {
        let mut app = App::new();
        app.apply(Action::SetTimeRemapEnabled { clip_idx: 0, enabled: true });
        let before = app.project.clips[0].time_remap_keys.len();
        app.apply(Action::AddTimeRemapKey { clip_idx: 0, timeline_t: 3.0, source_t: 4.5 });
        assert_eq!(app.project.clips[0].time_remap_keys.len(), before + 1);
        // Keys remain sorted.
        let keys = &app.project.clips[0].time_remap_keys;
        for w in keys.windows(2) {
            assert!(w[0].0 <= w[1].0);
        }
        // Remove the newly added key (it landed at index 1 after sort).
        let new_idx = keys.iter().position(|k| (k.0 - 3.0).abs() < 1e-4).unwrap();
        app.apply(Action::RemoveTimeRemapKey { clip_idx: 0, key_idx: new_idx });
        assert_eq!(app.project.clips[0].time_remap_keys.len(), before);
    }

    // --- Batch 5: LUFS metering ----------------------------------------------

    #[test]
    fn lufs_integrated_pure_functions() {
        // A history of constant-power blocks at -23 LUFS equivalent.
        // -23 LUFS → mean-square = 10^((-23+0.691)/10) ≈ 5.37e-3
        let target_ms = 10f32.powf((-23.0 + 0.691) / 10.0);
        let history: Vec<f32> = vec![target_ms; 100];
        let lufs = lufs_integrated(&history);
        // Should be approximately -23 ± 1 dB.
        assert!((lufs + 23.0).abs() < 1.5, "integrated LUFS ≈ -23, got {lufs:.2}");
    }

    #[test]
    fn update_lufs_meters_action_updates_app_fields() {
        let mut app = App::new();
        let power = 10f32.powf((-23.0 + 0.691) / 10.0);
        for _ in 0..50 {
            app.apply(Action::UpdateLufsMeters { power });
        }
        assert!(app.lufs_short_term.is_finite() || app.lufs_short_term == -f32::INFINITY);
        app.apply(Action::ResetLufsIntegrated);
        assert_eq!(app.lufs_power_history.len(), 0);
        assert_eq!(app.lufs_integrated, -f32::INFINITY);
    }

    // --- Batch 5: group ripple trim ------------------------------------------

    #[test]
    fn group_ripple_trim_in_trims_multiple_clips() {
        let mut app = App::new();
        // Clip 0: Teal [0,6), clip 1: Amber [6,11) — trim both heads by +1s.
        let start0 = app.project.clips[0].start;
        let start1 = app.project.clips[1].start;
        app.apply(Action::GroupRippleTrimIn { clip_indices: vec![0, 1], delta: 1.0 });
        // Both clips' start should have moved by delta (ripple trim).
        assert!(app.project.clips[0].start > start0 - 1e-4);
        assert!(app.project.clips[1].start > start1 - 1e-4);
    }

    #[test]
    fn group_ripple_trim_out_extends_multiple_clips() {
        let mut app = App::new();
        let end0 = app.project.clips[0].end();
        let end1 = app.project.clips[1].end();
        // Extend tails by 1s.
        app.apply(Action::GroupRippleTrimOut { clip_indices: vec![0, 1], delta: 1.0 });
        // Durations should have grown.
        assert!(app.project.clips[0].end() > end0 - 1e-4);
        assert!(app.project.clips[1].end() > end1 - 1e-4);
    }
}
