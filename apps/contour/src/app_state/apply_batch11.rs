//! Batch 11 real-geometry apply helpers: text→outlines, the full Pathfinder
//! (Divide / Trim / Merge / Crop / Outline / Minus Back + the four shape modes),
//! confirming a perspective distort, the warp-preset envelope distort, and the
//! Live Paint bucket fill.
//!
//! Each helper here is a `pub(super) fn` on `impl App`; the dispatch arms in the
//! `apply_*` chain call into these so the heavy geometry stays out of the already
//! large dispatcher files. Pure geometry lives in
//! [`super::geometry_warp`] (warp meshes, mesh deform, Live-Paint region detect);
//! Pathfinder geometry reuses [`crate::boolean`].

use super::geometry_warp::{live_paint_region, warp_preset_mesh, warp_shape_with_mesh};
use super::{App, EnvelopeWarpStyle, PathfinderOp};
use crate::boolean::{self, BoolFillRule, BoolOp};

impl App {
    /// **Object ▸ Type ▸ Create Outlines** — convert the text object at `idx` into
    /// an editable [`crate::document::Shape::Compound`] of its glyph outlines,
    /// keeping the text's paint / style and filling under even-odd so counters
    /// stay as holes. No-op on a non-text shape or an out-of-range index. One undo
    /// step.
    pub(super) fn apply_outline_text(&mut self, idx: usize) {
        let Some(shape) = self.doc.shapes.get(idx) else { return };
        if shape.text_params().is_none() {
            return; // only text objects outline
        }
        let outlined = shape.text_to_outlines();
        self.checkpoint();
        self.doc.shapes[idx] = outlined;
        self.select_single(idx);
        self.host.mark_dirty();
    }

    /// Run a full **Pathfinder** operation on the two selected shapes (primary =
    /// front, secondary = back), replacing both with the resulting geometry batch.
    /// The shape modes (Unite / Minus / Intersect / Exclude) and the effect ops
    /// (Divide / Trim / Merge / Crop / Outline / Minus Back) all route through the
    /// shared [`crate::boolean`] pipeline. Records `op` as the last-used Pathfinder
    /// for **Repeat**. No-op unless two distinct shapes are selected and the op
    /// yields geometry. One undo step.
    pub(super) fn apply_pathfinder_op(&mut self, op: PathfinderOp) {
        self.last_pathfinder_op = Some(op);
        let bool_op = pathfinder_to_bool(op);
        let (Some(front), Some(back)) = (self.selected, self.secondary) else {
            return;
        };
        if front == back || front >= self.doc.shapes.len() || back >= self.doc.shapes.len() {
            return;
        }
        // `apply(subj=back, clip=front)` — subj is the lower shape, clip the upper.
        let results = boolean::apply(
            &self.doc.shapes[back],
            &self.doc.shapes[front],
            bool_op,
            BoolFillRule::NonZero,
        );
        if results.is_empty() {
            return;
        }
        self.checkpoint();
        let (hi, lo) = if front > back { (front, back) } else { (back, front) };
        self.doc.shapes.remove(hi);
        self.doc.shapes.remove(lo);
        let first = self.doc.shapes.len();
        self.doc.shapes.extend(results);
        self.select_single(first);
        self.host.mark_dirty();
    }

    /// Confirm the on-canvas **Perspective / Free Distort**: warp the active
    /// shape's geometry through the homography defined by the dragged corners.
    /// Routes through the existing [`Action::ApplyPerspectiveDistort`] geometry so
    /// the same homography path drives both the one-click and the interactive
    /// flows. Clears the editing flag afterwards.
    ///
    /// [`Action::ApplyPerspectiveDistort`]: super::Action::ApplyPerspectiveDistort
    pub(super) fn apply_confirm_perspective(&mut self) {
        if let Some(idx) = self.selected {
            self.apply(super::Action::ApplyPerspectiveDistort { shape_id: idx });
        }
        self.perspective_distort_active = false;
        self.host.mark_dirty();
    }

