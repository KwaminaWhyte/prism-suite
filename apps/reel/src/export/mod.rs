//! The GPUI host's **video export engine** — render the program to an MP4 with
//! audio, off the UI thread, with progress.
//!
//! This mirrors the egui `reel-app`'s `export.rs` + `render_queue.rs`, reusing
//! Reel's shared CPU program sampler ([`crate::program_frame::render_program_at`])
//! at **full comp resolution** so the exported frames match the preview (same
//! track fold, opacity, and cross-dissolve transitions). Frames are rendered
//! **lazily**, one at a time, and piped to ffmpeg via the shared, read-only
//! [`prism_media`] encoder (`encode_h264` / `encode_h264_with_audio`), so a long
//! export never holds every frame in memory.
//!
//! # Audio
//!
//! The program audio mix is rendered by summing each [`ClipSource::Audio`] clip's
//! decoded PCM into the export span (see [`render_program_audio`]). A non-empty
//! mix is muxed as an AAC track; a silent program falls back to a silent encode.
//!
//! # Threading
//!
//! [`run_job`] runs one [`JobSpec`] to completion on a worker thread, streaming
//! [`JobUpdate`] progress over an `mpsc` channel back to the UI. The toolbar's
//! Export action gates on [`prism_media::ffmpeg_available`], snapshots the
//! project + audio mix into a `JobSpec`, spawns the worker, and polls the
//! channel each animation frame to show progress.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::app_state::Project;
use crate::program_frame::{render_program_at, FrameCache, GlobalGrade};

// --- Submodule (split out of this file per the ~1000-line rule) --------------
mod audio_mix;

// Re-export the audio-mix entry points so callers keep using `crate::export::X`
// unchanged. `MixClip` / `EXPORT_*` stay `pub` in `audio_mix` (reachable through
// these fns' signatures); only the entry points are surfaced at the module root.
pub use audio_mix::{build_mix_clips, render_program_audio};

/// Render `project`'s program at time `t` into a straight-sRGB RGBA8 buffer at
/// the comp's **full** `width`x`height`, flattened over opaque black. The
/// full-resolution twin of the preview sampler — it calls the same shared
/// compositor ([`render_program_at`]) so the export honors the same track fold,
/// opacity, and cross-dissolve transitions as the program monitor.
pub fn render_program(
    project: &Project,
    t: f32,
    cache: &mut FrameCache,
    captions: &[crate::app_state::Caption],
) -> (u32, u32, Vec<u8>) {
    render_program_at(
        project,
        t,
        project.width,
        project.height,
        cache,
        &GlobalGrade::identity(),
        None,
        captions,
    )
}

// --- Frame plan -------------------------------------------------------------

/// The inclusive frame index range `[first, last]` plus the frame `count` to
/// encode for a video export. Built by [`video_frame_plan`] from a time range +
/// fps. Pure / unit-tested. Mirrors the egui app's `FramePlan`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub first: u64,
    pub last: u64,
    pub count: u64,
}

/// Resolve the frame plan for a video export over `[start, end)` at `fps`.
/// Returns `None` for a zero-length / inverted / sub-frame range (a video must
/// have ≥1 real frame). Mirrors the egui app's `video_frame_plan`.
pub fn video_frame_plan(start: f32, end: f32, fps: f32) -> Option<FramePlan> {
    let fps = fps.max(1.0);
    let start = start.max(0.0);
    if end <= start {
        return None;
    }
    let count = ((end - start) * fps).round() as i64;
    if count <= 0 {
        return None;
    }
    let count = count as u64;
    let first = (start * fps).round().max(0.0) as u64;
    Some(FramePlan {
        first,
        last: first + count - 1,
        count,
    })
}

/// Ensure an output path ends in `.mp4` (case-insensitive): appends the
/// extension when missing / different. Pure. Mirrors the egui app's
/// `ensure_mp4_extension`.
pub fn ensure_mp4_extension(path: &Path) -> PathBuf {
    let is_mp4 = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false);
    if is_mp4 {
        path.to_path_buf()
    } else {
        let mut s = path.as_os_str().to_os_string();
        s.push(".mp4");
        PathBuf::from(s)
    }
}

/// Ensure an output path ends in `.mov` (case-insensitive).
pub fn ensure_mov_extension(path: &Path) -> PathBuf {
    let is_mov = path.extension().and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mov")).unwrap_or(false);
    if is_mov { path.to_path_buf() } else {
        let mut s = path.as_os_str().to_os_string(); s.push(".mov"); PathBuf::from(s)
    }
}

