//! Batch 12 apply logic: Pattern Brush, real Chart geometry, Document Setup, and
//! Symbol Edit Mode. All geometry lives in the dedicated modules
//! ([`crate::pattern_brush`], [`crate::graph_gen`]); this file only wires the
//! `Action`s into document mutations, checkpoints, and selection.

use super::{App, Action};
use crate::document::Shape;
use crate::graph_gen::{self, ChartKind};
use crate::pattern_brush;

impl App {
    /// Tail dispatcher for the Batch-12 actions, reached from
    /// `apply_waves_wn`'s catch-all.
    pub(super) fn apply_batch12(&mut self, action: Action) {
        match action {
            Action::SetDocSetupSize { width, height } => {
                self.doc_setup.width = width.clamp(1.0, 32000.0);
                self.doc_setup.height = height.clamp(1.0, 32000.0);
            }
            Action::SetDocSetupUnit(u) => self.doc_setup.unit = u,
            Action::SetDocSetupColorMode(m) => self.doc_setup.color_mode = m,
            Action::SetDocSetupBleed { top, right, bottom, left } => {
                self.doc_setup.bleed = [
                    top.max(0.0),
                    right.max(0.0),
                    bottom.max(0.0),
                    left.max(0.0),
                ];
            }
            Action::NewDocumentFromSetup => self.new_document_from_setup(),

            Action::EnterSymbolEdit(id) => self.enter_symbol_edit(id),
            Action::ExitSymbolEdit => self.exit_symbol_edit(),

            _ => {}
        }
    }

    /// Generate chart geometry from the current `chart_config` into a fixed frame
    /// and add it as a grouped block. Real implementation of `ApplyChartData`.
    /// Chooses the chart kind from the existing `graph_data.graph_type` so the
    /// panel's type selector drives it.
    pub(super) fn apply_chart_data_geometry(&mut self) {
        let kind = match self.graph_data.graph_type {
            super::GraphType::Bar => ChartKind::Bar,
            super::GraphType::Pie => ChartKind::Pie,
            super::GraphType::Line => ChartKind::Line,
            super::GraphType::Scatter => ChartKind::Line,
            super::GraphType::Column => ChartKind::Column,
        };
        // A default placement frame centred-ish on the canvas.
        let frame = [120.0, 120.0, 360.0, 240.0];
        let geo = graph_gen::generate(kind, &self.chart_config, frame);
        if geo.shapes.is_empty() {
            return;
        }
        let group_id = self.next_group_id();
        let shapes = graph_gen::group_shapes(geo.shapes, group_id);
        self.checkpoint();
        let first = self.doc.shapes.len();
        for s in shapes {
            self.doc.shapes.push(s);
        }
        if first < self.doc.shapes.len() {
            self.select_single(self.doc.shapes.len() - 1);
        }
        self.host.mark_dirty();
    }

    /// Place the configured pattern brush along each selected path. Each
    /// selected path's flattened polyline drives one brush stroke; the resulting
    /// tiles are appended (the source path is kept). The whole brush stroke is
    /// tagged into a fresh group id.
    pub(super) fn apply_pattern_brush_to_selected(&mut self) {
        let cfg = self.pattern_brush_config.clone();
        // Collect the flattened polyline of every selected path-like shape.
        let mut strokes: Vec<Vec<(f32, f32)>> = Vec::new();
        for &i in &self.selection {
            if let Some(shape) = self.doc.shapes.get(i) {
                if let Some(poly) = shape_polyline(shape) {
                    if poly.len() >= 2 {
                        strokes.push(poly);
                    }
                }
            }
        }
        if strokes.is_empty() {
            return;
        }
        // Scale 100% ⇒ a tile roughly `spacing` long; convert the % to a unit
        // scale relative to the spacing so tiles butt up at 100%.
        let spacing = cfg.spacing.max(1.0);
        let scale = (cfg.scale / 100.0).max(0.01) * spacing;
        let fill = self.default_fill;
        let tile = pattern_brush::default_tile(fill);

        self.checkpoint();
        let group_id = self.next_group_id();
        let mut placed_any = false;
        for poly in &strokes {
            let placed = pattern_brush::place_pattern_along_path(
                &tile,
                poly,
                spacing,
                scale,
                cfg.flip_across_path,
                cfg.flip_along_path,
                group_id,
            );
            for s in placed {
                self.doc.shapes.push(s);
                placed_any = true;
            }
        }
        if placed_any {
            self.select_single(self.doc.shapes.len() - 1);
            self.host.mark_dirty();
        }
    }

