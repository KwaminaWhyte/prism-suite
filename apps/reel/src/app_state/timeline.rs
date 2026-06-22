use std::path::PathBuf;
use std::time::Instant;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer};
use super::{
    App, Action,
    DEFAULT_VIDEO_FPS, MIN_DUR, DEFAULT_VIDEO_LEN, DEFAULT_IMAGE_LEN, DEFAULT_AUDIO_LEN,
    DEFAULT_TRANSITION_DUR, MIN_TRANSITION_DUR, MAX_AUDIO_GAIN, DEFAULT_AUDIO_GAIN,
    is_video_path, is_audio_path,
    rgb_to_hsl, hsl_to_rgb,
    snap_candidates,
    TrackEq, TrackCompressor, AudioTrackType, TrackEq3,
};

/// The editing tools.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Razor,
    Slip,
    Hand,
    RippleTrim,
    RollTrim,
    RateStretch,
}

impl Tool {
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

    pub const ALL: [Tool; 7] = [
        Tool::Select, Tool::Razor, Tool::Slip, Tool::Hand,
        Tool::RippleTrim, Tool::RollTrim, Tool::RateStretch,
    ];
}

/// A real movie source on disk.
#[derive(Clone, Debug)]
pub struct VideoSource {
    pub path: PathBuf,
    pub fps: Option<f64>,
    pub duration: Option<f32>,
}

impl VideoSource {
    #[allow(dead_code)]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), fps: None, duration: None }
    }

    pub fn probed(path: impl Into<PathBuf>) -> (Self, Option<f32>) {
        let path = path.into();
        match prism_media::probe(&path) {
            Ok(info) => {
                let dur = (info.duration_secs as f32)
                    .is_finite()
                    .then_some(info.duration_secs as f32)
                    .filter(|d| *d > 0.0);
                (Self { path, fps: Some(info.fps), duration: dur }, dur)
            }
            Err(e) => {
                log::warn!("reel-gpui: probe failed for {} ({e}); importing unprobed", path.display());
                (Self { path, fps: None, duration: None }, None)
            }
        }
    }

    pub fn fps(&self) -> f64 {
        self.fps.filter(|f| f.is_finite() && *f > 0.0).unwrap_or(DEFAULT_VIDEO_FPS)
    }

    pub fn frame_index_at(&self, source_time: f32) -> u64 {
        (source_time.max(0.0) as f64 * self.fps()).floor().max(0.0) as u64
    }

    pub fn frame_time(&self, index: u64) -> f64 {
        index as f64 / self.fps()
    }

    pub fn source_len(&self) -> f32 {
        self.duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(DEFAULT_VIDEO_LEN)
            .max(MIN_DUR)
    }
}

/// A real audio file on disk.
#[derive(Clone, Debug)]
pub struct AudioSource {
    pub path: PathBuf,
    pub duration: Option<f32>,
    pub gain: f32,
}

impl AudioSource {
    #[allow(dead_code)]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), duration: None, gain: DEFAULT_AUDIO_GAIN }
    }

    pub fn probed(path: impl Into<PathBuf>) -> (Self, Option<f32>) {
        let path = path.into();
        match prism_media::probe(&path) {
            Ok(info) => {
                let dur = (info.duration_secs as f32)
                    .is_finite()
                    .then_some(info.duration_secs as f32)
                    .filter(|d| *d > 0.0);
                (Self { path, duration: dur, gain: DEFAULT_AUDIO_GAIN }, dur)
            }
            Err(e) => {
                log::warn!("reel-gpui: audio probe failed for {} ({e}); importing unprobed", path.display());
                (Self { path, duration: None, gain: DEFAULT_AUDIO_GAIN }, None)
            }
        }
    }

    pub fn effective_gain(&self) -> f32 {
        if self.gain.is_finite() { self.gain.clamp(0.0, MAX_AUDIO_GAIN) } else { DEFAULT_AUDIO_GAIN }
    }

    pub fn source_len(&self) -> f32 {
        self.duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(DEFAULT_AUDIO_LEN)
            .max(MIN_DUR)
    }
}

/// Where a clip's pixels come from.
#[derive(Clone, Debug)]
pub enum ClipSource {
    Color([f32; 4]),
    Image(std::path::PathBuf),
    Video(VideoSource),
    Audio(AudioSource),
    Title { text: String, font_size: f32, color: [u8; 4], bg_color: Option<[u8; 4]> },
    NestedClip { tracks: Vec<Track>, clips: Vec<Clip>, duration_secs: f32 },
}

impl ClipSource {
    pub fn block_color(&self) -> [f32; 3] {
        match self {
            ClipSource::Color(c) => [c[0], c[1], c[2]],
            ClipSource::Image(_) => [0.30, 0.46, 0.62],
            ClipSource::Video(_) => [0.55, 0.36, 0.62],
            ClipSource::Audio(_) => [0.24, 0.50, 0.34],
            ClipSource::Title { color, .. } => [
                color[0] as f32 / 255.0, color[1] as f32 / 255.0, color[2] as f32 / 255.0,
            ],
            ClipSource::NestedClip { .. } => [0.45, 0.20, 0.75],
        }
    }

    pub fn is_audio(&self) -> bool { matches!(self, ClipSource::Audio(_)) }

    pub fn source_len(&self) -> Option<f32> {
        match self {
            ClipSource::Color(_) | ClipSource::Image(_) | ClipSource::Title { .. } => None,
            ClipSource::Video(v) => Some(v.source_len()),
            ClipSource::Audio(a) => Some(a.source_len()),
            ClipSource::NestedClip { duration_secs, .. } => Some(*duration_secs),
        }
    }
}

/// Per-clip basic color grade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorGrade {
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
}

impl Default for ColorGrade {
    fn default() -> Self { Self { exposure: 0.0, contrast: 0.0, saturation: 1.0 } }
}

impl ColorGrade {
    pub fn is_identity(&self) -> bool {
        self.exposure == 0.0 && self.contrast == 0.0 && self.saturation == 1.0
    }

