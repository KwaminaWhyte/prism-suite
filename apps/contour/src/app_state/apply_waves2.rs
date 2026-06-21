use super::*;

impl App {
    pub(super) fn apply_waves2(&mut self, action: Action) {
        match action {
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


            a => self.apply_waves2_b24(a),
        }
    }
}
