use super::*;

impl App {
    pub(super) fn apply_batch10(&mut self, action: Action) {
        match action {
            // =========================================================
            // Batch 10: Variable Fonts & OpenType
            // =========================================================
            Action::SetVariableAxis { shape_id, axis_tag, value } => {
                let axes = self.variable_axis_values.entry(shape_id).or_default();
                if let Some(a) = axes.iter_mut().find(|a| a.axis_tag == axis_tag) {
                    a.value = value;
                } else {
                    axes.push(VariableAxisValue { axis_tag, value });
                }
            }
            Action::ResetVariableAxes { shape_id } => {
                self.variable_axis_values.remove(&shape_id);
            }
            Action::SetOpenTypeFeature { shape_id, feature, enabled } => {
                let f = self.opentype_features.entry(shape_id).or_default();
                match feature.as_str() {
                    "ligatures"  => f.ligatures = enabled,
                    "disc_lig"   => f.discretionary_ligatures = enabled,
                    "hist_lig"   => f.historical_ligatures = enabled,
                    "calt"       => f.contextual_alternates = enabled,
                    "smcp"       => f.small_caps = enabled,
                    "c2sc"       => f.all_small_caps = enabled,
                    "ordn"       => f.ordinals = enabled,
                    "frac"       => f.fractions = enabled,
                    "zero"       => f.slashed_zero = enabled,
                    "tnum"       => f.tabular_figures = enabled,
                    "pnum"       => f.proportional_figures = enabled,
                    "sups"       => f.superscript = enabled,
                    "subs"       => f.subscript = enabled,
                    _ => {}
                }
            }
            Action::SetStylisticSet { shape_id, set } => {
                let f = self.opentype_features.entry(shape_id).or_default();
                f.stylistic_set = set.map(|s| s.clamp(1, 20));
            }
            Action::ApplyAllSmallCaps { shape_id } => {
                let f = self.opentype_features.entry(shape_id).or_default();
                f.all_small_caps = true;
                f.small_caps = false;
            }

            // =========================================================
            // Batch 10: Character & Paragraph Panel
            // =========================================================
            Action::SetCharacterTracking { shape_id, tracking } => {
                self.char_styles.entry(shape_id).or_default().tracking = tracking;
            }
            Action::SetCharacterKerning { shape_id, mode } => {
                self.char_styles.entry(shape_id).or_default().kerning = mode;
            }
            Action::SetBaselineShift { shape_id, shift } => {
                self.char_styles.entry(shape_id).or_default().baseline_shift = shift;
            }
            Action::SetHorizontalScale { shape_id, scale } => {
                self.char_styles.entry(shape_id).or_default().horizontal_scale =
                    scale.clamp(1.0, 1000.0);
            }
            Action::SetVerticalScale { shape_id, scale } => {
                self.char_styles.entry(shape_id).or_default().vertical_scale =
                    scale.clamp(1.0, 1000.0);
            }
            Action::SetUnderline { shape_id, underline } => {
                self.char_styles.entry(shape_id).or_default().underline = underline;
            }
            Action::SetStrikethrough { shape_id, strikethrough } => {
                self.char_styles.entry(shape_id).or_default().strikethrough = strikethrough;
            }
            Action::SetParagraphAlignment { shape_id, alignment } => {
                self.para_styles.entry(shape_id).or_default().alignment = alignment;
            }
            Action::SetParagraphSpacing { shape_id, before, after } => {
                let s = self.para_styles.entry(shape_id).or_default();
                s.space_before = before;
                s.space_after = after;
            }
            Action::SetFirstLineIndent { shape_id, indent } => {
                self.para_styles.entry(shape_id).or_default().first_line_indent = indent;
            }
            Action::SetHyphenation { shape_id, enabled } => {
                self.para_styles.entry(shape_id).or_default().hyphenation = enabled;
            }
            Action::AddTabStop { shape_id, position } => {
                let stops = &mut self.para_styles.entry(shape_id).or_default().tab_stops;
                if !stops.contains(&position) {
                    stops.push(position);
                    stops.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                }
            }
            Action::RemoveTabStop { shape_id, position } => {
                if let Some(s) = self.para_styles.get_mut(&shape_id) {
                    s.tab_stops.retain(|&t| (t - position).abs() > 0.001);
                }
            }

            // =========================================================
            // Batch 10: Blend Tool
            // =========================================================
            Action::MakeBlend { shape_id_a, shape_id_b, spacing } => {
                let id = self.next_blend_id;
                self.next_blend_id += 1;
                self.blends.push(BlendObject {
                    id,
                    name: format!("Blend {}", id + 1),
                    shape_ids: vec![shape_id_a, shape_id_b],
                    spacing,
                    orientation: BlendOrientation::AlignToPage,
                    spine_path_id: None,
                });
            }
            Action::ReleaseBlend { blend_id } => {
                self.blends.retain(|b| b.id != blend_id);
            }
            Action::ExpandBlend { blend_id } => {
                // Stub: just remove the blend record (expanded into doc shapes).
                self.blends.retain(|b| b.id != blend_id);
            }
            Action::SetBlendSpacing { blend_id, spacing } => {
                if let Some(b) = self.blends.iter_mut().find(|b| b.id == blend_id) {
                    b.spacing = spacing;
                }
            }
            Action::SetBlendOrientation { blend_id, orientation } => {
                if let Some(b) = self.blends.iter_mut().find(|b| b.id == blend_id) {
                    b.orientation = orientation;
                }
            }
            Action::ReplaceBlendSpine { blend_id, path_id } => {
                if let Some(b) = self.blends.iter_mut().find(|b| b.id == blend_id) {
                    b.spine_path_id = Some(path_id);
                }
            }
            Action::ReverseBlend { blend_id } => {
                if let Some(b) = self.blends.iter_mut().find(|b| b.id == blend_id) {
                    b.shape_ids.reverse();
                }
            }
            Action::ReverseBlendSpine { blend_id } => {
                // Stub: record a flag by clearing spine (spine is reversed, not tracked)
                if let Some(b) = self.blends.iter_mut().find(|b| b.id == blend_id) {
                    let _ = b.spine_path_id.take();
                }
            }

            // =========================================================
            // Batch 10: 3D Effects
            // =========================================================
            Action::Apply3DExtrude { shape_id, config } => {
                self.extrude_3d.insert(shape_id, config);
                // Remove any revolve for the same shape (can't have both).
                self.revolve_3d.remove(&shape_id);
            }
            Action::Update3DExtrude { shape_id, depth, rotate_x, rotate_y, rotate_z } => {
                if let Some(e) = self.extrude_3d.get_mut(&shape_id) {
                    if let Some(d) = depth {
                        e.depth = d.clamp(0.0, 2000.0);
                    }
                    if let Some(rx) = rotate_x {
                        e.rotate_x = rx.clamp(-180.0, 180.0);
                    }
                    if let Some(ry) = rotate_y {
                        e.rotate_y = ry.clamp(-180.0, 180.0);
                    }
                    if let Some(rz) = rotate_z {
                        e.rotate_z = rz.clamp(-180.0, 180.0);
                    }
                }
            }
            Action::Remove3DEffect { shape_id } => {
                self.extrude_3d.remove(&shape_id);
                self.revolve_3d.remove(&shape_id);
            }
            Action::Apply3DRevolve { shape_id, config } => {
                self.revolve_3d.insert(shape_id, config);
                // Remove any extrude for the same shape.
                self.extrude_3d.remove(&shape_id);
            }
            Action::Update3DRevolve { shape_id, angle, offset } => {
                if let Some(r) = self.revolve_3d.get_mut(&shape_id) {
                    if let Some(a) = angle {
                        r.angle = a.clamp(0.0, 360.0);
                    }
                    if let Some(o) = offset {
                        r.offset = o.max(0.0);
                    }
                }
            }
            Action::Set3DLighting { shape_id, intensity, ambient } => {
                if let Some(e) = self.extrude_3d.get_mut(&shape_id) {
                    e.light_intensity = intensity.clamp(0.0, 100.0);
                    e.ambient_light = ambient.clamp(0.0, 100.0);
                }
            }
            Action::Set3DPerspective { shape_id, degrees } => {
                if let Some(e) = self.extrude_3d.get_mut(&shape_id) {
                    e.perspective = degrees.clamp(0.0, 160.0);
                }
            }

            // =========================================================
            // Batch 10: PDF Export State
            // =========================================================
            Action::SetPdfStandard(s) => {
                self.pdf_export_config.standard = s;
            }
            Action::SetPdfCompatibility(c) => {
                self.pdf_export_config.compatibility = c;
            }
            Action::SetPdfEmbedFonts(v) => {
                self.pdf_export_config.embed_fonts = v;
            }
            Action::SetPdfFlattenTransparency(v) => {
                self.pdf_export_config.flatten_transparency = v;
            }
            Action::SetPdfColorSpace(cs) => {
                self.pdf_export_config.color_space = cs;
            }
            Action::SetPdfBleed { top, bottom, left, right } => {
                self.pdf_export_config.bleed_top = top.max(0.0);
                self.pdf_export_config.bleed_bottom = bottom.max(0.0);
                self.pdf_export_config.bleed_left = left.max(0.0);
                self.pdf_export_config.bleed_right = right.max(0.0);
                self.pdf_export_config.include_bleed = true;
            }
            Action::SetPdfMarks(m) => {
                self.pdf_export_config.marks = m;
            }
            Action::SetPdfPassword { user, owner } => {
                self.pdf_export_config.require_password = !user.is_empty() || !owner.is_empty();
                self.pdf_export_config.user_password = user;
                self.pdf_export_config.owner_password = owner;
            }
            Action::SetPdfPermissions { printing, editing, copying } => {
                self.pdf_export_config.allow_printing = printing;
                self.pdf_export_config.allow_editing = editing;
                self.pdf_export_config.allow_copying = copying;
            }
            Action::ExportAsPdf { path } => {
                self.status_message = Some((
                    format!("PDF export → {}", path),
                    std::time::Instant::now(),
                ));
            }

            _ => self.apply_waves(action),
        }
    }
}