    /// **Object ▸ Envelope Distort ▸ Make with Warp** — deform the selected shape's
    /// geometry through a warp-preset bilinear mesh (Arc / Bulge / Flag / Twist /
    /// …), built from the current [`EnvelopeConfig`]. The shape is replaced by its
    /// warped path/compound and its source mesh is stored on the shape so it can be
    /// released / expanded later. No-op without a selection or warpable geometry.
    /// One undo step.
    ///
    /// [`EnvelopeConfig`]: super::EnvelopeConfig
    pub(super) fn apply_make_envelope_warp(&mut self) {
        let Some(idx) = self.selected else { return };
        if idx >= self.doc.shapes.len() {
            return;
        }
        let Some(b) = self.doc.shapes[idx].bounds() else { return };
        let bbox = [b.x, b.y, b.w, b.h];
        let cfg = &self.envelope_config;
        // `bend` is Illustrator's −100..100; normalise to −1..1.
        let bend = (cfg.bend / 100.0).clamp(-1.0, 1.0);
        // Mesh resolution scales with the "fidelity" slider (smoother at higher).
        let res = (4 + (cfg.fidelity / 20.0) as u32).clamp(2, 12);
        let mesh = warp_preset_mesh(cfg.warp_style, bbox, bend, cfg.horizontal, res, res);

        let path = self.doc.shapes[idx].to_path();
        let Some(mut warped) = warp_shape_with_mesh(&path, bbox, &mesh) else { return };
        // Remember the mesh on the shape for a later release / re-edit.
        warped.set_envelope_mesh(Some(mesh));
        self.checkpoint();
        self.doc.shapes[idx] = warped;
        if !self.envelope_applied_shapes.contains(&idx) {
            self.envelope_applied_shapes.push(idx);
        }
        self.host.mark_dirty();
    }

    /// **Object ▸ Envelope Distort ▸ Expand** — bake the envelope: drop the stored
    /// mesh from every enveloped shape so the warped geometry becomes a plain
    /// editable path (the warp is already applied to the points). Clears the
    /// applied-shapes tracking. One undo step when anything changes.
    pub(super) fn apply_expand_envelope(&mut self) {
        if self.envelope_applied_shapes.is_empty() {
            return;
        }
        self.checkpoint();
        for &idx in &self.envelope_applied_shapes {
            if let Some(s) = self.doc.shapes.get_mut(idx) {
                s.set_envelope_mesh(None);
            }
        }
        self.envelope_applied_shapes.clear();
        self.host.mark_dirty();
    }

    /// **Live Paint bucket** — fill the closed region under document-space point
    /// `(x, y)` (delimited by the overlapping path outlines) with `color`, adding
    /// the detected face as a new top shape. Falls back to recolouring the topmost
    /// shape under the point when no bounded face is found (an isolated outline).
    /// No-op when the point is in empty space. One undo step.
    pub(super) fn apply_live_paint_fill(&mut self, x: f32, y: f32, color: [f32; 4]) {
        if let Some(region) = live_paint_region(&self.doc.shapes, x, y, color) {
            self.checkpoint();
            self.doc.shapes.push(region);
            let idx = self.doc.shapes.len() - 1;
            self.select_single(idx);
            self.host.mark_dirty();
            return;
        }
        // Fallback: no enclosed face — recolour the topmost shape hit (if any),
        // so a click on a single isolated outline still paints it.
        if let Some(idx) = (0..self.doc.shapes.len())
            .rev()
            .find(|&i| self.doc.shapes[i].hit(x, y, 2.0))
        {
            self.checkpoint();
            self.doc.shapes[idx].set_fill_color(color);
            self.host.mark_dirty();
        }
    }
}

