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

// ============================================================================
// Multicam sync + live angle switching
// ============================================================================

/// Cross-correlate two audio **peak envelopes** to find the integer sample
/// offset (in envelope bins) that best aligns `b` onto `a`. A positive result
/// means `b` starts `offset` bins *later* than `a` (delay `b` to align); a
/// negative result means `b` is earlier. Returns `0` for empty input.
///
/// This is the core of waveform multicam sync: feed both clips' peak envelopes
/// (same bin rate) and the returned offset is the lag (in bins) that maximises
/// the normalised correlation. Pure / O(n·m) — unit-tested with a known shift.
pub fn waveform_offset(a: &[f32], b: &[f32]) -> isize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let n = a.len() as isize;
    let m = b.len() as isize;
    // `lag` is how much `b` is delayed relative to `a`: we align `a[i]` with
    // `b[i + lag]`, so a positive best-lag means `b`'s features land `lag` bins
    // *later* than `a`'s (i.e. `b` starts later). Search every overlap.
    let mut best_lag = 0isize;
    let mut best_score = f32::NEG_INFINITY;
    for lag in -(n - 1)..m {
        let mut dot = 0.0f32;
        let mut count = 0u32;
        // a[i] aligns with b[i + lag]; valid when 0 <= i < n and 0 <= i+lag < m.
        let i_start = (-lag).max(0);
        let i_end = n.min(m - lag);
        for i in i_start..i_end {
            let bi = i + lag;
            if bi >= 0 && bi < m {
                dot += a[i as usize] * b[bi as usize];
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        // Normalise by overlap length so short tail-overlaps don't dominate.
        let score = dot / count as f32;
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }
    best_lag
}

/// Convert a cross-correlation `bin` offset into a time offset (seconds) given
/// the envelope `bins_per_sec` rate.
pub fn waveform_offset_seconds(a: &[f32], b: &[f32], bins_per_sec: f32) -> f32 {
    if bins_per_sec <= 0.0 {
        return 0.0;
    }
    waveform_offset(a, b) as f32 / bins_per_sec
}

/// A timecode in whole frames, used for timecode-based multicam sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timecode {
    pub frames: u64,
}

impl Timecode {
    pub fn new(frames: u64) -> Self {
        Self { frames }
    }
    /// Parse `HH:MM:SS:FF` at `fps` into a frame count.
    pub fn parse(s: &str, fps: u32) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 4 {
            return None;
        }
        let h: u64 = parts[0].parse().ok()?;
        let m: u64 = parts[1].parse().ok()?;
        let sec: u64 = parts[2].parse().ok()?;
        let f: u64 = parts[3].parse().ok()?;
        let fps = fps.max(1) as u64;
        Some(Self { frames: ((h * 3600 + m * 60 + sec) * fps) + f })
    }
    /// Seconds for this timecode at `fps`.
    pub fn seconds(self, fps: u32) -> f32 {
        self.frames as f32 / fps.max(1) as f32
    }
}

/// The sync offset (seconds) to align angle `b`'s timecode onto angle `a`'s:
/// positive means `b` starts later than `a`. Pure.
pub fn timecode_offset_seconds(a: Timecode, b: Timecode, fps: u32) -> f32 {
    b.seconds(fps) - a.seconds(fps)
}

/// A live multicam clip: per-time **angle selections** that drive which angle is
/// shown as the timeline plays. Selections are kept sorted by `time`; the active
/// angle at any time is the last selection at-or-before it (a step function).
#[derive(Clone, Debug, Default)]
pub struct MulticamClip {
    /// `(time_secs, angle_idx)` cut points, sorted ascending by time.
    pub selections: Vec<(f32, usize)>,
    /// Number of available angles (selections must reference `< angle_count`).
    pub angle_count: usize,
}

impl MulticamClip {
    pub fn new(angle_count: usize) -> Self {
        Self { selections: Vec::new(), angle_count }
    }