/// Ensure an output path ends in `.gif` (case-insensitive).
pub fn ensure_gif_extension(path: &Path) -> PathBuf {
    let is_gif = path.extension().and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gif")).unwrap_or(false);
    if is_gif { path.to_path_buf() } else {
        let mut s = path.as_os_str().to_os_string(); s.push(".gif"); PathBuf::from(s)
    }
}

/// Sanitize a string into a filesystem-safe file stem for the default export
/// name: keep `[A-Za-z0-9._-]`, collapse the rest to `_`, falling back to
/// `"export"` for an empty result. Pure. Mirrors the egui app's `sanitize_stem`.
pub fn sanitize_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "export".to_string()
    } else {
        cleaned
    }
}

// --- Lazy render + encode ---------------------------------------------------

/// Render + encode `project` over `plan` to an H.264 MP4 at `path` (extension
/// normalized to `.mp4`), emitting a **per-frame progress callback** so a
/// background worker can stream progress to the UI. Frames are rendered lazily
/// (decode cached across frames, never holding every frame in memory); each
/// rendered frame fires `on_frame(frame_index)` (0-based) just before it is piped
/// to ffmpeg. When `audio` is a non-empty mix it is muxed as an AAC track; an
/// empty / silent mix encodes silently. ffmpeg errors (incl. a missing binary)
/// return the encoder's [`prism_media::MediaError`] rather than panicking.
/// Mirrors the egui app's `with_progress_encode`.
pub fn with_progress_encode<P>(
    project: &Project,
    plan: FramePlan,
    audio: Option<&prism_media::AudioMix>,
    path: &Path,
    captions: &[crate::app_state::Caption],
    mut on_frame: P,
) -> Result<usize, prism_media::MediaError>
where
    P: FnMut(u64),
{
    let out = ensure_mp4_extension(path);
    let w = project.width.max(1);
    let h = project.height.max(1);
    let fps = project.fps.max(1.0);

    // A throwaway clone the lazy iterator owns so it renders across the encode
    // without borrowing `project` for the whole call.
    let project = project.clone();
    let captions = captions.to_vec();
    let mut cache = FrameCache::new();
    let frames = (plan.first..=plan.last).enumerate().map(move |(i, index)| {
        on_frame(i as u64);
        let t = index as f32 / fps;
        let (_, _, rgba) = render_program(&project, t, &mut cache, &captions);
        rgba
    });

    let params = prism_media::EncodeParams::new(w, h, fps as f64);
    match audio {
        Some(audio) if !audio.is_empty() => {
            prism_media::encode_h264_with_audio(frames, &params, audio, &out)
        }
        _ => prism_media::encode_h264(frames, &params, &out),
    }
}

// --- Background job ----------------------------------------------------------

/// The thread-safe snapshot one export job renders from, captured at enqueue time
/// so the worker is fully decoupled from the live editor. Everything is owned +
/// `Send`. Mirrors the egui app's `JobSpec`.
#[derive(Clone, Debug)]
pub struct JobSpec {
    /// A clone of the sequence to render.
    pub project: Project,
    /// The inclusive frame plan + count to encode.
    pub plan: FramePlan,
    /// The program audio mix over the export span (muxed when non-empty).
    pub audio: prism_media::AudioMix,
    /// True when `audio` should be muxed (non-empty, pre-computed at enqueue).
    pub has_audio: bool,
    /// The output `.mp4` path (normalized by the encoder).
    pub path: PathBuf,
    /// The caption cues to burn into each exported frame (snapshot at enqueue).
    pub captions: Vec<crate::app_state::Caption>,
}

impl JobSpec {
    /// The total frame count this job renders (drives the progress bar).
    pub fn total_frames(&self) -> u64 {
        self.plan.count
    }
}

/// The lifecycle state of an export job. `Rendering → (Done | Failed)`. Mirrors
/// the egui app's `JobStatus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    /// In flight: `done` of `total` frames encoded so far.
    Rendering { done: u64, total: u64 },
    /// Finished: `frames` were written to the output file.
    Done { frames: u64 },
    /// Failed before completing, with a human-readable reason.
    Failed { message: String },
}

