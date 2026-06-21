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

    // --- Wave N: Image Trace (extended) ---

    #[test]
    fn test_image_trace_mode_set() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceMode(ImageTraceMode::Logo));
        assert_eq!(app.image_trace_config.mode, ImageTraceMode::Logo);
    }

    #[test]
    fn test_image_trace_threshold_stored() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceThreshold(200));
        assert_eq!(app.image_trace_config.threshold, 200);
    }

    #[test]
    fn test_image_trace_colors_clamp() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceColors(1));
        assert_eq!(app.image_trace_config.colors, 2, "colors clamped to min 2");
        app.apply(Action::SetImageTraceColors(50));
        assert_eq!(app.image_trace_config.colors, 30, "colors clamped to max 30");
    }

    #[test]
    fn test_image_trace_paths_clamp() {
        let mut app = App::new();
        app.apply(Action::SetImageTracePaths(0));
        assert_eq!(app.image_trace_config.paths, 1, "paths clamped to min 1");
        app.apply(Action::SetImageTracePaths(200));
        assert_eq!(app.image_trace_config.paths, 100, "paths clamped to max 100");
    }

    #[test]
    fn test_run_image_trace_pushes_result() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceColors(6));
        app.apply(Action::RunImageTrace { image_id: 42 });
        assert_eq!(app.image_trace_results.len(), 1);
        let r = &app.image_trace_results[0];
        assert_eq!(r.source_image_id, 42);
        assert!(!r.expanded);
        // path_count = colors * 12 + noise = 6 * 12 + 25 = 97
        assert_eq!(r.path_count, 6 * 12 + 25);
    }

    #[test]
    fn test_expand_image_trace_result() {
        let mut app = App::new();
        app.apply(Action::RunImageTrace { image_id: 0 });
        assert!(!app.image_trace_results[0].expanded);
        app.apply(Action::ExpandImageTraceResult { result_index: 0 });
        assert!(app.image_trace_results[0].expanded);
    }

    // --- Wave N: Perspective Grid (extended config) ---

    #[test]
    fn test_perspective_grid_type_set() {
        let mut app = App::new();
        assert_eq!(app.perspective_grid_config.grid_type, PerspectiveGridType::TwoPoint);
        app.apply(Action::SetPerspectiveGridType(PerspectiveGridType::OnePoint));
        assert_eq!(app.perspective_grid_config.grid_type, PerspectiveGridType::OnePoint);
    }

    #[test]
    fn test_perspective_grid_config_toggle_visible() {
        let mut app = App::new();
        assert!(!app.perspective_grid_config.visible);
        app.apply(Action::TogglePerspectiveGridConfig);
        assert!(app.perspective_grid_config.visible);
        app.apply(Action::TogglePerspectiveGridConfig);
        assert!(!app.perspective_grid_config.visible);
    }

    #[test]
    fn test_perspective_grid_cell_size_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPerspectiveGridCellSize(0.0));
        assert!((app.perspective_grid_config.cell_size - 1.0).abs() < 1e-6, "cell_size clamped to 1");
        app.apply(Action::SetPerspectiveGridCellSize(9999.0));
        assert!((app.perspective_grid_config.cell_size - 500.0).abs() < 1e-6, "cell_size clamped to 500");
    }

    #[test]
    fn test_perspective_grid_active_plane() {
        let mut app = App::new();
        app.apply(Action::SetPerspectiveActivePlaneByName("floor".to_string()));
        assert!(!app.perspective_grid_config.left_plane.active);
        assert!(!app.perspective_grid_config.right_plane.active);
        assert!(app.perspective_grid_config.floor_plane.active);
    }

    #[test]
    fn test_move_vanishing_point() {
        let mut app = App::new();
        app.apply(Action::MoveVanishingPoint { which: "left".to_string(), x: -300.0, y: 50.0 });
        assert_eq!(app.perspective_grid_config.vanishing_point_left, (-300.0, 50.0));
        app.apply(Action::MoveVanishingPoint { which: "right".to_string(), x: 300.0, y: 50.0 });
        assert_eq!(app.perspective_grid_config.vanishing_point_right, (300.0, 50.0));
    }

    // --- Wave N: Global Swatches ---

    #[test]
    fn test_add_global_swatch() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "Crimson".to_string(), color: "#DC143C".to_string(), is_spot: false });
        assert_eq!(app.global_swatches.len(), 1);
        assert_eq!(app.global_swatches[0].name, "Crimson");
        assert_eq!(app.global_swatches[0].color, "#DC143C");
        assert!(app.global_swatches[0].is_global);
        assert_eq!(app.global_swatches[0].usage_count, 0);
    }

    #[test]
    fn test_edit_global_swatch() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "Blue".to_string(), color: "#0000FF".to_string(), is_spot: false });
        let id = app.global_swatches[0].id;
        app.apply(Action::EditGlobalSwatch { id, color: "#0033CC".to_string() });
        assert_eq!(app.global_swatches[0].color, "#0033CC");
    }

    #[test]
    fn test_delete_global_swatch() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "Red".to_string(), color: "#FF0000".to_string(), is_spot: false });
        let id = app.global_swatches[0].id;
        app.apply(Action::DeleteGlobalSwatch(id));
        assert!(app.global_swatches.is_empty());
    }

    #[test]
    fn test_create_swatch_group_and_add() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "A".to_string(), color: "#AAA".to_string(), is_spot: false });
        let sw_id = app.global_swatches[0].id;
        app.apply(Action::CreateSwatchGroup { name: "Warm".to_string() });
        let g_id = app.swatch_groups[0].id;
        app.apply(Action::AddSwatchToGroup { group_id: g_id, swatch_id: sw_id });
        // Adding again should be idempotent
        app.apply(Action::AddSwatchToGroup { group_id: g_id, swatch_id: sw_id });
        assert_eq!(app.swatch_groups[0].swatch_ids.len(), 1);
    }

    #[test]
    fn test_reorder_swatches() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "A".to_string(), color: "#111".to_string(), is_spot: false });
        app.apply(Action::AddGlobalSwatch { name: "B".to_string(), color: "#222".to_string(), is_spot: false });
        app.apply(Action::AddGlobalSwatch { name: "C".to_string(), color: "#333".to_string(), is_spot: false });
        let ids: Vec<usize> = app.global_swatches.iter().map(|s| s.id).collect();
        // Reverse the order
        let rev: Vec<usize> = ids.iter().rev().cloned().collect();
        app.apply(Action::ReorderSwatches(rev));
        assert_eq!(app.global_swatches[0].name, "C");
        assert_eq!(app.global_swatches[2].name, "A");
    }

    // --- Wave N: Artboards (extended) ---

    #[test]
    fn test_add_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 });
        assert_eq!(app.artboards_ex.len(), 1);
        assert_eq!(app.artboards_ex[0].width, 1920.0);
        assert_eq!(app.active_artboard_ex, Some(app.artboards_ex[0].id));
    }

    #[test]
    fn test_delete_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::DeleteArtboardEx(id));
        assert!(app.artboards_ex.is_empty());
        assert_eq!(app.active_artboard_ex, None);
    }

    #[test]
    fn test_rename_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::RenameArtboardEx { id, name: "Logo".to_string() });
        assert_eq!(app.artboards_ex[0].name, "Logo");
    }

    #[test]
    fn test_resize_artboard_clamp() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::ResizeArtboard { id, width: 0.0, height: 50000.0 });
        assert!((app.artboards_ex[0].width - 1.0).abs() < 1e-6, "width clamped to 1");
        assert!((app.artboards_ex[0].height - 32000.0).abs() < 1e-6, "height clamped to 32000");
    }

    #[test]
    fn test_duplicate_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 100.0, y: 50.0, width: 200.0, height: 150.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::DuplicateArtboardEx(id));
        assert_eq!(app.artboards_ex.len(), 2);
        let dup = &app.artboards_ex[1];
        assert_eq!(dup.x, 100.0 + 200.0 + 20.0, "x offset by width + 20");
        assert!(dup.name.starts_with("Copy of"));
        assert_eq!(app.active_artboard_ex, Some(dup.id));
    }

    #[test]
    fn test_reorder_artboards() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        app.apply(Action::AddArtboardEx { x: 200.0, y: 0.0, width: 100.0, height: 100.0 });
        let ids: Vec<usize> = app.artboards_ex.iter().map(|a| a.id).collect();
        let rev: Vec<usize> = ids.iter().rev().cloned().collect();
        app.apply(Action::ReorderArtboards(rev));
        assert_eq!(app.artboards_ex[0].x, 200.0, "second artboard now first");
    }
}
