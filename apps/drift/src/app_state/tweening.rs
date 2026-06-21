use super::{App, Action, EasingKind};

/// Type of tween interpolation.
#[derive(Clone, Debug, PartialEq)]
pub enum TweenKind {
    Classic,
    Shape,
    Motion,
}

/// Rotation direction for classic tweens.
#[derive(Clone, Debug, PartialEq)]
pub enum RotateDirection {
    None,
    Clockwise,
    CounterClockwise,
    Auto,
}

/// A tween spanning a range of frames on a layer.
#[derive(Clone, Debug)]
pub struct Tween {
    pub id: usize,
    pub layer_id: usize,
    pub kind: TweenKind,
    pub start_frame: usize,
    pub end_frame: usize,
    pub easing: EasingKind,
    pub rotate: RotateDirection,
    pub rotate_count: i32,
}

/// Onion skinning configuration.
#[derive(Clone, Debug)]
pub struct OnionSkinConfig {
    pub enabled: bool,
    pub frames_before: usize,
    pub frames_after: usize,
    pub before_alpha: f32,
    pub after_alpha: f32,
}

impl OnionSkinConfig {
    pub fn new() -> Self {
        Self {
            enabled: false,
            frames_before: 2,
            frames_after: 2,
            before_alpha: 0.5,
            after_alpha: 0.5,
        }
    }
}

impl App {
    pub fn apply_tweening(&mut self, action: Action) {
        match action {
            Action::CreateTween { layer_id, kind, start_frame, end_frame } => {
                let id = self.next_tween_id;
                self.next_tween_id += 1;
                self.tweens.push(Tween {
                    id,
                    layer_id,
                    kind,
                    start_frame,
                    end_frame,
                    easing: EasingKind::Linear,
                    rotate: RotateDirection::None,
                    rotate_count: 0,
                });
            }
            Action::RemoveTween { tween_id } => {
                self.tweens.retain(|t| t.id != tween_id);
            }
            Action::SetTweenEasing { tween_id, easing } => {
                if let Some(t) = self.tweens.iter_mut().find(|t| t.id == tween_id) {
                    t.easing = easing;
                }
            }
            Action::SetTweenRotation { tween_id, direction, count } => {
                if let Some(t) = self.tweens.iter_mut().find(|t| t.id == tween_id) {
                    t.rotate = direction;
                    t.rotate_count = count;
                }
            }
            Action::SetOnionSkin { enabled, frames_before, frames_after } => {
                self.onion_skin.enabled = enabled;
                self.onion_skin.frames_before = frames_before;
                self.onion_skin.frames_after = frames_after;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::{LayerKind, EasingKind};
    use super::{TweenKind, RotateDirection};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_create_tween_classic() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween {
            layer_id: lid,
            kind: TweenKind::Classic,
            start_frame: 0,
            end_frame: 24,
        });
        assert_eq!(a.tweens.len(), 1);
        assert_eq!(a.tweens[0].kind, TweenKind::Classic);
        assert_eq!(a.tweens[0].start_frame, 0);
        assert_eq!(a.tweens[0].end_frame, 24);
    }

    #[test]
    fn test_create_tween_shape() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween {
            layer_id: lid,
            kind: TweenKind::Shape,
            start_frame: 10,
            end_frame: 30,
        });
        assert_eq!(a.tweens[0].kind, TweenKind::Shape);
    }

    #[test]
    fn test_create_tween_motion() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween {
            layer_id: lid,
            kind: TweenKind::Motion,
            start_frame: 0,
            end_frame: 48,
        });
        assert_eq!(a.tweens[0].kind, TweenKind::Motion);
    }

    #[test]
    fn test_tween_id_increments() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 12 });
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Shape, start_frame: 12, end_frame: 24 });
        assert_ne!(a.tweens[0].id, a.tweens[1].id);
    }

    #[test]
    fn test_remove_tween() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 24 });
        let tween_id = a.tweens[0].id;
        a.apply(Action::RemoveTween { tween_id });
        assert!(a.tweens.is_empty());
    }

    #[test]
    fn test_set_tween_easing() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 24 });
        let tween_id = a.tweens[0].id;
        a.apply(Action::SetTweenEasing { tween_id, easing: EasingKind::EaseInOut });
        assert_eq!(a.tweens[0].easing, EasingKind::EaseInOut);
    }

    #[test]
    fn test_tween_default_easing_linear() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 24 });
        assert_eq!(a.tweens[0].easing, EasingKind::Linear);
    }

    #[test]
    fn test_set_tween_rotation_clockwise() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 24 });
        let tween_id = a.tweens[0].id;
        a.apply(Action::SetTweenRotation { tween_id, direction: RotateDirection::Clockwise, count: 2 });
        assert_eq!(a.tweens[0].rotate, RotateDirection::Clockwise);
        assert_eq!(a.tweens[0].rotate_count, 2);
    }

    #[test]
    fn test_set_tween_rotation_counter_clockwise() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 24 });
        let tween_id = a.tweens[0].id;
        a.apply(Action::SetTweenRotation { tween_id, direction: RotateDirection::CounterClockwise, count: 1 });
        assert_eq!(a.tweens[0].rotate, RotateDirection::CounterClockwise);
    }

    #[test]
    fn test_set_tween_rotation_auto() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Motion, start_frame: 0, end_frame: 24 });
        let tween_id = a.tweens[0].id;
        a.apply(Action::SetTweenRotation { tween_id, direction: RotateDirection::Auto, count: 0 });
        assert_eq!(a.tweens[0].rotate, RotateDirection::Auto);
    }

    #[test]
    fn test_onion_skin_disabled_by_default() {
        let a = app();
        assert!(!a.onion_skin.enabled);
    }

    #[test]
    fn test_set_onion_skin_enabled() {
        let mut a = app();
        a.apply(Action::SetOnionSkin { enabled: true, frames_before: 3, frames_after: 2 });
        assert!(a.onion_skin.enabled);
        assert_eq!(a.onion_skin.frames_before, 3);
        assert_eq!(a.onion_skin.frames_after, 2);
    }

    #[test]
    fn test_set_onion_skin_disabled() {
        let mut a = app();
        a.apply(Action::SetOnionSkin { enabled: true, frames_before: 3, frames_after: 3 });
        a.apply(Action::SetOnionSkin { enabled: false, frames_before: 3, frames_after: 3 });
        assert!(!a.onion_skin.enabled);
    }

    #[test]
    fn test_tween_default_rotate_none() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 12 });
        assert_eq!(a.tweens[0].rotate, RotateDirection::None);
        assert_eq!(a.tweens[0].rotate_count, 0);
    }

    #[test]
    fn test_multiple_tweens_same_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Classic, start_frame: 0, end_frame: 12 });
        a.apply(Action::CreateTween { layer_id: lid, kind: TweenKind::Shape, start_frame: 12, end_frame: 24 });
        assert_eq!(a.tweens.len(), 2);
        assert_eq!(a.tweens[0].layer_id, lid);
        assert_eq!(a.tweens[1].layer_id, lid);
    }
}
