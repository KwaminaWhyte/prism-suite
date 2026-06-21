use super::*;

// ---- New Feature: SmartObject (rich) -----------------------------------------

/// Whether the Smart Object embeds its contents or links to an external file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartObjectKind {
    Embedded,
    Linked,
}

/// A rich Smart Object entry — tracks id, name, kind, source path, and dirty flag.
#[derive(Debug, Clone)]
pub struct SmartObject {
    pub id: usize,
    pub name: String,
    pub kind: SmartObjectKind,
    pub source_path: Option<String>,
    pub contents_dirty: bool,
}

// ---- New Feature: AdvancedMasking (Select & Mask workspace) ------------------

/// Edge detection algorithm used in the Select & Mask workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeDetectMode {
    Object,
    Hair,
    Custom,
}

/// Full configuration for the Select & Mask / Refine Edge workspace.
#[derive(Debug, Clone)]
pub struct SelectMaskConfig {
    /// Detect Edges radius (0..=250).
    pub radius: f32,
    /// Automatically adjust the radius around complex edges.
    pub smart_radius: bool,
    /// Smooth the selection boundary (0..=100).
    pub smooth: u8,
    /// Gaussian feather applied to the mask edge (0..=250).
    pub feather: f32,
    /// Increase edge definition (0..=100).
    pub contrast: u8,
    /// Shrink or grow the selection boundary (-100..=100).
    pub shift_edge: i8,
    /// Where to deliver the refined mask.
    pub output_to: String,
    /// Remove colour fringing around the mask edge.
    pub decontaminate_colors: bool,
    /// Algorithm for Detect Edges.
    pub edge_detect: EdgeDetectMode,
}

impl SelectMaskConfig {
    pub fn new() -> Self {
        Self {
            radius: 3.0,
            smart_radius: true,
            smooth: 3,
            feather: 0.0,
            contrast: 0,
            shift_edge: 0,
            output_to: "Mask".into(),
            decontaminate_colors: false,
            edge_detect: EdgeDetectMode::Object,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_object_kind_variants() {
        let k = SmartObjectKind::Embedded;
        assert_eq!(k, SmartObjectKind::Embedded);
        assert_ne!(k, SmartObjectKind::Linked);
    }

    #[test]
    fn test_select_mask_config_new() {
        let cfg = SelectMaskConfig::new();
        assert_eq!(cfg.radius, 3.0);
        assert!(cfg.smart_radius);
        assert_eq!(cfg.smooth, 3);
        assert_eq!(cfg.feather, 0.0);
        assert_eq!(cfg.contrast, 0);
        assert_eq!(cfg.shift_edge, 0);
        assert_eq!(cfg.output_to, "Mask");
        assert!(!cfg.decontaminate_colors);
        assert_eq!(cfg.edge_detect, EdgeDetectMode::Object);
    }

    #[test]
    fn test_edge_detect_mode_variants() {
        assert_eq!(EdgeDetectMode::Object, EdgeDetectMode::Object);
        assert_ne!(EdgeDetectMode::Hair, EdgeDetectMode::Custom);
    }

    #[test]
    fn test_smart_object_convert_action() {
        let mut app = App::new();
        let initial_count = app.smart_object_list.len();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        assert_eq!(app.smart_object_list.len(), initial_count + 1);
        let so = app.smart_object_list.last().unwrap();
        assert_eq!(so.kind, SmartObjectKind::Embedded);
        assert!(!so.contents_dirty);
    }

    #[test]
    fn test_smart_object_replace() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 0 });
        let so_id = app.smart_object_list.last().unwrap().id;
        app.apply(Action::SmartObjectReplace { so_id, new_path: "/tmp/new.png".into() });
        let so = app.smart_object_list.iter().find(|s| s.id == so_id).unwrap();
        assert_eq!(so.source_path.as_deref(), Some("/tmp/new.png"));
        assert!(so.contents_dirty);
    }
}
