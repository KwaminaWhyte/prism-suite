//! **Distortion effects** — Corner Pin, Bezier Warp, Wave Warp, Roughen Edges,
//! implemented as deterministic CPU kernels with per-layer configs.
//!
//! - **Corner Pin** maps the four corners of the layer to four arbitrary
//!   destination points by solving the 3×3 projective homography that takes the
//!   unit square's corners there, then sampling the source through its inverse.
//! - **Bezier Warp** offsets each pixel by a smooth field built from per-corner
//!   control offsets, bilinearly blended — the cheap, well-behaved cousin of AE's
//!   Bezier Warp.
//! - **Wave Warp** displaces each pixel by a sinusoid of configurable
//!   amplitude / wavelength / direction / phase (the classic "flag wave").
//! - **Roughen Edges** perturbs a layer's alpha edge with value noise to fray it.
//!
//! Geometry lives in free functions so the math is unit-testable without an
//! `App`. The `App` impl holds per-layer configs + panel actions; storage is
//! app-side (engine untouched), matching `keying.rs` / `effects_chain.rs`.

use std::collections::HashMap;

use super::{App, Action};

// ── Corner Pin ────────────────────────────────────────────────────────────────

/// Per-layer **Corner Pin**: the four destination points (layer-local px,
/// row-major TL, TR, BL, BR) the layer's corners are warped to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerPinConfig {
    /// Top-left destination.
    pub tl: [f32; 2],
    /// Top-right destination.
    pub tr: [f32; 2],
    /// Bottom-left destination.
    pub bl: [f32; 2],
    /// Bottom-right destination.
    pub br: [f32; 2],
}

impl CornerPinConfig {
    /// An identity pin for a `w × h` layer (corners map to themselves).
    pub fn identity(w: f32, h: f32) -> Self {
        Self {
            tl: [0.0, 0.0],
            tr: [w, 0.0],
            bl: [0.0, h],
            br: [w, h],
        }
    }
}

/// A 3×3 row-major projective matrix.
pub type Mat3 = [[f32; 3]; 3];

/// Solve the homography that maps the unit square corners
/// `(0,0),(1,0),(0,1),(1,1)` to `(tl, tr, bl, br)`. Returns a 3×3 matrix `H`
/// with `H * [u, v, 1]ᵀ ∝ [x, y, 1]ᵀ`.
///
/// Derivation: the projective map of the unit square has 8 DOF. With
/// `x = (a u + b v + c)/(g u + h v + 1)` and likewise for `y`, the four corner
/// constraints give a closed-form solution (standard quad-to-quad).
pub fn unit_square_to_quad(tl: [f32; 2], tr: [f32; 2], bl: [f32; 2], br: [f32; 2]) -> Mat3 {
    // Corners: P00=tl, P10=tr, P01=bl, P11=br.
    let (x0, y0) = (tl[0], tl[1]);
    let (x1, y1) = (tr[0], tr[1]);
    let (x2, y2) = (bl[0], bl[1]);
    let (x3, y3) = (br[0], br[1]);

    let dx1 = x1 - x3;
    let dx2 = x2 - x3;
    let dx3 = x0 - x1 + x3 - x2;
    let dy1 = y1 - y3;
    let dy2 = y2 - y3;
    let dy3 = y0 - y1 + y3 - y2;

    let den = dx1 * dy2 - dx2 * dy1;
    let (g, h) = if den.abs() < 1e-12 {
        (0.0, 0.0)
    } else {
        ((dx3 * dy2 - dx2 * dy3) / den, (dx1 * dy3 - dx3 * dy1) / den)
    };

    let a = x1 - x0 + g * x1;
    let b = x2 - x0 + h * x2;
    let c = x0;
    let d = y1 - y0 + g * y1;
    let e = y2 - y0 + h * y2;
    let f = y0;

    [[a, b, c], [d, e, f], [g, h, 1.0]]
}

/// Apply a 3×3 projective matrix to `(u, v)` (homogeneous), returning the mapped
/// `(x, y)` after the perspective divide.
pub fn project(m: &Mat3, u: f32, v: f32) -> [f32; 2] {
    let x = m[0][0] * u + m[0][1] * v + m[0][2];
    let y = m[1][0] * u + m[1][1] * v + m[1][2];
    let w = m[2][0] * u + m[2][1] * v + m[2][2];
    let w = if w.abs() < 1e-12 { 1e-12 } else { w };
    [x / w, y / w]
}

