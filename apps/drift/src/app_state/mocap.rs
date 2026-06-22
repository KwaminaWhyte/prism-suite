use super::{App, Action};

/// Supported motion-capture file formats.
#[derive(Clone, Debug, PartialEq)]
pub enum MocapFormat {
    Bvh,
    Fbx,
    C3d,
    Json,
}

/// Mapping from a bone name in the mocap data to a bone id in the Drift rig.
#[derive(Clone, Debug)]
pub struct MocapBoneMapping {
    /// Name of the bone as it appears in the mocap file.
    pub mocap_bone: String,
    /// Corresponding bone id in the Drift rig.
    pub rig_bone_id: usize,
    /// Euler rotation offset applied after retargeting, in degrees (x, y, z).
    pub offset_rotation: (f32, f32, f32),
}

/// Imported mocap clip with its retargeting configuration.
#[derive(Clone, Debug)]
pub struct MocapImport {
    pub id: usize,
    pub name: String,
    pub path: String,
    pub format: MocapFormat,
    pub frame_rate: f32,
    pub frame_count: u32,
    pub bone_mapping: Vec<MocapBoneMapping>,
    /// Scale factor applied to position data during retargeting.
    pub retarget_scale: f32,
    /// Rig to apply this clip to, if set.
    pub apply_to_rig_id: Option<usize>,
    /// Set to true when BakeMocapToKeyframes has been requested.
    pub bake_requested: bool,
}

impl App {
    pub fn apply_mocap(&mut self, action: Action) {
        match action {
            Action::ImportMocap { name, path, format } => {
                let id = self.next_mocap_id;
                self.next_mocap_id += 1;
                self.mocap_imports.push(MocapImport {
                    id,
                    name,
                    path,
                    format,
                    frame_rate: 30.0,
                    frame_count: 0,
                    bone_mapping: Vec::new(),
                    retarget_scale: 1.0,
                    apply_to_rig_id: None,
                    bake_requested: false,
                });
            }
            Action::RemoveMocap { mocap_id } => {
                self.mocap_imports.retain(|m| m.id != mocap_id);
            }
            Action::SetMocapBoneMapping { mocap_id, mocap_bone, rig_bone_id } => {
                if let Some(m) = self.mocap_imports.iter_mut().find(|m| m.id == mocap_id) {
                    // Replace existing mapping for this mocap bone if present.
                    if let Some(existing) = m.bone_mapping.iter_mut().find(|b| b.mocap_bone == mocap_bone) {
                        existing.rig_bone_id = rig_bone_id;
                    } else {
                        m.bone_mapping.push(MocapBoneMapping {
                            mocap_bone,
                            rig_bone_id,
                            offset_rotation: (0.0, 0.0, 0.0),
                        });
                    }
                }
            }
            Action::SetMocapRetargetScale { mocap_id, scale } => {
                if let Some(m) = self.mocap_imports.iter_mut().find(|m| m.id == mocap_id) {
                    m.retarget_scale = scale.max(0.001);
                }
            }
            Action::SetMocapApplyRig { mocap_id, rig_id } => {
                if let Some(m) = self.mocap_imports.iter_mut().find(|m| m.id == mocap_id) {
                    m.apply_to_rig_id = rig_id;
                }
            }
            Action::BakeMocapToKeyframes { mocap_id } => {
                if let Some(m) = self.mocap_imports.iter_mut().find(|m| m.id == mocap_id) {
                    m.bake_requested = true;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::MocapFormat;

    fn app() -> App {
        App::new()
    }

    fn import_bvh(a: &mut App) -> usize {
        a.apply(Action::ImportMocap {
            name: "Walk.bvh".to_string(),
            path: "/mocap/walk.bvh".to_string(),
            format: MocapFormat::Bvh,
        });
        a.mocap_imports.last().unwrap().id
    }

    #[test]
    fn test_import_mocap_creates_entry() {
        let mut a = app();
        let id = import_bvh(&mut a);
        assert_eq!(a.mocap_imports.len(), 1);
        assert_eq!(a.mocap_imports[0].id, id);
        assert_eq!(a.mocap_imports[0].format, MocapFormat::Bvh);
    }

    #[test]
    fn test_import_mocap_ids_unique() {
        let mut a = app();
        let id1 = import_bvh(&mut a);
        let id2 = import_bvh(&mut a);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_remove_mocap() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::RemoveMocap { mocap_id: id });
        assert!(a.mocap_imports.is_empty());
    }

    #[test]
    fn test_set_mocap_bone_mapping_adds() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::SetMocapBoneMapping {
            mocap_id: id,
            mocap_bone: "Hips".to_string(),
            rig_bone_id: 0,
        });
        assert_eq!(a.mocap_imports[0].bone_mapping.len(), 1);
        assert_eq!(a.mocap_imports[0].bone_mapping[0].mocap_bone, "Hips");
        assert_eq!(a.mocap_imports[0].bone_mapping[0].rig_bone_id, 0);
    }

    #[test]
    fn test_set_mocap_bone_mapping_replaces() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::SetMocapBoneMapping { mocap_id: id, mocap_bone: "Hips".to_string(), rig_bone_id: 0 });
        a.apply(Action::SetMocapBoneMapping { mocap_id: id, mocap_bone: "Hips".to_string(), rig_bone_id: 5 });
        assert_eq!(a.mocap_imports[0].bone_mapping.len(), 1);
        assert_eq!(a.mocap_imports[0].bone_mapping[0].rig_bone_id, 5);
    }

    #[test]
    fn test_set_mocap_retarget_scale() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::SetMocapRetargetScale { mocap_id: id, scale: 0.5 });
        assert_eq!(a.mocap_imports[0].retarget_scale, 0.5);
    }

    #[test]
    fn test_set_mocap_retarget_scale_min_clamped() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::SetMocapRetargetScale { mocap_id: id, scale: 0.0 });
        assert!(a.mocap_imports[0].retarget_scale > 0.0);
    }

    #[test]
    fn test_set_mocap_apply_rig() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::SetMocapApplyRig { mocap_id: id, rig_id: Some(3) });
        assert_eq!(a.mocap_imports[0].apply_to_rig_id, Some(3));
    }

    #[test]
    fn test_set_mocap_apply_rig_clear() {
        let mut a = app();
        let id = import_bvh(&mut a);
        a.apply(Action::SetMocapApplyRig { mocap_id: id, rig_id: Some(3) });
        a.apply(Action::SetMocapApplyRig { mocap_id: id, rig_id: None });
        assert_eq!(a.mocap_imports[0].apply_to_rig_id, None);
    }

    #[test]
    fn test_bake_mocap_to_keyframes_stub() {
        let mut a = app();
        let id = import_bvh(&mut a);
        assert!(!a.mocap_imports[0].bake_requested);
        a.apply(Action::BakeMocapToKeyframes { mocap_id: id });
        assert!(a.mocap_imports[0].bake_requested);
    }

    #[test]
    fn test_import_fbx_format() {
        let mut a = app();
        a.apply(Action::ImportMocap {
            name: "Run.fbx".to_string(),
            path: "/mocap/run.fbx".to_string(),
            format: MocapFormat::Fbx,
        });
        assert_eq!(a.mocap_imports[0].format, MocapFormat::Fbx);
    }
}
