use super::*;

impl App {
    pub(super) fn apply_waves2_b24(&mut self, action: Action) {
        match action {
            // --- Batch 2: Variable font axes ---
            Action::SetFontAxis { axis, value } => {
                self.edit_text_params(|p| {
                    p.font_axes.insert(axis, value);
                });
            }

            // --- Batch 2: Symbol library (real) ---
            Action::DefineSymbol(name) => {
                let shapes: Vec<Shape> = self.selection.iter()
                    .filter_map(|&i| self.doc.shapes.get(i).cloned())
                    .collect();
                if !shapes.is_empty() {
                    self.symbol_lib.add(&name, shapes);
                }
            }
            Action::PlaceSymbol2(sym_id) => {
                if let Some(inst_id) = self.symbol_lib.place(
                    sym_id,
                    crate::transform::Affine::translate(180.0, 180.0),
                ) {
                    let resolved = {
                        let inst = self.symbol_lib.instance(inst_id).unwrap();
                        self.symbol_lib.resolve(inst)
                    };
                    self.checkpoint();
                    for s in resolved {
                        self.doc.shapes.push(s);
                    }
                    let last = self.doc.shapes.len().saturating_sub(1);
                    self.select_single(last);
                    self.host.mark_dirty();
                }
            }
            Action::EditSymbol2(id) => {
                log::info!("EditSymbol2({id}) — stub");
            }
            Action::DeleteSymbol(id) => {
                self.symbol_lib.remove(id);
            }

            // --- Batch 2: Multiple artboards (named) ---
            Action::AddArtboard2 { rect } => {
                let id = self.artboard_next_id;
                self.artboard_next_id += 1;
                let name = crate::artboard::default_name(self.artboard_entries.len());
                self.artboard_entries.push(ArtboardEntry::new(id, name, rect));
                self.artboards.push(rect);
                self.active_artboard = Some(id);
            }
            Action::RemoveArtboard(idx) => {
                if idx < self.artboard_entries.len() {
                    self.artboard_entries.remove(idx);
                    if idx < self.artboards.len() {
                        self.artboards.remove(idx);
                    }
                    if self.active_artboard.map_or(false, |_| true) {
                        self.active_artboard = self.artboard_entries.first().map(|e| e.id);
                    }
                }
            }
            Action::RenameArtboard { idx, name } => {
                if let Some(entry) = self.artboard_entries.get_mut(idx) {
                    entry.name = name;
                }
            }
            Action::SelectArtboard(id) => {
                if self.artboard_entries.iter().any(|e| e.id == id) {
                    self.active_artboard = Some(id);
                }
            }
            Action::DuplicateArtboard(idx) => {
                if let Some(entry) = self.artboard_entries.get(idx).cloned() {
                    let new_id = self.artboard_next_id;
                    self.artboard_next_id += 1;
                    let new_rect = [
                        entry.rect[0] + 20.0,
                        entry.rect[1] + 20.0,
                        entry.rect[2],
                        entry.rect[3],
                    ];
                    let new_name = format!("{} copy", entry.name);
                    self.artboard_entries.push(ArtboardEntry::new(new_id, new_name, new_rect));
                    self.artboards.push(new_rect);
                    self.active_artboard = Some(new_id);
                }
            }

            // --- Batch 2: Graph / chart tool stub ---
            Action::InsertGraph { kind, rect } => {
                self.checkpoint();
                let sample_data: &[&[f32]] = &[&[10.0, 20.0, 15.0], &[5.0, 25.0, 30.0]];
                let [gx, gy, gw, gh] = rect;
                match kind {
                    GraphKind::Bar => {
                        let series_count = sample_data.len() as f32;
                        let bar_count = sample_data[0].len();
                        let group_w = gw / bar_count as f32;
                        let max_val = sample_data.iter().flat_map(|s| s.iter()).cloned().fold(0.0_f32, f32::max);
                        for (si, series) in sample_data.iter().enumerate() {
                            for (bi, &val) in series.iter().enumerate() {
                                let bw = group_w / (series_count + 1.0);
                                let bx = gx + bi as f32 * group_w + si as f32 * bw;
                                let bh = (val / max_val.max(0.001)) * gh;
                                let by = gy + gh - bh;
                                let hue = si as f32 / series_count;
                                let fill = [0.3 + hue * 0.5, 0.5, 0.9 - hue * 0.4, 1.0];
                                self.doc.shapes.push(Shape::rect([bx, by, bw * 0.9, bh], fill, self.default_stroke, 0.5));
                            }
                        }
                    }
                    GraphKind::Pie => {
                        let all_vals: Vec<f32> = sample_data[0].to_vec();
                        let total: f32 = all_vals.iter().sum();
                        let cx_pt = gx + gw * 0.5;
                        let cy_pt = gy + gh * 0.5;
                        let r = gw.min(gh) * 0.45;
                        let mut angle = 0.0_f32;
                        for (i, &val) in all_vals.iter().enumerate() {
                            let sweep = (val / total.max(0.001)) * std::f32::consts::TAU;
                            let a1 = angle;
                            let a2 = angle + sweep;
                            let fill = [
                                0.3 + i as f32 * 0.2,
                                0.6 - i as f32 * 0.1,
                                0.8,
                                1.0,
                            ];
                            let px1 = cx_pt + r * a1.cos();
                            let py1 = cy_pt + r * a1.sin();
                            let px2 = cx_pt + r * a2.cos();
                            let py2 = cy_pt + r * a2.sin();
                            let points = vec![(cx_pt, cy_pt), (px1, py1), (px2, py2)];
                            let handles = vec![(0.0_f32, 0.0_f32); points.len()];
                            self.doc.shapes.push(Shape::path(points, handles, true, fill, self.default_stroke, 0.5));
                            angle += sweep;
                        }
                    }
                    GraphKind::Line | GraphKind::Scatter => {
                        let series_count = sample_data.len();
                        for (si, series) in sample_data.iter().enumerate() {
                            let n = series.len();
                            let max_val = series.iter().cloned().fold(0.0_f32, f32::max);
                            let hue = si as f32 / series_count as f32;
                            let stroke = [0.3 + hue * 0.5, 0.5, 0.9 - hue * 0.4, 1.0];
                            let pts: Vec<(f32, f32)> = series.iter().enumerate().map(|(i, &v)| {
                                let px = gx + (i as f32 / (n - 1).max(1) as f32) * gw;
                                let py = gy + gh - (v / max_val.max(0.001)) * gh;
                                (px, py)
                            }).collect();
                            let handles = vec![(0.0_f32, 0.0_f32); pts.len()];
                            self.doc.shapes.push(Shape::path(pts, handles, false, [0.0; 4], stroke, 1.5));
                        }
                    }
                }
                let last = self.doc.shapes.len().saturating_sub(1);
                self.select_single(last);
                self.host.mark_dirty();
            }

            // --- Batch 2: Stroke width profile presets ---
            Action::ApplyWidthPreset(preset) => {
                if let Some(idx) = self.selected {
                    let (start, end) = preset.profile();
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        shape.stroke_style_mut().width_profile = (start, end);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 3: Live Paint ---
            Action::ApplyLivePaint { fill } => {
                // Simple implementation: set fill on the topmost closed shape containing
                // the last clicked point. Caller should have pre-selected via HitTestSelect.
                if let Some(idx) = self.selected {
                    if idx < self.doc.shapes.len() {
                        self.checkpoint();
                        self.doc.shapes[idx].set_fill_color(fill);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Batch 3: Type on Path ---
            Action::PlaceTextOnPath => {
                // Find exactly one text and one non-text shape in the selection.
                let text_idx = self.selection.iter().copied()
                    .find(|&i| self.doc.shapes.get(i).map_or(false, |s| s.text_params().is_some()));
                let path_idx = self.selection.iter().copied()
                    .find(|&i| self.doc.shapes.get(i).map_or(false, |s| s.text_params().is_none()));
                if let (Some(tid), Some(pid)) = (text_idx, path_idx) {
                    self.checkpoint();
                    self.text_on_path.insert(tid, pid);
                    self.relayout_text_on_path(tid);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Perspective Distort ---
            Action::SetPerspectiveDistort { shape_id, corners } => {
                if shape_id < self.doc.shapes.len() {
                    self.perspective_distort_corners = corners;
                    self.perspective_distort_active = true;
                    self.host.mark_dirty();
                }
            }
            Action::ConfirmPerspectiveDistort => {
                log::info!(
                    "ConfirmPerspectiveDistort — corners: {:?} (homography warp stub)",
                    self.perspective_distort_corners
                );
                self.perspective_distort_active = false;
            }

            // --- Batch 3: Recolor Artwork HSL ---
            Action::OpenRecolorHSLPanel => {
                self.recolor_hsl_open = true;
            }
            Action::CloseRecolorHSLPanel => {
                self.recolor_hsl_open = false;
            }
            Action::RecolorArtworkHSL { hue_shift, saturation_scale, brightness_scale } => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(shape) = self.doc.shapes.get_mut(i) {
                            if let Some(c) = shape.fill_color() {
                                let nc = crate::recolor::shift_hsl(
                                    c, hue_shift, saturation_scale, brightness_scale,
                                );
                                shape.set_fill_color(nc);
                            }
                            if let Some(c) = shape.stroke_color() {
                                let nc = crate::recolor::shift_hsl(
                                    c, hue_shift, saturation_scale, brightness_scale,
                                );
                                shape.set_stroke_color(nc);
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 3: Align to Artboard ---
            Action::AlignToArtboard { alignment } => {
                let artboard_rect = self.active_artboard
                    .and_then(|id| self.artboard_entries.iter().find(|e| e.id == id))
                    .map(|e| CoreRect::new(e.rect[0], e.rect[1], e.rect[2], e.rect[3]))
                    .or_else(|| self.artboards.first().map(|r| CoreRect::new(r[0], r[1], r[2], r[3])));
                if let Some(frame) = artboard_rect {
                    let indices: Vec<usize> = self.selection.clone();
                    let boxes: Vec<CoreRect> = indices.iter()
                        .filter_map(|&i| self.doc.shapes.get(i)?.bounds())
                        .collect();
                    if !boxes.is_empty() {
                        self.checkpoint();
                        let deltas = align::align_deltas(&boxes, alignment, frame);
                        let mut moved = false;
                        for (k, &i) in indices.iter().enumerate() {
                            if k < deltas.len() {
                                let (dx, dy) = deltas[k];
                                if (dx != 0.0 || dy != 0.0) && i < self.doc.shapes.len() {
                                    self.doc.shapes[i].translate(dx, dy);
                                    moved = true;
                                }
                            }
                        }
                        if moved {
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            // --- Batch 4: Envelope Distort ---
            Action::MakeEnvelopeWithMesh { shape_id, rows, cols } => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    let bbox = self.doc.shapes[shape_id].bounds()
                        .map(|b| [b.x, b.y, b.w, b.h])
                        .unwrap_or([0.0, 0.0, 100.0, 100.0]);
                    let mesh = crate::envelope::EnvelopeMesh::new(rows, cols, bbox);
                    self.doc.shapes[shape_id].set_envelope_mesh(Some(mesh));
                    self.host.mark_dirty();
                }
            }
            Action::MoveEnvelopePoint { shape_id, idx, pos } => {
                let has_point = self.doc.shapes.get(shape_id)
                    .and_then(|s| s.envelope_mesh())
                    .map_or(false, |m| idx < m.points.len());
                if has_point {
                    self.history.begin(&self.doc);
                    if let Some(mesh) = self.doc.shapes[shape_id].envelope_mesh_mut() {
                        mesh.points[idx] = pos;
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ReleaseEnvelope(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_envelope_mesh(None);
                    self.host.mark_dirty();
                }
            }

            // --- Batch 4: Live Effects ---
            Action::AppearanceAddGlow => {
                if let Some(idx) = self.selected {
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        if shape.appearance().is_none() {
                            let ea = shape.effective_appearance();
                            *shape.appearance_mut() = Some(ea);
                        }
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            ap.effects.push(crate::appearance::Effect::glow());
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleEffect { shape_id, index } => {
                log::info!("ToggleEffect shape={shape_id} index={index} — effect toggle not yet destructive; use RemoveEffect to remove");
            }
            Action::RemoveEffect { shape_id, index } => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    if let Some(ap) = self.doc.shapes[shape_id].appearance_mut().as_mut() {
                        if index < ap.effects.len() {
                            ap.effects.remove(index);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ReorderEffect { shape_id, from, to } => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    if let Some(ap) = self.doc.shapes[shape_id].appearance_mut().as_mut() {
                        if from < ap.effects.len() && to < ap.effects.len() {
                            ap.effects.swap(from, to);
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Batch 4: Group blend modes ---
            Action::SetGroupBlendMode { group_id, mode } => {
                self.group_blend_modes.insert(group_id, mode);
                self.host.mark_dirty();
            }
            Action::SetGroupOpacity { group_id, opacity } => {
                self.group_opacities.insert(group_id, opacity.clamp(0.0, 1.0));
                self.host.mark_dirty();
            }

            // --- Batch 4: Pathfinder improvements ---
            Action::OutlineStroke(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    let stroke_w = self.doc.shapes[shape_id].stroke_width();
                    let stroke_color = self.doc.shapes[shape_id].stroke_color().unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    if let Some(pts) = self.doc.shapes[shape_id].outline_polygon() {
                        self.checkpoint();
                        let half_w = stroke_w * 0.5;
                        let outer = crate::stroke::offset_contour(&pts, half_w, true);
                        let inner = crate::stroke::offset_contour(&pts, -half_w, true);
                        use crate::document::{Shape, SubPath};
                        use crate::document::FillRule;
                        let outline_shape = Shape::Compound {
                            subpaths: vec![
                                SubPath::ring(outer),
                                SubPath::ring(inner),
                            ],
                            fill_rule: FillRule::EvenOdd,
                            fill: stroke_color,
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
                        };
                        let insert_at = shape_id + 1;
                        if insert_at <= self.doc.shapes.len() {
                            self.doc.shapes.insert(insert_at, outline_shape);
                        } else {
                            self.doc.shapes.push(outline_shape);
                        }
                        self.select_single(shape_id + 1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ExpandAppearance(shape_id) => {
                if shape_id < self.doc.shapes.len() {
                    self.checkpoint();
                    if let Some(ap) = self.doc.shapes[shape_id].appearance_mut().as_mut() {
                        ap.effects.clear();
                    }
                    self.host.mark_dirty();
                    log::info!("ExpandAppearance({shape_id}) — effects cleared (baked stub)");
                }
            }

            a => self.apply_advanced(a),
        }
    }
}