    /// Reset the document and seed a single artboard sized to the current
    /// [`DocumentSetup`]. Real implementation backing `NewDocument`-style flows.
    fn new_document_from_setup(&mut self) {
        self.checkpoint();
        let mut doc = crate::document::Document::default();
        // Size the (single) default artboard to the setup dimensions.
        if let Some(ab) = doc.artboards.first_mut() {
            ab.rect = [0.0, 0.0, self.doc_setup.width, self.doc_setup.height];
        }
        self.doc = doc;
        self.selection.clear();
        self.selected = None;
        self.secondary = None;
        self.show_welcome = false;
        self.editing_symbol = None;
        self.symbol_edit_backup = None;
        self.host.mark_dirty();
    }

    /// Enter symbol-edit mode: stash the document shapes, load the symbol's
    /// master shapes onto the canvas for in-place editing.
    fn enter_symbol_edit(&mut self, id: u64) {
        if self.editing_symbol.is_some() {
            return; // already editing; ignore re-entry
        }
        let Some(sym) = self.symbol_lib.get(id) else {
            return;
        };
        let master = sym.shapes.clone();
        self.symbol_edit_backup = Some(std::mem::take(&mut self.doc.shapes));
        self.doc.shapes = master;
        self.editing_symbol = Some(id);
        self.selection.clear();
        self.selected = None;
        self.secondary = None;
        self.host.mark_dirty();
    }

    /// Exit symbol-edit mode: write the (possibly edited) canvas shapes back to
    /// the symbol master — which propagates to every placed instance on the next
    /// resolve — then restore the document artwork.
    fn exit_symbol_edit(&mut self) {
        let Some(id) = self.editing_symbol.take() else {
            return;
        };
        let edited = std::mem::take(&mut self.doc.shapes);
        self.symbol_lib.set_master_shapes(id, edited);
        if let Some(backup) = self.symbol_edit_backup.take() {
            self.doc.shapes = backup;
        }
        self.selection.clear();
        self.selected = None;
        self.secondary = None;
        self.host.mark_dirty();
    }

    /// A fresh group id unique within the current document's shapes.
    pub(super) fn next_group_id(&self) -> u64 {
        let tags: Vec<Option<u64>> =
            self.doc.shapes.iter().map(|s| s.group()).collect();
        crate::group::next_group_id(&tags)
    }
}