    pub fn apply(&self, rgb: &mut [f32; 3]) {
        let exp = 2f32.powf(self.exposure);
        for v in rgb.iter_mut() { *v = (*v * exp).clamp(0.0, 1.0); }
        let c = 1.0 + self.contrast;
        for v in rgb.iter_mut() { *v = ((*v - 0.5) * c + 0.5).clamp(0.0, 1.0); }
        let l = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
        for v in rgb.iter_mut() { *v = (l + self.saturation * (*v - l)).clamp(0.0, 1.0); }
    }
}

/// HSL secondary grade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HslSecondaryGrade {
    pub hue_center: f32,
    pub hue_width: f32,
    pub sat_offset: f32,
    pub lum_offset: f32,
}

impl Default for HslSecondaryGrade {
    fn default() -> Self { Self { hue_center: 0.0, hue_width: 0.0, sat_offset: 0.0, lum_offset: 0.0 } }
}

impl HslSecondaryGrade {
    pub fn is_identity(&self) -> bool {
        self.hue_width == 0.0 && self.sat_offset == 0.0 && self.lum_offset == 0.0
    }

    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() { return; }
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        let dist = (h - self.hue_center).abs();
        let dist = dist.min(1.0 - dist);
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

/// Speed curve for a clip's playback ramp.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeedCurve {
    Constant,
    Bezier { p0: f32, p1: f32, p2: f32, p3: f32 },
}

impl Default for SpeedCurve { fn default() -> Self { SpeedCurve::Constant } }

/// Per-clip compositing blend mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipBlendMode { Normal, Multiply, Screen, Overlay, Add, Subtract }

impl Default for ClipBlendMode { fn default() -> Self { ClipBlendMode::Normal } }

/// Kind of video effect applied to a clip.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ClipEffectKind {
    #[default] GaussianBlur, Sharpen, Mosaic, DropShadow, Glow, ChromaticAberration,
}

/// A single video effect entry on a clip's effect stack.
#[derive(Debug, Clone)]
pub struct ClipEffect {
    pub kind: ClipEffectKind,
    pub enabled: bool,
    pub intensity: f32,
    pub secondary: f32,
    pub color: [f32; 4],
}

impl Default for ClipEffect {
    fn default() -> Self {
        Self { kind: ClipEffectKind::GaussianBlur, enabled: true, intensity: 0.5, secondary: 5.0, color: [0.0, 0.0, 0.0, 1.0] }
    }
}

/// Result of a scene-edit detection pass on a single clip.
#[derive(Debug, Clone)]
pub struct SceneEditResult {
    pub clip_idx: usize,
    pub cut_times: Vec<f32>,
}

fn default_one() -> f32 { 1.0 }
fn default_project_name() -> String { "Untitled Project".to_string() }
fn default_true_reel() -> bool { true }
fn default_auto_save_interval() -> u32 { 300 }
fn default_scene_edit_sensitivity() -> f32 { 0.5 }

// Suppress unused warnings for the above helper functions
#[allow(dead_code)]
fn _use_defaults() -> (f32, String, bool, u32, f32) {
    (default_one(), default_project_name(), default_true_reel(), default_auto_save_interval(), default_scene_edit_sensitivity())
}

/// A single clip placed on the timeline.
#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    pub source: ClipSource,
    pub track: usize,
    pub start: f32,
    pub duration: f32,
    pub source_in: f32,
    pub opacity: f32,
    pub grade: ColorGrade,
    pub hsl_secondary: HslSecondaryGrade,
    pub fade_in: f32,
    pub fade_out: f32,
    pub speed: f32,
    pub reversed: bool,
    pub speed_curve: SpeedCurve,
    pub proxy_path: Option<PathBuf>,
    pub link_group: Option<u64>,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub crop_left: f32,
    pub crop_right: f32,
    pub crop_top: f32,
    pub crop_bottom: f32,
    pub blend_mode: ClipBlendMode,
    pub time_remap_enabled: bool,
    pub time_remap_keys: Vec<(f32, f32)>,
    pub effects: Vec<ClipEffect>,
    pub motion_x: f32,
    pub motion_y: f32,
    pub motion_scale_x: f32,
    pub motion_scale_y: f32,
    pub motion_rotation: f32,
}

impl Clip {
    pub fn end(&self) -> f32 { self.start + self.duration }

    pub fn fade_gain_at(&self, t: f32) -> f32 {
        let local = (t - self.start).max(0.0);
        let rem = (self.end() - t).max(0.0);
        let mut g = 1.0f32;
        if self.fade_in > 0.0 && local < self.fade_in { g = g.min(local / self.fade_in); }
        if self.fade_out > 0.0 && rem < self.fade_out { g = g.min(rem / self.fade_out); }
        g.clamp(0.0, 1.0)
    }

    pub fn covers(&self, t: f32) -> bool { t >= self.start && t < self.end() }

    #[allow(dead_code)]
    pub fn source_out(&self) -> f32 { self.source_in + self.duration }

    fn clamp_to_source(&mut self) {
        self.source_in = self.source_in.max(0.0);
        if let Some(len) = self.source.source_len() {
            let max_dur = (len - self.source_in).max(0.0);
            self.duration = self.duration.clamp(MIN_DUR, max_dur.max(MIN_DUR));
        } else {
            self.duration = self.duration.max(MIN_DUR);
        }
    }

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
            track: 0, start: 0.0, duration: 1.0, source_in: 0.0, opacity: 1.0,
            grade: ColorGrade::default(), hsl_secondary: HslSecondaryGrade::default(),
            fade_in: 0.0, fade_out: 0.0, speed: 1.0, reversed: false,
            speed_curve: SpeedCurve::default(), proxy_path: None, link_group: None,
            anchor_x: 0.0, anchor_y: 0.0, crop_left: 0.0, crop_right: 0.0,
            crop_top: 0.0, crop_bottom: 0.0, blend_mode: ClipBlendMode::Normal,
            time_remap_enabled: false, time_remap_keys: Vec::new(), effects: Vec::new(),
            motion_x: 0.0, motion_y: 0.0, motion_scale_x: 1.0, motion_scale_y: 1.0, motion_rotation: 0.0,
        }
    }
}

/// Per-track controls.
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub enabled: bool,
}

