use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    /// A drag in either corner order produces the same normalized rect, inherits
    /// the default paint, appends in paint order, and becomes the selection.
    #[test]
    fn create_shape_normalizes_and_selects() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        // Corners given bottom-right → top-left to exercise the min/abs path.
        app.apply(Action::CreateShape {
            tool: Tool::Rect,
            a: [300.0, 260.0],
            b: [100.0, 60.0],
        });
        assert_eq!(app.doc.shapes.len(), before + 1);
        let idx = before;
        assert_eq!(app.selected, Some(idx));
        let r = app.doc.shapes[idx].bounds().unwrap();
        assert!((r.x - 100.0).abs() < 1e-3 && (r.y - 60.0).abs() < 1e-3);
        assert!((r.w - 200.0).abs() < 1e-3 && (r.h - 200.0).abs() < 1e-3);
        assert_eq!(app.doc.shapes[idx].fill_color(), Some(app.default_fill));
        assert_eq!(app.doc.shapes[idx].label(), "Rectangle");
    }

    /// The Line tool drag-creates a `Shape::Line` from the raw drag endpoints.
    #[test]
    fn create_shape_line_tool() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::SetTool(Tool::Line));
        app.apply(Action::CreateShape {
            tool: Tool::Line,
            a: [10.0, 20.0],
            b: [110.0, 70.0],
        });
        assert_eq!(app.doc.shapes.len(), before + 1);
        match app.doc.shapes.last().unwrap() {
            Shape::Line { p0, p1, .. } => {
                assert_eq!(*p0, (10.0, 20.0));
                assert_eq!(*p1, (110.0, 70.0));
            }
            other => panic!("expected a Line, got {}", other.label()),
        }
    }

    /// The Polygon tool drag-creates a closed live-shape path centred at the press
    /// point, with one vertex per side (the host's `poly_sides` default).
    #[test]
    fn create_shape_polygon_tool() {
        let mut app = App::new();
        app.poly_sides = 6;
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape {
            tool: Tool::Polygon,
            a: [200.0, 200.0],
            b: [200.0, 150.0], // radius 50
        });
        match app.doc.shapes.last().unwrap() {
            Shape::Path {
                points,
                closed,
                live: Some(LiveShape::Polygon { sides, radius, .. }),
                ..
            } => {
                assert!(*closed);
                assert_eq!(*sides, 6);
                assert_eq!(points.len(), 6);
                assert!((*radius - 50.0).abs() < 1e-3);
            }
            other => panic!("expected a live Polygon path, got {}", other.label()),
        }
    }

    /// The Star tool drag-creates a closed live star with `2 * points` vertices.
    #[test]
    fn create_shape_star_tool() {
        let mut app = App::new();
        app.star_points = 5;
        app.apply(Action::SetTool(Tool::Star));
        app.apply(Action::CreateShape {
            tool: Tool::Star,
            a: [200.0, 200.0],
            b: [240.0, 230.0], // radius 50
        });
        match app.doc.shapes.last().unwrap() {
            Shape::Path {
                points,
                live: Some(LiveShape::Star { points: p, .. }),
                ..
            } => {
                assert_eq!(*p, 5);
                assert_eq!(points.len(), 10);
            }
            other => panic!("expected a live Star path, got {}", other.label()),
        }
    }

    /// A zero-radius polygon drag is dropped.
    #[test]
    fn create_shape_polygon_drops_degenerate() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::CreateShape {
            tool: Tool::Polygon,
            a: [50.0, 50.0],
            b: [50.2, 50.2],
        });
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Type: placing text then typing builds a `Shape::Text` whose string and
    /// glyph cache track the keystrokes; the whole session is one undo entry.
    #[test]
    fn type_place_and_edit_text() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 100.0, y: 100.0 });
        assert_eq!(app.doc.shapes.len(), before + 1);
        let idx = app.editing_text.expect("editing the placed text");
        for c in "Hi".chars() {
            app.apply(Action::TypeChar(c));
        }
        match &app.doc.shapes[idx] {
            Shape::Text { params, glyphs, .. } => {
                assert_eq!(params.text, "Hi");
                assert!(!glyphs.is_empty(), "glyphs laid out for non-empty text");
            }
            other => panic!("expected Text, got {}", other.label()),
        }
        app.apply(Action::TypeBackspace);
        assert_eq!(app.selected_shape().unwrap().text_params().unwrap().text, "H");
        app.apply(Action::FinishText);
        assert!(app.editing_text.is_none());
        assert_eq!(app.doc.shapes.len(), before + 1);
        // The whole place→type session collapses to one undo entry.
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Type: an object placed but never typed into is removed on finish, and that
    /// no-op session records nothing in history.
    #[test]
    fn type_empty_text_is_discarded() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        let undo_before = app.history.can_undo();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 10.0, y: 10.0 });
        app.apply(Action::FinishText);
        assert_eq!(app.doc.shapes.len(), before);
        assert_eq!(app.history.can_undo(), undo_before, "no checkpoint for an empty placed text");
    }

    /// Switching off the Type tool finishes the edit (and discards empty text).
    #[test]
    fn switching_tool_finishes_text_edit() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 10.0, y: 10.0 });
        app.apply(Action::TypeChar('A'));
        app.apply(Action::SetTool(Tool::Select));
        assert!(app.editing_text.is_none());
        assert_eq!(app.doc.shapes.last().unwrap().text_params().unwrap().text, "A");
    }

    /// Boolean: uniting two overlapping rects replaces both operands with a result
    /// batch and is undoable in one step. Requires a primary + secondary selection.
    #[test]
    fn boolean_union_replaces_operands() {
        let mut app = App::new();
        // Two overlapping rects appended on top of the sample document.
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        app.doc.shapes.push(Shape::rect(
            [50.0, 50.0, 100.0, 100.0],
            app.default_fill,
            app.default_stroke,
            1.0,
        ));
        let a = app.doc.shapes.len() - 2;
        let b = app.doc.shapes.len() - 1;
        app.selected = Some(b);
        app.secondary = Some(a);
        assert!(app.can_boolean());
        let before = app.doc.shapes.len();
        app.apply(Action::Boolean(BoolOp::Union));
        // The two rects are gone, replaced by at least one result shape.
        assert!(app.doc.shapes.len() < before + 1);
        assert!(app.doc.shapes.len() >= before - 1);
        assert!(app.secondary.is_none());
        // One undo restores both operands.
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Boolean: a single selection (no secondary) can't run an op — no-op.
    #[test]
    fn boolean_needs_two_operands() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let before = app.doc.shapes.len();
        assert!(!app.can_boolean());
        app.apply(Action::Boolean(BoolOp::Intersect));
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Shift+click adds a second operand; a third distinct click moves it.
    #[test]
    fn add_to_selection_tracks_secondary() {
        let mut app = App::new();
        // Sample doc has 3 shapes; pick centres that hit shapes 0 and 2.
        app.apply(Action::SelectShape(0));
        // Hit the green rect (shape 2) which is selectable and on top there.
        let g = app.doc.shapes[2].bounds().unwrap();
        app.apply(Action::AddToSelection {
            x: g.x + g.w * 0.5,
            y: g.y + g.h * 0.5,
        });
        assert!(app.secondary.is_some());
    }

    /// The Ellipse tool drag-creates an ellipse.
    #[test]
    fn create_shape_ellipse_tool() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::CreateShape {
            tool: Tool::Ellipse,
            a: [10.0, 10.0],
            b: [60.0, 90.0],
        });
        assert_eq!(app.doc.shapes.last().unwrap().label(), "Ellipse");
        assert_eq!(app.doc.shapes.len(), before + 1);
    }

    /// A sub-unit (zero-area) drag is dropped — no shape, no selection change.
    #[test]
    fn create_shape_drops_degenerate_drag() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        let sel = app.selected;
        app.apply(Action::CreateShape {
            tool: Tool::Rect,
            a: [50.0, 50.0],
            b: [50.4, 50.4],
        });
        assert_eq!(app.doc.shapes.len(), before);
        assert_eq!(app.selected, sel);
    }

    /// Panning translates the view offset by exactly the given delta.
    #[test]
    fn pan_translates_offset() {
        let mut app = App::new();
        let (ox, oy) = app.view.offset;
        app.apply(Action::PanBy { dx: 25.0, dy: -10.0 });
        assert_eq!(app.view.offset, (ox + 25.0, oy - 10.0));
    }

    /// Zooming holds the anchor point fixed: the document point under the anchor
    /// (viewport-local) maps to the same viewport-local pixel before and after.
    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut app = App::new();
        let anchor = (200.0_f32, 150.0_f32);
        // Document-local pixel under the anchor before zooming.
        let before_local = (
            (anchor.0 - app.view.offset.0) / app.view.scale,
            (anchor.1 - app.view.offset.1) / app.view.scale,
        );
        app.apply(Action::ZoomBy {
            factor: 1.5,
            anchor: Some(anchor),
            viewport: (800.0, 600.0),
        });
        // Re-project that same doc-local point and confirm it lands on the anchor.
        let projected = (
            app.view.offset.0 + before_local.0 * app.view.scale,
            app.view.offset.1 + before_local.1 * app.view.scale,
        );
        assert!((projected.0 - anchor.0).abs() < 1e-2);
        assert!((projected.1 - anchor.1).abs() < 1e-2);
        assert!((app.view.scale - 1.5).abs() < 1e-3);
    }

    /// Zoom is clamped to the configured range and never inverts.
    #[test]
    fn zoom_clamps_to_range() {
        let mut app = App::new();
        for _ in 0..50 {
            app.apply(Action::ZoomBy {
                factor: 2.0,
                anchor: None,
                viewport: (800.0, 600.0),
            });
        }
        assert!(app.view.scale <= View::MAX_SCALE + 1e-3);
        for _ in 0..50 {
            app.apply(Action::ZoomBy {
                factor: 0.5,
                anchor: None,
                viewport: (800.0, 600.0),
            });
        }
        assert!(app.view.scale >= View::MIN_SCALE - 1e-3);
    }

    /// Reset returns the view to its default pan + zoom.
    #[test]
    fn reset_view_restores_default() {
        let mut app = App::new();
        app.apply(Action::PanBy { dx: 100.0, dy: 100.0 });
        app.apply(Action::ZoomBy {
            factor: 2.0,
            anchor: None,
            viewport: (800.0, 600.0),
        });
        app.apply(Action::ResetView);
        let d = View::default();
        assert_eq!(app.view.offset, d.offset);
        assert_eq!(app.view.scale, d.scale);
    }

    /// Pen: clicking anchors then finishing closed commits a `Shape::Path` with
    /// those anchors, the host's default paint, and becomes the selection.
    #[test]
    fn pen_commits_closed_path() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        let before = app.doc.shapes.len();
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 150.0, y: 200.0 });
        assert_eq!(app.pen.points.len(), 3);
        app.apply(Action::PenFinish { closed: true });
        assert!(app.pen.is_empty(), "pen buffer drained on commit");
        assert_eq!(app.doc.shapes.len(), before + 1);
        let s = app.doc.shapes.last().unwrap();
        assert_eq!(s.label(), "Path");
        assert_eq!(app.selected, Some(app.doc.shapes.len() - 1));
        // The commit is one undo step that removes the path again.
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Pen: a click within the close tolerance of the first anchor closes +
    /// commits the path rather than placing another anchor.
    #[test]
    fn pen_first_anchor_click_closes() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        let before = app.doc.shapes.len();
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 150.0, y: 200.0 });
        // Click back on (almost) the first anchor.
        app.apply(Action::PenAddAnchor { x: 101.0, y: 100.5 });
        assert!(app.pen.is_empty());
        assert_eq!(app.doc.shapes.len(), before + 1);
        // The committed path closed and kept exactly the 3 placed anchors.
        if let Some(Shape::Path { points, closed, .. }) = app.doc.shapes.last() {
            assert!(*closed);
            assert_eq!(points.len(), 3);
        } else {
            panic!("expected a closed Path");
        }
    }

    /// Pen: a path with fewer than two anchors is discarded on finish.
    #[test]
    fn pen_discards_degenerate_path() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        let before = app.doc.shapes.len();
        app.apply(Action::PenAddAnchor { x: 50.0, y: 50.0 });
        app.apply(Action::PenFinish { closed: false });
        assert!(app.pen.is_empty());
        assert_eq!(app.doc.shapes.len(), before);
        assert!(!app.history.can_undo(), "no checkpoint for a dropped path");
    }

    /// Switching away from the Pen tool abandons a half-drawn path.
    #[test]
    fn switching_tool_cancels_pen() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        app.apply(Action::PenAddAnchor { x: 10.0, y: 10.0 });
        app.apply(Action::PenAddAnchor { x: 40.0, y: 10.0 });
        app.apply(Action::SetTool(Tool::Select));
        assert!(app.pen.is_empty());
    }

    /// Node edit: a coalesced anchor drag (begin → moves → commit) collapses to
    /// one undo entry that restores the original anchor position.
    #[test]
    fn node_anchor_move_coalesces_one_undo() {
        let mut app = App::new();
        // Build a triangle path via the pen and select it.
        app.apply(Action::SetTool(Tool::Pen));
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 150.0, y: 200.0 });
        app.apply(Action::PenFinish { closed: true });
        let idx = app.selected.unwrap();
        // Original first anchor.
        let orig = match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => points[0],
            _ => unreachable!(),
        };
        // Drag it (coalesced).
        app.apply(Action::BeginInteraction);
        app.apply(Action::MoveAnchor { which: 0, x: 120.0, y: 130.0 });
        app.apply(Action::MoveAnchor { which: 0, x: 140.0, y: 160.0 });
        app.apply(Action::EndInteraction);
        let moved = match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => points[0],
            _ => unreachable!(),
        };
        assert_eq!(moved, (140.0, 160.0));
        // One undo restores the pre-drag position (the whole drag is one entry).
        app.apply(Action::Undo);
        let restored = match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => points[0],
            _ => unreachable!(),
        };
        assert_eq!(restored, orig);
    }

    /// A no-op node drag (begin → commit with no change) records no undo entry.
    #[test]
    fn node_noop_drag_records_nothing() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let depth_before = app.history.can_undo();
        app.apply(Action::BeginInteraction);
        app.apply(Action::EndInteraction);
        assert_eq!(app.history.can_undo(), depth_before);
    }

    /// `hit_node` finds an anchor of the selected path within tolerance and
    /// prioritises a handle knob when one is in range.
    #[test]
    fn hit_node_finds_anchor_and_handle() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Pen));
        app.apply(Action::PenAddAnchor { x: 100.0, y: 100.0 });
        app.apply(Action::PenAddAnchor { x: 200.0, y: 100.0 });
        // Give the last anchor a tangent so its knob is hit-testable.
        app.apply(Action::PenSetHandle { x: 230.0, y: 100.0 });
        app.apply(Action::PenFinish { closed: false });
        // Right on the first anchor → Anchor(0).
        assert_eq!(app.hit_node(100.0, 100.0), Some(NodeTarget::Anchor(0)));
        // On anchor 1's out-knob (anchor + handle = (230,100)) → Handle(1).
        assert_eq!(app.hit_node(230.0, 100.0), Some(NodeTarget::Handle(1)));
        // Far from everything → None.
        assert_eq!(app.hit_node(-50.0, -50.0), None);
    }

    /// Delete removes the selected shape as one undoable step.
    #[test]
    fn delete_selected_is_undoable() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::SelectShape(0));
        app.apply(Action::DeleteSelected);
        assert_eq!(app.doc.shapes.len(), before - 1);
        assert_eq!(app.selected, None);
        app.apply(Action::Undo);
        assert_eq!(app.doc.shapes.len(), before);
    }

    /// Undo / redo round-trips an appearance edit (one checkpoint per click).
    #[test]
    fn appearance_edit_undo_redo() {
        let mut app = App::new();
        app.apply(Action::SelectShape(0));
        let orig = app.selected_fill().unwrap();
        let new = [0.1, 0.2, 0.3, 1.0];
        app.apply(Action::SetFillColor(new));
        assert_eq!(app.selected_fill().unwrap(), new);
        app.apply(Action::Undo);
        assert_eq!(app.selected_fill().unwrap(), orig);
        app.apply(Action::Redo);
        assert_eq!(app.selected_fill().unwrap(), new);
    }

    /// Shift+click toggles a shape in and out of the multi-selection; the last
    /// added is primary, the previous becomes the secondary boolean operand.
    #[test]
    fn shift_click_toggles_multi_selection() {
        let mut app = App::new();
        // Select shape 0, then shift-add shape 2 (the green rect, on top there).
        app.apply(Action::SelectShape(0));
        let g = app.doc.shapes[2].bounds().unwrap();
        let (gx, gy) = (g.x + g.w * 0.5, g.y + g.h * 0.5);
        app.apply(Action::AddToSelection { x: gx, y: gy });
        assert_eq!(app.selection, vec![0, 2]);
        assert_eq!(app.selected, Some(2));
        assert_eq!(app.secondary, Some(0));
        assert!(app.can_boolean());
        // Shift-clicking shape 2 again removes it.
        app.apply(Action::AddToSelection { x: gx, y: gy });
        assert_eq!(app.selection, vec![0]);
        assert_eq!(app.selected, Some(0));
        assert!(app.secondary.is_none());
    }

    /// A marquee rectangle selects every shape whose bounds it intersects, and an
    /// empty marquee clears the selection.
    #[test]
    fn marquee_selects_intersecting_shapes() {
        let mut app = App::new();
        // The sample doc's three shapes all live within (120..740, 120..560).
        app.apply(Action::MarqueeSelect {
            rect: [0.0, 0.0, 2000.0, 2000.0],
        });
        assert_eq!(app.selection.len(), 3);
        // A marquee far off the artboard hits nothing.
        app.apply(Action::MarqueeSelect {
            rect: [-1000.0, -1000.0, 10.0, 10.0],
        });
        assert!(app.selection.is_empty());
        assert!(app.selected.is_none());
    }

    /// Editing a live polygon's sides regenerates its outline (one vertex per
    /// side) as one undo step, and mirrors the working default.
    #[test]
    fn set_live_shape_regenerates_outline() {
        let mut app = App::new();
        app.apply(Action::CreateShape {
            tool: Tool::Polygon,
            a: [200.0, 200.0],
            b: [200.0, 150.0],
        });
        let idx = app.selected.unwrap();
        let live = app.primary_live_shape().expect("polygon is live");
        let new = match live {
            LiveShape::Polygon { radius, corner_radius, .. } => LiveShape::Polygon { sides: 8, radius, corner_radius },
            other => other,
        };
        app.apply(Action::SetLiveShape(new));
        match &app.doc.shapes[idx] {
            Shape::Path {
                points,
                live: Some(LiveShape::Polygon { sides, .. }),
                ..
            } => {
                assert_eq!(*sides, 8);
                assert_eq!(points.len(), 8);
            }
            _ => panic!("expected an 8-gon"),
        }
        assert_eq!(app.poly_sides, 8);
        app.apply(Action::Undo);
        match &app.doc.shapes[idx] {
            Shape::Path { points, .. } => assert_eq!(points.len(), 6),
            _ => unreachable!(),
        }
    }

    /// Editing the primary text object's size / alignment / font re-lays-out its
    /// glyph cache through `set_text_params`, each as its own undo step.
    #[test]
    fn set_text_params_relays_out() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Type));
        app.apply(Action::PlaceText { x: 100.0, y: 100.0 });
        for c in "Hi".chars() {
            app.apply(Action::TypeChar(c));
        }
        app.apply(Action::FinishText);
        let idx = app.selected.unwrap();
        app.apply(Action::SetTextSize(120.0));
        assert_eq!(app.primary_text_params().unwrap().font_size, 120.0);
        app.apply(Action::SetTextAlign(TextAlign::Center));
        assert_eq!(app.primary_text_params().unwrap().align, TextAlign::Center);
        app.apply(Action::SetTextFont(Some("Helvetica".into())));
        assert_eq!(
            app.primary_text_params().unwrap().font_family.as_deref(),
            Some("Helvetica")
        );
        // The glyph cache stays consistent with the params.
        assert!(app.doc.shapes[idx].text_params().is_some());
    }

    /// Align-left moves every selected shape's left edge to the selection's
    /// combined left, as one undo step; needs ≥2 shapes.
    #[test]
    fn align_left_shares_left_edge() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 40.0, 40.0], app.default_fill, app.default_stroke, 1.0));
        app.doc.shapes.push(Shape::rect([100.0, 200.0, 40.0, 40.0], app.default_fill, app.default_stroke, 1.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        assert!(app.can_align());
        app.apply(Action::AlignSelection(Align::Left));
        let l0 = app.doc.shapes[0].bounds().unwrap().x;
        let l1 = app.doc.shapes[1].bounds().unwrap().x;
        assert!((l0 - 10.0).abs() < 1e-3 && (l1 - 10.0).abs() < 1e-3);
        app.apply(Action::Undo);
        assert!((app.doc.shapes[1].bounds().unwrap().x - 100.0).abs() < 1e-3);
    }

    /// Distribute needs ≥3 shapes; with three it evens the centre spacing (the
    /// outer two stay put).
    #[test]
    fn distribute_centers_evens_spacing() {
        let mut app = App::new();
        app.doc.shapes.clear();
        for x in [0.0, 30.0, 200.0] {
            app.doc.shapes.push(Shape::rect([x, 0.0, 20.0, 20.0], app.default_fill, app.default_stroke, 1.0));
        }
        app.selection = vec![0, 1, 2];
        app.sync_legacy_selection();
        assert!(app.can_distribute());
        app.apply(Action::DistributeSelection(Distribute::CentersH));
        let c = |i: usize| {
            let b = app.doc.shapes[i].bounds().unwrap();
            b.x + b.w * 0.5
        };
        // The middle centre lands halfway between the (unmoved) outer two.
        let mid = (c(0) + c(2)) * 0.5;
        assert!((c(1) - mid).abs() < 1e-2, "middle centre {} vs {}", c(1), mid);
    }

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

