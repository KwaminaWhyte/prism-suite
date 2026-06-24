//! Distort filters (Phase 8) — per-pixel coordinate-displacement remaps. Each
//! computes a *source* pixel coordinate for every output pixel and edge-clamp
//! bilinear-samples there, mirroring `filter.wgsl`'s distort branches (kinds
//! 8–12). All work in pixel space about a center (cx, cy).

use super::{sample_bilinear, TAU};

/// Twirl: rotate the source about `(cx, cy)` by an angle that falls off
/// quadratically to 0 at `radius`. Inside the radius only; outside is identity.
/// `angle` is the maximum rotation in radians at the center.
pub fn twirl(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    angle: f32,
    radius: f32,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let (mut sx, mut sy) = (x as f32, y as f32);
            if dist < radius && radius > 0.0 {
                let falloff = 1.0 - dist / radius;
                let a = angle * falloff * falloff;
                let (ca, sa) = (a.cos(), a.sin());
                // Inverse map: sample the source rotated by -a.
                sx = cx + dx * ca + dy * sa;
                sy = cy - dx * sa + dy * ca;
            }
            out[y * w + x] = sample_bilinear(img, w, h, sx, sy);
        }
    }
    out
}

/// Pinch (`amount` > 0, pulls toward the center) / Spherize-bulge (`amount` < 0,
/// pushes outward) about `(cx, cy)` within `radius`. Radial remap with a smooth
/// falloff to the edge of the affected disc.
pub fn pinch(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    amount: f32,
    radius: f32,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let (mut sx, mut sy) = (x as f32, y as f32);
            if dist < radius && radius > 0.0 && dist > 1e-4 {
                let nd = dist / radius; // 0 at center .. 1 at edge
                let s = nd.powf(1.0 + amount);
                let f = s / nd;
                sx = cx + dx * f;
                sy = cy + dy * f;
            }
            out[y * w + x] = sample_bilinear(img, w, h, sx, sy);
        }
    }
    out
}

/// Ripple / Wave: sinusoidal displacement — each axis is offset by a sine of the
/// *other* axis. `amplitude` is the peak displacement in pixels; `wavelength` is
/// the period in pixels.
pub fn ripple(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    amplitude: f32,
    wavelength: f32,
) -> Vec<[f32; 4]> {
    let wl = wavelength.max(1e-3);
    let k = TAU / wl;
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let sx = x as f32 + amplitude * (y as f32 * k).sin();
            let sy = y as f32 + amplitude * (x as f32 * k).sin();
            out[y * w + x] = sample_bilinear(img, w, h, sx, sy);
        }
    }
    out
}

/// Polar Coordinates mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PolarMode {
    /// Rectangular → polar: output x = angle, output y = radius.
    ToPolar,
    /// Polar → rectangular: the inverse of [`PolarMode::ToPolar`].
    ToRect,
}

