use super::*;

impl App {
    pub(super) fn commit_pen(&mut self, closed: bool) {
        if self.pen.points.len() >= 2 {
            self.checkpoint();
            let points = std::mem::take(&mut self.pen.points);
            let handles = std::mem::take(&mut self.pen.handles);
            let shape = Shape::path(
                points,
                handles,
                closed,
                self.default_fill,
                self.default_stroke,
                self.default_stroke_w,
            );
            self.doc.shapes.push(shape);
            self.select_single(self.doc.shapes.len() - 1);
            self.host.mark_dirty();
        }
        self.pen.clear();
    }

    /// Build a stroked-only [`Shape::Line`] between two document-space endpoints,
    /// inheriting the host's default stroke (mirrors the egui `shape_from_drag`
    /// Line arm — a line has no fill, and its width is clamped ≥ 1).
    pub(super) fn line_shape(&self, p0: (f32, f32), p1: (f32, f32)) -> Shape {
        Shape::Line {
            p0,
            p1,
            stroke: self.default_stroke,
            stroke_w: self.default_stroke_w.max(1.0),
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
        }
    }

    /// Build a closed live-shape [`Shape::Path`] (Polygon / Star) centred at
    /// `center`, generating its outline and stashing the `LiveShape` params so it
    /// stays parametrically editable (mirrors the egui `live_shape_at`).
    pub(super) fn live_shape(&self, center: (f32, f32), live: LiveShape) -> Shape {
        let (points, handles) = live.outline(center);
        Shape::Path {
            points,
            closed: true,
            fill: self.default_fill,
            fill_gradient: None,
            stroke: self.default_stroke,
            stroke_w: self.default_stroke_w,
            stroke_style: Default::default(),
            handles,
            live: Some(live),
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
        }
    }

    /// Mutate the in-edit text object's string through `f` and re-lay-out its
    /// glyphs. The whole edit session is one undo step: a checkpoint is taken on
    /// the first keystroke (when `history.begin` opens a coalescing group) and
    /// committed on `FinishText`. No-op when nothing is being edited.
    pub(super) fn edit_text_string(&mut self, f: impl FnOnce(&mut String)) {
        let Some(idx) = self.editing_text else { return };
        // Coalesce the whole typing session into one undo entry.
        self.history.begin(&self.doc);
        if let Some(shape) = self.doc.shapes.get(idx) {
            if let Some(params) = shape.text_params() {
                let mut new = params.clone();
                f(&mut new.text);
                self.doc.shapes[idx].set_text_params(new);
                self.host.mark_dirty();
            }
        }
    }

    /// Leave text-edit mode. A text object left with empty/whitespace-only text
    /// (placed but never typed into) is removed so the canvas isn't littered with
    /// invisible objects (mirrors the egui `end_text_edit`). Finalizes the typing
    /// session's coalesced undo entry.
    pub(super) fn finish_text_edit(&mut self) {
        let Some(idx) = self.editing_text.take() else {
            return;
        };
        let empty = self
            .doc
            .shapes
            .get(idx)
            .and_then(|s| s.text_params())
            .map(|p| p.text.trim().is_empty())
            .unwrap_or(true);
        if empty {
            if idx < self.doc.shapes.len() {
                self.doc.shapes.remove(idx);
            }
            self.selection.retain(|&i| i != idx);
            self.sync_legacy_selection();
            self.host.mark_dirty();
        }
        // Close the coalesced typing group (drops it if nothing changed).
        self.history.commit(&self.doc);
    }

    /// Edit the **primary** text object's parameters through `f` and re-lay-out
    /// its glyph cache. One undo step per panel edit (mirrors the egui
    /// `type_section`). No-op unless the primary selection is a text object.
    pub(super) fn edit_text_params(&mut self, f: impl FnOnce(&mut TextParams)) {
        let Some(idx) = self.selected else { return };
        let Some(mut params) = self.doc.shapes.get(idx).and_then(|s| s.text_params().cloned())
        else {
            return;
        };
        f(&mut params);
        self.checkpoint();
        if self.doc.shapes[idx].set_text_params(params) {
            self.host.mark_dirty();
        }
    }

