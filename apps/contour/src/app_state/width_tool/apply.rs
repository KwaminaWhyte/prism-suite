//! The apply layer for the **Width Tool**: wires the pure stroke-outline geometry
//! in [`super`] onto `impl App`. Editing a shape's [`WidthProfile`] (add / move /
//! delete a width point, set its widths, apply a preset) mutates an app-side
//! per-shape profile map; **Expand Stroke** bakes the variable-width outline into
//! a real filled [`Shape::Path`] with one undo checkpoint.
//!
//! This is the **terminal** stage of the `Action` dispatch chain: it is reached
//! from `apply_path_distort`'s catch-all, and its own `_ => {}` is the chain's
//! final no-op.
//!
//! The width profile lives on `App` (keyed by paint-order index), not in the
//! document model — like the text-on-path attachment maps — so profile-only edits
//! don't touch the undo history; only the geometry-changing Expand does.

use super::{stroke_outline, WidthPreset, WidthProfile};
use crate::app_state::{Action, App};
use crate::document::Shape;

/// The default uniform half-width for a freshly created profile: half the shape's
/// stroke weight, floored to a tiny positive value so the outline never starts
/// fully degenerate.
fn default_half(shape: &Shape) -> f32 {
    (shape.stroke_width().max(0.0) * 0.5).max(0.25)
}

impl App {
    /// Terminal dispatcher for the Width Tool actions. Reached from
    /// `apply_path_distort`'s catch-all; owns the dispatch chain's final no-op.
    pub(in crate::app_state) fn apply_width_tool(&mut self, action: Action) {
        match action {
            Action::WidthPointAdd {
                shape_id,
                t,
                left,
                right,
            } => self.width_add_point(shape_id, t, left, right),
            Action::WidthPointMove { shape_id, index, t } => {
                self.width_move_point(shape_id, index, t)
            }
            Action::WidthPointDelete { shape_id, index } => {
                self.width_delete_point(shape_id, index)
            }
            Action::WidthPointSetWidths {
                shape_id,
                index,
                left,
                right,
            } => self.width_set_point_widths(shape_id, index, left, right),
            Action::WidthApplyPreset { shape_id, preset } => {
                self.width_apply_preset(shape_id, preset)
            }
            Action::WidthExpandStroke { shape_id } => self.width_expand_stroke(shape_id),
            _ => {}
        }
    }

    /// The shape's width profile, creating a uniform one (from its stroke weight)
    /// on first edit. `None` for an out-of-range id.
    fn width_profile_or_default(&mut self, shape_id: usize) -> Option<&mut WidthProfile> {
        if shape_id >= self.doc.shapes.len() {
            return None;
        }
        if !self.width_profiles.contains_key(&shape_id) {
            let half = default_half(&self.doc.shapes[shape_id]);
            self.width_profiles
                .insert(shape_id, WidthProfile::uniform(half));
        }
        self.width_profiles.get_mut(&shape_id)
    }

    /// Add a width point at arc-length `t` with the given left/right half-widths.
    fn width_add_point(&mut self, shape_id: usize, t: f32, left: f32, right: f32) {
        if let Some(p) = self.width_profile_or_default(shape_id) {
            p.add_point(t, left, right);
            self.host.mark_dirty();
        }
    }

    /// Slide width point `index` along the path to `t` (clamped between its
    /// neighbours).
    fn width_move_point(&mut self, shape_id: usize, index: usize, t: f32) {
        if let Some(p) = self.width_profile_or_default(shape_id) {
            if p.move_point(index, t) {
                self.host.mark_dirty();
            }
        }
    }

    /// Delete width point `index` (keeps at least one).
    fn width_delete_point(&mut self, shape_id: usize, index: usize) {
        if let Some(p) = self.width_profile_or_default(shape_id) {
            if p.delete_point(index) {
                self.host.mark_dirty();
            }
        }
    }

    /// Set the left/right half-widths of width point `index`.
    fn width_set_point_widths(&mut self, shape_id: usize, index: usize, left: f32, right: f32) {
        if let Some(p) = self.width_profile_or_default(shape_id) {
            if p.set_widths(index, left, right) {
                self.host.mark_dirty();
            }
        }
    }

