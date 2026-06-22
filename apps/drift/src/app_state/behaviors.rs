use super::{App, Action};

/// The specific parameterization of a Character Animator behavior.
#[derive(Clone, Debug, PartialEq)]
pub enum BehaviorKind {
    /// Drive mouth shapes from an audio track.
    LipSync {
        audio_track_id: usize,
        /// Name of the swap set / group that contains mouth shapes.
        mouth_group: String,
    },
    /// Automatic eye-blink animation.
    Blink {
        interval_frames: u32,
        duration_frames: u32,
        /// ±N frames random jitter applied to each blink interval.
        random_offset: f32,
    },
    /// Head turns left/right based on mouse/webcam input.
    HeadTurn {
        sensitivity: f32,
        invert: bool,
        max_angle: f32,
    },
    /// Secondary "dangle" motion on a bone (gravity-like spring).
    DangleBone {
        bone_id: usize,
        gravity: f32,
        stiffness: f32,
    },
    /// Continuous breathing cycle on a bone.
    Breathe {
        cycle_frames: u32,
        amplitude: f32,
        bone_id: Option<usize>,
    },
    /// Walk cycle — alternates left/right leg bones.
    WalkCycle {
        step_frames: u32,
        stride: f32,
        left_bone: Option<usize>,
        right_bone: Option<usize>,
    },
    /// Loop a frame range, optionally ping-ponging.
    Cycle {
        start_frame: u32,
        end_frame: u32,
        ping_pong: bool,
    },
}

/// A behavior that drives the character rig automatically.
#[derive(Clone, Debug)]
pub struct Behavior {
    pub id: usize,
    pub name: String,
    pub kind: BehaviorKind,
    pub enabled: bool,
    /// Higher priority behaviors are evaluated first.
    pub priority: i32,
}

impl App {
    pub fn apply_behaviors(&mut self, action: Action) {
        match action {
            Action::AddBehavior { name, kind } => {
                let id = self.next_behavior_id;
                self.next_behavior_id += 1;
                self.behaviors.push(Behavior {
                    id,
                    name,
                    kind,
                    enabled: true,
                    priority: 0,
                });
            }
            Action::RemoveBehavior { id } => {
                self.behaviors.retain(|b| b.id != id);
            }
            Action::ToggleBehavior { id } => {
                if let Some(b) = self.behaviors.iter_mut().find(|b| b.id == id) {
                    b.enabled = !b.enabled;
                }
            }
            Action::SetBehaviorPriority { id, priority } => {
                if let Some(b) = self.behaviors.iter_mut().find(|b| b.id == id) {
                    b.priority = priority;
                }
            }
            Action::RenameBehavior { id, name } => {
                if let Some(b) = self.behaviors.iter_mut().find(|b| b.id == id) {
                    b.name = name;
                }
            }
            Action::UpdateBehaviorKind { id, kind } => {
                if let Some(b) = self.behaviors.iter_mut().find(|b| b.id == id) {
                    b.kind = kind;
                }
            }
            _ => {}
        }
    }

