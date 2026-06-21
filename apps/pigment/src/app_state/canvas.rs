use super::*;

/// The editing tools, mirroring the egui app's `Tool` enum (see
/// `pigment-app/src/app/mod.rs`). The full retouch family (Clone/Heal/etc.) is
/// included so panel parity is reachable; the GPUI host wires behavior per wave.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Move,      // pan the view (hand)
    MoveLayer, // translate the active layer
    Brush,
    Eraser,
    Clone, // clone stamp
    Heal,  // healing brush
    Dodge, // dodge (lighten)
    Burn,  // burn (darken)
    Smudge, // smudge (blend)
    Fill,
    Eyedropper,
    SelectRect,
    SelectEllipse,
    Lasso,
    MagicWand,
    Transform,
    Crop,
    Text,
    Pen,
    ShapeRect,
    ShapeEllipse,
    Gradient,
    Slice, // mark rectangular export regions
    Liquify, // warp mesh push/pull
}

/// A named rectangular region of the canvas for per-slice export.
#[derive(Clone, Debug)]
pub struct Slice {
    pub id: u32,
    /// `[x, y, w, h]` in document pixels.
    pub rect: [f32; 4],
    pub name: String,
}

impl Tool {
    /// Short label for the tools strip / toolbar.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::MoveLayer => "MoveL",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Clone => "Clone",
            Tool::Heal => "Heal",
            Tool::Dodge => "Dodge",
            Tool::Burn => "Burn",
            Tool::Smudge => "Smudge",
            Tool::Fill => "Fill",
            Tool::Eyedropper => "Eyedr",
            Tool::SelectRect => "Rect",
            Tool::SelectEllipse => "Ellip",
            Tool::Lasso => "Lasso",
            Tool::MagicWand => "Wand",
            Tool::Transform => "Xform",
            Tool::Crop => "Crop",
            Tool::Text => "Text",
            Tool::Pen => "Pen",
            Tool::ShapeRect => "RectS",
            Tool::ShapeEllipse => "EllpS",
            Tool::Gradient => "Grad",
            Tool::Slice => "Slice",
            Tool::Liquify => "Liqfy",
        }
    }

    /// Stable ordering for the tools strip (matches the egui palette grouping).
    pub const ALL: [Tool; 24] = [
        Tool::Move,
        Tool::MoveLayer,
        Tool::Brush,
        Tool::Eraser,
        Tool::Clone,
        Tool::Heal,
        Tool::Dodge,
        Tool::Burn,
        Tool::Smudge,
        Tool::Fill,
        Tool::Eyedropper,
        Tool::SelectRect,
        Tool::SelectEllipse,
        Tool::Lasso,
        Tool::MagicWand,
        Tool::Transform,
        Tool::Crop,
        Tool::Text,
        Tool::Pen,
        Tool::ShapeRect,
        Tool::ShapeEllipse,
        Tool::Gradient,
        Tool::Slice,
        Tool::Liquify,
    ];
}


/// Color profile for display simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorProfile {
    #[default]
    Srgb,
    AdobeRgb,
    P3,
    ProPhoto,
}

impl ColorProfile {
    pub fn label(self) -> &'static str {
        match self {
            ColorProfile::Srgb => "sRGB",
            ColorProfile::AdobeRgb => "Adobe RGB",
            ColorProfile::P3 => "Display P3",
            ColorProfile::ProPhoto => "ProPhoto RGB",
        }
    }
}

/// Soft-proof mode: simulates output gamut by converting to CMYK and back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SoftProofMode {
    #[default]
    Off,
    Cmyk,
    PrinterProfile,
}

impl SoftProofMode {
    pub fn label(self) -> &'static str {
        match self {
            SoftProofMode::Off => "Off",
            SoftProofMode::Cmyk => "CMYK",
            SoftProofMode::PrinterProfile => "Printer Profile",
        }
    }
    pub fn next(self) -> Self {
        match self {
            SoftProofMode::Off => SoftProofMode::Cmyk,
            SoftProofMode::Cmyk => SoftProofMode::PrinterProfile,
            SoftProofMode::PrinterProfile => SoftProofMode::Off,
        }
    }
}


/// Histogram display channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HistogramChannel {
    #[default]
    Luminosity,
    Rgb,
    Red,
    Green,
    Blue,
}

impl HistogramChannel {
    pub fn label(self) -> &'static str {
        match self {
            HistogramChannel::Luminosity => "Luma",
            HistogramChannel::Rgb => "RGB",
            HistogramChannel::Red => "R",
            HistogramChannel::Green => "G",
            HistogramChannel::Blue => "B",
        }
    }
    pub fn next(self) -> Self {
        match self {
            HistogramChannel::Luminosity => HistogramChannel::Rgb,
            HistogramChannel::Rgb => HistogramChannel::Red,
            HistogramChannel::Red => HistogramChannel::Green,
            HistogramChannel::Green => HistogramChannel::Blue,
            HistogramChannel::Blue => HistogramChannel::Luminosity,
        }
    }
}

/// Color mode for the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Rgb,
    Cmyk,
    Hsl,
    Lab,
}


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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_label() {
        assert_eq!(Tool::Brush.label(), "Brush");
        assert_eq!(Tool::Eraser.label(), "Eraser");
        assert_eq!(Tool::Liquify.label(), "Liqfy");
    }

    #[test]
    fn test_tool_all_count() {
        assert_eq!(Tool::ALL.len(), 24);
    }

    #[test]
    fn test_color_profile_label() {
        assert_eq!(ColorProfile::Srgb.label(), "sRGB");
        assert_eq!(ColorProfile::AdobeRgb.label(), "Adobe RGB");
        assert_eq!(ColorProfile::P3.label(), "Display P3");
    }

    #[test]
    fn test_soft_proof_mode_cycle() {
        assert_eq!(SoftProofMode::Off.next(), SoftProofMode::Cmyk);
        assert_eq!(SoftProofMode::Cmyk.next(), SoftProofMode::PrinterProfile);
        assert_eq!(SoftProofMode::PrinterProfile.next(), SoftProofMode::Off);
    }

    #[test]
    fn test_histogram_channel_cycle() {
        assert_eq!(HistogramChannel::Luminosity.next(), HistogramChannel::Rgb);
        assert_eq!(HistogramChannel::Blue.next(), HistogramChannel::Luminosity);
    }

    #[test]
    fn test_histogram_channel_label() {
        assert_eq!(HistogramChannel::Luminosity.label(), "Luma");
        assert_eq!(HistogramChannel::Red.label(), "R");
    }

    #[test]
    fn test_set_color_mode() {
        let mut app = App::new();
        app.apply(Action::SetColorMode(ColorMode::Cmyk));
        assert_eq!(app.color_mode, ColorMode::Cmyk);
    }

    #[test]
    fn test_set_color_profile() {
        let mut app = App::new();
        app.apply(Action::SetColorProfile(ColorProfile::P3));
        assert_eq!(app.color_profile, ColorProfile::P3);
    }

    #[test]
    fn test_toggle_grid_snap() {
        let mut app = App::new();
        let before = app.snap_to_grid;
        app.apply(Action::ToggleGridSnap);
        assert_eq!(app.snap_to_grid, !before);
    }

    #[test]
    fn test_add_remove_guide() {
        let mut app = App::new();
        app.apply(Action::AddGuideH(100.0));
        assert_eq!(app.guides_h.len(), 1);
        app.apply(Action::RemoveGuide { horizontal: true, idx: 0 });
        assert_eq!(app.guides_h.len(), 0);
    }
}
