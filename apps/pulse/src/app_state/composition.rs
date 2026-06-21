use super::*;

/// Pending (not-yet-applied) composition settings for the settings dialog.
#[derive(Clone, Debug)]
pub struct PendingCompSettings {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_secs: f32,
    pub bg_color: [f32; 4],
}

/// Cap on the undo/redo snapshot stack depth. Each entry is a full [`Project`]
/// clone; 64 levels is generous for an interactive session while bounding RAM.
const UNDO_LIMIT: usize = 64;

// ── Batch 4: Depth of Field / Camera ─────────────────────────────────────────

/// Iris shape for the depth-of-field blur.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum IrisShape {
    #[default]
    Fast,
    Hexagon,
    Octagon,
    Circle,
    Square,
    Blade(u8),
}

/// Depth-of-field settings attached to the active comp's camera.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DepthOfField {
    pub enabled: bool,
    pub focus_distance: f32,
    pub aperture: f32,
    pub blur_level: f32,
    pub iris_shape: IrisShape,
}

impl Default for DepthOfField {
    fn default() -> Self {
        Self {
            enabled: false,
            focus_distance: 500.0,
            aperture: 5.6,
            blur_level: 100.0,
            iris_shape: IrisShape::Fast,
        }
    }
}

// ── Batch 4: Collect Files / Package project ──────────────────────────────────

/// Configuration for the Collect Files / Package project feature.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectFilesConfig {
    pub destination: std::path::PathBuf,
    pub include_footage: bool,
    pub include_proxies: bool,
    pub generate_report: bool,
    pub reduce_project: bool,
}

impl Default for CollectFilesConfig {
    fn default() -> Self {
        Self {
            destination: std::path::PathBuf::from("."),
            include_footage: true,
            include_proxies: false,
            generate_report: true,
            reduce_project: false,
        }
    }
}

/// Full 3-D spatial configuration for a layer (enabled when the 3-D flag is set).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Layer3DConfig {
    pub enabled: bool,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub orientation: [f32; 3],
    pub scale: [f32; 3],
    pub anchor_point: [f32; 3],
    pub casts_shadows: bool,
    pub accepts_shadows: bool,
    pub casts_lights: bool,
    pub appears_in_reflections: bool,
    pub material_shininess: f32,
    pub material_metal: f32,
}

