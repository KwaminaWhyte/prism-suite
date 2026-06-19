//! Background PNG-sequence export for the GPUI host.
//!
//! The egui app exports a comp to a PNG image sequence **synchronously** on the
//! UI thread via [`pulse_app::render::export_sequence_in_project`]. The GPUI host
//! cannot block its render thread (it is also the event loop), so this module
//! drives the *same* render engine on a **background thread**, reporting
//! per-frame **progress** through a shared [`ExportProgress`] handle the UI polls
//! each frame.
//!
//! No engine fork: the worker reuses the exact framing / numbering / work-area
//! math the egui exporter uses — [`render_frame_in_project`] for the pixels,
//! [`frame_range`] for the inclusive comp-frame span the chosen [`RenderRange`]
//! covers, [`frame_time`] for each frame's presentation time, and [`frame_path`]
//! for the zero-padded `<stem>_<NNNN>.png` filename — so a sequence exported from
//! the GPUI host is byte-identical to one exported from the egui app.
//!
//! MP4 is intentionally out of scope here: the suite ships no video encoder
//! (`prism-io` is stills-only and there is no `prism-media` crate), and the egui
//! app it mirrors only writes PNG sequences. The progress/threading scaffold and
//! the `format` field leave a clean seam for an MP4 muxer later.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use pulse_app::comp::{Comp, FrameCache};
use pulse_app::render::{
    frame_count, frame_path, frame_range, frame_time, render_frame_in_project, RenderRange,
};

/// Output format an export writes. Only [`Png`](OutputFormat::Png) (image
/// sequence) is implemented today — see the module docs for why MP4 is deferred.
/// Kept as an enum so the File menu can surface the choice and a future MP4
/// muxer slots in behind the same threading/progress plumbing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// A PNG image sequence (`<stem>_<NNNN>.png`), one file per frame.
    Png,
}

/// A request to render the active comp to `dir`. Carries an owned snapshot of the
/// project's comps so the worker thread is fully detached from the live `App`
/// (which the UI keeps mutating) — the export renders the document as it was when
/// the user invoked it.
pub struct ExportRequest {
    /// A snapshot of every comp in the project (so precomp layers resolve).
    pub comps: Vec<Comp>,
    /// The id of the comp to render.
    pub comp_id: u64,
    /// Destination directory (created if missing).
    pub dir: PathBuf,
    /// File-name stem (`<stem>_<NNNN>.png`).
    pub stem: String,
    /// Which span of the timeline to render (work area or full comp).
    pub range: RenderRange,
    /// Output format (PNG sequence today).
    pub format: OutputFormat,
}

/// The live state of an in-flight (or finished) export, shared between the worker
/// thread (which mutates it) and the UI (which polls it each frame to draw a
/// progress bar / status). Behind a `Mutex` so the single writer and single
/// reader never tear a field.
#[derive(Clone, Debug, Default)]
pub struct ExportState {
    /// Frames written so far.
    pub done: u32,
    /// Total frames the export will write (the [`frame_range`] span length).
    pub total: u32,
    /// `true` once the worker has finished (successfully or with an error).
    pub finished: bool,
    /// `Some(msg)` on failure (the first IO/encode error), else `None`.
    pub error: Option<String>,
    /// The directory frames are being written to (for the done message).
    pub dir: PathBuf,
}

impl ExportState {
    /// Completion fraction in `[0, 1]` (0 when the total isn't known yet).
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }
}

/// A shared, cloneable handle to an export's [`ExportState`]. The worker holds
/// one clone and writes progress; the UI holds another and reads it each frame.
#[derive(Clone)]
pub struct ExportProgress(Arc<Mutex<ExportState>>);

impl ExportProgress {
    /// A fresh handle whose state shows `total` frames pending, none done.
    fn new(total: u32, dir: PathBuf) -> Self {
        Self(Arc::new(Mutex::new(ExportState {
            total,
            dir,
            ..Default::default()
        })))
    }

