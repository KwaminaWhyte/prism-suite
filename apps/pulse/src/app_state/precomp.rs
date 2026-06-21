use super::*;

impl App {
    pub(super) fn apply_precomp(&mut self, action: Action) {
        match action {
            // --- Batch 6 depth: Track Matte ---
            Action::SetTrackMatte { layer_id, config } => {
                self.track_matte_configs.insert(layer_id, config);
            }
            Action::SetTrackMatteMode { layer_id, mode } => {
                self.track_matte_configs.entry(layer_id).or_default().mode = mode;
            }
            Action::SetTrackMatteSource { layer_id, matte_layer } => {
                self.track_matte_configs.entry(layer_id).or_default().matte_layer_id = matte_layer;
            }
            Action::ToggleTrackMatteInvert { layer_id } => {
                let entry = self.track_matte_configs.entry(layer_id).or_default();
                entry.invert = !entry.invert;
            }
            Action::SetTrackMattePreserveTransparency { layer_id, preserve } => {
                self.track_matte_configs.entry(layer_id).or_default().preserve_transparency = preserve;
            }
            Action::ClearTrackMatte { layer_id } => {
                self.track_matte_configs.remove(&layer_id);
            }
            Action::ToggleTrackMattePanel => {
                self.track_matte_panel_open = !self.track_matte_panel_open;
            }

            // --- Batch 6 depth: Precomp ---
            Action::SetPrecompName(name) => {
                self.precomp_config.name = name;
            }
            Action::SetPrecompMoveAttribs(v) => {
                self.precomp_config.move_all_attributes = v;
            }
            Action::SetPrecompAdjustDuration(v) => {
                self.precomp_config.adjust_comp_duration = v;
            }
            Action::PrecomposeSelected => {
                let ci = self.active_comp_index();
                let layer_ids: Vec<usize> = self.selected_layer
                    .map(|i| vec![i])
                    .unwrap_or_else(|| {
                        if self.project.comps[ci].layers.is_empty() { vec![] } else { vec![0] }
                    });
                let duration_frames = (self.project.comps[ci].duration
                    * self.project.comps[ci].fps)
                    .round() as u32;
                let info = PrecompInfo {
                    name: self.precomp_config.name.clone(),
                    layer_ids,
                    duration_frames,
                };
                self.precomps.push(info);
            }
            Action::OpenPrecomp(idx) => {
                self.active_precomp = Some(idx);
            }
            Action::ClosePrecomp | Action::ReturnToMain => {
                self.active_precomp = None;
            }
            Action::RenamePrecomp { idx, name } => {
                if let Some(p) = self.precomps.get_mut(idx) {
                    p.name = name;
                }
            }
            Action::DeletePrecomp(idx) => {
                if idx < self.precomps.len() {
                    self.precomps.remove(idx);
                }
            }
            Action::CollapseTransformations { layer_id } => {
                if !self.collapse_transforms.remove(&layer_id) {
                    self.collapse_transforms.insert(layer_id);
                }
            }

            // --- Batch 6 depth: 3D Layer ---
            Action::Enable3DLayer { layer_id, enabled } => {
                self.layer_3d_configs.entry(layer_id).or_default().enabled = enabled;
            }
            Action::Set3DPosition { layer_id, pos } => {
                self.layer_3d_configs.entry(layer_id).or_default().position = pos;
            }
            Action::Set3DLayerRotation { layer_id, rot } => {
                self.layer_3d_configs.entry(layer_id).or_default().rotation = rot;
            }
            Action::Set3DOrientation { layer_id, orient } => {
                self.layer_3d_configs.entry(layer_id).or_default().orientation = orient;
            }
            Action::Set3DScale { layer_id, scale } => {
                self.layer_3d_configs.entry(layer_id).or_default().scale = scale;
            }
            Action::Set3DAnchor { layer_id, anchor } => {
                self.layer_3d_configs.entry(layer_id).or_default().anchor_point = anchor;
            }
            Action::Set3DShadows { layer_id, casts, accepts } => {
                let cfg = self.layer_3d_configs.entry(layer_id).or_default();
                cfg.casts_shadows = casts;
                cfg.accepts_shadows = accepts;
            }
            Action::Set3DMaterial { layer_id, shininess, metal } => {
                let cfg = self.layer_3d_configs.entry(layer_id).or_default();
                cfg.material_shininess = shininess.clamp(0.0, 100.0);
                cfg.material_metal = metal.clamp(0.0, 100.0);
            }
            Action::Reset3DLayer { layer_id } => {
                self.layer_3d_configs.remove(&layer_id);
            }

            _ => unreachable!("apply_precomp called with wrong action"),
        }
    }
}
