use super::{App, Action};

/// Symbol type.
#[derive(Clone, Debug, PartialEq)]
pub enum SymbolKind {
    MovieClip,
    Button,
    Graphic,
}

/// A reusable symbol in the library.
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: usize,
    pub name: String,
    pub kind: SymbolKind,
    pub width: f32,
    pub height: f32,
    pub registration_x: f32,
    pub registration_y: f32,
}

/// An instance of a symbol placed on a layer.
#[derive(Clone, Debug)]
pub struct SymbolInstance {
    pub id: usize,
    pub symbol_id: usize,
    pub layer_id: usize,
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub alpha: f32,
}

impl App {
    pub fn apply_symbols(&mut self, action: Action) {
        match action {
            Action::CreateSymbol { name, kind, width, height } => {
                let id = self.next_symbol_id;
                self.next_symbol_id += 1;
                self.symbols.push(Symbol {
                    id,
                    name,
                    kind,
                    width,
                    height,
                    registration_x: 0.0,
                    registration_y: 0.0,
                });
            }
            Action::DeleteSymbol { symbol_id } => {
                self.symbols.retain(|s| s.id != symbol_id);
                self.symbol_instances.retain(|si| si.symbol_id != symbol_id);
            }
            Action::PlaceSymbolInstance { symbol_id, layer_id, x, y } => {
                if self.symbols.iter().any(|s| s.id == symbol_id) {
                    let id = self.next_instance_id;
                    self.next_instance_id += 1;
                    self.symbol_instances.push(SymbolInstance {
                        id,
                        symbol_id,
                        layer_id,
                        x,
                        y,
                        scale_x: 1.0,
                        scale_y: 1.0,
                        rotation: 0.0,
                        alpha: 1.0,
                    });
                }
            }
            Action::RemoveSymbolInstance { instance_id } => {
                self.symbol_instances.retain(|si| si.id != instance_id);
            }
            Action::SetInstanceTransform { instance_id, x, y, scale_x, scale_y, rotation, alpha } => {
                if let Some(si) = self.symbol_instances.iter_mut().find(|si| si.id == instance_id) {
                    si.x = x;
                    si.y = y;
                    si.scale_x = scale_x;
                    si.scale_y = scale_y;
                    si.rotation = rotation;
                    si.alpha = alpha.clamp(0.0, 1.0);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::LayerKind;
    use super::SymbolKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_create_symbol() {
        let mut a = app();
        a.apply(Action::CreateSymbol {
            name: "Hero".to_string(),
            kind: SymbolKind::MovieClip,
            width: 100.0,
            height: 200.0,
        });
        assert_eq!(a.symbols.len(), 1);
        assert_eq!(a.symbols[0].name, "Hero");
        assert_eq!(a.symbols[0].kind, SymbolKind::MovieClip);
    }

    #[test]
    fn test_create_symbol_id_increments() {
        let mut a = app();
        a.apply(Action::CreateSymbol { name: "A".to_string(), kind: SymbolKind::Graphic, width: 50.0, height: 50.0 });
        a.apply(Action::CreateSymbol { name: "B".to_string(), kind: SymbolKind::Button, width: 80.0, height: 30.0 });
        assert_ne!(a.symbols[0].id, a.symbols[1].id);
        assert!(a.symbols[0].id < a.symbols[1].id);
    }

    #[test]
    fn test_delete_symbol() {
        let mut a = app();
        a.apply(Action::CreateSymbol { name: "Hero".to_string(), kind: SymbolKind::MovieClip, width: 100.0, height: 100.0 });
        let sid = a.symbols[0].id;
        a.apply(Action::DeleteSymbol { symbol_id: sid });
        assert!(a.symbols.is_empty());
    }

    #[test]
    fn test_place_symbol_instance() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateSymbol { name: "S".to_string(), kind: SymbolKind::Graphic, width: 50.0, height: 50.0 });
        let sym_id = a.symbols[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 100.0, y: 200.0 });
        assert_eq!(a.symbol_instances.len(), 1);
        assert_eq!(a.symbol_instances[0].x, 100.0);
        assert_eq!(a.symbol_instances[0].y, 200.0);
        assert_eq!(a.symbol_instances[0].symbol_id, sym_id);
    }

    #[test]
    fn test_place_instance_nonexistent_symbol_noop() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: 999, layer_id: lid, x: 0.0, y: 0.0 });
        assert!(a.symbol_instances.is_empty());
    }

    #[test]
    fn test_remove_symbol_instance() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateSymbol { name: "S".to_string(), kind: SymbolKind::Graphic, width: 50.0, height: 50.0 });
        let sym_id = a.symbols[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 0.0, y: 0.0 });
        let inst_id = a.symbol_instances[0].id;
        a.apply(Action::RemoveSymbolInstance { instance_id: inst_id });
        assert!(a.symbol_instances.is_empty());
    }

    #[test]
    fn test_set_instance_transform() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateSymbol { name: "S".to_string(), kind: SymbolKind::Graphic, width: 50.0, height: 50.0 });
        let sym_id = a.symbols[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 0.0, y: 0.0 });
        let inst_id = a.symbol_instances[0].id;
        a.apply(Action::SetInstanceTransform {
            instance_id: inst_id,
            x: 150.0, y: 250.0,
            scale_x: 2.0, scale_y: 0.5,
            rotation: 45.0,
            alpha: 0.8,
        });
        let si = &a.symbol_instances[0];
        assert_eq!(si.x, 150.0);
        assert_eq!(si.y, 250.0);
        assert_eq!(si.scale_x, 2.0);
        assert_eq!(si.scale_y, 0.5);
        assert_eq!(si.rotation, 45.0);
        assert_eq!(si.alpha, 0.8);
    }

    #[test]
    fn test_instance_alpha_clamp() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateSymbol { name: "S".to_string(), kind: SymbolKind::Graphic, width: 50.0, height: 50.0 });
        let sym_id = a.symbols[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 0.0, y: 0.0 });
        let inst_id = a.symbol_instances[0].id;
        a.apply(Action::SetInstanceTransform {
            instance_id: inst_id,
            x: 0.0, y: 0.0,
            scale_x: 1.0, scale_y: 1.0,
            rotation: 0.0,
            alpha: 2.5,  // should clamp to 1.0
        });
        assert_eq!(a.symbol_instances[0].alpha, 1.0);
    }

    #[test]
    fn test_delete_symbol_removes_instances() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateSymbol { name: "S".to_string(), kind: SymbolKind::MovieClip, width: 50.0, height: 50.0 });
        let sym_id = a.symbols[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 0.0, y: 0.0 });
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 100.0, y: 0.0 });
        assert_eq!(a.symbol_instances.len(), 2);
        a.apply(Action::DeleteSymbol { symbol_id: sym_id });
        assert!(a.symbol_instances.is_empty());
    }

    #[test]
    fn test_symbol_button_kind() {
        let mut a = app();
        a.apply(Action::CreateSymbol { name: "Btn".to_string(), kind: SymbolKind::Button, width: 80.0, height: 30.0 });
        assert_eq!(a.symbols[0].kind, SymbolKind::Button);
    }

    #[test]
    fn test_instance_default_transform() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::CreateSymbol { name: "S".to_string(), kind: SymbolKind::Graphic, width: 50.0, height: 50.0 });
        let sym_id = a.symbols[0].id;
        a.apply(Action::PlaceSymbolInstance { symbol_id: sym_id, layer_id: lid, x: 0.0, y: 0.0 });
        let si = &a.symbol_instances[0];
        assert_eq!(si.scale_x, 1.0);
        assert_eq!(si.scale_y, 1.0);
        assert_eq!(si.rotation, 0.0);
        assert_eq!(si.alpha, 1.0);
    }
}
