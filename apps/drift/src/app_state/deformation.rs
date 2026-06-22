//! Deformation domain: puppet pins, deform layers, stretch/squash.

use super::{App, Action};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum DeformKind {
    Bend,
    Skew,
    Warp,
    PuppetPin,
}

#[derive(Clone, Debug)]
pub struct PinAnchor {
    pub id: usize,
    pub layer_id: usize,
    pub x: f32,
    pub y: f32,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct DeformLayer {
    pub id: usize,
    pub layer_id: usize,
    pub kind: DeformKind,
    pub strength: f32,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct StretchSquash {
    pub layer_id: usize,
    pub stretch: f32,
    pub squash: f32,
    pub enabled: bool,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_deformation(&mut self, action: Action) {
        match action {
            Action::AddPinAnchor { layer_id, x, y } => {
                let id = self.next_anchor_id;
                self.next_anchor_id += 1;
                self.pin_anchors.push(PinAnchor {
                    id,
                    layer_id,
                    x,
                    y,
                    locked: false,
                });
            }
            Action::RemovePinAnchor { anchor_id } => {
                self.pin_anchors.retain(|a| a.id != anchor_id);
            }
            Action::MovePinAnchor { anchor_id, x, y } => {
                if let Some(a) = self.pin_anchors.iter_mut().find(|a| a.id == anchor_id) {
                    a.x = x;
                    a.y = y;
                }
            }
            Action::LockPinAnchor { anchor_id, locked } => {
                if let Some(a) = self.pin_anchors.iter_mut().find(|a| a.id == anchor_id) {
                    a.locked = locked;
                }
            }
            Action::AddDeformLayer { layer_id, kind } => {
                let id = self.next_deform_id;
                self.next_deform_id += 1;
                self.deform_layers.push(DeformLayer {
                    id,
                    layer_id,
                    kind,
                    strength: 1.0,
                    enabled: true,
                });
            }
            Action::RemoveDeformLayer { deform_id } => {
                self.deform_layers.retain(|d| d.id != deform_id);
            }
            Action::SetDeformStrength { deform_id, strength } => {
                if let Some(d) = self.deform_layers.iter_mut().find(|d| d.id == deform_id) {
                    d.strength = strength.clamp(0.0, 10.0);
                }
            }
            Action::ToggleDeform { deform_id } => {
                if let Some(d) = self.deform_layers.iter_mut().find(|d| d.id == deform_id) {
                    d.enabled = !d.enabled;
                }
            }
            Action::SetStretchSquash { layer_id, stretch, squash } => {
                if let Some(ss) = self.stretch_squash.iter_mut().find(|s| s.layer_id == layer_id) {
                    ss.stretch = stretch.max(0.01);
                    ss.squash = squash.max(0.01);
                } else {
                    self.stretch_squash.push(StretchSquash {
                        layer_id,
                        stretch: stretch.max(0.01),
                        squash: squash.max(0.01),
                        enabled: true,
                    });
                }
            }
            Action::ToggleStretchSquash { layer_id } => {
                if let Some(ss) = self.stretch_squash.iter_mut().find(|s| s.layer_id == layer_id) {
                    ss.enabled = !ss.enabled;
                }
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::DeformKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_pin_anchor() {
        let mut a = app();
        a.apply(Action::AddPinAnchor { layer_id: 1, x: 10.0, y: 20.0 });
        assert_eq!(a.pin_anchors.len(), 1);
        assert_eq!(a.pin_anchors[0].x, 10.0);
        assert_eq!(a.pin_anchors[0].y, 20.0);
        assert!(!a.pin_anchors[0].locked);
    }

    #[test]
    fn test_add_multiple_pin_anchors_increment_ids() {
        let mut a = app();
        a.apply(Action::AddPinAnchor { layer_id: 1, x: 0.0, y: 0.0 });
        a.apply(Action::AddPinAnchor { layer_id: 1, x: 5.0, y: 5.0 });
        assert_eq!(a.pin_anchors[0].id, 1);
        assert_eq!(a.pin_anchors[1].id, 2);
    }

    #[test]
    fn test_remove_pin_anchor() {
        let mut a = app();
        a.apply(Action::AddPinAnchor { layer_id: 1, x: 0.0, y: 0.0 });
        let id = a.pin_anchors[0].id;
        a.apply(Action::RemovePinAnchor { anchor_id: id });
        assert!(a.pin_anchors.is_empty());
    }

    #[test]
    fn test_move_pin_anchor() {
        let mut a = app();
        a.apply(Action::AddPinAnchor { layer_id: 1, x: 0.0, y: 0.0 });
        let id = a.pin_anchors[0].id;
        a.apply(Action::MovePinAnchor { anchor_id: id, x: 99.0, y: 88.0 });
        assert_eq!(a.pin_anchors[0].x, 99.0);
        assert_eq!(a.pin_anchors[0].y, 88.0);
    }

    #[test]
    fn test_lock_pin_anchor() {
        let mut a = app();
        a.apply(Action::AddPinAnchor { layer_id: 1, x: 0.0, y: 0.0 });
        let id = a.pin_anchors[0].id;
        a.apply(Action::LockPinAnchor { anchor_id: id, locked: true });
        assert!(a.pin_anchors[0].locked);
        a.apply(Action::LockPinAnchor { anchor_id: id, locked: false });
        assert!(!a.pin_anchors[0].locked);
    }

    #[test]
    fn test_add_deform_layer() {
        let mut a = app();
        a.apply(Action::AddDeformLayer { layer_id: 2, kind: DeformKind::Bend });
        assert_eq!(a.deform_layers.len(), 1);
        assert_eq!(a.deform_layers[0].kind, DeformKind::Bend);
        assert_eq!(a.deform_layers[0].strength, 1.0);
        assert!(a.deform_layers[0].enabled);
    }

    #[test]
    fn test_remove_deform_layer() {
        let mut a = app();
        a.apply(Action::AddDeformLayer { layer_id: 2, kind: DeformKind::Skew });
        let id = a.deform_layers[0].id;
        a.apply(Action::RemoveDeformLayer { deform_id: id });
        assert!(a.deform_layers.is_empty());
    }

    #[test]
    fn test_set_deform_strength_clamp() {
        let mut a = app();
        a.apply(Action::AddDeformLayer { layer_id: 2, kind: DeformKind::Warp });
        let id = a.deform_layers[0].id;
        a.apply(Action::SetDeformStrength { deform_id: id, strength: 15.0 });
        assert_eq!(a.deform_layers[0].strength, 10.0);
        a.apply(Action::SetDeformStrength { deform_id: id, strength: -1.0 });
        assert_eq!(a.deform_layers[0].strength, 0.0);
    }

    #[test]
    fn test_toggle_deform() {
        let mut a = app();
        a.apply(Action::AddDeformLayer { layer_id: 2, kind: DeformKind::PuppetPin });
        let id = a.deform_layers[0].id;
        a.apply(Action::ToggleDeform { deform_id: id });
        assert!(!a.deform_layers[0].enabled);
        a.apply(Action::ToggleDeform { deform_id: id });
        assert!(a.deform_layers[0].enabled);
    }

    #[test]
    fn test_set_stretch_squash_insert() {
        let mut a = app();
        a.apply(Action::SetStretchSquash { layer_id: 3, stretch: 1.5, squash: 0.5 });
        assert_eq!(a.stretch_squash.len(), 1);
        assert_eq!(a.stretch_squash[0].stretch, 1.5);
        assert_eq!(a.stretch_squash[0].squash, 0.5);
        assert!(a.stretch_squash[0].enabled);
    }

    #[test]
    fn test_set_stretch_squash_update() {
        let mut a = app();
        a.apply(Action::SetStretchSquash { layer_id: 3, stretch: 1.0, squash: 1.0 });
        a.apply(Action::SetStretchSquash { layer_id: 3, stretch: 2.0, squash: 0.8 });
        assert_eq!(a.stretch_squash.len(), 1);
        assert_eq!(a.stretch_squash[0].stretch, 2.0);
    }

    #[test]
    fn test_set_stretch_squash_min_clamp() {
        let mut a = app();
        a.apply(Action::SetStretchSquash { layer_id: 3, stretch: 0.0, squash: -1.0 });
        assert_eq!(a.stretch_squash[0].stretch, 0.01);
        assert_eq!(a.stretch_squash[0].squash, 0.01);
    }

    #[test]
    fn test_toggle_stretch_squash() {
        let mut a = app();
        a.apply(Action::SetStretchSquash { layer_id: 3, stretch: 1.0, squash: 1.0 });
        a.apply(Action::ToggleStretchSquash { layer_id: 3 });
        assert!(!a.stretch_squash[0].enabled);
        a.apply(Action::ToggleStretchSquash { layer_id: 3 });
        assert!(a.stretch_squash[0].enabled);
    }
}
