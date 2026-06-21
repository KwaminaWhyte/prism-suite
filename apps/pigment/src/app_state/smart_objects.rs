use super::*;

impl App {
    pub(super) fn apply_smart_objects(&mut self, action: Action) {
        match action {
            Action::ConvertToSmartObject(id) => {
                let path = std::path::PathBuf::from(format!("/tmp/pigment-so-{}.png", id.0));
                self.smart_objects.insert(id, path);
            }
            Action::EditSmartObject(id) => {
                if let Some(path) = self.smart_objects.get(&id) {
                    log::info!("would open {:?}", path);
                }
            }
            Action::RasterizeSmartObject(id) => {
                self.smart_objects.remove(&id);
            }
            Action::SmartObjectConvert { layer_id } => {
                let id = self.smart_object_counter;
                self.smart_object_counter += 1;
                self.smart_object_list.push(SmartObject {
                    id,
                    name: format!("Smart Object {id}"),
                    kind: SmartObjectKind::Embedded,
                    source_path: None,
                    contents_dirty: false,
                });
                let _ = layer_id;
            }
            Action::SmartObjectReplace { so_id, new_path } => {
                if let Some(so) = self.smart_object_list.iter_mut().find(|s| s.id == so_id) {
                    so.source_path = Some(new_path);
                    so.contents_dirty = true;
                }
            }
            Action::SmartObjectRasterize { so_id } => {
                self.smart_object_list.retain(|s| s.id != so_id);
            }
            Action::SmartObjectExport { so_id } => {
                if let Some(so) = self.smart_object_list.iter_mut().find(|s| s.id == so_id) {
                    so.contents_dirty = false;
                }
            }
            _ => {}
        }
    }
}
