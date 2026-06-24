//! App-local color management: color-mode model + sRGB↔CMYK and sRGB↔Lab
//! conversions, plus working-space assign/convert metadata.
//!
//! `prism-color` owns only the sRGB↔linear transfer curve and is deliberately
//! NOT modified here — all the CMYK / Lab math lives locally. Conversions are
//! straightforward, deterministic float math (naive CMYK; D65 CIE Lab via the
//! sRGB→XYZ matrix), good enough for an app-local model and fully unit testable.

use super::{App, Action};

/// The document's pixel color model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WorkingColorMode {
    #[default]
    Rgb,
    Grayscale,
    Cmyk,
    Lab,
}

impl WorkingColorMode {
    pub fn label(self) -> &'static str {
        match self {
            WorkingColorMode::Rgb => "RGB",
            WorkingColorMode::Grayscale => "Grayscale",
            WorkingColorMode::Cmyk => "CMYK",
            WorkingColorMode::Lab => "Lab",
        }
    }

    /// Channel count for the mode (Grayscale = 1, RGB/Lab = 3, CMYK = 4).
    pub fn channels(self) -> usize {
        match self {
            WorkingColorMode::Grayscale => 1,
            WorkingColorMode::Rgb | WorkingColorMode::Lab => 3,
            WorkingColorMode::Cmyk => 4,
        }
    }
}

/// Named working spaces tracked as metadata (no ICC transform applied — this is
/// an app-local profile assignment model).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum WorkingSpace {
    #[default]
    SRgb,
    AdobeRgb,
    DisplayP3,
    ProPhotoRgb,
    /// A US web-coated SWOP-style CMYK profile name.
    UsWebCoatedSwop,
    GrayGamma22,
    Custom(String),
}

impl WorkingSpace {
    pub fn label(&self) -> String {
        match self {
            WorkingSpace::SRgb => "sRGB IEC61966-2.1".to_string(),
            WorkingSpace::AdobeRgb => "Adobe RGB (1998)".to_string(),
            WorkingSpace::DisplayP3 => "Display P3".to_string(),
            WorkingSpace::ProPhotoRgb => "ProPhoto RGB".to_string(),
            WorkingSpace::UsWebCoatedSwop => "U.S. Web Coated (SWOP) v2".to_string(),
            WorkingSpace::GrayGamma22 => "Gray Gamma 2.2".to_string(),
            WorkingSpace::Custom(s) => s.clone(),
        }
    }
}

/// The document's color-management state.
#[derive(Clone, Debug, Default)]
pub struct ColorManagement {
    pub mode: WorkingColorMode,
    pub working_space: WorkingSpace,
    /// Whether the document's profile is embedded on export.
    pub embed_profile: bool,
}

impl ColorManagement {
    /// "Assign Profile": change the working-space tag without converting pixels.
    pub fn assign_space(&mut self, space: WorkingSpace) {
        self.working_space = space;
    }

    /// "Convert to Profile": in this model we change both the declared mode and
    /// space (pixel conversion is performed by the caller per-pixel using the
    /// free functions below).
    pub fn convert_to(&mut self, mode: WorkingColorMode, space: WorkingSpace) {
        self.mode = mode;
        self.working_space = space;
    }
}

// ---- sRGB ↔ CMYK ------------------------------------------------------------

