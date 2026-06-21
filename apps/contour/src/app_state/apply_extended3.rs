use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    // --- Batch 9: Type on Path depth ---

    #[test]
    fn test_text_on_path_offset() {
        let mut app = App::new();
        app.apply(Action::SetTextOnPathOffset { text_id: 7, offset: 42.5 });
        assert_eq!(app.text_on_path_offsets.get(&7).copied(), Some(42.5));
    }

    #[test]
    fn test_text_on_path_flip() {
        let mut app = App::new();
        // Default (no entry) should be treated as `true` (above).
        app.apply(Action::FlipTextOnPath(3));
        assert_eq!(app.text_on_path_above.get(&3).copied(), Some(false), "flip from default true → false");
        app.apply(Action::FlipTextOnPath(3));
        assert_eq!(app.text_on_path_above.get(&3).copied(), Some(true), "flip back to true");
    }

    #[test]
    fn test_detach_text_from_path() {
        let mut app = App::new();
        // Seed a fake path attachment.
        app.text_on_path.insert(1, 0);
        app.text_on_path_offsets.insert(1, 10.0);
        app.apply(Action::DetachTextFromPath(1));
        assert!(app.text_on_path.get(&1).is_none(), "attachment removed");
        // Offset entry is left in place (detach does not clear depth state).
    }

    // --- Batch 9: Recolor Artwork depth ---

    #[test]
    fn test_recolor_color_count_clamp() {
        let mut app = App::new();
        // Values below 2 clamp to 2.
        app.apply(Action::SetRecolorColorCount(1));
        assert_eq!(app.recolor_color_count, 2, "clamped to minimum 2");
        // Values above 30 clamp to 30.
        app.apply(Action::SetRecolorColorCount(40));
        assert_eq!(app.recolor_color_count, 30, "clamped to maximum 30");
        // In-range values are kept as-is.
        app.apply(Action::SetRecolorColorCount(12));
        assert_eq!(app.recolor_color_count, 12);
    }

    #[test]
    fn test_recolor_preserve_flags() {
        let mut app = App::new();
        assert!(app.recolor_config.preserve_black, "default preserve_black is true");
        app.apply(Action::SetRecolorPreserveBlack(false));
        assert!(!app.recolor_config.preserve_black);
        app.apply(Action::SetRecolorPreserveWhite(false));
        assert!(!app.recolor_config.preserve_white);
    }

    #[test]
    fn test_save_recolor_set() {
        let mut app = App::new();
        // Select the first two shapes so we have fills to save.
        app.selection = vec![0, 1];
        let before = app.recolor_history.len();
        app.apply(Action::SaveRecolorSet);
        assert!(app.recolor_history.len() > before, "recolor set saved");
        assert!(!app.recolor_history.last().unwrap().is_empty(), "saved set is non-empty");
    }

    // --- Batch 9: Live Paint depth ---

    #[test]
    fn test_live_paint_gap_detection() {
        let mut app = App::new();
        assert!(!app.live_paint_gap_detection, "gap detection starts false");
        app.apply(Action::SetLivePaintGapDetection(true));
        assert!(app.live_paint_gap_detection);
        app.apply(Action::SetLivePaintGapDetection(false));
        assert!(!app.live_paint_gap_detection);
    }

    #[test]
    fn test_live_paint_highlight_color() {
        let mut app = App::new();
        let color = [0.0, 1.0, 0.5, 1.0];
        app.apply(Action::SetLivePaintHighlightColor(color));
        assert_eq!(app.live_paint_highlight_color, color);
    }

    #[test]
    fn test_make_live_paint_group() {
        let mut app = App::new();
        // Select the first two shapes.
        app.selection = vec![0, 1];
        let before = app.live_paint_group_ids.len();
        app.apply(Action::MakeLivePaintGroup);
        assert_eq!(app.live_paint_group_ids.len(), before + 1, "one group added");
        // Both shapes should have the new group id.
        let gid = *app.live_paint_group_ids.last().unwrap();
        assert_eq!(app.doc.shapes[0].group(), Some(gid));
        assert_eq!(app.doc.shapes[1].group(), Some(gid));
    }

    // --- Batch 9: Symbol Sprayer ---

    #[test]
    fn test_spray_density_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDensity(15.0));
        assert!((app.symbol_spray_config.density - 10.0).abs() < 0.01, "density clamped to 10");
        app.apply(Action::SetSymbolSprayDensity(-1.0));
        assert!((app.symbol_spray_config.density - 0.0).abs() < 0.01, "density clamped to 0");
    }

    #[test]
    fn test_spray_diameter_min() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDiameter(0.0));
        assert!((app.symbol_spray_config.diameter - 1.0).abs() < 0.01, "diameter clamped to min 1.0");
        app.apply(Action::SetSymbolSprayDiameter(50.0));
        assert!((app.symbol_spray_config.diameter - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_spray_symbols_adds_shapes() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDensity(3.0));
        let before = app.doc.shapes.len();
        app.apply(Action::SpraySymbols { center: [0.0, 0.0], pressure: 1.0 });
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1, "SpraySymbols should add at least one shape, got {added}");
    }

    #[test]
    fn test_symbol_stain_changes_fill() {
        let mut app = App::new();
        // Add a shape at the origin so SymbolStain can reach it.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 20.0, 20.0], [1.0, 0.0, 0.0, 1.0], [0.0;4], 0.0));
        // Use a large diameter so the shape is within the brush.
        app.apply(Action::SetSymbolSprayDiameter(200.0));
        let idx = app.doc.shapes.len() - 1;
        let before_fill = app.doc.shapes[idx].fill_color().unwrap();
        let stain = [0.0, 0.0, 1.0, 1.0];
        app.apply(Action::SymbolStain { center: [10.0, 10.0], color: stain });
        let after_fill = app.doc.shapes[idx].fill_color().unwrap();
        // Fill should have changed toward the stain color.
        assert_ne!(before_fill, after_fill, "stain should change the fill color");
        // Blue channel should have increased.
        assert!(after_fill[2] > before_fill[2], "blue channel should increase toward stain");
    }

    // --- Batch 10: Gradient Mesh ---

    #[test]
    fn test_mesh_rows_cols_clamp() {
        let mut app = App::new();
        // Below minimum → clamp to 1.
        app.apply(Action::SetMeshRows(0));
        assert_eq!(app.gradient_mesh.rows, 1, "rows clamped to 1");
        // Above maximum → clamp to 50.
        app.apply(Action::SetMeshRows(100));
        assert_eq!(app.gradient_mesh.rows, 50, "rows clamped to 50");
        app.apply(Action::SetMeshCols(0));
        assert_eq!(app.gradient_mesh.cols, 1, "cols clamped to 1");
        app.apply(Action::SetMeshCols(100));
        assert_eq!(app.gradient_mesh.cols, 50, "cols clamped to 50");
    }

    #[test]
    fn test_create_mesh_fills_points() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(2));
        app.apply(Action::SetMeshCols(3));
        app.apply(Action::CreateMesh);
        assert_eq!(app.gradient_mesh.points.len(), 6, "2×3 mesh should have 6 points");
    }

    #[test]
    fn test_mesh_point_tension_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(1));
        app.apply(Action::SetMeshCols(1));
        app.apply(Action::CreateMesh);
        app.apply(Action::SetMeshPointTension { idx: 0, tension: 2.0 });
        assert!((app.gradient_mesh.points[0].tension - 1.0).abs() < 0.001, "tension clamped to 1.0");
        app.apply(Action::SetMeshPointTension { idx: 0, tension: -0.5 });
        assert!((app.gradient_mesh.points[0].tension - 0.0).abs() < 0.001, "tension clamped to 0.0");
    }

    #[test]
    fn test_release_mesh_clears() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(3));
        app.apply(Action::SetMeshCols(3));
        app.apply(Action::CreateMesh);
        assert!(!app.gradient_mesh.points.is_empty());
        app.apply(Action::ReleaseMesh);
        assert!(app.gradient_mesh.points.is_empty(), "ReleaseMesh should clear all points");
        assert_eq!(app.gradient_mesh.rows, 4, "rows reset to default");
        assert_eq!(app.gradient_mesh.cols, 4, "cols reset to default");
    }

    // --- Batch 10: Flare Tool ---

    #[test]
    fn test_flare_brightness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetFlareBrightness(150.0));
        assert!((app.flare_config.brightness - 100.0).abs() < 0.001, "brightness clamped to 100");
        app.apply(Action::SetFlareBrightness(-10.0));
        assert!((app.flare_config.brightness - 0.0).abs() < 0.001, "brightness clamped to 0");
    }

    #[test]
    fn test_flare_ray_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetFlareRayCount(255));
        assert_eq!(app.flare_config.ray_count, 250, "ray_count clamped to 250");
    }

    #[test]
    fn test_place_flare_adds_to_list() {
        let mut app = App::new();
        let before = app.flare_shapes.len();
        app.apply(Action::PlaceFlare([100.0, 200.0]));
        assert_eq!(app.flare_shapes.len(), before + 1, "PlaceFlare should add to flare_shapes");
        assert_eq!(app.flare_config.center, [100.0, 200.0]);
    }

    #[test]
    fn test_flare_toggle() {
        let mut app = App::new();
        assert!(!app.flare_tool_active);
        app.apply(Action::ToggleFlareTool);
        assert!(app.flare_tool_active);
        app.apply(Action::ToggleFlareTool);
        assert!(!app.flare_tool_active);
    }

    // --- Batch 10: Pattern Brush ---

    #[test]
    fn test_pattern_brush_scale_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPatternBrushScale(2000.0));
        assert!((app.pattern_brush_config.scale - 1000.0).abs() < 0.001, "scale clamped to 1000");
        app.apply(Action::SetPatternBrushScale(-10.0));
        assert!((app.pattern_brush_config.scale - 0.0).abs() < 0.001, "scale clamped to 0");
    }

    #[test]
    fn test_save_pattern_brush_library() {
        let mut app = App::new();
        app.apply(Action::SavePatternBrush { name: "Dots".to_string() });
        app.apply(Action::SavePatternBrush { name: "Waves".to_string() });
        assert_eq!(app.pattern_brush_library.len(), 2, "library should contain 2 entries");
    }

    #[test]
    fn test_delete_pattern_brush_oob() {
        let mut app = App::new();
        // Delete on empty library: no panic.
        app.apply(Action::DeletePatternBrush(99));
        assert!(app.pattern_brush_library.is_empty());
        // Add one and delete a valid index.
        app.apply(Action::SavePatternBrush { name: "X".to_string() });
        app.apply(Action::DeletePatternBrush(0));
        assert!(app.pattern_brush_library.is_empty());
    }

    // --- Batch 10: Variable Fonts ---

    #[test]
    fn test_font_axis_value_clamped_to_range() {
        let mut app = App::new();
        app.apply(Action::AddFontAxis(FontAxis { tag: "wght".to_string(), min: 100.0, max: 900.0, value: 400.0 }));
        app.apply(Action::SetFontAxisValue { idx: 0, value: 5000.0 });
        assert!((app.variable_font_config.axes[0].value - 900.0).abs() < 0.001, "value clamped to max 900");
        app.apply(Action::SetFontAxisValue { idx: 0, value: 5.0 });
        assert!((app.variable_font_config.axes[0].value - 100.0).abs() < 0.001, "value clamped to min 100");
    }

    #[test]
    fn test_remove_font_axis_oob_no_panic() {
        let mut app = App::new();
        // Removing from empty list should not panic.
        app.apply(Action::RemoveFontAxis(99));
        assert!(app.variable_font_config.axes.is_empty());
    }

    #[test]
    fn test_reset_font_axes_midpoint() {
        let mut app = App::new();
        app.apply(Action::AddFontAxis(FontAxis { tag: "wdth".to_string(), min: 0.0, max: 100.0, value: 75.0 }));
        app.apply(Action::ResetFontAxes);
        assert!((app.variable_font_config.axes[0].value - 50.0).abs() < 0.001, "reset to midpoint (min+max)/2 = 50");
    }

    #[test]
    fn test_variable_font_panel_toggle() {
        let mut app = App::new();
        assert!(!app.variable_font_panel_open);
        app.apply(Action::ToggleVariableFontPanel);
        assert!(app.variable_font_panel_open);
        app.apply(Action::ToggleVariableFontPanel);
        assert!(!app.variable_font_panel_open);
    }

    // --- Batch 11: Pathfinder depth ---

    #[test]
    fn test_pathfinder_op_recorded() {
        let mut app = App::new();
        assert!(app.last_pathfinder_op.is_none());
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Unite));
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Unite));
    }

    #[test]
    fn test_pathfinder_precision_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPathfinderPrecision(0.0));
        assert!((app.pathfinder_precision - 0.001).abs() < 1e-6, "should clamp to 0.001");
        app.apply(Action::SetPathfinderPrecision(100.0));
        assert!((app.pathfinder_precision - 10.0).abs() < 1e-6, "should clamp to 10.0");
    }

    #[test]
    fn test_repeat_pathfinder_no_panic_when_none() {
        let mut app = App::new();
        // RepeatPathfinder with no prior op should not panic.
        app.apply(Action::RepeatPathfinder);
        assert!(app.last_pathfinder_op.is_none());
    }

    #[test]
    fn test_repeat_pathfinder_reapplies() {
        let mut app = App::new();
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Minus));
        app.apply(Action::RepeatPathfinder);
        // last_pathfinder_op unchanged — still Minus
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Minus));
    }

    // --- Batch 11: 3D Extrude depth ---

    #[test]
    fn test_extrude_depth_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudeDepth(5000.0));
        assert!((app.extrude_config.depth - 2000.0).abs() < 1e-6, "depth should clamp to 2000");
    }

    #[test]
    fn test_extrude_perspective_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudePerspective(200.0));
        assert!((app.extrude_config.perspective - 160.0).abs() < 1e-6, "perspective should clamp to 160");
    }

    #[test]
    fn test_extrude_rotation_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudeRotation { x: -300.0, y: 0.0, z: 0.0 });
        assert!((app.extrude_config.rotation_x - (-180.0)).abs() < 1e-6, "x rotation should clamp to -180");
    }

    #[test]
    fn test_extrude_panel_toggle() {
        let mut app = App::new();
        assert!(!app.extrude_panel_open);
        app.apply(Action::ToggleExtrudePanel);
        assert!(app.extrude_panel_open);
        app.apply(Action::ToggleExtrudePanel);
        assert!(!app.extrude_panel_open);
    }

    // --- Batch 11: Chart depth ---

    #[test]
    fn test_chart_add_dataset() {
        let mut app = App::new();
        assert!(app.chart_config.datasets.is_empty());
        app.apply(Action::AddChartDataSet(ChartDataSet::default()));
        assert_eq!(app.chart_config.datasets.len(), 1);
        assert_eq!(app.chart_config.datasets[0].label, "Series 1");
    }

    #[test]
    fn test_chart_remove_dataset_oob_no_panic() {
        let mut app = App::new();
        // Removing from an empty list should not panic.
        app.apply(Action::RemoveChartDataSet(99));
        assert!(app.chart_config.datasets.is_empty());
    }

    #[test]
    fn test_chart_column_width_clamp() {
        let mut app = App::new();
        app.apply(Action::SetChartColumnWidth(5.0));
        assert!((app.chart_config.column_width - 20.0).abs() < 1e-6, "column_width should clamp to 20");
        app.apply(Action::SetChartColumnWidth(200.0));
        assert!((app.chart_config.column_width - 100.0).abs() < 1e-6, "column_width should clamp to 100");
    }

    #[test]
    fn test_chart_category_labels() {
        let mut app = App::new();
        let labels = vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()];
        app.apply(Action::SetChartCategoryLabels(labels.clone()));
        assert_eq!(app.chart_config.category_labels, labels);
    }

    // --- Batch 11: Envelope Distort depth ---

    #[test]
    fn test_envelope_bend_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEnvelopeBend(200.0));
        assert!((app.envelope_config.bend - 100.0).abs() < 1e-6, "bend should clamp to 100");
        app.apply(Action::SetEnvelopeBend(-200.0));
        assert!((app.envelope_config.bend - (-100.0)).abs() < 1e-6, "bend should clamp to -100");
    }

    #[test]
    fn test_envelope_fidelity_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEnvelopeFidelity(200.0));
        assert!((app.envelope_config.fidelity - 100.0).abs() < 1e-6, "fidelity should clamp to 100");
    }

    #[test]
    fn test_make_envelope_with_warp_no_panic_no_selection() {
        let mut app = App::new();
        // No shape selected — should not panic, applied list stays empty.
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(app.envelope_applied_shapes.is_empty());
    }

    #[test]
    fn test_release_envelope_clears() {
        let mut app = App::new();
        // Select first shape then apply an envelope.
        app.apply(Action::SelectShape(0));
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(!app.envelope_applied_shapes.is_empty());
        app.apply(Action::ReleaseEnvelopeAll);
        assert!(app.envelope_applied_shapes.is_empty());
    }
}

