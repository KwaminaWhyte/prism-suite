use super::*;

impl App {
    pub(super) fn apply_extended(&mut self, action: Action) {
        match action {
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

            a => self.apply_extended2(a),
        }
    }
}