impl JobStatus {
    /// True once the job has reached a terminal state (done or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Done { .. } | JobStatus::Failed { .. })
    }

    /// Fractional progress in `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        match self {
            JobStatus::Rendering { done, total } => {
                if *total == 0 {
                    0.0
                } else {
                    (*done as f32 / *total as f32).clamp(0.0, 1.0)
                }
            }
            JobStatus::Done { .. } => 1.0,
            JobStatus::Failed { .. } => 0.0,
        }
    }

    /// A short status label for the toolbar (e.g. `"Exporting 42%"`).
    pub fn label(&self) -> String {
        match self {
            JobStatus::Rendering { done, total } => {
                let pct = (self.fraction() * 100.0).round() as u32;
                format!("Exporting {pct}% ({done}/{total})")
            }
            JobStatus::Done { frames } => format!("Exported — {frames} frame(s)"),
            JobStatus::Failed { message } => format!("Export failed — {message}"),
        }
    }
}

/// A progress / completion update streamed from the worker thread to the UI over
/// an `mpsc` channel. Mirrors the egui app's `JobUpdate`.
#[derive(Clone, Debug)]
pub struct JobUpdate {
    pub status: JobStatus,
}

/// Run one export `spec` to completion on the **calling (worker) thread**,
/// streaming progress over `tx` as it renders and returning the final status. A
/// missing ffmpeg / encode error returns a [`JobStatus::Failed`] rather than
/// panicking. The final status is also sent over `tx` so a UI that only watches
/// the channel sees it. Mirrors the egui app's `run_job`.
pub fn run_job(spec: &JobSpec, tx: &Sender<JobUpdate>) -> JobStatus {
    let total = spec.total_frames();
    let _ = tx.send(JobUpdate {
        status: JobStatus::Rendering { done: 0, total },
    });

    let result = with_progress_encode(
        &spec.project,
        spec.plan,
        spec.has_audio.then_some(&spec.audio),
        &spec.path,
        &spec.captions,
        |frame_index| {
            let _ = tx.send(JobUpdate {
                status: JobStatus::Rendering {
                    done: frame_index + 1,
                    total,
                },
            });
        },
    );

    let status = match result {
        Ok(frames) => JobStatus::Done {
            frames: frames as u64,
        },
        Err(e) => JobStatus::Failed {
            message: e.to_string(),
        },
    };
    let _ = tx.send(JobUpdate {
        status: status.clone(),
    });
    status
}

/// Build a [`JobSpec`] to export the whole of `project` to `path`: the full
/// `[0, duration)` time span at the project fps, with the program audio mix
/// pre-rendered and `has_audio` decided. Returns `None` when the sequence has no
/// frames (a zero / degenerate duration). The audio mix is rendered here (on the
/// caller's thread) so the worker is pure. Mirrors the egui app's export command
/// snapshotting.
pub fn build_full_export(
    project: &Project,
    path: &Path,
    captions: &[crate::app_state::Caption],
) -> Option<JobSpec> {
    let plan = video_frame_plan(0.0, project.duration, project.fps)?;
    let mix_clips = build_mix_clips(project, &[], &[], &[], &[]);
    let audio = render_program_audio(&mix_clips, plan, project.fps);
    // Muxable only if there is any audible (non-zero) content.
    let has_audio = !audio.is_empty() && audio.samples.iter().any(|&s| s != 0.0);
    Some(JobSpec {
        project: project.clone(),
        plan,
        audio,
        has_audio,
        path: ensure_mp4_extension(path),
        captions: captions.to_vec(),
    })
}

/// Used by [`crate::Reel`] to hold a running export's progress receiver + status,
/// polled each animation frame. A small wrapper so the root view owns one field.
pub struct ExportJob {
    /// The channel the worker streams [`JobUpdate`]s over.
    pub rx: std::sync::mpsc::Receiver<JobUpdate>,
    /// The latest known status (updated by polling `rx`).
    pub status: JobStatus,
}