    /// Replace the shape's width profile with a preset (scaled to its current
    /// stroke weight).
    fn width_apply_preset(&mut self, shape_id: usize, preset: WidthPreset) {
        if shape_id >= self.doc.shapes.len() {
            return;
        }
        let half = default_half(&self.doc.shapes[shape_id]);
        self.width_profiles
            .insert(shape_id, WidthProfile::from_preset(preset, half));
        self.host.mark_dirty();
    }

    /// **Expand Stroke**: bake the shape's variable-width outline into a real
    /// closed, filled [`Shape::Path`] (fill = the old stroke colour, no stroke),
    /// replacing the shape. Uses the shape's profile, or a uniform one from its
    /// stroke weight if none has been edited. One undo step; no-op for a bad id,
    /// a non-path shape, or a degenerate result.
    fn width_expand_stroke(&mut self, shape_id: usize) {
        if shape_id >= self.doc.shapes.len() {
            return;
        }
        let path = self.doc.shapes[shape_id].to_path();
        let (points, handles, closed) = match &path {
            Shape::Path {
                points,
                handles,
                closed,
                ..
            } => (points.clone(), handles.clone(), *closed),
            _ => return,
        };
        if points.len() < 2 {
            return;
        }
        let half = default_half(&self.doc.shapes[shape_id]);
        let profile = self
            .width_profiles
            .get(&shape_id)
            .cloned()
            .unwrap_or_else(|| WidthProfile::uniform(half));
        let cap = self.doc.shapes[shape_id].stroke_style().cap;
        let outline = stroke_outline(&points, &handles, closed, &profile, cap);
        if outline.len() < 3 {
            return;
        }
        let fill = self.doc.shapes[shape_id]
            .stroke_color()
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        self.checkpoint();
        // The baked outline is a filled contour: fill = old stroke, no stroke.
        self.doc.shapes[shape_id] =
            Shape::path(outline, Vec::new(), true, fill, [0.0, 0.0, 0.0, 0.0], 0.0);
        self.width_profiles.remove(&shape_id);
        self.host.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::width_tool::contour_area;
    use crate::document::LineCap;

    /// An app holding a single selected horizontal open path, stroke weight 4
    /// (half-width 2), red stroke.
    fn app_with_path() -> App {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::path(
            vec![(0.0, 0.0), (100.0, 0.0)],
            Vec::new(),
            false,
            [0.0, 0.0, 0.0, 0.0], // transparent fill
            [1.0, 0.0, 0.0, 1.0], // red stroke
            4.0,
        ));
        app.select_single(0);
        app
    }

    #[test]
    fn add_point_action_creates_and_extends_profile() {
        let mut app = app_with_path();
        assert!(!app.width_profiles.contains_key(&0));
        app.apply(Action::WidthPointAdd {
            shape_id: 0,
            t: 0.5,
            left: 10.0,
            right: 6.0,
        });
        let p = app.width_profiles.get(&0).expect("profile lazily created");
        assert_eq!(p.points.len(), 3, "two seeded endpoints + the new point");
        assert_eq!((p.points[1].left, p.points[1].right), (10.0, 6.0));
    }

    #[test]
    fn move_point_action_mutates_position() {
        let mut app = app_with_path();
        app.apply(Action::WidthPointAdd {
            shape_id: 0,
            t: 0.5,
            left: 8.0,
            right: 8.0,
        });
        app.apply(Action::WidthPointMove {
            shape_id: 0,
            index: 1,
            t: 0.3,
        });
        let p = app.width_profiles.get(&0).unwrap();
        assert!((p.points[1].t - 0.3).abs() < 1e-5, "moved to 0.3");
    }

    #[test]
    fn delete_point_action_removes_it() {
        let mut app = app_with_path();
        app.apply(Action::WidthPointAdd {
            shape_id: 0,
            t: 0.5,
            left: 8.0,
            right: 8.0,
        });
        assert_eq!(app.width_profiles.get(&0).unwrap().points.len(), 3);
        app.apply(Action::WidthPointDelete {
            shape_id: 0,
            index: 1,
        });
        assert_eq!(app.width_profiles.get(&0).unwrap().points.len(), 2, "back to 2");
    }

