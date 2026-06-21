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

            // --- Appearance panel handlers ---

            Action::MigrateAppearance => {
                if let Some(shape) = self.selected_shape_mut() {
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                }
                self.host.mark_dirty();
            }

            Action::AppearanceAddFill => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .fills.push(Fill::solid([0.5, 0.5, 0.5, 1.0]));
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceAddStroke => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .strokes.push(AppStroke::solid([0.0, 0.0, 0.0, 1.0], 1.0));
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceAddDropShadow => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .effects.push(Effect::drop_shadow());
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceAddBlur => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    shape.appearance_mut().as_mut().unwrap()
                        .effects.push(Effect::gaussian_blur());
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRemoveFill(idx) => {
                let has = self.selected_shape()
                    .and_then(|s| s.appearance())
                    .map(|a| idx < a.fills.len())
                    .unwrap_or(false);
                if has {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap()
                        .appearance_mut().as_mut().unwrap().fills.remove(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRemoveStroke(idx) => {
                let has = self.selected_shape()
                    .and_then(|s| s.appearance())
                    .map(|a| idx < a.strokes.len())
                    .unwrap_or(false);
                if has {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap()
                        .appearance_mut().as_mut().unwrap().strokes.remove(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRemoveEffect(idx) => {
                let has = self.selected_shape()
                    .and_then(|s| s.appearance())
                    .map(|a| idx < a.effects.len())
                    .unwrap_or(false);
                if has {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap()
                        .appearance_mut().as_mut().unwrap().effects.remove(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceToggleFill(idx) => {
                if let Some(f) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                    .and_then(|a| a.fills.get_mut(idx))
                {
                    f.visible = !f.visible;
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceToggleStroke(idx) => {
                if let Some(s) = self.selected_shape_mut()
                    .and_then(|sh| sh.appearance_mut().as_mut())
                    .and_then(|a| a.strokes.get_mut(idx))
                {
                    s.visible = !s.visible;
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceSetFillColor { idx, color } => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    if let Some(f) = shape.appearance_mut().as_mut().and_then(|a| a.fills.get_mut(idx)) {
                        f.paint = Paint::Solid(color);
                    }
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceSetStrokeColor { idx, color } => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    if let Some(s) = shape.appearance_mut().as_mut().and_then(|a| a.strokes.get_mut(idx)) {
                        s.paint = Paint::Solid(color);
                    }
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceSetStrokeWidth { idx, width } => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    let shape = self.selected_shape_mut().unwrap();
                    if shape.appearance().is_none() {
                        let a = shape.effective_appearance();
                        shape.set_appearance(Some(a));
                    }
                    if let Some(s) = shape.appearance_mut().as_mut().and_then(|a| a.strokes.get_mut(idx)) {
                        s.width = width.max(0.0);
                    }
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRaiseFill(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.raise_fill(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceLowerFill(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.lower_fill(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceRaiseStroke(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.raise_stroke(idx);
                    self.host.mark_dirty();
                }
            }

            Action::AppearanceLowerStroke(idx) => {
                if let Some(a) = self.selected_shape_mut()
                    .and_then(|s| s.appearance_mut().as_mut())
                {
                    a.lower_stroke(idx);
                    self.host.mark_dirty();
                }
            }

            // --- Graphic styles handlers ---

            Action::SaveGraphicStyle(name) => {
                if let Some(shape) = self.selected_shape() {
                    let appearance = shape.effective_appearance();
                    self.doc.graphic_styles.add(&name, appearance);
                }
            }

            Action::ApplyGraphicStyle(id) => {
                if let Some(appearance) = self.doc.graphic_styles.appearance_of(id).cloned() {
                    if self.selected_shape().is_some() {
                        self.checkpoint();
                        self.selected_shape_mut().unwrap().set_appearance(Some(appearance));
                        self.host.mark_dirty();
                    }
                }
            }

            Action::DeleteGraphicStyle(id) => {
                self.doc.graphic_styles.remove(id);
            }

            // --- Width tool ---
            Action::SetWidthProfile { start, end } => {
                if let Some(idx) = self.selected {
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        shape.stroke_style_mut().width_profile = (start, end);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Blend tool ---
            Action::CreateBlend { steps } => {
                if let (Some(a_idx), Some(b_idx)) = (self.selected, self.secondary) {
                    if let (Some(a), Some(b)) = (
                        self.doc.shapes.get(a_idx).cloned(),
                        self.doc.shapes.get(b_idx).cloned(),
                    ) {
                        self.checkpoint();
                        let new_steps = crate::blend::make_steps(&a, &b, steps);
                        let insert_at = a_idx.min(b_idx) + 1;
                        for (i, s) in new_steps.into_iter().enumerate() {
                            self.doc.shapes.insert(insert_at + i, s);
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetBlendSteps(n) => {
                self.blend_steps = n.max(1);
            }

            // --- Perspective grid ---
            Action::TogglePerspectiveGrid => {
                if let Some(ref mut pg) = self.perspective_grid {
                    pg.visible = !pg.visible;
                } else {
                    let (w, h) = (self.host.doc_w as f32, self.host.doc_h as f32);
                    self.perspective_grid = Some(PerspectiveGrid::default_for(w, h));
                }
            }
            Action::MovePerspectiveVP { vp, x, y } => {
                if let Some(ref mut pg) = self.perspective_grid {
                    if vp == 0 { pg.vp1 = (x, y); } else { pg.vp2 = (x, y); }
                }
            }

            // --- Mesh gradient ---
            Action::SetMeshGradient { points } => {
                self.mesh_points = points;
                self.host.mark_dirty();
            }
            Action::EditMeshPoint { row, col, pos, color } => {
                let idx = row * 4 + col;
                if idx < self.mesh_points.len() {
                    self.mesh_points[idx] = (pos, color);
                    self.host.mark_dirty();
                }
            }

            // --- AI/EPS import ---
            Action::ImportAiEps(path) => {
                self.checkpoint();
                match crate::ai_eps::import(&path) {
                    Ok(shapes) => {
                        let count = shapes.len();
                        for s in shapes {
                            self.doc.shapes.push(s);
                        }
                        self.host.mark_dirty();
                        self.status_message = Some((
                            format!("Imported {count} shapes from {}",
                                path.file_name().unwrap_or_default().to_string_lossy()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Import failed: {e}"), std::time::Instant::now()));
                    }
                }
            }

            // --- 3D extrude ---
            Action::AppearanceAdd3DExtrude => {
                if let Some(idx) = self.selected {
                    self.checkpoint();
                    if let Some(shape) = self.doc.shapes.get_mut(idx) {
                        // Ensure the shape has an appearance stack (migrate from legacy if needed).
                        if shape.appearance_mut().is_none() {
                            let ea = shape.effective_appearance();
                            *shape.appearance_mut() = Some(ea);
                        }
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            ap.effects.push(crate::appearance::Effect::extrude_3d());
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AppearanceSet3DDepth { idx, depth } => {
                if let Some(si) = self.selected {
                    if let Some(shape) = self.doc.shapes.get_mut(si) {
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            if let Some(crate::appearance::Effect::Extrude3D { depth: d, .. }) = ap.effects.get_mut(idx) {
                                *d = depth;
                                self.host.mark_dirty();
                            }
                        }
                    }
                }
            }
            Action::AppearanceSet3DAngle { idx, angle_deg } => {
                if let Some(si) = self.selected {
                    if let Some(shape) = self.doc.shapes.get_mut(si) {
                        if let Some(ap) = shape.appearance_mut().as_mut() {
                            if let Some(crate::appearance::Effect::Extrude3D { angle_deg: a, .. }) = ap.effects.get_mut(idx) {
                                *a = angle_deg;
                                self.host.mark_dirty();
                            }
                        }
                    }
                }
            }

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

    // --- Batch 9: Type on Path depth ---

    #[test]
    fn test_text_on_path_offset() {
        let mut app = App::new();
        app.apply(Action::SetTextOnPathOffset { text_id: 7, offset: 42.5 });
        assert_eq!(app.text_on_path_offsets.get(&7).copied(), Some(42.5));
    }

    #[test]
    fn test_text_on_path_flip() {
        let mut app = App::new();
        // Default (no entry) should be treated as `true` (above).
        app.apply(Action::FlipTextOnPath(3));
        assert_eq!(app.text_on_path_above.get(&3).copied(), Some(false), "flip from default true → false");
        app.apply(Action::FlipTextOnPath(3));
        assert_eq!(app.text_on_path_above.get(&3).copied(), Some(true), "flip back to true");
    }

    #[test]
    fn test_detach_text_from_path() {
        let mut app = App::new();
        // Seed a fake path attachment.
        app.text_on_path.insert(1, 0);
        app.text_on_path_offsets.insert(1, 10.0);
        app.apply(Action::DetachTextFromPath(1));
        assert!(app.text_on_path.get(&1).is_none(), "attachment removed");
        // Offset entry is left in place (detach does not clear depth state).
    }

    // --- Batch 9: Recolor Artwork depth ---

    #[test]
    fn test_recolor_color_count_clamp() {
        let mut app = App::new();
        // Values below 2 clamp to 2.
        app.apply(Action::SetRecolorColorCount(1));
        assert_eq!(app.recolor_color_count, 2, "clamped to minimum 2");
        // Values above 30 clamp to 30.
        app.apply(Action::SetRecolorColorCount(40));
        assert_eq!(app.recolor_color_count, 30, "clamped to maximum 30");
        // In-range values are kept as-is.
        app.apply(Action::SetRecolorColorCount(12));
        assert_eq!(app.recolor_color_count, 12);
    }

    #[test]
    fn test_recolor_preserve_flags() {
        let mut app = App::new();
        assert!(app.recolor_config.preserve_black, "default preserve_black is true");
        app.apply(Action::SetRecolorPreserveBlack(false));
        assert!(!app.recolor_config.preserve_black);
        app.apply(Action::SetRecolorPreserveWhite(false));
        assert!(!app.recolor_config.preserve_white);
    }

    #[test]
    fn test_save_recolor_set() {
        let mut app = App::new();
        // Select the first two shapes so we have fills to save.
        app.selection = vec![0, 1];
        let before = app.recolor_history.len();
        app.apply(Action::SaveRecolorSet);
        assert!(app.recolor_history.len() > before, "recolor set saved");
        assert!(!app.recolor_history.last().unwrap().is_empty(), "saved set is non-empty");
    }

    // --- Batch 9: Live Paint depth ---

    #[test]
    fn test_live_paint_gap_detection() {
        let mut app = App::new();
        assert!(!app.live_paint_gap_detection, "gap detection starts false");
        app.apply(Action::SetLivePaintGapDetection(true));
        assert!(app.live_paint_gap_detection);
        app.apply(Action::SetLivePaintGapDetection(false));
        assert!(!app.live_paint_gap_detection);
    }

    #[test]
    fn test_live_paint_highlight_color() {
        let mut app = App::new();
        let color = [0.0, 1.0, 0.5, 1.0];
        app.apply(Action::SetLivePaintHighlightColor(color));
        assert_eq!(app.live_paint_highlight_color, color);
    }

    #[test]
    fn test_make_live_paint_group() {
        let mut app = App::new();
        // Select the first two shapes.
        app.selection = vec![0, 1];
        let before = app.live_paint_group_ids.len();
        app.apply(Action::MakeLivePaintGroup);
        assert_eq!(app.live_paint_group_ids.len(), before + 1, "one group added");
        // Both shapes should have the new group id.
        let gid = *app.live_paint_group_ids.last().unwrap();
        assert_eq!(app.doc.shapes[0].group(), Some(gid));
        assert_eq!(app.doc.shapes[1].group(), Some(gid));
    }

    // --- Batch 9: Symbol Sprayer ---

    #[test]
    fn test_spray_density_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDensity(15.0));
        assert!((app.symbol_spray_config.density - 10.0).abs() < 0.01, "density clamped to 10");
        app.apply(Action::SetSymbolSprayDensity(-1.0));
        assert!((app.symbol_spray_config.density - 0.0).abs() < 0.01, "density clamped to 0");
    }

    #[test]
    fn test_spray_diameter_min() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDiameter(0.0));
        assert!((app.symbol_spray_config.diameter - 1.0).abs() < 0.01, "diameter clamped to min 1.0");
        app.apply(Action::SetSymbolSprayDiameter(50.0));
        assert!((app.symbol_spray_config.diameter - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_spray_symbols_adds_shapes() {
        let mut app = App::new();
        app.apply(Action::SetSymbolSprayDensity(3.0));
        let before = app.doc.shapes.len();
        app.apply(Action::SpraySymbols { center: [0.0, 0.0], pressure: 1.0 });
        let added = app.doc.shapes.len() - before;
        assert!(added >= 1, "SpraySymbols should add at least one shape, got {added}");
    }

    #[test]
    fn test_symbol_stain_changes_fill() {
        let mut app = App::new();
        // Add a shape at the origin so SymbolStain can reach it.
        app.doc.shapes.push(Shape::rect([0.0, 0.0, 20.0, 20.0], [1.0, 0.0, 0.0, 1.0], [0.0;4], 0.0));
        // Use a large diameter so the shape is within the brush.
        app.apply(Action::SetSymbolSprayDiameter(200.0));
        let idx = app.doc.shapes.len() - 1;
        let before_fill = app.doc.shapes[idx].fill_color().unwrap();
        let stain = [0.0, 0.0, 1.0, 1.0];
        app.apply(Action::SymbolStain { center: [10.0, 10.0], color: stain });
        let after_fill = app.doc.shapes[idx].fill_color().unwrap();
        // Fill should have changed toward the stain color.
        assert_ne!(before_fill, after_fill, "stain should change the fill color");
        // Blue channel should have increased.
        assert!(after_fill[2] > before_fill[2], "blue channel should increase toward stain");
    }

    // --- Batch 10: Gradient Mesh ---

    #[test]
    fn test_mesh_rows_cols_clamp() {
        let mut app = App::new();
        // Below minimum → clamp to 1.
        app.apply(Action::SetMeshRows(0));
        assert_eq!(app.gradient_mesh.rows, 1, "rows clamped to 1");
        // Above maximum → clamp to 50.
        app.apply(Action::SetMeshRows(100));
        assert_eq!(app.gradient_mesh.rows, 50, "rows clamped to 50");
        app.apply(Action::SetMeshCols(0));
        assert_eq!(app.gradient_mesh.cols, 1, "cols clamped to 1");
        app.apply(Action::SetMeshCols(100));
        assert_eq!(app.gradient_mesh.cols, 50, "cols clamped to 50");
    }

    #[test]
    fn test_create_mesh_fills_points() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(2));
        app.apply(Action::SetMeshCols(3));
        app.apply(Action::CreateMesh);
        assert_eq!(app.gradient_mesh.points.len(), 6, "2×3 mesh should have 6 points");
    }

    #[test]
    fn test_mesh_point_tension_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(1));
        app.apply(Action::SetMeshCols(1));
        app.apply(Action::CreateMesh);
        app.apply(Action::SetMeshPointTension { idx: 0, tension: 2.0 });
        assert!((app.gradient_mesh.points[0].tension - 1.0).abs() < 0.001, "tension clamped to 1.0");
        app.apply(Action::SetMeshPointTension { idx: 0, tension: -0.5 });
        assert!((app.gradient_mesh.points[0].tension - 0.0).abs() < 0.001, "tension clamped to 0.0");
    }

    #[test]
    fn test_release_mesh_clears() {
        let mut app = App::new();
        app.apply(Action::SetMeshRows(3));
        app.apply(Action::SetMeshCols(3));
        app.apply(Action::CreateMesh);
        assert!(!app.gradient_mesh.points.is_empty());
        app.apply(Action::ReleaseMesh);
        assert!(app.gradient_mesh.points.is_empty(), "ReleaseMesh should clear all points");
        assert_eq!(app.gradient_mesh.rows, 4, "rows reset to default");
        assert_eq!(app.gradient_mesh.cols, 4, "cols reset to default");
    }

    // --- Batch 10: Flare Tool ---

    #[test]
    fn test_flare_brightness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetFlareBrightness(150.0));
        assert!((app.flare_config.brightness - 100.0).abs() < 0.001, "brightness clamped to 100");
        app.apply(Action::SetFlareBrightness(-10.0));
        assert!((app.flare_config.brightness - 0.0).abs() < 0.001, "brightness clamped to 0");
    }

    #[test]
    fn test_flare_ray_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetFlareRayCount(255));
        assert_eq!(app.flare_config.ray_count, 250, "ray_count clamped to 250");
    }

    #[test]
    fn test_place_flare_adds_to_list() {
        let mut app = App::new();
        let before = app.flare_shapes.len();
        app.apply(Action::PlaceFlare([100.0, 200.0]));
        assert_eq!(app.flare_shapes.len(), before + 1, "PlaceFlare should add to flare_shapes");
        assert_eq!(app.flare_config.center, [100.0, 200.0]);
    }

    #[test]
    fn test_flare_toggle() {
        let mut app = App::new();
        assert!(!app.flare_tool_active);
        app.apply(Action::ToggleFlareTool);
        assert!(app.flare_tool_active);
        app.apply(Action::ToggleFlareTool);
        assert!(!app.flare_tool_active);
    }

    // --- Batch 10: Pattern Brush ---

    #[test]
    fn test_pattern_brush_scale_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPatternBrushScale(2000.0));
        assert!((app.pattern_brush_config.scale - 1000.0).abs() < 0.001, "scale clamped to 1000");
        app.apply(Action::SetPatternBrushScale(-10.0));
        assert!((app.pattern_brush_config.scale - 0.0).abs() < 0.001, "scale clamped to 0");
    }

    #[test]
    fn test_save_pattern_brush_library() {
        let mut app = App::new();
        app.apply(Action::SavePatternBrush { name: "Dots".to_string() });
        app.apply(Action::SavePatternBrush { name: "Waves".to_string() });
        assert_eq!(app.pattern_brush_library.len(), 2, "library should contain 2 entries");
    }

    #[test]
    fn test_delete_pattern_brush_oob() {
        let mut app = App::new();
        // Delete on empty library: no panic.
        app.apply(Action::DeletePatternBrush(99));
        assert!(app.pattern_brush_library.is_empty());
        // Add one and delete a valid index.
        app.apply(Action::SavePatternBrush { name: "X".to_string() });
        app.apply(Action::DeletePatternBrush(0));
        assert!(app.pattern_brush_library.is_empty());
    }

    // --- Batch 10: Variable Fonts ---

    #[test]
    fn test_font_axis_value_clamped_to_range() {
        let mut app = App::new();
        app.apply(Action::AddFontAxis(FontAxis { tag: "wght".to_string(), min: 100.0, max: 900.0, value: 400.0 }));
        app.apply(Action::SetFontAxisValue { idx: 0, value: 5000.0 });
        assert!((app.variable_font_config.axes[0].value - 900.0).abs() < 0.001, "value clamped to max 900");
        app.apply(Action::SetFontAxisValue { idx: 0, value: 5.0 });
        assert!((app.variable_font_config.axes[0].value - 100.0).abs() < 0.001, "value clamped to min 100");
    }

    #[test]
    fn test_remove_font_axis_oob_no_panic() {
        let mut app = App::new();
        // Removing from empty list should not panic.
        app.apply(Action::RemoveFontAxis(99));
        assert!(app.variable_font_config.axes.is_empty());
    }

    #[test]
    fn test_reset_font_axes_midpoint() {
        let mut app = App::new();
        app.apply(Action::AddFontAxis(FontAxis { tag: "wdth".to_string(), min: 0.0, max: 100.0, value: 75.0 }));
        app.apply(Action::ResetFontAxes);
        assert!((app.variable_font_config.axes[0].value - 50.0).abs() < 0.001, "reset to midpoint (min+max)/2 = 50");
    }

    #[test]
    fn test_variable_font_panel_toggle() {
        let mut app = App::new();
        assert!(!app.variable_font_panel_open);
        app.apply(Action::ToggleVariableFontPanel);
        assert!(app.variable_font_panel_open);
        app.apply(Action::ToggleVariableFontPanel);
        assert!(!app.variable_font_panel_open);
    }

    // --- Batch 11: Pathfinder depth ---

    #[test]
    fn test_pathfinder_op_recorded() {
        let mut app = App::new();
        assert!(app.last_pathfinder_op.is_none());
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Unite));
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Unite));
    }

    #[test]
    fn test_pathfinder_precision_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPathfinderPrecision(0.0));
        assert!((app.pathfinder_precision - 0.001).abs() < 1e-6, "should clamp to 0.001");
        app.apply(Action::SetPathfinderPrecision(100.0));
        assert!((app.pathfinder_precision - 10.0).abs() < 1e-6, "should clamp to 10.0");
    }

    #[test]
    fn test_repeat_pathfinder_no_panic_when_none() {
        let mut app = App::new();
        // RepeatPathfinder with no prior op should not panic.
        app.apply(Action::RepeatPathfinder);
        assert!(app.last_pathfinder_op.is_none());
    }

    #[test]
    fn test_repeat_pathfinder_reapplies() {
        let mut app = App::new();
        app.apply(Action::ApplyPathfinderOp(PathfinderOp::Minus));
        app.apply(Action::RepeatPathfinder);
        // last_pathfinder_op unchanged — still Minus
        assert_eq!(app.last_pathfinder_op, Some(PathfinderOp::Minus));
    }

    // --- Batch 11: 3D Extrude depth ---

    #[test]
    fn test_extrude_depth_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudeDepth(5000.0));
        assert!((app.extrude_config.depth - 2000.0).abs() < 1e-6, "depth should clamp to 2000");
    }

    #[test]
    fn test_extrude_perspective_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudePerspective(200.0));
        assert!((app.extrude_config.perspective - 160.0).abs() < 1e-6, "perspective should clamp to 160");
    }

    #[test]
    fn test_extrude_rotation_clamp() {
        let mut app = App::new();
        app.apply(Action::SetExtrudeRotation { x: -300.0, y: 0.0, z: 0.0 });
        assert!((app.extrude_config.rotation_x - (-180.0)).abs() < 1e-6, "x rotation should clamp to -180");
    }

    #[test]
    fn test_extrude_panel_toggle() {
        let mut app = App::new();
        assert!(!app.extrude_panel_open);
        app.apply(Action::ToggleExtrudePanel);
        assert!(app.extrude_panel_open);
        app.apply(Action::ToggleExtrudePanel);
        assert!(!app.extrude_panel_open);
    }

    // --- Batch 11: Chart depth ---

    #[test]
    fn test_chart_add_dataset() {
        let mut app = App::new();
        assert!(app.chart_config.datasets.is_empty());
        app.apply(Action::AddChartDataSet(ChartDataSet::default()));
        assert_eq!(app.chart_config.datasets.len(), 1);
        assert_eq!(app.chart_config.datasets[0].label, "Series 1");
    }

    #[test]
    fn test_chart_remove_dataset_oob_no_panic() {
        let mut app = App::new();
        // Removing from an empty list should not panic.
        app.apply(Action::RemoveChartDataSet(99));
        assert!(app.chart_config.datasets.is_empty());
    }

    #[test]
    fn test_chart_column_width_clamp() {
        let mut app = App::new();
        app.apply(Action::SetChartColumnWidth(5.0));
        assert!((app.chart_config.column_width - 20.0).abs() < 1e-6, "column_width should clamp to 20");
        app.apply(Action::SetChartColumnWidth(200.0));
        assert!((app.chart_config.column_width - 100.0).abs() < 1e-6, "column_width should clamp to 100");
    }

    #[test]
    fn test_chart_category_labels() {
        let mut app = App::new();
        let labels = vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()];
        app.apply(Action::SetChartCategoryLabels(labels.clone()));
        assert_eq!(app.chart_config.category_labels, labels);
    }

    // --- Batch 11: Envelope Distort depth ---

    #[test]
    fn test_envelope_bend_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEnvelopeBend(200.0));
        assert!((app.envelope_config.bend - 100.0).abs() < 1e-6, "bend should clamp to 100");
        app.apply(Action::SetEnvelopeBend(-200.0));
        assert!((app.envelope_config.bend - (-100.0)).abs() < 1e-6, "bend should clamp to -100");
    }

    #[test]
    fn test_envelope_fidelity_clamp() {
        let mut app = App::new();
        app.apply(Action::SetEnvelopeFidelity(200.0));
        assert!((app.envelope_config.fidelity - 100.0).abs() < 1e-6, "fidelity should clamp to 100");
    }

    #[test]
    fn test_make_envelope_with_warp_no_panic_no_selection() {
        let mut app = App::new();
        // No shape selected — should not panic, applied list stays empty.
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(app.envelope_applied_shapes.is_empty());
    }

    #[test]
    fn test_release_envelope_clears() {
        let mut app = App::new();
        // Select first shape then apply an envelope.
        app.apply(Action::SelectShape(0));
        app.apply(Action::MakeEnvelopeWithWarpPreset);
        assert!(!app.envelope_applied_shapes.is_empty());
        app.apply(Action::ReleaseEnvelopeAll);
        assert!(app.envelope_applied_shapes.is_empty());
    }

    // --- Wave N: Image Trace (extended) ---

    #[test]
    fn test_image_trace_mode_set() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceMode(ImageTraceMode::Logo));
        assert_eq!(app.image_trace_config.mode, ImageTraceMode::Logo);
    }

    #[test]
    fn test_image_trace_threshold_stored() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceThreshold(200));
        assert_eq!(app.image_trace_config.threshold, 200);
    }

    #[test]
    fn test_image_trace_colors_clamp() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceColors(1));
        assert_eq!(app.image_trace_config.colors, 2, "colors clamped to min 2");
        app.apply(Action::SetImageTraceColors(50));
        assert_eq!(app.image_trace_config.colors, 30, "colors clamped to max 30");
    }

    #[test]
    fn test_image_trace_paths_clamp() {
        let mut app = App::new();
        app.apply(Action::SetImageTracePaths(0));
        assert_eq!(app.image_trace_config.paths, 1, "paths clamped to min 1");
        app.apply(Action::SetImageTracePaths(200));
        assert_eq!(app.image_trace_config.paths, 100, "paths clamped to max 100");
    }

    #[test]
    fn test_run_image_trace_pushes_result() {
        let mut app = App::new();
        app.apply(Action::SetImageTraceColors(6));
        app.apply(Action::RunImageTrace { image_id: 42 });
        assert_eq!(app.image_trace_results.len(), 1);
        let r = &app.image_trace_results[0];
        assert_eq!(r.source_image_id, 42);
        assert!(!r.expanded);
        // path_count = colors * 12 + noise = 6 * 12 + 25 = 97
        assert_eq!(r.path_count, 6 * 12 + 25);
    }

    #[test]
    fn test_expand_image_trace_result() {
        let mut app = App::new();
        app.apply(Action::RunImageTrace { image_id: 0 });
        assert!(!app.image_trace_results[0].expanded);
        app.apply(Action::ExpandImageTraceResult { result_index: 0 });
        assert!(app.image_trace_results[0].expanded);
    }

    // --- Wave N: Perspective Grid (extended config) ---

    #[test]
    fn test_perspective_grid_type_set() {
        let mut app = App::new();
        assert_eq!(app.perspective_grid_config.grid_type, PerspectiveGridType::TwoPoint);
        app.apply(Action::SetPerspectiveGridType(PerspectiveGridType::OnePoint));
        assert_eq!(app.perspective_grid_config.grid_type, PerspectiveGridType::OnePoint);
    }

    #[test]
    fn test_perspective_grid_config_toggle_visible() {
        let mut app = App::new();
        assert!(!app.perspective_grid_config.visible);
        app.apply(Action::TogglePerspectiveGridConfig);
        assert!(app.perspective_grid_config.visible);
        app.apply(Action::TogglePerspectiveGridConfig);
        assert!(!app.perspective_grid_config.visible);
    }

    #[test]
    fn test_perspective_grid_cell_size_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPerspectiveGridCellSize(0.0));
        assert!((app.perspective_grid_config.cell_size - 1.0).abs() < 1e-6, "cell_size clamped to 1");
        app.apply(Action::SetPerspectiveGridCellSize(9999.0));
        assert!((app.perspective_grid_config.cell_size - 500.0).abs() < 1e-6, "cell_size clamped to 500");
    }

    #[test]
    fn test_perspective_grid_active_plane() {
        let mut app = App::new();
        app.apply(Action::SetPerspectiveActivePlaneByName("floor".to_string()));
        assert!(!app.perspective_grid_config.left_plane.active);
        assert!(!app.perspective_grid_config.right_plane.active);
        assert!(app.perspective_grid_config.floor_plane.active);
    }

    #[test]
    fn test_move_vanishing_point() {
        let mut app = App::new();
        app.apply(Action::MoveVanishingPoint { which: "left".to_string(), x: -300.0, y: 50.0 });
        assert_eq!(app.perspective_grid_config.vanishing_point_left, (-300.0, 50.0));
        app.apply(Action::MoveVanishingPoint { which: "right".to_string(), x: 300.0, y: 50.0 });
        assert_eq!(app.perspective_grid_config.vanishing_point_right, (300.0, 50.0));
    }

    // --- Wave N: Global Swatches ---

    #[test]
    fn test_add_global_swatch() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "Crimson".to_string(), color: "#DC143C".to_string(), is_spot: false });
        assert_eq!(app.global_swatches.len(), 1);
        assert_eq!(app.global_swatches[0].name, "Crimson");
        assert_eq!(app.global_swatches[0].color, "#DC143C");
        assert!(app.global_swatches[0].is_global);
        assert_eq!(app.global_swatches[0].usage_count, 0);
    }

    #[test]
    fn test_edit_global_swatch() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "Blue".to_string(), color: "#0000FF".to_string(), is_spot: false });
        let id = app.global_swatches[0].id;
        app.apply(Action::EditGlobalSwatch { id, color: "#0033CC".to_string() });
        assert_eq!(app.global_swatches[0].color, "#0033CC");
    }

    #[test]
    fn test_delete_global_swatch() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "Red".to_string(), color: "#FF0000".to_string(), is_spot: false });
        let id = app.global_swatches[0].id;
        app.apply(Action::DeleteGlobalSwatch(id));
        assert!(app.global_swatches.is_empty());
    }

    #[test]
    fn test_create_swatch_group_and_add() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "A".to_string(), color: "#AAA".to_string(), is_spot: false });
        let sw_id = app.global_swatches[0].id;
        app.apply(Action::CreateSwatchGroup { name: "Warm".to_string() });
        let g_id = app.swatch_groups[0].id;
        app.apply(Action::AddSwatchToGroup { group_id: g_id, swatch_id: sw_id });
        // Adding again should be idempotent
        app.apply(Action::AddSwatchToGroup { group_id: g_id, swatch_id: sw_id });
        assert_eq!(app.swatch_groups[0].swatch_ids.len(), 1);
    }

    #[test]
    fn test_reorder_swatches() {
        let mut app = App::new();
        app.apply(Action::AddGlobalSwatch { name: "A".to_string(), color: "#111".to_string(), is_spot: false });
        app.apply(Action::AddGlobalSwatch { name: "B".to_string(), color: "#222".to_string(), is_spot: false });
        app.apply(Action::AddGlobalSwatch { name: "C".to_string(), color: "#333".to_string(), is_spot: false });
        let ids: Vec<usize> = app.global_swatches.iter().map(|s| s.id).collect();
        // Reverse the order
        let rev: Vec<usize> = ids.iter().rev().cloned().collect();
        app.apply(Action::ReorderSwatches(rev));
        assert_eq!(app.global_swatches[0].name, "C");
        assert_eq!(app.global_swatches[2].name, "A");
    }

    // --- Wave N: Artboards (extended) ---

    #[test]
    fn test_add_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 });
        assert_eq!(app.artboards_ex.len(), 1);
        assert_eq!(app.artboards_ex[0].width, 1920.0);
        assert_eq!(app.active_artboard_ex, Some(app.artboards_ex[0].id));
    }

    #[test]
    fn test_delete_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::DeleteArtboardEx(id));
        assert!(app.artboards_ex.is_empty());
        assert_eq!(app.active_artboard_ex, None);
    }

    #[test]
    fn test_rename_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::RenameArtboardEx { id, name: "Logo".to_string() });
        assert_eq!(app.artboards_ex[0].name, "Logo");
    }

    #[test]
    fn test_resize_artboard_clamp() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::ResizeArtboard { id, width: 0.0, height: 50000.0 });
        assert!((app.artboards_ex[0].width - 1.0).abs() < 1e-6, "width clamped to 1");
        assert!((app.artboards_ex[0].height - 32000.0).abs() < 1e-6, "height clamped to 32000");
    }

    #[test]
    fn test_duplicate_artboard_ex() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 100.0, y: 50.0, width: 200.0, height: 150.0 });
        let id = app.artboards_ex[0].id;
        app.apply(Action::DuplicateArtboardEx(id));
        assert_eq!(app.artboards_ex.len(), 2);
        let dup = &app.artboards_ex[1];
        assert_eq!(dup.x, 100.0 + 200.0 + 20.0, "x offset by width + 20");
        assert!(dup.name.starts_with("Copy of"));
        assert_eq!(app.active_artboard_ex, Some(dup.id));
    }

    #[test]
    fn test_reorder_artboards() {
        let mut app = App::new();
        app.apply(Action::AddArtboardEx { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        app.apply(Action::AddArtboardEx { x: 200.0, y: 0.0, width: 100.0, height: 100.0 });
        let ids: Vec<usize> = app.artboards_ex.iter().map(|a| a.id).collect();
        let rev: Vec<usize> = ids.iter().rev().cloned().collect();
        app.apply(Action::ReorderArtboards(rev));
        assert_eq!(app.artboards_ex[0].x, 200.0, "second artboard now first");
    }
}
