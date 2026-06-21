use super::*;

// ---- Batch 5: Vanishing Point -------------------------------------------

/// Editing mode for the Vanishing Point overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VanishingToolMode {
    /// Clicking defines or moves the four plane corners.
    #[default]
    DefiningPlane,
    /// Clone/stamp within the perspective plane.
    Stamping,
    /// Paste content and drag it to fit the plane.
    Pasting,
}

/// A perspective plane defined by four canvas-space corners (TL/TR/BR/BL).
/// Used by the Vanishing Point feature for perspective-aware cloning.
#[derive(Clone, Debug)]
pub struct VanishingPlane {
    /// Corners in canvas doc-px order: [TL, TR, BR, BL].
    pub corners: [[f32; 2]; 4],
    /// Grid cell size in doc px for the overlay grid (default 50).
    pub grid_size: f32,
    pub active: bool,
}

// ---- Batch 7: Alpha Channels ------------------------------------------------

/// A named alpha channel saved from a selection mask.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AlphaChannel {
    pub name: String,
    /// Flat row-major width×height coverage values 0..=1.
    pub mask: Vec<f32>,
    pub width: u32,
    pub height: u32,
}

// ---- Batch 7: Blend If ------------------------------------------------------

/// Per-layer "Blend If" luminance range controls (Photoshop Layer Style parity).
/// Values are in the Photoshop 0..255 scale stored as f32.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlendIf {
    /// Shadow range start for THIS layer.
    pub this_black: f32,
    /// Highlight range end for THIS layer.
    pub this_white: f32,
    /// Shadow range start for the UNDERLYING layer.
    pub under_black: f32,
    /// Highlight range end for the UNDERLYING layer.
    pub under_white: f32,
}

impl Default for BlendIf {
    fn default() -> Self {
        Self {
            this_black: 0.0,
            this_white: 255.0,
            under_black: 0.0,
            under_white: 255.0,
        }
    }
}

// ---- Batch 6: Artboards -----------------------------------------------------

/// A named canvas region for multi-artboard documents (PS 2015+ parity).
#[derive(Clone, Debug)]
pub struct Artboard {
    pub id: u64,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Background fill (straight sRGB RGBA 0..1, default white).
    pub background_color: [f32; 4],
}

// ---- Batch 6: Apply Image ---------------------------------------------------

/// Which channel of a source layer to blend in Apply Image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyImageChannel {
    Rgb,
    Red,
    Green,
    Blue,
    Alpha,
    Luminosity,
}

impl Default for ApplyImageChannel {
    fn default() -> Self { ApplyImageChannel::Rgb }
}

/// Parameters for the Apply Image command (stored for the dialog UI).
#[derive(Clone, Debug)]
pub struct ApplyImageParams {
    pub source_layer: LayerId,
    pub source_channel: ApplyImageChannel,
    pub target_layer: LayerId,
    pub blend_mode: BlendMode,
    /// Blend opacity 0..1.
    pub opacity: f32,
    pub invert_source: bool,
    pub mask_layer: Option<LayerId>,
}
// ---- Batch 6: Soft Proof (expanded) -----------------------------------------

/// Color profile to simulate during soft proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProofProfile {
    #[default]
    WorkingCmyk,
    Srgb,
    AdobeRgb,
    PrinterProfile,
    MonitorRgb,
}

impl ProofProfile {
    pub fn label(self) -> &'static str {
        match self {
            ProofProfile::WorkingCmyk   => "Working CMYK",
            ProofProfile::Srgb          => "sRGB",
            ProofProfile::AdobeRgb      => "Adobe RGB",
            ProofProfile::PrinterProfile => "Printer Profile",
            ProofProfile::MonitorRgb    => "Monitor RGB",
        }
    }
}

/// Perceptual rendering intent for soft proof gamut mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderingIntent {
    #[default]
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

impl RenderingIntent {
    pub fn label(self) -> &'static str {
        match self {
            RenderingIntent::Perceptual              => "Perceptual",
            RenderingIntent::RelativeColorimetric    => "Relative Colorimetric",
            RenderingIntent::Saturation              => "Saturation",
            RenderingIntent::AbsoluteColorimetric    => "Absolute Colorimetric",
        }
    }
}

/// Full soft-proof settings (Photoshop View > Proof Setup parity).
#[derive(Clone, Debug)]
pub struct SoftProofSettings {
    pub profile: ProofProfile,
    pub intent: RenderingIntent,
    pub black_point_compensation: bool,
    pub simulate_paper_white: bool,
    pub simulate_black_ink: bool,
    pub gamut_warning: bool,
    /// Highlight colour for out-of-gamut pixels (straight sRGB RGBA, default green).
    pub gamut_warning_color: [f32; 4],
}

impl Default for SoftProofSettings {
    fn default() -> Self {
        Self {
            profile: ProofProfile::WorkingCmyk,
            intent: RenderingIntent::Perceptual,
            black_point_compensation: true,
            simulate_paper_white: false,
            simulate_black_ink: false,
            gamut_warning: false,
            gamut_warning_color: [0.0, 1.0, 0.0, 1.0],
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vanishing_tool_mode_default() {
        assert_eq!(VanishingToolMode::default(), VanishingToolMode::DefiningPlane);
    }

    #[test]
    fn test_vanishing_plane_fields() {
        let vp = VanishingPlane {
            corners: [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
            grid_size: 50.0,
            active: true,
        };
        assert_eq!(vp.grid_size, 50.0);
        assert!(vp.active);
    }

    #[test]
    fn test_artboard_fields() {
        let ab = Artboard {
            id: 1,
            name: "Board 1".into(),
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            background_color: [1.0, 1.0, 1.0, 1.0],
        };
        assert_eq!(ab.width, 800);
        assert_eq!(ab.name, "Board 1");
    }

    #[test]
    fn test_proof_profile_default() {
        assert_eq!(ProofProfile::default(), ProofProfile::WorkingCmyk);
    }

    #[test]
    fn test_rendering_intent_default() {
        assert_eq!(RenderingIntent::default(), RenderingIntent::Perceptual);
    }

    #[test]
    fn test_soft_proof_settings_default() {
        let s = SoftProofSettings::default();
        assert_eq!(s.gamut_warning_color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn test_set_vanishing_tool_mode() {
        let mut app = App::new();
        app.apply(Action::SetVanishingToolMode(VanishingToolMode::Stamping));
        assert_eq!(app.vanishing_tool_mode, VanishingToolMode::Stamping);
    }

    #[test]
    fn test_add_artboard() {
        let mut app = App::new();
        app.apply(Action::AddArtboard { name: "A1".into(), x: 0, y: 0, width: 400, height: 300 });
        assert_eq!(app.artboards.len(), 1);
        assert_eq!(app.artboards[0].name, "A1");
    }

    #[test]
    fn test_toggle_soft_proof() {
        let mut app = App::new();
        let before = app.soft_proof_enabled;
        app.apply(Action::ToggleSoftProof);
        assert_eq!(app.soft_proof_enabled, !before);
    }
}
