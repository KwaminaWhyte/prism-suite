use super::{App, Action};

/// A single variant in a swap set (e.g. "Open", "Closed", "Half").
#[derive(Clone, Debug)]
pub struct SwapSetItem {
    pub id: usize,
    pub name: String,
    /// Which symbol to display when this item is active.
    pub symbol_id: Option<usize>,
}

/// A swap set allows swapping between alternate artwork for a body part.
#[derive(Clone, Debug)]
pub struct SwapSet {
    pub id: usize,
    /// Layer this swap set is attached to.
    pub layer_id: usize,
    /// Human-readable name, e.g. "Mouth".
    pub name: String,
    pub items: Vec<SwapSetItem>,
    /// ID of the currently active item.
    pub active_item_id: usize,
}

impl App {
    pub fn apply_swap_sets(&mut self, action: Action) {
        match action {
            Action::CreateSwapSet { layer_id, name } => {
                let id = self.next_swap_set_id;
                self.next_swap_set_id += 1;
                self.swap_sets.push(SwapSet {
                    id,
                    layer_id,
                    name,
                    items: Vec::new(),
                    active_item_id: 0,
                });
            }
            Action::AddSwapItem { swap_set_id, name, symbol_id } => {
                if let Some(ss) = self.swap_sets.iter_mut().find(|ss| ss.id == swap_set_id) {
                    let item_id = self.next_swap_item_id;
                    self.next_swap_item_id += 1;
                    // First item auto-activates.
                    let is_first = ss.items.is_empty();
                    ss.items.push(SwapSetItem { id: item_id, name, symbol_id });
                    if is_first {
                        ss.active_item_id = item_id;
                    }
                }
            }
            Action::RemoveSwapItem { swap_set_id, item_id } => {
                if let Some(ss) = self.swap_sets.iter_mut().find(|ss| ss.id == swap_set_id) {
                    ss.items.retain(|i| i.id != item_id);
                    // If the active item was removed, pick the first remaining or 0.
                    if ss.active_item_id == item_id {
                        ss.active_item_id = ss.items.first().map(|i| i.id).unwrap_or(0);
                    }
                }
            }
            Action::ActivateSwapItem { swap_set_id, item_id } => {
                if let Some(ss) = self.swap_sets.iter_mut().find(|ss| ss.id == swap_set_id) {
                    if ss.items.iter().any(|i| i.id == item_id) {
                        ss.active_item_id = item_id;
                    }
                }
            }
            Action::DeleteSwapSet { swap_set_id } => {
                self.swap_sets.retain(|ss| ss.id != swap_set_id);
            }
            _ => {}
        }
    }

    /// Returns the active SwapSetItem for a given swap set, if any.
    pub fn active_swap_item(&self, swap_set_id: usize) -> Option<&SwapSetItem> {
        let ss = self.swap_sets.iter().find(|ss| ss.id == swap_set_id)?;
        ss.items.iter().find(|i| i.id == ss.active_item_id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::LayerKind;

    fn app() -> App {
        App::new()
    }

    fn layer(a: &mut App) -> usize {
        a.apply(Action::AddLayer { name: "L".to_string(), kind: LayerKind::Bitmap });
        a.layers.last().unwrap().id
    }

    #[test]
    fn test_create_swap_set() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        assert_eq!(a.swap_sets.len(), 1);
        assert_eq!(a.swap_sets[0].name, "Mouth");
        assert_eq!(a.swap_sets[0].layer_id, lid);
    }

    #[test]
    fn test_create_swap_set_id_increments() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "A".to_string() });
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "B".to_string() });
        assert_ne!(a.swap_sets[0].id, a.swap_sets[1].id);
    }

    #[test]
    fn test_add_swap_item() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Open".to_string(), symbol_id: None });
        assert_eq!(a.swap_sets[0].items.len(), 1);
        assert_eq!(a.swap_sets[0].items[0].name, "Open");
    }

    #[test]
    fn test_first_swap_item_auto_activates() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Eyes".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Open".to_string(), symbol_id: None });
        let item_id = a.swap_sets[0].items[0].id;
        assert_eq!(a.swap_sets[0].active_item_id, item_id);
    }

    #[test]
    fn test_activate_swap_item() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Open".to_string(), symbol_id: None });
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Closed".to_string(), symbol_id: None });
        let closed_id = a.swap_sets[0].items[1].id;
        a.apply(Action::ActivateSwapItem { swap_set_id: ss_id, item_id: closed_id });
        assert_eq!(a.swap_sets[0].active_item_id, closed_id);
    }

    #[test]
    fn test_activate_nonexistent_item_noop() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "M".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Open".to_string(), symbol_id: None });
        let original = a.swap_sets[0].active_item_id;
        a.apply(Action::ActivateSwapItem { swap_set_id: ss_id, item_id: 9999 });
        assert_eq!(a.swap_sets[0].active_item_id, original);
    }

    #[test]
    fn test_remove_swap_item() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Open".to_string(), symbol_id: None });
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Closed".to_string(), symbol_id: None });
        let item_id = a.swap_sets[0].items[0].id;
        a.apply(Action::RemoveSwapItem { swap_set_id: ss_id, item_id });
        assert_eq!(a.swap_sets[0].items.len(), 1);
    }

    #[test]
    fn test_remove_active_item_switches_active() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "A".to_string(), symbol_id: None });
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "B".to_string(), symbol_id: None });
        let first_id = a.swap_sets[0].items[0].id;
        let second_id = a.swap_sets[0].items[1].id;
        // Active is first item
        a.apply(Action::RemoveSwapItem { swap_set_id: ss_id, item_id: first_id });
        // Should switch to next remaining item
        assert_eq!(a.swap_sets[0].active_item_id, second_id);
    }

    #[test]
    fn test_delete_swap_set() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::DeleteSwapSet { swap_set_id: ss_id });
        assert!(a.swap_sets.is_empty());
    }

    #[test]
    fn test_active_swap_item_helper() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Mouth".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Open".to_string(), symbol_id: Some(5) });
        let item = a.active_swap_item(ss_id);
        assert!(item.is_some());
        assert_eq!(item.unwrap().name, "Open");
        assert_eq!(item.unwrap().symbol_id, Some(5));
    }

    #[test]
    fn test_swap_item_with_symbol_id() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::CreateSwapSet { layer_id: lid, name: "Brows".to_string() });
        let ss_id = a.swap_sets[0].id;
        a.apply(Action::AddSwapItem { swap_set_id: ss_id, name: "Raised".to_string(), symbol_id: Some(3) });
        assert_eq!(a.swap_sets[0].items[0].symbol_id, Some(3));
    }
}