/// One band of a 5-band parametric EQ.
#[derive(Clone, Copy, Debug)]
pub struct EqBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Straight-sRGB black/white the dip-to-color presets dip through.
pub const DIP_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
pub const DIP_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Direction a wipe sweeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WipeDir { Left, Right, Up, Down }

impl WipeDir {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self { WipeDir::Left => "Left", WipeDir::Right => "Right", WipeDir::Up => "Up", WipeDir::Down => "Down" }
    }
}

/// Direction a slide transition moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideDirection { Left, Right, Up, Down }

/// Direction a split transition opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection { Horizontal, Vertical }

/// Direction a swap transition swaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapDirection { Left, Right }

/// Direction a spin transition rotates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpinDirection { Clockwise, CounterClockwise }

/// Corner from which a page peel originates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagePeelDirection { TopLeft, TopRight, BottomLeft, BottomRight }

/// Direction a cube transition faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeDirection { Left, Right, Up, Down }

/// Film-grain dissolve pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmPattern { Grain, Burn, Dissolve }

/// Kind of a transition applied at a cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionKind {
    CrossDissolve,
    DipToColor([f32; 4]),
    Wipe(WipeDir),
    Push(WipeDir),
    FilmDissolve,
    IrisCircle,
    ClockWipe,
    DiagonalWipe,
    PixelDissolve,
    // --- Batch 5 (new) ---
    Slide(SlideDirection),
    Split { direction: SplitDirection, flip: bool },
    Swap(SwapDirection),
    Zoom { grow: bool },
    SpinAway(SpinDirection),
    PagePeel { direction: PagePeelDirection, softness: f32 },
    PageTurn { reverse: bool },
    Cube { direction: CubeDirection, lighting: bool },
    Film(FilmPattern),
    Luma { invert: bool },
    DipToBlack,
    DipToWhite,
    AdditiveDissolve,
    NonAdditiveDissolve,
    RandomInvert,
}

impl TransitionKind {
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
            // --- Batch 5 (new) ---
            TransitionKind::Slide(_) => "Slide",
            TransitionKind::Split { .. } => "Split",
            TransitionKind::Swap(_) => "Swap",
            TransitionKind::Zoom { .. } => "Zoom",
            TransitionKind::SpinAway(_) => "Spin Away",
            TransitionKind::PagePeel { .. } => "Page Peel",
            TransitionKind::PageTurn { .. } => "Page Turn",
            TransitionKind::Cube { .. } => "Cube",
            TransitionKind::Film(_) => "Film",
            TransitionKind::Luma { .. } => "Luma",
            TransitionKind::DipToBlack => "Dip to Black",
            TransitionKind::DipToWhite => "Dip to White",
            TransitionKind::AdditiveDissolve => "Additive Dissolve",
            TransitionKind::NonAdditiveDissolve => "Non-Additive Dissolve",
            TransitionKind::RandomInvert => "Random Invert",
        }
    }
}

/// A transition centered on the cut between two clips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub from: usize,
    pub to: usize,
    pub center: f32,
    pub duration: f32,
}

impl Transition {
    pub fn start(&self) -> f32 { self.center - self.duration * 0.5 }
    pub fn end(&self) -> f32 { self.center + self.duration * 0.5 }
    pub fn covers(&self, t: f32) -> bool { t >= self.start() && t < self.end() }

    pub fn progress(&self, t: f32) -> f32 {
        if self.duration <= 0.0 { return if t < self.center { 0.0 } else { 1.0 }; }
        ((t - self.start()) / self.duration).clamp(0.0, 1.0)
    }

    pub fn weights(&self, t: f32) -> (f32, f32, Option<([f32; 4], f32)>) {
        let p = self.progress(t);
        match self.kind {
            TransitionKind::CrossDissolve => (1.0 - p, p, None),
            TransitionKind::DipToColor(color) => {
                if p < 0.5 { (1.0, 0.0, Some((color, p * 2.0))) }
                else { (0.0, 1.0, Some((color, (1.0 - p) * 2.0))) }
            }
            TransitionKind::Wipe(_)
            | TransitionKind::Push(_)
            | TransitionKind::IrisCircle
            | TransitionKind::ClockWipe
            | TransitionKind::DiagonalWipe
            | TransitionKind::PixelDissolve => {
                if p < 0.5 { (1.0, 0.0, None) } else { (0.0, 1.0, None) }
            }
            TransitionKind::FilmDissolve => {
                let film_p = p.powf(1.0 / 2.2);
                (1.0 - film_p, film_p, None)
            }
            // --- Batch 5 (new): simple dissolve-style transitions ---
            TransitionKind::AdditiveDissolve
            | TransitionKind::NonAdditiveDissolve
            | TransitionKind::Luma { .. }
            | TransitionKind::Film(_) => {
                (1.0 - p, p, None)
            }
            // Dip-to-black: fade out then fade in through black
            TransitionKind::DipToBlack => {
                let black = [0.0f32, 0.0, 0.0, 1.0];
                if p < 0.5 { (1.0, 0.0, Some((black, p * 2.0))) }
                else { (0.0, 1.0, Some((black, (1.0 - p) * 2.0))) }
            }
            // Dip-to-white: fade out then fade in through white
            TransitionKind::DipToWhite => {
                let white = [1.0f32, 1.0, 1.0, 1.0];
                if p < 0.5 { (1.0, 0.0, Some((white, p * 2.0))) }
                else { (0.0, 1.0, Some((white, (1.0 - p) * 2.0))) }
            }
            // Spatial / motion transitions: hard cut at 50%
            TransitionKind::Slide(_)
            | TransitionKind::Split { .. }
            | TransitionKind::Swap(_)
            | TransitionKind::Zoom { .. }
            | TransitionKind::SpinAway(_)
            | TransitionKind::PagePeel { .. }
            | TransitionKind::PageTurn { .. }
            | TransitionKind::Cube { .. }
            | TransitionKind::RandomInvert => {
                if p < 0.5 { (1.0, 0.0, None) } else { (0.0, 1.0, None) }
            }
        }
    }

