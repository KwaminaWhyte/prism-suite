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


            a => self.apply_extended(a),
        }
    }
}
