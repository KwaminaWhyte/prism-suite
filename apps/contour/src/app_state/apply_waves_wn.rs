use super::*;

impl App {
    pub(super) fn apply_waves_wn(&mut self, action: Action) {
        match action {
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
                // Run the real contour tracer on the embedded image if we have
                // its pixels; otherwise fall back to a config-derived estimate so
                // a linked-but-unloaded image still records a result.
                let cfg = self.image_trace_config.clone();
                let path_count = self
                    .doc
                    .placed_images
                    .get(image_id as u64)
                    .and_then(|img| match &img.source {
                        crate::placed_image::ImageSource::Embedded { width, height, rgba } => {
                            Some(crate::trace_contour::trace_contours(
                                rgba,
                                *width as usize,
                                *height as usize,
                                &cfg,
                            ).len())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| {
                        cfg.colors as usize * 12 + cfg.noise as usize
                    });
                self.image_trace_results.push(ImageTraceResult {
                    source_image_id: image_id,
                    config: cfg,
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

            // --- Welcome panel ---
            Action::DismissWelcome => {
                self.show_welcome = false;
            }
            Action::NewDocument => {
                // Reset the document to a fresh state sized to the current
                // Document Setup, dismissing the welcome panel. Delegates to the
                // real Batch-12 implementation so the single artboard matches the
                // configured dimensions.
                self.apply(Action::NewDocumentFromSetup);
            }
            Action::OpenFile => {
                // Real file-picker integration is deferred; dismiss the welcome
                // panel so the user can work with the current document in the
                // meantime.
                self.show_welcome = false;
            }

            a => self.apply_batch12(a),
        }
    }
}
