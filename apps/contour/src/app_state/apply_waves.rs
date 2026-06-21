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
