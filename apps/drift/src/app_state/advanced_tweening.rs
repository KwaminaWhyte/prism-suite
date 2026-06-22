use super::{App, Action};

/// Richer tween kinds beyond the basic classic/shape tweens.
#[derive(Clone, Debug, PartialEq)]
pub enum AdvancedTweenKind {
    /// Move along a VectorPath.
    PathMotion { path_id: usize, orient_to_path: bool, ease_along: bool },
    /// Elastic spring-like overshoot.
    Elastic { amplitude: f32, period: f32 },
    /// Ball-bounce effect with decaying bounces.
    Bounce { num_bounces: u32, decay: f32 },
    /// Physical spring simulation.
    Spring { stiffness: f32, damping: f32, mass: f32 },
    /// CSS-style cubic bezier easing.
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
    /// Reference to a user-defined easing curve.
    Custom { easing_id: usize },
}

/// Binds a layer to follow a VectorPath over a frame range.
#[derive(Clone, Debug)]
pub struct MotionGuide {
    pub id: usize,
    pub layer_id: usize,
    /// ID of the VectorPath to follow.
    pub path_id: usize,
    pub start_frame: u32,
    pub end_frame: u32,
    pub orient_to_path: bool,
    pub snap_to_path: bool,
}

/// Animates a single layer property between two values with an advanced easing.
#[derive(Clone, Debug)]
pub struct PropertyTween {
    pub id: usize,
    pub layer_id: usize,
    /// Property name, e.g. "rotation" or "opacity".
    pub property: String,
    pub from_frame: u32,
    pub to_frame: u32,
    pub from_value: f32,
    pub to_value: f32,
    pub kind: AdvancedTweenKind,
}

impl App {
    pub fn apply_advanced_tweening(&mut self, action: Action) {
        match action {
            Action::AddMotionGuide { layer_id, path_id, start_frame, end_frame } => {
                let id = self.next_guide_id;
                self.next_guide_id += 1;
                self.motion_guides.push(MotionGuide {
                    id,
                    layer_id,
                    path_id,
                    start_frame,
                    end_frame,
                    orient_to_path: false,
                    snap_to_path: true,
                });
            }
            Action::RemoveMotionGuide { guide_id } => {
                self.motion_guides.retain(|g| g.id != guide_id);
            }
            Action::SetMotionGuideOrient { guide_id, orient } => {
                if let Some(g) = self.motion_guides.iter_mut().find(|g| g.id == guide_id) {
                    g.orient_to_path = orient;
                }
            }
            Action::SetMotionGuideSnap { guide_id, snap } => {
                if let Some(g) = self.motion_guides.iter_mut().find(|g| g.id == guide_id) {
                    g.snap_to_path = snap;
                }
            }
            Action::AddPropertyTween {
                layer_id, property, from_frame, to_frame, from_val, to_val, kind,
            } => {
                let id = self.next_prop_tween_id;
                self.next_prop_tween_id += 1;
                self.property_tweens.push(PropertyTween {
                    id,
                    layer_id,
                    property,
                    from_frame,
                    to_frame,
                    from_value: from_val,
                    to_value: to_val,
                    kind,
                });
            }
            Action::RemovePropertyTween { tween_id } => {
                self.property_tweens.retain(|t| t.id != tween_id);
            }
            Action::UpdatePropertyTweenKind { tween_id, kind } => {
                if let Some(t) = self.property_tweens.iter_mut().find(|t| t.id == tween_id) {
                    t.kind = kind;
                }
            }
            Action::SetPropertyTweenRange { tween_id, from_frame, to_frame } => {
                if let Some(t) = self.property_tweens.iter_mut().find(|t| t.id == tween_id) {
                    t.from_frame = from_frame;
                    t.to_frame = to_frame;
                }
            }
            _ => {}
        }
    }