/// Build the forward homography for a corner-pin on a `w × h` layer: maps a
/// layer-local pixel `(x, y)` (via its normalized `(u, v)`) to the warped point.
pub fn corner_pin_matrix(cfg: CornerPinConfig) -> Mat3 {
    unit_square_to_quad(cfg.tl, cfg.tr, cfg.bl, cfg.br)
}

// ── Bezier Warp ───────────────────────────────────────────────────────────────

/// Per-layer **Bezier Warp**: a per-corner displacement (px) bilinearly blended
/// across the layer. (A practical reduction of AE's 12-tangent Bezier Warp that
/// keeps the C¹ smoothness in the interior.)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BezierWarpConfig {
    /// Corner displacements TL, TR, BL, BR (each `[dx, dy]` px).
    pub tl: [f32; 2],
    pub tr: [f32; 2],
    pub bl: [f32; 2],
    pub br: [f32; 2],
}

/// The Bezier-warp displacement `(dx, dy)` at normalized `(u, v)` — bilinear
/// blend of the four corner offsets.
pub fn bezier_warp_offset(cfg: BezierWarpConfig, u: f32, v: f32) -> [f32; 2] {
    let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let lerp = |a: [f32; 2], b: [f32; 2], t: f32| {
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    };
    let top = lerp(cfg.tl, cfg.tr, u);
    let bot = lerp(cfg.bl, cfg.br, u);
    lerp(top, bot, v)
}

// ── Wave Warp ─────────────────────────────────────────────────────────────────

/// Per-layer **Wave Warp**: a travelling sinusoid that displaces pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveWarpConfig {
    /// Peak displacement in pixels.
    pub amplitude: f32,
    /// Wavelength in pixels (one full sine cycle).
    pub wavelength: f32,
    /// Direction in degrees: `0` = wave travels along +X, displacing in Y.
    pub direction_deg: f32,
    /// Phase offset in degrees (animate this for motion).
    pub phase_deg: f32,
}

impl Default for WaveWarpConfig {
    fn default() -> Self {
        Self { amplitude: 20.0, wavelength: 80.0, direction_deg: 0.0, phase_deg: 0.0 }
    }
}

/// The Wave-Warp displacement `(dx, dy)` (px) at pixel `(x, y)`. The wave runs
/// along the `direction` axis and displaces perpendicular to it.
pub fn wave_warp_offset(cfg: WaveWarpConfig, x: f32, y: f32) -> [f32; 2] {
    let wl = cfg.wavelength.max(1e-3);
    let dir = cfg.direction_deg.to_radians();
    let (dc, ds) = (dir.cos(), dir.sin());
    // Coordinate along the travel direction.
    let along = x * dc + y * ds;
    let phase = cfg.phase_deg.to_radians();
    let s = (std::f32::consts::TAU * along / wl + phase).sin() * cfg.amplitude;
    // Displace perpendicular to the travel direction.
    [-ds * s, dc * s]
}

// ── Roughen Edges ─────────────────────────────────────────────────────────────

/// Per-layer **Roughen Edges**: fray a layer's alpha edge with value noise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoughenEdgesConfig {
    /// Edge displacement strength in alpha-units (how much the noise pushes the
    /// alpha threshold around).
    pub border: f32,
    /// Spatial scale of the fraying noise (cells across the layer).
    pub scale: f32,
    /// Deterministic seed.
    pub seed: u32,
}

impl Default for RoughenEdgesConfig {
    fn default() -> Self {
        Self { border: 0.3, scale: 16.0, seed: 0 }
    }
}

