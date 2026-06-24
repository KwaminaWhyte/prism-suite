//! HSL **secondary curve** grading (hue-vs-hue, hue-vs-sat, hue-vs-luma) plus
//! `.3dl` LUT import and `.cube` LUT export.
//!
//! Lumetri's HSL secondary panel offers three curve families keyed on the
//! pixel's *hue*: shift the hue (hue-vs-hue), scale saturation (hue-vs-sat), or
//! scale luma (hue-vs-luma). Each is a control-point curve `hue → adjustment`
//! over the `[0,1]` hue wheel; the curve wraps (hue 0 ≡ hue 1) so a point near
//! either edge influences both. [`HslCurves::apply`] converts a straight-sRGB
//! pixel to HSL, looks up each curve at the pixel's hue, applies the three
//! adjustments, and converts back.
//!
//! This file also owns the `.3dl` LUT *parser* (Autodesk/Lustre 3-column integer
//! cube) and the `.cube` LUT *serializer* — the import twin of the `.cube`
//! parser in `super::parse_cube_lut`, and the export twin so a graded LUT can be
//! round-tripped out. All logic here is pure and unit-tested.

use super::{App, Action, rgb_to_hsl, hsl_to_rgb};

/// Wrapping linear interpolation of a hue-keyed curve. `pts` are `[hue, value]`
/// with `hue ∈ [0,1]`; the curve wraps so a query past the last point blends
/// back to the first (+1.0 in hue space). An empty curve returns `default`.
fn eval_wrapping(pts: &[[f32; 2]], hue: f32, default: f32) -> f32 {
    if pts.is_empty() {
        return default;
    }
    if pts.len() == 1 {
        return pts[0][1];
    }
    let hue = hue.rem_euclid(1.0);
    // Find the bracketing segment, treating the list as circular.
    for w in pts.windows(2) {
        if hue >= w[0][0] && hue <= w[1][0] {
            let span = (w[1][0] - w[0][0]).max(1e-9);
            let f = (hue - w[0][0]) / span;
            return w[0][1] + f * (w[1][1] - w[0][1]);
        }
    }
    // Wrap segment: between the last point and the first point + 1.0.
    let last = pts[pts.len() - 1];
    let first = pts[0];
    let span = (1.0 - last[0] + first[0]).max(1e-9);
    let h2 = if hue < first[0] { hue + 1.0 } else { hue };
    let f = (h2 - last[0]) / span;
    last[1] + f * (first[1] - last[1])
}

/// HSL secondary curves: three hue-keyed control-point curves.
#[derive(Clone, Debug, PartialEq)]
pub struct HslCurves {
    /// hue → hue *shift* in `[-0.5, 0.5]` turns (0 = no shift).
    pub hue_vs_hue: Vec<[f32; 2]>,
    /// hue → saturation *multiplier* (1.0 = unchanged).
    pub hue_vs_sat: Vec<[f32; 2]>,
    /// hue → luma *multiplier* (1.0 = unchanged).
    pub hue_vs_luma: Vec<[f32; 2]>,
}

impl Default for HslCurves {
    fn default() -> Self {
        Self {
            hue_vs_hue: vec![[0.0, 0.0], [1.0, 0.0]],
            hue_vs_sat: vec![[0.0, 1.0], [1.0, 1.0]],
            hue_vs_luma: vec![[0.0, 1.0], [1.0, 1.0]],
        }
    }
}

impl HslCurves {
    /// True when all three curves are flat at their no-op value.
    pub fn is_identity(&self) -> bool {
        let flat = |pts: &[[f32; 2]], v: f32| pts.iter().all(|p| (p[1] - v).abs() < 1e-6);
        flat(&self.hue_vs_hue, 0.0) && flat(&self.hue_vs_sat, 1.0) && flat(&self.hue_vs_luma, 1.0)
    }

    /// Apply the three curves to one straight-sRGB pixel (0..1) in place.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() {
            return;
        }
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        // Grey pixels (s≈0) have an undefined hue — leave them untouched so the
        // curve doesn't tint neutrals.
        if s < 1e-4 {
            return;
        }
        let dh = eval_wrapping(&self.hue_vs_hue, h, 0.0);
        let ds = eval_wrapping(&self.hue_vs_sat, h, 1.0);
        let dl = eval_wrapping(&self.hue_vs_luma, h, 1.0);
        let new_h = (h + dh).rem_euclid(1.0);
        let new_s = (s * ds).clamp(0.0, 1.0);
        let new_l = (l * dl).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(new_h, new_s, new_l);
        rgb[0] = r.clamp(0.0, 1.0);
        rgb[1] = g.clamp(0.0, 1.0);
        rgb[2] = b.clamp(0.0, 1.0);
    }
}

