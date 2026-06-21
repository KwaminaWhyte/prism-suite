use super::*;

/// A single 2-D motion track point with per-frame position keyframes.
#[derive(Clone, Debug)]
pub struct TrackPoint {
    pub name: String,
    /// Position in comp space at the reference time.
    pub position: [f32; 2],
    /// `(time_secs, comp_space_position)` keyframes for this point.
    pub keyframes: Vec<(f32, [f32; 2])>,
}

/// State for the 2-point camera tracker panel.
#[derive(Clone, Debug, Default)]
pub struct CameraTracker {
    pub track_points: Vec<TrackPoint>,
    pub solved: bool,
    /// `(time_secs, [tx, ty, scale])` — the solved camera motion.
    pub camera_keyframes: Vec<(f32, [f32; 3])>,
    pub open: bool,
    pub analyze_progress: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionSketchStroke {
    pub points: Vec<[f32; 2]>,
    pub timestamps: Vec<f32>,
    pub layer_id: usize,
    pub smoothing: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionSketchConfig {
    pub capture_speed: f32,
    pub smoothing: f32,
    pub show_wireframe: bool,
    pub start_capture_at_outpoint: bool,
}

impl Default for MotionSketchConfig {
    fn default() -> Self {
        Self {
            capture_speed: 100.0,
            smoothing: 25.0,
            show_wireframe: true,
            start_capture_at_outpoint: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum StabilizeResult {
    #[default]
    Smooth,
    NoBgMotion,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum StabilizeMethod {
    #[default]
    Subspace,
    PositionScale,
    Position,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum StabilizeFraming {
    #[default]
    StabilizeOnlyCropSmooth,
    Stabilize,
    NoCropSmooth,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WarpStabConfig {
    pub result: StabilizeResult,
    pub smoothness: f32,
    pub method: StabilizeMethod,
    pub framing: StabilizeFraming,
    pub crop_less_smooth_more: f32,
    pub detailed_analysis: bool,
    pub rolling_shutter_ripple: f32,
}

impl Default for WarpStabConfig {
    fn default() -> Self {
        Self {
            result: StabilizeResult::default(),
            smoothness: 50.0,
            method: StabilizeMethod::Subspace,
            framing: StabilizeFraming::StabilizeOnlyCropSmooth,
            crop_less_smooth_more: 50.0,
            detailed_analysis: false,
            rolling_shutter_ripple: 0.0,
        }
    }
}

/// Status of a 3D camera track solve.
#[derive(Clone, Debug, PartialEq)]
pub enum CameraTrackStatus {
    Idle,
    Analyzing,
    Solving,
    Done,
    Failed,
}

/// A single 3D track point from a camera solve.
#[derive(Clone, Debug)]
pub struct CameraTrackPoint {
    pub id: usize,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Confidence: 0.0..=1.0
    pub confidence: f32,
    pub selected: bool,
}

/// State for a 3D camera track solve on a specific layer.
#[derive(Clone, Debug)]
pub struct CameraTrackSolve {
    pub layer_id: usize,
    pub status: CameraTrackStatus,
    pub solve_error: f32,
    /// Method: "Typical", "Mostly Flat", "Tripod"
    pub method: String,
    pub track_points: Vec<CameraTrackPoint>,
    pub attached_layer_ids: Vec<usize>,
}


impl App {
    pub(super) fn apply_tracking(&mut self, action: Action) {
        match action {
            // --- Batch 2: Track Camera ---
            Action::ToggleCameraTracker => {
                self.camera_tracker.open = !self.camera_tracker.open;
            }
            Action::AddTrackPoint { name, pos } => {
                self.camera_tracker.track_points.push(TrackPoint {
                    name,
                    position: pos,
                    keyframes: vec![(0.0, pos)],
                });
            }
            Action::RemoveTrackPoint(idx) => {
                if idx < self.camera_tracker.track_points.len() {
                    self.camera_tracker.track_points.remove(idx);
                    self.camera_tracker.solved = false;
                }
            }
            Action::MoveTrackPoint { idx, time, pos } => {
                if let Some(pt) = self.camera_tracker.track_points.get_mut(idx) {
                    if let Some(kf) = pt.keyframes.iter_mut().find(|(t, _)| (*t - time).abs() < 1e-4) {
                        kf.1 = pos;
                    } else {
                        pt.keyframes.push((time, pos));
                        pt.keyframes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    }
                }
            }
            Action::SolveCameraTrack => {
                let pts = &self.camera_tracker.track_points;
                if pts.len() < 2 || pts.iter().any(|p| p.keyframes.len() < 2) {
                    return;
                }
                let mut times: Vec<f32> = pts.iter()
                    .flat_map(|p| p.keyframes.iter().map(|(t, _)| *t))
                    .collect();
                times.sort_by(|a, b| a.partial_cmp(b).unwrap());
                times.dedup();

                let mut camera_keyframes = Vec::new();
                let ref_positions: Vec<[f32; 2]> = pts.iter()
                    .map(|p| p.keyframes[0].1)
                    .collect();

                for t in &times {
                    let cur_positions: Vec<[f32; 2]> = pts.iter().map(|p| {
                        let kfs = &p.keyframes;
                        if *t <= kfs[0].0 {
                            kfs[0].1
                        } else if *t >= kfs[kfs.len()-1].0 {
                            kfs[kfs.len()-1].1
                        } else {
                            let j = kfs.partition_point(|(kt, _)| *kt < *t);
                            let (t0, p0) = kfs[j-1];
                            let (t1, p1) = kfs[j];
                            let frac = if (t1 - t0).abs() < 1e-6 { 0.0 } else { (*t - t0) / (t1 - t0) };
                            [p0[0] + (p1[0] - p0[0]) * frac, p0[1] + (p1[1] - p0[1]) * frac]
                        }
                    }).collect();

                    let n = cur_positions.len() as f32;
                    let tx = -cur_positions.iter().zip(ref_positions.iter())
                        .map(|(c, r)| c[0] - r[0]).sum::<f32>() / n;
                    let ty = -cur_positions.iter().zip(ref_positions.iter())
                        .map(|(c, r)| c[1] - r[1]).sum::<f32>() / n;

                    let d_ref = {
                        let dx = ref_positions[1][0] - ref_positions[0][0];
                        let dy = ref_positions[1][1] - ref_positions[0][1];
                        (dx*dx + dy*dy).sqrt()
                    };
                    let d_cur = {
                        let dx = cur_positions[1][0] - cur_positions[0][0];
                        let dy = cur_positions[1][1] - cur_positions[0][1];
                        (dx*dx + dy*dy).sqrt()
                    };
                    let scale = if d_ref < 1e-6 { 1.0 } else { d_ref / d_cur };
                    camera_keyframes.push((*t, [tx, ty, scale]));
                }
                self.camera_tracker.camera_keyframes = camera_keyframes;
                self.camera_tracker.solved = true;
            }
            Action::CreateCameraFromTrack => {
                if !self.camera_tracker.solved {
                    return;
                }
                for (t, [tx, ty, _scale]) in &self.camera_tracker.camera_keyframes {
                    let ci = self.active_comp_index();
                    let cur_z = self.project.comps[ci].camera.position[2];
                    self.project.comps[ci].camera.position = [*tx, *ty, cur_z];
                    let _ = t;
                }
                self.host.mark_dirty();
            }
            Action::SetCameraTrackerProgress(p) => {
                self.camera_tracker.analyze_progress = p.clamp(0.0, 1.0);
            }
            Action::ClearCameraTrack => {
                self.camera_tracker = CameraTracker::default();
            }

            // --- Batch 3 extended: Rotobrush ---
            Action::SetRotobrushMode { subtract } => {
                self.rotobrush_subtract = subtract;
            }
            Action::SetRotobrushRadius(r) => {
                self.rotobrush_radius = r.max(1.0);
            }
            Action::AddRotobrushStroke { layer_id, frame, pts } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.rotobrush_strokes.push(RotobrushStroke {
                        frame,
                        pts,
                        is_subtract: self.rotobrush_subtract,
                    });
                    self.host.mark_dirty();
                }
            }
            Action::ClearRotobrushStrokes { layer_id } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.rotobrush_strokes.clear();
                    self.host.mark_dirty();
                }
            }
            Action::PropagateRotobrush { layer_id, forward_frames } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.rotobrush_propagated_frames = forward_frames;
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: Warp Stabilizer ---
            Action::SetWarpStabResult(r) => {
                self.warp_stab_config.result = r;
            }
            Action::SetWarpStabSmoothness(v) => {
                self.warp_stab_config.smoothness = v.clamp(0.0, 100.0);
            }
            Action::SetWarpStabMethod(m) => {
                self.warp_stab_config.method = m;
            }
            Action::SetWarpStabFraming(f) => {
                self.warp_stab_config.framing = f;
            }
            Action::SetWarpStabCropSmooth(v) => {
                self.warp_stab_config.crop_less_smooth_more = v.clamp(0.0, 100.0);
            }
            Action::SetWarpStabDetailedAnalysis(b) => {
                self.warp_stab_config.detailed_analysis = b;
            }
            Action::SetWarpStabRollingShutter(v) => {
                self.warp_stab_config.rolling_shutter_ripple = v.clamp(0.0, 100.0);
            }
            Action::AnalyzeWarpStab { layer_id } => {
                self.warp_stab_analyzing = true;
                self.warp_stab_progress = 0.0;
                self.warp_stab_applied_layer = Some(layer_id);
            }
            Action::WarpStabAnalysisComplete => {
                self.warp_stab_analyzing = false;
                self.warp_stab_progress = 1.0;
            }

            // --- Batch 5: Motion Sketch ---
            Action::SetMotionSketchCaptureSpeed(v) => {
                self.motion_sketch_config.capture_speed = v.clamp(0.0, 100.0);
            }
            Action::SetMotionSketchSmoothing(v) => {
                self.motion_sketch_config.smoothing = v.clamp(0.0, 100.0);
            }
            Action::SetMotionSketchShowWireframe(b) => {
                self.motion_sketch_config.show_wireframe = b;
            }
            Action::ToggleMotionSketchRecord => {
                self.motion_sketch_recording = !self.motion_sketch_recording;
            }
            Action::ApplyMotionSketchStroke(stroke) => {
                self.motion_sketch_strokes.push(stroke);
            }
            Action::ClearMotionSketchStrokes => {
                self.motion_sketch_strokes.clear();
            }
            Action::ApplyMotionSketchToLayer { layer_id: _ } => {
                self.motion_sketch_recording = false;
            }

            // --- Batch 7: CameraTracker (3D solve) ---
            Action::StartCameraTrackSolve { layer_id } => {
                let track_points: Vec<CameraTrackPoint> = (0..40usize).map(|index| CameraTrackPoint {
                    id: index,
                    x: (index * 7 % 1920) as f32,
                    y: (index * 11 % 1080) as f32,
                    z: 0.0,
                    confidence: 0.8,
                    selected: false,
                }).collect();
                self.camera_track_solves.push(CameraTrackSolve {
                    layer_id,
                    status: CameraTrackStatus::Analyzing,
                    solve_error: 0.0,
                    method: "Typical".to_string(),
                    track_points,
                    attached_layer_ids: Vec::new(),
                });
            }
            Action::SolveCameraTrackExt { layer_id } => {
                if let Some(solve) = self.camera_track_solves.iter_mut().find(|s| s.layer_id == layer_id) {
                    solve.status = CameraTrackStatus::Done;
                    solve.solve_error = 0.73;
                }
            }
            Action::SelectTrackPoints { layer_id, point_ids } => {
                if let Some(solve) = self.camera_track_solves.iter_mut().find(|s| s.layer_id == layer_id) {
                    for pt in &mut solve.track_points {
                        pt.selected = point_ids.contains(&pt.id);
                    }
                }
            }
            Action::CreateSolvedCamera { layer_id } => {
                if let Some(solve) = self.camera_track_solves.iter_mut().find(|s| s.layer_id == layer_id) {
                    if solve.status == CameraTrackStatus::Done {
                        solve.attached_layer_ids.push(layer_id);
                    }
                }
            }
            Action::DeleteCameraTrackSolve { layer_id } => {
                self.camera_track_solves.retain(|s| s.layer_id != layer_id);
            }

            _ => unreachable!("apply_tracking called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_tracker_toggle() {
        let mut app = App::new();
        assert!(!app.camera_tracker.open);
        app.apply(Action::ToggleCameraTracker);
        assert!(app.camera_tracker.open);
    }

    #[test]
    fn test_camera_tracker_add_remove_point() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "A".to_string(), pos: [100.0, 200.0] });
        app.apply(Action::AddTrackPoint { name: "B".to_string(), pos: [400.0, 300.0] });
        assert_eq!(app.camera_tracker.track_points.len(), 2);
        app.apply(Action::RemoveTrackPoint(0));
        assert_eq!(app.camera_tracker.track_points.len(), 1);
    }

    #[test]
    fn test_camera_tracker_solve() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "A".to_string(), pos: [100.0, 100.0] });
        app.apply(Action::AddTrackPoint { name: "B".to_string(), pos: [300.0, 100.0] });
        app.apply(Action::MoveTrackPoint { idx: 0, time: 1.0, pos: [110.0, 110.0] });
        app.apply(Action::MoveTrackPoint { idx: 1, time: 1.0, pos: [310.0, 110.0] });
        app.apply(Action::SolveCameraTrack);
        assert!(app.camera_tracker.solved);
        assert!(!app.camera_tracker.camera_keyframes.is_empty());
    }

