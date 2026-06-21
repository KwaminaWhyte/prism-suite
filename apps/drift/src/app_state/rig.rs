use super::{App, Action};

/// One bone in a puppet rig hierarchy.
#[derive(Clone, Debug)]
pub struct RigBone {
    pub id: usize,
    pub name: String,
    pub parent_id: Option<usize>,
    pub x: f32,
    pub y: f32,
    pub length: f32,
    pub rotation: f32,
    pub locked: bool,
}

/// The full puppet rig for a layer.
#[derive(Clone, Debug)]
pub struct LayerRig {
    pub layer_id: usize,
    pub bones: Vec<RigBone>,
    /// (bone_id, target_x, target_y)
    pub ik_targets: Vec<(usize, f32, f32)>,
}

impl App {
    pub fn apply_rig(&mut self, action: Action) {
        match action {
            Action::AddBone { layer_id, parent_id, x, y, length } => {
                let bone_id = self.rig_bone_counter;
                self.rig_bone_counter += 1;
                let bone = RigBone {
                    id: bone_id,
                    name: format!("bone_{bone_id}"),
                    parent_id,
                    x,
                    y,
                    length,
                    rotation: 0.0,
                    locked: false,
                };
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    rig.bones.push(bone);
                } else {
                    self.layer_rigs.push(LayerRig {
                        layer_id,
                        bones: vec![bone],
                        ik_targets: Vec::new(),
                    });
                }
            }
            Action::DeleteBone { layer_id, bone_id } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    rig.bones.retain(|b| b.id != bone_id);
                    rig.ik_targets.retain(|(bid, _, _)| *bid != bone_id);
                }
            }
            Action::MoveBone { layer_id, bone_id, x, y } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    if let Some(b) = rig.bones.iter_mut().find(|b| b.id == bone_id) {
                        b.x = x;
                        b.y = y;
                    }
                }
            }
            Action::RotateBone { layer_id, bone_id, rotation } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    if let Some(b) = rig.bones.iter_mut().find(|b| b.id == bone_id) {
                        b.rotation = rotation;
                    }
                }
            }
            Action::SetIKTarget { layer_id, bone_id, x, y } => {
                if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
                    if let Some(t) = rig.ik_targets.iter_mut().find(|(bid, _, _)| *bid == bone_id) {
                        *t = (bone_id, x, y);
                    } else {
                        rig.ik_targets.push((bone_id, x, y));
                    }
                }
            }
            Action::AutoRigLayer(layer_id) => {
                self.auto_rig_layer(layer_id);
            }
            _ => {}
        }
    }

    /// Shared helper: push 8 default bones for a humanoid character puppet rig.
    /// Called by both `AutoRigLayer` and `AutoRigWithAi`.
    pub fn auto_rig_layer(&mut self, layer_id: usize) {
        let defaults: &[(&str, Option<usize>, f32, f32, f32)] = &[
            ("hip",    None,    0.0,   0.0,  40.0),
            ("torso",  Some(0), 0.0,  -40.0, 50.0),
            ("neck",   Some(1), 0.0,  -90.0, 20.0),
            ("head",   Some(2), 0.0, -110.0, 30.0),
            ("l_arm",  Some(1), -30.0, -60.0, 45.0),
            ("r_arm",  Some(1),  30.0, -60.0, 45.0),
            ("l_leg",  Some(0), -20.0,  40.0, 50.0),
            ("r_leg",  Some(0),  20.0,  40.0, 50.0),
        ];

        let base_bone_id = self.rig_bone_counter;

        let bones: Vec<RigBone> = defaults
            .iter()
            .enumerate()
            .map(|(i, &(name, parent_local, x, y, length))| {
                let id = base_bone_id + i;
                RigBone {
                    id,
                    name: name.to_string(),
                    parent_id: parent_local.map(|p| base_bone_id + p),
                    x,
                    y,
                    length,
                    rotation: 0.0,
                    locked: false,
                }
            })
            .collect();

        self.rig_bone_counter += bones.len();

        if let Some(rig) = self.layer_rigs.iter_mut().find(|r| r.layer_id == layer_id) {
            rig.bones.extend(bones);
        } else {
            self.layer_rigs.push(LayerRig {
                layer_id,
                bones,
                ik_targets: Vec::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::LayerKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_bone_creates_rig() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone {
            layer_id: lid,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            length: 50.0,
        });
        assert_eq!(a.layer_rigs.len(), 1);
        assert_eq!(a.layer_rigs[0].bones.len(), 1);
    }

    #[test]
    fn test_add_bone_to_existing_rig() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        a.apply(Action::AddBone { layer_id: lid, parent_id: Some(0), x: 0.0, y: -50.0, length: 30.0 });
        assert_eq!(a.layer_rigs[0].bones.len(), 2);
    }

    #[test]
    fn test_delete_bone() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::DeleteBone { layer_id: lid, bone_id });
        assert!(a.layer_rigs[0].bones.is_empty());
    }

    #[test]
    fn test_move_bone() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::MoveBone { layer_id: lid, bone_id, x: 10.0, y: 20.0 });
        let b = &a.layer_rigs[0].bones[0];
        assert_eq!(b.x, 10.0);
        assert_eq!(b.y, 20.0);
    }

    #[test]
    fn test_rotate_bone() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::RotateBone { layer_id: lid, bone_id, rotation: 45.0 });
        assert_eq!(a.layer_rigs[0].bones[0].rotation, 45.0);
    }

    #[test]
    fn test_set_ik_target() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bone_id = a.layer_rigs[0].bones[0].id;
        a.apply(Action::SetIKTarget { layer_id: lid, bone_id, x: 100.0, y: 200.0 });
        assert_eq!(a.layer_rigs[0].ik_targets.len(), 1);
        assert_eq!(a.layer_rigs[0].ik_targets[0], (bone_id, 100.0, 200.0));
    }

    #[test]
    fn test_auto_rig_layer_creates_8_bones() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AutoRigLayer(lid));
        assert_eq!(a.layer_rigs.len(), 1);
        assert_eq!(a.layer_rigs[0].bones.len(), 8);
    }

    #[test]
    fn test_auto_rig_with_ai_creates_8_bones() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AutoRigWithAi(lid));
        assert_eq!(a.layer_rigs[0].bones.len(), 8);
    }

    #[test]
    fn test_auto_rig_bone_names() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AutoRigLayer(lid));
        let names: Vec<&str> =
            a.layer_rigs[0].bones.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"hip"));
        assert!(names.contains(&"head"));
        assert!(names.contains(&"l_arm"));
        assert!(names.contains(&"r_arm"));
    }

    #[test]
    fn test_multiple_rigs_different_layers() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Char1".to_string(), kind: LayerKind::Bitmap });
        a.apply(Action::AddLayer { name: "Char2".to_string(), kind: LayerKind::Bitmap });
        let l0 = a.layers[0].id;
        let l1 = a.layers[1].id;
        a.apply(Action::AutoRigLayer(l0));
        a.apply(Action::AutoRigLayer(l1));
        assert_eq!(a.layer_rigs.len(), 2);
        assert_eq!(a.layer_rigs[0].layer_id, l0);
        assert_eq!(a.layer_rigs[1].layer_id, l1);
    }

    #[test]
    fn test_ik_target_update_in_place() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "C".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::SetIKTarget { layer_id: lid, bone_id: bid, x: 10.0, y: 20.0 });
        a.apply(Action::SetIKTarget { layer_id: lid, bone_id: bid, x: 30.0, y: 40.0 });
        assert_eq!(a.layer_rigs[0].ik_targets.len(), 1);
        assert_eq!(a.layer_rigs[0].ik_targets[0], (bid, 30.0, 40.0));
    }
}
