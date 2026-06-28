//! The apply layer for the **Shape Builder** tool: wires the pure planar-region
//! geometry in [`super`] onto `impl App`.
//!
//! Entering the tool snapshots the current selection's overlapping outlines into
//! a [`ShapeBuilderSession`] of atomic faces (an editing overlay — the document
//! is untouched until commit). Merge welds a set of faces, delete drops a face,
//! and commit replaces the source shapes with the resulting faces as new
//! document shapes (one undo step).
//!
//! This is the **terminal** stage of the `Action` dispatch chain: it is reached
//! from `apply_width_tool`'s catch-all, and its own `_ => {}` is the chain's final
//! no-op.

use super::{build_regions, merge_regions, region_at, Region};
use crate::app_state::{Action, App};
use crate::document::Shape;

/// An in-progress Shape Builder session: the source shapes that were selected on
/// entry plus the live list of atomic faces the user is welding / deleting. Pure
/// overlay state on `App` — the document changes only on
/// [`commit`](App::shape_builder_commit).
#[derive(Clone, Debug, Default)]
pub struct ShapeBuilderSession {
    /// Paint-order indices of the shapes the arrangement was built from (sorted),
    /// removed from the document on commit.
    pub source_ids: Vec<usize>,
    /// The current atomic faces (the planar arrangement), edited by merge/delete.
    pub regions: Vec<Region>,
}

impl App {
    /// Terminal dispatcher for the Shape Builder actions. Reached from
    /// `apply_width_tool`'s catch-all; owns the dispatch chain's final no-op.
    pub(in crate::app_state) fn apply_shape_builder(&mut self, action: Action) {
        match action {
            Action::EnterShapeBuilder => self.shape_builder_enter(),
            Action::ShapeBuilderMergeRegions(idxs) => self.shape_builder_merge(idxs),
            Action::ShapeBuilderDeleteRegion(i) => self.shape_builder_delete(i),
            Action::ShapeBuilderCommit => self.shape_builder_commit(),
            Action::ShapeBuilderCancel => {
                self.shape_builder = None;
            }
            // The chain's final no-op: nothing downstream handles this action.
            _ => {}
        }
    }

    /// Enter the Shape Builder: build the planar arrangement of the current
    /// selection's overlapping shapes into a session overlay. No-op for an empty
    /// selection or a selection whose shapes carve no fillable face. Does not
    /// touch the document (no checkpoint) — only commit does.
    fn shape_builder_enter(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let mut ids: Vec<usize> = self
            .selection
            .iter()
            .copied()
            .filter(|&i| i < self.doc.shapes.len())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let shapes: Vec<Shape> = ids.iter().map(|&i| self.doc.shapes[i].clone()).collect();
        let regions = build_regions(&shapes);
        if regions.is_empty() {
            return;
        }
        self.shape_builder = Some(ShapeBuilderSession {
            source_ids: ids,
            regions,
        });
    }

    /// The face index under the document-space point `(x, y)` in the active
    /// session, for the canvas to pick a region. `None` with no session or no hit.
    pub fn shape_builder_region_at(&self, x: f32, y: f32) -> Option<usize> {
        self.shape_builder
            .as_ref()
            .and_then(|s| region_at(&s.regions, x, y))
    }

    /// **Merge**: union the listed face indices into one combined face, replacing
    /// them in the session. Needs at least two valid, distinct indices. No-op
    /// without a session or fewer than two faces selected.
    fn shape_builder_merge(&mut self, mut idxs: Vec<usize>) {
        let Some(session) = self.shape_builder.as_mut() else {
            return;
        };
        idxs.retain(|&i| i < session.regions.len());
        idxs.sort_unstable();
        idxs.dedup();
        if idxs.len() < 2 {
            return;
        }
        let selected: Vec<Region> = idxs.iter().map(|&i| session.regions[i].clone()).collect();
        let Some(merged) = merge_regions(&selected) else {
            return;
        };
        // Drop the merged faces (highest index first so the rest stay valid),
        // then append the welded face.
        for &i in idxs.iter().rev() {
            session.regions.remove(i);
        }
        session.regions.push(Region::new(merged));
    }

