use super::{App, Action};

/// Interpolation easing for a keyframe segment.
#[derive(Clone, Debug, PartialEq)]
pub enum EasingKind {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bezier,
    Hold,
    Spring,
}

/// One keyframe on a named property of a layer.
#[derive(Clone, Debug)]
pub struct Keyframe {
    pub id: usize,
    pub layer_id: usize,
    pub property: String,
    pub frame: usize,
    pub value: f32,
    pub easing: EasingKind,
    pub bezier_handle_in: (f32, f32),
    pub bezier_handle_out: (f32, f32),
}

impl App {
    pub fn apply_keyframes(&mut self, action: Action) {
        match action {
            Action::AddKeyframe { layer_id, property, frame, value, easing } => {
                let id = self.keyframe_counter;
                self.keyframe_counter += 1;
                self.keyframes.push(Keyframe {
                    id,
                    layer_id,
                    property,
                    frame,
                    value,
                    easing,
                    bezier_handle_in: (0.0, 0.0),
                    bezier_handle_out: (1.0, 1.0),
                });
            }
            Action::DeleteKeyframe(id) => {
                self.keyframes.retain(|k| k.id != id);
            }
            Action::MoveKeyframe { id, frame } => {
                if let Some(k) = self.keyframes.iter_mut().find(|k| k.id == id) {
                    k.frame = frame;
                }
            }
            Action::SetKeyframeValue { id, value } => {
                if let Some(k) = self.keyframes.iter_mut().find(|k| k.id == id) {
                    k.value = value;
                }
            }
            Action::SetKeyframeEasing { id, easing } => {
                if let Some(k) = self.keyframes.iter_mut().find(|k| k.id == id) {
                    k.easing = easing;
                }
            }
            Action::SetPropertyAtFrame { layer_id, property, frame, value } => {
                let existing = self
                    .keyframes
                    .iter_mut()
                    .find(|k| k.layer_id == layer_id && k.property == property && k.frame == frame);
                if let Some(k) = existing {
                    k.value = value;
                } else {
                    let id = self.keyframe_counter;
                    self.keyframe_counter += 1;
                    self.keyframes.push(Keyframe {
                        id,
                        layer_id,
                        property,
                        frame,
                        value,
                        easing: EasingKind::Linear,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::EasingKind;
    use super::super::LayerKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 5,
            value: 100.0,
            easing: EasingKind::EaseIn,
        });
        assert_eq!(a.keyframes.len(), 1);
        assert_eq!(a.keyframes[0].frame, 5);
        assert_eq!(a.keyframes[0].value, 100.0);
    }

    #[test]
    fn test_delete_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 5,
            value: 100.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::DeleteKeyframe(kid));
        assert!(a.keyframes.is_empty());
    }

    #[test]
    fn test_move_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_x".to_string(),
            frame: 5,
            value: 0.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::MoveKeyframe { id: kid, frame: 15 });
        assert_eq!(a.keyframes[0].frame, 15);
    }

    #[test]
    fn test_set_keyframe_value() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "opacity".to_string(),
            frame: 0,
            value: 1.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::SetKeyframeValue { id: kid, value: 0.5 });
        assert_eq!(a.keyframes[0].value, 0.5);
    }

    #[test]
    fn test_set_keyframe_easing() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "position_y".to_string(),
            frame: 0,
            value: 0.0,
            easing: EasingKind::Linear,
        });
        let kid = a.keyframes[0].id;
        a.apply(Action::SetKeyframeEasing { id: kid, easing: EasingKind::Spring });
        assert_eq!(a.keyframes[0].easing, EasingKind::Spring);
    }

    #[test]
    fn test_set_property_at_frame_creates_keyframe() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetPropertyAtFrame {
            layer_id: lid,
            property: "scale_x".to_string(),
            frame: 10,
            value: 2.0,
        });
        assert_eq!(a.keyframes.len(), 1);
        assert_eq!(a.keyframes[0].easing, EasingKind::Linear);
    }

    #[test]
    fn test_set_property_at_frame_updates_existing() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetPropertyAtFrame {
            layer_id: lid,
            property: "scale_x".to_string(),
            frame: 10,
            value: 2.0,
        });
        a.apply(Action::SetPropertyAtFrame {
            layer_id: lid,
            property: "scale_x".to_string(),
            frame: 10,
            value: 3.0,
        });
        assert_eq!(a.keyframes.len(), 1);
        assert_eq!(a.keyframes[0].value, 3.0);
    }

    #[test]
    fn test_keyframe_counter_monotonic() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        for i in 0..10 {
            a.apply(Action::AddKeyframe {
                layer_id: lid,
                property: "scale".to_string(),
                frame: i,
                value: 1.0,
                easing: EasingKind::Hold,
            });
        }
        let ids: Vec<usize> = a.keyframes.iter().map(|k| k.id).collect();
        for i in 1..ids.len() {
            assert!(ids[i] > ids[i - 1]);
        }
    }

    #[test]
    fn test_easing_bezier_variant() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: lid,
            property: "rotation".to_string(),
            frame: 0,
            value: 0.0,
            easing: EasingKind::Bezier,
        });
        assert_eq!(a.keyframes[0].easing, EasingKind::Bezier);
    }
}