    /// Record an angle switch at `time` (live switching). Clamps `angle` to the
    /// available range, replaces an existing selection at the same time, and
    /// keeps the list sorted.
    pub fn switch_at(&mut self, time: f32, angle: usize) {
        if self.angle_count == 0 {
            return;
        }
        let angle = angle.min(self.angle_count - 1);
        let t = time.max(0.0);
        if let Some(slot) = self.selections.iter_mut().find(|(st, _)| (*st - t).abs() < 1e-6) {
            slot.1 = angle;
        } else {
            self.selections.push((t, angle));
            self.selections.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    /// The active angle at `time`: the last selection at-or-before it, or `0`
    /// when nothing has been selected yet (the default angle).
    pub fn active_angle_at(&self, time: f32) -> usize {
        let mut active = 0usize;
        for &(t, a) in &self.selections {
            if t <= time + 1e-6 {
                active = a;
            } else {
                break;
            }
        }
        active
    }

    /// The flat list of cut segments `(start, end, angle)` over `[0, duration]`,
    /// merging consecutive same-angle runs. A timeline "cut" samples the active
    /// angle of the segment it falls in.
    pub fn cut_segments(&self, duration: f32) -> Vec<(f32, f32, usize)> {
        let duration = duration.max(0.0);
        if self.selections.is_empty() {
            return vec![(0.0, duration, 0)];
        }
        let mut segs: Vec<(f32, f32, usize)> = Vec::new();
        // Leading segment from 0 to the first selection (default angle 0) if the
        // first selection isn't at t=0.
        let first_t = self.selections[0].0;
        let mut cursor = 0.0f32;
        let mut cur_angle = 0usize;
        if first_t > 1e-6 {
            // angle 0 until the first switch.
        }
        for &(t, a) in &self.selections {
            let t = t.clamp(0.0, duration);
            if t > cursor + 1e-6 {
                segs.push((cursor, t, cur_angle));
                cursor = t;
            }
            cur_angle = a;
        }
        if cursor < duration - 1e-6 || segs.is_empty() {
            segs.push((cursor, duration, cur_angle));
        }
        // Merge consecutive same-angle runs.
        let mut merged: Vec<(f32, f32, usize)> = Vec::new();
        for seg in segs {
            if let Some(last) = merged.last_mut() {
                if last.2 == seg.2 && (last.1 - seg.0).abs() < 1e-4 {
                    last.1 = seg.1;
                    continue;
                }
            }
            merged.push(seg);
        }
        merged
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

            // --- Live multicam angle switching + sync ---
            Action::SwitchMulticamLive { time, angle } => {
                self.multicam_clip.switch_at(time, angle);
                self.multicam_active_angle = self.multicam_clip.active_angle_at(time);
                self.host.mark_dirty();
            }
            Action::SetMulticamAngleCount(n) => {
                self.multicam_clip.angle_count = n;
            }
            Action::SyncMulticamWaveform { a_idx, b_idx, bins_per_sec } => {
                // Cross-correlate two angles' cached audio peak envelopes and store
                // the resulting offset on angle `b_idx`.
                let a_env = self.multicam_angle_envelope(a_idx);
                let b_env = self.multicam_angle_envelope(b_idx);
                let offset = waveform_offset_seconds(&a_env, &b_env, bins_per_sec);
                if let Some(angle) = self.multicam_angles.get_mut(b_idx) {
                    angle.sync_offset = offset;
                }
                self.multicam_sync_mode = MulticamSyncMode::Waveform;
            }
            Action::SyncMulticamTimecode { a_idx, b_idx, a_tc, b_tc, fps } => {
                let a = Timecode::new(a_tc);
                let b = Timecode::new(b_tc);
                let offset = timecode_offset_seconds(a, b, fps);
                if let Some(angle) = self.multicam_angles.get_mut(b_idx) {
                    angle.sync_offset = offset;
                }
                let _ = a_idx;
                self.multicam_sync_mode = MulticamSyncMode::Timecode;
            }
            _ => {}
        }
    }
}

impl App {
    /// The cached audio peak envelope for a multicam angle's source clip (mono
    /// min/max folded to magnitude), or an empty vec when unavailable. Used as the
    /// correlation input for waveform sync.
    pub(crate) fn multicam_angle_envelope(&self, angle_idx: usize) -> Vec<f32> {
        let Some(angle) = self.multicam_angles.get(angle_idx) else { return Vec::new() };
        let clip = self.project.clips.get(angle.source_clip_idx);
        let Some(clip) = clip else { return Vec::new() };
        let crate::app_state::ClipSource::Audio(audio) = &clip.source else { return Vec::new() };
        // Sample the cached envelope across the clip's duration.
        let cols = 512usize;
        let dur = clip.duration.max(0.001);
        self.waveforms
            .borrow_mut()
            .peaks_for(&audio.path, clip.source_in, clip.source_in + dur, cols)
            .into_iter()
            .map(|p| p.max.abs().max(p.min.abs()))
            .collect()
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

    // --- Multicam sync + live angle switch -----------------------------------

    /// Cross-correlation recovers a known positive shift: `b` is `a` delayed by
    /// `k` bins → the offset is `+k`.
    #[test]
    fn waveform_offset_recovers_known_shift() {
        // A short impulse-like envelope.
        let a = [0.0f32, 0.0, 1.0, 0.8, 0.3, 0.0, 0.0, 0.0];
        // b is a delayed by 3 bins (b[i] = a[i-3]).
        let b = [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.8, 0.3];
        assert_eq!(waveform_offset(&a, &b), 3, "b lags a by 3 bins");
        // The reverse: a lags b by -3.
        assert_eq!(waveform_offset(&b, &a), -3);
        // Empty input → 0.
        assert_eq!(waveform_offset(&[], &b), 0);
    }

    #[test]
    fn waveform_offset_seconds_uses_bin_rate() {
        let a = [0.0f32, 1.0, 0.5, 0.0, 0.0, 0.0];
        let b = [0.0f32, 0.0, 0.0, 1.0, 0.5, 0.0]; // shifted +2
        // At 100 bins/sec a 2-bin offset is 0.02 s.
        let off = waveform_offset_seconds(&a, &b, 100.0);
        assert!((off - 0.02).abs() < 1e-4, "got {off}");
        // Zero bin-rate is a no-op.
        assert_eq!(waveform_offset_seconds(&a, &b, 0.0), 0.0);
    }

    #[test]
    fn timecode_parse_and_offset() {
        let a = Timecode::parse("00:00:01:00", 30).expect("tc");
        assert_eq!(a.frames, 30);
        let b = Timecode::parse("00:00:02:15", 30).expect("tc");
        assert_eq!(b.frames, 75);
        // b is 1.5 s after a at 30fps.
        let off = timecode_offset_seconds(a, b, 30);
        assert!((off - 1.5).abs() < 1e-4, "got {off}");
        // Bad timecode strings reject.
        assert!(Timecode::parse("01:02:03", 30).is_none());
    }

    #[test]
    fn multicam_clip_live_switch_and_sample() {
        let mut clip = MulticamClip::new(4);
        // Default before any switch → angle 0.
        assert_eq!(clip.active_angle_at(2.0), 0);
        clip.switch_at(1.0, 2);
        clip.switch_at(3.0, 1);
        clip.switch_at(5.0, 3);
        // Step function: angle holds until the next cut.
        assert_eq!(clip.active_angle_at(0.5), 0);
        assert_eq!(clip.active_angle_at(1.0), 2);
        assert_eq!(clip.active_angle_at(2.9), 2);
        assert_eq!(clip.active_angle_at(3.0), 1);
        assert_eq!(clip.active_angle_at(6.0), 3);
        // Out-of-range angle clamps to angle_count-1.
        clip.switch_at(7.0, 99);
        assert_eq!(clip.active_angle_at(7.0), 3);
        // Re-switch at the same time replaces.
        clip.switch_at(1.0, 1);
        assert_eq!(clip.active_angle_at(1.0), 1);
    }

    #[test]
    fn multicam_cut_segments_merge_and_cover_duration() {
        let mut clip = MulticamClip::new(3);
        clip.switch_at(2.0, 1);
        clip.switch_at(4.0, 1); // same angle → merges with previous
        clip.switch_at(6.0, 2);
        let segs = clip.cut_segments(10.0);
        // [0,2) angle 0, [2,6) angle 1 (merged), [6,10) angle 2.
        assert_eq!(segs.len(), 3, "{segs:?}");
        assert_eq!(segs[0], (0.0, 2.0, 0));
        assert!((segs[1].0 - 2.0).abs() < 1e-4 && (segs[1].1 - 6.0).abs() < 1e-4 && segs[1].2 == 1);
        assert!((segs[2].0 - 6.0).abs() < 1e-4 && (segs[2].1 - 10.0).abs() < 1e-4 && segs[2].2 == 2);
    }

    #[test]
    fn live_switch_action_updates_active_angle() {
        let mut app = App::new();
        app.apply(Action::SetMulticamAngleCount(3));
        app.apply(Action::SwitchMulticamLive { time: 2.0, angle: 2 });
        assert_eq!(app.multicam_active_angle, 2);
        assert_eq!(app.multicam_clip.active_angle_at(2.0), 2);
    }

    #[test]
    fn sync_timecode_action_sets_angle_offset() {
        let mut app = App::new();
        app.apply(Action::AddMulticamAngle(MulticamAngle::default()));
        app.apply(Action::AddMulticamAngle(MulticamAngle::default()));
        // Angle 1 timecode is 90 frames after angle 0 at 30fps → +3.0 s offset.
        app.apply(Action::SyncMulticamTimecode { a_idx: 0, b_idx: 1, a_tc: 0, b_tc: 90, fps: 30 });
        assert!((app.multicam_angles[1].sync_offset - 3.0).abs() < 1e-4);
        assert_eq!(app.multicam_sync_mode, MulticamSyncMode::Timecode);
    }
}
