use super::*;

impl App {
    pub(super) fn apply_extended2(&mut self, action: Action) {
        match action {
            // --- Batch 9: Type on Path depth ---
            Action::SetTextOnPathOffset { text_id, offset } => {
                self.text_on_path_offsets.insert(text_id, offset);
                self.relayout_text_on_path(text_id);
                self.host.mark_dirty();
            }
            Action::SetTextOnPathSide { text_id, above } => {
                self.text_on_path_above.insert(text_id, above);
                self.relayout_text_on_path(text_id);
                self.host.mark_dirty();
            }
            Action::SetTextOnPathSpacing { text_id, spacing } => {
                self.text_on_path_spacing.insert(text_id, spacing);
                self.relayout_text_on_path(text_id);
                self.host.mark_dirty();
            }
            Action::FlipTextOnPath(id) => {
                let current = self.text_on_path_above.get(&id).copied().unwrap_or(true);
                self.text_on_path_above.insert(id, !current);
                self.relayout_text_on_path(id);
                self.host.mark_dirty();
            }

            // --- Batch 9: Recolor Artwork depth ---
            Action::SetRecolorConfig(c) => {
                self.recolor_config = c;
            }
            Action::SetRecolorColorCount(n) => {
                self.recolor_color_count = n.clamp(2, 30);
            }
            Action::SetRecolorPreserveBlack(b) => {
                self.recolor_config.preserve_black = b;
            }
            Action::SetRecolorPreserveWhite(b) => {
                self.recolor_config.preserve_white = b;
            }
            Action::RandomizeRecolor => {
                self.recolor_config.randomize = !self.recolor_config.randomize;
            }
            Action::SaveRecolorSet => {
                let fills: Vec<[f32; 4]> = self.selection.iter()
                    .filter_map(|&i| self.doc.shapes.get(i)?.fill_color())
                    .take(10)
                    .collect();
                if !fills.is_empty() {
                    self.recolor_history.push(fills);
                }
            }
            Action::ApplyRecolorToSelected => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let step = 1.0 / self.recolor_color_count.max(1) as f32;
                    let sel: Vec<usize> = self.selection.clone();
                    for (k, &i) in sel.iter().enumerate() {
                        if let Some(shape) = self.doc.shapes.get_mut(i) {
                            if let Some(c) = shape.fill_color() {
                                let t = (k as f32 * step).fract();
                                let rotated = [
                                    c[0] * (1.0 - t) + c[1] * t,
                                    c[1] * (1.0 - t) + c[2] * t,
                                    c[2] * (1.0 - t) + c[0] * t,
                                    c[3],
                                ];
                                shape.set_fill_color(rotated);
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 9: Live Paint depth ---
            Action::LivePaintFill { x, y, color } => {
                self.default_fill = color;
                self.live_paint_hit = Some((x, y));
                self.apply_live_paint_fill(x, y, color);
            }

            // --- Batch 10: Gradient Mesh depth ---
            Action::SetMeshRows(rows) => {
                self.gradient_mesh.rows = rows.clamp(1, 50);
            }
            Action::SetMeshCols(cols) => {
                self.gradient_mesh.cols = cols.clamp(1, 50);
            }
            Action::CreateMesh => {
                let count = self.gradient_mesh.rows as usize * self.gradient_mesh.cols as usize;
                self.gradient_mesh.points = (0..count).map(|_| MeshPoint::default()).collect();
            }
            Action::SetMeshPointColor { idx, color } => {
                if let Some(pt) = self.gradient_mesh.points.get_mut(idx) {
                    pt.color = color;
                }
            }
            Action::SetMeshPointTension { idx, tension } => {
                if let Some(pt) = self.gradient_mesh.points.get_mut(idx) {
                    pt.tension = tension.clamp(0.0, 1.0);
                }
            }
            Action::SelectMeshPoint(idx) => {
                self.selected_mesh_point = Some(idx);
            }
            Action::MoveMeshPoint { idx, pos } => {
                if let Some(pt) = self.gradient_mesh.points.get_mut(idx) {
                    pt.position = pos;
                }
            }
            Action::ExpandMeshToShape => {
                self.selected_mesh_point = None;
            }
            Action::ToggleMeshTool => {
                self.mesh_tool_active = !self.mesh_tool_active;
            }
            Action::ReleaseMesh => {
                self.gradient_mesh = GradientMeshConfig::default();
            }

            // --- Batch 10: Flare Tool ---
            Action::ToggleFlareTool => {
                self.flare_tool_active = !self.flare_tool_active;
            }
            Action::SetFlareBrightness(v) => {
                self.flare_config.brightness = v.clamp(0.0, 100.0);
            }
            Action::SetFlareHaloSize(v) => {
                self.flare_config.halo_size = v.clamp(0.0, 100.0);
            }
            Action::SetFlareRayCount(v) => {
                self.flare_config.ray_count = v.clamp(0, 250);
            }
            Action::SetFlareRayLength(v) => {
                self.flare_config.ray_length = v.clamp(0.0, 300.0);
            }
            Action::SetFlareRingCount(v) => {
                self.flare_config.ring_count = v.clamp(0, 50);
            }
            Action::SetFlareRingSpacing(v) => {
                self.flare_config.ring_spacing = v.clamp(0.0, 300.0);
            }
            Action::SetFlareColor(color) => {
                self.flare_config.color = color;
            }
            Action::PlaceFlare(pos) => {
                let idx = self.doc.shapes.len();
                self.flare_config.center = pos;
                self.flare_shapes.push(idx);
            }

            // --- Batch 10: Pattern Brush depth ---
            Action::SetPatternBrushScale(v) => {
                self.pattern_brush_config.scale = v.clamp(0.0, 1000.0);
            }
            Action::SetPatternBrushSpacing(v) => {
                self.pattern_brush_config.spacing = v.clamp(0.0, 1000.0);
            }
            Action::SetPatternBrushFit(fit) => {
                self.pattern_brush_config.fit = fit;
            }
            Action::SetPatternBrushFlipAcross(v) => {
                self.pattern_brush_config.flip_across_path = v;
            }
            Action::SetPatternBrushFlipAlong(v) => {
                self.pattern_brush_config.flip_along_path = v;
            }
            Action::SavePatternBrush { name } => {
                self.pattern_brush_library.push(name);
            }
            Action::DeletePatternBrush(idx) => {
                if idx < self.pattern_brush_library.len() {
                    self.pattern_brush_library.remove(idx);
                }
            }
            Action::ApplyPatternBrushToSelected => {
                // Real pattern brush: tile a unit shape along each selected path's
                // tangent at the configured spacing (see `apply_batch12`).
                self.apply_pattern_brush_to_selected();
            }

            Action::LivePaintStroke { x: _, y: _, color, width } => {
                // Stub: record the stroke defaults.
                self.default_stroke = color;
                self.default_stroke_w = width;
            }
            Action::MakeLivePaintGroup => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    // Assign all selected shapes to a new Live Paint group id.
                    let gid = self.live_paint_group_ids.len() as u64 + 1;
                    for &i in &self.selection {
                        if let Some(s) = self.doc.shapes.get_mut(i) {
                            s.set_group(Some(gid));
                        }
                    }
                    self.live_paint_group_ids.push(gid);
                    self.host.mark_dirty();
                }
            }
            Action::ReleaseLivePaintGroup => {
                self.live_paint_group_ids.clear();
            }
            Action::ExpandLivePaintGroup => {
                // Stub: no geometric change; just log intent.
                log::info!("ExpandLivePaintGroup: stub");
            }
            Action::SetLivePaintGapDetection(b) => {
                self.live_paint_gap_detection = b;
            }
            Action::SetLivePaintHighlightColor(c) => {
                self.live_paint_highlight_color = c;
            }

            // --- Batch 9: Symbol Sprayer ---
            Action::SetSymbolSprayConfig(c) => {
                self.symbol_spray_config = c;
            }
            Action::SetSymbolSprayDensity(d) => {
                self.symbol_spray_config.density = d.clamp(0.0, 10.0);
            }
            Action::SetSymbolSprayDiameter(d) => {
                self.symbol_spray_config.diameter = d.max(1.0);
            }
            Action::SpraySymbols { center, pressure } => {
                let n = (self.symbol_spray_config.density * pressure * 3.0).ceil() as usize;
                if n > 0 {
                    self.checkpoint();
                    let radius = self.symbol_spray_config.diameter * 0.5;
                    let scatter = self.symbol_spray_config.scatter;
                    let fill = self.default_fill;
                    let stroke = self.default_stroke;
                    let sw = self.default_stroke_w;
                    for i in 0..n {
                        // Deterministic jitter from index — no random state needed.
                        let jx = ((i * 37 % 17) as f32 / 17.0 * 2.0 - 1.0) * radius * scatter;
                        let jy = ((i * 53 % 19) as f32 / 19.0 * 2.0 - 1.0) * radius * scatter;
                        let x = center[0] + jx;
                        let y = center[1] + jy;
                        let size = 20.0;
                        self.doc.shapes.push(Shape::rect(
                            [x - size * 0.5, y - size * 0.5, size, size],
                            fill,
                            stroke,
                            sw,
                        ));
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SymbolShift { center, delta } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                let aff = Affine::translate(delta[0], delta[1]);
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            shape.apply_affine(&aff);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolScale { center, scale } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            let aff = Affine::scale_about(scale, scale, cx, cy);
                            shape.apply_affine(&aff);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolSpin { center, angle } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            let aff = Affine::rotate_about(angle, cx, cy);
                            shape.apply_affine(&aff);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolStain { center, color } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            if let Some(fc) = shape.fill_color() {
                                let t = 0.25; // blend 25% toward stain color
                                let blended = [
                                    fc[0] * (1.0 - t) + color[0] * t,
                                    fc[1] * (1.0 - t) + color[1] * t,
                                    fc[2] * (1.0 - t) + color[2] * t,
                                    fc[3],
                                ];
                                shape.set_fill_color(blended);
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }
            Action::SymbolScreen { center, opacity } => {
                let r = self.symbol_spray_config.diameter * 0.5;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(b) = shape.bounds() {
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        let dx = cx - center[0];
                        let dy = cy - center[1];
                        if (dx * dx + dy * dy).sqrt() <= r {
                            if let Some(fc) = shape.fill_color() {
                                let new_alpha = (fc[3] * opacity).clamp(0.0, 1.0);
                                shape.set_fill_color([fc[0], fc[1], fc[2], new_alpha]);
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }

            // --- Batch 10: Variable Fonts ---
            Action::ToggleVariableFontPanel => {
                self.variable_font_panel_open = !self.variable_font_panel_open;
            }
            Action::AddFontAxis(axis) => {
                self.variable_font_config.axes.push(axis);
            }
            Action::RemoveFontAxis(idx) => {
                if idx < self.variable_font_config.axes.len() {
                    self.variable_font_config.axes.remove(idx);
                }
            }
            Action::SetFontAxisValue { idx, value } => {
                if let Some(axis) = self.variable_font_config.axes.get_mut(idx) {
                    axis.value = value.clamp(axis.min, axis.max);
                }
            }
            Action::SetFontPreviewText(text) => {
                self.variable_font_config.preview_text = text;
            }
            Action::ApplyVariableFontToSelected => {
                if self.selected.is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::ResetFontAxes => {
                for axis in self.variable_font_config.axes.iter_mut() {
                    axis.value = (axis.min + axis.max) / 2.0;
                }
            }


            a => self.apply_extended4(a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // ---- Batch 8 tests -------------------------------------------------------

    // --- Image Trace ---

    #[test]
    fn test_image_trace_config() {
        let mut app = App::new();
        app.apply(Action::SetImageTrace {
            mode: ImageTraceMode::BlackWhite,
            threshold: 180.0,
            colors: 4,
        });
        assert_eq!(app.image_trace_mode, ImageTraceMode::BlackWhite);
        assert!((app.image_trace_threshold - 180.0).abs() < 0.01);
        assert_eq!(app.image_trace_colors, 4);
    }

    #[test]
    fn test_image_trace_config_clamping() {
        let mut app = App::new();
        // threshold clamped 0..=255, colors >= 2
        app.apply(Action::SetImageTrace { mode: ImageTraceMode::Color, threshold: 999.0, colors: 0 });
        assert!((app.image_trace_threshold - 255.0).abs() < 0.01, "threshold clamped to 255");
        assert_eq!(app.image_trace_colors, 2, "colors min is 2");
    }

    #[test]
    fn test_apply_image_trace_adds_shapes() {
        let mut app = App::new();
        app.apply(Action::SetImageTrace { mode: ImageTraceMode::Color, threshold: 128.0, colors: 4 });
        let before = app.doc.shapes.len();
        app.apply(Action::ApplyImageTrace);
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1 && added <= 6, "expected 1-6 traced shapes, got {added}");
        assert!(app.history.can_undo(), "ApplyImageTrace is undoable");
    }

    #[test]
    fn test_expand_image_trace() {
        let mut app = App::new();
        assert!(!app.image_trace_expanded);
        app.apply(Action::ExpandImageTrace);
        assert!(app.image_trace_expanded, "ExpandImageTrace sets the flag");
    }

    // --- Opacity Masks ---

    #[test]
    fn test_make_opacity_mask_links_two_shapes() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 30.0, 30.0], [0.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        // Both shapes should share the same omask id.
        let id0 = app.doc.shapes[0].omask();
        let id1 = app.doc.shapes[1].omask();
        assert!(id0.is_some(), "bottom shape should have omask id");
        assert!(id1.is_some(), "top shape should have omask id");
        assert_eq!(id0, id1, "both shapes share the same mask group id");
        // Top shape (index 1) is the mask path.
        assert!(app.doc.shapes[1].is_omask(), "top shape is the mask path");
        assert!(!app.doc.shapes[0].is_omask(), "bottom shape is NOT the mask path");
    }

    #[test]
    fn test_make_opacity_mask_needs_two_selected() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        // Only one shape selected → no-op.
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        assert!(app.doc.shapes[0].omask().is_none(), "no omask set with only 1 shape selected");
    }

    #[test]
    fn test_release_opacity_mask() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 30.0, 30.0], [0.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        assert!(app.doc.shapes[0].omask().is_some(), "mask set created");
        // Now release with both shapes selected.
        app.selection = vec![0, 1];
        app.apply(Action::ReleaseOpacityMask);
        assert!(app.doc.shapes[0].omask().is_none(), "bottom shape released");
        assert!(app.doc.shapes[1].omask().is_none(), "top shape released");
    }

    #[test]
    fn test_invert_opacity_mask() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.doc.shapes.push(Shape::rect([10.0, 10.0, 30.0, 30.0], [0.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::MakeOpacityMask);
        let before_invert = app.doc.shapes[0].omask_invert();
        // Select only the masked content (index 0, not the mask path).
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::InvertOpacityMask);
        let after_invert = app.doc.shapes[0].omask_invert();
        assert_ne!(before_invert, after_invert, "InvertOpacityMask should toggle omask_invert");
    }

    // --- Graph Tool ---

    #[test]
    fn test_graph_type_set() {
        let mut app = App::new();
        app.apply(Action::SetGraphType(GraphType::Pie));
        assert_eq!(app.graph_data.graph_type, GraphType::Pie);
        app.apply(Action::SetGraphType(GraphType::Line));
        assert_eq!(app.graph_data.graph_type, GraphType::Line);
    }

    #[test]
    fn test_graph_data_set() {
        let mut app = App::new();
        app.apply(Action::SetGraphData { cols: 4, rows: 3, values: vec![1.0, 2.0, 3.0, 4.0] });
        assert_eq!(app.graph_data.cols, 4);
        assert_eq!(app.graph_data.rows, 3);
        assert_eq!(app.graph_data.values, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_apply_graph_adds_rects() {
        let mut app = App::new();
        app.apply(Action::SetGraphData { cols: 3, rows: 1, values: vec![10.0, 20.0, 30.0] });
        let before = app.doc.shapes.len();
        app.apply(Action::ApplyGraph { x: 0.0, y: 0.0, width: 300.0, height: 200.0 });
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1, "ApplyGraph should add at least one shape, got {added}");
        assert!(app.history.can_undo(), "ApplyGraph is undoable");
    }

    #[test]
    fn test_graph_legend_toggle() {
        let mut app = App::new();
        assert!(!app.graph_show_legend, "legend starts false");
        app.apply(Action::ToggleGraphLegend);
        assert!(app.graph_show_legend, "legend toggled on");
        app.apply(Action::ToggleGraphLegend);
        assert!(!app.graph_show_legend, "legend toggled off again");
    }

    #[test]
    fn test_graph_labels_set() {
        let mut app = App::new();
        let labels = vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()];
        app.apply(Action::SetGraphLabels(labels.clone()));
        assert_eq!(app.graph_data.labels, labels);
    }

    #[test]
    fn test_graph_style_fill_set() {
        let mut app = App::new();
        let color = [0.8, 0.2, 0.4, 1.0];
        app.apply(Action::SetGraphStyleFill(color));
        assert_eq!(app.graph_style_fill, color);
    }

}

