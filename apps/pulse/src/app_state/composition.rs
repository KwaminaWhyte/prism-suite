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

            // All layer-management / footage / marker / split / light /
            // comp-motion-blur / 3-D-camera arms live in `composition_layers.rs`
            // to keep this file under the 1000-line limit.
            _ => self.apply_composition_layers(action),
        }
    }
}