/// Convert straight sRGB (0..1 each) to naive CMYK (0..1 each). Uses the standard
/// GCR-free formula: `K = 1 - max(r,g,b)`, then the chromatic channels relative
/// to `1-K`. Black (0,0,0) maps to pure K; white maps to all-zero.
pub fn srgb_to_cmyk(r: f32, g: f32, b: f32) -> [f32; 4] {
    let (r, g, b) = (r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
    let k = 1.0 - r.max(g).max(b);
    if k >= 1.0 - 1e-6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = 1.0 - k;
    let c = (1.0 - r - k) / inv;
    let m = (1.0 - g - k) / inv;
    let y = (1.0 - b - k) / inv;
    [
        c.clamp(0.0, 1.0),
        m.clamp(0.0, 1.0),
        y.clamp(0.0, 1.0),
        k.clamp(0.0, 1.0),
    ]
}

/// Convert naive CMYK (0..1 each) back to straight sRGB (0..1 each).
pub fn cmyk_to_srgb(c: f32, m: f32, y: f32, k: f32) -> [f32; 3] {
    let (c, m, y, k) = (
        c.clamp(0.0, 1.0),
        m.clamp(0.0, 1.0),
        y.clamp(0.0, 1.0),
        k.clamp(0.0, 1.0),
    );
    let inv = 1.0 - k;
    [
        ((1.0 - c) * inv).clamp(0.0, 1.0),
        ((1.0 - m) * inv).clamp(0.0, 1.0),
        ((1.0 - y) * inv).clamp(0.0, 1.0),
    ]
}

// ---- sRGB ↔ CIE Lab (D65) ---------------------------------------------------

const D65_XN: f32 = 0.95047;
const D65_YN: f32 = 1.0;
const D65_ZN: f32 = 1.08883;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn lab_f(t: f32) -> f32 {
    const DELTA: f32 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        t.cbrt()
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

fn lab_f_inv(t: f32) -> f32 {
    const DELTA: f32 = 6.0 / 29.0;
    if t > DELTA {
        t * t * t
    } else {
        3.0 * DELTA * DELTA * (t - 4.0 / 29.0)
    }
}

/// Convert straight sRGB (0..1) to CIE Lab. Returns `[L (0..100), a, b]`.
pub fn srgb_to_lab(r: f32, g: f32, b: f32) -> [f32; 3] {
    let rl = srgb_to_linear(r.clamp(0.0, 1.0));
    let gl = srgb_to_linear(g.clamp(0.0, 1.0));
    let bl = srgb_to_linear(b.clamp(0.0, 1.0));
    // Linear sRGB → XYZ (D65), sRGB primaries matrix.
    let x = 0.4124564 * rl + 0.3575761 * gl + 0.1804375 * bl;
    let y = 0.2126729 * rl + 0.7151522 * gl + 0.0721750 * bl;
    let z = 0.0193339 * rl + 0.1191920 * gl + 0.9503041 * bl;
    let fx = lab_f(x / D65_XN);
    let fy = lab_f(y / D65_YN);
    let fz = lab_f(z / D65_ZN);
    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let bb = 200.0 * (fy - fz);
    [l, a, bb]
}

/// Convert CIE Lab (`L` 0..100, `a`, `b`) back to straight sRGB (0..1).
pub fn lab_to_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let x = D65_XN * lab_f_inv(fx);
    let y = D65_YN * lab_f_inv(fy);
    let z = D65_ZN * lab_f_inv(fz);
    // XYZ → linear sRGB.
    let rl = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let gl = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
    let bl = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
    [
        linear_to_srgb(rl.clamp(0.0, 1.0)),
        linear_to_srgb(gl.clamp(0.0, 1.0)),
        linear_to_srgb(bl.clamp(0.0, 1.0)),
    ]
}

/// Rec.709 luma of straight-sRGB color — used for the Grayscale conversion.
pub fn srgb_luma(r: f32, g: f32, b: f32) -> f32 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0)
}