    #[test]
    fn set_widths_action_updates_sides() {
        let mut app = app_with_path();
        // Seeds a uniform profile, then overrides endpoint 0's sides.
        app.apply(Action::WidthPointSetWidths {
            shape_id: 0,
            index: 0,
            left: 12.0,
            right: 3.0,
        });
        let p = app.width_profiles.get(&0).unwrap();
        assert_eq!((p.points[0].left, p.points[0].right), (12.0, 3.0));
    }

    #[test]
    fn apply_preset_action_replaces_profile() {
        let mut app = app_with_path();
        app.apply(Action::WidthApplyPreset {
            shape_id: 0,
            preset: WidthPreset::TaperBoth,
        });
        let p = app.width_profiles.get(&0).expect("preset set a profile");
        assert_eq!(p.points.len(), 3, "taper-both has 3 width points");
        assert!(p.sample(0.0).0 < 1e-4 && p.sample(1.0).0 < 1e-4, "pointed ends");
    }

    #[test]
    fn expand_stroke_bakes_a_filled_closed_path() {
        let mut app = app_with_path();
        app.apply(Action::WidthApplyPreset {
            shape_id: 0,
            preset: WidthPreset::TaperEnd,
        });
        app.apply(Action::WidthExpandStroke { shape_id: 0 });
        match &app.doc.shapes[0] {
            Shape::Path {
                points,
                closed,
                fill,
                stroke_w,
                ..
            } => {
                assert!(*closed, "baked outline is a closed contour");
                assert!(points.len() >= 4, "real outline geometry");
                assert_eq!(*fill, [1.0, 0.0, 0.0, 1.0], "old stroke colour became fill");
                assert_eq!(*stroke_w, 0.0, "expanded shape has no stroke");
            }
            other => panic!("expected a filled path, got {other:?}"),
        }
        assert!(app.history.can_undo(), "expand is one undo step");
        assert!(!app.width_profiles.contains_key(&0), "profile baked away");
    }

    #[test]
    fn expand_with_default_uniform_has_expected_area() {
        // No profile edited → uniform half-width 2 → width 4 over length 100.
        let mut app = app_with_path();
        app.apply(Action::WidthExpandStroke { shape_id: 0 });
        let pts = match &app.doc.shapes[0] {
            Shape::Path { points, .. } => points.clone(),
            _ => panic!("expected path"),
        };
        let area = contour_area(&pts).abs();
        // Butt-capped rectangle: 100 × 4 = 400.
        assert!((area - 400.0).abs() < 1.0, "area ≈ 400, got {area}");
    }

    #[test]
    fn expand_round_cap_grows_area_past_butt() {
        let mut app = app_with_path();
        app.doc.shapes[0].stroke_style_mut().cap = LineCap::Round;
        app.apply(Action::WidthExpandStroke { shape_id: 0 });
        let pts = match &app.doc.shapes[0] {
            Shape::Path { points, .. } => points.clone(),
            _ => panic!("expected path"),
        };
        // Round caps add two half-disc bulges → area > the 400 butt rectangle.
        assert!(contour_area(&pts).abs() > 400.0, "round caps add area");
    }

    #[test]
    fn expand_out_of_range_is_noop() {
        let mut app = app_with_path();
        app.apply(Action::WidthExpandStroke { shape_id: 99 });
        assert!(!app.history.can_undo(), "bad id changes nothing");
        assert_eq!(app.doc.shapes.len(), 1);
    }

    #[test]
    fn add_point_out_of_range_is_noop() {
        let mut app = app_with_path();
        app.apply(Action::WidthPointAdd {
            shape_id: 99,
            t: 0.5,
            left: 5.0,
            right: 5.0,
        });
        assert!(app.width_profiles.is_empty(), "bad id creates no profile");
    }

    #[test]
    fn expand_converts_a_rect_via_to_path() {
        // A non-path shape (rect) is converted to a path first, then expanded.
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [10.0, 10.0, 80.0, 40.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            6.0,
        ));
        app.select_single(0);
        app.apply(Action::WidthExpandStroke { shape_id: 0 });
        match &app.doc.shapes[0] {
            Shape::Path { closed, fill, .. } => {
                assert!(*closed);
                assert_eq!(*fill, [0.0, 0.0, 1.0, 1.0], "rect's stroke became the fill");
            }
            _ => panic!("rect should expand into a filled path"),
        }
    }
}
