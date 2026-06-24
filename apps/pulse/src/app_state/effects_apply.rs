//! Layer-data effect actions: text animator, shape trim-paths / repeater,
//! Lumetri Color and Color Finesse.
//!
//! Split out of `effects_chain.rs` (which kept growing past the 1000-line
//! limit): `apply_effects_chain` handles the effect browser + the per-pixel
//! GPUI-effect stack arms and delegates these layer-data arms here via its `_`
//! arm. Both files share the same `App` impl; the effect enums/types live in
//! `effects_chain.rs`.

use super::*;

impl App {
    /// The text-animator / shape / Lumetri / Color-Finesse tail of
    /// [`App::apply_effects_chain`]. Reached via that match's `_` arm.
    pub(super) fn apply_effects_apply(&mut self, action: Action) {
        match action {
            Action::AddTextAnimator(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.text_animator = Some(crate::comp::TextAnimator::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveTextAnimator(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.text_animator = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetTextAnimatorRange { layer_id, start, end } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.range_start = start.clamp(0.0, 1.0);
                        ta.range_end = end.clamp(0.0, 1.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOffsetX { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.offset_x = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOffsetY { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.offset_y = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorRotation { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.rotation_deg = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorScale { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.scale = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOpacity { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.opacity = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetShapeTrimPaths { layer_id, start, end, offset } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.shape.trim_paths = Some(crate::comp::TrimPaths {
                        start: start.clamp(0.0, 1.0),
                        end: end.clamp(0.0, 1.0),
                        offset: offset.clamp(0.0, 1.0),
                    });
                    self.host.mark_dirty();
                }
            }
            Action::ClearShapeTrimPaths(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.trim_paths = None;
                    self.host.mark_dirty();
                }
            }
            Action::AddShapeRepeater(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.repeater = Some(crate::comp::ShapeRepeater::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveShapeRepeater(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.repeater = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetRepeaterCopies { layer_id, copies } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.copies = copies.max(1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterOffset { layer_id, x, y } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.offset_x = x;
                        r.offset_y = y;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterRotation { layer_id, deg } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.rotation_deg = deg;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterScale { layer_id, scale } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.scale = scale;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterOpacity { layer_id, start, end } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.opacity_start = start.clamp(0.0, 1.0);
                        r.opacity_end = end.clamp(0.0, 1.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AddLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.lumetri = Some(crate::comp::LumetriColor::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.lumetri = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetLumetriParam { layer_id, param, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(lc) = &mut l.lumetri {
                        match param {
                            "exposure"    => lc.exposure = value,
                            "contrast"    => lc.contrast = value,
                            "highlights"  => lc.highlights = value,
                            "shadows"     => lc.shadows = value,
                            "whites"      => lc.whites = value,
                            "blacks"      => lc.blacks = value,
                            "temperature" => lc.temperature = value,
                            "tint"        => lc.tint = value,
                            "saturation"  => lc.saturation = value,
                            "vibrance"    => lc.vibrance = value,
                            _             => {}
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleLumetriEnabled(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    if let Some(lc) = &mut l.lumetri {
                        lc.enabled = !lc.enabled;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ResetLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    if l.lumetri.is_some() {
                        l.lumetri = Some(crate::comp::LumetriColor::default());
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AddColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if l.color_finesse.is_none() {
                        l.color_finesse = Some(crate::comp::ColorFinesse::default());
                        self.host.mark_dirty();
                    }
                }
            }
            Action::RemoveColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.color_finesse = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetColorFinesseEnabled { layer_id, enabled } => {
                let ci = self.active_comp_index();
                if let Some(cf) = self.project.comps[ci]
                    .layers.get_mut(layer_id)
                    .and_then(|l| l.color_finesse.as_mut())
                {
                    cf.enabled = enabled;
                    self.host.mark_dirty();
                }
            }
            Action::SetColorFinesseParam { layer_id, range, prop, value } => {
                let ci = self.active_comp_index();
                if let Some(cf) = self.project.comps[ci]
                    .layers.get_mut(layer_id)
                    .and_then(|l| l.color_finesse.as_mut())
                {
                    let r = match range {
                        "master"   => &mut cf.master,
                        "reds"     => &mut cf.reds,
                        "yellows"  => &mut cf.yellows,
                        "greens"   => &mut cf.greens,
                        "cyans"    => &mut cf.cyans,
                        "blues"    => &mut cf.blues,
                        "magentas" => &mut cf.magentas,
                        _          => return,
                    };
                    match prop {
                        "hue"        => r.hue_shift = value,
                        "saturation" => r.saturation = value,
                        "lightness"  => r.lightness = value,
                        _            => return,
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ResetColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if l.color_finesse.is_some() {
                        l.color_finesse = Some(crate::comp::ColorFinesse::default());
                        self.host.mark_dirty();
                    }
                }
            }
            _ => unreachable!("apply_effects_apply called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Batch 6: Text Animator tests ---

    #[test]
    fn test_text_animator_add_remove() {
        let mut app = App::new();
        // Layer 0 exists in the default demo project.
        assert!(app.project.comps[app.active_comp_index()].layers[0].text_animator.is_none());
        app.apply(Action::AddTextAnimator(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].text_animator.is_some());
        assert!(app.can_undo());
        app.apply(Action::RemoveTextAnimator(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].text_animator.is_none());
    }

    #[test]
    fn test_text_animator_range() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimator(0));
        app.apply(Action::SetTextAnimatorRange { layer_id: 0, start: 0.2, end: 0.7 });
        let ci = app.active_comp_index();
        let ta = app.project.comps[ci].layers[0].text_animator.as_ref().unwrap();
        assert!((ta.range_start - 0.2).abs() < 1e-5);
        assert!((ta.range_end - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_text_animator_range_clamps() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimator(0));
        // Values outside [0,1] should be clamped.
        app.apply(Action::SetTextAnimatorRange { layer_id: 0, start: -0.5, end: 1.5 });
        let ci = app.active_comp_index();
        let ta = app.project.comps[ci].layers[0].text_animator.as_ref().unwrap();
        assert!((ta.range_start - 0.0).abs() < 1e-5);
        assert!((ta.range_end - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_text_animator_offsets() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimator(0));
        app.apply(Action::SetTextAnimatorOffsetX { layer_id: 0, value: 50.0 });
        app.apply(Action::SetTextAnimatorOffsetY { layer_id: 0, value: -20.0 });
        let ci = app.active_comp_index();
        let ta = app.project.comps[ci].layers[0].text_animator.as_ref().unwrap();
        assert!((ta.offset_x - 50.0).abs() < 1e-5);
        assert!((ta.offset_y - (-20.0)).abs() < 1e-5);
    }

    #[test]
    fn test_text_animator_default_scale_and_opacity() {
        let ta = crate::comp::TextAnimator::default();
        assert!((ta.scale - 1.0).abs() < 1e-5, "default scale = 1.0");
        assert!((ta.opacity - 1.0).abs() < 1e-5, "default opacity = 1.0");
    }

    // --- Batch 6: Trim Paths tests ---

    fn make_shape_layer_app() -> App {
        use crate::comp::{LayerKind, ShapeItem, ShapePrimitive};
        let mut app = App::new();
        // Replace layer 0 with a shape layer.
        let ci = app.active_comp_index();
        app.project.comps[ci].layers[0].kind = LayerKind::Shape;
        app.project.comps[ci].layers[0].shape.items.push(
            ShapeItem::new(ShapePrimitive::Rectangle { half_w: 50.0, half_h: 50.0, radius: 0.0 })
        );
        app
    }

    #[test]
    fn test_trim_paths_set() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: 0.1, end: 0.8, offset: 0.0 });
        let ci = app.active_comp_index();
        let tp = app.project.comps[ci].layers[0].shape.trim_paths.as_ref().unwrap();
        assert!((tp.start - 0.1).abs() < 1e-5);
        assert!((tp.end - 0.8).abs() < 1e-5);
    }

    #[test]
    fn test_trim_paths_clamp() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: -1.0, end: 2.0, offset: -0.5 });
        let ci = app.active_comp_index();
        let tp = app.project.comps[ci].layers[0].shape.trim_paths.as_ref().unwrap();
        assert!((tp.start - 0.0).abs() < 1e-5, "start clamped to 0");
        assert!((tp.end - 1.0).abs() < 1e-5, "end clamped to 1");
        assert!((tp.offset - 0.0).abs() < 1e-5, "offset clamped to 0");
    }

    #[test]
    fn test_trim_paths_clear() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: 0.0, end: 0.5, offset: 0.0 });
        app.apply(Action::ClearShapeTrimPaths(0));
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].shape.trim_paths.is_none());
    }

    #[test]
    fn test_trim_paths_undo() {
        let mut app = make_shape_layer_app();
        app.apply(Action::SetShapeTrimPaths { layer_id: 0, start: 0.2, end: 0.9, offset: 0.1 });
        assert!(app.can_undo());
        app.apply(Action::Undo);
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].shape.trim_paths.is_none());
    }

    // --- Batch 6: Shape Repeater tests ---

    #[test]
    fn test_repeater_add_remove() {
        let mut app = make_shape_layer_app();
        assert!(app.project.comps[app.active_comp_index()].layers[0].shape.repeater.is_none());
        app.apply(Action::AddShapeRepeater(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].shape.repeater.is_some());
        app.apply(Action::RemoveShapeRepeater(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].shape.repeater.is_none());
    }

    #[test]
    fn test_repeater_set_copies() {
        let mut app = make_shape_layer_app();
        app.apply(Action::AddShapeRepeater(0));
        app.apply(Action::SetRepeaterCopies { layer_id: 0, copies: 5 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].shape.repeater.as_ref().unwrap().copies, 5);
    }

    #[test]
    fn test_repeater_opacity_lerp() {
        use crate::comp::ShapeRepeater;
        let r = ShapeRepeater {
            copies: 3,
            opacity_start: 1.0,
            opacity_end: 0.0,
            ..ShapeRepeater::default()
        };
        assert!((r.copy_opacity(0) - 1.0).abs() < 1e-5, "first copy = opacity_start");
        assert!((r.copy_opacity(1) - 0.5).abs() < 1e-5, "middle copy = 0.5");
        assert!((r.copy_opacity(2) - 0.0).abs() < 1e-5, "last copy = opacity_end");
    }

    #[test]
    fn test_repeater_undo() {
        let mut app = make_shape_layer_app();
        app.apply(Action::AddShapeRepeater(0));
        assert!(app.can_undo());
        app.apply(Action::Undo);
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].shape.repeater.is_none());
    }

    // --- Batch 6: Lumetri Color tests ---

    #[test]
    fn test_lumetri_add_remove() {
        let mut app = App::new();
        assert!(app.project.comps[app.active_comp_index()].layers[0].lumetri.is_none());
        app.apply(Action::AddLumetriColor(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].lumetri.is_some());
        app.apply(Action::RemoveLumetriColor(0));
        assert!(app.project.comps[app.active_comp_index()].layers[0].lumetri.is_none());
    }

    #[test]
    fn test_lumetri_set_param() {
        let mut app = App::new();
        app.apply(Action::AddLumetriColor(0));
        app.apply(Action::SetLumetriParam { layer_id: 0, param: "exposure", value: 2.0 });
        let ci = app.active_comp_index();
        let lc = app.project.comps[ci].layers[0].lumetri.as_ref().unwrap();
        assert!((lc.exposure - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_lumetri_reset() {
        let mut app = App::new();
        app.apply(Action::AddLumetriColor(0));
        app.apply(Action::SetLumetriParam { layer_id: 0, param: "exposure", value: 3.0 });
        app.apply(Action::SetLumetriParam { layer_id: 0, param: "contrast", value: 50.0 });
        app.apply(Action::ResetLumetriColor(0));
        let ci = app.active_comp_index();
        let lc = app.project.comps[ci].layers[0].lumetri.as_ref().unwrap();
        assert!((lc.exposure - 0.0).abs() < 1e-5, "exposure reset");
        assert!((lc.contrast - 0.0).abs() < 1e-5, "contrast reset");
        assert!((lc.saturation - 100.0).abs() < 1e-5, "saturation default 100");
    }

    #[test]
    fn test_lumetri_enabled_toggle() {
        let mut app = App::new();
        app.apply(Action::AddLumetriColor(0));
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].lumetri.as_ref().unwrap().enabled);
        app.apply(Action::ToggleLumetriEnabled(0));
        assert!(!app.project.comps[app.active_comp_index()].layers[0].lumetri.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_lumetri_pixel_exposure() {
        use crate::comp::LumetriColor;
        let mut lc = LumetriColor::default();
        lc.exposure = 1.0; // +1 EV = ×2
        let pixel = [0.25, 0.25, 0.25, 1.0];
        let result = lc.apply(pixel);
        // After 1 EV exposure, 0.25 → 0.5 (before contrast/etc., which are 0).
        assert!((result[0] - 0.5).abs() < 0.01, "exposure doubles luminance, got {}", result[0]);
        assert!((result[3] - 1.0).abs() < 1e-5, "alpha unchanged");
    }

    // --- Batch 6: Essential Graphics / MoGrt tests ---

    #[test]
    fn test_mogrt_add_remove() {
        let mut app = App::new();
        assert!(app.mogrt_templates.is_empty());
        app.apply(Action::AddMoGrtTemplate("Lower Third".to_string()));
        assert_eq!(app.mogrt_templates.len(), 1);
        assert_eq!(app.mogrt_templates[0].name, "Lower Third");
        app.apply(Action::RemoveMoGrtTemplate(0));
        assert!(app.mogrt_templates.is_empty());
    }

    #[test]
    fn test_mogrt_add_control() {
        let mut app = App::new();
        app.apply(Action::AddMoGrtTemplate("Intro".to_string()));
        app.apply(Action::AddMoGrtControl {
            template_idx: 0,
            control: MoGrtControl::Text {
                label: "Title".to_string(),
                value: "My Video".to_string(),
            },
        });
        assert_eq!(app.mogrt_templates[0].controls.len(), 1);
        assert_eq!(app.mogrt_templates[0].controls[0].label(), "Title");
    }

    #[test]
    fn test_mogrt_set_text_value() {
        let mut app = App::new();
        app.apply(Action::AddMoGrtTemplate("T".to_string()));
        app.apply(Action::AddMoGrtControl {
            template_idx: 0,
            control: MoGrtControl::Text { label: "Name".to_string(), value: "Old".to_string() },
        });
        app.apply(Action::SetMoGrtTextValue {
            template_idx: 0,
            control_idx: 0,
            value: "New".to_string(),
        });
        if let MoGrtControl::Text { value, .. } = &app.mogrt_templates[0].controls[0] {
            assert_eq!(value, "New");
        } else {
            panic!("expected Text control");
        }
    }

    #[test]
    fn test_mogrt_set_color_value() {
        let mut app = App::new();
        app.apply(Action::AddMoGrtTemplate("T".to_string()));
        app.apply(Action::AddMoGrtControl {
            template_idx: 0,
            control: MoGrtControl::Color {
                label: "BG".to_string(),
                value: [0.0, 0.0, 0.0, 1.0],
            },
        });
        app.apply(Action::SetMoGrtColorValue {
            template_idx: 0,
            control_idx: 0,
            value: [1.0, 0.5, 0.0, 1.0],
        });
        if let MoGrtControl::Color { value, .. } = &app.mogrt_templates[0].controls[0] {
            assert!((value[0] - 1.0).abs() < 1e-5);
            assert!((value[1] - 0.5).abs() < 1e-5);
        } else {
            panic!("expected Color control");
        }
    }

    // ── Batch 2: Layer Split ─────────────────────────────────────────────────

    #[test]
    fn test_split_layer_creates_two() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let before = app.project.comps[ci].layers.len();
        app.apply(Action::SetTime(2.5));
        app.apply(Action::SplitLayer(0));
        let after = app.project.comps[ci].layers.len();
        assert_eq!(after, before + 1, "split must add one layer");
    }

    #[test]
    fn test_split_layer_timing() {
        let mut app = App::new();
        app.apply(Action::SetTime(2.0));
        app.apply(Action::SplitLayer(0));
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].out_point, Some(2.0));
        assert_eq!(app.project.comps[ci].layers[1].in_point, Some(2.0));
    }

    #[test]
    fn test_split_layer_preserves_content() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let orig_name = app.project.comps[ci].layers[0].name.clone();
        app.apply(Action::SetTime(1.0));
        app.apply(Action::SplitLayer(0));
        // Both layers must have the same name (the copy is a literal clone before timing edits).
        assert_eq!(app.project.comps[ci].layers[0].name, orig_name);
        assert_eq!(app.project.comps[ci].layers[1].name, orig_name);
    }

    #[test]
    fn test_split_layer_undo() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        let before = app.project.comps[ci].layers.len();
        app.apply(Action::SetTime(1.5));
        app.apply(Action::SplitLayer(0));
        assert_eq!(app.project.comps[ci].layers.len(), before + 1);
        app.apply(Action::Undo);
        assert_eq!(app.project.comps[ci].layers.len(), before, "undo must restore original count");
    }

    // ── Batch 2: Color Finesse ───────────────────────────────────────────────

    #[test]
    fn test_color_finesse_add_remove() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].color_finesse.is_none());
        app.apply(Action::AddColorFinesse(0));
        assert!(app.project.comps[ci].layers[0].color_finesse.is_some());
        app.apply(Action::RemoveColorFinesse(0));
        assert!(app.project.comps[ci].layers[0].color_finesse.is_none());
    }