/// Flatten a path-like `Shape` to a document-space polyline for the pattern
/// brush. Returns `None` for non-path shapes (rect/ellipse/text), which the
/// brush ignores. A `Rect`/`Ellipse` is first reduced via `to_path`.
fn shape_polyline(shape: &Shape) -> Option<Vec<(f32, f32)>> {
    use crate::document::flatten;
    match shape {
        Shape::Path { points, handles, closed, .. } => {
            let mut poly = flatten(points, handles, *closed);
            if *closed && poly.len() >= 2 {
                // Re-append the start so a closed brush stroke wraps fully.
                poly.push(poly[0]);
            }
            Some(poly)
        }
        Shape::Line { p0, p1, .. } => Some(vec![*p0, *p1]),
        Shape::Compound { subpaths, .. } => {
            // Use the longest sub-path as the brush spine.
            subpaths
                .iter()
                .map(|sp| flatten(&sp.points, &sp.handles, sp.closed))
                .max_by_key(|p| p.len())
        }
        _ => {
            // Reduce a primitive to its path then flatten.
            let reduced = shape.to_path();
            if matches!(reduced, Shape::Path { .. }) {
                shape_polyline(&reduced)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{ChartDataSet, DocColorMode, DocUnit, GraphType};
    use crate::document::Shape;

    fn app() -> App {
        let mut a = App::new();
        a.doc.shapes.clear();
        a.selection.clear();
        a.selected = None;
        a
    }

    fn open_path(points: Vec<(f32, f32)>) -> Shape {
        let h = vec![(0.0, 0.0); points.len()];
        Shape::path(points, h, false, [0.0; 4], [0.0, 0.0, 0.0, 1.0], 1.0)
    }

    #[test]
    fn pattern_brush_places_tiles_along_selected_path() {
        let mut a = app();
        a.doc.shapes.push(open_path(vec![(0.0, 0.0), (100.0, 0.0)]));
        a.select_single(0);
        a.pattern_brush_config.spacing = 25.0;
        a.pattern_brush_config.scale = 100.0;
        a.apply(Action::ApplyPatternBrushToSelected);
        // 1 source path + 5 placed tiles (floor(100/25)+1).
        assert_eq!(a.doc.shapes.len(), 6, "tiles placed along the path");
        // All placed tiles share one group.
        let groups: Vec<Option<u64>> = a.doc.shapes[1..].iter().map(|s| s.group()).collect();
        assert!(groups.iter().all(|g| g.is_some() && *g == groups[0]));
    }

    #[test]
    fn pattern_brush_ignores_empty_selection() {
        let mut a = app();
        a.doc.shapes.push(open_path(vec![(0.0, 0.0), (100.0, 0.0)]));
        // no selection
        a.apply(Action::ApplyPatternBrushToSelected);
        assert_eq!(a.doc.shapes.len(), 1, "nothing placed without a selection");
    }

    #[test]
    fn chart_data_generates_grouped_shapes() {
        let mut a = app();
        a.chart_config.datasets = vec![
            ChartDataSet { label: "A".into(), values: vec![10.0, 20.0, 30.0], color: [0.0; 4] },
        ];
        a.graph_data.graph_type = GraphType::Column;
        a.apply(Action::ApplyChartData);
        // 3 columns.
        assert_eq!(a.doc.shapes.len(), 3);
        let g = a.doc.shapes[0].group();
        assert!(g.is_some());
        assert!(a.doc.shapes.iter().all(|s| s.group() == g));
    }

    #[test]
    fn chart_pie_generates_wedges() {
        let mut a = app();
        a.chart_config.datasets = vec![
            ChartDataSet { label: "P".into(), values: vec![1.0, 2.0, 3.0, 4.0], color: [0.0; 4] },
        ];
        a.graph_data.graph_type = GraphType::Pie;
        a.apply(Action::ApplyChartData);
        assert_eq!(a.doc.shapes.len(), 4, "4 pie wedges");
    }

    #[test]
    fn doc_setup_actions_update_model() {
        let mut a = app();
        a.apply(Action::SetDocSetupSize { width: 800.0, height: 600.0 });
        assert_eq!((a.doc_setup.width, a.doc_setup.height), (800.0, 600.0));
        a.apply(Action::SetDocSetupUnit(DocUnit::Inches));
        assert_eq!(a.doc_setup.unit, DocUnit::Inches);
        a.apply(Action::SetDocSetupColorMode(DocColorMode::Cmyk));
        assert_eq!(a.doc_setup.color_mode, DocColorMode::Cmyk);
        a.apply(Action::SetDocSetupBleed { top: 9.0, right: 9.0, bottom: 9.0, left: 9.0 });
        assert_eq!(a.doc_setup.bleed, [9.0, 9.0, 9.0, 9.0]);
        let (bw, bh) = a.doc_setup.bleed_size();
        assert!((bw - 818.0).abs() < 1e-3 && (bh - 618.0).abs() < 1e-3);
    }

    #[test]
    fn new_document_from_setup_sizes_artboard() {
        let mut a = app();
        a.doc.shapes.push(open_path(vec![(0.0, 0.0), (1.0, 1.0)]));
        a.doc_setup.width = 1000.0;
        a.doc_setup.height = 500.0;
        a.apply(Action::NewDocumentFromSetup);
        assert!(a.doc.shapes.is_empty(), "new document clears shapes");
        let ab = a.doc.artboards.first().expect("an artboard");
        assert_eq!(ab.rect, [0.0, 0.0, 1000.0, 500.0]);
    }

    #[test]
    fn symbol_edit_propagates_to_instances() {
        let mut a = app();
        // Define a symbol with a 10-wide red square, place two instances.
        let sq = Shape::rect([0.0, 0.0, 10.0, 10.0], [1.0, 0.0, 0.0, 1.0], [0.0; 4], 0.0);
        let id = a.symbol_lib.add("Box", vec![sq]);
        a.symbol_lib.place(id, crate::transform::Affine::translate(100.0, 0.0));
        a.symbol_lib.place(id, crate::transform::Affine::translate(0.0, 100.0));

        // Enter edit mode: the master square is now on the canvas.
        a.apply(Action::EnterSymbolEdit(id));
        assert_eq!(a.editing_symbol, Some(id));
        assert_eq!(a.doc.shapes.len(), 1, "master loaded onto canvas");

        // Edit it: make the square 40 wide.
        if let Some(Shape::Rect { rect, .. }) = a.doc.shapes.get_mut(0) {
            *rect = [0.0, 0.0, 40.0, 10.0];
        }

        // Exit: the edit is written back to the master.
        a.apply(Action::ExitSymbolEdit);
        assert_eq!(a.editing_symbol, None);
        let master = a.symbol_lib.get(id).expect("symbol survives");
        match &master.shapes[0] {
            Shape::Rect { rect, .. } => assert_eq!(rect[2], 40.0, "master is now 40 wide"),
            _ => panic!("expected a rect master"),
        }
        // Both instances resolve through the new master geometry.
        for inst in &a.symbol_lib.instances.clone() {
            let resolved = a.symbol_lib.resolve(inst);
            match &resolved[0] {
                Shape::Rect { rect, .. } => assert_eq!(rect[2], 40.0, "instance reflects edit"),
                _ => panic!("expected a rect instance"),
            }
        }
    }

    #[test]
    fn symbol_edit_restores_document_artwork() {
        let mut a = app();
        a.doc.shapes.push(open_path(vec![(0.0, 0.0), (5.0, 5.0)]));
        let sq = Shape::rect([0.0, 0.0, 10.0, 10.0], [1.0, 0.0, 0.0, 1.0], [0.0; 4], 0.0);
        let id = a.symbol_lib.add("Box", vec![sq]);
        a.apply(Action::EnterSymbolEdit(id));
        assert_eq!(a.doc.shapes.len(), 1, "canvas now shows the master only");
        a.apply(Action::ExitSymbolEdit);
        // The original document artwork is restored (the open path).
        assert_eq!(a.doc.shapes.len(), 1);
        assert!(matches!(a.doc.shapes[0], Shape::Path { .. }), "doc artwork restored");
    }

    #[test]
    fn enter_symbol_edit_ignores_unknown_id() {
        let mut a = app();
        a.doc.shapes.push(open_path(vec![(0.0, 0.0), (5.0, 5.0)]));
        a.apply(Action::EnterSymbolEdit(999));
        assert_eq!(a.editing_symbol, None);
        assert_eq!(a.doc.shapes.len(), 1, "canvas untouched for an unknown symbol");
    }
}
