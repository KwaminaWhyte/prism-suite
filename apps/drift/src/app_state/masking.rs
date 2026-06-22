//! Masking domain: alpha/luma masks and clipping groups.

use super::{App, Action};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum MaskKind {
    Alpha,
    Luma,
    InvertAlpha,
    InvertLuma,
}

#[derive(Clone, Debug)]
pub struct LayerMask {
    pub id: usize,
    pub layer_id: usize,
    pub mask_source_id: usize,
    pub kind: MaskKind,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct ClippingGroup {
    pub id: usize,
    pub base_layer_id: usize,
    pub clipped_ids: Vec<usize>,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_masking(&mut self, action: Action) {
        match action {
            Action::AddLayerMask { layer_id, mask_source_id, kind } => {
                let id = self.next_mask_id;
                self.next_mask_id += 1;
                self.layer_masks.push(LayerMask {
                    id,
                    layer_id,
                    mask_source_id,
                    kind,
                    enabled: true,
                });
            }
            Action::RemoveLayerMask { mask_id } => {
                self.layer_masks.retain(|m| m.id != mask_id);
            }
            Action::SetMaskKind { mask_id, kind } => {
                if let Some(m) = self.layer_masks.iter_mut().find(|m| m.id == mask_id) {
                    m.kind = kind;
                }
            }
            Action::ToggleMask { mask_id } => {
                if let Some(m) = self.layer_masks.iter_mut().find(|m| m.id == mask_id) {
                    m.enabled = !m.enabled;
                }
            }
            Action::CreateClippingGroup { base_layer_id } => {
                let id = self.next_clip_group_id;
                self.next_clip_group_id += 1;
                self.clipping_groups.push(ClippingGroup {
                    id,
                    base_layer_id,
                    clipped_ids: vec![],
                });
            }
            Action::AddToClippingGroup { group_id, layer_id } => {
                if let Some(g) = self.clipping_groups.iter_mut().find(|g| g.id == group_id) {
                    if !g.clipped_ids.contains(&layer_id) {
                        g.clipped_ids.push(layer_id);
                    }
                }
            }
            Action::RemoveFromClippingGroup { group_id, layer_id } => {
                if let Some(g) = self.clipping_groups.iter_mut().find(|g| g.id == group_id) {
                    g.clipped_ids.retain(|id| *id != layer_id);
                }
            }
            Action::DissolveClippingGroup { group_id } => {
                self.clipping_groups.retain(|g| g.id != group_id);
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::MaskKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_layer_mask() {
        let mut a = app();
        a.apply(Action::AddLayerMask { layer_id: 1, mask_source_id: 2, kind: MaskKind::Alpha });
        assert_eq!(a.layer_masks.len(), 1);
        assert_eq!(a.layer_masks[0].kind, MaskKind::Alpha);
        assert!(a.layer_masks[0].enabled);
    }

    #[test]
    fn test_add_layer_mask_increments_id() {
        let mut a = app();
        a.apply(Action::AddLayerMask { layer_id: 1, mask_source_id: 2, kind: MaskKind::Luma });
        a.apply(Action::AddLayerMask { layer_id: 3, mask_source_id: 4, kind: MaskKind::InvertAlpha });
        assert_eq!(a.layer_masks[0].id, 1);
        assert_eq!(a.layer_masks[1].id, 2);
    }

    #[test]
    fn test_remove_layer_mask() {
        let mut a = app();
        a.apply(Action::AddLayerMask { layer_id: 1, mask_source_id: 2, kind: MaskKind::Alpha });
        let id = a.layer_masks[0].id;
        a.apply(Action::RemoveLayerMask { mask_id: id });
        assert!(a.layer_masks.is_empty());
    }

    #[test]
    fn test_set_mask_kind() {
        let mut a = app();
        a.apply(Action::AddLayerMask { layer_id: 1, mask_source_id: 2, kind: MaskKind::Alpha });
        let id = a.layer_masks[0].id;
        a.apply(Action::SetMaskKind { mask_id: id, kind: MaskKind::InvertLuma });
        assert_eq!(a.layer_masks[0].kind, MaskKind::InvertLuma);
    }

    #[test]
    fn test_toggle_mask() {
        let mut a = app();
        a.apply(Action::AddLayerMask { layer_id: 1, mask_source_id: 2, kind: MaskKind::Alpha });
        let id = a.layer_masks[0].id;
        a.apply(Action::ToggleMask { mask_id: id });
        assert!(!a.layer_masks[0].enabled);
        a.apply(Action::ToggleMask { mask_id: id });
        assert!(a.layer_masks[0].enabled);
    }

    #[test]
    fn test_create_clipping_group() {
        let mut a = app();
        a.apply(Action::CreateClippingGroup { base_layer_id: 10 });
        assert_eq!(a.clipping_groups.len(), 1);
        assert_eq!(a.clipping_groups[0].base_layer_id, 10);
        assert!(a.clipping_groups[0].clipped_ids.is_empty());
    }

    #[test]
    fn test_add_to_clipping_group() {
        let mut a = app();
        a.apply(Action::CreateClippingGroup { base_layer_id: 10 });
        let gid = a.clipping_groups[0].id;
        a.apply(Action::AddToClippingGroup { group_id: gid, layer_id: 20 });
        a.apply(Action::AddToClippingGroup { group_id: gid, layer_id: 21 });
        assert_eq!(a.clipping_groups[0].clipped_ids.len(), 2);
    }

    #[test]
    fn test_add_to_clipping_group_no_duplicates() {
        let mut a = app();
        a.apply(Action::CreateClippingGroup { base_layer_id: 10 });
        let gid = a.clipping_groups[0].id;
        a.apply(Action::AddToClippingGroup { group_id: gid, layer_id: 20 });
        a.apply(Action::AddToClippingGroup { group_id: gid, layer_id: 20 });
        assert_eq!(a.clipping_groups[0].clipped_ids.len(), 1);
    }

    #[test]
    fn test_remove_from_clipping_group() {
        let mut a = app();
        a.apply(Action::CreateClippingGroup { base_layer_id: 10 });
        let gid = a.clipping_groups[0].id;
        a.apply(Action::AddToClippingGroup { group_id: gid, layer_id: 20 });
        a.apply(Action::RemoveFromClippingGroup { group_id: gid, layer_id: 20 });
        assert!(a.clipping_groups[0].clipped_ids.is_empty());
    }

    #[test]
    fn test_dissolve_clipping_group() {
        let mut a = app();
        a.apply(Action::CreateClippingGroup { base_layer_id: 10 });
        let gid = a.clipping_groups[0].id;
        a.apply(Action::DissolveClippingGroup { group_id: gid });
        assert!(a.clipping_groups.is_empty());
    }
}
