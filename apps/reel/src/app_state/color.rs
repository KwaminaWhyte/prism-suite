use super::{App, Action, parse_cube_lut};

/// 3-way color wheels: Lift, Gamma, Gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorWheels {
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
}

impl Default for ColorWheels {
    fn default() -> Self {
        Self { lift: [0.0, 0.0, 0.0], gamma: [1.0, 1.0, 1.0], gain: [1.0, 1.0, 1.0] }
    }
}

impl ColorWheels {
    pub fn is_identity(&self) -> bool {
        self.lift == [0.0, 0.0, 0.0] && self.gamma == [1.0, 1.0, 1.0] && self.gain == [1.0, 1.0, 1.0]
    }

    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() { return; }
        for c in 0..3 {
            let v = rgb[c].clamp(0.0, 1.0);
            let g = self.gamma[c].max(0.01);
            let out = self.gain[c] * v.powf(1.0 / g) + self.lift[c];
            rgb[c] = out.clamp(0.0, 1.0);
        }
    }
}

/// Per-channel RGB tone curves.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbCurves {
    pub master: Vec<[f32; 2]>,
    pub red:    Vec<[f32; 2]>,
    pub green:  Vec<[f32; 2]>,
    pub blue:   Vec<[f32; 2]>,
}

impl Default for RgbCurves {
    fn default() -> Self {
        let identity = vec![[0.0f32, 0.0], [1.0, 1.0]];
        Self { master: identity.clone(), red: identity.clone(), green: identity.clone(), blue: identity }
    }
}

impl RgbCurves {
    pub fn is_identity(&self) -> bool {
        let id: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 1.0]];
        self.master == id && self.red == id && self.green == id && self.blue == id
    }

    fn eval_curve(pts: &[[f32; 2]], x: f32) -> f32 {
        if pts.is_empty() { return x; }
        if x <= pts[0][0] { return pts[0][1]; }
        if x >= pts[pts.len()-1][0] { return pts[pts.len()-1][1]; }
        for w in pts.windows(2) {
            if x <= w[1][0] {
                let t = (x - w[0][0]) / (w[1][0] - w[0][0]).max(1e-6);
                return w[0][1] + t * (w[1][1] - w[0][1]);
            }
        }
        pts[pts.len()-1][1]
    }

    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() { return; }
        let ch_curves = [&self.red, &self.green, &self.blue];
        for c in 0..3 {
            let v = Self::eval_curve(&self.master, rgb[c]);
            rgb[c] = Self::eval_curve(ch_curves[c], v).clamp(0.0, 1.0);
        }
    }
}

/// Lumetri Color panel tabs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LumetriPanel {
    #[default] Basic, Creative, Curves, ColorWheels, HslSecondary, Vignette,
}

/// Color wheel mode.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ColorWheelMode {
    #[default] ThreeWay, Compare,
}

/// Full Lumetri Color configuration.
#[derive(Debug, Clone, Default)]
pub struct LumetriColorConfig {
    pub active_panel: LumetriPanel,
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temp: f32,
    pub tint: f32,
    pub saturation: f32,
    pub faded_film: f32,
    pub sharpen: f32,
    pub vibrance: f32,
    pub shadow_tint: [f32; 3],
    pub highlight_tint: [f32; 3],
    pub luma_curve: Vec<[f32; 2]>,
    pub hsl_hue_range: [f32; 2],
    pub hsl_sat_range: [f32; 2],
    pub hsl_luma_range: [f32; 2],
    pub hsl_hue_shift: f32,
    pub hsl_sat_shift: f32,
    pub hsl_luma_shift: f32,
    pub vignette_amount: f32,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,
}

impl LumetriColorConfig {
    pub fn new() -> Self {
        let mut cfg = Self::default();
        cfg.luma_curve = vec![[0.0, 0.0], [0.25, 0.25], [0.5, 0.5], [0.75, 0.75], [1.0, 1.0]];
        cfg.hsl_hue_range = [0.0, 360.0];
        cfg.hsl_sat_range = [0.0, 100.0];
        cfg.hsl_luma_range = [0.0, 100.0];
        cfg.vignette_midpoint = 50.0;
        cfg.vignette_feather = 50.0;
        cfg
    }
}

/// Marker kind on the master timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkerKind { InPoint, OutPoint, Chapter, Comment }

impl MarkerKind {
    pub fn label(&self) -> &'static str {
        match self {
            MarkerKind::InPoint => "In",
            MarkerKind::OutPoint => "Out",
            MarkerKind::Chapter => "Chapter",
            MarkerKind::Comment => "Comment",
        }
    }
}

/// A timeline marker.
#[derive(Clone, Debug)]
pub struct GpuiMarker {
    pub time_secs: f32,
    pub name: String,
    pub color: [f32; 3],
    pub kind: MarkerKind,
}

