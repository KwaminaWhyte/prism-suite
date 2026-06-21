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
            Action::LivePaintFill { x: _, y: _, color } => {
                self.default_fill = color;
                if let Some(idx) = self.selected {
                    if idx < self.doc.shapes.len() {
                        self.checkpoint();
                        self.doc.shapes[idx].set_fill_color(color);
                        self.host.mark_dirty();
                    }
                }
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
                if let Some(i) = self.selected {
                    if let Some(shape) = self.doc.shapes.get_mut(i) {
                        shape.set_stroke_width(self.pattern_brush_config.scale / 100.0);
                        self.host.mark_dirty();
                    }
                }
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

            // --- Batch 11: Pathfinder depth ---
            Action::ApplyPathfinderOp(op) => {
                self.last_pathfinder_op = Some(op);
            }
            Action::SetPathfinderPrecision(v) => {
                self.pathfinder_precision = v.clamp(0.001, 10.0);
            }
            Action::SetPathfinderRemoveRedundant(v) => {
                self.pathfinder_remove_redundant = v;
            }
            Action::SetPathfinderDivideStroke(v) => {
                self.pathfinder_divide_stroke = v;
            }
            Action::RepeatPathfinder => {
                // Re-apply the last recorded op if one exists (geometry stub: no-op).
                if let Some(_op) = self.last_pathfinder_op {
                    // stub — geometry would be applied here
                }
            }

            // --- Batch 11: 3D Extrude depth ---
            Action::ToggleExtrudePanel => {
                self.extrude_panel_open = !self.extrude_panel_open;
            }
            Action::SetExtrudeDepth(v) => {
                self.extrude_config.depth = v.clamp(0.0, 2000.0);
            }
            Action::SetExtrudeRotation { x, y, z } => {
                self.extrude_config.rotation_x = x.clamp(-180.0, 180.0);
                self.extrude_config.rotation_y = y.clamp(-180.0, 180.0);
                self.extrude_config.rotation_z = z.clamp(-180.0, 180.0);
            }
            Action::SetExtrudePerspective(v) => {
                self.extrude_config.perspective = v.clamp(0.0, 160.0);
            }
            Action::SetExtrudeSurface(s) => {
                self.extrude_config.surface = s;
            }
            Action::SetExtrudeCapStyle(c) => {
                self.extrude_config.cap_style = c;
            }
            Action::SetExtrudeBevelHeight(v) => {
                self.extrude_config.bevel_height = v.clamp(0.0, 100.0);
            }
            Action::SetExtrudeLighting { intensity, ambient, specular, gloss } => {
                self.extrude_config.light_intensity = intensity.clamp(0.0, 100.0);
                self.extrude_config.ambient_light = ambient.clamp(0.0, 100.0);
                self.extrude_config.specular_highlight = specular.clamp(0.0, 100.0);
                self.extrude_config.gloss = gloss.clamp(0.0, 100.0);
            }
            Action::SetExtrudeMapArt(v) => {
                self.extrude_config.map_art = v;
            }
            Action::ApplyExtrude => {
                if let Some(idx) = self.selected {
                    if !self.extrude_applied_shapes.contains(&idx) {
                        self.extrude_applied_shapes.push(idx);
                    }
                }
            }
            Action::ExpandExtrude => {
                self.extrude_applied_shapes.clear();
            }

            // --- Batch 11: Chart depth ---
            Action::ToggleChartPanel => {
                self.chart_panel_open = !self.chart_panel_open;
            }
            Action::AddChartDataSet(ds) => {
                self.chart_config.datasets.push(ds);
            }
            Action::RemoveChartDataSet(idx) => {
                if idx < self.chart_config.datasets.len() {
                    self.chart_config.datasets.remove(idx);
                }
            }
            Action::SetChartDataSetValues { idx, values } => {
                if let Some(ds) = self.chart_config.datasets.get_mut(idx) {
                    ds.values = values;
                }
            }
            Action::SetChartDataSetLabel { idx, label } => {
                if let Some(ds) = self.chart_config.datasets.get_mut(idx) {
                    ds.label = label;
                }
            }
            Action::SetChartCategoryLabels(labels) => {
                self.chart_config.category_labels = labels;
            }
            Action::SetChartTitle(title) => {
                self.chart_config.title = title;
            }
            Action::SetChartShowLegend(v) => {
                self.chart_config.show_legend = v;
            }
            Action::SetChartShowGrid(v) => {
                self.chart_config.show_grid = v;
            }
            Action::SetChartColumnWidth(v) => {
                self.chart_config.column_width = v.clamp(20.0, 100.0);
            }
            Action::SetChartClusterWidth(v) => {
                self.chart_config.cluster_width = v.clamp(20.0, 100.0);
            }
            Action::SetChartValueRange { min, max } => {
                self.chart_config.value_axis_min = min;
                self.chart_config.value_axis_max = max;
            }
            Action::ApplyChartData => {
                // stub: in a full impl, shapes would be generated per dataset value
            }

            // --- Batch 11: Envelope Distort depth ---
            Action::ToggleEnvelopePanel => {
                self.envelope_panel_open = !self.envelope_panel_open;
            }
            Action::SetEnvelopeWarpStyle(s) => {
                self.envelope_config.warp_style = s;
            }
            Action::SetEnvelopeAxis(h) => {
                self.envelope_config.horizontal = h;
            }
            Action::SetEnvelopeBend(v) => {
                self.envelope_config.bend = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeHDistortion(v) => {
                self.envelope_config.h_distortion = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeVDistortion(v) => {
                self.envelope_config.v_distortion = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeFidelity(v) => {
                self.envelope_config.fidelity = v.clamp(0.0, 100.0);
            }
            Action::SetEnvelopeEditMode(m) => {
                self.envelope_config.edit_mode = m;
            }
            Action::MakeEnvelopeWithWarpPreset => {
                if let Some(idx) = self.selected {
                    if !self.envelope_applied_shapes.contains(&idx) {
                        self.envelope_applied_shapes.push(idx);
                    }
                }
            }
            Action::MakeEnvelopeWithMeshPreset => {
                if let Some(idx) = self.selected {
                    if !self.envelope_applied_shapes.contains(&idx) {
                        self.envelope_applied_shapes.push(idx);
                    }
                }
            }
            Action::ReleaseEnvelopeAll => {
                self.envelope_applied_shapes.clear();
            }
            Action::ExpandEnvelope => {
                self.envelope_applied_shapes.clear();
            }

            // --- Wave N: Image Trace (extended panel) ---
            Action::SetImageTraceMode(mode) => {
                self.image_trace_config.mode = mode;
            }
            Action::SetImageTraceThreshold(v) => {
                self.image_trace_config.threshold = v;
            }
            Action::SetImageTraceColors(v) => {
                self.image_trace_config.colors = v.clamp(2, 30);
            }
            Action::SetImageTracePaths(v) => {
                self.image_trace_config.paths = v.clamp(1, 100);
            }
            Action::SetImageTraceCorners(v) => {
                self.image_trace_config.corners = v.clamp(0, 100);
            }
            Action::RunImageTrace { image_id } => {
                let path_count = self.image_trace_config.colors as usize * 12
                    + self.image_trace_config.noise as usize;
                self.image_trace_results.push(ImageTraceResult {
                    source_image_id: image_id,
                    config: self.image_trace_config.clone(),
                    path_count,
                    expanded: false,
                });
            }
            Action::ExpandImageTraceResult { result_index } => {
                if let Some(r) = self.image_trace_results.get_mut(result_index) {
                    r.expanded = true;
                }
            }

            // --- Wave N: Perspective Grid (extended config) ---
            Action::SetPerspectiveGridType(t) => {
                self.perspective_grid_config.grid_type = t;
            }
            Action::TogglePerspectiveGridConfig => {
                self.perspective_grid_config.visible = !self.perspective_grid_config.visible;
            }
            Action::SetPerspectiveGridSnap(v) => {
                self.perspective_grid_config.snap = v;
            }
            Action::SetPerspectiveGridCellSize(v) => {
                self.perspective_grid_config.cell_size = v.clamp(1.0, 500.0);
            }
            Action::SetPerspectiveGridOpacity(v) => {
                self.perspective_grid_config.opacity = v.clamp(0, 100);
            }
            Action::SetPerspectiveActivePlaneByName(name) => {
                let n = name.as_str();
                self.perspective_grid_config.left_plane.active = n == "left";
                self.perspective_grid_config.right_plane.active = n == "right";
                self.perspective_grid_config.floor_plane.active = n == "floor";
            }
            Action::MoveVanishingPoint { which, x, y } => {
                match which.as_str() {
                    "left" => self.perspective_grid_config.vanishing_point_left = (x, y),
                    "right" => self.perspective_grid_config.vanishing_point_right = (x, y),
                    _ => {}
                }
            }

            // --- Wave N: Global Swatches ---
            Action::AddGlobalSwatch { name, color, is_spot } => {
                let id = self.swatch_counter;
                self.swatch_counter += 1;
                self.global_swatches.push(GlobalSwatch {
                    id,
                    name,
                    color,
                    is_global: true,
                    is_spot,
                    usage_count: 0,
                });
            }
            Action::EditGlobalSwatch { id, color } => {
                if let Some(sw) = self.global_swatches.iter_mut().find(|s| s.id == id) {
                    sw.color = color;
                }
            }
            Action::DeleteGlobalSwatch(id) => {
                self.global_swatches.retain(|s| s.id != id);
            }
            Action::CreateSwatchGroup { name } => {
                let id = self.swatch_group_counter;
                self.swatch_group_counter += 1;
                self.swatch_groups.push(SwatchGroup { id, name, swatch_ids: Vec::new() });
            }
            Action::AddSwatchToGroup { group_id, swatch_id } => {
                if let Some(g) = self.swatch_groups.iter_mut().find(|g| g.id == group_id) {
                    if !g.swatch_ids.contains(&swatch_id) {
                        g.swatch_ids.push(swatch_id);
                    }
                }
            }
            Action::ReorderSwatches(order) => {
                let mut reordered: Vec<GlobalSwatch> = Vec::with_capacity(order.len());
                for &id in &order {
                    if let Some(pos) = self.global_swatches.iter().position(|s| s.id == id) {
                        reordered.push(self.global_swatches.remove(pos));
                    }
                }
                // Append any swatches not mentioned in the order vec.
                reordered.append(&mut self.global_swatches);
                self.global_swatches = reordered;
            }

            // --- Wave N: Artboards (extended) ---
            Action::AddArtboardEx { x, y, width, height } => {
                let id = self.artboard_counter;
                self.artboard_counter += 1;
                self.artboards_ex.push(Artboard::new(id, x, y, width, height));
                self.active_artboard_ex = Some(id);
            }
            Action::DeleteArtboardEx(id) => {
                self.artboards_ex.retain(|a| a.id != id);
                if self.active_artboard_ex == Some(id) {
                    self.active_artboard_ex = None;
                }
            }
            Action::RenameArtboardEx { id, name } => {
                if let Some(a) = self.artboards_ex.iter_mut().find(|a| a.id == id) {
                    a.name = name;
                }
            }
            Action::ResizeArtboard { id, width, height } => {
                if let Some(a) = self.artboards_ex.iter_mut().find(|a| a.id == id) {
                    a.width = width.clamp(1.0, 32000.0);
                    a.height = height.clamp(1.0, 32000.0);
                }
            }
            Action::SetActiveArtboard(id) => {
                self.active_artboard_ex = Some(id);
            }
            Action::ReorderArtboards(order) => {
                let mut reordered: Vec<Artboard> = Vec::with_capacity(order.len());
                for &id in &order {
                    if let Some(pos) = self.artboards_ex.iter().position(|a| a.id == id) {
                        reordered.push(self.artboards_ex.remove(pos));
                    }
                }
                reordered.append(&mut self.artboards_ex);
                self.artboards_ex = reordered;
            }
            Action::DuplicateArtboardEx(id) => {
                if let Some(src) = self.artboards_ex.iter().find(|a| a.id == id).cloned() {
                    let new_id = self.artboard_counter;
                    self.artboard_counter += 1;
                    let mut copy = Artboard::new(new_id, src.x + src.width + 20.0, src.y, src.width, src.height);
                    copy.name = format!("Copy of {}", src.name);
                    copy.preset = src.preset.clone();
                    self.artboards_ex.push(copy);
                    self.active_artboard_ex = Some(new_id);
                }
            }
            _ => {}
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

