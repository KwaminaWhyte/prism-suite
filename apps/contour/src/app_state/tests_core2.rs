use super::*;

#[cfg(test)]
mod tests {
    use super::*;
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
}