    #[test]
    fn test_camera_tracker_clear() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "X".to_string(), pos: [0.0, 0.0] });
        app.apply(Action::ClearCameraTrack);
        assert!(app.camera_tracker.track_points.is_empty());
        assert!(!app.camera_tracker.solved);
    }

    #[test]
    fn test_camera_tracker_move_point() {
        let mut app = App::new();
        app.apply(Action::AddTrackPoint { name: "P".to_string(), pos: [50.0, 50.0] });
        app.apply(Action::MoveTrackPoint { idx: 0, time: 1.0, pos: [60.0, 70.0] });
        let kfs = &app.camera_tracker.track_points[0].keyframes;
        let kf = kfs.iter().find(|(t, _)| (*t - 1.0).abs() < 1e-3);
        assert!(kf.is_some());
        let pos = kf.unwrap().1;
        assert!((pos[0] - 60.0).abs() < 1e-3);
        assert!((pos[1] - 70.0).abs() < 1e-3);
    }

    #[test]
    fn test_start_camera_track_solve_creates_solve() {
        let mut app = App::new();
        assert!(app.camera_track_solves.is_empty());
        app.apply(Action::StartCameraTrackSolve { layer_id: 0 });
        assert_eq!(app.camera_track_solves.len(), 1);
        assert_eq!(app.camera_track_solves[0].layer_id, 0);
        assert_eq!(app.camera_track_solves[0].status, CameraTrackStatus::Analyzing);
        assert_eq!(app.camera_track_solves[0].track_points.len(), 40);
    }

    #[test]
    fn test_start_camera_track_stub_points_positions() {
        let mut app = App::new();
        app.apply(Action::StartCameraTrackSolve { layer_id: 2 });
        let pts = &app.camera_track_solves[0].track_points;
        assert!((pts[0].x - 0.0).abs() < 1e-5);
        assert!((pts[0].y - 0.0).abs() < 1e-5);
        assert!((pts[1].x - 7.0).abs() < 1e-5);
        assert!((pts[1].y - 11.0).abs() < 1e-5);
        for pt in pts {
            assert!((pt.confidence - 0.8).abs() < 1e-5);
        }
    }

    #[test]
    fn test_solve_camera_track_ext() {
        let mut app = App::new();
        app.apply(Action::StartCameraTrackSolve { layer_id: 0 });
        app.apply(Action::SolveCameraTrackExt { layer_id: 0 });
        assert_eq!(app.camera_track_solves[0].status, CameraTrackStatus::Done);
        assert!((app.camera_track_solves[0].solve_error - 0.73).abs() < 1e-5);
    }

    #[test]
    fn test_select_track_points() {
        let mut app = App::new();
        app.apply(Action::StartCameraTrackSolve { layer_id: 0 });
        let id0 = app.camera_track_solves[0].track_points[0].id;
        let id1 = app.camera_track_solves[0].track_points[1].id;
        app.apply(Action::SelectTrackPoints { layer_id: 0, point_ids: vec![id0] });
        assert!(app.camera_track_solves[0].track_points[0].selected);
        assert!(!app.camera_track_solves[0].track_points[1].selected);
        app.apply(Action::SelectTrackPoints { layer_id: 0, point_ids: vec![id1] });
        assert!(!app.camera_track_solves[0].track_points[0].selected);
        assert!(app.camera_track_solves[0].track_points[1].selected);
    }

    #[test]
    fn test_create_solved_camera_requires_done() {
        let mut app = App::new();
        app.apply(Action::StartCameraTrackSolve { layer_id: 0 });
        app.apply(Action::CreateSolvedCamera { layer_id: 0 });
        assert!(app.camera_track_solves[0].attached_layer_ids.is_empty());
        app.apply(Action::SolveCameraTrackExt { layer_id: 0 });
        app.apply(Action::CreateSolvedCamera { layer_id: 0 });
        assert_eq!(app.camera_track_solves[0].attached_layer_ids.len(), 1);
    }

    #[test]
    fn test_delete_camera_track_solve() {
        let mut app = App::new();
        app.apply(Action::StartCameraTrackSolve { layer_id: 0 });
        app.apply(Action::StartCameraTrackSolve { layer_id: 1 });
        assert_eq!(app.camera_track_solves.len(), 2);
        app.apply(Action::DeleteCameraTrackSolve { layer_id: 0 });
        assert_eq!(app.camera_track_solves.len(), 1);
        assert_eq!(app.camera_track_solves[0].layer_id, 1);
    }
}