/// Parse an Autodesk `.3dl` 3D LUT. The format is a header line of mesh
/// breakpoints (integers) followed by `N^3` lines of `R G B` integers in the
/// LUT's input bit depth (the max value implies the depth — typically 1023 for
/// 10-bit or 4095 for 12-bit). Returns `(size, table)` with values normalized to
/// `[0,1]`, matching the shape `super::parse_cube_lut` returns.
pub fn parse_3dl_lut(path: &std::path::Path) -> Result<(u32, Vec<[f32; 3]>), String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);

    let mut mesh_count: Option<u32> = None;
    let mut entries: Vec<[u32; 3]> = Vec::new();
    let mut max_val: u32 = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let nums: Vec<&str> = line.split_whitespace().collect();
        // The mesh / breakpoint header is the first line with >3 integers
        // (e.g. "0 64 128 ... 1023"); the cube size is its element count.
        if mesh_count.is_none() && nums.len() > 3 {
            mesh_count = Some(nums.len() as u32);
            continue;
        }
        if nums.len() == 3 {
            let r: u32 = nums[0].parse().map_err(|_| "bad r")?;
            let g: u32 = nums[1].parse().map_err(|_| "bad g")?;
            let b: u32 = nums[2].parse().map_err(|_| "bad b")?;
            max_val = max_val.max(r).max(g).max(b);
            entries.push([r, g, b]);
        }
    }

    if entries.is_empty() {
        return Err("no LUT entries found".to_string());
    }
    // Cube size: cube root of the entry count (preferred) or the mesh header.
    let size = mesh_count.unwrap_or_else(|| {
        (entries.len() as f64).cbrt().round() as u32
    });
    let expected = (size * size * size) as usize;
    if entries.len() != expected {
        return Err(format!("expected {} entries for size {}, got {}", expected, size, entries.len()));
    }
    // Normalize against the next power-of-two-minus-one bit-depth ceiling so a
    // 1023-max table maps 1023 → 1.0, a 4095-max table 4095 → 1.0.
    let denom = if max_val <= 255 { 255.0 }
        else if max_val <= 1023 { 1023.0 }
        else if max_val <= 4095 { 4095.0 }
        else { max_val as f32 };
    // `.3dl` enumerates with blue varying fastest; the rest of the suite (and the
    // `.cube` parser) uses red-fastest. Re-index so `idx = r + g*n + b*n*n`.
    let n = size as usize;
    let mut table = vec![[0.0f32; 3]; expected];
    for (i, e) in entries.iter().enumerate() {
        // `.3dl` order: index i = r_idx*n*n + g_idx*n + b_idx (blue fastest).
        let b_idx = i % n;
        let g_idx = (i / n) % n;
        let r_idx = i / (n * n);
        let dst = r_idx + g_idx * n + b_idx * n * n;
        table[dst] = [e[0] as f32 / denom, e[1] as f32 / denom, e[2] as f32 / denom];
    }
    Ok((size, table))
}

/// Serialize a 3D LUT `(size, table)` (red-fastest, values in `[0,1]`) to
/// `.cube` text. The output round-trips through `super::parse_cube_lut`.
pub fn serialize_cube_lut(size: u32, table: &[[f32; 3]], title: &str) -> String {
    let mut out = String::new();
    if !title.is_empty() {
        out.push_str(&format!("TITLE \"{}\"\n", title));
    }
    out.push_str(&format!("LUT_3D_SIZE {}\n", size));
    out.push_str("DOMAIN_MIN 0.0 0.0 0.0\n");
    out.push_str("DOMAIN_MAX 1.0 1.0 1.0\n");
    for px in table {
        out.push_str(&format!("{:.6} {:.6} {:.6}\n", px[0], px[1], px[2]));
    }
    out
}

pub trait AppColorCurvesExt {
    fn apply_color_curves(&mut self, action: Action);
}

