use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    // --- Batch 7: Art Brush ---

    #[test]
    fn test_art_brush_set_clear() {
        let mut app = App::new();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: false });
        assert!(app.art_brush.is_some());
        app.apply(Action::ClearArtBrush);
        assert!(app.art_brush.is_none());
    }

    #[test]
    fn test_art_brush_apply_produces_shape() {
        let mut app = App::new();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 2.0, colorize: ArtBrushColorize::None, flip: false });
        let before = app.doc.shapes.len();
        // Straight horizontal path long enough to deform.
        let path: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32 * 10.0, 0.0]).collect();
        app.apply(Action::PaintArtBrushPath { path });
        assert!(app.doc.shapes.len() > before, "art brush should add a shape");
    }

    #[test]
    fn test_art_brush_flip() {
        let mut app = App::new();
        // Two strokes: one normal, one flipped — both should produce shapes.
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: false });
        let path: Vec<[f32; 2]> = (0..=5).map(|i| [i as f32 * 20.0, 0.0]).collect();
        app.apply(Action::PaintArtBrushPath { path: path.clone() });
        let after_normal = app.doc.shapes.len();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: true });
        app.apply(Action::PaintArtBrushPath { path });
        assert!(app.doc.shapes.len() > after_normal, "flipped brush also produces a shape");
    }

    // --- Batch 7: Live Corners ---

    #[test]
    fn test_live_corner_polygon_set() {
        let mut app = App::new();
        // Create a live polygon and select it.
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape { tool: Tool::Polygon, a: [100.0, 100.0], b: [180.0, 180.0] });
        let idx = app.doc.shapes.len() - 1;
        app.selection = vec![idx];
        app.apply(Action::SetPolygonCornerRadius(12.0));
        if let Some(crate::liveshape::LiveShape::Polygon { corner_radius, .. }) = app.doc.shapes[idx].live_shape() {
            assert!((corner_radius - 12.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_live_corner_clamp() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape { tool: Tool::Polygon, a: [100.0, 100.0], b: [180.0, 180.0] });
        let idx = app.doc.shapes.len() - 1;
        app.selection = vec![idx];
        app.apply(Action::SetPolygonCornerRadius(-5.0));
        if let Some(crate::liveshape::LiveShape::Polygon { corner_radius, .. }) = app.doc.shapes[idx].live_shape() {
            assert!(corner_radius >= 0.0, "corner radius clamped to >= 0");
        }
    }

    // --- Batch 7: Perspective Grid ---

    #[test]
    fn test_perspective_grid_toggle() {
        let mut app = App::new();
        let was_on = app.perspective_grid.is_some();
        app.apply(Action::TogglePerspectiveGrid);
        assert_ne!(app.perspective_grid.is_some(), was_on, "toggle should flip grid state");
    }

    #[test]
    fn test_perspective_plane_select() {
        let mut app = App::new();
        app.apply(Action::SetPerspectivePlane(1));
        assert_eq!(app.perspective_active_plane, 1);
        app.apply(Action::SetPerspectivePlane(2));
        assert_eq!(app.perspective_active_plane, 2);
    }

    #[test]
    fn test_perspective_plane_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPerspectivePlane(99));
        assert!(app.perspective_active_plane <= 2, "plane clamped to 0..=2");
    }

    // --- Batch 7: Color Guide ---

    #[test]
    fn test_color_guide_complementary() {
        let mut app = App::new();
        let key = [1.0_f32, 0.0, 0.0, 1.0]; // red
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Complementary, key_color: key });
        assert!(!app.color_guide.swatches.is_empty(), "complementary should produce swatches");
    }

    #[test]
    fn test_color_guide_triadic() {
        let mut app = App::new();
        let key = [0.0_f32, 0.8, 0.0, 1.0];
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Triadic, key_color: key });
        assert_eq!(app.color_guide.swatches.len(), 3, "triadic = 3 swatches");
    }

    #[test]
    fn test_color_guide_apply_sets_fill() {
        let mut app = App::new();
        let key = [0.5_f32, 0.2, 0.8, 1.0];
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Complementary, key_color: key });
        assert!(!app.color_guide.swatches.is_empty());
        let original_fill = app.default_fill;
        app.apply(Action::ApplyColorGuide(0));
        // Fill should have changed to the guide swatch.
        assert_ne!(app.default_fill, original_fill, "fill should change after applying guide color");
    }

    // --- Batch 7: Warp Tools ---

    #[test]
    fn test_warp_tool_kind() {
        let mut app = App::new();
        app.apply(Action::SetWarpToolKind(WarpToolKind::Crystallize));
        assert_eq!(app.warp_tool_kind, WarpToolKind::Crystallize);
    }

    #[test]
    fn test_warp_brush_params() {
        let mut app = App::new();
        app.apply(Action::SetWarpBrush { size: 50.0, intensity: 0.8, detail: 2.0 });
        assert!((app.warp_brush_size - 50.0).abs() < 0.01);
        assert!((app.warp_brush_intensity - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_scallop_pulls_inward() {
        let mut app = App::new();
        // Point at (100, 0). Center at (50, 0): dx=50, scallop pulls toward center → x decreases.
        app.doc.shapes.push(Shape::path(vec![(100.0, 0.0), (200.0, 0.0), (150.0, 50.0)], vec![], true, [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.apply(Action::SetWarpToolKind(WarpToolKind::Scallop));
        app.apply(Action::SetWarpBrush { size: 30.0, intensity: 1.0, detail: 1.0 });
        let before = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0].0 } else { 0.0 };
        // Center at (50,0), radius 80 — point inside, dx=50.
        app.apply(Action::ApplyWarpStroke { center: (50.0, 0.0), radius: 80.0 });
        let after = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0].0 } else { 0.0 };
        assert!(after < before, "scallop should pull point toward center: {} -> {}", before, after);
    }

    #[test]
    fn test_warp_outside_radius_unchanged() {
        let mut app = App::new();
        app.doc.shapes.push(Shape::path(vec![(500.0, 500.0), (600.0, 500.0), (550.0, 600.0)], vec![], true, [0.0,1.0,0.0,1.0], [0.0;4], 0.0));
        app.apply(Action::SetWarpToolKind(WarpToolKind::Crystallize));
        app.apply(Action::SetWarpBrush { size: 20.0, intensity: 1.0, detail: 1.0 });
        let before = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0] } else { (0.0, 0.0) };
        // Center far away.
        app.apply(Action::ApplyWarpStroke { center: (0.0, 0.0), radius: 20.0 });
        let after = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0] } else { (0.0, 0.0) };
        assert_eq!(before, after, "point outside radius should not move");
    }

}

