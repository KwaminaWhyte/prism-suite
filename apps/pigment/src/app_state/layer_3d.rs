use super::*;

// ---- New Feature: Basic3DLayer -----------------------------------------------

/// The built-in primitive shape kinds for a 3-D layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape3DKind {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Plane,
    Custom,
}

/// Transform and geometry for a single 3-D layer.
#[derive(Debug, Clone)]
pub struct Layer3DProps {
    pub layer_id: usize,
    pub shape: Shape3DKind,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
    pub extrude_depth: f32,
}

impl Layer3DProps {
    pub fn new(layer_id: usize) -> Self {
        Self {
            layer_id,
            shape: Shape3DKind::Cube,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
            extrude_depth: 0.0,
        }
    }
}

impl App {
    pub(super) fn apply_layer_3d(&mut self, action: Action) {
        match action {
            Action::Create3DLayer { layer_id, shape } => {
                let mut props = Layer3DProps::new(layer_id);
                props.shape = shape;
                self.layer_3d_props.push(props);
                self.active_3d_layer = Some(layer_id);
            }
            Action::Set3DPosition { layer_id, x, y, z } => {
                if let Some(p) = self.layer_3d_props.iter_mut().find(|p| p.layer_id == layer_id) {
                    p.pos_x = x;
                    p.pos_y = y;
                    p.pos_z = z;
                }
            }
            Action::SetLayer3DRotation { layer_id, x, y, z } => {
                if let Some(p) = self.layer_3d_props.iter_mut().find(|p| p.layer_id == layer_id) {
                    p.rot_x = x;
                    p.rot_y = y;
                    p.rot_z = z;
                }
            }
            Action::Set3DScale { layer_id, x, y, z } => {
                if let Some(p) = self.layer_3d_props.iter_mut().find(|p| p.layer_id == layer_id) {
                    p.scale_x = x.clamp(0.01, 10.0);
                    p.scale_y = y.clamp(0.01, 10.0);
                    p.scale_z = z.clamp(0.01, 10.0);
                }
            }
            Action::Set3DExtrudeDepth { layer_id, depth } => {
                if let Some(p) = self.layer_3d_props.iter_mut().find(|p| p.layer_id == layer_id) {
                    p.extrude_depth = depth.clamp(0.0, 5000.0);
                }
            }
            Action::Flatten3DLayer { layer_id } => {
                self.layer_3d_props.retain(|p| p.layer_id != layer_id);
                if self.active_3d_layer == Some(layer_id) {
                    self.active_3d_layer = None;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_3d_props_new_defaults() {
        let props = Layer3DProps::new(7);
        assert_eq!(props.layer_id, 7);
        assert_eq!(props.shape, Shape3DKind::Cube);
        assert_eq!(props.pos_x, 0.0);
        assert_eq!(props.scale_x, 1.0);
        assert_eq!(props.extrude_depth, 0.0);
    }

    #[test]
    fn test_shape_3d_kind_variants() {
        assert_eq!(Shape3DKind::Cube, Shape3DKind::Cube);
        assert_ne!(Shape3DKind::Sphere, Shape3DKind::Cylinder);
    }

    #[test]
    fn test_create_3d_layer() {
        let mut app = App::new();
        let initial = app.layer_3d_props.len();
        app.apply(Action::Create3DLayer { layer_id: 1, shape: Shape3DKind::Sphere });
        assert_eq!(app.layer_3d_props.len(), initial + 1);
        let p = app.layer_3d_props.last().unwrap();
        assert_eq!(p.shape, Shape3DKind::Sphere);
        assert_eq!(app.active_3d_layer, Some(1));
    }

    #[test]
    fn test_set_3d_scale_clamps() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 2, shape: Shape3DKind::Cube });
        app.apply(Action::Set3DScale { layer_id: 2, x: 20.0, y: -1.0, z: 0.005 });
        let p = app.layer_3d_props.iter().find(|p| p.layer_id == 2).unwrap();
        assert_eq!(p.scale_x, 10.0);
        assert_eq!(p.scale_y, 0.01);
        assert_eq!(p.scale_z, 0.01);
    }

    #[test]
    fn test_flatten_3d_layer() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 3, shape: Shape3DKind::Plane });
        let before = app.layer_3d_props.len();
        app.apply(Action::Flatten3DLayer { layer_id: 3 });
        assert_eq!(app.layer_3d_props.len(), before - 1);
        assert!(app.active_3d_layer.is_none());
    }
}
