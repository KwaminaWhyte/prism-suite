use super::{App, Action};

/// The full spatial transform for a layer at a given instant.
#[derive(Clone, Debug)]
pub struct LayerTransform {
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
}

impl LayerTransform {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            anchor_x: 0.0,
            anchor_y: 0.0,
        }
    }
}

impl App {
    pub fn apply_transforms(&mut self, action: Action) {
        match action {
            Action::SetLayerPosition { id, x, y } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.x = x;
                t.y = y;
            }
            Action::SetLayerScale { id, x, y } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.scale_x = x.clamp(0.001, 100.0);
                t.scale_y = y.clamp(0.001, 100.0);
            }
            Action::SetLayerRotation { id, degrees } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.rotation = degrees;
            }
            Action::SetLayerOpacity { id, opacity } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.opacity = opacity.clamp(0.0, 1.0);
            }
            Action::SetLayerAnchor { id, x, y } => {
                let t = self.transforms.entry(id).or_insert_with(LayerTransform::new);
                t.anchor_x = x;
                t.anchor_y = y;
            }
            Action::ResetLayerTransform(id) => {
                self.transforms.insert(id, LayerTransform::new());
            }
            _ => {}
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
    fn test_set_layer_position() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerPosition { id, x: 100.0, y: 200.0 });
        let t = &a.transforms[&id];
        assert_eq!(t.x, 100.0);
        assert_eq!(t.y, 200.0);
    }

    #[test]
    fn test_set_layer_scale_clamp() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerScale { id, x: -5.0, y: 999.0 });
        let t = &a.transforms[&id];
        assert_eq!(t.scale_x, 0.001);
        assert_eq!(t.scale_y, 100.0);
    }

    #[test]
    fn test_set_layer_rotation() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerRotation { id, degrees: 45.0 });
        assert_eq!(a.transforms[&id].rotation, 45.0);
    }

    #[test]
    fn test_set_layer_opacity_clamp() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerOpacity { id, opacity: 1.5 });
        assert_eq!(a.transforms[&id].opacity, 1.0);
        a.apply(Action::SetLayerOpacity { id, opacity: -0.5 });
        assert_eq!(a.transforms[&id].opacity, 0.0);
    }

    #[test]
    fn test_set_layer_anchor() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerAnchor { id, x: 50.0, y: 50.0 });
        let t = &a.transforms[&id];
        assert_eq!(t.anchor_x, 50.0);
        assert_eq!(t.anchor_y, 50.0);
    }

    #[test]
    fn test_reset_layer_transform() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerPosition { id, x: 500.0, y: 500.0 });
        a.apply(Action::ResetLayerTransform(id));
        let t = &a.transforms[&id];
        assert_eq!(t.x, 0.0);
        assert_eq!(t.y, 0.0);
        assert_eq!(t.scale_x, 1.0);
    }
}
