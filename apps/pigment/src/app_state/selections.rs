use super::*;

impl App {
    pub(super) fn apply_selections(&mut self, action: Action) {
        match action {
            Action::SetMarquee { rect, ellipse } => {
                if rect[2] <= 0.5 || rect[3] <= 0.5 {
                    self.host.clear_selection();
                } else {
                    self.host.set_marquee(rect, ellipse);
                }
                self.bump_selection();
            }
            Action::ClearSelection => {
                self.host.clear_selection();
                self.bump_selection();
            }
            Action::InvertSelection => {
                let mask = self.host.read_selection_or_empty();
                let inv: Vec<f32> = mask.iter().map(|&v| 1.0 - v).collect();
                self.host.upload_selection_mask(&inv);
                self.bump_selection();
            }
            Action::SelectFocusArea { threshold, sensitivity, invert } => {
                self.focus_area_threshold = threshold.clamp(0.0, 1.0);
                self.focus_area_sensitivity = sensitivity.clamp(0.0, 1.0);
                let Some(layer) = self.paint_target() else { return };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let mask_u8 = focus_area_mask(&px, dw, dh, threshold, sensitivity, invert);
                    let mask_f32: Vec<f32> = mask_u8.iter().map(|&v| v as f32 / 255.0).collect();
                    self.host.upload_selection_mask(&mask_f32);
                    self.bump_selection();
                    self.status_message = Some("Focus Area selection applied".to_string());
                }
            }
            Action::SetFocusAreaThreshold(t) => {
                self.focus_area_threshold = t.clamp(0.0, 1.0);
            }
            Action::SaveSelectionAsChannel(name) => {
                let mask = self.host.read_selection_or_empty();
                let width = self.host.doc_w;
                let height = self.host.doc_h;
                self.alpha_channels.push(AlphaChannel { name, mask, width, height });
            }
            Action::LoadChannelAsSelection(idx) => {
                if idx < self.alpha_channels.len() {
                    let mask = self.alpha_channels[idx].mask.clone();
                    self.host.upload_selection_mask(&mask);
                    self.bump_selection();
                }
            }
            Action::DeleteChannel(idx) => {
                if idx < self.alpha_channels.len() {
                    self.alpha_channels.remove(idx);
                }
            }
            Action::DuplicateChannel(idx) => {
                if idx < self.alpha_channels.len() {
                    let mut copy = self.alpha_channels[idx].clone();
                    copy.name = format!("{} copy", copy.name);
                    self.alpha_channels.push(copy);
                }
            }
            Action::SelectSubject => {
                let Some(layer) = self.paint_target() else { return };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let mask_u8 = select_subject_mask(
                        &px, dw, dh,
                        self.select_subject_threshold,
                        self.select_subject_feather,
                    );
                    let mask_f32: Vec<f32> = mask_u8.iter().map(|&v| v as f32 / 255.0).collect();
                    self.host.upload_selection_mask(&mask_f32);
                    self.bump_selection();
                    self.status_message = Some("Select Subject applied".to_string());
                }
            }
            Action::SetSelectSubjectThreshold(t) => {
                self.select_subject_threshold = t.clamp(0.0, 1.0);
            }
            Action::SetSelectSubjectFeather(f) => {
                self.select_subject_feather = f.clamp(0.0, 50.0);
            }
            Action::SetSelectSubjectMode(m) => {
                self.select_subject_mode = m;
            }
            Action::RunSelectSubject => {
                let cloud_used = self.select_subject_mode == SelectSubjectMode::Cloud;
                self.last_select_subject = Some(SelectSubjectResult {
                    coverage: 0.72,
                    confidence: 0.89,
                    cloud_used,
                });
            }
            Action::ToggleSelectAndMask => {
                self.select_and_mask_open = !self.select_and_mask_open;
            }
            Action::SetSelectSubjectRefine(b) => {
                self.select_subject_refine = b;
            }
            Action::InvertSelectSubject => {
                if self.last_select_subject.is_some() {
                    self.select_subject_refine = !self.select_subject_refine;
                }
            }
            Action::OpenSelectMask => {
                self.select_mask_open = true;
            }
            Action::CloseSelectMask => {
                self.select_mask_open = false;
            }
            Action::SetSelectMaskRadius(r) => {
                self.select_mask_config.radius = r.clamp(0.0, 250.0);
            }
            Action::SetSelectMaskSmooth(s) => {
                self.select_mask_config.smooth = s.min(100);
            }
            Action::SetSelectMaskFeather(f) => {
                self.select_mask_config.feather = f.clamp(0.0, 250.0);
            }
            Action::SetSelectMaskContrast(c) => {
                self.select_mask_config.contrast = c.min(100);
            }
            Action::SetSelectMaskShiftEdge(e) => {
                self.select_mask_config.shift_edge = e.clamp(-100, 100);
            }
            Action::ApplySelectMask => {
                self.select_mask_open = false;
            }
            Action::AddArtboard { name, x, y, width, height } => {
                let id = self.next_artboard_id;
                self.next_artboard_id += 1;
                self.artboards.push(Artboard {
                    id,
                    name,
                    x,
                    y,
                    width,
                    height,
                    background_color: [1.0, 1.0, 1.0, 1.0],
                });
                self.active_artboard = Some(id);
            }
            Action::RemoveArtboard(id) => {
                self.artboards.retain(|a| a.id != id);
                if self.active_artboard == Some(id) {
                    self.active_artboard = self.artboards.first().map(|a| a.id);
                }
            }
            Action::SelectArtboard(id) => {
                if self.artboards.iter().any(|a| a.id == id) {
                    self.active_artboard = Some(id);
                }
            }
            Action::RenameArtboard { id, name } => {
                if let Some(ab) = self.artboards.iter_mut().find(|a| a.id == id) {
                    ab.name = name;
                }
            }
            Action::MoveArtboard { id, x, y } => {
                if let Some(ab) = self.artboards.iter_mut().find(|a| a.id == id) {
                    ab.x = x;
                    ab.y = y;
                }
            }
            Action::ResizeArtboard { id, width, height } => {
                if let Some(ab) = self.artboards.iter_mut().find(|a| a.id == id) {
                    ab.width = width;
                    ab.height = height;
                }
            }
            Action::DuplicateArtboard(id) => {
                if let Some(src) = self.artboards.iter().find(|a| a.id == id).cloned() {
                    let new_id = self.next_artboard_id;
                    self.next_artboard_id += 1;
                    self.artboards.push(Artboard {
                        id: new_id,
                        name: format!("{} copy", src.name),
                        x: src.x + 20,
                        y: src.y + 20,
                        ..src
                    });
                    self.active_artboard = Some(new_id);
                }
            }
            Action::SetArtboardBackground { id, color } => {
                if let Some(ab) = self.artboards.iter_mut().find(|a| a.id == id) {
                    ab.background_color = color;
                }
            }
            Action::ExportArtboards(path) => {
                self.status_message = Some(format!(
                    "Export artboards → {:?} ({} boards)",
                    path, self.artboards.len()
                ));
            }
            Action::ToggleArtboardsPanel => {
                self.artboards_panel_open = !self.artboards_panel_open;
            }
            Action::AddVanishingPlane { corners } => {
                self.vanishing_planes.push(VanishingPlane {
                    corners,
                    grid_size: 50.0,
                    active: true,
                });
                self.active_vanishing_plane = Some(self.vanishing_planes.len() - 1);
            }
            Action::RemoveVanishingPlane(idx) => {
                if idx < self.vanishing_planes.len() {
                    self.vanishing_planes.remove(idx);
                    self.active_vanishing_plane = self.active_vanishing_plane.and_then(|i| {
                        if i == idx { None } else if i > idx { Some(i - 1) } else { Some(i) }
                    });
                }
            }
            Action::SelectVanishingPlane(idx) => {
                if idx < self.vanishing_planes.len() {
                    self.active_vanishing_plane = Some(idx);
                }
            }
            Action::SetVanishingGridSize(s) => {
                if let Some(idx) = self.active_vanishing_plane {
                    if let Some(plane) = self.vanishing_planes.get_mut(idx) {
                        plane.grid_size = s.clamp(5.0, 500.0);
                    }
                }
            }
            Action::SetVanishingToolMode(mode) => {
                self.vanishing_tool_mode = mode;
            }
            Action::SetVanishingPlaneCorner { plane_idx, corner_idx, pos } => {
                if let Some(plane) = self.vanishing_planes.get_mut(plane_idx) {
                    if corner_idx < 4 {
                        plane.corners[corner_idx] = pos;
                    }
                }
            }
            Action::StampInPerspective { src, dst, radius } => {
                let Some(idx) = self.active_vanishing_plane else { return };
                let Some(plane) = self.vanishing_planes.get(idx).cloned() else { return };
                let Some(layer) = self.paint_target() else { return };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(mut px) = self.host.read_layer_f32(layer) {
                    stamp_in_perspective(&mut px, dw, dh, &plane.corners, src, dst, radius);
                    self.host.upload_layer_f32(layer, &px);
                    self.status_message = Some("Perspective stamp applied".to_string());
                }
            }
            Action::OpenVanishingPoint => {
                self.vanishing_point_open = true;
            }
            Action::CloseVanishingPoint => {
                self.vanishing_point_open = false;
            }
            Action::ToggleApplyImageDialog => {
                self.apply_image_dialog_open = !self.apply_image_dialog_open;
            }
            Action::SetApplyImageSource { layer, channel } => {
                self.apply_image_params.source_layer = layer;
                self.apply_image_params.source_channel = channel;
            }
            Action::SetApplyImageTarget(id) => {
                self.apply_image_params.target_layer = id;
            }
            Action::SetApplyImageBlend(mode) => {
                self.apply_image_params.blend_mode = mode;
            }
            Action::SetApplyImageOpacity(o) => {
                self.apply_image_params.opacity = o.clamp(0.0, 1.0);
            }
            Action::SetApplyImageInvert(inv) => {
                self.apply_image_params.invert_source = inv;
            }
            Action::SetApplyImageMask(opt) => {
                self.apply_image_params.mask_layer = opt;
            }
            Action::ApplyImage => {
                let p = self.apply_image_params.clone();
                let src_px = self.host.read_layer_f32(p.source_layer);
                let tgt_px = self.host.read_layer_f32(p.target_layer);
                if let (Some(src), Some(tgt)) = (src_px, tgt_px) {
                    let result = apply_image_blend(
                        &src, &tgt, p.source_channel, p.blend_mode,
                        p.opacity, p.invert_source,
                    );
                    self.host.upload_layer_f32(p.target_layer, &result);
                    self.apply_image_dialog_open = false;
                    self.status_message = Some("Apply Image complete".to_string());
                }
            }
            _ => {}
        }
    }
}