impl Default for Layer3DConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
            scale: [100.0, 100.0, 100.0],
            anchor_point: [0.0, 0.0, 0.0],
            casts_shadows: false,
            accepts_shadows: true,
            casts_lights: false,
            appears_in_reflections: true,
            material_shininess: 50.0,
            material_metal: 0.0,
        }
    }
}


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

            // --- Welcome screen ---
            Action::DismissWelcome | Action::NewComposition | Action::OpenProject => {
                self.welcome_visible = false;
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

#[cfg(test)]
mod tests {
    use super::*;
    // ── Batch 3 extended: Time Stretch ───────────────────────────────────────

    #[test]
    fn test_time_stretch_set() {
        let mut app = App::new();
        app.apply(Action::SetLayerTimeStretch { layer_id: 0, factor: 2.0 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].time_stretch - 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_time_stretch_clamp() {
        let mut app = App::new();
        // Setting factor to 0.0 should clamp to 0.01
        app.apply(Action::SetLayerTimeStretch { layer_id: 0, factor: 0.0 });
        let ci = app.active_comp_index();
        let stretch = app.project.comps[ci].layers[0].time_stretch;
        assert!(stretch >= 0.01, "time_stretch must be at least 0.01, got {stretch}");
    }

    // ── Batch 3 extended: Audio Fades ────────────────────────────────────────

    #[test]
    fn test_audio_fade_set() {
        let mut app = App::new();
        app.apply(Action::SetLayerAudioFade { layer_id: 0, fade_in: 1.5, fade_out: 2.0 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].audio_fade_in - 1.5).abs() < 1e-4);
        assert!((app.project.comps[ci].layers[0].audio_fade_out - 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_audio_fade_clamp_negative() {
        let mut app = App::new();
        app.apply(Action::SetLayerAudioFade { layer_id: 0, fade_in: -1.0, fade_out: -5.0 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].audio_fade_in - 0.0).abs() < 1e-4);
        assert!((app.project.comps[ci].layers[0].audio_fade_out - 0.0).abs() < 1e-4);
    }

    // ── Batch 3 extended: Puppet Pin Stiffness ───────────────────────────────

    #[test]
    fn test_puppet_stiffness_set() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPin { layer_id: 0, pos: [100.0, 100.0] });
        let ci = app.active_comp_index();
        let pin_id = app.project.comps[ci].layers[0].puppet_pins[0].id;

        app.apply(Action::SetPuppetPinStiffness { layer_id: 0, pin_id, stiffness: 0.75 });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].puppet_pins[0].stiffness - 0.75).abs() < 1e-4);
    }

    #[test]
    fn test_puppet_stiff_toggle() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPin { layer_id: 0, pos: [50.0, 50.0] });
        let ci = app.active_comp_index();
        let pin_id = app.project.comps[ci].layers[0].puppet_pins[0].id;
        assert!(!app.project.comps[ci].layers[0].puppet_pins[0].is_stiff);

        app.apply(Action::TogglePuppetPinStiff { layer_id: 0, pin_id });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].puppet_pins[0].is_stiff);

        app.apply(Action::TogglePuppetPinStiff { layer_id: 0, pin_id });
        let ci = app.active_comp_index();
        assert!(!app.project.comps[ci].layers[0].puppet_pins[0].is_stiff);
    }

    #[test]
    fn test_puppet_mesh_density() {
        let mut app = App::new();
        app.apply(Action::SetPuppetMeshDensity { layer_id: 0, density: 8 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].puppet_mesh_density, 8);

        // Clamp: density below 2 → 2
        app.apply(Action::SetPuppetMeshDensity { layer_id: 0, density: 0 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].puppet_mesh_density, 2);
    }

    // ── Batch 3 extended: Echo Effect ────────────────────────────────────────

    #[test]
    fn test_echo_set_clear() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].echo.is_none());

        app.apply(Action::SetLayerEcho {
            layer_id: 0,
            config: EchoConfig::default(),
        });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].echo.is_some());

        app.apply(Action::ClearLayerEcho { layer_id: 0 });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].echo.is_none());
    }

    #[test]
    fn test_echo_config_params() {
        let mut app = App::new();
        let config = EchoConfig {
            delay_seconds: 0.25,
            count: 5,
            decay: 0.8,
            blend_mode: 1,
        };
        app.apply(Action::SetLayerEcho { layer_id: 0, config });
        let ci = app.active_comp_index();
        let echo = app.project.comps[ci].layers[0].echo.as_ref().unwrap();
        assert!((echo.delay_seconds - 0.25).abs() < 1e-4);
        assert_eq!(echo.count, 5);
        assert!((echo.decay - 0.8).abs() < 1e-4);
        assert_eq!(echo.blend_mode, 1);
    }

    // ── Batch 4: DoF / Camera ─────────────────────────────────────────────────

    #[test]
    fn test_dof_enabled_toggle() {
        let mut app = App::new();
        assert!(!app.dof.enabled);
        app.apply(Action::SetDofEnabled(true));
        assert!(app.dof.enabled);
        app.apply(Action::SetDofEnabled(false));
        assert!(!app.dof.enabled);
    }

    #[test]
    fn test_dof_aperture_clamp() {
        let mut app = App::new();
        // Below minimum → clamped to 1.4
        app.apply(Action::SetDofAperture(0.5));
        assert!((app.dof.aperture - 1.4).abs() < 1e-4, "below min clamps to 1.4");
        // Above maximum → clamped to 22.0
        app.apply(Action::SetDofAperture(100.0));
        assert!((app.dof.aperture - 22.0).abs() < 1e-4, "above max clamps to 22.0");
    }

    #[test]
    fn test_dof_blur_clamp() {
        let mut app = App::new();
        // Above maximum → clamped to 300
        app.apply(Action::SetDofBlurLevel(500.0));
        assert!((app.dof.blur_level - 300.0).abs() < 1e-4, "above max clamps to 300");
        // Below minimum → clamped to 0
        app.apply(Action::SetDofBlurLevel(-10.0));
        assert!((app.dof.blur_level - 0.0).abs() < 1e-4, "below min clamps to 0");
    }

    #[test]
    fn test_camera_zoom_min() {
        let mut app = App::new();
        // Setting zoom to 0.0 should clamp to 0.01
        app.apply(Action::SetCameraZoom(0.0));
        assert!(app.camera_zoom >= 0.01, "zoom clamped to 0.01, got {}", app.camera_zoom);
    }

    #[test]
    fn test_reset_camera() {
        let mut app = App::new();
        app.apply(Action::SetCameraZoom(5.0));
        app.apply(Action::SetDofEnabled(true));
        app.apply(Action::SetCameraPointOfInterest([100.0, 200.0, 50.0]));
        app.apply(Action::SetCameraOrbitSpeed(45.0));
        app.apply(Action::ResetCamera);
        assert!((app.camera_zoom - 1.0).abs() < 1e-4, "zoom reset to 1.0");
        assert!(!app.dof.enabled, "dof disabled after reset");
        assert_eq!(app.camera_point_of_interest, [0.0, 0.0, 0.0]);
        assert!((app.camera_orbit_speed - 0.0).abs() < 1e-4);
    }

    // ── Batch 4: Brainstorm depth ─────────────────────────────────────────────



    // ── Batch 4: Collect Files ────────────────────────────────────────────────

    #[test]
    fn test_collect_files_panel_toggle() {
        let mut app = App::new();
        assert!(!app.collect_files_panel_open);
        app.apply(Action::ToggleCollectFilesPanel);
        assert!(app.collect_files_panel_open);
        app.apply(Action::ToggleCollectFilesPanel);
        assert!(!app.collect_files_panel_open);
    }

    #[test]
    fn test_collect_run_sets_result() {
        let mut app = App::new();
        assert!(app.last_collect_result.is_none());
        app.apply(Action::RunCollectFiles);
        assert!(app.last_collect_result.is_some());
        let result = app.last_collect_result.as_ref().unwrap();
        assert!(result.contains("Collected"), "result should mention 'Collected': {result}");
    }

    #[test]
    fn test_collect_destination() {
        let mut app = App::new();
        let dest = std::path::PathBuf::from("/tmp/my_project");
        app.apply(Action::SetCollectDestination(dest.clone()));
        assert_eq!(app.collect_files_config.destination, dest);
        // Run and verify result mentions the path
        app.apply(Action::RunCollectFiles);
        let result = app.last_collect_result.as_ref().unwrap();
        assert!(result.contains("my_project"), "result should reference destination: {result}");
    }


    use super::*;
    use crate::comp::filter_grouped;

    /// The number of colour effects on the selected layer of the active comp.
    fn color_effect_count(app: &App) -> usize {
        let ci = app.active_comp_index();
        app.project.comps[ci].layers[app.selected_layer.unwrap()]
            .effects
            .len()
    }

    /// Find a browser entry by display name (the registry is the engine's).
    fn entry(name: &str) -> crate::comp::BrowserEntry {
        filter_grouped("")
            .into_iter()
            .flat_map(|(_, hits)| hits)
            .map(|h| *h.entry)
            .find(|e| e.name == name)
            .expect("entry present")
    }

    #[test]
    fn add_remove_effect_is_undoable() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        let before = color_effect_count(&app);

        // Add a Levels colour effect → stack grows, undo available.
        app.apply(Action::AddEffect(entry("Levels")));
        assert_eq!(color_effect_count(&app), before + 1);
        assert!(app.can_undo());

        // Undo removes it; redo re-adds it.
        app.apply(Action::Undo);
        assert_eq!(color_effect_count(&app), before);
        assert!(app.can_redo());
        app.apply(Action::Redo);
        assert_eq!(color_effect_count(&app), before + 1);

        // Remove it explicitly, then undo restores it.
        let idx = color_effect_count(&app) - 1;
        app.apply(Action::RemoveEffect {
            stack: EffectStack::Color,
            index: idx,
        });
        assert_eq!(color_effect_count(&app), before);
        app.apply(Action::Undo);
        assert_eq!(color_effect_count(&app), before + 1);
    }

    #[test]
    fn set_effect_param_edits_and_undoes() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        app.apply(Action::AddEffect(entry("Brightness & Contrast")));
        let ei = color_effect_count(&app) - 1;

        let read = |app: &App| {
            let ci = app.active_comp_index();
            let e = &app.project.comps[ci].layers[0].effects[ei];
            effect_params::color_params(e)[0].value // Brightness
        };
        let original = read(&app);
        app.apply(Action::SetEffectParam {
            stack: EffectStack::Color,
            index: ei,
            param: 0,
            value: 0.5,
        });
        assert_eq!(read(&app), 0.5);
        app.apply(Action::Undo);
        assert_eq!(read(&app), original);
    }

    #[test]
    fn fresh_edit_clears_redo() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        app.apply(Action::AddEffect(entry("Levels")));
        app.apply(Action::Undo);
        assert!(app.can_redo());
        // A new edit invalidates the redo stack (linear history).
        app.apply(Action::AddEffect(entry("Exposure")));
        assert!(!app.can_redo());
    }

    #[test]
    fn transport_actions_are_not_undoable() {
        let mut app = App::new();
        app.apply(Action::SetTime(1.0));
        app.apply(Action::TogglePlay);
        app.apply(Action::SelectLayer(0));
        // None of those mutate the document, so the undo stack stays empty.
        assert!(!app.can_undo());
    }

    #[test]
    fn work_area_actions_trim_and_loop() {
        let mut app = App::new();
        let dur = app.active_duration();
        // Trim the work area to a real sub-range.
        app.apply(Action::SetWorkAreaStart(1.0));
        app.apply(Action::SetWorkAreaEnd(3.0));
        let wa = app.active_work_area();
        assert!((wa.start - 1.0).abs() < 1e-4);
        assert!((wa.end - 3.0).abs() < 1e-4);
        // The work-area edit is undoable; undo restores the full range.
        assert!(app.can_undo());

        // The play loop wraps within the work area: a playhead at the end (or
        // before the in-point) snaps back to the in-point.
        app.playing = true;
        app.last_tick = Some(Instant::now());
        app.time = 2.99; // near the out-point
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.tick();
        // Whatever the exact dt, the playhead stays within [in, out] (it wrapped
        // to `in` if it crossed `out`).
        let wa = app.active_work_area();
        assert!(app.time >= wa.start - 1e-3 && app.time <= wa.end + 0.05);

        // A start past the end clamps to the end (no inversion).
        app.apply(Action::SetWorkAreaStart(dur + 5.0));
        let wa = app.active_work_area();
        assert!(wa.start <= wa.end + 1e-4);

        // Reset spans the whole timeline again.
        app.apply(Action::ResetWorkArea);
        assert!(app.active_work_area().is_full(dur));
    }

    #[test]
    fn default_export_range_follows_work_area() {
        use crate::render::RenderRange;
        let mut app = App::new();
        // A full work area defaults to the full comp.
        assert_eq!(app.default_export_range(), RenderRange::Full);
        // A trimmed work area defaults to the work area.
        app.apply(Action::SetWorkAreaStart(1.0));
        app.apply(Action::SetWorkAreaEnd(3.0));
        assert_eq!(app.default_export_range(), RenderRange::WorkArea);
    }

    #[test]
    fn undo_drops_stale_selection() {
        // Removing the last layer then undoing keeps the selection valid; here we
        // just check selection clamps when a restored project would orphan it.
        let mut app = App::new();
        let ci = app.active_comp_index();
        let last = app.project.comps[ci].layers.len() - 1;
        app.apply(Action::SelectLayer(last));
        // Add an effect so there's an undoable step, then undo: selection must
        // remain in range (it does here — same layer count) and not panic.
        app.apply(Action::AddEffect(entry("Levels")));
        app.apply(Action::Undo);
        assert_eq!(app.selected_layer, Some(last));
    }

    /// Paint a synthetic preview rect so the comp↔screen mapping is defined: a
    /// 320-wide image whose comp is `comp.width`, centred at the origin for easy
    /// math (comp space center maps to (160, 90) here).
    fn set_preview_rect(app: &App, w: f32, h: f32) {
        use gpui::{px, size, Bounds, Point};
        app.preview_rect.set(Some(Bounds {
            origin: Point {
                x: px(0.0),
                y: px(0.0),
            },
            size: size(px(w), px(h)),
        }));
    }

    #[test]
    fn graph_toggle_and_shown_props() {
        let mut app = App::new();
        assert!(!app.graph_open);
        app.apply(Action::ToggleGraph);
        assert!(app.graph_open);
        // Toggle a prop into the shown set, then clear.
        app.apply(Action::ToggleGraphProp(Prop::X));
        assert_eq!(app.graph_shown, vec![Prop::X]);
        app.apply(Action::ToggleGraphProp(Prop::X));
        assert!(app.graph_shown.is_empty());
        app.apply(Action::ToggleGraphProp(Prop::Scale));
        app.apply(Action::ClearGraphProps);
        assert!(app.graph_shown.is_empty());
        // Toggles are pure UI: no undo entry.
        assert!(!app.can_undo());
    }

    #[test]
    fn set_interp_promotes_segment_and_is_undoable() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        // Lay down two keys on X so there's a segment to ease.
        app.apply(Action::ToggleKeyframe(Prop::X)); // first key at t=0
        app.apply(Action::SetTime(1.0));
        app.apply(Action::SetTransform(Prop::X, 100.0)); // second key at t=1
        let ci = app.active_comp_index();
        // The outgoing key (index 0) starts non-eased.
        let before = app.project.comps[ci].layers[0].track(Prop::X).keys[0].interp;
        assert!(!matches!(before, Interp::Ease(_)));
        app.apply(Action::SetInterp {
            prop: Prop::X,
            key_index: 0,
            interp: Interp::Ease(crate::comp::Ease::EASY),
        });
        let after = app.project.comps[ci].layers[0].track(Prop::X).keys[0].interp;
        assert!(matches!(after, Interp::Ease(_)));
        // Undoable: undo restores the pre-ease interp.
        app.apply(Action::Undo);
        let restored = app.project.comps[ci].layers[0].track(Prop::X).keys[0].interp;
        assert!(!matches!(restored, Interp::Ease(_)));
    }

    #[test]
    fn move_keyframe_xy_retimes_and_revalues() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        app.apply(Action::ToggleKeyframe(Prop::Opacity)); // key at t=0
        let ci = app.active_comp_index();
        let v0 = app.project.comps[ci].layers[0].track(Prop::Opacity).keys[0].value;
        app.apply(Action::MoveKeyframeXY {
            prop: Prop::Opacity,
            key_index: 0,
            time: 0.5,
            value: v0 - 0.3,
        });
        let k = app.project.comps[ci].layers[0].track(Prop::Opacity).keys[0];
        assert!((k.t - 0.5).abs() < 1e-4);
        assert!((k.value - (v0 - 0.3)).abs() < 1e-4);
        assert!(app.can_undo());
    }

    #[test]
    fn gizmo_drag_moves_selected_layer_and_keys() {
        let mut app = App::new();
        app.apply(Action::SelectLayer(0));
        set_preview_rect(&app, 320.0, 180.0);
        // Grab the layer body (its center in comp space) and drag it.
        let geom = app.selected_gizmo().expect("gizmo for selected layer");
        let ((cx, cy), scale) = app.preview_fit().expect("fit defined");
        // Screen position of the gizmo's anchor's *body*: pick a point inside the
        // box — the box center (average of corners).
        let bcx = geom.corners.iter().map(|c| c.0).sum::<f32>() / 4.0;
        let bcy = geom.corners.iter().map(|c| c.1).sum::<f32>() / 4.0;
        let sx = cx + bcx * scale;
        let sy = cy + bcy * scale;
        assert!(app.begin_gizmo_drag(sx, sy, 8.0));
        let ci = app.active_comp_index();
        let x_before = app.project.comps[ci].layers[0].track(Prop::X).sample(app.time, 0.0);
        // Drag right by 40 screen px → +40/scale comp px on X.
        app.update_gizmo_drag(sx + 40.0, sy);
        let x_after = app.project.comps[ci].layers[0].track(Prop::X).sample(app.time, 0.0);
        assert!(x_after > x_before, "x should increase: {x_before} -> {x_after}");
        // The drag keyed the transform (undoable).
        assert!(app.can_undo());
    }

    #[test]
    fn layer_at_pointer_picks_under_cursor() {
        let app = App::new();
        set_preview_rect(&app, 320.0, 180.0);
        // The topmost layer's quad center should resolve to a layer index.
        let ci = app.active_comp_index();
        let top = app.project.comps[ci].layers.len() - 1;
        let world = app.project.comps[ci].world_matrix(top, app.time);
        let (wx, wy) = world.apply(0.0, 0.0);
        let ((cx, cy), scale) = app.preview_fit().unwrap();
        let hit = app.layer_at_pointer(cx + wx * scale, cy + wy * scale);
        assert!(hit.is_some());
    }

    // ---- Wave 8 tests ----

    #[test]
    fn set_layer_3d_toggles_flag_and_is_undoable() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(!app.project.comps[ci].layers[0].threed);
        app.apply(Action::SetLayer3D(0, true));
        assert!(app.project.comps[ci].layers[0].threed);
        assert!(app.can_undo());
        app.apply(Action::Undo);
        assert!(!app.project.comps[ci].layers[0].threed);
    }

    #[test]
    fn set_position_z_keys_z_track() {
        let mut app = App::new();
        app.apply(Action::SetLayer3D(0, true));
        app.apply(Action::SetTime(0.5));
        app.apply(Action::SetPositionZ(0, 200.0));
        let ci = app.active_comp_index();
        let z = app.project.comps[ci].layers[0].z.sample(0.5, 0.0);
        assert!((z - 200.0).abs() < 1e-3);
        assert!(app.can_undo());
    }

    #[test]
    fn set_3d_rotation_keys_orient_tracks() {
        let mut app = App::new();
        app.apply(Action::SetLayer3D(0, true));
        app.apply(Action::Set3DRotation(0, 10.0, 20.0, 30.0));
        let ci = app.active_comp_index();
        let l = &app.project.comps[ci].layers[0];
        assert!((l.orient_x.sample(app.time, 0.0) - 10.0).abs() < 1e-3);
        assert!((l.orient_y.sample(app.time, 0.0) - 20.0).abs() < 1e-3);
        assert!((l.orient_z.sample(app.time, 0.0) - 30.0).abs() < 1e-3);
        assert!(app.can_undo());
    }

    #[test]
    fn duplicate_layer_inserts_copy_and_updates_selection() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let before = app.project.comps[ci].layers.len();
        let orig_name = app.project.comps[ci].layers[0].name.clone();
        app.apply(Action::DuplicateLayer(0));
        let after = app.project.comps[ci].layers.len();
        assert_eq!(after, before + 1);
        assert_eq!(app.selected_layer, Some(1));
        assert!(app.project.comps[ci].layers[1].name.contains("copy"));
        assert_eq!(app.project.comps[ci].layers[0].name, orig_name);
        assert!(app.can_undo());
    }

    #[test]
    fn set_expression_writes_to_layer_track_and_map() {
        let mut app = App::new();
        app.apply(Action::SetExpression {
            layer_id: 0,
            prop: "X".to_string(),
            expr: "time * 50".to_string(),
        });
        let ci = app.active_comp_index();
        let expr = app.project.comps[ci].layers[0].x.expression.as_deref();
        assert_eq!(expr, Some("time * 50"));
        assert!(app.expressions.contains_key(&(0, "X".to_string())));
        // Clear by setting empty.
        app.apply(Action::SetExpression {
            layer_id: 0,
            prop: "X".to_string(),
            expr: String::new(),
        });
        let expr = app.project.comps[ci].layers[0].x.expression.as_deref();
        assert_eq!(expr, None);
        assert!(!app.expressions.contains_key(&(0, "X".to_string())));
    }

    #[test]
    fn hovered_gizmo_handle_is_not_undoable() {
        let mut app = App::new();
        app.apply(Action::SetHoveredGizmoHandle(Some(GizmoHandle::Rotate)));
        assert_eq!(app.hovered_gizmo_handle, Some(GizmoHandle::Rotate));
        // Pure UI — no undo entry.
        assert!(!app.can_undo());
    }
}
