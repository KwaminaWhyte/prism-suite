use super::*;

impl App {
    pub(super) fn apply_painting(&mut self, action: Action) {
        match action {
            Action::SetBrushColor(c) => {
                self.brush.color = c;
                if let Some(edit) = self.text_edit.as_ref() {
                    let (layer, origin, string) = (edit.layer, edit.origin, edit.string.clone());
                    self.host.update_text_layer(
                        layer, &string, self.text_size, c, origin,
                        prism_io::text::TextAlign::Left, None,
                    );
                }
            }
            Action::SetBrushSize(s) => self.brush.size = s.clamp(1.0, 400.0),
            Action::SetBrushHardness(h) => self.brush.hardness = h.clamp(0.0, 0.99),
            Action::SetBrushOpacity(o) => self.brush.opacity = o.clamp(0.0, 1.0),
            Action::SetFillTolerance(t) => self.fill_tolerance = t.clamp(0.0, 1.0),
            Action::ToggleFillContiguous => self.fill_contiguous = !self.fill_contiguous,
            Action::ToggleGradientDither => self.gradient_dither = !self.gradient_dither,
            Action::PenAddNode(pos) => {
                self.pen_path.push(PenNode { pos, ctrl_in: pos, ctrl_out: pos });
            }
            Action::PenMoveHandle { idx, is_out, delta } => {
                if let Some(node) = self.pen_path.get_mut(idx) {
                    if is_out {
                        node.ctrl_out.0 += delta.0;
                        node.ctrl_out.1 += delta.1;
                    } else {
                        node.ctrl_in.0 += delta.0;
                        node.ctrl_in.1 += delta.1;
                    }
                }
            }
            Action::PenClose => {
                if self.pen_path.len() >= 2 {
                    if let Some(layer) = self.paint_target() {
                        let dabs = rasterize_pen_path(&self.pen_path, &self.brush);
                        if !dabs.is_empty() {
                            self.host.paint_dabs(layer, &dabs, false, true, false);
                        }
                    }
                }
                self.pen_path.clear();
                self.pen_closed = true;
            }
            Action::SetDodgeSize(s) => {
                self.dodge_size = s.clamp(1.0, 400.0);
            }
            Action::SetDodgeStrength(s) => {
                self.dodge_strength = s.clamp(0.01, 1.0);
            }
            Action::SetSmudgeStrength(s) => {
                self.smudge_strength = s.clamp(0.0, 1.0);
            }
            Action::SetLiquifyMode(m) => {
                self.liquify_mode = m;
            }
            Action::SetHealRadius(r) => {
                self.heal_radius = r.clamp(1, 500);
            }
            Action::SetHealMode(m) => {
                self.heal_mode = m;
            }
            Action::SetSpotHealMode(m) => {
                self.spot_heal_mode = m;
            }
            Action::SetSpotHealRadius(r) => {
                self.spot_heal_radius = r.max(1.0);
            }
            Action::SpotHeal { center, radius } => {
                self.last_spot_heal = Some((center, radius));
            }
            Action::RedEye { center, radius, darken } => {
                self.last_red_eye = Some((center, radius, darken));
            }
            Action::SetLiquifyTool(t) => {
                self.liquify_tool = t;
            }
            Action::SetLiquifyBrushSize(s) => {
                self.liquify_brush_size = s.clamp(1.0, 1500.0);
            }
            Action::SetLiquifyBrushPressure(p) => {
                self.liquify_brush_pressure = p.clamp(1.0, 100.0);
            }
            Action::SetLiquifyBrushDensity(d) => {
                self.liquify_brush_density = d.clamp(1.0, 100.0);
            }
            Action::ApplyLiquifyStroke(stroke) => {
                self.liquify_strokes.push(stroke);
            }
            Action::FreezeMaskRegion { center, radius } => {
                let ix = center[0] as usize;
                let iy = center[1] as usize;
                let needed = ix.max(iy) + 1;
                if self.liquify_frozen_mask.len() < needed {
                    self.liquify_frozen_mask.resize(needed, false);
                }
                let r = radius as usize;
                for dy in 0..=r {
                    for dx in 0..=r {
                        if dx * dx + dy * dy <= r * r {
                            let px = (ix + dx).min(self.liquify_frozen_mask.len() - 1);
                            let py = (iy + dy).min(self.liquify_frozen_mask.len() - 1);
                            let _ = (px, py);
                        }
                    }
                }
                self.liquify_frozen_mask.push(true);
            }
            Action::ThawAllMask => {
                self.liquify_frozen_mask.clear();
            }
            Action::ReconstructLiquify => {
                self.liquify_strokes.pop();
            }
            Action::RevertLiquify => {
                self.liquify_strokes.clear();
            }
            Action::SetLiquifyShowMesh(b) => {
                self.liquify_show_mesh = b;
            }
            Action::SetLiquifySmartRadius(b) => {
                self.liquify_smart_radius = b;
            }
            Action::SaveLiquifyMesh => {
                // Stub: mesh subdivisions unchanged; just marks intent.
            }
            Action::DefinePattern { name, pixels, width, height } => {
                self.pattern_library.push(PatternDef { name, pixels, width, height });
            }
            Action::SelectPattern(idx) => {
                if idx < self.pattern_library.len() {
                    self.active_pattern_idx = Some(idx);
                }
            }
            Action::DeletePattern(idx) => {
                if idx < self.pattern_library.len() {
                    self.pattern_library.remove(idx);
                    self.active_pattern_idx = self.active_pattern_idx.and_then(|i| {
                        if i == idx { None } else if i > idx { Some(i - 1) } else { Some(i) }
                    });
                }
            }
            Action::SetPatternStampScale(s) => {
                self.pattern_stamp_scale = s.clamp(0.1, 10.0);
            }
            Action::SetPatternStampAligned(a) => {
                self.pattern_stamp_aligned = a;
            }
            _ => {}
        }
    }
}