    pub fn push_offsets(&self, t: f32) -> Option<((f32, f32), (f32, f32))> {
        let TransitionKind::Push(dir) = self.kind else { return None; };
        let p = self.progress(t);
        let (from_off, to_off) = match dir {
            WipeDir::Left  => ((-p, 0.0),       (1.0 - p, 0.0)),
            WipeDir::Right => ((p, 0.0),         (-(1.0 - p), 0.0)),
            WipeDir::Up    => ((0.0, -p),        (0.0, 1.0 - p)),
            WipeDir::Down  => ((0.0, p),         (0.0, -(1.0 - p))),
        };
        Some((from_off, to_off))
    }

    pub fn wipe_reveal(&self, t: f32) -> Option<(f32, f32, f32, f32)> {
        let TransitionKind::Wipe(dir) = &self.kind else { return None; };
        let dir = *dir;
        let p = self.progress(t);
        Some(match dir {
            WipeDir::Left => (0.0, 0.0, p, 1.0),
            WipeDir::Right => (1.0 - p, 0.0, 1.0, 1.0),
            WipeDir::Up => (0.0, 0.0, 1.0, p),
            WipeDir::Down => (0.0, 1.0 - p, 1.0, 1.0),
        })
    }

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
                        let angle = dy.atan2(dx);
                        let norm = ((angle + std::f32::consts::FRAC_PI_2)
                            / (2.0 * std::f32::consts::PI)).rem_euclid(1.0);
                        let aa = 0.005_f32;
                        let alpha = ((p - norm + aa) / (2.0 * aa)).clamp(0.0, 1.0);
                        mask.push((alpha * 255.0).round() as u8);
                    }
                }
                Some(mask)
            }
            TransitionKind::DiagonalWipe => {
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
                let mut mask = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let idx = y * w + x;
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

/// The whole edit: a sized comp, track count, and clip list.
#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration: f32,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub transitions: Vec<Transition>,
}

impl Project {
    pub fn new() -> Self {
        let tracks = vec![
            Track { name: "V1".into(), enabled: true },
            Track { name: "V2".into(), enabled: true },
        ];
        let clips = vec![
            Clip { name: "Teal".into(), source: ClipSource::Color([0.18, 0.71, 0.66, 1.0]), track: 0, start: 0.0, duration: 6.0, ..Clip::default() },
            Clip { name: "Amber".into(), source: ClipSource::Color([0.95, 0.55, 0.16, 1.0]), track: 0, start: 6.0, duration: 5.0, ..Clip::default() },
            Clip { name: "Indigo".into(), source: ClipSource::Color([0.36, 0.40, 0.85, 1.0]), track: 1, start: 3.0, duration: 4.0, opacity: 0.6, ..Clip::default() },
        ];
        Self { name: "Sequence 1".into(), width: 1920, height: 1080, fps: 30.0, duration: 30.0, tracks, clips, transitions: Vec::new() }
    }

    pub fn effective_clips_at(&self, t: f32) -> Vec<&Clip> {
        let mut out = Vec::new();
        for (ti, track) in self.tracks.iter().enumerate() {
            if !track.enabled { continue; }
            let top = self.clips.iter().filter(|c| c.track == ti && c.covers(t)).last();
            if let Some(c) = top { out.push(c); }
        }
        out
    }

    pub fn frame_at(&self, t: f32) -> u64 { (t * self.fps).round().max(0.0) as u64 }

    pub fn find_cut(&self, near_idx: usize, t: f32) -> Option<(usize, usize, f32)> {
        let clip = self.clips.get(near_idx)?;
        let track = clip.track;
        let left_edge = clip.start;
        let right_edge = clip.end();
        let use_right = (t - right_edge).abs() <= (t - left_edge).abs();
        let cut = if use_right { right_edge } else { left_edge };
        let left = self.clips.iter().enumerate()
            .find(|(_, c)| c.track == track && (c.end() - cut).abs() < 1e-3)
            .map(|(j, _)| j)?;
        let right = self.clips.iter().enumerate()
            .find(|(j, c)| *j != left && c.track == track && (c.start - cut).abs() < 1e-3)
            .map(|(j, _)| j)?;
        Some((left, right, cut))
    }

    pub fn add_transition(&mut self, idx: usize, t: f32, kind: TransitionKind, duration: f32) -> Option<usize> {
        let (from, to, cut) = self.find_cut(idx, t)?;
        let left_room = self.clips[from].duration;
        let right_room = self.clips[to].duration;
        let max_dur = left_room.min(right_room).max(MIN_TRANSITION_DUR);
        let dur = duration.clamp(MIN_TRANSITION_DUR, max_dur);
        let tr = Transition { kind, from, to, center: cut, duration: dur };
        if let Some(existing) = self.transitions.iter()
            .position(|x| x.from == from && x.to == to && (x.center - cut).abs() < 1e-3)
        {
            self.transitions[existing] = tr;
            Some(existing)
        } else {
            self.transitions.push(tr);
            Some(self.transitions.len() - 1)
        }
    }

    pub fn active_transition(&self, t: f32) -> Option<&Transition> {
        self.transitions.iter().rev().find(|tr| {
            tr.covers(t) && self.clips.get(tr.from)
                .and_then(|c| self.tracks.get(c.track))
                .map(|trk| trk.enabled)
                .unwrap_or(false)
        })
    }
}

impl Default for Project { fn default() -> Self { Self::new() } }

/// A group of clip indices treated as multi-camera angles.
#[derive(Clone, Debug)]
pub struct MulticamGroup {
    pub clips: Vec<usize>,
    pub active_angle: usize,
}

