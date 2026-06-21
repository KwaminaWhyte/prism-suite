use super::*;

impl App {
    pub(super) fn apply_canvas(&mut self, action: Action) {
        match action {
            Action::ZoomBy(factor) => {
                self.view.zoom_to(factor, glam::Vec2::ZERO);
            }
            Action::ResetView => {
                self.view = ViewTransform::default();
            }
            Action::SetColorMode(mode) => {
                self.color_mode = mode;
            }
            Action::RotateCanvas(deg) => {
                self.canvas_rotation_deg = (self.canvas_rotation_deg + deg).rem_euclid(360.0);
                self.host.mark_dirty();
            }
            Action::ResetCanvasRotation => {
                self.canvas_rotation_deg = 0.0;
                self.host.mark_dirty();
            }
            Action::TogglePanel(name) => {
                let v = self.panel_visibility.entry(name).or_insert(true);
                *v = !*v;
            }
            Action::SaveWorkspace(name) => {
                let config_dir = dirs_home_workspace_dir();
                if let Some(dir) = config_dir {
                    let _ = std::fs::create_dir_all(&dir);
                    let path = dir.join(format!("{name}.json"));
                    let obj: serde_json::Map<String, serde_json::Value> = self
                        .panel_visibility
                        .iter()
                        .map(|(k, &v)| (k.clone(), serde_json::Value::Bool(v)))
                        .collect();
                    if let Ok(json) = serde_json::to_string_pretty(&obj) {
                        let _ = std::fs::write(path, json);
                    }
                }
                self.status_message = Some(format!("Workspace '{name}' saved"));
            }
            Action::ResetWorkspace => {
                for v in self.panel_visibility.values_mut() {
                    *v = true;
                }
            }
            Action::AddGuideH(pos) => {
                self.guides_h.push(pos);
            }
            Action::AddGuideV(pos) => {
                self.guides_v.push(pos);
            }
            Action::RemoveGuide { horizontal, idx } => {
                let v = if horizontal { &mut self.guides_h } else { &mut self.guides_v };
                if idx < v.len() {
                    v.remove(idx);
                }
            }
            Action::ClearGuides => {
                self.guides_h.clear();
                self.guides_v.clear();
            }
            Action::ToggleGuides => {
                self.guides_visible = !self.guides_visible;
            }
            Action::SetImageSize { width, height } => {
                if width > 0 && height > 0 {
                    self.status_message = Some(format!(
                        "Image Size: resample to {width}×{height}px — coming in a future update"
                    ));
                }
            }
            Action::SetCanvasSize { width, height } => {
                if width > 0 && height > 0 {
                    self.status_message = Some(format!(
                        "Canvas Size: crop/expand to {width}×{height}px — coming in a future update"
                    ));
                }
            }
            Action::ToggleGridSnap => {
                self.snap_to_grid = !self.snap_to_grid;
            }
            Action::SetGridSize(s) => {
                self.grid_size = s.clamp(1.0, 256.0);
            }
            Action::OpenMenu(name) => {
                self.active_menu = name;
            }
            Action::SetColorProfile(p) => {
                self.color_profile = p;
                self.status_message = if p == ColorProfile::Srgb {
                    None
                } else {
                    Some(format!("Soft proof: {}", p.label()))
                };
            }
            Action::SetSoftProof(mode) => {
                self.soft_proof = mode;
                self.host.set_soft_proof(mode);
                self.status_message = if mode == SoftProofMode::Off {
                    None
                } else {
                    Some(format!("Soft proof: {}", mode.label()))
                };
            }
            Action::SetHistogramChannel(ch) => {
                self.histogram_channel = ch;
            }
            Action::DetachPanel(name) => {
                self.panel_detached.insert(name, true);
                self.panel_positions.entry(name).or_insert((200.0, 200.0));
            }
            Action::AttachPanel(name) => {
                self.panel_detached.insert(name, false);
            }
            Action::MovePanel(name, x, y) => {
                self.panel_positions.insert(name, (x, y));
            }
            Action::ToggleSoftProof => {
                self.soft_proof_enabled = !self.soft_proof_enabled;
            }
            Action::SetProofProfile(profile) => {
                self.soft_proof_settings.profile = profile;
            }
            Action::SetRenderingIntent(intent) => {
                self.soft_proof_settings.intent = intent;
            }
            Action::SetBlackPointCompensation(v) => {
                self.soft_proof_settings.black_point_compensation = v;
            }
            Action::SetSimulatePaperWhite(v) => {
                self.soft_proof_settings.simulate_paper_white = v;
            }
            Action::SetSimulateBlackInk(v) => {
                self.soft_proof_settings.simulate_black_ink = v;
            }
            Action::ToggleGamutWarning => {
                self.soft_proof_settings.gamut_warning = !self.soft_proof_settings.gamut_warning;
            }
            Action::SetGamutWarningColor(c) => {
                self.soft_proof_settings.gamut_warning_color = c;
            }
            _ => {}
        }
    }
}
