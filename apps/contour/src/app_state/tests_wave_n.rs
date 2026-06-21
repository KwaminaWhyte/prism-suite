use super::*;

#[cfg(test)]
mod tests {
    use super::*;
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

