// apps/pigment/src/app_state/tests_shapes.rs
#[cfg(test)]
mod tests {
    use crate::app_state::{App, Action};
    use crate::app_state::shapes::{
        BooleanOp, LineCap, LineJoin, PsdEncoding, SatinEffect, ColorOverlay,
        GradientOverlay, PatternOverlay, GradientOverlayStyle, StyleKind,
        ExtendedShapeKind,
    };
    use prism_core::LayerId;

    fn make_app() -> App { App::new() }

    // ---- A: Shape Primitives ----

    #[test]
    fn add_polygon_layer_creates_entry() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 6, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        assert!(!app.extended_shapes.is_empty());
    }

    #[test]
    fn polygon_min_sides_is_3() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 1, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let shape = app.extended_shapes.values().next().unwrap();
        if let ExtendedShapeKind::Polygon { sides, .. } = shape.shape {
            assert_eq!(sides, 3);
        } else { panic!("expected Polygon"); }
    }

    #[test]
    fn add_star_layer_creates_entry() {
        let mut app = make_app();
        app.apply(Action::AddStarLayer { points: 5, inner_radius: 20.0, outer_radius: 50.0, color: [0, 255, 0, 255] });
        assert!(!app.extended_shapes.is_empty());
        let shape = app.extended_shapes.values().next().unwrap();
        assert!(matches!(shape.shape, ExtendedShapeKind::Star { points: 5, .. }));
    }

    #[test]
    fn star_min_points_is_3() {
        let mut app = make_app();
        app.apply(Action::AddStarLayer { points: 2, inner_radius: 10.0, outer_radius: 30.0, color: [0, 0, 255, 255] });
        let shape = app.extended_shapes.values().next().unwrap();
        if let ExtendedShapeKind::Star { points, .. } = shape.shape {
            assert_eq!(points, 3);
        } else { panic!("expected Star"); }
    }

    #[test]
    fn add_line_layer_creates_entry() {
        let mut app = make_app();
        app.apply(Action::AddLineLayer { x1: 0.0, y1: 0.0, x2: 100.0, y2: 100.0, width: 3.0, color: [255, 255, 0, 255] });
        assert!(!app.extended_shapes.is_empty());
        let shape = app.extended_shapes.values().next().unwrap();
        assert!(matches!(shape.shape, ExtendedShapeKind::Line { .. }));
    }

    #[test]
    fn line_width_min_is_clamped() {
        let mut app = make_app();
        app.apply(Action::AddLineLayer { x1: 0.0, y1: 0.0, x2: 10.0, y2: 10.0, width: 0.0, color: [0, 0, 0, 255] });
        let shape = app.extended_shapes.values().next().unwrap();
        if let ExtendedShapeKind::Line { width, .. } = shape.shape {
            assert!(width >= 0.1);
        }
    }

    #[test]
    fn add_rounded_rect_layer_creates_entry() {
        let mut app = make_app();
        app.apply(Action::AddRoundedRectLayer { width: 200.0, height: 100.0, corner_radius: 12.0, color: [128, 0, 128, 255] });
        assert!(!app.extended_shapes.is_empty());
        let shape = app.extended_shapes.values().next().unwrap();
        assert!(matches!(shape.shape, ExtendedShapeKind::RoundedRect { .. }));
    }

    #[test]
    fn add_triangle_layer_creates_entry() {
        let mut app = make_app();
        app.apply(Action::AddTriangleLayer { base: 100.0, height: 80.0, rotation: 0.0, color: [255, 128, 0, 255] });
        assert!(!app.extended_shapes.is_empty());
        let shape = app.extended_shapes.values().next().unwrap();
        assert!(matches!(shape.shape, ExtendedShapeKind::Triangle { .. }));
    }

    #[test]
    fn set_shape_sides_updates_polygon() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 5, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetShapeSides { layer_id: id, sides: 8 });
        if let ExtendedShapeKind::Polygon { sides, .. } = app.extended_shapes[&id].shape {
            assert_eq!(sides, 8);
        }
    }

    #[test]
    fn set_shape_sides_min_3() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 6, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetShapeSides { layer_id: id, sides: 1 });
        if let ExtendedShapeKind::Polygon { sides, .. } = app.extended_shapes[&id].shape {
            assert_eq!(sides, 3);
        }
    }

    #[test]
    fn set_shape_corner_radius_rounded_rect() {
        let mut app = make_app();
        app.apply(Action::AddRoundedRectLayer { width: 100.0, height: 50.0, corner_radius: 0.0, color: [0, 0, 255, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetShapeCornerRadius { layer_id: id, radius: 16.0 });
        if let ExtendedShapeKind::RoundedRect { corner_radius, .. } = app.extended_shapes[&id].shape {
            assert_eq!(corner_radius, 16.0);
        }
    }

    #[test]
    fn set_star_points_updates() {
        let mut app = make_app();
        app.apply(Action::AddStarLayer { points: 5, inner_radius: 20.0, outer_radius: 50.0, color: [0, 255, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetStarPoints { layer_id: id, points: 7 });
        if let ExtendedShapeKind::Star { points, .. } = app.extended_shapes[&id].shape {
            assert_eq!(points, 7);
        }
    }

    #[test]
    fn set_star_inner_radius_updates() {
        let mut app = make_app();
        app.apply(Action::AddStarLayer { points: 5, inner_radius: 20.0, outer_radius: 50.0, color: [0, 0, 255, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetStarInnerRadius { layer_id: id, inner_radius: 30.0 });
        if let ExtendedShapeKind::Star { inner_radius, .. } = app.extended_shapes[&id].shape {
            assert_eq!(inner_radius, 30.0);
        }
    }

    #[test]
    fn set_line_cap_updates() {
        let mut app = make_app();
        app.apply(Action::AddLineLayer { x1: 0.0, y1: 0.0, x2: 50.0, y2: 50.0, width: 2.0, color: [0, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetLineCap { layer_id: id, cap: LineCap::Round });
        assert_eq!(app.extended_shapes[&id].line_cap, LineCap::Round);
    }

    #[test]
    fn set_line_join_updates() {
        let mut app = make_app();
        app.apply(Action::AddLineLayer { x1: 0.0, y1: 0.0, x2: 50.0, y2: 50.0, width: 2.0, color: [0, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetLineJoin { layer_id: id, join: LineJoin::Bevel });
        assert_eq!(app.extended_shapes[&id].line_join, LineJoin::Bevel);
    }

    #[test]
    fn set_shape_stroke_updates() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 6, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetShapeStroke { layer_id: id, color: [0, 255, 0, 255], width: 3.0 });
        let s = &app.extended_shapes[&id];
        assert_eq!(s.stroke_color, [0, 255, 0, 255]);
        assert_eq!(s.stroke_width, 3.0);
    }

    #[test]
    fn set_shape_fill_updates() {
        let mut app = make_app();
        app.apply(Action::AddRoundedRectLayer { width: 100.0, height: 50.0, corner_radius: 8.0, color: [255, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetShapeFill { layer_id: id, color: [0, 128, 255, 255] });
        assert_eq!(app.extended_shapes[&id].fill_color, [0, 128, 255, 255]);
    }

    // ---- B: Boolean Shape Operations ----

    #[test]
    fn boolean_op_with_zero_layers_is_noop() {
        let mut app = make_app();
        let initial_count = app.extended_shapes.len();
        app.apply_boolean_op(&[], BooleanOp::Unite);
        assert_eq!(app.extended_shapes.len(), initial_count);
    }

    #[test]
    fn boolean_op_with_one_layer_is_noop() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 6, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let initial_count = app.extended_shapes.len();
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply_boolean_op(&[id], BooleanOp::Unite);
        assert_eq!(app.extended_shapes.len(), initial_count);
    }

    #[test]
    fn boolean_op_with_two_layers_creates_result() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 5, radius: 40.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let id1 = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::AddStarLayer { points: 5, inner_radius: 20.0, outer_radius: 40.0, color: [0, 255, 0, 255] });
        let ids: Vec<usize> = app.extended_shapes.keys().cloned().collect();
        let count_before = ids.len();
        app.apply(Action::BooleanShapeOp { layer_ids: ids.clone(), op: BooleanOp::Unite });
        // Should create one more entry
        assert_eq!(app.extended_shapes.len(), count_before + 1);
        // Sources should be hidden
        assert!(app.extended_shapes[&id1].hidden_by_boolean);
    }

    #[test]
    fn boolean_subtract_creates_result() {
        let mut app = make_app();
        app.apply(Action::AddRoundedRectLayer { width: 100.0, height: 100.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        app.apply(Action::AddRoundedRectLayer { width: 50.0, height: 50.0, corner_radius: 0.0, color: [0, 255, 0, 255] });
        let ids: Vec<usize> = app.extended_shapes.keys().cloned().collect();
        app.apply(Action::BooleanShapeOp { layer_ids: ids.clone(), op: BooleanOp::Subtract });
        let result = app.extended_shapes.values().find(|s| {
            matches!(s.shape, ExtendedShapeKind::BooleanResult { op: BooleanOp::Subtract, .. })
        });
        assert!(result.is_some());
    }

    #[test]
    fn boolean_intersect_creates_result() {
        let mut app = make_app();
        app.apply(Action::AddTriangleLayer { base: 100.0, height: 100.0, rotation: 0.0, color: [255, 0, 0, 255] });
        app.apply(Action::AddTriangleLayer { base: 80.0, height: 80.0, rotation: 45.0, color: [0, 0, 255, 255] });
        let ids: Vec<usize> = app.extended_shapes.keys().cloned().collect();
        app.apply(Action::BooleanShapeOp { layer_ids: ids, op: BooleanOp::Intersect });
        let result = app.extended_shapes.values().find(|s| {
            matches!(s.shape, ExtendedShapeKind::BooleanResult { op: BooleanOp::Intersect, .. })
        });
        assert!(result.is_some());
    }

    #[test]
    fn boolean_exclude_creates_result() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 4, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        app.apply(Action::AddPolygonLayer { sides: 4, radius: 30.0, corner_radius: 0.0, color: [0, 0, 255, 255] });
        let ids: Vec<usize> = app.extended_shapes.keys().cloned().collect();
        app.apply(Action::BooleanShapeOp { layer_ids: ids, op: BooleanOp::Exclude });
        let result = app.extended_shapes.values().find(|s| {
            matches!(s.shape, ExtendedShapeKind::BooleanResult { op: BooleanOp::Exclude, .. })
        });
        assert!(result.is_some());
    }

    #[test]
    fn boolean_with_n_layers_hides_all_sources() {
        let mut app = make_app();
        for i in 0..3 {
            app.apply(Action::AddPolygonLayer { sides: 3 + i, radius: 40.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        }
        let ids: Vec<usize> = app.extended_shapes.keys().cloned().collect();
        app.apply(Action::BooleanShapeOp { layer_ids: ids.clone(), op: BooleanOp::Unite });
        for id in &ids {
            if !matches!(app.extended_shapes[id].shape, ExtendedShapeKind::BooleanResult { .. }) {
                assert!(app.extended_shapes[id].hidden_by_boolean);
            }
        }
    }

    #[test]
    fn expand_stroke_zeroes_stroke_width() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 5, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::SetShapeStroke { layer_id: id, color: [0, 0, 0, 255], width: 5.0 });
        app.apply(Action::ExpandStroke { layer_id: id });
        assert_eq!(app.extended_shapes[&id].stroke_width, 0.0);
    }

    #[test]
    fn flatten_to_pixels_removes_shape_entries() {
        let mut app = make_app();
        app.apply(Action::AddPolygonLayer { sides: 6, radius: 50.0, corner_radius: 0.0, color: [255, 0, 0, 255] });
        let id = *app.extended_shapes.keys().next().unwrap();
        app.apply(Action::FlattenToPixels { layer_ids: vec![id] });
        assert!(app.extended_shapes.is_empty());
    }

    // ---- C: Clipping Masks ----

    #[test]
    fn set_clipping_mask_true_inserts() {
        let mut app = make_app();
        let lid = LayerId(1);
        app.apply(Action::SetClippingMask { layer_id: lid, clipped: true });
        assert!(app.clipping_masks.contains(&lid));
    }

    #[test]
    fn set_clipping_mask_false_removes() {
        let mut app = make_app();
        let lid = LayerId(1);
        app.apply(Action::SetClippingMask { layer_id: lid, clipped: true });
        app.apply(Action::SetClippingMask { layer_id: lid, clipped: false });
        assert!(!app.clipping_masks.contains(&lid));
    }

    #[test]
    fn create_clipping_mask_adds() {
        let mut app = make_app();
        let lid = LayerId(2);
        app.apply(Action::CreateClippingMask { layer_id: lid });
        assert!(app.clipping_masks.contains(&lid));
    }

    #[test]
    fn release_clipping_mask_removes() {
        let mut app = make_app();
        let lid = LayerId(3);
        app.apply(Action::CreateClippingMask { layer_id: lid });
        app.apply(Action::ReleaseClippingMask { layer_id: lid });
        assert!(!app.clipping_masks.contains(&lid));
    }

    #[test]
    fn clipping_group_for_layer_empty_when_no_clipped() {
        let app = make_app();
        // Fresh app has 1 layer at LayerId(0)
        let group = app.clipping_group_for_layer(LayerId(0));
        assert!(group.is_empty());
    }

    #[test]
    fn clipping_group_for_nonexistent_layer_is_empty() {
        let app = make_app();
        let group = app.clipping_group_for_layer(LayerId(9999));
        assert!(group.is_empty());
    }

    #[test]
    fn clipping_group_detects_chain() {
        let mut app = make_app();
        // Add two more layers
        app.apply(Action::NewLayer); // LayerId(1)
        app.apply(Action::NewLayer); // LayerId(2)
        // Mark layer 1 and 2 as clipped
        app.apply(Action::CreateClippingMask { layer_id: LayerId(1) });
        app.apply(Action::CreateClippingMask { layer_id: LayerId(2) });
        // Group above LayerId(0) should contain 1 and 2
        let group = app.clipping_group_for_layer(LayerId(0));
        assert!(group.contains(&LayerId(1)));
        assert!(group.contains(&LayerId(2)));
    }

    #[test]
    fn release_breaks_clipping_chain() {
        let mut app = make_app();
        app.apply(Action::NewLayer);
        let lid = LayerId(1);
        app.apply(Action::CreateClippingMask { layer_id: lid });
        app.apply(Action::ReleaseClippingMask { layer_id: lid });
        let group = app.clipping_group_for_layer(LayerId(0));
        assert!(!group.contains(&lid));
    }

    // ---- D: Extended Layer Styles ----

    #[test]
    fn set_satin_effect_stores_config() {
        let mut app = make_app();
        let fx = SatinEffect { opacity: 75, distance: 20, ..Default::default() };
        app.apply(Action::SetSatinEffect { layer_id: 0, config: fx.clone() });
        let stored = app.satin_effects.get(&0).unwrap();
        assert_eq!(stored.opacity, 75);
        assert_eq!(stored.distance, 20);
    }

    #[test]
    fn set_color_overlay_stores_config() {
        let mut app = make_app();
        let fx = ColorOverlay { color: [255, 0, 255, 255], opacity: 80, ..Default::default() };
        app.apply(Action::SetExtendedColorOverlay { layer_id: 1, config: fx.clone() });
        let stored = app.color_overlays.get(&1).unwrap();
        assert_eq!(stored.color, [255, 0, 255, 255]);
        assert_eq!(stored.opacity, 80);
    }

    #[test]
    fn set_gradient_overlay_stores_config() {
        let mut app = make_app();
        let fx = GradientOverlay { angle: 45.0, scale: 120, style: GradientOverlayStyle::Radial, ..Default::default() };
        app.apply(Action::SetGradientOverlay { layer_id: 2, config: fx.clone() });
        let stored = app.gradient_overlays.get(&2).unwrap();
        assert_eq!(stored.angle, 45.0);
        assert_eq!(stored.scale, 120);
        assert_eq!(stored.style, GradientOverlayStyle::Radial);
    }

    #[test]
    fn set_pattern_overlay_stores_config() {
        let mut app = make_app();
        let fx = PatternOverlay { scale: 50, offset_x: 10.0, ..Default::default() };
        app.apply(Action::SetPatternOverlay { layer_id: 3, config: fx.clone() });
        let stored = app.pattern_overlays.get(&3).unwrap();
        assert_eq!(stored.scale, 50);
        assert_eq!(stored.offset_x, 10.0);
    }

    #[test]
    fn set_layer_style_opacity_clamps_to_100() {
        let mut app = make_app();
        app.apply(Action::SetSatinEffect { layer_id: 0, config: SatinEffect::default() });
        app.apply(Action::SetLayerStyleOpacity { layer_id: 0, style: StyleKind::Satin, opacity: 200 });
        assert_eq!(app.satin_effects[&0].opacity, 100);
    }

    #[test]
    fn set_layer_style_blend_mode_updates_satin() {
        let mut app = make_app();
        app.apply(Action::SetSatinEffect { layer_id: 0, config: SatinEffect::default() });
        app.apply(Action::SetLayerStyleBlendMode { layer_id: 0, style: StyleKind::Satin, blend_mode: "Screen".to_string() });
        assert_eq!(app.satin_effects[&0].blend_mode, "Screen");
    }

    #[test]
    fn copy_paste_layer_styles_ext() {
        let mut app = make_app();
        let fx_satin = SatinEffect { opacity: 60, ..Default::default() };
        let fx_color = ColorOverlay { opacity: 40, ..Default::default() };
        app.apply(Action::SetSatinEffect { layer_id: 0, config: fx_satin });
        app.apply(Action::SetExtendedColorOverlay { layer_id: 0, config: fx_color });
        app.apply(Action::CopyLayerStylesExt { from_layer_id: 0 });
        // Paste to layer 5
        app.apply(Action::PasteLayerStylesExt { to_layer_ids: vec![5] });
        assert_eq!(app.satin_effects[&5].opacity, 60);
        assert_eq!(app.color_overlays[&5].opacity, 40);
    }

    #[test]
    fn clear_layer_styles_ext_removes_all() {
        let mut app = make_app();
        app.apply(Action::SetSatinEffect { layer_id: 0, config: SatinEffect::default() });
        app.apply(Action::SetExtendedColorOverlay { layer_id: 0, config: ColorOverlay::default() });
        app.apply(Action::SetGradientOverlay { layer_id: 0, config: GradientOverlay::default() });
        app.apply(Action::SetPatternOverlay { layer_id: 0, config: PatternOverlay::default() });
        app.apply(Action::ClearLayerStylesExt { layer_id: 0 });
        assert!(app.satin_effects.get(&0).is_none());
        assert!(app.color_overlays.get(&0).is_none());
        assert!(app.gradient_overlays.get(&0).is_none());
        assert!(app.pattern_overlays.get(&0).is_none());
    }

    #[test]
    fn gradient_overlay_style_variants() {
        assert_ne!(GradientOverlayStyle::Linear, GradientOverlayStyle::Radial);
        assert_ne!(GradientOverlayStyle::Angle, GradientOverlayStyle::Diamond);
        assert_ne!(GradientOverlayStyle::Reflected, GradientOverlayStyle::Linear);
    }

    #[test]
    fn set_layer_style_blend_mode_color_overlay() {
        let mut app = make_app();
        app.apply(Action::SetExtendedColorOverlay { layer_id: 0, config: ColorOverlay::default() });
        app.apply(Action::SetLayerStyleBlendMode { layer_id: 0, style: StyleKind::ColorOverlay, blend_mode: "Multiply".to_string() });
        assert_eq!(app.color_overlays[&0].blend_mode, "Multiply");
    }

    #[test]
    fn set_layer_style_opacity_gradient_overlay() {
        let mut app = make_app();
        app.apply(Action::SetGradientOverlay { layer_id: 0, config: GradientOverlay::default() });
        app.apply(Action::SetLayerStyleOpacity { layer_id: 0, style: StyleKind::GradientOverlay, opacity: 50 });
        assert_eq!(app.gradient_overlays[&0].opacity, 50);
    }

    // ---- E: PSD Export Config ----

    #[test]
    fn psd_export_default_config() {
        let app = make_app();
        assert_eq!(app.psd_export_config.encoding, PsdEncoding::Rle);
        assert!(app.psd_export_config.maximize_compatibility);
        assert_eq!(app.psd_export_config.resolution, 72.0);
    }

    #[test]
    fn set_psd_export_path() {
        let mut app = make_app();
        app.apply(Action::SetPsdExportPath("/tmp/test.psd".to_string()));
        assert_eq!(app.psd_export_config.path, "/tmp/test.psd");
    }

    #[test]
    fn set_psd_maximize_compatibility() {
        let mut app = make_app();
        app.apply(Action::SetPsdMaximizeCompatibility(false));
        assert!(!app.psd_export_config.maximize_compatibility);
    }

    #[test]
    fn set_psd_encoding_raw() {
        let mut app = make_app();
        app.apply(Action::SetPsdEncoding(PsdEncoding::Raw));
        assert_eq!(app.psd_export_config.encoding, PsdEncoding::Raw);
    }

    #[test]
    fn set_psd_embed_color_profile() {
        let mut app = make_app();
        app.apply(Action::SetPsdEmbedColorProfile(false));
        assert!(!app.psd_export_config.embed_color_profile);
    }

    #[test]
    fn export_as_psd_records_path() {
        let mut app = make_app();
        app.apply(Action::ExportAsPsd { path: "/output/final.psd".to_string() });
        assert_eq!(app.last_psd_export_path.as_deref(), Some("/output/final.psd"));
        assert_eq!(app.psd_export_config.path, "/output/final.psd");
    }

    #[test]
    fn psd_encoding_rle_is_default() {
        let config = crate::app_state::shapes::PsdExportConfig::default();
        assert_eq!(config.encoding, PsdEncoding::Rle);
    }
}