/// Map a [`PathfinderOp`] to the [`crate::boolean::BoolOp`] that realises it.
fn pathfinder_to_bool(op: PathfinderOp) -> BoolOp {
    match op {
        PathfinderOp::Unite => BoolOp::Union,
        PathfinderOp::Minus => BoolOp::Difference,
        PathfinderOp::Intersect => BoolOp::Intersect,
        PathfinderOp::Exclude => BoolOp::Exclude,
        PathfinderOp::Divide => BoolOp::Divide,
        PathfinderOp::Trim => BoolOp::Trim,
        PathfinderOp::Merge => BoolOp::Merge,
        PathfinderOp::Crop => BoolOp::Crop,
        PathfinderOp::Outline => BoolOp::Outline,
        PathfinderOp::MinusBack => BoolOp::MinusBack,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::Action;
    use crate::document::Shape;

    fn rect(x: f32, y: f32, w: f32, h: f32, fill: [f32; 4]) -> Shape {
        Shape::rect([x, y, w, h], fill, [0.0, 0.0, 0.0, 1.0], 1.0)
    }

    /// Net filled area of a shape (outer ring minus holes for a compound).
    fn shape_area(s: &Shape) -> f32 {
        let area = |pts: &[(f32, f32)]| {
            let n = pts.len();
            let mut a = 0.0;
            for i in 0..n {
                let (x0, y0) = pts[i];
                let (x1, y1) = pts[(i + 1) % n];
                a += x0 * y1 - x1 * y0;
            }
            (a * 0.5).abs()
        };
        match s {
            Shape::Path { points, .. } => area(points),
            Shape::Compound { subpaths, .. } => {
                let mut areas: Vec<f32> = subpaths.iter().map(|sp| area(&sp.flatten())).collect();
                areas.sort_by(|a, b| b.partial_cmp(a).unwrap());
                let outer = areas.first().copied().unwrap_or(0.0);
                outer - areas.iter().skip(1).sum::<f32>()
            }
            _ => 0.0,
        }
    }

    fn total_area(shapes: &[Shape], from: usize) -> f32 {
        shapes[from..].iter().map(shape_area).sum()
    }

    // ---- Outline Text -------------------------------------------------------

    #[test]
    fn outline_text_converts_to_compound() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A text object via the document helper used elsewhere in the suite.
        let params = crate::text::TextParams {
            text: "Ab".to_string(),
            ..Default::default()
        };
        let (glyphs, _) = crate::text::layout(&params, (0.0, 0.0));
        app.doc.shapes.push(Shape::Text {
            params,
            origin: (0.0, 0.0),
            glyphs,
            fill: [0.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
            stroke_style: Default::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        });
        app.select_single(0);
        app.apply(Action::OutlineText(0));
        assert!(
            matches!(app.doc.shapes[0], Shape::Compound { .. }),
            "text became a compound of glyph outlines"
        );
        assert!(app.history.can_undo(), "OutlineText is undoable");
    }

    #[test]
    fn outline_text_noop_on_non_text() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]));
        app.select_single(0);
        app.apply(Action::OutlineText(0));
        assert!(matches!(app.doc.shapes[0], Shape::Rect { .. }), "rect untouched");
        assert!(!app.history.can_undo(), "no-op leaves no undo step");
    }

    // ---- Pathfinder ---------------------------------------------------------

    fn two_overlapping(app: &mut App) {
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0])); // back
        app.doc.shapes.push(rect(5.0, 5.0, 10.0, 10.0, [0.0, 0.0, 1.0, 1.0])); // front
        app.selection = vec![0, 1]; // back then front; primary=front=1, secondary=back=0
        app.sync_legacy_selection();
    }

    #[test]
    fn pathfinder_divide_tiles_the_union() {
        let mut app = App::new();
        two_overlapping(&mut app);
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Divide));
        // overlap + (a−b) + (b−a) = three faces tiling 175.
        assert_eq!(app.doc.shapes.len(), 3, "divide into three faces");
        assert!((total_area(&app.doc.shapes, 0) - 175.0).abs() < 0.6);
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Divide));
    }

    #[test]
    fn pathfinder_trim_keeps_front_and_trims_back() {
        let mut app = App::new();
        two_overlapping(&mut app);
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Trim));
        assert_eq!(app.doc.shapes.len(), 2, "front + trimmed back");
        assert!((total_area(&app.doc.shapes, 0) - 175.0).abs() < 0.6);
    }

    #[test]
    fn pathfinder_merge_welds_same_color() {
        let mut app = App::new();
        app.doc.shapes.clear();
        let c = [0.2, 0.7, 0.3, 1.0];
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, c));
        app.doc.shapes.push(rect(5.0, 5.0, 10.0, 10.0, c));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Merge));
        assert_eq!(app.doc.shapes.len(), 1, "same colour welds into one region");
        assert!((shape_area(&app.doc.shapes[0]) - 175.0).abs() < 0.6);
    }

    #[test]
    fn pathfinder_crop_keeps_overlap() {
        let mut app = App::new();
        two_overlapping(&mut app);
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Crop));
        assert_eq!(app.doc.shapes.len(), 1);
        assert!((shape_area(&app.doc.shapes[0]) - 25.0).abs() < 0.6);
    }

    #[test]
    fn pathfinder_minus_back_keeps_front() {
        let mut app = App::new();
        two_overlapping(&mut app);
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::MinusBack));
        assert_eq!(app.doc.shapes.len(), 1);
        // front − back = 100 − 25 = 75.
        assert!((shape_area(&app.doc.shapes[0]) - 75.0).abs() < 0.6);
    }

    #[test]
    fn pathfinder_outline_emits_strokes() {
        let mut app = App::new();
        two_overlapping(&mut app);
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Outline));
        assert!(!app.doc.shapes.is_empty());
        for s in &app.doc.shapes {
            assert_eq!(s.fill_color(), Some([0.0, 0.0, 0.0, 0.0]), "unfilled");
        }
    }

    #[test]
    fn pathfinder_noop_without_two_shapes() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]));
        app.select_single(0);
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Divide));
        assert_eq!(app.doc.shapes.len(), 1, "single shape: no pathfinder");
        assert!(!app.history.can_undo());
    }

    // ---- Envelope warp ------------------------------------------------------

    #[test]
    fn envelope_warp_distorts_and_tracks() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 100.0, 100.0, [1.0, 0.0, 0.0, 1.0]));
        app.select_single(0);
        app.envelope_config.warp_style = EnvelopeWarpStyle::Arc;
        app.envelope_config.bend = 60.0;
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        // The shape is now a warped path that carries its source mesh.
        assert!(app.envelope_applied_shapes.contains(&0));
        assert!(
            app.doc.shapes[0].envelope_mesh().is_some(),
            "warped shape stores its mesh for release/expand"
        );
        assert!(app.history.can_undo());
    }

    #[test]
    fn expand_envelope_drops_mesh() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 100.0, 100.0, [1.0, 0.0, 0.0, 1.0]));
        app.select_single(0);
        app.envelope_config.warp_style = EnvelopeWarpStyle::Bulge;
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(app.doc.shapes[0].envelope_mesh().is_some());
        app.apply(Action::ExpandEnvelope);
        assert!(
            app.doc.shapes[0].envelope_mesh().is_none(),
            "expand bakes the warp and drops the mesh"
        );
        assert!(app.envelope_applied_shapes.is_empty());
    }

    // ---- Live Paint ---------------------------------------------------------

    #[test]
    fn live_paint_fills_enclosed_face() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]));
        app.doc.shapes.push(rect(5.0, 5.0, 10.0, 10.0, [0.0, 1.0, 0.0, 1.0]));
        let before = app.doc.shapes.len();
        app.apply(Action::LivePaintFill { x: 7.5, y: 7.5, color: [0.0, 0.0, 1.0, 1.0] });
        assert_eq!(app.doc.shapes.len(), before + 1, "a face shape was added");
        let added = app.doc.shapes.last().unwrap();
        assert_eq!(added.fill_color(), Some([0.0, 0.0, 1.0, 1.0]));
        assert!((shape_area(added) - 25.0).abs() < 0.6, "overlap face ~25");
    }

    #[test]
    fn apply_live_paint_action_also_fills() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]));
        app.doc.shapes.push(rect(5.0, 5.0, 10.0, 10.0, [0.0, 1.0, 0.0, 1.0]));
        let before = app.doc.shapes.len();
        app.live_paint_hit = Some((7.5, 7.5));
        app.apply(Action::ApplyLivePaint { fill: [0.5, 0.5, 0.5, 1.0] });
        assert_eq!(app.doc.shapes.len(), before + 1);
        assert_eq!(app.doc.shapes.last().unwrap().fill_color(), Some([0.5, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn live_paint_open_space_is_noop() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(rect(0.0, 0.0, 10.0, 10.0, [1.0, 0.0, 0.0, 1.0]));
        let before = app.doc.shapes.len();
        app.apply(Action::LivePaintFill { x: 100.0, y: 100.0, color: [0.0, 0.0, 1.0, 1.0] });
        assert_eq!(app.doc.shapes.len(), before, "click in empty space adds nothing");
        assert!(!app.history.can_undo());
    }
}