impl ExportJob {
    /// Drain any pending updates from the worker, advancing `status` to the most
    /// recent. Returns `true` while the job is still in flight (the caller keeps
    /// re-arming the next animation-frame poll), `false` once terminal.
    pub fn poll(&mut self) -> bool {
        while let Ok(update) = self.rx.try_recv() {
            self.status = update.status;
        }
        !self.status.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::audio_mix::{EXPORT_CHANNELS, EXPORT_SAMPLE_RATE};
    use crate::app_state::{Clip, ClipSource, Project};

    #[test]
    fn video_frame_plan_indexing() {
        let p = video_frame_plan(0.0, 1.0, 24.0).expect("non-empty");
        assert_eq!((p.first, p.last, p.count), (0, 23, 24));
        let p = video_frame_plan(1.0, 2.0, 24.0).expect("non-empty");
        assert_eq!((p.first, p.last, p.count), (24, 47, 24));
        let p = video_frame_plan(0.0, 0.5, 30.0).expect("non-empty");
        assert_eq!(p.count, 15);
    }

    #[test]
    fn video_frame_plan_rejects_zero_length() {
        assert!(video_frame_plan(0.5, 0.5, 24.0).is_none());
        assert!(video_frame_plan(2.0, 1.0, 24.0).is_none());
        assert!(video_frame_plan(0.0, 0.04, 24.0).is_some());
        assert!(video_frame_plan(0.0, 0.001, 24.0).is_none());
    }

    #[test]
    fn ensure_mp4_extension_appends_when_needed() {
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out.mp4")), PathBuf::from("/tmp/out.mp4"));
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out.MP4")), PathBuf::from("/tmp/out.MP4"));
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out")), PathBuf::from("/tmp/out.mp4"));
        assert_eq!(ensure_mp4_extension(Path::new("/tmp/out.mov")), PathBuf::from("/tmp/out.mov.mp4"));
    }

    #[test]
    fn sanitize_stem_is_filesystem_safe() {
        assert_eq!(sanitize_stem("My Clip!"), "My_Clip_");
        assert_eq!(sanitize_stem("a/b:c"), "a_b_c");
        assert_eq!(sanitize_stem("   "), "export");
        assert_eq!(sanitize_stem("ok-name_1.2"), "ok-name_1.2");
    }

    /// The default project renders at full comp resolution into a buffer of the
    /// right length whose center is the seeded teal/over-clip content (opaque).
    #[test]
    fn render_program_full_res_buffer_length_and_opaque() {
        let project = Project::new();
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, 0.0, &mut cache, &[]);
        assert_eq!(w, project.width);
        assert_eq!(h, project.height);
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
        // Center pixel is opaque after flatten over black.
        let idx = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        assert_eq!(rgba[idx + 3], 255);
    }

