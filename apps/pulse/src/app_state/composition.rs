use super::*;

impl App {
    pub(super) fn apply_composition(&mut self, action: Action) {
        match action {
            Action::SetTime(t) => {
                let dur = self.active_duration();
                self.time = t.clamp(0.0, dur);
                self.host.mark_dirty();
            }
            Action::StepTime(delta) => {
                let dur = self.active_duration();
                self.time = (self.time + delta).clamp(0.0, dur);
                self.host.mark_dirty();
            }
            Action::GoToStart => {
                self.time = 0.0;
                self.host.mark_dirty();
            }
            Action::GoToEnd => {
                let ci = self.active_comp_index();
                self.time = self.project.comps[ci].duration;
                self.host.mark_dirty();
            }
            Action::ToggleLoop => {
                self.loop_enabled = !self.loop_enabled;
            }
            Action::TogglePlay => {
                self.playing = !self.playing;
                self.last_tick = self.playing.then(Instant::now);
                if !self.playing {
                    self.host.mark_dirty();
                }
            }
            Action::Pause => {
                self.playing = false;
                self.last_tick = None;
                self.host.mark_dirty();
            }
            Action::ToggleLayerVisible(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.visible = !l.visible;
                    self.host.mark_dirty();
                }
            }
            Action::SelectLayer(i) => {
                let ci = self.active_comp_index();
                if self.project.comps[ci].layers.get(i).is_some() {
                    self.selected_layer = Some(i);
                }
            }
            Action::SetTransform(prop, value) => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let t = self.time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    layer.track_mut(prop).set_key(t, value);
                    self.host.mark_dirty();
                }
            }
            Action::ToggleKeyframe(prop) => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let t = self.time;
                let cur = self.project.comps[ci].layer_value(i, prop, t);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if track.keys.is_empty() {
                        track.set_key(t, cur);
                    } else {
                        track.keys.clear();
                    }
                    self.host.mark_dirty();
                }
            }
            Action::DuplicateLayer(layer_id) => {
                let ci = self.active_comp_index();
                if layer_id < self.project.comps[ci].layers.len() {
                    let mut dup = self.project.comps[ci].layers[layer_id].clone();
                    dup.name = format!("{} (copy)", dup.name);
                    let new_idx = layer_id + 1;
                    self.project.comps[ci].layers.insert(new_idx, dup);
                    self.selected_layer = Some(new_idx);
                    self.host.mark_dirty();
                }
            }
            Action::SetWorkAreaStart(t) => {
                let ci = self.active_comp_index();
                let comp = &mut self.project.comps[ci];
                let dur = comp.duration;
                let t = t.clamp(0.0, dur);
                comp.work_area.start = t.min(comp.work_area.end);
                comp.work_area = comp.work_area.clamped(dur);
            }
            Action::SetWorkAreaEnd(t) => {
                let ci = self.active_comp_index();
                let comp = &mut self.project.comps[ci];
                let dur = comp.duration;
                let t = t.clamp(0.0, dur);
                comp.work_area.end = t.max(comp.work_area.start);
                comp.work_area = comp.work_area.clamped(dur);
            }
            Action::ResetWorkArea => {
                let ci = self.active_comp_index();
                let comp = &mut self.project.comps[ci];
                comp.work_area = WorkArea::full(comp.duration);
            }
            Action::SetExpression { layer_id, prop, expr } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    let track = match prop.as_str() {
                        "X" => Some(&mut layer.x),
                        "Y" => Some(&mut layer.y),
                        "Z" => Some(&mut layer.z),
                        "Scale" => Some(&mut layer.scale),
                        "Rotation" => Some(&mut layer.rotation),
                        "Opacity" => Some(&mut layer.opacity),
                        "AnchorX" => Some(&mut layer.anchor_x),
                        "AnchorY" => Some(&mut layer.anchor_y),
                        "OrientX" => Some(&mut layer.orient_x),
                        "OrientY" => Some(&mut layer.orient_y),
                        "OrientZ" => Some(&mut layer.orient_z),
                        _ => None,
                    };
                    if let Some(t) = track {
                        if expr.is_empty() {
                            t.expression = None;
                        } else {
                            t.expression = Some(expr.clone());
                        }
                    }
                }
                if expr.is_empty() {
                    self.expressions.remove(&(layer_id, prop));
                } else {
                    self.expressions.insert((layer_id, prop), expr);
                }
                self.host.mark_dirty();
            }
            Action::Undo => {
                if let Some(prev) = self.undo.pop() {
                    let cur = std::mem::replace(&mut self.project, prev);
                    self.redo.push(cur);
                    self.after_history_swap();
                }
            }
            Action::Redo => {
                if let Some(next) = self.redo.pop() {
                    let cur = std::mem::replace(&mut self.project, next);
                    self.undo.push(cur);
                    self.after_history_swap();
                }
            }
            Action::SetLayer3D(layer_id, enable) => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.threed = enable;
                    self.host.mark_dirty();
                }
            }
            Action::SetPositionZ(layer_id, z) => {
                let ci = self.active_comp_index();
                let t = self.time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.z.set_key(t, z);
                    self.host.mark_dirty();
                }
            }
            Action::Set3DRotation(layer_id, rx, ry, rz) => {
                let ci = self.active_comp_index();
                let t = self.time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.orient_x.set_key(t, rx);
                    layer.orient_y.set_key(t, ry);
                    layer.orient_z.set_key(t, rz);
                    self.host.mark_dirty();
                }
            }
            Action::SetParent(child, parent) => {
                let ci = self.active_comp_index();
                if child != parent {
                    self.layer_parents.insert(child, parent);
                    if let Some(layer) = self.project.comps[ci].layers.get_mut(child) {
                        layer.parent = Some(parent);
                    }
                    self.picking_parent_for = None;
                    self.host.mark_dirty();
                }
            }
            Action::ClearParent(child) => {
                let ci = self.active_comp_index();
                self.layer_parents.remove(&child);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(child) {
                    layer.parent = None;
                }
                self.host.mark_dirty();
            }
            Action::SetPickingParent(opt) => {
                self.picking_parent_for = opt;
            }
            Action::PreCompose(indices, name) => {
                if indices.is_empty() { return; }
                let ci = self.active_comp_index();
                let mut sorted = indices.clone();
                sorted.sort_unstable_by(|a, b| b.cmp(a));
                let mut taken_layers = Vec::new();
                for &idx in &sorted {
                    if idx < self.project.comps[ci].layers.len() {
                        taken_layers.push(self.project.comps[ci].layers.remove(idx));
                    }
                }
                taken_layers.reverse();
                let sub_idx = self.sub_comps.len();
                self.sub_comps.push(SubComp { name: name.clone(), layers: taken_layers });
                let insert_at = *indices.iter().min().unwrap_or(&0);
                let insert_at = insert_at.min(self.project.comps[ci].layers.len());
                let placeholder = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Null,
                    name,
                    [0.5, 0.5, 0.5, 1.0],
                );
                self.project.comps[ci].layers.insert(insert_at, placeholder);
                self.pre_comp_layers.insert(insert_at, sub_idx);
                self.host.mark_dirty();
                self.selected_layer = None;
            }
            Action::OpenSubComp(idx) => {
                self.active_sub_comp = Some(idx);
            }
            Action::CloseSubComp => {
                self.active_sub_comp = None;
            }
            Action::AddNullLayer => {
                let ci = self.active_comp_index();
                let null = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Null,
                    "Null 1",
                    [0.8, 0.8, 0.8, 1.0],
                );
                self.project.comps[ci].layers.push(null);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }
            Action::AddGuideLayer => {
                let ci = self.active_comp_index();
                let n = self.project.comps[ci].layers.len() + 1;
                let guide = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Guide,
                    &format!("Guide {n}"),
                    [0.2, 0.2, 0.8, 1.0],
                );
                self.project.comps[ci].layers.push(guide);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }
            Action::AddSolidLayer(rgba) => {
                let ci = self.active_comp_index();
                let color = [
                    rgba[0] as f32 / 255.0,
                    rgba[1] as f32 / 255.0,
                    rgba[2] as f32 / 255.0,
                    rgba[3] as f32 / 255.0,
                ];
                let layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Solid,
                    "Solid 1",
                    color,
                );
                self.project.comps[ci].layers.push(layer);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }
            Action::AddAdjustmentLayer => {
                let ci = self.active_comp_index();
                let layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::Adjustment,
                    "Adjustment 1",
                    [1.0, 1.0, 1.0, 1.0],
                );
                self.project.comps[ci].layers.push(layer);
                self.selected_layer = Some(self.project.comps[ci].layers.len() - 1);
                self.host.mark_dirty();
            }
            Action::ImportVideoFootage => {
                let path = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mov", "mkv", "avi", "webm", "mts", "m2ts"])
                    .pick_file();
                if let Some(path) = path {
                    match prism_media::probe(&path) {
                        Ok(info) => {
                            let ci = self.active_comp_index();
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Video")
                                .to_string();
                            let frame_count =
                                (info.duration_secs * info.fps).ceil() as u64;
                            let mut layer = crate::comp::PulseLayer::of_kind(
                                crate::comp::LayerKind::Footage,
                                &name,
                                [0.4, 0.4, 0.8, 1.0],
                            );
                            layer.footage.source = Some(crate::comp::FootageSource::Video {
                                path: path.clone(),
                                fps: info.fps,
                                frame_count: frame_count.max(1),
                                width: info.width,
                                height: info.height,
                            });
                            self.project.comps[ci].layers.push(layer);
                            self.selected_layer =
                                Some(self.project.comps[ci].layers.len() - 1);
                            self.host.mark_dirty();
                        }
                        Err(e) => {
                            log::warn!("Video probe failed (ffprobe not installed?): {e}");
                            let ci = self.active_comp_index();
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Video")
                                .to_string();
                            let mut layer = crate::comp::PulseLayer::of_kind(
                                crate::comp::LayerKind::Footage,
                                &name,
                                [0.4, 0.4, 0.8, 1.0],
                            );
                            layer.footage.source =
                                Some(crate::comp::FootageSource::still(path));
                            self.project.comps[ci].layers.push(layer);
                            self.selected_layer =
                                Some(self.project.comps[ci].layers.len() - 1);
                            self.host.mark_dirty();
                        }
                    }
                }
            }
            Action::SetFootageVideo { layer_idx, path, fps, frame_count, width, height } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    layer.footage.source = Some(crate::comp::FootageSource::Video {
                        path,
                        fps,
                        frame_count,
                        width,
                        height,
                    });
                    self.host.mark_dirty();
                }
            }
            Action::AddCompMarker { time, label } => {
                let ci = self.active_comp_index();
                let mut m = crate::comp::Marker::at(time);
                m.label = label;
                self.project.comps[ci].markers.push(m);
            }
            Action::RemoveCompMarker(idx) => {
                let ci = self.active_comp_index();
                let markers = &mut self.project.comps[ci].markers;
                if idx < markers.len() {
                    markers.remove(idx);
                }
            }
            Action::AddLayerMarker { layer_idx, time, label } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    let mut m = crate::comp::Marker::at(time);
                    m.label = label;
                    layer.markers.push(m);
                }
            }
            Action::RemoveLayerMarker { layer_idx, idx } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    if idx < layer.markers.len() {
                        layer.markers.remove(idx);
                    }
                }
            }
            Action::SetROI(roi) => {
                self.roi = roi;
                self.host.mark_dirty();
            }
            Action::SetAnchorPoint { layer_idx, x, y } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    layer.anchor_x.set_key(0.0, x);
                    layer.anchor_y.set_key(0.0, y);
                    self.host.mark_dirty();
                }
            }
            Action::SetCameraPosition(pos) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].camera.position = pos;
                self.host.mark_dirty();
            }
            Action::SetCameraFov(fov) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].camera.fov_deg = fov.clamp(1.0, 179.0);
                self.host.mark_dirty();
            }
            Action::SetParentLayer { child, parent } => {
                let ci = self.active_comp_index();
                if child < self.project.comps[ci].layers.len() {
                    match parent {
                        Some(p) if p != child && p < self.project.comps[ci].layers.len() => {
                            self.project.comps[ci].layers[child].parent = Some(p);
                            self.layer_parents.insert(child, p);
                        }
                        _ => {
                            self.project.comps[ci].layers[child].parent = None;
                            self.layer_parents.remove(&child);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetTimeRemapEnabled { layer_id, enabled } => {
                let ci = self.active_comp_index();
                let dur = self.project.comps[ci].duration;
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap.enabled = enabled;
                    if enabled && l.time_remap.track.keys.is_empty() {
                        l.time_remap.seed_default(dur, None);
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetTimeRemapKey { layer_id, comp_time, source_time } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap.track.set_key(comp_time as f32, source_time as f32);
                    self.host.mark_dirty();
                }
            }
            Action::SetLayerMatte { layer_id, mode } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.matte = mode;
                    self.host.mark_dirty();
                }
            }
            Action::ToggleSolo(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.solo = !l.solo;
                    self.host.mark_dirty();
                }
            }
            Action::ToggleShy(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.shy = !l.shy;
                }
            }
            Action::ToggleHideShy => {
                let ci = self.active_comp_index();
                self.project.comps[ci].hide_shy = !self.project.comps[ci].hide_shy;
            }
            Action::AddLight(light) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].lights.push(light);
                self.host.mark_dirty();
            }
            Action::RemoveLight(i) => {
                let ci = self.active_comp_index();
                if i < self.project.comps[ci].lights.len() {
                    self.project.comps[ci].lights.remove(i);
                    self.host.mark_dirty();
                }
            }
            Action::UpdateLight { index, light } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].lights.get_mut(index) {
                    *l = light;
                    self.host.mark_dirty();
                }
            }
            Action::SetMotionBlurEnabled(on) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.enabled = on;
                self.host.mark_dirty();
            }
            Action::SetMotionBlurAngle(a) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.angle = a.clamp(1.0, 720.0);
                self.host.mark_dirty();
            }
            Action::SetMotionBlurPhase(p) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.phase = p.clamp(-360.0, 360.0);
                self.host.mark_dirty();
            }
            Action::SetMotionBlurSamples(n) => {
                let ci = self.active_comp_index();
                self.project.comps[ci].motion_blur.samples = n.clamp(1, 64);
                self.host.mark_dirty();
            }
            Action::ToggleLayerMotionBlur(i) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(i) {
                    l.motion_blur = !l.motion_blur;
                    self.host.mark_dirty();
                }
            }
            Action::SplitLayer(id) => {
                let t = self.time;
                // Inline SplitLayerAt logic:
                let ci = self.active_comp_index();
                let dur = self.project.comps[ci].duration;
                let time = t.clamp(0.0, dur);
                if id >= self.project.comps[ci].layers.len() { return; }
                let mut second = self.project.comps[ci].layers[id].clone();
                self.project.comps[ci].layers[id].out_point = Some(time);
                second.in_point = Some(time);
                for track in [
                    &mut second.x, &mut second.y, &mut second.scale,
                    &mut second.rotation, &mut second.opacity,
                    &mut second.anchor_x, &mut second.anchor_y,
                    &mut second.z, &mut second.orient_x, &mut second.orient_y, &mut second.orient_z,
                ] {
                    for key in &mut track.keys { key.t = (key.t - time).max(0.0); }
                }
                self.project.comps[ci].layers.insert(id + 1, second);
                self.host.mark_dirty();
            }
            Action::SplitLayerAt { layer_id, time } => {
                let ci = self.active_comp_index();
                let dur = self.project.comps[ci].duration;
                let t = time.clamp(0.0, dur);
                if layer_id >= self.project.comps[ci].layers.len() {
                    return;
                }
                let mut second = self.project.comps[ci].layers[layer_id].clone();
                self.project.comps[ci].layers[layer_id].out_point = Some(t);
                second.in_point = Some(t);
                for track in [
                    &mut second.x, &mut second.y, &mut second.scale,
                    &mut second.rotation, &mut second.opacity,
                    &mut second.anchor_x, &mut second.anchor_y,
                    &mut second.z, &mut second.orient_x, &mut second.orient_y, &mut second.orient_z,
                ] {
                    for key in &mut track.keys {
                        key.t = (key.t - t).max(0.0);
                    }
                }
                self.project.comps[ci].layers.insert(layer_id + 1, second);
                self.host.mark_dirty();
            }
            Action::SetLayerTimeStretch { layer_id, factor } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.time_stretch = factor.max(0.01);
                    self.host.mark_dirty();
                }
            }
            Action::SetLayerAudioFade { layer_id, fade_in, fade_out } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.audio_fade_in = fade_in.max(0.0);
                    layer.audio_fade_out = fade_out.max(0.0);
                }
            }
            Action::SetLayerEcho { layer_id, config } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.echo = Some(config);
                    self.host.mark_dirty();
                }
            }
            Action::ClearLayerEcho { layer_id } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.echo = None;
                    self.host.mark_dirty();
                }
            }

            // --- UI / tool state ---
            Action::SetTool(t) => {
                self.active = t;
            }
            Action::SetHoveredGizmoHandle(h) => {
                self.hovered_gizmo_handle = h;
            }

            // --- Batch 4: 3D Camera depth ---
            Action::SetDepthOfField(d) => {
                self.dof = d;
            }
            Action::SetDofEnabled(b) => {
                self.dof.enabled = b;
            }
            Action::SetDofFocusDistance(d) => {
                self.dof.focus_distance = d.max(0.0);
            }
            Action::SetDofAperture(a) => {
                self.dof.aperture = a.clamp(1.4, 22.0);
            }
            Action::SetDofBlurLevel(l) => {
                self.dof.blur_level = l.clamp(0.0, 300.0);
            }
            Action::SetCameraZoom(z) => {
                self.camera_zoom = z.max(0.01);
            }
            Action::SetCameraPointOfInterest(p) => {
                self.camera_point_of_interest = p;
            }
            Action::SetCameraOrbitSpeed(s) => {
                self.camera_orbit_speed = s;
            }
            Action::ResetCamera => {
                self.dof = DepthOfField::default();
                self.camera_zoom = 1.0;
                self.camera_point_of_interest = [0.0, 0.0, 0.0];
                self.camera_orbit_speed = 0.0;
            }

            _ => unreachable!("apply_composition called with wrong action"),
        }
    }
}
