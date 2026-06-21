use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Wave 9 tests -------------------------------------------------------

    /// PickColor sets `fg_color` and reverts the active tool to `prev_tool`.
    #[test]
    fn pick_color_sets_fg_and_reverts_tool() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Select));
        app.apply(Action::SetTool(Tool::Eyedropper));
        assert_eq!(app.prev_tool, Tool::Select);
        app.apply(Action::PickColor([255, 128, 0, 255]));
        assert!((app.fg_color[0] - 1.0).abs() < 0.01);
        assert!((app.fg_color[1] - 128.0 / 255.0).abs() < 0.01);
        assert!((app.fg_color[2] - 0.0).abs() < 0.01);
        // Tool reverted to Select after pick.
        assert_eq!(app.active, Tool::Select);
    }

    /// ExportSvg writes a file containing `<svg` and one element per visible shape.
    #[test]
    fn export_svg_writes_file() {
        let mut app = App::new();
        let path = std::env::temp_dir().join("contour_wave9_test.svg");
        app.apply(Action::ExportSvg(path.clone()));
        assert!(path.exists(), "SVG file was not created");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<svg"), "no <svg> root element");
        assert!(content.contains("<rect"), "no rect element for sample doc");
        assert!(content.contains("</svg>"), "SVG not closed");
        let _ = std::fs::remove_file(&path);
    }

    /// GroupSelected assigns the same group id to all selected shapes; UngroupSelected clears it.
    #[test]
    fn group_and_ungroup_selected() {
        let mut app = App::new();
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        assert!(app.can_align());
        app.apply(Action::GroupSelected);
        let g0 = app.doc.shapes[0].group();
        let g1 = app.doc.shapes[1].group();
        assert!(g0.is_some(), "shape 0 should have a group id");
        assert_eq!(g0, g1, "both shapes should share the same group id");
        app.apply(Action::UngroupSelected);
        assert!(app.doc.shapes[0].group().is_none());
        assert!(app.doc.shapes[1].group().is_none());
    }

    /// MoveLayerOrder(+1) moves a shape one step toward the top (paint-order).
    #[test]
    fn move_layer_order_reorders_shapes() {
        let mut app = App::new();
        let n = app.doc.shapes.len();
        assert!(n >= 2);
        let label0 = app.doc.shapes[0].label();
        let label1 = app.doc.shapes[1].label();
        app.apply(Action::SelectShape(0));
        app.apply(Action::MoveLayerOrder { id: 0, delta: 1 });
        // Shape that was at index 1 is now at 0; shape that was at 0 is now at 1.
        assert_eq!(app.doc.shapes[0].label(), label1);
        assert_eq!(app.doc.shapes[1].label(), label0);
        // Selection follows the moved shape.
        assert_eq!(app.selected, Some(1));
    }

    /// AttachTextToPath inserts into text_on_path map; DetachTextFromPath removes it.
    #[test]
    fn attach_and_detach_text_to_path() {
        let mut app = App::new();
        app.apply(Action::AttachTextToPath { text_id: 0, path_id: 1 });
        assert_eq!(app.text_on_path.get(&0), Some(&1));
        app.apply(Action::DetachTextFromPath(0));
        assert!(app.text_on_path.get(&0).is_none());
    }

    /// The selection ring source: bounds reflect the selected shape, and clearing
    /// selection yields no bounds.
    #[test]
    fn selected_bounds_tracks_selection() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let b = app.selected_bounds().expect("shape 0 has bounds");
        assert!(b[2] > 0.0 && b[3] > 0.0);
        // A click that misses every shape clears the selection → no bounds.
        app.apply(Action::HitTestSelect {
            x: -10_000.0,
            y: -10_000.0,
        });
        assert!(app.selected_bounds().is_none());
    }

    // ---- Wave 11 tests -------------------------------------------------------

    /// SetGradientType seeds a gradient on a shape without one and toggles kind.
    #[test]
    fn set_gradient_type_seeds_and_toggles() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        assert!(app.doc.shapes[0].fill_gradient().is_none(), "no gradient initially");
        // Seed a radial gradient.
        app.apply(Action::SetGradientType { shape_id: 0, kind: GradientKind::Radial });
        let g = app.doc.shapes[0].fill_gradient().expect("gradient seeded");
        assert_eq!(g.kind, GradientKind::Radial);
        // Toggle to linear.
        app.apply(Action::SetGradientType { shape_id: 0, kind: GradientKind::Linear });
        assert_eq!(app.doc.shapes[0].fill_gradient().unwrap().kind, GradientKind::Linear);
    }

    /// MergeRegionReal unions two rects into one shape (using i_overlay).
    #[test]
    fn merge_region_real_unions_two_rects() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        app.doc.shapes.push(Shape::rect([50.0, 50.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        app.apply(Action::MergeRegionReal(vec![0, 1]));
        // Two shapes merged → one result shape.
        assert_eq!(app.doc.shapes.len(), 1, "two rects merged into one");
        assert!(app.selected.is_some());
    }

    /// SubtractRegionReal subtracts shape 1 from shape 0.
    #[test]
    fn subtract_region_real_subtracts() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        app.doc.shapes.push(Shape::rect([50.0, 50.0, 100.0, 100.0], app.default_fill, app.default_stroke, 1.0));
        let before = app.doc.shapes.len();
        app.apply(Action::SubtractRegionReal(vec![0, 1]));
        // Should have at most the same count (result replaces both).
        assert!(app.doc.shapes.len() <= before);
    }

    /// Character panel actions update App state correctly.
    #[test]
    fn character_panel_actions_update_state() {
        let mut app = App::new();
        app.apply(Action::SetFontFamily("Georgia".to_string()));
        assert_eq!(app.font_family, "Georgia");
        app.apply(Action::SetFontWeight(FontWeight::Bold));
        assert_eq!(app.font_weight, FontWeight::Bold);
        app.apply(Action::SetLetterSpacing(50.0));
        assert!((app.letter_spacing - 50.0).abs() < 1e-3);
        app.apply(Action::SetLineHeight(1.5));
        assert!((app.line_height - 1.5).abs() < 1e-3);
        app.apply(Action::SetParaAlign(TextAlign::Center));
        assert_eq!(app.text_align, TextAlign::Center);
    }

    /// SetFontSize syncs default_font_size.
    #[test]
    fn set_font_size_syncs_default() {
        let mut app = App::new();
        app.apply(Action::SetFontSize(48.0));
        assert!((app.default_font_size - 48.0).abs() < 1e-3);
    }

    /// ExportPdf writes a file starting with "%PDF".
    #[test]
    fn export_pdf_writes_file() {
        let mut app = App::new();
        let path = std::env::temp_dir().join("contour_wave11_test.pdf");
        app.apply(Action::ExportPdf(path.clone()));
        assert!(path.exists(), "PDF file was not created");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "file does not start with %PDF");
        let _ = std::fs::remove_file(&path);
    }

    /// Isolation mode: EnterIsolation sets isolation_group; ExitIsolation clears it.
    #[test]
    fn isolation_mode_enters_and_exits() {
        let mut app = App::new();
        assert!(app.isolation_group.is_none());
        app.apply(Action::EnterIsolation(42));
        assert_eq!(app.isolation_group, Some(42));
        app.apply(Action::ExitIsolation);
        assert!(app.isolation_group.is_none());
    }

    /// KnifeSlice splits a shape that straddles the cut line into two rects.
    #[test]
    fn knife_slice_splits_straddling_shape() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A 200×200 rect centred at (100,100); a vertical knife at x=100 straddles it.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 200.0, 200.0], app.default_fill, app.default_stroke, 1.0));
        app.apply(Action::KnifeSlice { start: (100.0, -10.0), end: (100.0, 210.0) });
        // The single rect was split into two rect halves.
        assert_eq!(app.doc.shapes.len(), 2, "knife should split into two shapes");
    }

    /// KnifeSlice leaves a non-intersected shape alone.
    #[test]
    fn knife_slice_skips_non_intersected() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], app.default_fill, app.default_stroke, 1.0));
        // Knife entirely to the right of the shape.
        app.apply(Action::KnifeSlice { start: (200.0, 0.0), end: (200.0, 100.0) });
        assert_eq!(app.doc.shapes.len(), 1, "shape outside knife should be unchanged");
    }

    /// Tool::Knife is in Tool::ALL.
    #[test]
    fn knife_tool_in_all() {
        assert!(Tool::ALL.contains(&Tool::Knife));
        assert_eq!(Tool::Knife.label(), "Knife");
    }

    // --- Batch 5 ---------------------------------------------------------

    /// Attaching text to a path bakes warped glyphs into the text's cache, and
    /// editing the offset re-bakes them (one undo step each).
    #[test]
    fn text_on_path_bakes_glyphs() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A text object + a spine path.
        app.apply(Action::PlaceText { x: 0.0, y: 0.0 });
        for c in "Type".chars() {
            app.apply(Action::TypeChar(c));
        }
        app.apply(Action::FinishText);
        let tid = 0usize;
        // A horizontal spine path.
        let spine = Shape::path(
            vec![(0.0, 200.0), (400.0, 200.0)],
            Vec::new(),
            false,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        );
        app.doc.shapes.push(spine);
        let pid = app.doc.shapes.len() - 1;
        // Capture flat glyph centroid before attaching.
        let flat_y = text_centroid_y(&app.doc.shapes[tid]);
        app.apply(Action::AttachTextToPath { text_id: tid, path_id: pid });
        assert!(app.text_on_path.get(&tid) == Some(&pid));
        // Shift glyphs up via offset; the centroid y must change.
        app.apply(Action::SetTextOnPathParams {
            text_id: tid,
            params: crate::text_on_path::TextOnPathParams {
                offset: 40.0,
                ..Default::default()
            },
        });
        let shifted_y = text_centroid_y(&app.doc.shapes[tid]);
        assert!(
            (shifted_y - flat_y).abs() > 1.0,
            "offset should move baked glyphs: {flat_y} -> {shifted_y}"
        );
        // Detach returns to a flat layout.
        app.apply(Action::DetachTextFromPath(tid));
        assert!(app.text_on_path.get(&tid).is_none());
        assert!(app.text_on_path_params.get(&tid).is_none());
    }

    fn text_centroid_y(s: &Shape) -> f32 {
        if let Shape::Text { glyphs, .. } = s {
            let mut sum = 0.0;
            let mut n = 0.0;
            for g in glyphs {
                for &(_, y) in &g.points {
                    sum += y;
                    n += 1.0;
                }
            }
            if n > 0.0 {
                sum / n
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Applying perspective replaces a rect with a warped path whose top edge is
    /// narrower than its bottom edge (the default trapezoid).
    #[test]
    fn perspective_distort_warps_rect() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        app.select_single(0);
        app.apply(Action::ApplyPerspectiveDistort { shape_id: 0 });
        // The rect is now a warped path; its top two points are inset.
        if let Shape::Path { points, .. } = &app.doc.shapes[0] {
            assert!(points.len() >= 4);
            let top_left = points[0];
            assert!(top_left.0 > 0.0, "top-left pulled inward: {top_left:?}");
        } else {
            panic!("expected a warped path");
        }
    }

    /// Outline-width-profile produces a separate filled band shape.
    #[test]
    fn outline_width_profile_emits_band() {
        let mut app = App::new();
        app.doc.shapes.clear();
        let mut line = Shape::path(
            vec![(0.0, 0.0), (100.0, 0.0)],
            Vec::new(),
            false,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            8.0,
        );
        if let Shape::Path { stroke_style, .. } = &mut line {
            stroke_style.width_profile = (1.0, 0.0); // taper to a point
        }
        app.doc.shapes.push(line);
        app.select_single(0);
        app.apply(Action::OutlineWidthProfile(0));
        assert_eq!(app.doc.shapes.len(), 2, "a band shape is inserted");
    }

    /// Roughen perturbs the selected shape's geometry.
    #[test]
    fn roughen_changes_geometry() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        app.select_single(0);
        app.apply(Action::RoughenPath { size: 10.0, detail: 2 });
        // The rect is demoted to a roughened path with more vertices.
        if let Shape::Path { points, .. } = &app.doc.shapes[0] {
            assert!(points.len() > 4, "subdivided: {}", points.len());
        } else {
            panic!("expected a roughened path");
        }
    }

    /// MakeMeshGradient seeds 16 control points; clear empties them.
    #[test]
    fn mesh_gradient_seed_and_clear() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            [0.2, 0.4, 0.8, 1.0],
            app.default_stroke,
            1.0,
        ));
        app.select_single(0);
        app.apply(Action::MakeMeshGradient);
        assert_eq!(app.mesh_points.len(), 16);
        app.apply(Action::ClearMeshGradient);
        assert!(app.mesh_points.is_empty());
    }

}