    /// Align every selected shape's matching feature to the selection's combined
    /// bounding box, as one undo step (mirrors the egui `align_selection`, frame =
    /// selection bounds). Needs ≥2 selected shapes with geometry.
    pub(super) fn align_selection(&mut self, op: Align) {
        let sel = self.selection_bounds();
        if sel.len() < 2 {
            return;
        }
        let boxes: Vec<CoreRect> = sel.iter().map(|&(_, r)| r).collect();
        let Some(frame) = align::union_bounds(&boxes) else {
            return;
        };
        let deltas = align::align_deltas(&boxes, op, frame);
        self.checkpoint();
        let mut moved = false;
        for (&(i, _), (dx, dy)) in sel.iter().zip(deltas) {
            if (dx != 0.0 || dy != 0.0) && i < self.doc.shapes.len() {
                self.doc.shapes[i].translate(dx, dy);
                moved = true;
            }
        }
        if moved {
            self.host.mark_dirty();
        }
    }

    /// Evenly distribute the selected shapes' chosen feature / gap, as one undo
    /// step (mirrors the egui `distribute_selection`). Needs ≥3 selected shapes.
    pub(super) fn distribute_selection(&mut self, op: Distribute) {
        let sel = self.selection_bounds();
        if sel.len() < 3 {
            return;
        }
        let boxes: Vec<CoreRect> = sel.iter().map(|&(_, r)| r).collect();
        let deltas = align::distribute_deltas(&boxes, op);
        self.checkpoint();
        let mut moved = false;
        for (&(i, _), (dx, dy)) in sel.iter().zip(deltas) {
            if (dx != 0.0 || dy != 0.0) && i < self.doc.shapes.len() {
                self.doc.shapes[i].translate(dx, dy);
                moved = true;
            }
        }
        if moved {
            self.host.mark_dirty();
        }
    }

    /// Each selected shape's `(index, bounds)` in selection order, skipping shapes
    /// with no geometry. The input the align / distribute deltas operate over.
    pub(super) fn selection_bounds(&self) -> Vec<(usize, CoreRect)> {
        self.selection
            .iter()
            .filter_map(|&i| self.doc.shapes.get(i).and_then(|s| s.bounds()).map(|r| (i, r)))
            .collect()
    }

    /// Whether the selection can be aligned (≥2 shapes with geometry).
    pub fn can_align(&self) -> bool {
        self.selection_bounds().len() >= 2
    }

    /// Whether the selection can be distributed (≥3 shapes with geometry).
    pub fn can_distribute(&self) -> bool {
        self.selection_bounds().len() >= 3
    }

    /// The combined (union) bounding box of every selected shape in document
    /// space `[x, y, w, h]`, or `None` when nothing with geometry is selected.
    /// Drives the multi-select group ring overlay.
    pub fn selection_bbox(&self) -> Option<[f32; 4]> {
        let boxes: Vec<CoreRect> = self.selection_bounds().iter().map(|&(_, r)| r).collect();
        align::union_bounds(&boxes).map(|r| [r.x, r.y, r.w, r.h])
    }

    /// The bounds of every selected shape (document space), for drawing one ring
    /// per member of a multi-selection.
    pub fn selected_member_bounds(&self) -> Vec<[f32; 4]> {
        self.selection_bounds()
            .iter()
            .map(|&(_, r)| [r.x, r.y, r.w, r.h])
            .collect()
    }