impl AppColorCurvesExt for App {
    fn apply_color_curves(&mut self, action: Action) {
        match action {
            Action::SetHslHueVsHue(curve) => { self.hsl_curves.hue_vs_hue = curve; self.host.mark_dirty(); }
            Action::SetHslHueVsSat(curve) => { self.hsl_curves.hue_vs_sat = curve; self.host.mark_dirty(); }
            Action::SetHslHueVsLuma(curve) => { self.hsl_curves.hue_vs_luma = curve; self.host.mark_dirty(); }
            Action::ResetHslCurves => { self.hsl_curves = HslCurves::default(); self.host.mark_dirty(); }
            Action::Load3dlLut { path } => {
                match parse_3dl_lut(&path) {
                    Ok((size, table)) => {
                        self.lut_path = Some(path);
                        self.lut_table = Some((size, table));
                        self.host.mark_dirty();
                    }
                    Err(e) => log::warn!("reel-gpui: .3dl LUT load failed: {e}"),
                }
            }
            Action::ExportCubeLut { path } => {
                if let Some((size, table)) = &self.lut_table {
                    let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("LUT");
                    let text = serialize_cube_lut(*size, table, title);
                    if let Err(e) = std::fs::write(&path, text) {
                        log::warn!("reel-gpui: .cube LUT export failed: {e}");
                    } else {
                        self.last_cube_export_path = Some(path);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{App, Action};

    #[test]
    fn identity_curves_are_noop() {
        let c = HslCurves::default();
        assert!(c.is_identity());
        let mut rgb = [0.8, 0.2, 0.2];
        let before = rgb;
        c.apply(&mut rgb);
        assert_eq!(rgb, before);
    }

    #[test]
    fn hue_vs_sat_desaturates_target_hue() {
        // Pull red saturation to 0 at hue 0. A red pixel should go grey.
        let mut c = HslCurves::default();
        c.hue_vs_sat = vec![[0.0, 0.0], [1.0, 0.0]];
        let mut rgb = [1.0, 0.0, 0.0];
        c.apply(&mut rgb);
        // r≈g≈b once desaturated.
        assert!((rgb[0] - rgb[1]).abs() < 0.05 && (rgb[1] - rgb[2]).abs() < 0.05, "desaturated: {rgb:?}");
    }

    #[test]
    fn hue_vs_luma_darkens_target_hue() {
        let mut c = HslCurves::default();
        c.hue_vs_luma = vec![[0.0, 0.5], [1.0, 0.5]]; // halve luma everywhere
        let mut rgb = [0.0, 0.8, 0.0]; // green
        let before_l = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
        c.apply(&mut rgb);
        let after_l = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
        assert!(after_l < before_l, "luma reduced: {before_l} -> {after_l}");
    }

    #[test]
    fn hue_vs_hue_shifts_color() {
        // Shift every hue by +1/3 turn: red (h=0) → green (h≈1/3).
        let mut c = HslCurves::default();
        c.hue_vs_hue = vec![[0.0, 1.0 / 3.0], [1.0, 1.0 / 3.0]];
        let mut rgb = [1.0, 0.0, 0.0];
        c.apply(&mut rgb);
        // Now the green channel should dominate.
        assert!(rgb[1] > rgb[0] && rgb[1] > rgb[2], "shifted to green: {rgb:?}");
    }

    #[test]
    fn grey_pixels_untouched_by_hsl_curves() {
        let mut c = HslCurves::default();
        c.hue_vs_sat = vec![[0.0, 2.0], [1.0, 2.0]];
        let mut rgb = [0.5, 0.5, 0.5];
        c.apply(&mut rgb);
        assert_eq!(rgb, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn eval_wrapping_blends_across_the_seam() {
        // Points at 0.1 and 0.9; query at 0.0 should blend the wrap segment.
        let pts = vec![[0.1, 0.0], [0.9, 1.0]];
        let v = eval_wrapping(&pts, 0.0, 0.0);
        assert!(v > 0.0 && v < 1.0, "wrap blend produced {v}");
    }

    #[test]
    fn cube_serialize_roundtrips_through_parser() {
        // A tiny 2^3 identity-ish LUT.
        let size = 2u32;
        let table = vec![
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0], [1.0, 1.0, 0.0],
            [0.0, 0.0, 1.0], [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0], [1.0, 1.0, 1.0],
        ];
        let text = serialize_cube_lut(size, &table, "Test");
        assert!(text.contains("LUT_3D_SIZE 2"));
        // Write + re-parse via the shared .cube parser.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("reel_cube_{}.cube", std::process::id()));
        std::fs::write(&path, &text).unwrap();
        let (sz, parsed) = super::super::parse_cube_lut(&path).unwrap();
        assert_eq!(sz, size);
        assert_eq!(parsed.len(), table.len());
        for (a, b) in parsed.iter().zip(table.iter()) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-4);
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parse_3dl_reads_and_normalizes() {
        // A 2^3 .3dl with 10-bit values (max 1023). Header has the breakpoints.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("reel_3dl_{}.3dl", std::process::id()));
        let mut s = String::from("0 1023\n");
        // 8 entries, blue-fastest, all corners.
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    s.push_str(&format!("{} {} {}\n", r * 1023, g * 1023, b * 1023));
                }
            }
        }
        std::fs::write(&path, &s).unwrap();
        let (size, table) = parse_3dl_lut(&path).unwrap();
        assert_eq!(size, 2);
        assert_eq!(table.len(), 8);
        // Values normalized to 0/1.
        for px in &table {
            for &c in px {
                assert!((c - 0.0).abs() < 1e-4 || (c - 1.0).abs() < 1e-4, "normalized 0/1, got {c}");
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_hsl_curve_actions_mark_state() {
        let mut app = App::new();
        assert!(app.hsl_curves.is_identity());
        app.apply(Action::SetHslHueVsSat(vec![[0.0, 0.0], [1.0, 0.0]]));
        assert!(!app.hsl_curves.is_identity());
        app.apply(Action::ResetHslCurves);
        assert!(app.hsl_curves.is_identity());
    }

    #[test]
    fn export_cube_action_writes_file() {
        let mut app = App::new();
        app.lut_table = Some((2, vec![[0.0; 3]; 8]));
        let dir = std::env::temp_dir();
        let path = dir.join(format!("reel_exp_{}.cube", std::process::id()));
        app.apply(Action::ExportCubeLut { path: path.clone() });
        assert_eq!(app.last_cube_export_path, Some(path.clone()));
        let contents = std::fs::read_to_string(&path).expect("cube written");
        assert!(contents.contains("LUT_3D_SIZE 2"));
        let _ = std::fs::remove_file(&path);
    }
}
