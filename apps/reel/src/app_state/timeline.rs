use std::path::PathBuf;
use super::{
    App, Action,
    DEFAULT_VIDEO_FPS, MIN_DUR, DEFAULT_VIDEO_LEN, DEFAULT_AUDIO_LEN,
    MIN_TRANSITION_DUR, MAX_AUDIO_GAIN, DEFAULT_AUDIO_GAIN,
    is_video_path, is_audio_path,
    rgb_to_hsl, hsl_to_rgb,
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
    /// Speed-factor time-remap keys: `(timeline_t, speed_factor)`. A factor of
    /// `1.0` is real-time, `2.0` double-speed, `0.0` a freeze-frame. The per-frame
    /// source time is the piecewise-linear integral of these factors (see
    /// [`crate::program_frame::remapped_source_time`]). When empty, the
    /// `time_remap_keys` position model (or constant `speed`) is used instead.
    pub time_remap_speed_keys: Vec<(f32, f32)>,
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

    pub(crate) fn clamp_to_source(&mut self) {
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
            time_remap_enabled: false, time_remap_keys: Vec::new(),
            time_remap_speed_keys: Vec::new(), effects: Vec::new(),
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

// Transition domain (kinds, direction enums, `Transition` math) lives in
// `timeline_transitions.rs`; re-exported here so `super::timeline::Transition`
// (etc.) keeps resolving for every existing reference.
pub use super::timeline_transitions::{
    CubeDirection, FilmPattern, PagePeelDirection, SlideDirection, SpinDirection,
    SplitDirection, SwapDirection, Transition, TransitionKind, WipeDir,
};

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
        // Route clip-edit and selection/snap actions to their split-out
        // sub-handlers first (see `timeline_clips.rs` / `timeline_selection.rs`).
        // Each returns `Some(action)` when it didn't handle the action.
        let action = match self.apply_timeline_clips(action) {
            Some(a) => a,
            None => return,
        };
        let action = match self.apply_timeline_selection(action) {
            Some(a) => a,
            None => return,
        };
        let action = match self.apply_timeline_transitions(action) {
            Some(a) => a,
            None => return,
        };
        let action = match self.apply_timeline_playback(action) {
            Some(a) => a,
            None => return,
        };
        match action {
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
            Action::SetTimeRemapSpeedKeys { clip_idx, mut keys } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    c.time_remap_speed_keys = keys;
                    c.time_remap_enabled = true;
                    self.host.mark_dirty();
                }
            }
            Action::AddTimeRemapSpeedKey { clip_idx, timeline_t, factor } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    c.time_remap_speed_keys.push((timeline_t, factor.max(0.0)));
                    c.time_remap_speed_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::AddSpeedFreezeFrame { clip_idx, at_t, hold_secs } => {
                // Insert a freeze (factor 0) of `hold_secs` at `at_t`: unity speed
                // before and after, a zero-speed plateau in between. The clip's
                // duration grows by `hold_secs` so the held frames have room.
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    let hold = hold_secs.max(0.0);
                    let end = c.end();
                    if c.time_remap_speed_keys.is_empty() {
                        c.time_remap_speed_keys = vec![(c.start, 1.0), (end, 1.0)];
                    }
                    c.duration += hold;
                    // Shift any keys at/after the freeze point later by `hold`.
                    for k in c.time_remap_speed_keys.iter_mut() {
                        if k.0 > at_t { k.0 += hold; }
                    }
                    c.time_remap_speed_keys.push((at_t, 0.0));
                    c.time_remap_speed_keys.push((at_t + hold, 0.0));
                    c.time_remap_speed_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
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


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{
        App, Action, DEFAULT_VIDEO_FPS, MIN_DUR,
        DEFAULT_AUDIO_GAIN, MAX_AUDIO_GAIN, is_video_path,
    };

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
    fn video_extension_classifier() {
        assert!(is_video_path(std::path::Path::new("/a/b.MP4")));
        assert!(is_video_path(std::path::Path::new("/a/b.mov")));
        assert!(!is_video_path(std::path::Path::new("/a/b.png")));
        assert!(!is_video_path(std::path::Path::new("/a/b")));
    }

    // --- Razor / split ------------------------------------------------------

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

}
