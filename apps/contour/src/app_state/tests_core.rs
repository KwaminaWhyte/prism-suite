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
}

