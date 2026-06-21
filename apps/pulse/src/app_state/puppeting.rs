use super::*;

impl App {
    pub(super) fn apply_puppeting(&mut self, action: Action) {
        match action {
            // --- Batch 3: Puppet pins ---
            Action::AddPuppetPin { layer_id, pos } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    let id = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(l.puppet_pins.len() as u64);
                    l.puppet_pins.push(crate::comp::PuppetPin { id, position: pos, is_stiff: false, stiffness: 0.0 });
                    self.host.mark_dirty();
                }
            }
            Action::MovePuppetPin { layer_id, pin_id, pos } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(pin) = l.puppet_pins.iter_mut().find(|p| p.id == pin_id) {
                        pin.position = pos;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::RemovePuppetPin { layer_id, pin_id } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.puppet_pins.retain(|p| p.id != pin_id);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3 extended: Puppet Pin Stiffness ---
            Action::SetPuppetPinStiffness { layer_id, pin_id, stiffness } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(pin) = layer.puppet_pins.iter_mut().find(|p| p.id == pin_id) {
                        pin.stiffness = stiffness.clamp(0.0, 1.0);
                    }
                }
            }
            Action::TogglePuppetPinStiff { layer_id, pin_id } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(pin) = layer.puppet_pins.iter_mut().find(|p| p.id == pin_id) {
                        pin.is_stiff = !pin.is_stiff;
                    }
                }
            }
            Action::SetPuppetMeshDensity { layer_id, density } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_id) {
                    layer.puppet_mesh_density = density.max(2);
                }
            }

            // --- Batch 5: Shape Layer Morphing ---
            Action::SetShapeMorphEnabled(b) => {
                self.shape_morph_config.enabled = b;
            }
            Action::AddMorphKeyframe(kf) => {
                self.shape_morph_config.keyframes.push(kf);
            }
            Action::RemoveMorphKeyframe(idx) => {
                if idx < self.shape_morph_config.keyframes.len() {
                    self.shape_morph_config.keyframes.remove(idx);
                }
            }
            Action::SetMorphMode { kf_idx, mode } => {
                if let Some(kf) = self.shape_morph_config.keyframes.get_mut(kf_idx) {
                    kf.mode = mode;
                }
            }
            Action::SetCorrespondenceMode(m) => {
                self.shape_morph_config.correspondence_mode = m;
            }
            Action::SetMorphPreviewTime(t) => {
                self.morph_preview_time = t.max(0.0);
            }
            Action::PreviewMorphAtTime(t) => {
                self.morph_preview_time = t;
            }
            Action::ClearMorphKeyframes => {
                self.shape_morph_config.keyframes.clear();
            }

            _ => unreachable!("apply_puppeting called with wrong action"),
        }
    }
}