impl App {
    /// Dispatch for the color-management domain actions.
    pub(super) fn apply_color_management(&mut self, action: Action) {
        match action {
            Action::SetWorkingColorMode(mode) => {
                self.color_management.mode = mode;
                self.status_message = Some(format!("Color mode: {}", mode.label()));
            }
            Action::AssignWorkingSpace(space) => {
                let label = space.label();
                self.color_management.assign_space(space);
                self.status_message = Some(format!("Assigned profile: {label}"));
            }
            Action::ConvertWorkingSpace { mode, space } => {
                let label = space.label();
                self.color_management.convert_to(mode, space);
                self.status_message =
                    Some(format!("Converted to {}: {label}", mode.label()));
            }
            Action::SetEmbedColorProfile(b) => {
                self.color_management.embed_profile = b;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close3(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < tol)
    }

    #[test]
    fn cmyk_roundtrip_within_tolerance() {
        let samples = [
            [0.2, 0.5, 0.8],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.7, 0.1, 0.4],
            [0.33, 0.66, 0.99],
        ];
        for s in samples {
            let cmyk = srgb_to_cmyk(s[0], s[1], s[2]);
            let back = cmyk_to_srgb(cmyk[0], cmyk[1], cmyk[2], cmyk[3]);
            assert!(
                close3(s, back, 1e-4),
                "rgb {s:?} → cmyk {cmyk:?} → rgb {back:?}"
            );
        }
    }

    #[test]
    fn cmyk_black_maps_to_k() {
        let cmyk = srgb_to_cmyk(0.0, 0.0, 0.0);
        assert_eq!(cmyk, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn cmyk_pure_red() {
        // Pure red: no black, full magenta + yellow.
        let cmyk = srgb_to_cmyk(1.0, 0.0, 0.0);
        assert!((cmyk[3]).abs() < 1e-5); // K = 0
        assert!((cmyk[1] - 1.0).abs() < 1e-5); // M = 1
        assert!((cmyk[2] - 1.0).abs() < 1e-5); // Y = 1
    }

    #[test]
    fn lab_roundtrip_within_tolerance() {
        let samples = [
            [0.2, 0.5, 0.8],
            [1.0, 1.0, 1.0],
            [0.5, 0.5, 0.5],
            [0.9, 0.2, 0.3],
        ];
        for s in samples {
            let lab = srgb_to_lab(s[0], s[1], s[2]);
            let back = lab_to_srgb(lab[0], lab[1], lab[2]);
            assert!(close3(s, back, 2e-3), "rgb {s:?} → lab {lab:?} → {back:?}");
        }
    }

    #[test]
    fn lab_white_is_l100() {
        let lab = srgb_to_lab(1.0, 1.0, 1.0);
        assert!((lab[0] - 100.0).abs() < 0.2);
        assert!(lab[1].abs() < 0.5);
        assert!(lab[2].abs() < 0.5);
    }

    #[test]
    fn lab_black_is_l0() {
        let lab = srgb_to_lab(0.0, 0.0, 0.0);
        assert!(lab[0].abs() < 1e-3);
    }

    #[test]
    fn grayscale_luma() {
        assert!((srgb_luma(1.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!(srgb_luma(0.0, 0.0, 0.0).abs() < 1e-6);
        // Green dominates Rec.709 luma.
        assert!(srgb_luma(0.0, 1.0, 0.0) > srgb_luma(1.0, 0.0, 0.0));
    }

    #[test]
    fn assign_does_not_change_mode() {
        let mut cm = ColorManagement::default();
        assert_eq!(cm.mode, WorkingColorMode::Rgb);
        cm.assign_space(WorkingSpace::AdobeRgb);
        assert_eq!(cm.mode, WorkingColorMode::Rgb);
        assert_eq!(cm.working_space, WorkingSpace::AdobeRgb);
    }

    #[test]
    fn convert_changes_mode_and_space() {
        let mut cm = ColorManagement::default();
        cm.convert_to(WorkingColorMode::Cmyk, WorkingSpace::UsWebCoatedSwop);
        assert_eq!(cm.mode, WorkingColorMode::Cmyk);
        assert_eq!(cm.working_space, WorkingSpace::UsWebCoatedSwop);
        assert_eq!(cm.mode.channels(), 4);
    }

    #[test]
    fn apply_color_mode_action() {
        let mut app = App::new();
        app.apply(Action::SetWorkingColorMode(WorkingColorMode::Lab));
        assert_eq!(app.color_management.mode, WorkingColorMode::Lab);
        app.apply(Action::AssignWorkingSpace(WorkingSpace::DisplayP3));
        assert_eq!(app.color_management.working_space, WorkingSpace::DisplayP3);
    }
}
