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

// ── IK Chain (FABRIK solver) ──────────────────────────────────────────────────

/// An IK chain that drives a sequence of bones toward a target position.
#[derive(Clone, Debug)]
pub struct IkChain {
    pub id: usize,
    pub layer_id: usize,
    /// Bone IDs ordered root→tip.
    pub bone_ids: Vec<usize>,
    pub target_x: f32,
    pub target_y: f32,
    /// Convergence tolerance (pixels).
    pub tolerance: f32,
    pub max_iterations: u32,
}

// ── Spring Dynamics ───────────────────────────────────────────────────────────

/// Spring-damper secondary motion attached to a single bone.
#[derive(Clone, Debug)]
pub struct BoneSpring {
    pub bone_id: usize,
    /// Spring stiffness 0.0–1.0.
    pub stiffness: f32,
    /// Damping 0.0–1.0.
    pub damping: f32,
    /// Mass (must be > 0).
    pub mass: f32,
    pub enabled: bool,
    // Runtime state — not serialised.
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub current_x: f32,
    pub current_y: f32,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

fn normalize(dx: f32, dy: f32) -> (f32, f32) {
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        (0.0, 0.0)
    } else {
        (dx / len, dy / len)
    }
}

// ── App impls ─────────────────────────────────────────────────────────────────

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
                self.ik_chains.retain(|c| !c.bone_ids.contains(&bone_id));
                self.bone_springs.retain(|s| s.bone_id != bone_id);
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
            // IK chain actions
            Action::AddIkChain { layer_id, bone_ids, target_x, target_y } => {
                let id = self.next_chain_id;
                self.next_chain_id += 1;
                self.ik_chains.push(IkChain {
                    id,
                    layer_id,
                    bone_ids,
                    target_x,
                    target_y,
                    tolerance: 0.1,
                    max_iterations: 20,
                });
            }
            Action::RemoveIkChain { chain_id } => {
                self.ik_chains.retain(|c| c.id != chain_id);
            }
            Action::SetIkTarget { chain_id, target_x, target_y } => {
                if let Some(c) = self.ik_chains.iter_mut().find(|c| c.id == chain_id) {
                    c.target_x = target_x;
                    c.target_y = target_y;
                }
            }
            Action::SolveIk { chain_id } => {
                self.solve_ik_chain(chain_id);
            }
            // Spring dynamics actions
            Action::AddBoneSpring { bone_id, stiffness, damping, mass } => {
                // Remove existing spring for this bone first.
                self.bone_springs.retain(|s| s.bone_id != bone_id);
                self.bone_springs.push(BoneSpring {
                    bone_id,
                    stiffness: stiffness.clamp(0.0, 1.0),
                    damping: damping.clamp(0.0, 1.0),
                    mass: mass.max(1e-6),
                    enabled: true,
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                    current_x: 0.0,
                    current_y: 0.0,
                });
            }
            Action::RemoveBoneSpring { bone_id } => {
                self.bone_springs.retain(|s| s.bone_id != bone_id);
            }
            Action::SetSpringParams { bone_id, stiffness, damping, mass } => {
                if let Some(s) = self.bone_springs.iter_mut().find(|s| s.bone_id == bone_id) {
                    s.stiffness = stiffness.clamp(0.0, 1.0);
                    s.damping = damping.clamp(0.0, 1.0);
                    s.mass = mass.max(1e-6);
                }
            }
            Action::TickSprings { delta_t } => {
                self.tick_springs(delta_t);
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

    // ── FABRIK IK Solver ─────────────────────────────────────────────────────

    /// Solve an IK chain using the FABRIK algorithm.
    pub fn solve_ik_chain(&mut self, chain_id: usize) {
        // Collect chain info without borrowing self.
        let chain = match self.ik_chains.iter().find(|c| c.id == chain_id) {
            Some(c) => c.clone(),
            None => return,
        };
        if chain.bone_ids.is_empty() {
            return;
        }

        // Find the rig for this layer.
        let rig_idx = match self.layer_rigs.iter().position(|r| r.layer_id == chain.layer_id) {
            Some(i) => i,
            None => return,
        };

        // Gather joint positions (n+1 joints for n bones) and bone lengths.
        let n = chain.bone_ids.len();
        let mut positions: Vec<(f32, f32)> = Vec::with_capacity(n + 1);
        let mut lengths: Vec<f32> = Vec::with_capacity(n);

        for &bid in &chain.bone_ids {
            match self.layer_rigs[rig_idx].bones.iter().find(|b| b.id == bid) {
                Some(b) => {
                    positions.push((b.x, b.y));
                    lengths.push(b.length);
                }
                None => return,
            }
        }
        // Tip joint = last bone's endpoint.
        {
            let last_bone = self.layer_rigs[rig_idx]
                .bones
                .iter()
                .find(|b| b.id == *chain.bone_ids.last().unwrap())
                .unwrap();
            // Approximate tip as bone position + length along current rotation.
            let angle = last_bone.rotation.to_radians();
            positions.push((
                last_bone.x + last_bone.length * angle.cos(),
                last_bone.y + last_bone.length * angle.sin(),
            ));
        }

        let root = positions[0];
        let target = (chain.target_x, chain.target_y);
        let total_length: f32 = lengths.iter().sum();

        // Check reachability.
        let dist_to_target = dist(root.0, root.1, target.0, target.1);
        if dist_to_target >= total_length {
            // Fully extend the chain toward the target.
            let (dx, dy) = normalize(target.0 - root.0, target.1 - root.1);
            let mut cum = 0.0;
            for i in 0..n {
                positions[i] = (root.0 + dx * cum, root.1 + dy * cum);
                cum += lengths[i];
            }
            positions[n] = (root.0 + dx * cum, root.1 + dy * cum);
        } else {
            // FABRIK iterations.
            for _ in 0..chain.max_iterations {
                // Forward pass: set tip to target, walk backward.
                positions[n] = target;
                for i in (0..n).rev() {
                    let (dx, dy) = normalize(
                        positions[i].0 - positions[i + 1].0,
                        positions[i].1 - positions[i + 1].1,
                    );
                    positions[i] = (
                        positions[i + 1].0 + dx * lengths[i],
                        positions[i + 1].1 + dy * lengths[i],
                    );
                }
                // Backward pass: fix root, walk forward.
                positions[0] = root;
                for i in 0..n {
                    let (dx, dy) = normalize(
                        positions[i + 1].0 - positions[i].0,
                        positions[i + 1].1 - positions[i].1,
                    );
                    positions[i + 1] = (
                        positions[i].0 + dx * lengths[i],
                        positions[i].1 + dy * lengths[i],
                    );
                }
                if dist(positions[n].0, positions[n].1, target.0, target.1) < chain.tolerance {
                    break;
                }
            }
        }

        // Write positions + rotations back to bones.
        for (i, &bid) in chain.bone_ids.iter().enumerate() {
            if let Some(b) = self.layer_rigs[rig_idx].bones.iter_mut().find(|b| b.id == bid) {
                b.x = positions[i].0;
                b.y = positions[i].1;
                // Rotation from this joint to next joint.
                let dx = positions[i + 1].0 - positions[i].0;
                let dy = positions[i + 1].1 - positions[i].1;
                b.rotation = dy.atan2(dx).to_degrees();
            }
        }
    }

    // ── Spring Dynamics ───────────────────────────────────────────────────────

    /// Advance spring simulation by `delta_t` seconds.
    pub fn tick_springs(&mut self, delta_t: f32) {
        if delta_t <= 0.0 {
            return;
        }
        // Collect bone positions first to avoid borrowing issues.
        let bone_positions: std::collections::HashMap<usize, (f32, f32)> = self
            .layer_rigs
            .iter()
            .flat_map(|r| r.bones.iter().map(|b| (b.id, (b.x, b.y))))
            .collect();

        for spring in &mut self.bone_springs {
            if !spring.enabled {
                continue;
            }
            let target = bone_positions.get(&spring.bone_id).copied().unwrap_or((0.0, 0.0));
            // Spring-damper: F = k*(target - current) - d*velocity
            let force_x = spring.stiffness * (target.0 - spring.current_x)
                - spring.damping * spring.velocity_x;
            let force_y = spring.stiffness * (target.1 - spring.current_y)
                - spring.damping * spring.velocity_y;
            let accel_x = force_x / spring.mass;
            let accel_y = force_y / spring.mass;
            spring.velocity_x += accel_x * delta_t;
            spring.velocity_y += accel_y * delta_t;
            spring.current_x += spring.velocity_x * delta_t;
            spring.current_y += spring.velocity_y * delta_t;
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

    // ── Original rig tests ────────────────────────────────────────────────────

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

    // ── IK Chain / FABRIK tests ───────────────────────────────────────────────

    fn setup_two_bone_rig(a: &mut App) -> (usize, usize, usize) {
        a.apply(Action::AddLayer { name: "Arm".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 50.0, length: 50.0 });
        let b0 = a.layer_rigs[0].bones[0].id;
        let b1 = a.layer_rigs[0].bones[1].id;
        (lid, b0, b1)
    }

    #[test]
    fn test_add_ik_chain() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 80.0, target_y: 0.0 });
        assert_eq!(a.ik_chains.len(), 1);
        assert_eq!(a.ik_chains[0].bone_ids.len(), 2);
    }

    #[test]
    fn test_remove_ik_chain() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 50.0, target_y: 0.0 });
        let cid = a.ik_chains[0].id;
        a.apply(Action::RemoveIkChain { chain_id: cid });
        assert!(a.ik_chains.is_empty());
    }

    #[test]
    fn test_set_ik_target_on_chain() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 0.0, target_y: 0.0 });
        let cid = a.ik_chains[0].id;
        a.apply(Action::SetIkTarget { chain_id: cid, target_x: 70.0, target_y: 30.0 });
        assert_eq!(a.ik_chains[0].target_x, 70.0);
        assert_eq!(a.ik_chains[0].target_y, 30.0);
    }

    #[test]
    fn test_solve_ik_reachable_target() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        // Target within reach (total length = 100)
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 70.0, target_y: 0.0 });
        let cid = a.ik_chains[0].id;
        a.apply(Action::SolveIk { chain_id: cid });
        // Root bone should still be at origin
        assert_eq!(a.layer_rigs[0].bones[0].x, 0.0);
        assert_eq!(a.layer_rigs[0].bones[0].y, 0.0);
    }

    #[test]
    fn test_solve_ik_unreachable_target_extends_chain() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        // Target far beyond reach (total length = 100, target = 500)
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 500.0, target_y: 0.0 });
        let cid = a.ik_chains[0].id;
        a.apply(Action::SolveIk { chain_id: cid });
        // Chain should be fully extended toward target (all along x axis)
        let b0_after = &a.layer_rigs[0].bones[0];
        let b1_after = &a.layer_rigs[0].bones[1];
        assert!((b0_after.x - 0.0).abs() < 0.1, "root stays at 0");
        assert!(b1_after.x > 40.0, "second bone moves in target direction");
    }

    #[test]
    fn test_solve_ik_noop_for_unknown_chain() {
        let mut a = app();
        // Should not panic
        a.solve_ik_chain(9999);
    }

    #[test]
    fn test_ik_chain_default_tolerance() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 0.0, target_y: 0.0 });
        assert_eq!(a.ik_chains[0].tolerance, 0.1);
        assert_eq!(a.ik_chains[0].max_iterations, 20);
    }

    #[test]
    fn test_delete_bone_removes_ik_chains() {
        let mut a = app();
        let (lid, b0, b1) = setup_two_bone_rig(&mut a);
        a.apply(Action::AddIkChain { layer_id: lid, bone_ids: vec![b0, b1], target_x: 0.0, target_y: 0.0 });
        a.apply(Action::DeleteBone { layer_id: lid, bone_id: b0 });
        assert!(a.ik_chains.is_empty(), "chain referencing deleted bone should be removed");
    }

    // ── Spring dynamics tests ─────────────────────────────────────────────────

    #[test]
    fn test_add_bone_spring() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::AddBoneSpring { bone_id: bid, stiffness: 0.5, damping: 0.3, mass: 1.0 });
        assert_eq!(a.bone_springs.len(), 1);
        assert_eq!(a.bone_springs[0].bone_id, bid);
        assert_eq!(a.bone_springs[0].stiffness, 0.5);
    }

    #[test]
    fn test_remove_bone_spring() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::AddBoneSpring { bone_id: bid, stiffness: 0.5, damping: 0.3, mass: 1.0 });
        a.apply(Action::RemoveBoneSpring { bone_id: bid });
        assert!(a.bone_springs.is_empty());
    }

    #[test]
    fn test_set_spring_params() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::AddBoneSpring { bone_id: bid, stiffness: 0.5, damping: 0.3, mass: 1.0 });
        a.apply(Action::SetSpringParams { bone_id: bid, stiffness: 0.8, damping: 0.1, mass: 2.0 });
        assert_eq!(a.bone_springs[0].stiffness, 0.8);
        assert_eq!(a.bone_springs[0].damping, 0.1);
        assert_eq!(a.bone_springs[0].mass, 2.0);
    }

    #[test]
    fn test_tick_springs_moves_toward_target() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 100.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        // Spring starts at (0,0) but bone is at (100,0).
        a.apply(Action::AddBoneSpring { bone_id: bid, stiffness: 0.8, damping: 0.3, mass: 1.0 });
        a.apply(Action::TickSprings { delta_t: 1.0 / 60.0 });
        // current_x should have moved toward 100.0
        assert!(a.bone_springs[0].current_x > 0.0);
    }

    #[test]
    fn test_tick_springs_zero_delta_noop() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 100.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::AddBoneSpring { bone_id: bid, stiffness: 0.5, damping: 0.3, mass: 1.0 });
        a.apply(Action::TickSprings { delta_t: 0.0 });
        assert_eq!(a.bone_springs[0].current_x, 0.0);
        assert_eq!(a.bone_springs[0].velocity_x, 0.0);
    }

    #[test]
    fn test_spring_stiffness_clamp() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::AddBone { layer_id: lid, parent_id: None, x: 0.0, y: 0.0, length: 50.0 });
        let bid = a.layer_rigs[0].bones[0].id;
        a.apply(Action::AddBoneSpring { bone_id: bid, stiffness: 5.0, damping: -1.0, mass: 0.0 });
        assert_eq!(a.bone_springs[0].stiffness, 1.0, "stiffness clamped to 1.0");
        assert_eq!(a.bone_springs[0].damping, 0.0, "damping clamped to 0.0");
        assert!(a.bone_springs[0].mass > 0.0, "mass floored above 0");
    }
}