/// Scope data computed from the last rendered program frame.
#[allow(dead_code)]
pub struct ScopeData {
    pub waveform_cols: Vec<Vec<f32>>,
    pub vectorscope_dots: Vec<(f32, f32)>,
    pub hist_r: Vec<f32>,
    pub hist_g: Vec<f32>,
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

pub trait AppTimelineExt {
    fn apply_timeline(&mut self, action: Action);
}

impl AppTimelineExt for App {
    fn apply_timeline(&mut self, action: Action) {
        match action {
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
                    if let Some(gid) = group {
                        let partners: Vec<usize> = self.project.clips.iter().enumerate()
                            .filter(|(i, c)| *i != index && c.link_group == Some(gid))
                            .map(|(i, _)| i).collect();
                        for pi in partners { self.trim_in(pi, t); }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::TrimClipOut { index, t } => {
                let t = self.snap_to_frame(t);
                let group = self.project.clips.get(index).and_then(|c| c.link_group);
                if self.trim_out(index, t).is_some() {
                    if let Some(gid) = group {
                        let partners: Vec<usize> = self.project.clips.iter().enumerate()
                            .filter(|(i, c)| *i != index && c.link_group == Some(gid))
                            .map(|(i, _)| i).collect();
                        for pi in partners { self.trim_out(pi, t); }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::RippleTrimClipIn { index, t } => {
                let t = self.snap_to_frame(t);
                if self.ripple_trim_in(index, t).is_some() { self.host.mark_dirty(); }
            }
            Action::RippleTrimClipOut { index, t } => {
                let t = self.snap_to_frame(t);
                if self.ripple_trim_out(index, t).is_some() { self.host.mark_dirty(); }
            }
            Action::RollTrimEdit { index, delta } => {
                if self.roll_trim(index, delta) { self.host.mark_dirty(); }
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
                if self.split_at(t) > 0 { self.host.mark_dirty(); }
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
                    self.scrub_audio_burst(t);
                }
            }
            Action::StepBy(delta) => {
                self.playing = false;
                self.last_tick = None;
                self.time = (self.time + delta).clamp(0.0, self.project.duration);
                self.host.mark_dirty();
            }
            Action::TogglePlay => {
                self.playing = !self.playing;
                if self.playing {
                    if self.time >= self.project.duration.max(1e-3) {
                        self.time = 0.0;
                        self.host.mark_dirty();
                    }
                    self.last_tick = Some(std::time::Instant::now());
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
                if self.project.add_transition(index, t, TransitionKind::CrossDissolve, DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddTransition { index, kind } => {
                let t = self.snap_to_frame(self.time);
                if self.project.add_transition(index, t, kind, DEFAULT_TRANSITION_DUR).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::AddDiagonalWipe { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::DiagonalWipe, 1.0);
            }
            Action::AddPixelDissolve { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::PixelDissolve, 1.0);
            }
            Action::AddFilmDissolve { index } => {
                let dur = self.project.duration;
                self.project.add_transition(index, dur, TransitionKind::FilmDissolve, 1.0);
            }
            Action::SetTransitionDuration { track_idx: _, trans_idx, duration } => {
                if let Some(t) = self.project.transitions.get_mut(trans_idx) {
                    t.duration = duration.max(0.0);
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
                if i < self.project.clips.len() { self.selected = Some(i); }
            }
            Action::ZoomBy(factor) => { self.zoom = (self.zoom * factor).clamp(0.1, 8.0); }
            Action::ResetView => { self.zoom = 1.0; }
            Action::SetClipFadeIn { index, secs } => {
                if let Some(clip) = self.project.clips.get_mut(index) { clip.fade_in = secs.max(0.0); }
            }
            Action::SetClipFadeOut { index, secs } => {
                if let Some(clip) = self.project.clips.get_mut(index) { clip.fade_out = secs.max(0.0); }
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
                if let Some(c) = self.project.clips.get_mut(clip_id) { c.speed_curve = curve; }
            }
            Action::RateStretchClip { index, new_duration } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    let orig_duration = clip.duration;
                    let new_dur = new_duration.max(MIN_DUR);
                    if orig_duration > 0.0 { clip.speed = (orig_duration / new_dur).clamp(0.01, 10.0); }
                    clip.duration = new_dur;
                    self.host.mark_dirty();
                }
            }
            Action::AddTitle => {
                let start = self.snap_to_frame(self.time);
                let track_idx = self.project.tracks.len();
                self.project.tracks.push(Track { name: format!("T{}", track_idx + 1), enabled: true });
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
                    source: ClipSource::Title { text: "Title Text".into(), font_size: 48.0, color: [255, 255, 255, 255], bg_color: None },
                    track: track_idx, start, duration: 5.0,
                    ..Clip::default()
                };
                self.project.clips.push(clip);
                self.selected = Some(self.project.clips.len() - 1);
                self.host.mark_dirty();
            }
            Action::SetTitleText { index, text } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { text: t, .. } = &mut clip.source { *t = text; self.host.mark_dirty(); }
                }
            }
            Action::SetTitleFontSize { index, size } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { font_size, .. } = &mut clip.source { *font_size = size.max(4.0); self.host.mark_dirty(); }
                }
            }
            Action::SetTitleColor { index, color } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { color: c, .. } = &mut clip.source { *c = color; self.host.mark_dirty(); }
                }
            }
            Action::SetTitleBgColor { index, color } => {
                if let Some(clip) = self.project.clips.get_mut(index) {
                    if let ClipSource::Title { bg_color, .. } = &mut clip.source { *bg_color = color; self.host.mark_dirty(); }
                }
            }
            Action::ToggleSnap => { self.snap_enabled = !self.snap_enabled; }
            Action::BeginClipDrag { clip_id, grab_offset } => {
                self.clip_drag.set(Some(super::ClipDrag { index: clip_id, kind: super::ClipDragKind::Move, grab_offset }));
            }
            Action::MoveClipDrag { track_idx: _, raw_t } => {
                let drag = match self.clip_drag.get() { Some(d) => d, None => return };
                let snapped = self.snapped_time(raw_t, drag.index, 8.0);
                self.snap_point = if self.snap_enabled && (snapped - raw_t).abs() > 1e-6 { Some(snapped) } else { None };
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
            Action::ToggleScopes => { self.scopes_open = !self.scopes_open; }
            Action::SetScopeTab(tab) => { self.scope_tab = tab.min(2); }
            Action::ToggleDualViewer => { self.dual_viewer = !self.dual_viewer; }
            Action::SeekSource(t) => { self.source_playhead = t.clamp(0.0, self.project.duration); }
            Action::ToggleSourcePlay => { self.source_playing = !self.source_playing; }
            Action::SetSourceIn(t) => { self.source_in = t.clamp(0.0, self.project.duration); }
            Action::SetSourceOut(t) => { self.source_out = t.clamp(0.0, self.project.duration); }
            Action::InsertFromSource { clip_id, in_t, out_t } => {
                log::info!("reel-gpui: InsertFromSource clip={clip_id} in={in_t:.2} out={out_t:.2}");
            }
            Action::ToggleBins => { self.bins_open = !self.bins_open; }
            Action::AddBin(name) => { self.bins.push(Bin { name, clips: Vec::new() }); }
            Action::SelectBin(idx) => {
                if idx < self.bins.len() { self.selected_bin = idx; self.selected_bin_clip = None; }
            }
            Action::ImportToBin { bin_idx, path } => {
                if let Some(bin) = self.bins.get_mut(bin_idx) {
                    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "media".into());
                    let clip_type = if is_video_path(&path) { BinClipType::Video }
                        else if is_audio_path(&path) { BinClipType::Audio }
                        else { BinClipType::Image };
                    bin.clips.push(BinClip { name, path, duration: 0.0, clip_type });
                }
            }
            Action::RemoveFromBin { bin_idx, clip_idx } => {
                if let Some(bin) = self.bins.get_mut(bin_idx) {
                    if clip_idx < bin.clips.len() {
                        bin.clips.remove(clip_idx);
                        if self.selected_bin_clip == Some(clip_idx) { self.selected_bin_clip = None; }
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
                let bin_clip = self.bins.get(self.selected_bin).and_then(|b| b.clips.get(bin_clip_idx)).cloned();
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
            Action::AddChapterMarker { time, label } => {
                self.chapter_markers.push((time, label));
                self.chapter_markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            }
            Action::RemoveChapterMarker(idx) => {
                if idx < self.chapter_markers.len() { self.chapter_markers.remove(idx); }
            }
            Action::ToggleLinkClip(clip_idx) => {
                if self.linked_clips.contains(&clip_idx) {
                    self.linked_clips.remove(&clip_idx);
                    if let Some(clip) = self.project.clips.get_mut(clip_idx) { clip.link_group = None; }
                } else {
                    self.linked_clips.insert(clip_idx);
                    let new_gid = self.selected.and_then(|sel| self.project.clips.get(sel))
                        .and_then(|c| c.link_group).unwrap_or(clip_idx as u64 + 1);
                    if let Some(clip) = self.project.clips.get_mut(clip_idx) { clip.link_group = Some(new_gid); }
                    if let Some(sel) = self.selected {
                        if sel != clip_idx {
                            if let Some(c) = self.project.clips.get_mut(sel) {
                                if c.link_group.is_none() { c.link_group = Some(new_gid); }
                            }
                        }
                    }
                }
            }
            Action::NestSelectedClips { name } => {
                let Some(sel_idx) = self.selected else { return };
                if sel_idx >= self.project.clips.len() { return };
                let inner_clip = self.project.clips[sel_idx].clone();
                let dur = inner_clip.duration;
                let inner_tracks = self.project.tracks.clone();
                self.project.clips[sel_idx].source = ClipSource::NestedClip {
                    tracks: inner_tracks, clips: vec![inner_clip], duration_secs: dur.max(MIN_DUR),
                };
                self.project.clips[sel_idx].name = name;
                self.host.mark_dirty();
                log::info!("reel-gpui: NestSelectedClips — clip {} wrapped into nested sequence", sel_idx);
            }
            Action::SetHslSecondaryGrade { clip_id, grade } => {
                if let Some(clip) = self.project.clips.get_mut(clip_id) { clip.hsl_secondary = grade; }
            }
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
            Action::ToggleLogTransform { track_idx } => {
                while self.track_log_transform.len() <= track_idx { self.track_log_transform.push(false); }
                if let Some(f) = self.track_log_transform.get_mut(track_idx) { *f = !*f; }
                self.host.mark_dirty();
            }
            Action::ToggleMulticam => {
                self.multicam_mode = !self.multicam_mode;
                if self.multicam_mode { self.multicam_tracks = (0..self.project.tracks.len()).collect(); }
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
                let clips = if let Some(sel) = self.selected { vec![sel] } else { vec![] };
                self.multicam_groups.push(MulticamGroup { clips, active_angle: 0 });
            }
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
            Action::SetClipAnchor { clip_idx, x, y } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.anchor_x = x.clamp(-1.0, 1.0);
                    c.anchor_y = y.clamp(-1.0, 1.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetClipCrop { clip_idx, left, right, top, bottom } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.crop_left = left.clamp(0.0, 1.0);
                    c.crop_right = right.clamp(0.0, 1.0);
                    c.crop_top = top.clamp(0.0, 1.0);
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
            Action::SetTimeRemapEnabled { clip_idx, enabled } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = enabled;
                    if enabled && c.time_remap_keys.is_empty() {
                        c.time_remap_keys = vec![(c.start, c.source_in), (c.end(), c.source_in + c.duration)];
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
                    if let Some(k) = c.time_remap_keys.get_mut(key_idx) { k.1 = source_t.max(0.0); self.host.mark_dirty(); }
                }
            }
            Action::RemoveTimeRemapKey { clip_idx, key_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if key_idx < c.time_remap_keys.len() { c.time_remap_keys.remove(key_idx); self.host.mark_dirty(); }
                }
            }
            Action::SetFreezeFrame { clip_idx, at_t } => {
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
            Action::GroupRippleTrimIn { clip_indices, delta } => {
                if delta == 0.0 || clip_indices.is_empty() { return; }
                for &ci in &clip_indices {
                    let t = self.project.clips.get(ci).map(|c| c.start + delta);
                    if let Some(t) = t { self.ripple_trim_in(ci, t); }
                }
                self.host.mark_dirty();
            }
            Action::GroupRippleTrimOut { clip_indices, delta } => {
                if delta == 0.0 || clip_indices.is_empty() { return; }
                for &ci in &clip_indices {
                    let t = self.project.clips.get(ci).map(|c| c.end() + delta);
                    if let Some(t) = t { self.ripple_trim_out(ci, t); }
                }
                self.host.mark_dirty();
            }
            Action::SetProjectName(n) => { self.project_name = n; }
            Action::SetProjectPath(p) => {
                self.recent_project_paths.insert(0, p.clone());
                self.recent_project_paths.dedup();
                self.recent_project_paths.truncate(10);
                self.project_path = Some(p);
            }
            Action::AddRecentProject(p) => {
                self.recent_project_paths.insert(0, p);
                self.recent_project_paths.dedup();
                self.recent_project_paths.truncate(10);
            }
            Action::SetProjectNotes(n) => { self.project_notes = n; }
            Action::SetAutoSaveEnabled(b) => { self.auto_save_enabled = b; }
            Action::SetAutoSaveInterval(s) => { self.auto_save_interval_sec = s.max(30); }
            Action::TriggerAutoSave => {}
            Action::SaveExportPreset { name, format, width, height, fps } => {
                self.export_presets.push((name, format, width, height, fps));
            }
            Action::DeleteExportPreset(idx) => {
                if idx < self.export_presets.len() {
                    self.export_presets.remove(idx);
                    if self.active_preset == Some(idx) { self.active_preset = None; }
                }
            }
            Action::ApplyExportPreset(idx) => {
                if idx < self.export_presets.len() { self.active_preset = Some(idx); }
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
            Action::SetSampleRate(rate) => { self.sequence_sample_rate = rate; }
            _ => {}
        }
    }
}

// --- Helper methods on App (used by apply_timeline and other callers) ---------

impl App {
    /// Advance the playhead by wall-clock elapsed time. Returns `false` when
    /// playback stops (end of sequence or already paused).
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
    pub(crate) fn start_audio(&mut self, start_t: f32) {
        self.stop_audio();
        let mix_clips = crate::export::build_mix_clips(
            &self.project, &self.track_eq, &self.track_comp, &self.track_types, &self.audio_effects,
        );
        if mix_clips.is_empty() { return; }
        let fps = self.project.fps.max(1.0);
        let dur = self.project.duration.max(1e-3);
        let first_frame = (start_t * fps).round() as u64;
        let total_frames = ((dur - start_t).max(0.0) * fps).ceil() as u64;
        if total_frames == 0 { return; }
        let plan = crate::export::FramePlan {
            first: first_frame,
            last: first_frame + total_frames - 1,
            count: total_frames,
        };
        let audio_mix = crate::export::render_program_audio(&mix_clips, plan, fps);
        if audio_mix.samples.is_empty() { return; }
        let device_sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(s) => s,
            Err(e) => { log::warn!("reel-gpui: rodio open_default_sink: {e}"); return; }
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
    pub(crate) fn stop_audio(&mut self) {
        if let Some(player) = self.audio_player.take() {
            player.stop();
        }
        self.audio_device_sink = None;
        self.audio_playing = false;
    }

    /// Map an absolute window x (pixels) to a timeline time (seconds).
    pub fn timeline_x_to_time(&self, x_px: f32) -> Option<f32> {
        let bounds = self.timeline_bounds.get()?;
        let left = f32::from(bounds.origin.x);
        let width = f32::from(bounds.size.width);
        if width <= 0.0 { return None; }
        let frac = ((x_px - left) / width).clamp(0.0, 1.0);
        Some(frac * self.project.duration)
    }

    /// Convert a horizontal pixel delta on the timeline into a time delta.
    pub fn timeline_dx_to_dt(&self, dx_px: f32) -> Option<f32> {
        let bounds = self.timeline_bounds.get()?;
        let width = f32::from(bounds.size.width);
        if width <= 0.0 { return None; }
        Some(dx_px / width * self.project.duration)
    }

    /// Play a short 100ms audio burst at `t` when scrubbing while paused.
    pub(crate) fn scrub_audio_burst(&mut self, t: f32) {
        let mix_clips = crate::export::build_mix_clips(
            &self.project, &self.track_eq, &self.track_comp, &self.track_types, &self.audio_effects,
        );
        if mix_clips.is_empty() { return; }
        let fps = self.project.fps.max(1.0);
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

    /// Snap a timeline time to the nearest whole frame at the project fps.
    pub fn snap_to_frame(&self, t: f32) -> f32 {
        let fps = self.project.fps;
        if !fps.is_finite() || fps <= 0.0 { return t.max(0.0); }
        ((t * fps).round() / fps).max(0.0)
    }

    /// Snap `raw_t` to the nearest candidate if snap is enabled and close enough.
    pub fn snapped_time(&self, raw_t: f32, exclude_clip: usize, snap_threshold_px: f32) -> f32 {
        if !self.snap_enabled { return raw_t.max(0.0); }
        let bounds = match self.timeline_bounds.get() {
            Some(b) => b,
            None => return raw_t.max(0.0),
        };
        let width_px = f32::from(bounds.size.width);
        if width_px <= 0.0 { return raw_t.max(0.0); }
        let dur = self.project.duration.max(1e-3);
        let threshold_t = snap_threshold_px / width_px * dur;
        let candidates = snap_candidates(&self.project.clips, self.time, self.work_area_in, self.work_area_out);
        let exclude_start = self.project.clips.get(exclude_clip).map(|c| c.start);
        let exclude_end = self.project.clips.get(exclude_clip).map(|c| c.end());
        let best = candidates.into_iter()
            .filter(|&t| Some(t) != exclude_start && Some(t) != exclude_end)
            .min_by(|a, b| (a - raw_t).abs().partial_cmp(&(b - raw_t).abs()).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(t) = best {
            if (t - raw_t).abs() <= threshold_t { return t.max(0.0); }
        }
        raw_t.max(0.0)
    }

    /// Trim a clip's left edge to `t` without rippling.
    pub fn trim_in(&mut self, idx: usize, t: f32) -> Option<f32> {
        let clip = self.project.clips.get(idx)?;
        let end = clip.end();
        let max_start = end - MIN_DUR;
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

    /// Trim a clip's right edge to `t` without rippling.
    pub fn trim_out(&mut self, idx: usize, t: f32) -> Option<f32> {
        let clip = self.project.clips.get(idx)?;
        let start = clip.start;
        let old_dur = clip.duration;
        let mut new_end = t.max(start + MIN_DUR);
        if let Some(len) = clip.source.source_len() {
            new_end = new_end.min(start + (len - clip.source_in));
        }
        let clip = &mut self.project.clips[idx];
        clip.duration = new_end - start;
        clip.clamp_to_source();
        Some(clip.duration - old_dur)
    }

    /// Ripple-trim the left edge of clip `idx` to `t`.
    pub fn ripple_trim_in(&mut self, idx: usize, t: f32) -> Option<f32> {
        let track = self.project.clips.get(idx)?.track;
        let old_start = self.project.clips[idx].start;
        let shift = self.trim_in(idx, t)?;
        if shift == 0.0 { return Some(0.0); }
        for i in 0..self.project.clips.len() {
            if i != idx && self.project.clips[i].track == track && self.project.clips[i].start < old_start {
                self.project.clips[i].start -= shift;
            }
        }
        Some(shift)
    }

    /// Ripple-trim the right edge of clip `idx` to `t`.
    pub fn ripple_trim_out(&mut self, idx: usize, t: f32) -> Option<f32> {
        let track = self.project.clips.get(idx)?.track;
        let old_end = self.project.clips[idx].end();
        let delta = self.trim_out(idx, t)?;
        if delta == 0.0 { return Some(0.0); }
        for i in 0..self.project.clips.len() {
            if i != idx && self.project.clips[i].track == track && self.project.clips[i].start >= old_end - 1e-4 {
                self.project.clips[i].start += delta;
            }
        }
        Some(delta)
    }

    /// Roll-trim: shift the edit point between clip `idx` and its right neighbour.
    pub fn roll_trim(&mut self, idx: usize, delta: f32) -> bool {
        let track = self.project.clips.get(idx).map(|c| c.track);
        let Some(track) = track else { return false };
        let right_idx = {
            let end = self.project.clips[idx].end();
            self.project.clips.iter().enumerate()
                .filter(|(i, c)| *i != idx && c.track == track && c.start >= end - 1e-3)
                .min_by(|(_, a), (_, b)| a.start.partial_cmp(&b.start).unwrap())
                .map(|(i, _)| i)
        };
        let Some(right) = right_idx else { return false };
        let old_out = self.project.clips[idx].end();
        let new_out = (old_out + delta).max(self.project.clips[idx].start + MIN_DUR);
        let actual_delta = new_out - old_out;
        if actual_delta == 0.0 { return false; }
        let idx_new_dur = self.project.clips[idx].duration + actual_delta;
        if idx_new_dur < MIN_DUR { return false; }
        self.project.clips[idx].duration = idx_new_dur;
        let nb = &mut self.project.clips[right];
        let nb_new_in = nb.source_in + actual_delta;
        if nb_new_in < 0.0 { return false; }
        nb.start += actual_delta;
        nb.source_in = nb_new_in;
        nb.duration = (nb.duration - actual_delta).max(MIN_DUR);
        true
    }

    /// Split clip `idx` at time `t`. Returns the new right clip's index, or `None`.
    pub fn split_clip(&mut self, idx: usize, t: f32) -> Option<usize> {
        let clip = self.project.clips.get(idx)?;
        let left_dur = t - clip.start;
        let right_dur = clip.end() - t;
        if left_dur < MIN_DUR || right_dur < MIN_DUR { return None; }
        let mut right = clip.clone();
        right.start = t;
        right.source_in = clip.source_in + left_dur;
        right.duration = right_dur;
        self.project.clips[idx].duration = left_dur;
        self.project.clips.push(right);
        Some(self.project.clips.len() - 1)
    }

    /// Split every clip straddling `t` on enabled tracks. Returns count cut.
    pub fn split_at(&mut self, t: f32) -> usize {
        let straddling: Vec<usize> = self.project.clips.iter().enumerate()
            .filter(|(_, c)| {
                c.covers(t)
                    && (t - c.start) >= MIN_DUR
                    && (c.end() - t) >= MIN_DUR
                    && self.project.tracks.get(c.track).map(|tr| tr.enabled).unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        let mut n = 0;
        for i in straddling {
            if self.split_clip(i, t).is_some() { n += 1; }
        }
        n
    }

    /// Index of the first enabled track, or 0.
    fn first_visible_track(&self) -> usize {
        self.project.tracks.iter().position(|t| t.enabled).unwrap_or(0)
    }

    /// Build a clip from `path` placed at the playhead on the first visible track.
    pub(crate) fn build_imported_clip(&self, path: &std::path::Path) -> Option<Clip> {
        if path.as_os_str().is_empty() { return None; }
        let name = path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "media".into());
        let start = self.snap_to_frame(self.time);
        let track = self.first_visible_track();
        let (source, duration) = if is_video_path(path) {
            let (video, probed_len) = VideoSource::probed(path);
            (ClipSource::Video(video), probed_len.unwrap_or(DEFAULT_VIDEO_LEN))
        } else if is_audio_path(path) {
            let (audio, probed_len) = AudioSource::probed(path);
            (ClipSource::Audio(audio), probed_len.unwrap_or(DEFAULT_AUDIO_LEN))
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
        if rgba.is_empty() { self.scope_data = None; return; }
        let (w, h) = self.host.last_dims;
        if w == 0 || h == 0 { self.scope_data = None; return; }
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
            let col = (x as usize * cols / w as usize).min(cols - 1);
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            waveform_cols[col].push(luma);
            if i % 8 == 0 {
                let u = -0.147 * r - 0.289 * g + 0.436 * b;
                let v = 0.615 * r - 0.515 * g - 0.100 * b;
                vectorscope_dots.push((u, v));
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{
        App, Action, DEFAULT_VIDEO_FPS, MIN_DUR, DEFAULT_IMAGE_LEN, DEFAULT_AUDIO_LEN,
        DEFAULT_AUDIO_GAIN, MAX_AUDIO_GAIN, DEFAULT_TRANSITION_DUR, is_video_path,
        lufs_integrated,
    };
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
