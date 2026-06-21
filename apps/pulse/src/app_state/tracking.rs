use super::*;

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

            _ => unreachable!("apply_tracking called with wrong action"),
        }
    }
}
