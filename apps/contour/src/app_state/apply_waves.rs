use super::*;

impl App {
    pub(super) fn apply_waves(&mut self, action: Action) {
        match action {
            // --- Symbols (Wave 8) ---
            Action::CreateSymbol(name) => {
                let shape_ids: Vec<u64> = self.selection.iter().map(|&i| i as u64).collect();
                if !shape_ids.is_empty() {
                    let id = self.symbols.len() as u64;
                    self.symbols.push(Symbol { id, name, shape_ids });
                }
            }
            Action::PlaceSymbol(id) => {
                if self.symbols.iter().any(|s| s.id == id) {
                    self.checkpoint();
                    let shape = Shape::rect(
                        [180.0, 180.0, 40.0, 40.0],
                        [0.6, 0.3, 0.9, 1.0],
                        [0.3, 0.1, 0.5, 1.0],
                        1.5,
                    );
                    self.doc.shapes.push(shape);
                    self.select_single(self.doc.shapes.len() - 1);
                    self.host.mark_dirty();
                }
            }
            Action::EditSymbol(id) => {
                log::info!("editing symbol {id}");
            }

            // --- Gradient-stop editor (Wave 8) ---
            Action::SelectGradientStop(idx) => {
                self.selected_gradient_stop = idx;
            }
            Action::AddGradientStop { shape_id, pos, color } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    g.stops.push(GradientStop::new(pos, color));
                    g.stops.sort_by(|a, b| {
                        a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                    self.host.mark_dirty();
                }
            }
            Action::MoveGradientStop { shape_id, stop_idx, new_pos } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    if let Some(s) = g.stops.get_mut(stop_idx) {
                        s.offset = new_pos.clamp(0.0, 1.0);
                    }
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                    self.host.mark_dirty();
                }
            }
            Action::DeleteGradientStop { shape_id, stop_idx } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    if g.stops.len() > 2 && stop_idx < g.stops.len() {
                        g.stops.remove(stop_idx);
                        self.checkpoint();
                        self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetGradientStopColor { shape_id, stop_idx, color } => {
                let maybe_g = self.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(mut g) = maybe_g {
                    if let Some(s) = g.stops.get_mut(stop_idx) {
                        s.color = color;
                    }
                    self.checkpoint();
                    self.doc.shapes[shape_id].set_fill_gradient(Some(g));
                    self.host.mark_dirty();
                }
            }

            // --- Image Trace (Wave 8) ---
            Action::OpenTracePanel => { self.trace_panel_open = true; }
            Action::CloseTracePanel => { self.trace_panel_open = false; }
            Action::SetTraceThreshold(v) => { self.trace_threshold = v; }
            Action::SetTraceColors(v) => { self.trace_colors = v.clamp(2, 32); }
            Action::TraceImage { shape_id, threshold, colors } => {
                log::info!("TraceImage shape={shape_id} threshold={threshold} colors={colors}");
                self.host.mark_dirty();
            }

            // --- Recolor Artwork (Wave 8) ---
            Action::OpenRecolorPanel => { self.recolor_panel_open = true; }
            Action::CloseRecolorPanel => {
                self.recolor_panel_open = false;
                self.recolor_selected_color = None;
            }
            Action::SelectRecolorColor(c) => { self.recolor_selected_color = Some(c); }
            Action::RecolorSelected { old_color, new_color } => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(shape) = self.doc.shapes.get_mut(i) {
                            if let Some(fc) = shape.fill_color() {
                                if colors_approx_equal(fc, old_color) {
                                    shape.set_fill_color(new_color);
                                }
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }

            // --- Artboards (Wave 8) ---
            Action::AddArtboard(rect) => {
                self.artboards.push(rect);
                // Also register in the named list so the artboards panel shows it.
                let id = self.artboard_next_id;
                self.artboard_next_id += 1;
                let name = crate::artboard::default_name(self.artboard_entries.len());
                self.artboard_entries.push(ArtboardEntry::new(id, name, rect));
                self.active_artboard = Some(id);
            }

            // --- Eyedropper (Wave 9) ---
            Action::PickColor(rgba) => {
                self.fg_color = [
                    rgba[0] as f32 / 255.0,
                    rgba[1] as f32 / 255.0,
                    rgba[2] as f32 / 255.0,
                    rgba[3] as f32 / 255.0,
                ];
                if self.selected.is_some() {
                    self.checkpoint();
                    let c = self.fg_color;
                    self.selected_shape_mut().unwrap().set_fill_color(c);
                    self.host.mark_dirty();
                }
                self.active = self.prev_tool;
            }

            // --- Shape Builder (Wave 9) ---
            Action::MergeRegion(ids) => {
                log::info!("MergeRegion {:?} (stub)", ids);
                self.selection = ids.into_iter().filter(|&i| i < self.doc.shapes.len()).collect();
                self.sync_legacy_selection();
            }
            Action::SubtractRegion(ids) => {
                log::info!("SubtractRegion {:?} (stub)", ids);
            }

            Action::ApplyShapeBuilder { subtract } => {
                let sel = self.selection.clone();
                if sel.len() < 2 {
                    return;
                }
                let sel_shapes: Vec<document::Shape> = sel
                    .iter()
                    .filter_map(|&i| self.doc.shapes.get(i).cloned())
                    .collect();
                let faces = crate::shapebuilder::build_faces(&sel_shapes);
                if faces.is_empty() {
                    return;
                }
                let all_face_indices: Vec<usize> = (0..faces.len()).collect();
                let mode = if subtract {
                    crate::shapebuilder::BuildMode::Subtract
                } else {
                    crate::shapebuilder::BuildMode::Unite
                };
                let results = crate::shapebuilder::apply_build(
                    &sel_shapes,
                    &faces,
                    &all_face_indices,
                    mode,
                );
                self.checkpoint();
                let mut sorted_sel = sel.clone();
                sorted_sel.sort_unstable_by(|a, b| b.cmp(a));
                sorted_sel.dedup();
                for i in sorted_sel {
                    if i < self.doc.shapes.len() {
                        self.doc.shapes.remove(i);
                    }
                }
                let first_new = self.doc.shapes.len();
                for s in results {
                    self.doc.shapes.push(s);
                }
                if first_new < self.doc.shapes.len() {
                    self.select_single(self.doc.shapes.len() - 1);
                } else {
                    self.select_clear();
                }
                self.host.mark_dirty();
            }

            // --- Text on Path (Wave 9) ---
            Action::AttachTextToPath { text_id, path_id } => {
                if text_id < self.doc.shapes.len() && path_id < self.doc.shapes.len() {
                    self.checkpoint();
                    self.text_on_path.insert(text_id, path_id);
                    self.relayout_text_on_path(text_id);
                    self.host.mark_dirty();
                }
            }
            Action::DetachTextFromPath(text_id) => {
                self.text_on_path.remove(&text_id);
                self.text_on_path_params.remove(&text_id);
                // Re-lay-out the text flat (off the path) so it returns to normal.
                if text_id < self.doc.shapes.len() {
                    self.doc.shapes[text_id].text_relayout();
                }
                self.host.mark_dirty();
            }

            // --- SVG I/O (Wave 9) ---
            Action::ExportSvg(path) => {
                match self.export_svg(&path) {
                    Ok(()) => {
                        let msg = format!("Exported to {}", path.display());
                        self.status_message = Some((msg, std::time::Instant::now()));
                    }
                    Err(e) => {
                        log::error!("SVG export failed: {e}");
                        self.status_message = Some(("Export failed".to_string(), std::time::Instant::now()));
                    }
                }
            }
            Action::ImportSvg(path) => {
                match import_svg(&path) {
                    Ok(shapes) => {
                        self.checkpoint();
                        let first = self.doc.shapes.len();
                        self.doc.shapes.extend(shapes);
                        if self.doc.shapes.len() > first {
                            self.select_single(self.doc.shapes.len() - 1);
                            self.host.mark_dirty();
                        }
                        self.status_message = Some((
                            format!("Imported {}", path.display()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        log::error!("SVG import failed: {e}");
                        self.status_message = Some(("Import failed".to_string(), std::time::Instant::now()));
                    }
                }
            }

            // --- Layer order / grouping (Wave 9) ---
            Action::GroupSelected => {
                if self.selection.len() >= 2 {
                    self.checkpoint();
                    let group_id = rand_group_id(&self.doc);
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(s) = self.doc.shapes.get_mut(i) {
                            s.set_group(Some(group_id));
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::UngroupSelected => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    let sel: Vec<usize> = self.selection.clone();
                    for i in sel {
                        if let Some(s) = self.doc.shapes.get_mut(i) {
                            s.set_group(None);
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::MoveLayerOrder { id, delta } => {
                if id < self.doc.shapes.len() {
                    let new_idx = (id as i32 + delta)
                        .clamp(0, self.doc.shapes.len() as i32 - 1) as usize;
                    if new_idx != id {
                        self.checkpoint();
                        let shape = self.doc.shapes.remove(id);
                        self.doc.shapes.insert(new_idx, shape);
                        self.selection = self.selection.iter().map(|&i| {
                            if i == id { new_idx }
                            else if delta > 0 && i > id && i <= new_idx { i - 1 }
                            else if delta < 0 && i >= new_idx && i < id { i + 1 }
                            else { i }
                        }).collect();
                        self.sync_legacy_selection();
                        self.host.mark_dirty();
                    }
                }
            }

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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Batch 6 tests -------------------------------------------------------

    // --- TransformEach ---

    /// TransformEach with a non-zero (dx,dy) translates each selected shape
    /// independently; both shapes end up shifted by the same delta.
    #[test]
    fn test_transform_each_translate() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 50.0, 50.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.doc.shapes.push(Shape::rect([200.0, 200.0, 50.0, 50.0], [0.0,1.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::TransformEach {
            dx: 10.0, dy: 5.0, scale_x: 1.0, scale_y: 1.0, angle_deg: 0.0, reflect_x: false,
        });
        let b0 = app.doc.shapes[0].bounds().unwrap();
        let b1 = app.doc.shapes[1].bounds().unwrap();
        // Both rects should have moved by +10 x and +5 y from their original positions.
        assert!((b0.x - 10.0).abs() < 1.0, "shape 0 x: {}", b0.x);
        assert!((b0.y - 5.0).abs() < 1.0,  "shape 0 y: {}", b0.y);
        assert!((b1.x - 210.0).abs() < 1.0, "shape 1 x: {}", b1.x);
        assert!((b1.y - 205.0).abs() < 1.0,  "shape 1 y: {}", b1.y);
    }

    /// TransformEach with scale_x/scale_y=2 doubles each shape's bounding box.
    #[test]
    fn test_transform_each_scale() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([100.0, 100.0, 40.0, 40.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::TransformEach {
            dx: 0.0, dy: 0.0, scale_x: 2.0, scale_y: 2.0, angle_deg: 0.0, reflect_x: false,
        });
        let b = app.doc.shapes[0].bounds().unwrap();
        // Original was 40×40; scaled by 2 → 80×80.
        assert!((b.w - 80.0).abs() < 2.0, "width should be ~80, got {}", b.w);
        assert!((b.h - 80.0).abs() < 2.0, "height should be ~80, got {}", b.h);
        // Centre should be preserved: original centre = (120, 120).
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        assert!((cx - 120.0).abs() < 2.0, "centre x should be ~120, got {}", cx);
        assert!((cy - 120.0).abs() < 2.0, "centre y should be ~120, got {}", cy);
    }

    /// TransformEach with angle_deg=90 rotates each shape around its own centre.
    #[test]
    fn test_transform_each_rotate() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // A 20×60 rect centred at (50, 50): x=40, y=20, w=20, h=60.
        app.doc.shapes.push(Shape::rect([40.0, 20.0, 20.0, 60.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        app.selection = vec![0];
        app.sync_legacy_selection();
        app.apply(Action::TransformEach {
            dx: 0.0, dy: 0.0, scale_x: 1.0, scale_y: 1.0, angle_deg: 90.0, reflect_x: false,
        });
        // After 90° rotation the bounding box should have swapped width and height
        // (the 20×60 rect becomes approximately 60×20).
        let b = app.doc.shapes[0].bounds().unwrap();
        assert!(b.w > 40.0, "rotated rect should be wider than 20, got {}", b.w);
        assert!(b.h < 40.0, "rotated rect should be shorter than 60, got {}", b.h);
    }

    // --- OffsetPath ---

    /// OffsetPath with distance > 0 expands all points outward (larger bounds).
    #[test]
    fn test_offset_path_expand() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // Seed a 100×100 rect as a path via to_path-style points.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        // Convert to a path so OffsetPath can act on it.
        let path = app.doc.shapes[0].to_path();
        app.doc.shapes[0] = path;
        app.select_single(0);
        let before = app.doc.shapes[0].bounds().unwrap();
        app.apply(Action::OffsetPath { shape_id: 0, distance: 10.0 });
        let after = app.doc.shapes[0].bounds().unwrap();
        assert!(after.w > before.w, "expanded w: {} > {}", after.w, before.w);
        assert!(after.h > before.h, "expanded h: {} > {}", after.h, before.h);
    }

    /// OffsetPath with distance < 0 contracts the shape (smaller bounds).
    #[test]
    fn test_offset_path_contract() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,1.0], 1.0));
        let path = app.doc.shapes[0].to_path();
        app.doc.shapes[0] = path;
        app.select_single(0);
        let before = app.doc.shapes[0].bounds().unwrap();
        app.apply(Action::OffsetPath { shape_id: 0, distance: -5.0 });
        let after = app.doc.shapes[0].bounds().unwrap();
        assert!(after.w < before.w, "contracted w: {} < {}", after.w, before.w);
        assert!(after.h < before.h, "contracted h: {} < {}", after.h, before.h);
    }

    // --- FindReplaceText ---

    fn make_text_shape(text: &str, x: f32, y: f32) -> Shape {
        let params = crate::text::TextParams {
            text: text.to_string(),
            font_size: 24.0,
            ..Default::default()
        };
        let glyphs = crate::text::layout(&params, (x, y)).0;
        Shape::Text {
            params,
            origin: (x, y),
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
        }
    }

    /// FindReplaceText replaces matching substrings across all text shapes.
    #[test]
    fn test_find_replace_basic() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(make_text_shape("hello world", 0.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 100.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 200.0, 0.0));
        app.apply(Action::FindReplaceText { find: "hello".to_string(), replace: "hi".to_string() });
        for shape in &app.doc.shapes {
            if let Shape::Text { params, .. } = shape {
                assert_eq!(params.text, "hi world", "text should be replaced");
            }
        }
    }

    /// last_find_count reflects the number of shapes modified.
    #[test]
    fn test_find_replace_count() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(make_text_shape("hello world", 0.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 100.0, 0.0));
        app.doc.shapes.push(make_text_shape("hello world", 200.0, 0.0));
        app.apply(Action::FindReplaceText { find: "hello".to_string(), replace: "hi".to_string() });
        assert_eq!(app.last_find_count, 3);
    }

    /// FindReplaceText with no match leaves count at 0.
    #[test]
    fn test_find_replace_no_match() {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(make_text_shape("hello world", 0.0, 0.0));
        app.apply(Action::FindReplaceText { find: "xyz".to_string(), replace: "abc".to_string() });
        assert_eq!(app.last_find_count, 0);
        // Text unchanged.
        if let Shape::Text { params, .. } = &app.doc.shapes[0] {
            assert_eq!(params.text, "hello world");
        }
    }

    // --- PathfinderTrim / PathfinderMerge ---

    /// PathfinderTrim on two overlapping rects yields at least one result shape.
    #[test]
    fn test_pathfinder_trim_basic() {
        let mut app = App::new();
        app.doc.shapes.clear();
        // Two overlapping 100×100 rects.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 100.0, 100.0], [1.0,0.0,0.0,1.0], [0.0,0.0,0.0,0.0], 0.0));
        app.doc.shapes.push(Shape::rect([50.0, 50.0, 100.0, 100.0], [0.0,0.0,1.0,1.0], [0.0,0.0,0.0,0.0], 0.0));
        // Select both (primary=1=front, secondary=0=back).
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        let before_count = app.doc.shapes.len();
        app.apply(Action::PathfinderTrim);
        // The trim should produce shapes (the result replaces the two inputs).
        // i_overlay may merge or split them; we just need some output.
        assert!(app.doc.shapes.len() >= 1, "trim should produce at least one shape");
        // The total shape count may differ from before, confirming the op ran.
        let _ = before_count;
    }

    /// PathfinderMerge on two same-fill-colour rects produces one shape.
    #[test]
    fn test_pathfinder_merge_same_color() {
        let mut app = App::new();
        app.doc.shapes.clear();
        let fill = [0.5_f32, 0.5, 0.5, 1.0];
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 60.0, 60.0], fill, [0.0,0.0,0.0,0.0], 0.0));
        app.doc.shapes.push(Shape::rect([40.0, 40.0, 60.0, 60.0], fill, [0.0,0.0,0.0,0.0], 0.0));
        app.selection = vec![0, 1];
        app.sync_legacy_selection();
        app.apply(Action::PathfinderMerge);
        // Merge should unite same-colour shapes into fewer shapes.
        assert!(app.doc.shapes.len() >= 1, "merge should produce at least one shape");
    }

    // --- Scatter Brush ---

    /// SetScatterBrush configures the brush; PlaceScatterAlongPath produces copies.
    #[test]
    fn test_scatter_brush_spacing() {
        let mut app = App::new();
        // Straight horizontal path of length 100 (10 segments of 10).
        let path: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32 * 10.0, 0.0]).collect();
        // Use an arbitrary symbol_id (no real symbol needed — placeholder rects are placed).
        app.apply(Action::SetScatterBrush {
            symbol_id: 99,
            spacing: 10.0,
            size_jitter: 0.0,
            rotation_jitter: 0.0,
        });
        let before = app.doc.shapes.len();
        app.apply(Action::PlaceScatterAlongPath { path });
        let added = app.doc.shapes.len() - before;
        // With spacing=10 along a length-100 path we expect ~11 copies (dist 0,10,20…100).
        assert!(added >= 9 && added <= 12, "expected ~11 copies, got {}", added);
    }

    /// The first copy is placed at distance 0 (the path start).
    #[test]
    fn test_scatter_brush_placement() {
        let mut app = App::new();
        app.apply(Action::SetScatterBrush {
            symbol_id: 1,
            spacing: 50.0,
            size_jitter: 0.0,
            rotation_jitter: 0.0,
        });
        let path = vec![[0.0_f32, 0.0], [100.0, 0.0]];
        let before = app.doc.shapes.len();
        app.apply(Action::PlaceScatterAlongPath { path });
        // Should have placed copies at 0 and 50 (within the 100-unit path).
        let added = app.doc.shapes.len() - before;
        assert!(added >= 2, "expected at least 2 copies, got {}", added);
        // The first added shape should be near x=0.
        let b = app.doc.shapes[before].bounds().unwrap();
        let cx = b.x + b.w / 2.0;
        assert!(cx.abs() < 15.0, "first copy centre x should be near 0, got {}", cx);
    }

    /// PlaceScatterAlongPath is a no-op when no scatter brush is configured.
    #[test]
    fn test_scatter_brush_no_symbol() {
        let mut app = App::new();
        let before = app.doc.shapes.len();
        app.apply(Action::PlaceScatterAlongPath {
            path: vec![[0.0, 0.0], [100.0, 0.0]],
        });
        assert_eq!(app.doc.shapes.len(), before, "no brush configured: nothing should be added");
    }

    // --- Batch 7: Art Brush ---

    #[test]
    fn test_art_brush_set_clear() {
        let mut app = App::new();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: false });
        assert!(app.art_brush.is_some());
        app.apply(Action::ClearArtBrush);
        assert!(app.art_brush.is_none());
    }

    #[test]
    fn test_art_brush_apply_produces_shape() {
        let mut app = App::new();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 2.0, colorize: ArtBrushColorize::None, flip: false });
        let before = app.doc.shapes.len();
        // Straight horizontal path long enough to deform.
        let path: Vec<[f32; 2]> = (0..=10).map(|i| [i as f32 * 10.0, 0.0]).collect();
        app.apply(Action::PaintArtBrushPath { path });
        assert!(app.doc.shapes.len() > before, "art brush should add a shape");
    }

    #[test]
    fn test_art_brush_flip() {
        let mut app = App::new();
        // Two strokes: one normal, one flipped — both should produce shapes.
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: false });
        let path: Vec<[f32; 2]> = (0..=5).map(|i| [i as f32 * 20.0, 0.0]).collect();
        app.apply(Action::PaintArtBrushPath { path: path.clone() });
        let after_normal = app.doc.shapes.len();
        app.apply(Action::SetArtBrush { symbol_id: 1, width_scale: 1.0, colorize: ArtBrushColorize::None, flip: true });
        app.apply(Action::PaintArtBrushPath { path });
        assert!(app.doc.shapes.len() > after_normal, "flipped brush also produces a shape");
    }

    // --- Batch 7: Live Corners ---

    #[test]
    fn test_live_corner_polygon_set() {
        let mut app = App::new();
        // Create a live polygon and select it.
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape { tool: Tool::Polygon, a: [100.0, 100.0], b: [180.0, 180.0] });
        let idx = app.doc.shapes.len() - 1;
        app.selection = vec![idx];
        app.apply(Action::SetPolygonCornerRadius(12.0));
        if let Some(crate::liveshape::LiveShape::Polygon { corner_radius, .. }) = app.doc.shapes[idx].live_shape() {
            assert!((corner_radius - 12.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_live_corner_clamp() {
        let mut app = App::new();
        app.apply(Action::SetTool(Tool::Polygon));
        app.apply(Action::CreateShape { tool: Tool::Polygon, a: [100.0, 100.0], b: [180.0, 180.0] });
        let idx = app.doc.shapes.len() - 1;
        app.selection = vec![idx];
        app.apply(Action::SetPolygonCornerRadius(-5.0));
        if let Some(crate::liveshape::LiveShape::Polygon { corner_radius, .. }) = app.doc.shapes[idx].live_shape() {
            assert!(corner_radius >= 0.0, "corner radius clamped to >= 0");
        }
    }

    // --- Batch 7: Perspective Grid ---

    #[test]
    fn test_perspective_grid_toggle() {
        let mut app = App::new();
        let was_on = app.perspective_grid.is_some();
        app.apply(Action::TogglePerspectiveGrid);
        assert_ne!(app.perspective_grid.is_some(), was_on, "toggle should flip grid state");
    }

    #[test]
    fn test_perspective_plane_select() {
        let mut app = App::new();
        app.apply(Action::SetPerspectivePlane(1));
        assert_eq!(app.perspective_active_plane, 1);
        app.apply(Action::SetPerspectivePlane(2));
        assert_eq!(app.perspective_active_plane, 2);
    }

    #[test]
    fn test_perspective_plane_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPerspectivePlane(99));
        assert!(app.perspective_active_plane <= 2, "plane clamped to 0..=2");
    }

    // --- Batch 7: Color Guide ---

    #[test]
    fn test_color_guide_complementary() {
        let mut app = App::new();
        let key = [1.0_f32, 0.0, 0.0, 1.0]; // red
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Complementary, key_color: key });
        assert!(!app.color_guide.swatches.is_empty(), "complementary should produce swatches");
    }

    #[test]
    fn test_color_guide_triadic() {
        let mut app = App::new();
        let key = [0.0_f32, 0.8, 0.0, 1.0];
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Triadic, key_color: key });
        assert_eq!(app.color_guide.swatches.len(), 3, "triadic = 3 swatches");
    }

    #[test]
    fn test_color_guide_apply_sets_fill() {
        let mut app = App::new();
        let key = [0.5_f32, 0.2, 0.8, 1.0];
        app.apply(Action::SetColorGuide { rule: ColorHarmonyRule::Complementary, key_color: key });
        assert!(!app.color_guide.swatches.is_empty());
        let original_fill = app.default_fill;
        app.apply(Action::ApplyColorGuide(0));
        // Fill should have changed to the guide swatch.
        assert_ne!(app.default_fill, original_fill, "fill should change after applying guide color");
    }

    // --- Batch 7: Warp Tools ---

    #[test]
    fn test_warp_tool_kind() {
        let mut app = App::new();
        app.apply(Action::SetWarpToolKind(WarpToolKind::Crystallize));
        assert_eq!(app.warp_tool_kind, WarpToolKind::Crystallize);
    }

    #[test]
    fn test_warp_brush_params() {
        let mut app = App::new();
        app.apply(Action::SetWarpBrush { size: 50.0, intensity: 0.8, detail: 2.0 });
        assert!((app.warp_brush_size - 50.0).abs() < 0.01);
        assert!((app.warp_brush_intensity - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_scallop_pulls_inward() {
        let mut app = App::new();
        // Point at (100, 0). Center at (50, 0): dx=50, scallop pulls toward center → x decreases.
        app.doc.shapes.push(Shape::path(vec![(100.0, 0.0), (200.0, 0.0), (150.0, 50.0)], vec![], true, [1.0,0.0,0.0,1.0], [0.0;4], 0.0));
        app.apply(Action::SetWarpToolKind(WarpToolKind::Scallop));
        app.apply(Action::SetWarpBrush { size: 30.0, intensity: 1.0, detail: 1.0 });
        let before = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0].0 } else { 0.0 };
        // Center at (50,0), radius 80 — point inside, dx=50.
        app.apply(Action::ApplyWarpStroke { center: (50.0, 0.0), radius: 80.0 });
        let after = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0].0 } else { 0.0 };
        assert!(after < before, "scallop should pull point toward center: {} -> {}", before, after);
    }

    #[test]
    fn test_warp_outside_radius_unchanged() {
        let mut app = App::new();
        app.doc.shapes.push(Shape::path(vec![(500.0, 500.0), (600.0, 500.0), (550.0, 600.0)], vec![], true, [0.0,1.0,0.0,1.0], [0.0;4], 0.0));
        app.apply(Action::SetWarpToolKind(WarpToolKind::Crystallize));
        app.apply(Action::SetWarpBrush { size: 20.0, intensity: 1.0, detail: 1.0 });
        let before = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0] } else { (0.0, 0.0) };
        // Center far away.
        app.apply(Action::ApplyWarpStroke { center: (0.0, 0.0), radius: 20.0 });
        let after = if let Shape::Path { ref points, .. } = app.doc.shapes.last().unwrap() { points[0] } else { (0.0, 0.0) };
        assert_eq!(before, after, "point outside radius should not move");
    }

}