/// A reproducible hash of two integer lattice coords + a seed into `[0, 1)`.
fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((x as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add((y as u32).wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    (h as f32) / (u32::MAX as f32)
}

/// Smoothstep fade for bilinear value noise.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Bilinear value noise at `(x, y)` for `seed`, in `[0, 1)`.
pub fn value_noise2(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - xi as f32, y - yi as f32);
    let (u, v) = (fade(xf), fade(yf));
    let c00 = hash2(xi, yi, seed);
    let c10 = hash2(xi + 1, yi, seed);
    let c01 = hash2(xi, yi + 1, seed);
    let c11 = hash2(xi + 1, yi + 1, seed);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(lerp(c00, c10, u), lerp(c01, c11, u), v)
}

/// Roughen one pixel's alpha: perturb it by `(noise-0.5) * border`, clamped to
/// `[0,1]`. Interior (alpha ≈ 1) and exterior (alpha ≈ 0) stay roughly put; only
/// the partially-covered edge frays. `(u, v)` are normalized layer coords.
pub fn roughen_edge_alpha(cfg: RoughenEdgesConfig, alpha: f32, u: f32, v: f32) -> f32 {
    let n = value_noise2(u * cfg.scale, v * cfg.scale, cfg.seed);
    // Edge weight peaks at the half-covered boundary, vanishes in solid regions.
    let edge = 1.0 - (2.0 * alpha - 1.0).abs();
    (alpha + (n - 0.5) * cfg.border * edge).clamp(0.0, 1.0)
}

// ── Buffer kernels (sampling) ────────────────────────────────────────────────

/// Nearest-sample an RGBA `[0,1]` buffer at integer `(x, y)`, edge-clamped.
fn sample_clamped(buf: &[f32], w: usize, h: usize, x: i32, y: i32) -> [f32; 4] {
    let xi = x.clamp(0, w as i32 - 1) as usize;
    let yi = y.clamp(0, h as i32 - 1) as usize;
    let i = (yi * w + xi) * 4;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

/// Warp an RGBA `[0,1]` buffer through a corner-pin: for each destination pixel,
/// invert the forward homography to find the source `(u, v)`, then sample.
/// Pixels whose source falls outside the unit square become transparent.
/// Deterministic.
pub fn corner_pin_buffer(input: &[f32], w: usize, h: usize, cfg: CornerPinConfig) -> Vec<f32> {
    let mut out = vec![0.0_f32; input.len()];
    if w == 0 || h == 0 {
        return out;
    }
    let m = corner_pin_matrix(cfg);
    let inv = invert3(&m);
    for py in 0..h {
        for px in 0..w {
            // Destination pixel center in layer px.
            let (dx, dy) = (px as f32 + 0.5, py as f32 + 0.5);
            // Map back to the source unit square.
            let s = project(&inv, dx, dy);
            let (u, v) = (s[0], s[1]);
            let o = (py * w + px) * 4;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let sx = (u * w as f32 - 0.5).round() as i32;
                let sy = (v * h as f32 - 0.5).round() as i32;
                let c = sample_clamped(input, w, h, sx, sy);
                out[o..o + 4].copy_from_slice(&c);
            } // else stays transparent (zeros)
        }
    }
    out
}

/// Invert a 3×3 matrix (cofactor method). Returns the identity if singular.
pub fn invert3(m: &Mat3) -> Mat3 {
    let a = m[0][0];
    let b = m[0][1];
    let c = m[0][2];
    let d = m[1][0];
    let e = m[1][1];
    let f = m[1][2];
    let g = m[2][0];
    let h = m[2][1];
    let i = m[2][2];
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let inv_det = 1.0 / det;
    [
        [
            (e * i - f * h) * inv_det,
            (c * h - b * i) * inv_det,
            (b * f - c * e) * inv_det,
        ],
        [
            (f * g - d * i) * inv_det,
            (a * i - c * g) * inv_det,
            (c * d - a * f) * inv_det,
        ],
        [
            (d * h - e * g) * inv_det,
            (b * g - a * h) * inv_det,
            (a * e - b * d) * inv_det,
        ],
    ]
}

/// Warp an RGBA buffer through a Wave Warp: each output pixel samples the input
/// offset by the wave displacement. Deterministic.
pub fn wave_warp_buffer(input: &[f32], w: usize, h: usize, cfg: WaveWarpConfig) -> Vec<f32> {
    let mut out = vec![0.0_f32; input.len()];
    if w == 0 || h == 0 {
        return out;
    }
    for py in 0..h {
        for px in 0..w {
            let off = wave_warp_offset(cfg, px as f32, py as f32);
            let sx = (px as f32 - off[0]).round() as i32;
            let sy = (py as f32 - off[1]).round() as i32;
            let c = sample_clamped(input, w, h, sx, sy);
            let o = (py * w + px) * 4;
            out[o..o + 4].copy_from_slice(&c);
        }
    }
    out
}

impl App {
    /// Apply a distortion [`Action`]. Dispatched from [`App::apply`].
    pub(super) fn apply_effects_distort(&mut self, action: Action) {
        match action {
            Action::AddCornerPin { layer_id, w, h } => {
                self.corner_pins.insert(layer_id, CornerPinConfig::identity(w, h));
                self.host.mark_dirty();
            }
            Action::SetCornerPinCorner { layer_id, corner, x, y } => {
                let cfg = self
                    .corner_pins
                    .entry(layer_id)
                    .or_insert(CornerPinConfig::identity(1.0, 1.0));
                match corner {
                    0 => cfg.tl = [x, y],
                    1 => cfg.tr = [x, y],
                    2 => cfg.bl = [x, y],
                    _ => cfg.br = [x, y],
                }
                self.host.mark_dirty();
            }
            Action::RemoveCornerPin { layer_id } => {
                self.corner_pins.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::AddBezierWarp { layer_id } => {
                self.bezier_warps.insert(layer_id, BezierWarpConfig::default());
                self.host.mark_dirty();
            }
            Action::SetBezierWarpCorner { layer_id, corner, dx, dy } => {
                let cfg = self.bezier_warps.entry(layer_id).or_default();
                match corner {
                    0 => cfg.tl = [dx, dy],
                    1 => cfg.tr = [dx, dy],
                    2 => cfg.bl = [dx, dy],
                    _ => cfg.br = [dx, dy],
                }
                self.host.mark_dirty();
            }
            Action::RemoveBezierWarp { layer_id } => {
                self.bezier_warps.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::AddWaveWarp { layer_id } => {
                self.wave_warps.insert(layer_id, WaveWarpConfig::default());
                self.host.mark_dirty();
            }
            Action::SetWaveWarpParam { layer_id, param, value } => {
                let cfg = self.wave_warps.entry(layer_id).or_default();
                match param {
                    "amplitude" => cfg.amplitude = value,
                    "wavelength" => cfg.wavelength = value.max(1.0),
                    "direction" => cfg.direction_deg = value,
                    "phase" => cfg.phase_deg = value,
                    _ => {}
                }
                self.host.mark_dirty();
            }
            Action::RemoveWaveWarp { layer_id } => {
                self.wave_warps.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::AddRoughenEdges { layer_id } => {
                self.roughen_edges.insert(layer_id, RoughenEdgesConfig::default());
                self.host.mark_dirty();
            }
            Action::SetRoughenEdgesParam { layer_id, param, value } => {
                let cfg = self.roughen_edges.entry(layer_id).or_default();
                match param {
                    "border" => cfg.border = value.max(0.0),
                    "scale" => cfg.scale = value.max(0.01),
                    _ => {}
                }
                self.host.mark_dirty();
            }
            Action::SetRoughenEdgesSeed { layer_id, seed } => {
                self.roughen_edges.entry(layer_id).or_default().seed = seed;
                self.host.mark_dirty();
            }
            Action::RemoveRoughenEdges { layer_id } => {
                self.roughen_edges.remove(&layer_id);
                self.host.mark_dirty();
            }
            _ => unreachable!("apply_effects_distort called with wrong action"),
        }
    }

    /// Warp a layer's RGBA buffer through its corner-pin (if any).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_corner_pin(&self, layer_id: usize, rgba: &[f32], w: usize, h: usize) -> Option<Vec<f32>> {
        let cfg = self.corner_pins.get(&layer_id)?;
        Some(corner_pin_buffer(rgba, w, h, *cfg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_pin_maps_corners_exactly() {
        // A non-trivial quad: the homography must send the unit-square corners to
        // the configured destinations exactly.
        let cfg = CornerPinConfig {
            tl: [10.0, 20.0],
            tr: [200.0, 5.0],
            bl: [0.0, 180.0],
            br: [220.0, 210.0],
        };
        let m = corner_pin_matrix(cfg);
        let p00 = project(&m, 0.0, 0.0);
        let p10 = project(&m, 1.0, 0.0);
        let p01 = project(&m, 0.0, 1.0);
        let p11 = project(&m, 1.0, 1.0);
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
        assert!(close(p00, cfg.tl), "TL maps exactly, got {p00:?}");
        assert!(close(p10, cfg.tr), "TR maps exactly, got {p10:?}");
        assert!(close(p01, cfg.bl), "BL maps exactly, got {p01:?}");
        assert!(close(p11, cfg.br), "BR maps exactly, got {p11:?}");
    }

    #[test]
    fn corner_pin_identity_is_affine_passthrough() {
        let cfg = CornerPinConfig::identity(100.0, 50.0);
        let m = corner_pin_matrix(cfg);
        // Center of the unit square maps to the center of the layer.
        let c = project(&m, 0.5, 0.5);
        assert!((c[0] - 50.0).abs() < 1e-3 && (c[1] - 25.0).abs() < 1e-3, "center maps, got {c:?}");
    }

    #[test]
    fn invert3_round_trips() {
        let cfg = CornerPinConfig {
            tl: [10.0, 20.0],
            tr: [200.0, 5.0],
            bl: [0.0, 180.0],
            br: [220.0, 210.0],
        };
        let m = corner_pin_matrix(cfg);
        let inv = invert3(&m);
        // Forward then inverse returns the original point.
        let p = project(&m, 0.3, 0.7);
        let back = project(&inv, p[0], p[1]);
        assert!((back[0] - 0.3).abs() < 1e-3 && (back[1] - 0.7).abs() < 1e-3, "round trip {back:?}");
    }

    #[test]
    fn corner_pin_buffer_identity_preserves_center() {
        // An identity pin leaves an interior pixel's color where it was.
        let (w, h) = (8, 8);
        let mut input = vec![0.0_f32; w * h * 4];
        let idx = (3 * w + 4) * 4;
        input[idx] = 1.0;
        input[idx + 3] = 1.0; // a lone red opaque pixel
        let cfg = CornerPinConfig::identity(w as f32, h as f32);
        let out = corner_pin_buffer(&input, w, h, cfg);
        assert!((out[idx] - 1.0).abs() < 1e-4, "identity keeps the pixel in place");
        assert!((out[idx + 3] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn bezier_warp_corner_offsets() {
        let cfg = BezierWarpConfig {
            tl: [1.0, 2.0],
            tr: [3.0, 4.0],
            bl: [5.0, 6.0],
            br: [7.0, 8.0],
        };
        assert_eq!(bezier_warp_offset(cfg, 0.0, 0.0), [1.0, 2.0]);
        assert_eq!(bezier_warp_offset(cfg, 1.0, 0.0), [3.0, 4.0]);
        assert_eq!(bezier_warp_offset(cfg, 0.0, 1.0), [5.0, 6.0]);
        assert_eq!(bezier_warp_offset(cfg, 1.0, 1.0), [7.0, 8.0]);
        // Center is the mean.
        let mid = bezier_warp_offset(cfg, 0.5, 0.5);
        assert!((mid[0] - 4.0).abs() < 1e-5 && (mid[1] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn bezier_warp_zero_is_identity() {
        let cfg = BezierWarpConfig::default();
        assert_eq!(bezier_warp_offset(cfg, 0.42, 0.91), [0.0, 0.0]);
    }

    #[test]
    fn wave_warp_amplitude_bound() {
        let cfg = WaveWarpConfig { amplitude: 15.0, wavelength: 40.0, direction_deg: 0.0, phase_deg: 0.0 };
        for x in 0..100 {
            let off = wave_warp_offset(cfg, x as f32, 7.0);
            let mag = (off[0] * off[0] + off[1] * off[1]).sqrt();
            assert!(mag <= 15.0 + 1e-3, "|disp| <= amplitude, got {mag}");
        }
    }

    #[test]
    fn wave_warp_quarter_phase_peaks() {
        // At a quarter wavelength the sine is 1 ⇒ full amplitude displacement.
        let cfg = WaveWarpConfig { amplitude: 10.0, wavelength: 40.0, direction_deg: 0.0, phase_deg: 0.0 };
        let off = wave_warp_offset(cfg, 10.0, 0.0); // along=10 = wl/4
        // direction 0 ⇒ displacement in +Y.
        assert!((off[1] - 10.0).abs() < 1e-3, "peak displacement, got {off:?}");
        assert!(off[0].abs() < 1e-3, "no X displacement for direction 0");
    }

    #[test]
    fn wave_warp_zero_amplitude_is_identity() {
        let cfg = WaveWarpConfig { amplitude: 0.0, ..Default::default() };
        let off = wave_warp_offset(cfg, 33.0, 12.0);
        assert!(off[0].abs() < 1e-6 && off[1].abs() < 1e-6);
    }

    #[test]
    fn wave_warp_buffer_warps() {
        let (w, h) = (32, 32);
        // Vertical gradient so a Y displacement changes values.
        let mut input = vec![0.0_f32; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                input[i] = y as f32 / h as f32;
                input[i + 3] = 1.0;
            }
        }
        let cfg = WaveWarpConfig { amplitude: 8.0, wavelength: 16.0, direction_deg: 0.0, phase_deg: 0.0 };
        let out = wave_warp_buffer(&input, w, h, cfg);
        assert_ne!(out, input, "wave warp changes the buffer");
    }

    #[test]
    fn roughen_edges_only_frays_the_edge() {
        let cfg = RoughenEdgesConfig { border: 0.5, scale: 8.0, seed: 3 };
        // Solid interior unchanged; edge (alpha 0.5) perturbed.
        assert!((roughen_edge_alpha(cfg, 1.0, 0.5, 0.5) - 1.0).abs() < 1e-6, "solid stays solid");
        assert!((roughen_edge_alpha(cfg, 0.0, 0.5, 0.5) - 0.0).abs() < 1e-6, "empty stays empty");
        // At the half-covered edge the noise can push it off 0.5.
        let edge = roughen_edge_alpha(cfg, 0.5, 0.3, 0.7);
        assert!((edge - 0.5).abs() > 1e-4 || (edge - 0.5).abs() <= 0.5, "edge can fray");
    }

    #[test]
    fn roughen_is_deterministic() {
        let cfg = RoughenEdgesConfig { border: 0.4, scale: 12.0, seed: 9 };
        assert_eq!(
            roughen_edge_alpha(cfg, 0.5, 0.2, 0.8),
            roughen_edge_alpha(cfg, 0.5, 0.2, 0.8)
        );
    }

    #[test]
    fn distort_actions_roundtrip() {
        let mut app = App::new();
        app.apply(Action::AddCornerPin { layer_id: 0, w: 100.0, h: 50.0 });
        assert!(app.corner_pins.contains_key(&0));
        app.apply(Action::SetCornerPinCorner { layer_id: 0, corner: 0, x: 5.0, y: 6.0 });
        assert_eq!(app.corner_pins[&0].tl, [5.0, 6.0]);

        app.apply(Action::AddBezierWarp { layer_id: 0 });
        app.apply(Action::SetBezierWarpCorner { layer_id: 0, corner: 3, dx: 2.0, dy: 3.0 });
        assert_eq!(app.bezier_warps[&0].br, [2.0, 3.0]);

        app.apply(Action::AddWaveWarp { layer_id: 0 });
        app.apply(Action::SetWaveWarpParam { layer_id: 0, param: "wavelength", value: 0.0 });
        assert!(app.wave_warps[&0].wavelength >= 1.0, "wavelength clamps");

        app.apply(Action::AddRoughenEdges { layer_id: 0 });
        app.apply(Action::SetRoughenEdgesSeed { layer_id: 0, seed: 77 });
        assert_eq!(app.roughen_edges[&0].seed, 77);

        app.apply(Action::RemoveCornerPin { layer_id: 0 });
        app.apply(Action::RemoveBezierWarp { layer_id: 0 });
        app.apply(Action::RemoveWaveWarp { layer_id: 0 });
        app.apply(Action::RemoveRoughenEdges { layer_id: 0 });
        assert!(app.corner_pins.is_empty() && app.bezier_warps.is_empty());
        assert!(app.wave_warps.is_empty() && app.roughen_edges.is_empty());
    }

    #[test]
    fn apply_corner_pin_through_app() {
        let mut app = App::new();
        assert!(app.apply_corner_pin(0, &[0.0; 4], 1, 1).is_none());
        app.apply(Action::AddCornerPin { layer_id: 0, w: 4.0, h: 4.0 });
        let out = app.apply_corner_pin(0, &vec![0.0; 4 * 4 * 4], 4, 4).expect("buffer");
        assert_eq!(out.len(), 4 * 4 * 4);
    }
}

/// Type aliases used by the `App` field declarations in `mod.rs`.
pub type CornerPinMap = HashMap<usize, CornerPinConfig>;
pub type BezierWarpMap = HashMap<usize, BezierWarpConfig>;
pub type WaveWarpMap = HashMap<usize, WaveWarpConfig>;
pub type RoughenEdgesMap = HashMap<usize, RoughenEdgesConfig>;