    #[test]
    fn test_color_finesse_set_master_hue() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        app.apply(Action::SetColorFinesseParam {
            layer_id: 0,
            range: "master",
            prop: "hue",
            value: 90.0,
        });
        let cf = app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap();
        assert!((cf.master.hue_shift - 90.0).abs() < 1e-3);
    }

    #[test]
    fn test_color_finesse_set_range() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        app.apply(Action::SetColorFinesseParam {
            layer_id: 0,
            range: "reds",
            prop: "saturation",
            value: 50.0,
        });
        let cf = app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap();
        assert!((cf.reds.saturation - 50.0).abs() < 1e-3);
    }

    #[test]
    fn test_color_finesse_reset() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        app.apply(Action::SetColorFinesseParam { layer_id: 0, range: "master", prop: "hue", value: 45.0 });
        app.apply(Action::ResetColorFinesse(0));
        let cf = app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap();
        assert!((cf.master.hue_shift).abs() < 1e-3);
    }

    #[test]
    fn test_color_finesse_enabled_toggle() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.apply(Action::AddColorFinesse(0));
        assert!(app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap().enabled);
        app.apply(Action::SetColorFinesseEnabled { layer_id: 0, enabled: false });
        assert!(!app.project.comps[ci].layers[0].color_finesse.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_color_finesse_pixel_grade() {
        use crate::comp::ColorFinesse;
        // A green-hued pixel: R=0.0, G=1.0, B=0.0 (hue=120°, greens range).
        let cf = ColorFinesse {
            greens: crate::comp::ColorFinesseRange { hue_shift: 0.0, saturation: 100.0, lightness: 0.0 },
            enabled: true,
            ..ColorFinesse::default()
        };
        let out = cf.apply([0.0, 1.0, 0.0, 1.0]);
        // Saturation boosted; green should still be the dominant channel but fully saturated.
        assert!(out[1] > out[0], "green dominant after saturation boost");
    }

}
