use super::*;

// --- Render range (work area vs full comp) -------------------------------

/// A 10 s, 30 fps comp (300 frames: 0..=299) with the work area trimmed to
/// `[1.0s, 2.0s]` — frames 30..=60 on the comp grid.
fn ranged_comp() -> Comp {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.width = 16;
    c.height = 16;
    c.duration = 10.0;
    c.fps = 30.0;
    c.work_area = WorkArea { start: 1.0, end: 2.0 };
    c
}

#[test]
fn frame_range_full_spans_whole_timeline() {
    let c = ranged_comp();
    // Full: every frame, 0..=last (300 frames, indices 0..=299).
    assert_eq!(frame_range(&c, RenderRange::Full), (0, 299));
    assert_eq!(range_frame_count(&c, RenderRange::Full), 300);
}

#[test]
fn frame_range_work_area_uses_in_out_frames() {
    let c = ranged_comp();
    // Work area [1.0s, 2.0s] @30fps → frames 30..=60 inclusive (31 frames).
    let (first, last) = frame_range(&c, RenderRange::WorkArea);
    assert_eq!((first, last), (30, 60), "first = in-point frame, last = out");
    assert_eq!(range_frame_count(&c, RenderRange::WorkArea), 31);
    // The first exported frame's time is the work-area start.
    assert!((frame_time(&c, first) - 1.0).abs() < 1e-6);
    assert!((frame_time(&c, last) - 2.0).abs() < 1e-6);
}

#[test]
fn full_work_area_falls_back_to_full_render() {
    // A work area spanning the whole timeline is not a real sub-range, so a
    // work-area render renders the full comp (never a degenerate empty render).
    let mut c = ranged_comp();
    c.work_area = WorkArea::full(c.duration);
    assert_eq!(frame_range(&c, RenderRange::WorkArea), (0, 299));
    assert_eq!(range_frame_count(&c, RenderRange::WorkArea), 300);
}

#[test]
fn degenerate_work_area_falls_back_to_full_render() {
    // A zero-length (in == out) work area would render nothing useful; fall back
    // to the full comp so an export is never empty.
    let mut c = ranged_comp();
    c.work_area = WorkArea { start: 3.0, end: 3.0 };
    assert_eq!(frame_range(&c, RenderRange::WorkArea), (0, 299));
    assert_eq!(range_frame_count(&c, RenderRange::WorkArea), 300);
}

#[test]
fn empty_serde_default_work_area_falls_back_to_full() {
    // The serde-default empty [0,0] range (a pre-work-area `.pulse`) self-heals
    // to the whole timeline via `clamped_work_area`, so it renders full.
    let mut c = ranged_comp();
    c.work_area = WorkArea::default(); // {0,0}
    assert_eq!(frame_range(&c, RenderRange::WorkArea), (0, 299));
}

#[test]
fn default_range_prefers_a_trimmed_work_area() {
    // A real sub-range → default to the work area (After Effects' default).
    let c = ranged_comp();
    assert_eq!(RenderRange::default_for(&c), RenderRange::WorkArea);
    // A full work area → default to the full comp.
    let mut full = ranged_comp();
    full.work_area = WorkArea::full(full.duration);
    assert_eq!(RenderRange::default_for(&full), RenderRange::Full);
    // A degenerate work area → default to the full comp.
    let mut degen = ranged_comp();
    degen.work_area = WorkArea { start: 3.0, end: 3.0 };
    assert_eq!(RenderRange::default_for(&degen), RenderRange::Full);
}

#[test]
fn export_work_area_writes_only_in_out_frames_numbered_by_comp_index() {
    let c = ranged_comp();
    let dir =
        std::env::temp_dir().join(format!("pulse_export_wa_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let summary = export_sequence(&c, &dir, "seq", RenderRange::WorkArea).expect("export");
    // 31 frames (frames 30..=60), the work-area in/out range only.
    assert_eq!(summary.frames, 31);
    let total = frame_count(&c); // numbering padding is over the full count
    // The first exported file is the work-area start frame (frame 30), not 0.
    assert!(
        !frame_path(&dir, "seq", 29, total).exists(),
        "frame before the work area must not be written"
    );
    assert!(
        frame_path(&dir, "seq", 30, total).exists(),
        "first work-area frame (in-point) must be written"
    );
    assert!(
        frame_path(&dir, "seq", 60, total).exists(),
        "last work-area frame (out-point) must be written"
    );
    assert!(
        !frame_path(&dir, "seq", 61, total).exists(),
        "frame after the work area must not be written"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_full_writes_every_frame() {
    let mut c = ranged_comp();
    c.duration = 0.1; // keep IO small: 3 frames, work area trimmed but ignored
    c.work_area = WorkArea { start: 0.0, end: 0.05 };
    let dir =
        std::env::temp_dir().join(format!("pulse_export_full_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let summary = export_sequence(&c, &dir, "seq", RenderRange::Full).expect("export");
    assert_eq!(summary.frames, 3, "full render ignores the work area");
    for i in 0..3 {
        assert!(frame_path(&dir, "seq", i, 3).exists());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