    /// A snapshot of the current state (cheap clone behind the lock).
    pub fn snapshot(&self) -> ExportState {
        self.0.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Whether the worker has finished (success or error).
    pub fn is_finished(&self) -> bool {
        self.0.lock().map(|s| s.finished).unwrap_or(true)
    }

    /// Record one more frame written.
    fn bump(&self) {
        if let Ok(mut s) = self.0.lock() {
            s.done += 1;
        }
    }

    /// Mark the export finished, optionally with the first error encountered.
    fn finish(&self, error: Option<String>) {
        if let Ok(mut s) = self.0.lock() {
            s.finished = true;
            s.error = error;
        }
    }
}

/// Launch `request` on a background thread and return a progress handle the UI
/// polls. The worker drives the shared render engine frame-by-frame (so each
/// frame bumps progress), writing the PNG sequence; it never touches the UI. A
/// destination directory that can't be created fails the export immediately
/// (still off-thread, surfaced through the handle).
pub fn spawn(request: ExportRequest) -> ExportProgress {
    let Some(comp) = request.comps.iter().find(|c| c.id == request.comp_id) else {
        // No such comp: return an already-finished, errored handle.
        let p = ExportProgress::new(0, request.dir.clone());
        p.finish(Some("comp not found".into()));
        return p;
    };
    let total_frames = frame_count(comp);
    let (first, last) = frame_range(comp, request.range);
    let count = last - first + 1;
    let progress = ExportProgress::new(count, request.dir.clone());

    let worker = progress.clone();
    thread::spawn(move || {
        match request.format {
            OutputFormat::Png => run_png_sequence(&request, total_frames, first, last, &worker),
        }
    });

    progress
}

/// The worker body for a PNG image-sequence export: create the directory, then
/// render + write each frame in `[first, last]`, bumping progress per frame and
/// finishing (with the first error, if any). Mirrors the egui app's
/// `export_sequence_in_project` loop, but threaded + progress-reporting.
fn run_png_sequence(
    request: &ExportRequest,
    total_frames: u32,
    first: u32,
    last: u32,
    progress: &ExportProgress,
) {
    if let Err(e) = std::fs::create_dir_all(&request.dir) {
        progress.finish(Some(format!("create dir failed: {e}")));
        return;
    }
    // One footage cache for the whole export, like the egui exporter — a
    // sequence's source frames decode at most once each, not per comp frame.
    let mut cache = FrameCache::new();
    // The comp is guaranteed present (the caller resolved it before spawning).
    let comp = match request.comps.iter().find(|c| c.id == request.comp_id) {
        Some(c) => c,
        None => {
            progress.finish(Some("comp not found".into()));
            return;
        }
    };
    for i in first..=last {
        let t = frame_time(comp, i);
        let frame = render_frame_in_project(&request.comps, request.comp_id, t, &mut cache);
        let path = frame_path(&request.dir, &request.stem, i, total_frames);
        let img = match image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels) {
            Some(img) => img,
            None => {
                progress.finish(Some("frame buffer size mismatch".into()));
                return;
            }
        };
        if let Err(e) = img.save_with_format(&path, image::ImageFormat::Png) {
            progress.finish(Some(format!("write {} failed: {e}", path.display())));
            return;
        }
        progress.bump();
    }
    progress.finish(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulse_app::comp::Project;
    use pulse_app::render::range_frame_count;
    use std::time::{Duration, Instant};

    /// A unique temp directory for an export test (cleaned up afterwards).
    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pulse_gpui_export_{tag}_{nanos}"));
        dir
    }

    /// Block until the export finishes (or a timeout), returning its final state.
    fn await_done(progress: &ExportProgress) -> ExportState {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !progress.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        progress.snapshot()
    }

    #[test]
    fn fraction_is_clamped_and_zero_when_unknown() {
        let s = ExportState {
            done: 5,
            total: 10,
            ..Default::default()
        };
        assert!((s.fraction() - 0.5).abs() < 1e-6);
        let s = ExportState::default(); // total 0
        assert_eq!(s.fraction(), 0.0);
        let s = ExportState {
            done: 99,
            total: 10,
            ..Default::default()
        };
        assert_eq!(s.fraction(), 1.0); // clamped
    }

    /// Shrink every comp to a tiny, short timeline so export tests render in
    /// milliseconds (the demo comp is 1280x720 @ 5 s = 150 frames, far too heavy
    /// for a unit test). The numbering/range math is identical at any size.
    fn tiny(project: &mut Project) {
        for c in &mut project.comps {
            c.width = 16;
            c.height = 16;
            c.duration = 0.2; // ~6 frames at 30 fps
            c.work_area = pulse_app::comp::WorkArea::full(c.duration);
        }
    }

    #[test]
    fn spawn_writes_full_range_png_sequence() {
        let mut project = Project::new();
        tiny(&mut project);
        let comp = &project.comps[project.active];
        let comp_id = comp.id;
        let expected = range_frame_count_full(comp);
        let dir = temp_dir("full");

        let progress = spawn(ExportRequest {
            comps: project.comps.clone(),
            comp_id,
            dir: dir.clone(),
            stem: "comp".into(),
            range: RenderRange::Full,
            format: OutputFormat::Png,
        });
        let state = await_done(&progress);

        assert!(state.finished);
        assert_eq!(state.error, None, "export errored: {:?}", state.error);
        assert_eq!(state.done, expected);
        let written = std::fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        assert_eq!(written as u32, expected);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spawn_work_area_writes_only_trimmed_frames() {
        let mut project = Project::new();
        tiny(&mut project);
        // Trim the active comp's work area to a real sub-range.
        let ci = project.active;
        project.comps[ci].duration = 0.5; // ~15 frames at 30 fps
        let dur = project.comps[ci].duration;
        project.comps[ci].work_area = pulse_app::comp::WorkArea { start: 0.1, end: 0.3 };
        project.comps[ci].work_area = project.comps[ci].work_area.clamped(dur);
        let comp_id = project.comps[ci].id;
        let expected = range_frame_count(&project.comps[ci], RenderRange::WorkArea);
        let dir = temp_dir("wa");

        let progress = spawn(ExportRequest {
            comps: project.comps.clone(),
            comp_id,
            dir: dir.clone(),
            stem: "comp".into(),
            range: RenderRange::WorkArea,
            format: OutputFormat::Png,
        });
        let state = await_done(&progress);

        assert!(state.finished && state.error.is_none());
        assert_eq!(state.done, expected);
        // A trimmed work area renders fewer frames than the full comp.
        assert!(expected < range_frame_count_full(&project.comps[ci]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The full-comp frame count, via the engine's range helper.
    fn range_frame_count_full(comp: &Comp) -> u32 {
        range_frame_count(comp, RenderRange::Full)
    }
}