/// Color space tag for the sequence output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace { Rec709, Rec2020, SRGB }

impl Default for ColorSpace { fn default() -> Self { ColorSpace::Rec709 } }

/// Display color space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DisplayColorSpace {
    #[default] Rec709, Rec2020, P3D65, P3DCI,
    #[allow(non_camel_case_types)] sRGB,
}

/// Working color space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WorkingColorSpace {
    #[default] Rec709, Rec2020Hlg, Rec2020Pq, LogC, SLog3,
}

/// Color management configuration.
#[derive(Debug, Clone, Default)]
pub struct ColorManagementConfig {
    pub enabled: bool,
    pub display_space: DisplayColorSpace,
    pub working_space: WorkingColorSpace,
    pub output_lut_path: Option<std::path::PathBuf>,
    pub hdr_output: bool,
    pub max_luminance_nits: f32,
    pub apply_lut_on_export: bool,
}

impl ColorManagementConfig {
    pub fn new() -> Self {
        Self { max_luminance_nits: 1000.0, ..Default::default() }
    }
}

pub trait AppColorExt {
    fn apply_color(&mut self, action: Action);
}

impl AppColorExt for App {
    fn apply_color(&mut self, action: Action) {
        match action {
            Action::SetColorWheels(cw) => {
                self.color_wheels = cw;
                self.host.mark_dirty();
            }
            Action::SetRgbCurves(c) => { self.rgb_curves = c; self.host.mark_dirty(); }
            Action::SetCurvesChannel(ch) => { self.curves_channel = ch; }
            Action::AddCurvePoint { channel, point } => {
                let target = match channel {
                    1 => &mut self.rgb_curves.red,
                    2 => &mut self.rgb_curves.green,
                    3 => &mut self.rgb_curves.blue,
                    _ => &mut self.rgb_curves.master,
                };
                target.push(point);
                target.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
                self.host.mark_dirty();
            }
            Action::MoveCurvePoint { channel, index, point } => {
                let target = match channel {
                    1 => &mut self.rgb_curves.red,
                    2 => &mut self.rgb_curves.green,
                    3 => &mut self.rgb_curves.blue,
                    _ => &mut self.rgb_curves.master,
                };
                if index < target.len() {
                    target[index] = point;
                    target.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::LoadLut { path } => {
                match parse_cube_lut(&path) {
                    Ok((size, table)) => {
                        self.lut_path = Some(path);
                        self.lut_table = Some((size, table));
                        self.host.mark_dirty();
                    }
                    Err(e) => { log::warn!("reel-gpui: LUT load failed: {e}"); }
                }
            }
            Action::SetWhiteBalance { temp, tint } => {
                self.white_balance_temp = temp.clamp(2000.0, 10000.0);
                self.white_balance_tint = tint.clamp(-150.0, 150.0);
                self.host.mark_dirty();
            }
            Action::ToggleSequenceSettings => { self.show_sequence_settings = !self.show_sequence_settings; }
            Action::SetColorSpace(cs) => { self.sequence_color_space = cs; }
            Action::ToggleMarkersPanel => { self.show_markers_panel = !self.show_markers_panel; }
            Action::AddMarkerAt { time, name, kind } => {
                let color = match kind {
                    MarkerKind::InPoint  => [0.2f32, 0.8, 0.2],
                    MarkerKind::OutPoint => [0.8, 0.2, 0.2],
                    MarkerKind::Chapter  => [0.9, 0.7, 0.0],
                    MarkerKind::Comment  => [0.3, 0.6, 1.0],
                };
                self.gpui_markers.push(GpuiMarker { time_secs: time, name, color, kind });
                self.gpui_markers.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap_or(std::cmp::Ordering::Equal));
            }
            Action::RemoveMarker(idx) => {
                if idx < self.gpui_markers.len() { self.gpui_markers.remove(idx); }
            }
            Action::RenameMarker { index, name } => {
                if let Some(m) = self.gpui_markers.get_mut(index) { m.name = name; }
            }
            Action::SetInPoint(t) => { self.work_area_in = t.max(0.0); }
            Action::SetOutPoint(t) => { self.work_area_out = t.max(0.0); }
            Action::SetLumetriPanel(p) => { self.lumetri.active_panel = p; }
            Action::SetLumetriExposure(v) => { self.lumetri.exposure = v.clamp(-5.0, 5.0); }
            Action::SetLumetriContrast(v) => { self.lumetri.contrast = v.clamp(-100.0, 100.0); }
            Action::SetLumetriHighlights(v) => { self.lumetri.highlights = v.clamp(-100.0, 100.0); }
            Action::SetLumetriShadows(v) => { self.lumetri.shadows = v.clamp(-100.0, 100.0); }
            Action::SetLumetriWhites(v) => { self.lumetri.whites = v.clamp(-100.0, 100.0); }
            Action::SetLumetriBlacks(v) => { self.lumetri.blacks = v.clamp(-100.0, 100.0); }
            Action::SetLumetriTemp(v) => { self.lumetri.temp = v.clamp(-100.0, 100.0); }
            Action::SetLumetriTint(v) => { self.lumetri.tint = v.clamp(-100.0, 100.0); }
            Action::SetLumetriSaturation(v) => { self.lumetri.saturation = v.clamp(-100.0, 100.0); }
            Action::SetLumetriFadedFilm(v) => { self.lumetri.faded_film = v.clamp(0.0, 100.0); }
            Action::SetLumetriSharpen(v) => { self.lumetri.sharpen = v.clamp(0.0, 100.0); }
            Action::SetLumetriVibrance(v) => { self.lumetri.vibrance = v.clamp(-100.0, 100.0); }
            Action::SetLumetriHslHueRange(r) => { self.lumetri.hsl_hue_range = r; }
            Action::SetLumetriHslSatRange(r) => { self.lumetri.hsl_sat_range = r; }
            Action::SetLumetriHslShifts { hue, sat, luma } => {
                self.lumetri.hsl_hue_shift = hue;
                self.lumetri.hsl_sat_shift = sat;
                self.lumetri.hsl_luma_shift = luma;
            }
            Action::SetLumetriVignette { amount, midpoint, roundness, feather } => {
                self.lumetri.vignette_amount = amount;
                self.lumetri.vignette_midpoint = midpoint;
                self.lumetri.vignette_roundness = roundness;
                self.lumetri.vignette_feather = feather;
            }
            Action::ResetLumetriPanel => {
                let fresh = LumetriColorConfig::new();
                match self.lumetri.active_panel {
                    LumetriPanel::Basic => {
                        self.lumetri.exposure = fresh.exposure;
                        self.lumetri.contrast = fresh.contrast;
                        self.lumetri.highlights = fresh.highlights;
                        self.lumetri.shadows = fresh.shadows;
                        self.lumetri.whites = fresh.whites;
                        self.lumetri.blacks = fresh.blacks;
                        self.lumetri.temp = fresh.temp;
                        self.lumetri.tint = fresh.tint;
                        self.lumetri.saturation = fresh.saturation;
                    }
                    LumetriPanel::Creative => {
                        self.lumetri.faded_film = fresh.faded_film;
                        self.lumetri.sharpen = fresh.sharpen;
                        self.lumetri.vibrance = fresh.vibrance;
                        self.lumetri.shadow_tint = fresh.shadow_tint;
                        self.lumetri.highlight_tint = fresh.highlight_tint;
                    }
                    LumetriPanel::Curves => { self.lumetri.luma_curve = fresh.luma_curve; }
                    LumetriPanel::HslSecondary => {
                        self.lumetri.hsl_hue_range = fresh.hsl_hue_range;
                        self.lumetri.hsl_sat_range = fresh.hsl_sat_range;
                        self.lumetri.hsl_luma_range = fresh.hsl_luma_range;
                        self.lumetri.hsl_hue_shift = fresh.hsl_hue_shift;
                        self.lumetri.hsl_sat_shift = fresh.hsl_sat_shift;
                        self.lumetri.hsl_luma_shift = fresh.hsl_luma_shift;
                    }
                    LumetriPanel::Vignette => {
                        self.lumetri.vignette_amount = fresh.vignette_amount;
                        self.lumetri.vignette_midpoint = fresh.vignette_midpoint;
                        self.lumetri.vignette_roundness = fresh.vignette_roundness;
                        self.lumetri.vignette_feather = fresh.vignette_feather;
                    }
                    LumetriPanel::ColorWheels => {}
                }
            }
            Action::ApplyLumetriToClip { clip_idx } => { self.lumetri_applied_clip = Some(clip_idx); }
            Action::ToggleLumetriPanel => { self.lumetri_panel_open = !self.lumetri_panel_open; }
            Action::ToggleColorManagementPanel => { self.color_management_open = !self.color_management_open; }
            Action::SetColorManagementEnabled(v) => { self.color_management.enabled = v; }
            Action::SetDisplayColorSpace(s) => { self.color_management.display_space = s; }
            Action::SetWorkingColorSpace(s) => { self.color_management.working_space = s; }
            Action::SetOutputLutPath(p) => { self.color_management.output_lut_path = p; }
            Action::SetHdrOutput(v) => { self.color_management.hdr_output = v; }
            Action::SetMaxLuminance(v) => { self.color_management.max_luminance_nits = v.clamp(100.0, 10000.0); }
            Action::SetApplyLutOnExport(v) => { self.color_management.apply_lut_on_export = v; }
            Action::ResetColorManagement => { self.color_management = ColorManagementConfig::new(); }
            _ => {}
        }
    }
}
