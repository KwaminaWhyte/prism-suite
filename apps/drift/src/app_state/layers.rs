use super::{App, Action, LayerTransform};

/// The kind of content a layer holds.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerKind {
    Vector,
    Bitmap,
    Audio,
    Camera,
    Guide,
    Null,
}

/// A single timeline layer.
#[derive(Clone, Debug)]
pub struct DriftLayer {
    pub id: usize,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub locked: bool,
    pub solo: bool,
    pub parent_id: Option<usize>,
    pub z_order: usize,
    pub color_tag: String,
    pub start_frame: usize,
    pub end_frame: usize,
}

impl App {
    pub fn apply_layers(&mut self, action: Action) {
        match action {
            Action::AddLayer { name, kind } => {
                let id = self.layer_counter;
                self.layer_counter += 1;
                let z = self.layers.len();
                self.layers.push(DriftLayer {
                    id,
                    name,
                    kind,
                    visible: true,
                    locked: false,
                    solo: false,
                    parent_id: None,
                    z_order: z,
                    color_tag: String::new(),
                    start_frame: 0,
                    end_frame: self.document.duration_frames,
                });
                self.transforms.insert(id, LayerTransform::new());
                self.active_layer = Some(id);
            }
            Action::DeleteLayer(id) => {
                self.layers.retain(|l| l.id != id);
                self.keyframes.retain(|k| k.layer_id != id);
                self.transforms.remove(&id);
                self.layer_rigs.retain(|r| r.layer_id != id);
                if self.active_layer == Some(id) {
                    self.active_layer = self.layers.last().map(|l| l.id);
                }
            }
            Action::RenameLayer { id, name } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.name = name;
                }
            }
            Action::SetLayerVisible { id, visible } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.visible = visible;
                }
            }
            Action::SetLayerLocked { id, locked } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.locked = locked;
                }
            }
            Action::SetLayerSolo { id, solo } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.solo = solo;
                }
            }
            Action::SetLayerParent { id, parent_id } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.parent_id = parent_id;
                }
            }
            Action::ReorderLayers(order) => {
                let mut new_layers: Vec<DriftLayer> = Vec::with_capacity(order.len());
                for (z, &oid) in order.iter().enumerate() {
                    if let Some(mut layer) = self.layers.iter().find(|l| l.id == oid).cloned() {
                        layer.z_order = z;
                        new_layers.push(layer);
                    }
                }
                self.layers = new_layers;
            }
            Action::SetLayerColorTag { id, tag } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.color_tag = tag;
                }
            }
            Action::DuplicateLayer(id) => {
                if let Some(src) = self.layers.iter().find(|l| l.id == id).cloned() {
                    let new_id = self.layer_counter;
                    self.layer_counter += 1;
                    let z = self.layers.len();
                    let src_transform = self
                        .transforms
                        .get(&src.id)
                        .cloned()
                        .unwrap_or_else(LayerTransform::new);
                    self.layers.push(DriftLayer {
                        id: new_id,
                        name: format!("{} copy", src.name),
                        z_order: z,
                        ..src
                    });
                    self.transforms.insert(new_id, src_transform);
                    self.active_layer = Some(new_id);
                }
            }
            Action::SetLayerStartFrame { id, frame } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.start_frame = frame;
                }
            }
            Action::SetLayerEndFrame { id, frame } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                    l.end_frame = frame;
                }
            }
            Action::SetActiveLayer(id) => {
                if self.layers.iter().any(|l| l.id == id) {
                    self.active_layer = Some(id);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::LayerKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Layer 1".to_string(), kind: LayerKind::Vector });
        assert_eq!(a.layers.len(), 1);
        assert_eq!(a.layers[0].name, "Layer 1");
        assert_eq!(a.active_layer, Some(0));
    }

    #[test]
    fn test_add_multiple_layers() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Bitmap });
        assert_eq!(a.layers.len(), 2);
        assert_eq!(a.active_layer, Some(1));
    }

    #[test]
    fn test_delete_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::DeleteLayer(id));
        assert!(a.layers.is_empty());
    }

    #[test]
    fn test_delete_layer_removes_keyframes() {
        use super::super::EasingKind;
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::AddKeyframe {
            layer_id: id,
            property: "position_x".to_string(),
            frame: 0,
            value: 0.0,
            easing: EasingKind::Linear,
        });
        a.apply(Action::DeleteLayer(id));
        assert!(a.keyframes.is_empty());
    }

    #[test]
    fn test_rename_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Old".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::RenameLayer { id, name: "New".to_string() });
        assert_eq!(a.layers[0].name, "New");
    }

    #[test]
    fn test_set_layer_visible() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerVisible { id, visible: false });
        assert!(!a.layers[0].visible);
    }

    #[test]
    fn test_set_layer_locked() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerLocked { id, locked: true });
        assert!(a.layers[0].locked);
    }

    #[test]
    fn test_set_layer_solo() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerSolo { id, solo: true });
        assert!(a.layers[0].solo);
    }

    #[test]
    fn test_set_layer_parent() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Parent".to_string(), kind: LayerKind::Null });
        a.apply(Action::AddLayer { name: "Child".to_string(), kind: LayerKind::Vector });
        let parent_id = a.layers[0].id;
        let child_id = a.layers[1].id;
        a.apply(Action::SetLayerParent { id: child_id, parent_id: Some(parent_id) });
        assert_eq!(a.layers[1].parent_id, Some(parent_id));
    }

    #[test]
    fn test_reorder_layers() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        let id_a = a.layers[0].id;
        let id_b = a.layers[1].id;
        a.apply(Action::ReorderLayers(vec![id_b, id_a]));
        assert_eq!(a.layers[0].id, id_b);
        assert_eq!(a.layers[1].id, id_a);
    }

    #[test]
    fn test_set_layer_color_tag() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerColorTag { id, tag: "red".to_string() });
        assert_eq!(a.layers[0].color_tag, "red");
    }

    #[test]
    fn test_duplicate_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Original".to_string(), kind: LayerKind::Vector });
        let orig_id = a.layers[0].id;
        a.apply(Action::DuplicateLayer(orig_id));
        assert_eq!(a.layers.len(), 2);
        assert!(a.layers[1].name.contains("copy"));
    }

    #[test]
    fn test_set_layer_start_end_frame() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        a.apply(Action::SetLayerStartFrame { id, frame: 10 });
        a.apply(Action::SetLayerEndFrame { id, frame: 100 });
        assert_eq!(a.layers[0].start_frame, 10);
        assert_eq!(a.layers[0].end_frame, 100);
    }

    #[test]
    fn test_set_active_layer() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        let id_a = a.layers[0].id;
        a.apply(Action::SetActiveLayer(id_a));
        assert_eq!(a.active_layer, Some(id_a));
    }

    #[test]
    fn test_layer_adds_transform() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let id = a.layers[0].id;
        assert!(a.transforms.contains_key(&id));
    }

    #[test]
    fn test_layer_kind_camera() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Camera".to_string(), kind: LayerKind::Camera });
        assert_eq!(a.layers[0].kind, LayerKind::Camera);
    }

    #[test]
    fn test_layer_kind_audio() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "VO".to_string(), kind: LayerKind::Audio });
        assert_eq!(a.layers[0].kind, LayerKind::Audio);
    }

    #[test]
    fn test_layer_kind_null() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Ctrl".to_string(), kind: LayerKind::Null });
        assert_eq!(a.layers[0].kind, LayerKind::Null);
    }

    #[test]
    fn test_guide_layer_kind() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Guide".to_string(), kind: LayerKind::Guide });
        assert_eq!(a.layers[0].kind, LayerKind::Guide);
    }

    #[test]
    fn test_set_layer_parent_to_none() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Null });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        let pa = a.layers[0].id;
        let pb = a.layers[1].id;
        a.apply(Action::SetLayerParent { id: pb, parent_id: Some(pa) });
        a.apply(Action::SetLayerParent { id: pb, parent_id: None });
        assert_eq!(a.layers[1].parent_id, None);
    }

    #[test]
    fn test_layer_default_visible_unlocked() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        assert!(a.layers[0].visible);
        assert!(!a.layers[0].locked);
        assert!(!a.layers[0].solo);
    }

    #[test]
    fn test_delete_nonexistent_layer_noop() {
        let mut a = app();
        a.apply(Action::DeleteLayer(999));
        assert!(a.layers.is_empty());
    }

    #[test]
    fn test_layer_z_order_assigned() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        a.apply(Action::AddLayer { name: "B".to_string(), kind: LayerKind::Vector });
        assert_eq!(a.layers[0].z_order, 0);
        assert_eq!(a.layers[1].z_order, 1);
    }
}
