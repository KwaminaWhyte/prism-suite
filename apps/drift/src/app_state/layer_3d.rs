use super::{App, Action};

/// Projection mode for 3D layer rendering.
#[derive(Clone, Debug, PartialEq)]
pub enum Projection3D {
    Perspective,
    Orthographic,
}

/// Per-layer 3D transform data (Flash/Animate 3D parity).
#[derive(Clone, Debug)]
pub struct Layer3DTransform {
    pub layer_id: usize,
    /// Rotation around the X axis in degrees.
    pub rotation_x: f32,
    /// Rotation around the Y axis in degrees.
    pub rotation_y: f32,
    /// Depth offset along the Z axis.
    pub z_position: f32,
    /// Vanishing point as a fraction of the stage (0.5, 0.5 = center).
    pub vanishing_point: (f32, f32),
    pub projection: Projection3D,
    pub enabled: bool,
}

impl Layer3DTransform {
    pub fn new(layer_id: usize) -> Self {
        Self {
            layer_id,
            rotation_x: 0.0,
            rotation_y: 0.0,
            z_position: 0.0,
            vanishing_point: (0.5, 0.5),
            projection: Projection3D::Perspective,
            enabled: true,
        }
    }
}

impl App {
    /// Returns true if the layer has a 3D transform entry.
    pub fn layer_has_3d(&self, layer_id: usize) -> bool {
        self.layer_3d.contains_key(&layer_id)
    }

    pub fn apply_layer_3d(&mut self, action: Action) {
        match action {
            Action::Enable3DLayer { layer_id, enabled } => {
                let entry = self
                    .layer_3d
                    .entry(layer_id)
                    .or_insert_with(|| Layer3DTransform::new(layer_id));
                entry.enabled = enabled;
            }
            Action::SetLayer3DRotationX { layer_id, degrees } => {
                let entry = self
                    .layer_3d
                    .entry(layer_id)
                    .or_insert_with(|| Layer3DTransform::new(layer_id));
                entry.rotation_x = degrees;
            }
            Action::SetLayer3DRotationY { layer_id, degrees } => {
                let entry = self
                    .layer_3d
                    .entry(layer_id)
                    .or_insert_with(|| Layer3DTransform::new(layer_id));
                entry.rotation_y = degrees;
            }
            Action::SetLayerZPosition { layer_id, z } => {
                let entry = self
                    .layer_3d
                    .entry(layer_id)
                    .or_insert_with(|| Layer3DTransform::new(layer_id));
                entry.z_position = z;
            }
            Action::SetVanishingPoint { x, y } => {
                let clamped = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
                for t in self.layer_3d.values_mut() {
                    t.vanishing_point = clamped;
                }
                self.global_vanishing_point = clamped;
            }
            Action::SetProjection3D(proj) => {
                for t in self.layer_3d.values_mut() {
                    t.projection = proj.clone();
                }
                self.global_projection_3d = proj;
            }
            Action::Reset3DTransform { layer_id } => {
                if let Some(t) = self.layer_3d.get_mut(&layer_id) {
                    t.rotation_x = 0.0;
                    t.rotation_y = 0.0;
                    t.z_position = 0.0;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::Projection3D;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_layer_has_3d_false_initially() {
        let a = app();
        assert!(!a.layer_has_3d(0));
    }

    #[test]
    fn test_enable_3d_layer_creates_entry() {
        let mut a = app();
        a.apply(Action::Enable3DLayer { layer_id: 1, enabled: true });
        assert!(a.layer_has_3d(1));
        assert!(a.layer_3d[&1].enabled);
    }

    #[test]
    fn test_disable_3d_layer() {
        let mut a = app();
        a.apply(Action::Enable3DLayer { layer_id: 1, enabled: true });
        a.apply(Action::Enable3DLayer { layer_id: 1, enabled: false });
        assert!(!a.layer_3d[&1].enabled);
    }

    #[test]
    fn test_set_rotation_x() {
        let mut a = app();
        a.apply(Action::SetLayer3DRotationX { layer_id: 2, degrees: 45.0 });
        assert_eq!(a.layer_3d[&2].rotation_x, 45.0);
    }

    #[test]
    fn test_set_rotation_y() {
        let mut a = app();
        a.apply(Action::SetLayer3DRotationY { layer_id: 3, degrees: -30.0 });
        assert_eq!(a.layer_3d[&3].rotation_y, -30.0);
    }

    #[test]
    fn test_set_z_position() {
        let mut a = app();
        a.apply(Action::SetLayerZPosition { layer_id: 4, z: 200.0 });
        assert_eq!(a.layer_3d[&4].z_position, 200.0);
    }

    #[test]
    fn test_set_vanishing_point_clamps() {
        let mut a = app();
        a.apply(Action::Enable3DLayer { layer_id: 1, enabled: true });
        a.apply(Action::SetVanishingPoint { x: -0.5, y: 1.5 });
        assert_eq!(a.layer_3d[&1].vanishing_point, (0.0, 1.0));
    }

    #[test]
    fn test_set_vanishing_point_stored_globally() {
        let mut a = app();
        a.apply(Action::SetVanishingPoint { x: 0.3, y: 0.7 });
        assert_eq!(a.global_vanishing_point, (0.3, 0.7));
    }

    #[test]
    fn test_set_projection_3d_orthographic() {
        let mut a = app();
        a.apply(Action::Enable3DLayer { layer_id: 1, enabled: true });
        a.apply(Action::SetProjection3D(Projection3D::Orthographic));
        assert_eq!(a.layer_3d[&1].projection, Projection3D::Orthographic);
    }

    #[test]
    fn test_reset_3d_transform() {
        let mut a = app();
        a.apply(Action::SetLayer3DRotationX { layer_id: 5, degrees: 90.0 });
        a.apply(Action::SetLayer3DRotationY { layer_id: 5, degrees: 45.0 });
        a.apply(Action::SetLayerZPosition { layer_id: 5, z: 100.0 });
        a.apply(Action::Reset3DTransform { layer_id: 5 });
        let t = &a.layer_3d[&5];
        assert_eq!(t.rotation_x, 0.0);
        assert_eq!(t.rotation_y, 0.0);
        assert_eq!(t.z_position, 0.0);
    }

    #[test]
    fn test_multiple_layers_independent_3d() {
        let mut a = app();
        a.apply(Action::SetLayer3DRotationX { layer_id: 10, degrees: 20.0 });
        a.apply(Action::SetLayer3DRotationX { layer_id: 11, degrees: 80.0 });
        assert_eq!(a.layer_3d[&10].rotation_x, 20.0);
        assert_eq!(a.layer_3d[&11].rotation_x, 80.0);
    }

    #[test]
    fn test_default_projection_perspective() {
        let mut a = app();
        a.apply(Action::Enable3DLayer { layer_id: 1, enabled: true });
        assert_eq!(a.layer_3d[&1].projection, Projection3D::Perspective);
    }
}