    /// Returns references to all enabled behaviors sorted by descending priority.
    pub fn active_behaviors(&self) -> Vec<&Behavior> {
        let mut v: Vec<&Behavior> = self.behaviors.iter().filter(|b| b.enabled).collect();
        v.sort_by(|a, b| b.priority.cmp(&a.priority));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::BehaviorKind;

    fn app() -> App {
        App::new()
    }

    fn blink_kind() -> BehaviorKind {
        BehaviorKind::Blink {
            interval_frames: 90,
            duration_frames: 4,
            random_offset: 10.0,
        }
    }

    #[test]
    fn test_add_behavior() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "Blink".to_string(), kind: blink_kind() });
        assert_eq!(a.behaviors.len(), 1);
        assert_eq!(a.behaviors[0].name, "Blink");
        assert!(a.behaviors[0].enabled);
    }

    #[test]
    fn test_add_multiple_behaviors_unique_ids() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "A".to_string(), kind: blink_kind() });
        a.apply(Action::AddBehavior { name: "B".to_string(), kind: blink_kind() });
        assert_ne!(a.behaviors[0].id, a.behaviors[1].id);
    }

    #[test]
    fn test_remove_behavior() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "Blink".to_string(), kind: blink_kind() });
        let id = a.behaviors[0].id;
        a.apply(Action::RemoveBehavior { id });
        assert!(a.behaviors.is_empty());
    }

    #[test]
    fn test_toggle_behavior() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "Blink".to_string(), kind: blink_kind() });
        let id = a.behaviors[0].id;
        a.apply(Action::ToggleBehavior { id });
        assert!(!a.behaviors[0].enabled);
        a.apply(Action::ToggleBehavior { id });
        assert!(a.behaviors[0].enabled);
    }

    #[test]
    fn test_set_behavior_priority() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "B".to_string(), kind: blink_kind() });
        let id = a.behaviors[0].id;
        a.apply(Action::SetBehaviorPriority { id, priority: 10 });
        assert_eq!(a.behaviors[0].priority, 10);
    }

    #[test]
    fn test_rename_behavior() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "Old".to_string(), kind: blink_kind() });
        let id = a.behaviors[0].id;
        a.apply(Action::RenameBehavior { id, name: "New".to_string() });
        assert_eq!(a.behaviors[0].name, "New");
    }

    #[test]
    fn test_update_behavior_kind() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "B".to_string(), kind: blink_kind() });
        let id = a.behaviors[0].id;
        let new_kind = BehaviorKind::Breathe {
            cycle_frames: 60,
            amplitude: 0.05,
            bone_id: Some(2),
        };
        a.apply(Action::UpdateBehaviorKind { id, kind: new_kind.clone() });
        assert_eq!(a.behaviors[0].kind, new_kind);
    }

    #[test]
    fn test_active_behaviors_excludes_disabled() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "Blink".to_string(), kind: blink_kind() });
        a.apply(Action::AddBehavior { name: "Breathe".to_string(), kind: BehaviorKind::Breathe { cycle_frames: 60, amplitude: 0.03, bone_id: None } });
        let b0_id = a.behaviors[0].id;
        a.apply(Action::ToggleBehavior { id: b0_id });
        let active = a.active_behaviors();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "Breathe");
    }

    #[test]
    fn test_active_behaviors_sorted_by_priority() {
        let mut a = app();
        a.apply(Action::AddBehavior { name: "Low".to_string(), kind: blink_kind() });
        a.apply(Action::AddBehavior { name: "High".to_string(), kind: blink_kind() });
        let lo_id = a.behaviors[0].id;
        let hi_id = a.behaviors[1].id;
        a.apply(Action::SetBehaviorPriority { id: lo_id, priority: 1 });
        a.apply(Action::SetBehaviorPriority { id: hi_id, priority: 5 });
        let active = a.active_behaviors();
        assert_eq!(active[0].name, "High");
        assert_eq!(active[1].name, "Low");
    }

    #[test]
    fn test_lip_sync_behavior() {
        let mut a = app();
        let kind = BehaviorKind::LipSync {
            audio_track_id: 1,
            mouth_group: "Mouth".to_string(),
        };
        a.apply(Action::AddBehavior { name: "LipSync".to_string(), kind: kind.clone() });
        assert_eq!(a.behaviors[0].kind, kind);
    }

    #[test]
    fn test_walk_cycle_behavior() {
        let mut a = app();
        let kind = BehaviorKind::WalkCycle {
            step_frames: 12,
            stride: 30.0,
            left_bone: Some(6),
            right_bone: Some(7),
        };
        a.apply(Action::AddBehavior { name: "Walk".to_string(), kind: kind.clone() });
        assert_eq!(a.behaviors[0].kind, kind);
    }
}