    /// **Delete** (Alt-click semantics): drop the face at `index` from the
    /// session, subtracting it from the result. No-op without a session or for an
    /// out-of-range index.
    fn shape_builder_delete(&mut self, index: usize) {
        let Some(session) = self.shape_builder.as_mut() else {
            return;
        };
        if index < session.regions.len() {
            session.regions.remove(index);
        }
    }

    /// **Commit**: replace the session's source shapes with its faces as new
    /// document shapes, ending the session. One undo step; the new shapes become
    /// the selection. No-op (just clears the session) when nothing remains.
    fn shape_builder_commit(&mut self) {
        let Some(session) = self.shape_builder.take() else {
            return;
        };
        if session.regions.is_empty() {
            return;
        }
        self.checkpoint();
        // Remove the source shapes, highest index first so the lower stay valid.
        let mut ids = session.source_ids.clone();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids.dedup();
        for i in ids {
            if i < self.doc.shapes.len() {
                self.doc.shapes.remove(i);
            }
        }
        // Append the resulting faces as new shapes and select them.
        let start = self.doc.shapes.len();
        for r in session.regions {
            self.doc.shapes.push(r.shape);
        }
        self.selection = (start..self.doc.shapes.len()).collect();
        self.sync_legacy_selection();
        self.host.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Shape {
        Shape::rect([x, y, w, h], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 1.0)
    }

    /// An app whose document is exactly two overlapping rectangles, both selected.
    fn app_two_overlapping() -> App {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0));
        app.doc.shapes.push(rect(5.0, 5.0, 10.0, 10.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app
    }

    fn region_area(s: &Shape) -> f32 {
        Region::new(s.clone()).area()
    }

    #[test]
    fn enter_builds_a_session_of_faces() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        let session = app.shape_builder.as_ref().expect("session created");
        assert_eq!(session.regions.len(), 3, "three atomic faces");
        assert_eq!(session.source_ids, vec![0, 1]);
    }