    /// Past the end of the program the frame is the black comp of the right size.
    #[test]
    fn render_past_end_is_black() {
        let project = Project::new();
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, project.duration + 5.0, &mut cache, &[]);
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
        assert!(rgba
            .chunks_exact(4)
            .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0 && p[3] == 255));
    }

    /// A cross-dissolve at the midpoint blends both color clips: at the exact cut
    /// (progress 0.5) the rendered center is roughly the average of the two.
    #[test]
    fn render_cross_dissolve_blends_two_color_clips() {
        // Two abutting full-frame color clips on one track + a dissolve at the cut.
        let mut project = Project {
            name: "x".into(),
            width: 16,
            height: 16,
            fps: 30.0,
            duration: 10.0,
            tracks: vec![crate::app_state::Track { name: "V1".into(), enabled: true }],
            clips: vec![
                Clip { name: "Red".into(), source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]), track: 0, start: 0.0, duration: 4.0, ..Clip::default() },
                Clip { name: "Blue".into(), source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]), track: 0, start: 4.0, duration: 4.0, ..Clip::default() },
            ],
            transitions: Vec::new(),
        };
        // Add a 2s cross-dissolve centered on the cut at t=4.
        let added =
            project.add_transition(0, 4.0, crate::app_state::TransitionKind::CrossDissolve, 2.0);
        assert!(added.is_some(), "an adjacent clip exists → a transition is added");

        let mut cache = FrameCache::new();
        // At the cut center (t=4.0): progress 0.5 → both clips at weight 0.5.
        let (w, h, rgba) = render_program(&project, 4.0, &mut cache, &[]);
        let idx = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        let px = &rgba[idx..idx + 4];
        // Straight-over fold then flatten over black: red folds to R=128 (A=0.5),
        // blue over that gives R=64, B=128, A=0.75; the flatten scales RGB by A →
        // R≈48, B≈96. Both clips are visibly present (a real dissolve), blue
        // (the incoming clip, layered last) reading stronger than red.
        assert!(px[0] > 20 && px[0] < 80, "outgoing red present but faded: {}", px[0]);
        assert!(px[2] > 70 && px[2] > px[0], "incoming blue present + stronger: {}", px[2]);
        assert_eq!(px[3], 255, "opaque after flatten");
    }

    /// Before and after the dissolve span, the program is a clean single clip
    /// (no blend): fully red before the cut-region, fully blue after.
    #[test]
    fn render_outside_dissolve_is_a_single_clip() {
        let mut project = Project {
            name: "x".into(),
            width: 8,
            height: 8,
            fps: 30.0,
            duration: 10.0,
            tracks: vec![crate::app_state::Track { name: "V1".into(), enabled: true }],
            clips: vec![
                Clip { name: "Red".into(), source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]), track: 0, start: 0.0, duration: 4.0, ..Clip::default() },
                Clip { name: "Blue".into(), source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]), track: 0, start: 4.0, duration: 4.0, ..Clip::default() },
            ],
            transitions: Vec::new(),
        };
        project
            .add_transition(0, 4.0, crate::app_state::TransitionKind::CrossDissolve, 2.0)
            .expect("transition");
        let mut cache = FrameCache::new();
        let center = |t: f32, cache: &mut FrameCache| {
            let (w, h, rgba) = render_program(&project, t, cache, &[]);
            let i = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        // Well before the 2s span [3,5): pure red.
        assert_eq!(center(1.0, &mut cache), [255, 0, 0]);
        // Well after: pure blue.
        assert_eq!(center(6.0, &mut cache), [0, 0, 255]);
    }

    /// A two-color helper project with one transition on the cut at t=4.
    fn two_clip_project(kind: crate::app_state::TransitionKind) -> Project {
        let mut project = Project {
            name: "x".into(),
            width: 16,
            height: 16,
            fps: 30.0,
            duration: 10.0,
            tracks: vec![crate::app_state::Track { name: "V1".into(), enabled: true }],
            clips: vec![
                Clip { name: "Red".into(), source: ClipSource::Color([1.0, 0.0, 0.0, 1.0]), track: 0, start: 0.0, duration: 4.0, ..Clip::default() },
                Clip { name: "Blue".into(), source: ClipSource::Color([0.0, 0.0, 1.0, 1.0]), track: 0, start: 4.0, duration: 4.0, ..Clip::default() },
            ],
            transitions: Vec::new(),
        };
        project.add_transition(0, 4.0, kind, 2.0).expect("transition");
        project
    }

    /// A dip-to-black at its midpoint renders (near-)black at the center — the
    /// program dips through the color before the incoming clip resolves.
    #[test]
    fn render_dip_to_black_is_dark_at_midpoint() {
        use crate::app_state::{TransitionKind, DIP_BLACK};
        let project = two_clip_project(TransitionKind::DipToColor(DIP_BLACK));
        let mut cache = FrameCache::new();
        // At the cut center (t=4.0) the dip fully covers → black.
        let (w, h, rgba) = render_program(&project, 4.0, &mut cache, &[]);
        let i = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        assert!(rgba[i] < 20 && rgba[i + 1] < 20 && rgba[i + 2] < 20, "dipped dark: {:?}", &rgba[i..i + 3]);
        // Outside the span: clean single clip (pure red before, blue after).
        let (_, _, before) = render_program(&project, 1.0, &mut cache, &[]);
        assert_eq!(&before[i..i + 3], &[255, 0, 0]);
    }

    /// A left wipe at its midpoint shows the incoming (blue) clip on the left half
    /// of the frame and the outgoing (red) clip on the right half.
    #[test]
    fn render_wipe_reveals_incoming_on_swept_side() {
        use crate::app_state::{TransitionKind, WipeDir};
        let project = two_clip_project(TransitionKind::Wipe(WipeDir::Left));
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, 4.0, &mut cache, &[]);
        let row = (h / 2) as usize;
        let left = (row * w as usize + (w / 8) as usize) * 4; // far left → incoming blue
        let right = (row * w as usize + (7 * w / 8) as usize) * 4; // far right → outgoing red
        assert!(rgba[left + 2] > rgba[left], "left half is incoming blue");
        assert!(rgba[right] > rgba[right + 2], "right half is outgoing red");
    }

    /// A clip with a non-identity color grade renders graded pixels: a +saturation
    /// boost on a saturated red leaves red dominant; exposure +1 brightens.
    #[test]
    fn render_applies_clip_color_grade() {
        use crate::app_state::{Clip, ColorGrade, Project, Track};
        let mut project = Project {
            name: "g".into(),
            width: 8,
            height: 8,
            fps: 30.0,
            duration: 5.0,
            tracks: vec![Track { name: "V1".into(), enabled: true }],
            clips: vec![Clip {
                name: "Grey".into(),
                source: ClipSource::Color([0.25, 0.25, 0.25, 1.0]),
                track: 0,
                start: 0.0,
                duration: 5.0,
                grade: ColorGrade { exposure: 1.0, ..Default::default() },
                ..Clip::default()
            }],
            transitions: Vec::new(),
        };
        let mut cache = FrameCache::new();
        let (w, h, rgba) = render_program(&project, 1.0, &mut cache, &[]);
        let i = (((h / 2) as usize) * (w as usize) + (w / 2) as usize) * 4;
        // +1 stop ≈ ×2: a 0.25 grey (≈64) brightens toward 0.5 (≈128).
        assert!(rgba[i] > 110 && rgba[i] < 140, "exposure brightened: {}", rgba[i]);
        // The ungraded baseline (exposure 0) stays ≈64.
        project.clips[0].grade = ColorGrade::default();
        let (_, _, base) = render_program(&project, 1.0, &mut cache, &[]);
        assert!(base[i] > 55 && base[i] < 75, "ungraded baseline ≈64: {}", base[i]);
    }

    /// `build_full_export` snapshots the whole sequence into a plan covering
    /// `[0, duration)` and a silent (no audio clips) default project mix.
    #[test]
    fn build_full_export_covers_whole_sequence() {
        let project = Project::new();
        let spec = build_full_export(&project, Path::new("/tmp/x"), &[]).expect("plan");
        assert_eq!(spec.path, PathBuf::from("/tmp/x.mp4"));
        assert_eq!(spec.plan.first, 0);
        // duration 30s at 30fps → 900 frames.
        assert_eq!(spec.plan.count, (project.duration * project.fps) as u64);
        assert!(!spec.has_audio, "the default project has no audio clips");
    }

    /// `JobStatus::fraction` is clamped and labels read sensibly.
    #[test]
    fn job_status_fraction_and_label() {
        assert_eq!(JobStatus::Rendering { done: 0, total: 0 }.fraction(), 0.0);
        assert_eq!(JobStatus::Rendering { done: 9, total: 4 }.fraction(), 1.0);
        assert_eq!(JobStatus::Done { frames: 3 }.fraction(), 1.0);
        assert!(JobStatus::Done { frames: 3 }.is_terminal());
        assert!(JobStatus::Failed { message: "x".into() }.is_terminal());
        assert!(!JobStatus::Rendering { done: 1, total: 2 }.is_terminal());
        assert!(JobStatus::Failed { message: "ffmpeg".into() }.label().contains("ffmpeg"));
    }

    /// End-to-end render→encode→probe of a tiny export, gated on ffmpeg presence
    /// (skips with a printed note otherwise, like the suite's decode/GPU skips).
    #[test]
    fn run_job_encodes_when_ffmpeg_present() {
        if !prism_media::ffmpeg_available() {
            eprintln!("ffmpeg not available — skipping reel-gpui run_job test");
            return;
        }
        let mut project = Project::new();
        project.width = 48;
        project.height = 32;
        project.fps = 10.0;
        project.duration = 0.5;
        let plan = video_frame_plan(0.0, project.duration, project.fps).expect("plan");
        let mut out = std::env::temp_dir();
        out.push(format!("reel_gpui_export_{}.mp4", std::process::id()));
        let spec = JobSpec {
            project,
            plan,
            audio: prism_media::AudioMix::new(Vec::new(), EXPORT_SAMPLE_RATE, EXPORT_CHANNELS),
            has_audio: false,
            path: out.clone(),
            captions: Vec::new(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let status = run_job(&spec, &tx);
        assert!(status.is_terminal());
        assert!(matches!(status, JobStatus::Done { .. }), "encode succeeded: {status:?}");
        let info = prism_media::probe(&out).expect("probe encoded mp4");
        assert_eq!(info.width, 48);
        assert_eq!(info.height, 32);
        // The worker streamed at least the final update.
        let updates: Vec<_> = rx.try_iter().collect();
        assert!(!updates.is_empty(), "progress streamed");
        assert!(updates.last().unwrap().status.is_terminal());
        let _ = std::fs::remove_file(&out);
    }
}
