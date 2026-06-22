//! Layer blend mode domain for Drift.

use super::{App, Action};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum LayerBlend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Add,
    Dissolve,
}

#[derive(Clone, Debug)]
pub struct LayerBlendConfig {
    pub layer_id: usize,
    pub blend: LayerBlend,
    pub fill_opacity: f32,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_blend_modes(&mut self, action: &Action) {
        match action {
            Action::SetLayerBlend { layer_id, blend } => {
                if let Some(cfg) = self.layer_blend_configs.iter_mut().find(|c| c.layer_id == *layer_id) {
                    cfg.blend = blend.clone();
                } else {
                    self.layer_blend_configs.push(LayerBlendConfig {
                        layer_id: *layer_id,
                        blend: blend.clone(),
                        fill_opacity: 1.0,
                    });
                }
            }
            Action::SetLayerFillOpacity { layer_id, opacity } => {
                if let Some(cfg) = self.layer_blend_configs.iter_mut().find(|c| c.layer_id == *layer_id) {
                    cfg.fill_opacity = opacity.clamp(0.0, 1.0);
                } else {
                    self.layer_blend_configs.push(LayerBlendConfig {
                        layer_id: *layer_id,
                        blend: LayerBlend::Normal,
                        fill_opacity: opacity.clamp(0.0, 1.0),
                    });
                }
            }
            Action::ResetLayerBlend { layer_id } => {
                self.layer_blend_configs.retain(|c| c.layer_id != *layer_id);
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::LayerBlend;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_set_layer_blend_creates_config() {
        let mut a = app();
        a.apply(Action::SetLayerBlend { layer_id: 1, blend: LayerBlend::Multiply });
        assert_eq!(a.layer_blend_configs.len(), 1);
        assert_eq!(a.layer_blend_configs[0].blend, LayerBlend::Multiply);
        assert_eq!(a.layer_blend_configs[0].layer_id, 1);
    }

    #[test]
    fn test_set_layer_blend_updates_existing() {
        let mut a = app();
        a.apply(Action::SetLayerBlend { layer_id: 1, blend: LayerBlend::Screen });
        a.apply(Action::SetLayerBlend { layer_id: 1, blend: LayerBlend::Overlay });
        assert_eq!(a.layer_blend_configs.len(), 1);
        assert_eq!(a.layer_blend_configs[0].blend, LayerBlend::Overlay);
    }

    #[test]
    fn test_set_fill_opacity_creates_config() {
        let mut a = app();
        a.apply(Action::SetLayerFillOpacity { layer_id: 2, opacity: 0.5 });
        assert_eq!(a.layer_blend_configs.len(), 1);
        assert!((a.layer_blend_configs[0].fill_opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_set_fill_opacity_clamps_low() {
        let mut a = app();
        a.apply(Action::SetLayerFillOpacity { layer_id: 3, opacity: -0.5 });
        assert!((a.layer_blend_configs[0].fill_opacity - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_set_fill_opacity_clamps_high() {
        let mut a = app();
        a.apply(Action::SetLayerFillOpacity { layer_id: 4, opacity: 1.5 });
        assert!((a.layer_blend_configs[0].fill_opacity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_set_fill_opacity_updates_existing() {
        let mut a = app();
        a.apply(Action::SetLayerFillOpacity { layer_id: 5, opacity: 0.8 });
        a.apply(Action::SetLayerFillOpacity { layer_id: 5, opacity: 0.3 });
        assert_eq!(a.layer_blend_configs.len(), 1);
        assert!((a.layer_blend_configs[0].fill_opacity - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_reset_layer_blend_removes_config() {
        let mut a = app();
        a.apply(Action::SetLayerBlend { layer_id: 6, blend: LayerBlend::Difference });
        a.apply(Action::ResetLayerBlend { layer_id: 6 });
        assert!(a.layer_blend_configs.is_empty());
    }

    #[test]
    fn test_reset_layer_blend_only_removes_target() {
        let mut a = app();
        a.apply(Action::SetLayerBlend { layer_id: 7, blend: LayerBlend::Add });
        a.apply(Action::SetLayerBlend { layer_id: 8, blend: LayerBlend::Dissolve });
        a.apply(Action::ResetLayerBlend { layer_id: 7 });
        assert_eq!(a.layer_blend_configs.len(), 1);
        assert_eq!(a.layer_blend_configs[0].layer_id, 8);
    }

    #[test]
    fn test_reset_nonexistent_layer_is_noop() {
        let mut a = app();
        // Should not panic
        a.apply(Action::ResetLayerBlend { layer_id: 999 });
        assert!(a.layer_blend_configs.is_empty());
    }
}
