use super::*;

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
