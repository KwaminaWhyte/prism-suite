use super::{App, Action};

/// The influence a single bone has on a layer.
#[derive(Clone, Debug)]
pub struct BoneInfluence {
    pub bone_id: usize,
    /// Weight in the range 0.0..=1.0.
    pub weight: f32,
}

/// All bone weights for a single layer.
#[derive(Clone, Debug)]
pub struct LayerBoneWeights {
    pub layer_id: usize,
    /// Sum is unconstrained here; use `NormalizeBoneWeights` to bring it to 1.0.
    pub influences: Vec<BoneInfluence>,
}

impl App {
    pub fn apply_bone_weights(&mut self, action: Action) {
        match action {
            Action::SetBoneWeight { layer_id, bone_id, weight } => {
                let weight = weight.clamp(0.0, 1.0);
                let entry = self
                    .bone_weights
                    .entry(layer_id)
                    .or_insert_with(|| LayerBoneWeights {
                        layer_id,
                        influences: Vec::new(),
                    });
                if let Some(inf) = entry.influences.iter_mut().find(|i| i.bone_id == bone_id) {
                    inf.weight = weight;
                } else {
                    entry.influences.push(BoneInfluence { bone_id, weight });
                }
            }
            Action::RemoveBoneWeight { layer_id, bone_id } => {
                if let Some(lbw) = self.bone_weights.get_mut(&layer_id) {
                    lbw.influences.retain(|i| i.bone_id != bone_id);
                }
            }
            Action::ClearBoneWeights { layer_id } => {
                self.bone_weights.remove(&layer_id);
            }
            Action::NormalizeBoneWeights { layer_id } => {
                if let Some(lbw) = self.bone_weights.get_mut(&layer_id) {
                    let total: f32 = lbw.influences.iter().map(|i| i.weight).sum();
                    if total > 1e-9 {
                        for inf in &mut lbw.influences {
                            inf.weight /= total;
                        }
                    }
                }
            }
            Action::SetWeightPaintingActive(active) => {
                self.weight_painting_active = active;
            }
            Action::SetActiveWeightBone { bone_id } => {
                self.active_weight_bone = bone_id;
            }
            Action::SetWeightBrushRadius(radius) => {
                self.weight_brush_radius = radius.max(0.0);
            }
            _ => {}
        }
    }

    /// Returns the effective weight for `bone_id` on `layer_id`, or 0.0 if
    /// no weight has been set.
    pub fn effective_bone_weight(&self, layer_id: usize, bone_id: usize) -> f32 {
        self.bone_weights
            .get(&layer_id)
            .and_then(|lbw| lbw.influences.iter().find(|i| i.bone_id == bone_id))
            .map(|i| i.weight)
            .unwrap_or(0.0)
    }

    /// Returns the sum of all bone weights on a layer.
    pub fn total_weight_for_layer(&self, layer_id: usize) -> f32 {
        self.bone_weights
            .get(&layer_id)
            .map(|lbw| lbw.influences.iter().map(|i| i.weight).sum())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action, LayerKind};

    fn app() -> App {
        App::new()
    }

    fn layer(a: &mut App) -> usize {
        a.apply(Action::AddLayer {
            name: "L".to_string(),
            kind: LayerKind::Bitmap,
        });
        a.layers.last().unwrap().id
    }

    #[test]
    fn test_set_bone_weight() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.8 });
        assert_eq!(a.effective_bone_weight(lid, 0), 0.8);
    }

    #[test]
    fn test_set_bone_weight_clamps() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 2.5 });
        assert_eq!(a.effective_bone_weight(lid, 0), 1.0, "weight clamped to 1.0");
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 1, weight: -0.5 });
        assert_eq!(a.effective_bone_weight(lid, 1), 0.0, "weight clamped to 0.0");
    }

    #[test]
    fn test_set_bone_weight_updates_existing() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.3 });
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.7 });
        assert_eq!(a.effective_bone_weight(lid, 0), 0.7);
        // Still only one entry
        assert_eq!(a.bone_weights[&lid].influences.len(), 1);
    }

    #[test]
    fn test_effective_bone_weight_default_zero() {
        let a = app();
        assert_eq!(a.effective_bone_weight(99, 0), 0.0);
    }

    #[test]
    fn test_remove_bone_weight() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.5 });
        a.apply(Action::RemoveBoneWeight { layer_id: lid, bone_id: 0 });
        assert_eq!(a.effective_bone_weight(lid, 0), 0.0);
    }

    #[test]
    fn test_clear_bone_weights() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.4 });
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 1, weight: 0.3 });
        a.apply(Action::ClearBoneWeights { layer_id: lid });
        assert!(!a.bone_weights.contains_key(&lid));
        assert_eq!(a.total_weight_for_layer(lid), 0.0);
    }

    #[test]
    fn test_total_weight_for_layer() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.4 });
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 1, weight: 0.3 });
        let total = a.total_weight_for_layer(lid);
        assert!((total - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_bone_weights() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.4 });
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 1, weight: 0.6 });
        // Already sums to 1.0 — normalize is idempotent
        a.apply(Action::NormalizeBoneWeights { layer_id: lid });
        let total = a.total_weight_for_layer(lid);
        assert!((total - 1.0).abs() < 1e-5, "total should be ~1.0 after normalize");
    }

    #[test]
    fn test_normalize_bone_weights_scales_up() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 0, weight: 0.2 });
        a.apply(Action::SetBoneWeight { layer_id: lid, bone_id: 1, weight: 0.3 });
        // Total = 0.5 — after normalize each should be doubled
        a.apply(Action::NormalizeBoneWeights { layer_id: lid });
        let total = a.total_weight_for_layer(lid);
        assert!((total - 1.0).abs() < 1e-5);
        let w0 = a.effective_bone_weight(lid, 0);
        assert!((w0 - 0.4).abs() < 1e-5, "bone0 weight should be 0.4 after normalize");
    }

    #[test]
    fn test_set_weight_painting_active() {
        let mut a = app();
        assert!(!a.weight_painting_active);
        a.apply(Action::SetWeightPaintingActive(true));
        assert!(a.weight_painting_active);
        a.apply(Action::SetWeightPaintingActive(false));
        assert!(!a.weight_painting_active);
    }

    #[test]
    fn test_set_active_weight_bone() {
        let mut a = app();
        assert!(a.active_weight_bone.is_none());
        a.apply(Action::SetActiveWeightBone { bone_id: Some(3) });
        assert_eq!(a.active_weight_bone, Some(3));
        a.apply(Action::SetActiveWeightBone { bone_id: None });
        assert!(a.active_weight_bone.is_none());
    }

    #[test]
    fn test_set_weight_brush_radius() {
        let mut a = app();
        a.apply(Action::SetWeightBrushRadius(25.0));
        assert_eq!(a.weight_brush_radius, 25.0);
        a.apply(Action::SetWeightBrushRadius(-5.0));
        assert_eq!(a.weight_brush_radius, 0.0, "radius clamped to 0");
    }
}