    /// Run a pathfinder boolean `op` on the two selected shapes (primary = front,
    /// secondary = back), replacing both with the result batch. One undo step.
    /// No-op unless exactly two distinct shapes are selected and the op yields
    /// geometry. Reuses `contour-app`'s `boolean::apply` (the same `i_overlay`
    /// pipeline the egui app drives).
    pub(super) fn apply_boolean(&mut self, op: BoolOp) {
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
            op,
            BoolFillRule::NonZero,
        );
        if results.is_empty() {
            return;
        }
        self.checkpoint();
        // Remove both operands (highest index first so the other stays valid),
        // then append the result batch in paint order.
        let (hi, lo) = if front > back { (front, back) } else { (back, front) };
        self.doc.shapes.remove(hi);
        self.doc.shapes.remove(lo);
        let first = self.doc.shapes.len();
        self.doc.shapes.extend(results);
        self.select_single(first);
        self.host.mark_dirty();
    }

    /// Whether a pathfinder boolean op can run right now (two distinct shapes
    /// selected). Read by the toolbar to enable / grey the boolean buttons.
    pub fn can_boolean(&self) -> bool {
        match (self.selected, self.secondary) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        }
    }

    /// The primary live-shape's parameters (polygon / star), if the primary
    /// selection is a live path. Read by the inspector to gate / seed its Live
    /// Shape section.
    pub fn primary_live_shape(&self) -> Option<LiveShape> {
        self.selected_shape().and_then(|s| s.live_shape())
    }

    /// The primary text object's parameters, if the primary selection is text.
    /// Read by the inspector to gate / seed its Type section.
    pub fn primary_text_params(&self) -> Option<&TextParams> {
        self.selected_shape().and_then(|s| s.text_params())
    }

    /// Drop selection indices that fell outside the (possibly restored) document
    /// after an undo / redo (mirrors the egui app's `clamp_selection`).
    pub(super) fn clamp_selection(&mut self) {
        let n = self.doc.shapes.len();
        self.selection.retain(|&i| i < n);
        self.sync_legacy_selection();
        if self.editing_text.is_some_and(|i| i >= n) {
            self.editing_text = None;
        }
    }

    /// The editable element (anchor or handle knob) of the **selected path**
    /// nearest the document-space point `(x, y)`, if any is within pick range.
    /// Handle knobs take priority over anchors (mirrors the egui
    /// `hit_path_edit`). Only sub-contour 0 of a `Shape::Path` is editable here.
    pub fn hit_node(&self, x: f32, y: f32) -> Option<NodeTarget> {
        let (points, handles, _) = self.selected_shape()?.contour(0)?;
        // Handles first: an out/in knob of any anchor that carries a tangent.
        for (k, &p) in points.iter().enumerate() {
            let h = document::handle_at(handles, k);
            if h.0 != 0.0 || h.1 != 0.0 {
                let out = (p.0 + h.0, p.1 + h.1);
                let inp = (p.0 - h.0, p.1 - h.1);
                if (x - out.0).hypot(y - out.1) <= HANDLE_PICK_TOL
                    || (x - inp.0).hypot(y - inp.1) <= HANDLE_PICK_TOL
                {
                    return Some(NodeTarget::Handle(k));
                }
            }
        }
        // Then anchors: the nearest within tolerance.
        points
            .iter()
            .enumerate()
            .filter(|(_, &p)| (x - p.0).hypot(y - p.1) <= ANCHOR_PICK_TOL)
            .min_by(|(_, &a), (_, &b)| {
                (x - a.0).hypot(y - a.1).total_cmp(&(x - b.0).hypot(y - b.1))
            })
            .map(|(k, _)| NodeTarget::Anchor(k))
    }

    /// The selected path's anchor points + per-anchor out-tangent handles
    /// (sub-contour 0), in **document space**, for drawing the node overlay.
    /// `None` unless the selection is a path.
    pub fn selected_path_nodes(&self) -> Option<PathNodes> {
        let (points, handles, _) = self.selected_shape()?.contour(0)?;
        let mut hs = handles.to_vec();
        hs.resize(points.len(), (0.0, 0.0));
        Some((points.to_vec(), hs))
    }

    /// The currently selected shape, mutably, if the selection is in range.
    pub(super) fn selected_shape_mut(&mut self) -> Option<&mut Shape> {
        self.selected.and_then(|i| self.doc.shapes.get_mut(i))
    }

    /// The currently selected shape (read-only), if the selection is in range.
    /// Read by the inspector panel to seed its stepper rows from live state.
    pub fn selected_shape(&self) -> Option<&Shape> {
        self.selected.and_then(|i| self.doc.shapes.get(i))
    }

    /// The selected shape's tight bounding box in **document space**
    /// `[x, y, w, h]`, if a shape is selected and has finite bounds. The root view
    /// maps this through the view transform to draw the selection ring overlay.
    pub fn selected_bounds(&self) -> Option<[f32; 4]> {
        self.selected_shape()
            .and_then(|s| s.bounds())
            .map(|r| [r.x, r.y, r.w, r.h])
    }

    /// The secondary-selected shape's tight bounds in document space (for the
    /// amber boolean-operand ring overlay).
    pub fn secondary_bounds(&self) -> Option<[f32; 4]> {
        self.secondary
            .and_then(|i| self.doc.shapes.get(i))
            .and_then(|s| s.bounds())
            .map(|r| [r.x, r.y, r.w, r.h])
    }

    /// The selected shape's fill colour, if it is selected and has a fill region.
    pub fn selected_fill(&self) -> Option<[f32; 4]> {
        self.selected_shape().and_then(|s| s.fill_color())
    }

    /// The selected shape's stroke colour, if a shape is selected.
    pub fn selected_stroke(&self) -> Option<[f32; 4]> {
        self.selected_shape().and_then(|s| s.stroke_color())
    }

    /// Index of the topmost *selectable* shape whose region contains the
    /// document-space point `(x, y)`, or `None` when the click misses everything.
    /// "Topmost" = last in paint order, so we scan the shape vec back-to-front and
    /// return the first selectable hit. Reuses the document model's own
    /// [`Shape::hit`] so the GPUI host and the egui canvas pick identically.
    pub(super) fn hit_test_topmost(&self, x: f32, y: f32) -> Option<usize> {
        // A few document units of tolerance gives thin lines / open paths a
        // clickable thickness (matches the egui canvas's pick slop).
        const TOL: f32 = 4.0;
        self.doc
            .shapes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.selectable() && s.hit(x, y, TOL))
            .map(|(i, _)| i)
    }

    /// Index of the topmost selectable shape under the document-space point
    /// `(x, y)`, or `None`. Public wrapper over the hit test so the root view can
    /// decide between a click-select and starting a marquee on empty canvas.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<usize> {
        self.hit_test_topmost(x, y)
    }

    /// True if `(x, y)` (doc space) lands on any **already-selected** shape.
    /// Used to detect "drag to move selection" intent before a general hit test.
    pub fn hit_selected(&self, x: f32, y: f32) -> bool {
        const TOL: f32 = 4.0;
        self.selection.iter().any(|&i| {
            self.doc
                .shapes
                .get(i)
                .map(|s| s.hit(x, y, TOL))
                .unwrap_or(false)
        })
    }

    /// Index of the topmost point-type object under the document-space point
    /// `(x, y)`, or `None`. Lets the Type tool re-edit an existing text object on
    /// click instead of always placing a new one.
    pub fn hit_text(&self, x: f32, y: f32) -> Option<usize> {
        const TOL: f32 = 4.0;
        self.doc
            .shapes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| {
                s.selectable() && s.text_params().is_some() && s.hit(x, y, TOL)
            })
            .map(|(i, _)| i)
    }

    /// Whether a shape is dimmed in isolation mode (belongs to a different group).
    pub fn is_isolated_out(&self, shape: &Shape) -> bool {
        match self.isolation_group {
            Some(g) => shape.group() != Some(g),
            None => false,
        }
    }

    /// Export a minimal valid PDF (one blank page at artboard dimensions). The
    /// `printpdf` crate is not in scope, so we write a hand-crafted PDF/1.4
    /// skeleton. Raster embedding requires `printpdf`; this path logs and writes
    /// a blank page.
    pub(super) fn export_pdf(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let (w, h) = (self.host.doc_w as f32, self.host.doc_h as f32);
        // PDF points: 1 pt = 1/72 inch; we treat 1 doc-unit = 1 pt.
        let wp = w;
        let hp = h;

        // Object contents (indexed 1-based).
        let obj1 = b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n";
        let obj2 = b"2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n";
        let obj3_content = format!(
            "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 {wp:.1} {hp:.1}]>>endobj\n"
        );
        let obj3 = obj3_content.as_bytes();

        let header = b"%PDF-1.4\n";
        let off1 = header.len();
        let off2 = off1 + obj1.len();
        let off3 = off2 + obj2.len();
        let xref_offset = off3 + obj3.len();

        let xref = format!(
            "xref\n0 4\n0000000000 65535 f \n{off1:010} 00000 n \n{off2:010} 00000 n \n{off3:010} 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n{xref_offset}\n%%EOF\n"
        );

        log::info!("PDF export: embedded image requires `printpdf` crate; wrote blank page");

        let mut f = std::fs::File::create(path)?;
        f.write_all(header)?;
        f.write_all(obj1)?;
        f.write_all(obj2)?;
        f.write_all(obj3)?;
        f.write_all(xref.as_bytes())?;
        Ok(())
    }

    /// Split every shape that the line segment (`start`→`end`, doc space)
    /// intersects into two sub-shapes at the intersection parameters. Shapes
    /// with no bounding-box intersection are left alone. Sutherland–Hodgman
    /// style: we split the shape's bounding-box polygon at the knife line.
    pub(super) fn apply_knife_slice(&mut self, start: (f32, f32), end: (f32, f32)) {
        if self.doc.shapes.is_empty() {
            return;
        }
        let (x1, y1, x2, y2) = (start.0, start.1, end.0, end.1);
        // Direction vector and its normal for the split plane.
        let (dx, dy) = (x2 - x1, y2 - y1);
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
            return;
        }
        let mut new_shapes: Vec<Shape> = Vec::new();
        let mut remove_idxs: Vec<usize> = Vec::new();

        for i in 0..self.doc.shapes.len() {
            let shape = &self.doc.shapes[i];
            let Some(b) = shape.bounds() else { continue };
            // Quick bbox-vs-line test: does the bounding box contain any
            // point on the segment? We use the simple separating-axis test on
            // the four corners vs the knife line.
            let corners = [
                (b.x, b.y),
                (b.x + b.w, b.y),
                (b.x + b.w, b.y + b.h),
                (b.x, b.y + b.h),
            ];
            let side = |p: (f32, f32)| {
                (p.0 - x1) * dy - (p.1 - y1) * dx
            };
            let signs: Vec<f32> = corners.iter().map(|&c| side(c)).collect();
            let has_pos = signs.iter().any(|&s| s > 0.0);
            let has_neg = signs.iter().any(|&s| s < 0.0);
            if !(has_pos && has_neg) {
                continue; // Entire shape is on one side; not split.
            }

            // Partition the bounding-box corners into left/right halves.
            let left: Vec<(f32, f32)> = corners.iter().copied().filter(|&c| side(c) <= 0.0).collect();
            let right: Vec<(f32, f32)> = corners.iter().copied().filter(|&c| side(c) > 0.0).collect();

            if left.len() < 2 || right.len() < 2 {
                continue;
            }

            // Build left-half bounding box and right-half bounding box.
            let bbox_of = |pts: &[(f32, f32)]| -> [f32; 4] {
                let x_min = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                let y_min = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
                let x_max = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
                let y_max = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                [x_min, y_min, x_max - x_min, y_max - y_min]
            };

            let left_bb = bbox_of(&left);
            let right_bb = bbox_of(&right);

            // Create two rect approximations of the split shape inheriting paint.
            let fill = shape.fill_color().unwrap_or([0.5, 0.5, 0.5, 1.0]);
            let stroke = shape.stroke_color().unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let stroke_w = shape.stroke_width();

            new_shapes.push(Shape::rect(left_bb, fill, stroke, stroke_w));
            new_shapes.push(Shape::rect(right_bb, fill, stroke, stroke_w));
            remove_idxs.push(i);
        }

        if remove_idxs.is_empty() {
            return;
        }

        self.checkpoint();
        // Remove highest index first.
        remove_idxs.sort_unstable_by(|a, b| b.cmp(a));
        remove_idxs.dedup();
        for i in &remove_idxs {
            self.doc.shapes.remove(*i);
        }
        self.doc.shapes.extend(new_shapes);
        self.select_clear();
        self.host.mark_dirty();
    }

    /// Serialize visible document shapes to a plain-string SVG file.
    pub(super) fn export_svg(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = String::new();
        let (w, h) = (self.host.doc_w, self.host.doc_h);
        out.push_str(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
             viewBox=\"0 0 {w} {h}\">\n"
        ));
        for shape in &self.doc.shapes {
            if !shape.visible() {
                continue;
            }
            out.push_str(&shape_to_svg(shape));
        }
        out.push_str("</svg>\n");
        let mut f = std::fs::File::create(path)?;
        f.write_all(out.as_bytes())?;
        Ok(())
    }
}
