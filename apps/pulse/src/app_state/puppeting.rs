use super::*;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum MorphMode {
    #[default]
    Linear,
    Smooth,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum CorrespondenceMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShapeMorphKeyframe {
    pub time: f32,
    pub layer_id: usize,
    pub path_idx: usize,
    pub mode: MorphMode,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShapeMorphConfig {
    pub enabled: bool,
    pub keyframes: Vec<ShapeMorphKeyframe>,
    pub correspondence_mode: CorrespondenceMode,
}

impl Default for ShapeMorphConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            keyframes: vec![],
            correspondence_mode: CorrespondenceMode::Auto,
        }
    }
}

/// Puppet pin operating mode.
#[derive(Clone, Debug, PartialEq)]
pub enum PuppetPinMode {
    Deform,
    Starch,
    Overlap,
}

/// A single puppet deformation pin (app-level state).
#[derive(Clone, Debug)]
pub struct PuppetPin {
    pub id: usize,
    pub layer_id: usize,
    pub name: String,
    pub mode: PuppetPinMode,
    pub x: f32,
    pub y: f32,
    pub stiffness: f32,
    pub extent: f32,
}

/// Puppet mesh parameters for a layer.
#[derive(Clone, Debug)]
pub struct PuppetMesh {
    pub layer_id: usize,
    pub triangle_count: usize,
    pub expansion: f32,
    pub density: u8,
}


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

            // --- Batch 7: PuppetPin (app-level) ---
            Action::ActivatePuppetTool(b) => {
                self.puppet_tool_active = b;
            }
            Action::AddPuppetPinExt { layer_id, x, y, mode } => {
                self.puppet_pin_counter += 1;
                let id = self.puppet_pin_counter;
                self.puppet_pins.push(PuppetPin {
                    id,
                    layer_id,
                    name: format!("Pin {}", id),
                    mode,
                    x,
                    y,
                    stiffness: 0.0,
                    extent: 0.0,
                });
            }
            Action::MovePuppetPinExt { pin_id, x, y } => {
                if let Some(pin) = self.puppet_pins.iter_mut().find(|p| p.id == pin_id) {
                    pin.x = x;
                    pin.y = y;
                }
            }
            Action::SetPuppetPinStiffnessExt { pin_id, stiffness } => {
                if let Some(pin) = self.puppet_pins.iter_mut().find(|p| p.id == pin_id) {
                    pin.stiffness = stiffness.clamp(0.0, 100.0);
                }
            }
            Action::DeletePuppetPin(pin_id) => {
                self.puppet_pins.retain(|p| p.id != pin_id);
            }
            Action::SetPuppetMeshDensityExt { layer_id, density } => {
                let density = density.clamp(1, 30);
                if let Some(mesh) = self.puppet_meshes.iter_mut().find(|m| m.layer_id == layer_id) {
                    mesh.density = density;
                } else {
                    self.puppet_meshes.push(PuppetMesh {
                        layer_id,
                        triangle_count: 0,
                        expansion: 10.0,
                        density,
                    });
                }
            }
            Action::SetPuppetMeshExpansion { layer_id, expansion } => {
                let expansion = expansion.clamp(3.0, 100.0);
                if let Some(mesh) = self.puppet_meshes.iter_mut().find(|m| m.layer_id == layer_id) {
                    mesh.expansion = expansion;
                } else {
                    self.puppet_meshes.push(PuppetMesh {
                        layer_id,
                        triangle_count: 0,
                        expansion,
                        density: 5,
                    });
                }
            }

            _ => unreachable!("apply_puppeting called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morph_add_keyframe() {
        let mut app = App::new();
        assert!(app.shape_morph_config.keyframes.is_empty());
        let kf = ShapeMorphKeyframe { time: 1.0, layer_id: 0, path_idx: 0, mode: MorphMode::Linear };
        app.apply(Action::AddMorphKeyframe(kf));
        assert_eq!(app.shape_morph_config.keyframes.len(), 1);
    }

    #[test]
    fn test_morph_remove_keyframe_oob() {
        let mut app = App::new();
        app.apply(Action::RemoveMorphKeyframe(99));
        assert!(app.shape_morph_config.keyframes.is_empty());
    }

    #[test]
    fn test_morph_clear() {
        let mut app = App::new();
        let kf = ShapeMorphKeyframe { time: 0.5, layer_id: 0, path_idx: 0, mode: MorphMode::Smooth };
        app.apply(Action::AddMorphKeyframe(kf));
        assert!(!app.shape_morph_config.keyframes.is_empty());
        app.apply(Action::ClearMorphKeyframes);
        assert!(app.shape_morph_config.keyframes.is_empty());
    }

    #[test]
    fn test_morph_preview_time_non_neg() {
        let mut app = App::new();
        app.apply(Action::SetMorphPreviewTime(-1.0));
        assert!((app.morph_preview_time - 0.0).abs() < 1e-3);
        app.apply(Action::SetMorphPreviewTime(2.5));
        assert!((app.morph_preview_time - 2.5).abs() < 1e-3);
    }

    #[test]
    fn test_add_puppet_pin_ext() {
        let mut app = App::new();
        assert!(app.puppet_pins.is_empty());
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 100.0, y: 200.0, mode: PuppetPinMode::Deform });
        assert_eq!(app.puppet_pins.len(), 1);
        assert_eq!(app.puppet_pins[0].layer_id, 0);
        assert_eq!(app.puppet_pins[0].name, "Pin 1");
        assert_eq!(app.puppet_pins[0].mode, PuppetPinMode::Deform);
        assert!((app.puppet_pins[0].x - 100.0).abs() < 1e-5);
        assert!((app.puppet_pins[0].y - 200.0).abs() < 1e-5);
    }

    #[test]
    fn test_add_puppet_pin_auto_name() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 0.0, y: 0.0, mode: PuppetPinMode::Deform });
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 10.0, y: 10.0, mode: PuppetPinMode::Starch });
        assert_eq!(app.puppet_pins[0].name, "Pin 1");
        assert_eq!(app.puppet_pins[1].name, "Pin 2");
    }

    #[test]
    fn test_move_puppet_pin_ext() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 0.0, y: 0.0, mode: PuppetPinMode::Deform });
        let pin_id = app.puppet_pins[0].id;
        app.apply(Action::MovePuppetPinExt { pin_id, x: 50.0, y: 75.0 });
        assert!((app.puppet_pins[0].x - 50.0).abs() < 1e-5);
        assert!((app.puppet_pins[0].y - 75.0).abs() < 1e-5);
    }

    #[test]
    fn test_puppet_pin_stiffness_clamp() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 0.0, y: 0.0, mode: PuppetPinMode::Starch });
        let pin_id = app.puppet_pins[0].id;
        app.apply(Action::SetPuppetPinStiffnessExt { pin_id, stiffness: 75.0 });
        assert!((app.puppet_pins[0].stiffness - 75.0).abs() < 1e-5);
        app.apply(Action::SetPuppetPinStiffnessExt { pin_id, stiffness: 200.0 });
        assert!((app.puppet_pins[0].stiffness - 100.0).abs() < 1e-5);
        app.apply(Action::SetPuppetPinStiffnessExt { pin_id, stiffness: -5.0 });
        assert!((app.puppet_pins[0].stiffness - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_delete_puppet_pin() {
        let mut app = App::new();
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 0.0, y: 0.0, mode: PuppetPinMode::Deform });
        app.apply(Action::AddPuppetPinExt { layer_id: 0, x: 10.0, y: 10.0, mode: PuppetPinMode::Starch });
        let pin_id = app.puppet_pins[0].id;
        app.apply(Action::DeletePuppetPin(pin_id));
        assert_eq!(app.puppet_pins.len(), 1);
    }

    #[test]
    fn test_puppet_mesh_density_upsert() {
        let mut app = App::new();
        app.apply(Action::SetPuppetMeshDensityExt { layer_id: 0, density: 10 });
        assert_eq!(app.puppet_meshes.len(), 1);
        assert_eq!(app.puppet_meshes[0].density, 10);
        app.apply(Action::SetPuppetMeshDensityExt { layer_id: 0, density: 20 });
        assert_eq!(app.puppet_meshes.len(), 1);
        assert_eq!(app.puppet_meshes[0].density, 20);
        app.apply(Action::SetPuppetMeshDensityExt { layer_id: 0, density: 0 });
        assert_eq!(app.puppet_meshes[0].density, 1);
        app.apply(Action::SetPuppetMeshDensityExt { layer_id: 0, density: 99 });
        assert_eq!(app.puppet_meshes[0].density, 30);
    }

    #[test]
    fn test_puppet_mesh_expansion_upsert() {
        let mut app = App::new();
        app.apply(Action::SetPuppetMeshExpansion { layer_id: 1, expansion: 50.0 });
        assert_eq!(app.puppet_meshes.len(), 1);
        assert!((app.puppet_meshes[0].expansion - 50.0).abs() < 1e-5);
        app.apply(Action::SetPuppetMeshExpansion { layer_id: 1, expansion: 1.0 });
        assert!((app.puppet_meshes[0].expansion - 3.0).abs() < 1e-5);
        app.apply(Action::SetPuppetMeshExpansion { layer_id: 1, expansion: 200.0 });
        assert!((app.puppet_meshes[0].expansion - 100.0).abs() < 1e-5);
    }
}
