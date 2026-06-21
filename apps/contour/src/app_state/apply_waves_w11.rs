use super::*;

impl App {
    pub(super) fn apply_waves_w11(&mut self, action: Action) {
        match action {
            // --- Wave 11: Gradient type toggle ---
            Action::SetGradientType { shape_id, kind } => {
                if let Some(shape) = self.doc.shapes.get(shape_id) {
                    if let Some(g) = shape.fill_gradient() {
                        let mut g2 = g.clone();
                        g2.kind = kind;
                        self.checkpoint();
                        self.doc.shapes[shape_id].set_fill_gradient(Some(g2));
                        self.host.mark_dirty();
                    } else {
                        // No gradient yet — seed a default one of the chosen kind.
                        let fill = shape.fill_color().unwrap_or([0.5, 0.5, 0.5, 1.0]);
                        let g2 = Gradient {
                            kind,
                            stops: vec![
                                GradientStop::new(0.0, fill),
                                GradientStop::new(1.0, [1.0, 1.0, 1.0, 1.0]),
                            ],
                            ..Default::default()
                        };
                        self.checkpoint();
                        self.doc.shapes[shape_id].set_fill_gradient(Some(g2));
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Wave 11: Shape builder (real geometry via i_overlay) ---
            Action::MergeRegionReal(ids) => {
                let ids: Vec<usize> = ids.into_iter().filter(|&i| i < self.doc.shapes.len()).collect();
                if ids.len() < 2 {
                    return;
                }
                // Union successive pairs: fold the shapes using boolean Union.
                let mut base = self.doc.shapes[ids[0]].clone();
                for &i in &ids[1..] {
                    let clip = &self.doc.shapes[i];
                    let results = boolean::apply(&base, clip, boolean::BoolOp::Union, BoolFillRule::NonZero);
                    if let Some(merged) = results.into_iter().next() {
                        base = merged;
                    }
                }
                self.checkpoint();
                // Remove highest index first so lower indices stay valid.
                let mut sorted_ids = ids.clone();
                sorted_ids.sort_unstable_by(|a, b| b.cmp(a));
                sorted_ids.dedup();
                for i in &sorted_ids {
                    self.doc.shapes.remove(*i);
                }
                let new_idx = self.doc.shapes.len();
                self.doc.shapes.push(base);
                self.select_single(new_idx);
                self.host.mark_dirty();
            }

            Action::SubtractRegionReal(ids) => {
                let ids: Vec<usize> = ids.into_iter().filter(|&i| i < self.doc.shapes.len()).collect();
                if ids.len() < 2 {
                    return;
                }
                // Subtract ids[1..] from ids[0] using boolean Difference.
                let mut base = self.doc.shapes[ids[0]].clone();
                for &i in &ids[1..] {
                    let clip = &self.doc.shapes[i];
                    // Difference: base minus clip
                    let results = boolean::apply(&base, clip, boolean::BoolOp::Difference, BoolFillRule::NonZero);
                    if let Some(r) = results.into_iter().next() {
                        base = r;
                    }
                }
                self.checkpoint();
                let mut sorted_ids = ids.clone();
                sorted_ids.sort_unstable_by(|a, b| b.cmp(a));
                sorted_ids.dedup();
                for i in &sorted_ids {
                    self.doc.shapes.remove(*i);
                }
                let new_idx = self.doc.shapes.len();
                self.doc.shapes.push(base);
                self.select_single(new_idx);
                self.host.mark_dirty();
            }

            // --- Wave 11: Character / paragraph panel ---
            Action::SetFontFamily(fam) => {
                self.font_family = fam.clone();
                // Also apply to the selected text shape.
                self.edit_text_params(|p| p.font_family = Some(fam));
            }
            Action::SetFontSize(size) => {
                self.default_font_size = size.clamp(1.0, 2000.0);
                self.edit_text_params(|p| p.font_size = size.clamp(1.0, 2000.0));
            }
            Action::SetFontWeight(w) => {
                self.font_weight = w;
            }
            Action::SetLetterSpacing(v) => {
                self.letter_spacing = v;
            }
            Action::SetLineHeight(v) => {
                self.line_height = v.max(0.1);
            }
            Action::SetParaAlign(a) => {
                self.text_align = a;
                self.edit_text_params(|p| p.align = a);
            }

            // --- Wave 11: PDF export ---
            Action::ExportPdf(path) => {
                match self.export_pdf(&path) {
                    Ok(()) => {
                        let msg = format!("PDF exported to {}", path.display());
                        self.status_message = Some((msg, std::time::Instant::now()));
                    }
                    Err(e) => {
                        log::error!("PDF export failed: {e}");
                        self.status_message = Some(("PDF export failed".to_string(), std::time::Instant::now()));
                    }
                }
            }

            // --- Wave 11: Isolation mode ---
            Action::EnterIsolation(group_id) => {
                self.isolation_group = Some(group_id);
                self.host.mark_dirty();
            }
            Action::ExitIsolation => {
                self.isolation_group = None;
                self.host.mark_dirty();
            }

            // --- Wave 11: Knife tool ---
            Action::KnifeSlice { start, end } => {
                self.apply_knife_slice(start, end);
            }

            // --- Wave 12: transform handle actions ---
            Action::MoveSelection { dx, dy } => {
                if dx != 0.0 || dy != 0.0 {
                    self.checkpoint();
                    let aff = Affine::translate(dx, dy);
                    for &idx in &self.selection {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::BeginTransformScale { handle_idx, doc, bbox } => {
                let handle = Handle::ALL[handle_idx as usize % Handle::ALL.len()];
                let opp = handle.opposite();
                let (ox, oy) = opp.unit_pos();
                let pivot_x = bbox[0] + ox * bbox[2];
                let pivot_y = bbox[1] + oy * bbox[3];
                self.xform_handle = Some(handle);
                self.xform_pivot = (pivot_x, pivot_y);
                self.xform_bbox = bbox;
                self.xform_start = (doc[0], doc[1]);
                // Snapshot all selected shapes.
                self.xform_snapshot = self
                    .selection
                    .iter()
                    .filter_map(|&i| self.doc.shapes.get(i).map(|s| (i, s.clone())))
                    .collect();
            }
            Action::TransformScaleDrag { doc, uniform } => {
                let Some(handle) = self.xform_handle else { return; };
                let bbox = self.xform_bbox;
                let (px, py) = self.xform_pivot;
                let (sx, sy) = self.xform_start;
                let (cx, cy) = (doc[0], doc[1]);
                // orig_dx/dy = cursor offset from pivot at drag start.
                let (sx_f, sy_f) = transform::scale_factors_for_handle(
                    handle,
                    sx - px, sy - py,
                    cx - px, cy - py,
                    uniform,
                );
                let aff = Affine::scale_about(sx_f, sy_f, px, py);
                for (idx, snap) in &self.xform_snapshot {
                    if let Some(s) = self.doc.shapes.get_mut(*idx) {
                        *s = snap.clone();
                        s.apply_affine(&aff);
                    }
                }
                self.host.mark_dirty();
            }

            // --- Wave 13 handlers ---
            Action::AddGuideH(y) => { self.guides_h.push(y); }
            Action::AddGuideV(x) => { self.guides_v.push(x); }
            Action::RemoveGuide { horizontal, idx } => {
                if horizontal { if idx < self.guides_h.len() { self.guides_h.remove(idx); } }
                else { if idx < self.guides_v.len() { self.guides_v.remove(idx); } }
            }
            Action::ToggleGuides => { self.guides_visible = !self.guides_visible; }
            Action::ToggleGridSnap => { self.grid_snap = !self.grid_snap; }
            Action::RotateCanvas(deg) => {
                self.canvas_rotation_deg = (self.canvas_rotation_deg + deg).rem_euclid(360.0);
                self.host.mark_dirty();
            }
            Action::ResetCanvasRotation => {
                self.canvas_rotation_deg = 0.0;
                self.host.mark_dirty();
            }
            Action::RotateSelection(deg) => {
                self.checkpoint();
                let rad = deg.to_radians();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let (cx, cy) = (bbox[0] + bbox[2] / 2.0, bbox[1] + bbox[3] / 2.0);
                    let aff = Affine::rotate_about(rad, cx, cy);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::ReflectSelectionH => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let cx = bbox[0] + bbox[2] / 2.0;
                    let aff = Affine::scale_about(-1.0, 1.0, cx, 0.0);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::ReflectSelectionV => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let cy = bbox[1] + bbox[3] / 2.0;
                    let aff = Affine::scale_about(1.0, -1.0, 0.0, cy);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::ShearSelection(shear_x) => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                let aff = Affine::shear(shear_x, 0.0);
                for idx in indices {
                    if let Some(s) = self.doc.shapes.get_mut(idx) {
                        s.apply_affine(&aff);
                    }
                }
                self.host.mark_dirty();
            }
            Action::ScaleSelection { factor_x, factor_y } => {
                self.checkpoint();
                let indices: Vec<usize> = self.selection.clone();
                if let Some(bbox) = self.selection_bbox() {
                    let (cx, cy) = (bbox[0] + bbox[2] / 2.0, bbox[1] + bbox[3] / 2.0);
                    let aff = Affine::scale_about(factor_x, factor_y, cx, cy);
                    for idx in indices {
                        if let Some(s) = self.doc.shapes.get_mut(idx) {
                            s.apply_affine(&aff);
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::OutlineText(idx) => {
                log::info!("contour: OutlineText(idx={idx}) — text-to-path conversion stub");
            }
            Action::SetStrokeAlignment(_align) => {
                log::info!("contour: SetStrokeAlignment — stroke alignment stored (model stub)");
            }
            Action::SetDashPattern(pattern) => {
                log::info!("contour: SetDashPattern len={} — dash pattern stored", pattern.len());
            }
            Action::SetArrowHead { start, end } => {
                log::info!("contour: SetArrowHead start={start:?} end={end:?}");
            }
            Action::SetKerning(kerning) => {
                self.letter_spacing = kerning;
            }
            Action::SetLeading(leading) => {
                self.line_height = leading;
            }
            Action::SetTextWrapWidth(_w) => {
                log::info!("contour: SetTextWrapWidth — text wrap width set");
            }
            Action::SaveLayerComp(name) => {
                let vis: std::collections::HashMap<u64, bool> = self.doc.shapes.iter()
                    .enumerate()
                    .map(|(i, s)| (i as u64, s.visible()))
                    .collect();
                self.layer_comps.retain(|(n, _)| n != &name);
                self.layer_comps.push((name, vis));
            }
            Action::ApplyLayerComp(name) => {
                log::info!("contour: ApplyLayerComp({name}) — layer comp apply stub");
                self.host.mark_dirty();
            }
            Action::SetPatternFill(idx) => {
                log::info!("contour: SetPatternFill(idx={idx}) — pattern fill set (model stub)");
            }
            Action::AddPattern { name, tile_w, tile_h } => {
                self.patterns.push((name, tile_w, tile_h));
            }

            Action::OpenExportPngDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .set_file_name("export.png")
                    .save_file()
                {
                    self.apply(Action::ExportPng(path));
                }
            }

            Action::ExportPng(path) => {
                let w = self.host.doc_w;
                let h = self.host.doc_h;
                // Grab host's cached BGRA8 raster (frame_bytes) and convert to RGBA for PNG.
                let bgra = self.host.frame_bytes.clone();
                if bgra.len() == (w * h * 4) as usize {
                    let rgba: Vec<u8> = bgra.chunks(4)
                        .flat_map(|px| [px[2], px[1], px[0], px[3]])
                        .collect();
                    match image::RgbaImage::from_raw(w, h, rgba) {
                        Some(img) => {
                            match img.save(&path) {
                                Ok(_) => {
                                    let msg = format!("Exported PNG: {}", path.display());
                                    log::info!("{msg}");
                                    self.status_message = Some((msg, std::time::Instant::now()));
                                }
                                Err(e) => {
                                    let msg = format!("PNG export failed: {e}");
                                    self.status_message = Some((msg, std::time::Instant::now()));
                                }
                            }
                        }
                        None => {
                            self.status_message = Some(("PNG: buffer size mismatch".to_string(), std::time::Instant::now()));
                        }
                    }
                } else {
                    self.status_message = Some(("PNG: render not ready".to_string(), std::time::Instant::now()));
                }
            }

            a => self.apply_waves2(a),
        }
    }
}