    #[test]
    fn enter_empty_selection_is_noop() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.select_clear();
        app.apply(Action::EnterShapeBuilder);
        assert!(app.shape_builder.is_none(), "no selection → no session");
    }

    /// Routing guard: the action reaches the terminal `apply_shape_builder` only
    /// if every catch-all in the dispatch chain forwards it. A dropped arm would
    /// leave the session `None`.
    #[test]
    fn enter_action_routes_through_the_dispatch_chain() {
        let mut app = app_two_overlapping();
        assert!(app.shape_builder.is_none());
        app.apply(Action::EnterShapeBuilder);
        assert!(
            app.shape_builder.is_some(),
            "EnterShapeBuilder must route through the full chain and mutate state"
        );
    }

    #[test]
    fn merge_action_reduces_face_count() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        app.apply(Action::ShapeBuilderMergeRegions(vec![0, 1, 2]));
        let session = app.shape_builder.as_ref().unwrap();
        assert_eq!(session.regions.len(), 1, "all faces welded into one");
        assert!((session.regions[0].area() - 175.0).abs() < 0.5, "union area");
    }

    #[test]
    fn merge_with_one_index_is_noop() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        app.apply(Action::ShapeBuilderMergeRegions(vec![0]));
        assert_eq!(
            app.shape_builder.as_ref().unwrap().regions.len(),
            3,
            "a single index cannot merge anything"
        );
    }

    #[test]
    fn merge_without_session_is_noop() {
        let mut app = app_two_overlapping();
        // No EnterShapeBuilder first.
        app.apply(Action::ShapeBuilderMergeRegions(vec![0, 1]));
        assert!(app.shape_builder.is_none(), "no session → nothing happens");
    }

    #[test]
    fn delete_action_removes_a_face() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        // Drop the overlap face (Alt-click semantics).
        let over = app.shape_builder_region_at(7.5, 7.5).expect("overlap hit");
        app.apply(Action::ShapeBuilderDeleteRegion(over));
        let session = app.shape_builder.as_ref().unwrap();
        assert_eq!(session.regions.len(), 2, "overlap removed");
        // What remains is the symmetric difference: two crescents, area 150.
        let total: f32 = session.regions.iter().map(|r| r.area()).sum();
        assert!((total - 150.0).abs() < 0.5, "symmetric difference remains");
    }

    #[test]
    fn delete_out_of_range_is_noop() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        app.apply(Action::ShapeBuilderDeleteRegion(99));
        assert_eq!(app.shape_builder.as_ref().unwrap().regions.len(), 3);
    }

    #[test]
    fn region_at_method_picks_the_face_under_a_point() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        let over = app.shape_builder_region_at(7.5, 7.5).expect("overlap");
        let session = app.shape_builder.as_ref().unwrap();
        assert!((session.regions[over].area() - 25.0).abs() < 0.5, "the overlap face");
        assert!(app.shape_builder_region_at(-50.0, -50.0).is_none(), "miss");
    }

    #[test]
    fn commit_replaces_sources_with_faces() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        app.apply(Action::ShapeBuilderCommit);
        // Two sources removed, three faces appended.
        assert_eq!(app.doc.shapes.len(), 3, "sources → three faces");
        assert!(app.shape_builder.is_none(), "session ended");
        assert!(app.history.can_undo(), "commit is one undo step");
        // The new shapes are selected.
        assert_eq!(app.selection.len(), 3);
        // The committed shapes tile the union (area 175).
        let total: f32 = app.doc.shapes.iter().map(region_area).sum();
        assert!((total - 175.0).abs() < 0.5, "committed faces tile the union");
    }

    #[test]
    fn merge_then_commit_writes_one_shape() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        app.apply(Action::ShapeBuilderMergeRegions(vec![0, 1, 2]));
        app.apply(Action::ShapeBuilderCommit);
        assert_eq!(app.doc.shapes.len(), 1, "one welded shape committed");
        assert!((region_area(&app.doc.shapes[0]) - 175.0).abs() < 0.5);
    }

    #[test]
    fn delete_overlap_then_commit_leaves_symmetric_difference() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        let over = app.shape_builder_region_at(7.5, 7.5).unwrap();
        app.apply(Action::ShapeBuilderDeleteRegion(over));
        app.apply(Action::ShapeBuilderCommit);
        // The committed result is the two crescents (Exclude), total area 150.
        assert_eq!(app.doc.shapes.len(), 2, "two crescent faces");
        let total: f32 = app.doc.shapes.iter().map(region_area).sum();
        assert!((total - 150.0).abs() < 0.5, "exclude area committed");
    }

    #[test]
    fn commit_without_session_is_noop() {
        let mut app = app_two_overlapping();
        app.apply(Action::ShapeBuilderCommit);
        assert_eq!(app.doc.shapes.len(), 2, "document untouched");
        assert!(!app.history.can_undo(), "no checkpoint without a session");
    }

    #[test]
    fn cancel_clears_the_session_without_changing_the_document() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        assert!(app.shape_builder.is_some());
        app.apply(Action::ShapeBuilderCancel);
        assert!(app.shape_builder.is_none(), "session discarded");
        assert_eq!(app.doc.shapes.len(), 2, "document unchanged");
        assert!(!app.history.can_undo(), "cancel is not an undo step");
    }

    #[test]
    fn enter_does_not_change_the_document_or_history() {
        let mut app = app_two_overlapping();
        app.apply(Action::EnterShapeBuilder);
        assert_eq!(app.doc.shapes.len(), 2, "entering is overlay-only");
        assert!(!app.history.can_undo(), "no checkpoint on enter");
    }
}