    /// Returns all property tweens for a given layer.
    pub fn tweens_for_layer(&self, layer_id: usize) -> Vec<&PropertyTween> {
        self.property_tweens.iter().filter(|t| t.layer_id == layer_id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::AdvancedTweenKind;

    fn app() -> App {
        App::new()
    }

    fn elastic() -> AdvancedTweenKind {
        AdvancedTweenKind::Elastic { amplitude: 1.0, period: 0.3 }
    }

    #[test]
    fn test_add_motion_guide() {
        let mut a = app();
        a.apply(Action::AddMotionGuide { layer_id: 0, path_id: 1, start_frame: 0, end_frame: 30 });
        assert_eq!(a.motion_guides.len(), 1);
        assert_eq!(a.motion_guides[0].path_id, 1);
        assert!(a.motion_guides[0].snap_to_path);
    }

    #[test]
    fn test_add_multiple_guides_unique_ids() {
        let mut a = app();
        a.apply(Action::AddMotionGuide { layer_id: 0, path_id: 1, start_frame: 0, end_frame: 30 });
        a.apply(Action::AddMotionGuide { layer_id: 1, path_id: 2, start_frame: 0, end_frame: 60 });
        assert_ne!(a.motion_guides[0].id, a.motion_guides[1].id);
    }

    #[test]
    fn test_remove_motion_guide() {
        let mut a = app();
        a.apply(Action::AddMotionGuide { layer_id: 0, path_id: 1, start_frame: 0, end_frame: 30 });
        let id = a.motion_guides[0].id;
        a.apply(Action::RemoveMotionGuide { guide_id: id });
        assert!(a.motion_guides.is_empty());
    }

    #[test]
    fn test_set_motion_guide_orient() {
        let mut a = app();
        a.apply(Action::AddMotionGuide { layer_id: 0, path_id: 1, start_frame: 0, end_frame: 30 });
        let id = a.motion_guides[0].id;
        a.apply(Action::SetMotionGuideOrient { guide_id: id, orient: true });
        assert!(a.motion_guides[0].orient_to_path);
    }

    #[test]
    fn test_set_motion_guide_snap() {
        let mut a = app();
        a.apply(Action::AddMotionGuide { layer_id: 0, path_id: 1, start_frame: 0, end_frame: 30 });
        let id = a.motion_guides[0].id;
        a.apply(Action::SetMotionGuideSnap { guide_id: id, snap: false });
        assert!(!a.motion_guides[0].snap_to_path);
    }

    #[test]
    fn test_add_property_tween_elastic() {
        let mut a = app();
        a.apply(Action::AddPropertyTween {
            layer_id: 0,
            property: "rotation".to_string(),
            from_frame: 0,
            to_frame: 24,
            from_val: 0.0,
            to_val: 360.0,
            kind: elastic(),
        });
        assert_eq!(a.property_tweens.len(), 1);
        assert_eq!(a.property_tweens[0].property, "rotation");
        assert_eq!(a.property_tweens[0].kind, elastic());
    }

    #[test]
    fn test_add_multiple_property_tweens_unique_ids() {
        let mut a = app();
        a.apply(Action::AddPropertyTween {
            layer_id: 0, property: "opacity".to_string(), from_frame: 0, to_frame: 10,
            from_val: 1.0, to_val: 0.0, kind: elastic(),
        });
        a.apply(Action::AddPropertyTween {
            layer_id: 0, property: "scale_x".to_string(), from_frame: 0, to_frame: 10,
            from_val: 1.0, to_val: 2.0, kind: AdvancedTweenKind::Bounce { num_bounces: 3, decay: 0.5 },
        });
        assert_ne!(a.property_tweens[0].id, a.property_tweens[1].id);
    }

    #[test]
    fn test_remove_property_tween() {
        let mut a = app();
        a.apply(Action::AddPropertyTween {
            layer_id: 0, property: "opacity".to_string(), from_frame: 0, to_frame: 10,
            from_val: 1.0, to_val: 0.0, kind: elastic(),
        });
        let id = a.property_tweens[0].id;
        a.apply(Action::RemovePropertyTween { tween_id: id });
        assert!(a.property_tweens.is_empty());
    }

    #[test]
    fn test_update_property_tween_kind() {
        let mut a = app();
        a.apply(Action::AddPropertyTween {
            layer_id: 0, property: "opacity".to_string(), from_frame: 0, to_frame: 10,
            from_val: 1.0, to_val: 0.0, kind: elastic(),
        });
        let id = a.property_tweens[0].id;
        let spring = AdvancedTweenKind::Spring { stiffness: 100.0, damping: 10.0, mass: 1.0 };
        a.apply(Action::UpdatePropertyTweenKind { tween_id: id, kind: spring.clone() });
        assert_eq!(a.property_tweens[0].kind, spring);
    }

    #[test]
    fn test_set_property_tween_range() {
        let mut a = app();
        a.apply(Action::AddPropertyTween {
            layer_id: 0, property: "opacity".to_string(), from_frame: 0, to_frame: 10,
            from_val: 1.0, to_val: 0.0, kind: elastic(),
        });
        let id = a.property_tweens[0].id;
        a.apply(Action::SetPropertyTweenRange { tween_id: id, from_frame: 5, to_frame: 20 });
        assert_eq!(a.property_tweens[0].from_frame, 5);
        assert_eq!(a.property_tweens[0].to_frame, 20);
    }

    #[test]
    fn test_tweens_for_layer_filter() {
        let mut a = app();
        a.apply(Action::AddPropertyTween {
            layer_id: 0, property: "opacity".to_string(), from_frame: 0, to_frame: 10,
            from_val: 1.0, to_val: 0.0, kind: elastic(),
        });
        a.apply(Action::AddPropertyTween {
            layer_id: 1, property: "rotation".to_string(), from_frame: 0, to_frame: 10,
            from_val: 0.0, to_val: 90.0, kind: elastic(),
        });
        let for_layer_0 = a.tweens_for_layer(0);
        assert_eq!(for_layer_0.len(), 1);
        assert_eq!(for_layer_0[0].property, "opacity");
    }

    #[test]
    fn test_cubic_bezier_tween() {
        let mut a = app();
        let kind = AdvancedTweenKind::CubicBezier { x1: 0.25, y1: 0.1, x2: 0.25, y2: 1.0 };
        a.apply(Action::AddPropertyTween {
            layer_id: 2, property: "position_x".to_string(), from_frame: 0, to_frame: 60,
            from_val: 0.0, to_val: 500.0, kind: kind.clone(),
        });
        assert_eq!(a.property_tweens[0].kind, kind);
    }
}
