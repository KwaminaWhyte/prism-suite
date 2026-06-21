use super::*;

impl App {
    pub(super) fn apply_advanced(&mut self, action: Action) {
        match action {
            // --- Batch 5: Type on a Path ---
            Action::SetTextOnPathParams { text_id, params } => {
                if self.text_on_path.contains_key(&text_id) {
                    self.checkpoint();
                    self.text_on_path_params.insert(text_id, params);
                    self.relayout_text_on_path(text_id);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: Perspective Distort (real homography) ---
            Action::ApplyPerspectiveDistort { shape_id } => {
                if shape_id < self.doc.shapes.len() {
                    if let Some(b) = self.doc.shapes[shape_id].bounds() {
                        let bbox = [b.x, b.y, b.w, b.h];
                        // Use the on-canvas corners if a distort is being edited;
                        // otherwise apply a default top-narrowing trapezoid about
                        // the shape's bounds (one-click perspective).
                        let corners = if self.perspective_distort_active {
                            self.perspective_distort_corners
                        } else {
                            let inset = b.w * 0.2;
                            [
                                [b.x + inset, b.y],
                                [b.x + b.w - inset, b.y],
                                [b.x + b.w, b.y + b.h],
                                [b.x, b.y + b.h],
                            ]
                        };
                        // Warp the shape's editable path geometry through the
                        // homography (anchors + bezier handles), preserving curves.
                        let path = self.doc.shapes[shape_id].to_path();
                        let warped = warp_shape_perspective(&path, bbox, &corners);
                        if let Some(w) = warped {
                            self.checkpoint();
                            self.doc.shapes[shape_id] = w;
                            self.perspective_distort_active = false;
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            // --- Batch 5: Gradient Mesh object ---
            Action::MakeMeshGradient => {
                if let Some(idx) = self.selected {
                    if let Some(b) = self.doc.shapes[idx].bounds() {
                        let base = self.doc.shapes[idx].fill_color().unwrap_or([0.6, 0.6, 0.6, 1.0]);
                        // Mesh control points are in canvas-pixel space (artboard
                        // origin + doc point), matching the renderer's expectation.
                        let (ox, oy) = self.host.artboard_origin(&self.doc);
                        let bbox = [b.x - ox, b.y - oy, b.w, b.h];
                        self.mesh_points = crate::mesh_gradient::seed_grid(bbox, base);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ClearMeshGradient => {
                if !self.mesh_points.is_empty() {
                    self.mesh_points.clear();
                    self.host.mark_dirty();
                }
            }

            // --- Batch 5: Variable-width stroke outline ---
            Action::OutlineWidthProfile(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    let stroke_w = self.doc.shapes[shape_id].stroke_width();
                    let (ms, me) = self.doc.shapes[shape_id].stroke_style().width_profile;
                    let stroke_color = self.doc.shapes[shape_id]
                        .stroke_color()
                        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    // Flatten the centreline from the editable path (works for open
                    // paths/lines, where variable-width strokes matter most, and
                    // closed paths). Open paths are not auto-closed.
                    let centreline = match self.doc.shapes[shape_id].to_path() {
                        Shape::Path { points, handles, closed, .. } => {
                            Some(crate::document::flatten(&points, &handles, closed))
                        }
                        _ => None,
                    };
                    if stroke_w > 0.0 {
                        if let Some(pts) = centreline {
                            let half_start = stroke_w * 0.5 * ms;
                            let half_end = stroke_w * 0.5 * me;
                            let band = crate::pathedit::variable_width_outline(
                                &pts, half_start, half_end,
                            );
                            if band.len() >= 3 {
                                self.checkpoint();
                                let outline = Shape::path(
                                    band,
                                    Vec::new(),
                                    true,
                                    stroke_color,
                                    [0.0, 0.0, 0.0, 0.0],
                                    0.0,
                                );
                                let insert_at = (shape_id + 1).min(self.doc.shapes.len());
                                self.doc.shapes.insert(insert_at, outline);
                                self.select_single(insert_at);
                                self.host.mark_dirty();
                            }
                        }
                    }
                }
            }

            // --- Batch 5: Roughen distort ---
            Action::RoughenPath { size, detail } => {
                let sel: Vec<usize> = self.selection.clone();
                if !sel.is_empty() {
                    self.checkpoint();
                    let mut changed = false;
                    for i in sel {
                        if i >= self.doc.shapes.len() {
                            continue;
                        }
                        // Demote to a plain corner path, roughen its outline.
                        let path = self.doc.shapes[i].to_path();
                        if let Shape::Path {
                            points,
                            closed,
                            fill,
                            stroke,
                            stroke_w,
                            ..
                        } = &path
                        {
                            let rough = crate::pathedit::roughen(points, *closed, size, detail);
                            if rough.len() >= 2 {
                                self.doc.shapes[i] = Shape::path(
                                    rough,
                                    Vec::new(),
                                    *closed,
                                    *fill,
                                    *stroke,
                                    *stroke_w,
                                );
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 6: Transform Each ---
            Action::TransformEach { dx, dy, scale_x, scale_y, angle_deg, reflect_x } => {
                let sel: Vec<usize> = self.selection.clone();
                if sel.is_empty() {
                    return;
                }
                self.checkpoint();
                let rad = angle_deg.to_radians();
                for i in sel {
                    if i >= self.doc.shapes.len() {
                        continue;
                    }
                    // Compute the shape's own bbox centre as the pivot.
                    let (cx, cy) = self.doc.shapes[i]
                        .bounds()
                        .map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
                        .unwrap_or((0.0, 0.0));
                    // Build and compose transforms: translate → scale about centre
                    // → rotate about centre → optional x-reflect about centre.
                    let mut aff = Affine::translate(dx, dy);
                    if scale_x != 1.0 || scale_y != 1.0 {
                        // Scale about the (already-translated) centre: the centre
                        // after a pure translate is (cx+dx, cy+dy), but Illustrator
                        // applies each sub-transform independently from the *original*
                        // bbox centre, so we do likewise — scale about original centre.
                        aff = aff.then(Affine::scale_about(scale_x, scale_y, cx, cy));
                    }
                    if angle_deg != 0.0 {
                        aff = aff.then(Affine::rotate_about(rad, cx, cy));
                    }
                    if reflect_x {
                        aff = aff.then(Affine::scale_about(-1.0, 1.0, cx, cy));
                    }
                    self.doc.shapes[i].apply_affine(&aff);
                }
                self.host.mark_dirty();
            }

            // --- Batch 6: Offset Path ---
            Action::OffsetPath { shape_id, distance } => {
                if shape_id >= self.doc.shapes.len() {
                    return;
                }
                let path = self.doc.shapes[shape_id].to_path();
                if let Shape::Path { points, closed, fill, stroke, stroke_w, .. } = &path {
                    if !closed || points.len() < 3 {
                        return;
                    }
                    let offset_pts = offset_polygon(points, distance);
                    if offset_pts.len() < 3 {
                        return;
                    }
                    self.checkpoint();
                    self.doc.shapes[shape_id] =
                        Shape::path(offset_pts, Vec::new(), true, *fill, *stroke, *stroke_w);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 6: Find / Replace ---
            Action::ToggleFindReplacePanel => {
                self.find_replace_open = !self.find_replace_open;
            }
            Action::SetFindReplaceQuery { find, replace } => {
                self.find_query = find;
                self.replace_query = replace;
            }
            Action::FindReplaceText { find, replace } => {
                if find.is_empty() {
                    return;
                }
                let mut count = 0usize;
                // Snapshot the shapes we'll change so we can checkpoint once.
                let mut changed_any = false;
                for shape in &mut self.doc.shapes {
                    if let Shape::Text { params, .. } = shape {
                        if params.text.contains(&find as &str) {
                            if !changed_any {
                                // checkpoint before first mutation
                                changed_any = true;
                            }
                            params.text = params.text.replace(&find as &str, &replace as &str);
                            count += 1;
                        }
                    }
                }
                if changed_any {
                    // Re-layout all modified text glyphs.
                    for shape in &mut self.doc.shapes {
                        shape.text_relayout();
                    }
                    self.history.push(self.doc.clone());
                    self.host.mark_dirty();
                }
                self.last_find_count = count;
            }

            // --- Batch 6: Pathfinder shortcuts ---
            Action::PathfinderTrim => self.apply_boolean(BoolOp::Trim),
            Action::PathfinderMerge => self.apply_boolean(BoolOp::Merge),

            // --- Batch 6: Scatter Brush ---
            Action::SetScatterBrush { symbol_id, spacing, size_jitter, rotation_jitter } => {
                self.scatter_brush = Some(ScatterBrushConfig {
                    symbol_id,
                    spacing: spacing.max(1.0),
                    size_jitter: size_jitter.clamp(0.0, 1.0),
                    rotation_jitter,
                });
            }
            Action::ClearScatterBrush => {
                self.scatter_brush = None;
            }
            Action::PlaceScatterAlongPath { path } => {
                let Some(cfg) = self.scatter_brush.clone() else { return; };
                if path.len() < 2 || cfg.spacing < 1.0 {
                    return;
                }
                // Compute arc-length at each polyline vertex.
                let mut arc: Vec<f32> = Vec::with_capacity(path.len());
                arc.push(0.0);
                for i in 1..path.len() {
                    let dx = path[i][0] - path[i - 1][0];
                    let dy = path[i][1] - path[i - 1][1];
                    arc.push(arc[i - 1] + (dx * dx + dy * dy).sqrt());
                }
                let total = *arc.last().unwrap_or(&0.0);
                if total < cfg.spacing {
                    return;
                }
                // Sample at multiples of spacing along the arc.
                let mut dist = 0.0f32;
                let mut copy_idx: usize = 0;
                let mut new_shapes: Vec<Shape> = Vec::new();
                while dist <= total {
                    // Interpolate position on the polyline at distance `dist`.
                    let (px, py) = sample_polyline(&path, &arc, dist);
                    // Deterministic pseudo-random for jitter (no stdlib random).
                    let rng = |seed: usize| -> f32 {
                        let v = (seed.wrapping_mul(1_234_567).wrapping_add(7_654_321)) % 1000;
                        v as f32 / 1000.0
                    };
                    let size_scale = 1.0
                        + cfg.size_jitter * (rng(copy_idx) * 2.0 - 1.0);
                    let rot_deg = cfg.rotation_jitter * (rng(copy_idx + 500) * 2.0 - 1.0);
                    // Build a rect placeholder for symbol instances whose content
                    // we can't clone here (symbol lookup is outside this fn's scope).
                    // We create a small filled rect at (px, py) scaled by size_scale
                    // and rotated. A proper renderer would look up the symbol shapes.
                    let half = 10.0 * size_scale;
                    let fill = self.default_fill;
                    let stroke = self.default_stroke;
                    let sw = self.default_stroke_w;
                    let mut inst = Shape::rect(
                        [px - half, py - half, half * 2.0, half * 2.0],
                        fill,
                        stroke,
                        sw,
                    );
                    if rot_deg != 0.0 {
                        inst.apply_affine(&Affine::rotate_about(
                            rot_deg.to_radians(),
                            px,
                            py,
                        ));
                    }
                    new_shapes.push(inst);
                    dist += cfg.spacing;
                    copy_idx += 1;
                }
                if !new_shapes.is_empty() {
                    self.checkpoint();
                    self.doc.shapes.extend(new_shapes);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 7: Art Brush ---
            Action::SetArtBrush { symbol_id, width_scale, colorize, flip } => {
                self.art_brush = Some(ArtBrushConfig { symbol_id, width_scale, colorize, flip });
            }
            Action::ClearArtBrush => {
                self.art_brush = None;
            }
            Action::PaintArtBrushPath { path } => {
                let Some(cfg) = self.art_brush.clone() else { return; };
                if path.len() < 2 { return; }
                // Compute arc length.
                let mut arc: Vec<f32> = vec![0.0];
                for i in 1..path.len() {
                    let dx = path[i][0] - path[i-1][0];
                    let dy = path[i][1] - path[i-1][1];
                    arc.push(arc[i-1] + (dx*dx + dy*dy).sqrt());
                }
                let total = *arc.last().unwrap_or(&0.0);
                if total < 1.0 { return; }
                // Build a deformed polygon approximating the stretched symbol.
                // Sample the path at N points and produce a thin ribbon.
                let n = (total / 20.0).ceil() as usize + 1;
                let half_h = 10.0 * cfg.width_scale;
                let flip_sign = if cfg.flip { -1.0 } else { 1.0 };
                let mut pts_top: Vec<(f32, f32)> = Vec::with_capacity(n);
                let mut pts_bot: Vec<(f32, f32)> = Vec::with_capacity(n);
                for i in 0..=n {
                    let t = total * i as f32 / n as f32;
                    let (cx, cy) = sample_polyline(&path, &arc, t);
                    // Approximate tangent via finite difference.
                    let dt = total * 0.5 / n as f32;
                    let (ax, ay) = sample_polyline(&path, &arc, (t - dt).max(0.0));
                    let (bx, by) = sample_polyline(&path, &arc, (t + dt).min(total));
                    let tx = bx - ax; let ty = by - ay;
                    let len = (tx*tx + ty*ty).sqrt().max(1e-6);
                    let nx = -ty / len; let ny = tx / len;
                    pts_top.push((cx + nx * half_h * flip_sign, cy + ny * half_h * flip_sign));
                    pts_bot.push((cx - nx * half_h * flip_sign, cy - ny * half_h * flip_sign));
                }
                let mut poly: Vec<(f32, f32)> = pts_top;
                pts_bot.reverse();
                poly.extend(pts_bot);
                if poly.len() >= 3 {
                    self.checkpoint();
                    self.doc.shapes.push(Shape::path(
                        poly,
                        vec![],
                        true,
                        self.default_fill,
                        [0.0; 4],
                        0.0,
                    ));
                    self.host.mark_dirty();
                }
            }

            // --- Batch 7: Live Corners ---
            Action::SetPolygonCornerRadius(r) => {
                let r = r.max(0.0);
                let sel = self.selection.clone();
                for idx in sel {
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        if let Some(crate::liveshape::LiveShape::Polygon { sides, radius, .. }) = shape.live_shape() {
                            shape.set_live_shape(crate::liveshape::LiveShape::Polygon { sides, radius, corner_radius: r });
                        }
                    }
                }
                self.host.mark_dirty();
            }

            // --- Batch 7: Perspective Grid (active plane) ---
            Action::SetPerspectivePlane(plane) => {
                self.perspective_active_plane = plane.min(2);
            }
            Action::SnapToPerspectivePlane => {
                // Stub: record plane binding on selected shapes without full projection.
                // Full geometric projection requires the grid VP math which is a
                // larger refactor; this at minimum marks the plane as active.
                let _ = self.perspective_active_plane;
            }

            // --- Batch 7: Color Guide ---
            Action::SetColorGuide { rule, key_color } => {
                self.color_guide.rule = rule;
                self.color_guide.recompute(key_color);
            }
            Action::ApplyColorGuide(idx) => {
                if let Some(&color) = self.color_guide.swatches.get(idx) {
                    self.default_fill = color;
                    if self.selected_shape().is_some() {
                        self.checkpoint();
                        self.selected_shape_mut().unwrap().set_fill_color(color);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 7: Warp Tools ---
            Action::SetWarpToolKind(kind) => {
                self.warp_tool_kind = kind;
            }
            Action::SetWarpBrush { size, intensity, detail } => {
                self.warp_brush_size = size.max(1.0);
                self.warp_brush_intensity = intensity.clamp(0.0, 1.0);
                self.warp_detail = detail.max(0.0);
            }
            Action::ApplyWarpStroke { center, radius } => {
                let kind = self.warp_tool_kind;
                let intensity = self.warp_brush_intensity;
                let mut changed = false;
                for shape in self.doc.shapes.iter_mut() {
                    let points = match shape {
                        Shape::Path { ref mut points, .. } => points,
                        _ => continue,
                    };
                    for (i, pt) in points.iter_mut().enumerate() {
                        let dx = pt.0 - center.0;
                        let dy = pt.1 - center.1;
                        let dist = (dx*dx + dy*dy).sqrt();
                        if dist >= radius { continue; }
                        let weight = intensity * (1.0 - dist / radius);
                        match kind {
                            WarpToolKind::Scallop => {
                                pt.0 -= dx * weight;
                                pt.1 -= dy * weight;
                            }
                            WarpToolKind::Crystallize => {
                                pt.0 += dx * weight;
                                pt.1 += dy * weight;
                            }
                            WarpToolKind::Wrinkle => {
                                let seed = i.wrapping_mul(37).wrapping_add(1);
                                let jitter = ((seed % 17) as f32 / 17.0 * 2.0 - 1.0) * weight * radius * 0.2;
                                pt.0 += -dy / (dist + 1e-6) * jitter;
                                pt.1 +=  dx / (dist + 1e-6) * jitter;
                            }
                        }
                        changed = true;
                    }
                }
                if changed {
                    self.checkpoint();
                    self.host.mark_dirty();
                }
            }


            // --- Batch 8: Image Trace (extended) ---
            Action::SetImageTrace { mode, threshold, colors } => {
                self.image_trace_mode = mode;
                self.image_trace_threshold = threshold.clamp(0.0, 255.0);
                self.image_trace_colors = colors.max(2);
            }
            Action::ApplyImageTrace => {
                let n = (self.image_trace_colors as usize).min(6);
                self.checkpoint();
                for i in 0..n {
                    let t = i as f32 / n.max(1) as f32;
                    let fill = match self.image_trace_mode {
                        ImageTraceMode::Grayscale => [t, t, t, 1.0],
                        ImageTraceMode::BlackWhite | ImageTraceMode::BlackAndWhite => {
                            if t < 0.5 { [0.0, 0.0, 0.0, 1.0] } else { [1.0, 1.0, 1.0, 1.0] }
                        }
                        ImageTraceMode::Outlined | ImageTraceMode::Silhouette => [0.0, 0.0, 0.0, 0.0],
                        ImageTraceMode::Sketch | ImageTraceMode::Technical => [0.1, 0.1, 0.1, 1.0],
                        _ => {
                            [t, 1.0 - t * 0.5, 0.3 + t * 0.4, 1.0]
                        }
                    };
                    self.doc.shapes.push(Shape::rect(
                        [i as f32 * 20.0, 0.0, 15.0, 15.0],
                        fill,
                        [0.0; 4],
                        0.0,
                    ));
                }
                self.host.mark_dirty();
            }
            Action::ExpandImageTrace => {
                self.image_trace_expanded = true;
            }

            // --- Batch 8: Opacity Masks ---
            Action::MakeOpacityMask => {
                if self.selection.len() < 2 {
                    return;
                }
                let n = self.selection.len();
                let bottom_idx = self.selection[n - 2];
                let top_idx = self.selection[n - 1];
                // Allocate a unique id for this mask set.
                let mask_id = self.omask_id_counter;
                self.omask_id_counter += 1;
                self.checkpoint();
                // TOP shape → mask path (luminance source).
                if top_idx < self.doc.shapes.len() {
                    self.doc.shapes[top_idx].set_omask(Some(mask_id));
                    self.doc.shapes[top_idx].set_omask_path(true);
                }
                // BOTTOM shape → masked content.
                if bottom_idx < self.doc.shapes.len() {
                    self.doc.shapes[bottom_idx].set_omask(Some(mask_id));
                    self.doc.shapes[bottom_idx].set_omask_path(false);
                }
                self.host.mark_dirty();
            }
            Action::ReleaseOpacityMask => {
                // Collect all omask ids present in the selection.
                let ids: std::collections::HashSet<u64> = self
                    .selection
                    .iter()
                    .filter_map(|&i| self.doc.shapes.get(i)?.omask())
                    .collect();
                if ids.is_empty() {
                    return;
                }
                self.checkpoint();
                for shape in self.doc.shapes.iter_mut() {
                    if let Some(id) = shape.omask() {
                        if ids.contains(&id) {
                            shape.clear_omask();
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::InvertOpacityMask => {
                let mut changed = false;
                let sel: Vec<usize> = self.selection.clone();
                for &i in &sel {
                    if let Some(s) = self.doc.shapes.get_mut(i) {
                        if s.omask().is_some() && !s.is_omask() {
                            let cur = s.omask_invert();
                            s.set_omask_invert(!cur);
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.host.mark_dirty();
                }
            }

            // --- Batch 8: Symbol extras ---
            Action::BreakSymbolLink { shape_idx } => {
                log::info!("BreakSymbolLink({shape_idx}) — stub: instance detached");
            }
            Action::ExpandSymbol(sym_id) => {
                log::info!("ExpandSymbol({sym_id}) — stub: all instances converted to editable copies");
            }

            // --- Batch 8: Graph Tool ---
            Action::SetGraphType(t) => {
                self.graph_data.graph_type = t;
            }
            Action::SetGraphData { cols, rows, values } => {
                self.graph_data.cols = cols;
                self.graph_data.rows = rows;
                self.graph_data.values = values;
            }
            Action::SetGraphLabels(l) => {
                self.graph_data.labels = l;
            }
            Action::SetGraphStyleFill(c) => {
                self.graph_style_fill = c;
            }
            Action::ToggleGraphLegend => {
                self.graph_show_legend = !self.graph_show_legend;
            }
            Action::ApplyGraph { x, y, width, height } => {
                let values = self.graph_data.values.clone();
                let n = values.len().min(self.graph_data.cols * self.graph_data.rows).max(1);
                let max_val = values.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
                let bar_w = width / n as f32 * 0.8;
                let gap = width / n as f32 * 0.2;
                let fill = self.graph_style_fill;
                self.checkpoint();
                for (i, &v) in values.iter().take(n).enumerate() {
                    let bar_h = (v / max_val) * height;
                    let bx = x + i as f32 * (bar_w + gap);
                    let by = y + height - bar_h;
                    let shade = (i as f32 / n as f32 * 0.4 + 0.8).min(1.0);
                    let c = [fill[0] * shade, fill[1] * shade, fill[2] * shade, fill[3]];
                    self.doc.shapes.push(Shape::rect([bx, by, bar_w, bar_h], c, [0.0; 4], 0.0));
                }
                self.host.mark_dirty();
            }

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