/// Polar Coordinates remap about `(cx, cy)`. `ToPolar` lays the image out so the
/// horizontal axis sweeps the angle (0..2π over the width, starting at the top)
/// and the vertical axis is the radius (0 at top → `maxr` at the bottom).
/// `ToRect` is the exact inverse, so `ToRect ∘ ToPolar` is (modulo resampling)
/// the identity.
pub fn polar(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    mode: PolarMode,
) -> Vec<[f32; 4]> {
    let (wf, hf) = (w as f32, h as f32);
    let maxr = wf.min(hf) * 0.5;
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = match mode {
                PolarMode::ToPolar => {
                    let theta = (x as f32 / wf) * TAU - std::f32::consts::FRAC_PI_2;
                    let rr = (y as f32 / hf) * maxr;
                    (cx + rr * theta.cos(), cy + rr * theta.sin())
                }
                PolarMode::ToRect => {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let mut theta = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                    if theta < 0.0 {
                        theta += TAU;
                    }
                    if theta >= TAU {
                        theta -= TAU;
                    }
                    let rr = (dx * dx + dy * dy).sqrt();
                    ((theta / TAU) * wf, (rr / maxr) * hf)
                }
            };
            out[y * w + x] = sample_bilinear(img, w, h, sx, sy);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, ramp_x};

    // ---- Twirl ------------------------------------------------------------

    #[test]
    fn twirl_angle_zero_is_identity() {
        let n = 21;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let out = twirl(&img, n, n, c, c, 0.0, 8.0);
        for i in 0..img.len() {
            for ch in 0..4 {
                assert!(approx(out[i][ch], img[i][ch], 1e-4), "twirl 0 identity");
            }
        }
    }

    #[test]
    fn twirl_leaves_pixels_outside_radius_untouched() {
        // A point well outside the twirl disc must be exactly the source.
        let n = 31;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let out = twirl(&img, n, n, c, c, 1.2, 5.0);
        // Corner is far outside radius 5.
        let p = n + 1;
        for ch in 0..4 {
            assert!(approx(out[p][ch], img[p][ch], 1e-4), "outside radius fixed");
        }
        // The exact center is the fixed point of the rotation.
        let cc = (n / 2) * n + n / 2;
        for ch in 0..4 {
            assert!(approx(out[cc][ch], img[cc][ch], 1e-4), "center fixed");
        }
    }

    #[test]
    fn twirl_rotates_a_point_by_the_falloff_angle() {
        // On a ramp, a CCW image rotation pulls the source from a rotated-by-(−a)
        // location. Place a probe directly above center: with a positive angle,
        // the source x shifts off the column, changing the sampled value.
        let n = 41;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let out = twirl(&img, n, n, c, c, 1.5, 18.0);
        // A pixel above the center (dx=0): rotation moves the sample sideways, so
        // its value departs from the center column value (0.5 on a symmetric ramp).
        let px = n / 2;
        let py = n / 2 - 6;
        let v = out[py * n + px][0];
        assert!(
            (v - 0.5).abs() > 0.05,
            "twirl rotated the sample off the center column: {v}"
        );
    }

    #[test]
    fn twirl_is_deterministic() {
        let n = 25;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let a = twirl(&img, n, n, c, c, 0.9, 10.0);
        let b = twirl(&img, n, n, c, c, 0.9, 10.0);
        assert_eq!(a, b);
    }

    // ---- Pinch / Spherize -------------------------------------------------

    #[test]
    fn pinch_amount_zero_is_identity() {
        let n = 21;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let out = pinch(&img, n, n, c, c, 0.0, 8.0);
        for i in 0..img.len() {
            for ch in 0..4 {
                assert!(approx(out[i][ch], img[i][ch], 1e-4), "pinch 0 identity");
            }
        }
    }

    #[test]
    fn pinch_and_bulge_displace_in_opposite_directions() {
        // Probe a pixel to the right of center on a left→right ramp. Pinch
        // (amount > 0) maps the *source* radius inward (smaller src radius ⇒
        // sampled value closer to the center value 0.5, i.e. *smaller* than the
        // identity value here). Bulge (amount < 0) maps it outward (larger src
        // radius ⇒ value further from center, *larger*). So the two move the
        // sampled value to opposite sides of the identity.
        let n = 41;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let radius = 18.0;
        let px = n / 2 + 6; // right of center
        let py = n / 2;
        let idx = py * n + px;
        let identity = img[idx][0];

        let p = pinch(&img, n, n, c, c, 0.6, radius);
        let b = pinch(&img, n, n, c, c, -0.6, radius);
        let pv = p[idx][0];
        let bv = b[idx][0];
        assert!(pv < identity - 1e-3, "pinch pulls source inward: {pv}");
        assert!(bv > identity + 1e-3, "bulge pushes source outward: {bv}");
        assert!(bv > pv, "bulge and pinch are signed-opposite");
    }

    #[test]
    fn pinch_keeps_the_center_fixed() {
        let n = 21;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let out = pinch(&img, n, n, c, c, 0.7, 9.0);
        let cc = (n / 2) * n + n / 2;
        for ch in 0..4 {
            assert!(approx(out[cc][ch], img[cc][ch], 1e-3), "center fixed");
        }
    }

    // ---- Ripple / Wave ----------------------------------------------------

    #[test]
    fn ripple_zero_amplitude_is_identity() {
        let n = 16;
        let img = ramp_x(n);
        let out = ripple(&img, n, n, 0.0, 20.0);
        for i in 0..img.len() {
            for ch in 0..4 {
                assert!(approx(out[i][ch], img[i][ch], 1e-4), "ripple 0 identity");
            }
        }
    }

    #[test]
    fn ripple_displacement_is_periodic_in_the_wavelength() {
        // The x-axis displacement is amplitude*sin(y*k); rows one wavelength
        // apart get the same displacement, so identical output rows (on a column-
        // invariant ramp the y-offset doesn't change the value, only the x-offset
        // does — and that depends on y only). Use a ramp_x so the value depends
        // purely on the (displaced) x coordinate.
        let n = 60;
        let wl = 20.0;
        let img = ramp_x(n);
        let out = ripple(&img, n, n, 6.0, wl);
        let row = |r: usize| (0..n).map(|x| out[r * n + x][0]).collect::<Vec<_>>();
        // Rows y and y+wl share the same sin(y*k) displacement → identical rows.
        let r0 = row(10);
        let r1 = row(10 + wl as usize);
        for x in 0..n {
            assert!(
                approx(r0[x], r1[x], 1e-4),
                "rows one wavelength apart match at x={x}: {} vs {}",
                r0[x],
                r1[x]
            );
        }
    }

    #[test]
    fn ripple_actually_displaces() {
        let n = 40;
        let img = ramp_x(n);
        let out = ripple(&img, n, n, 8.0, 16.0);
        // At least some pixels must differ from the source (non-trivial warp).
        let changed = (0..img.len()).any(|i| (out[i][0] - img[i][0]).abs() > 1e-3);
        assert!(changed, "ripple must move some pixels");
    }

    // ---- Polar Coordinates ------------------------------------------------

    #[test]
    fn polar_round_trip_recovers_the_interior() {
        // ToRect ∘ ToPolar should recover the original within the inscribed disc,
        // up to resampling error. Use a smooth radial test image so bilinear
        // resampling is well-behaved, and check pixels near the center.
        let n = 64;
        let c = (n / 2) as f32;
        let mut img = vec![[0.0f32; 4]; n * n];
        for y in 0..n {
            for x in 0..n {
                let dx = x as f32 - c;
                let dy = y as f32 - c;
                let r = (dx * dx + dy * dy).sqrt() / c;
                let v = (1.0 - r).clamp(0.0, 1.0); // smooth radial bump
                img[y * n + x] = [v, v, v, 1.0];
            }
        }
        let pol = polar(&img, n, n, c, c, PolarMode::ToPolar);
        let back = polar(&pol, n, n, c, c, PolarMode::ToRect);

        // Sample a ring of points at moderate radius and compare.
        let mut max_err = 0.0f32;
        let probe_r = (n as f32) * 0.25;
        for k in 0..16 {
            let a = k as f32 / 16.0 * TAU;
            let x = (c + probe_r * a.cos()).round() as usize;
            let y = (c + probe_r * a.sin()).round() as usize;
            let err = (back[y * n + x][0] - img[y * n + x][0]).abs();
            max_err = max_err.max(err);
        }
        assert!(max_err < 0.1, "polar round-trip error too large: {max_err}");
    }

    #[test]
    fn polar_to_polar_maps_radius_to_rows() {
        // In ToPolar, the top row (y=0) maps to radius 0 → the center pixel, and
        // lower rows map to larger radii. A radial bump (bright center) should
        // therefore be bright along the top and dim toward the bottom.
        let n = 48;
        let c = (n / 2) as f32;
        let mut img = vec![[0.0f32; 4]; n * n];
        for y in 0..n {
            for x in 0..n {
                let dx = x as f32 - c;
                let dy = y as f32 - c;
                let r = (dx * dx + dy * dy).sqrt() / c;
                let v = (1.0 - r).clamp(0.0, 1.0);
                img[y * n + x] = [v, v, v, 1.0];
            }
        }
        let pol = polar(&img, n, n, c, c, PolarMode::ToPolar);
        let mid = n / 2;
        let top = pol[n + mid][0];
        let bottom = pol[(n - 2) * n + mid][0];
        assert!(
            top > bottom + 0.2,
            "top (r≈0) brighter than bottom: {top} {bottom}"
        );
    }

    #[test]
    fn polar_is_deterministic() {
        let n = 32;
        let c = (n / 2) as f32;
        let img = ramp_x(n);
        let a = polar(&img, n, n, c, c, PolarMode::ToPolar);
        let b = polar(&img, n, n, c, c, PolarMode::ToPolar);
        assert_eq!(a, b);
    }
}
