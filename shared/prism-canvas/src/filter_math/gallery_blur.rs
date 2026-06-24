//! Blur Gallery CPU references: the variable-radius blurs whose per-pixel
//! Gaussian radius is driven by a focus band ([`tilt_shift`]), an elliptical
//! focus region ([`iris_blur`]), or inverse-distance pin interpolation
//! ([`field_blur`]). Mirrors `filter.wgsl`'s kinds 30/31/33; the focus-weight
//! falloffs ([`focus_weight`], [`iris_weight`], [`field_blur_radius_at`]) are
//! split out so they can be unit-tested without running the full kernel.

/// Tilt-shift focus weight in `[0, 1]` for a point `dist` pixels from the focus
/// line, given a sharp band half-width `half_band` and a `feather` ramp (both in
/// pixels). Returns `0` inside the band (fully sharp) and ramps linearly to `1`
/// once past `half_band + feather` (fully blurred). Bit-for-bit the WGSL
/// `focus_weight` (kind 30) so the GPU tilt-shift and this reference agree, and
/// the falloff is unit-testable without a GPU adapter.
pub fn focus_weight(dist: f32, half_band: f32, feather: f32) -> f32 {
    let d = dist.abs();
    if d <= half_band {
        return 0.0;
    }
    let f = feather.max(1e-3);
    ((d - half_band) / f).clamp(0.0, 1.0)
}

/// Tilt-Shift (Blur Gallery, kind 30): a graduated/positional blur. The image
/// stays sharp inside a focus band centred on `center_y` (in pixels) and blurs
/// progressively outside it. The per-pixel blur radius is `max_radius` scaled by
/// [`focus_weight`] of the signed distance to the (optionally tilted) focus line;
/// each pixel runs a local 2D Gaussian over that radius — mirroring the shader.
/// `angle_rad` tilts the band (0 = horizontal). Edge-clamped sampling.
pub fn tilt_shift(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    center_y: f32,
    half_band: f32,
    feather: f32,
    max_radius: f32,
    angle_rad: f32,
) -> Vec<[f32; 4]> {
    // Band normal (angle 0 → (0, 1)), matching the GPU's `apply_tilt_shift`.
    let (nx, ny) = (-angle_rad.sin(), angle_rad.cos());
    let (cx, cy) = (w as f32 * 0.5, center_y);
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let dist = (x as f32 - cx) * nx + (y as f32 - cy) * ny;
            let rad = max_radius * focus_weight(dist, half_band, feather);
            let r = rad.clamp(0.0, 64.0) as i32;
            let p = y * w + x;
            if r <= 0 {
                out[p] = img[p];
                continue;
            }
            let sigma = (rad * 0.5).max(0.5);
            let mut sum = [0.0f32; 4];
            let mut wsum = 0.0f32;
            for j in -r..=r {
                for i in -r..=r {
                    let weight = (-0.5 * (i * i + j * j) as f32 / (sigma * sigma)).exp();
                    let sx = (x as i32 + i).clamp(0, w as i32 - 1) as usize;
                    let sy = (y as i32 + j).clamp(0, h as i32 - 1) as usize;
                    let s = img[sy * w + sx];
                    for c in 0..4 {
                        sum[c] += s[c] * weight;
                    }
                    wsum += weight;
                }
            }
            for c in 0..4 {
                out[p][c] = sum[c] / wsum.max(1e-5);
            }
        }
    }
    out
}

/// Iris focus weight in `[0, 1]` from the normalized elliptical radius `e`
/// (`e = sqrt((dx/rx)^2 + (dy/ry)^2)`; `e <= 1` is inside the ellipse), given a
/// `feather` ramp expressed as a normalized fraction of the ellipse radius.
/// Returns `0` inside the ellipse (fully sharp) and ramps linearly to `1` once
/// past the boundary by `feather`. Bit-for-bit the WGSL `iris_weight` (kind 31)
/// so the GPU iris blur and this reference agree, and the falloff is
/// unit-testable without a GPU adapter.
pub fn iris_weight(e: f32, feather: f32) -> f32 {
    if e <= 1.0 {
        return 0.0;
    }
    let f = feather.max(1e-3);
    ((e - 1.0) / f).clamp(0.0, 1.0)
}

/// Iris Blur (Blur Gallery, kind 31): the radial sibling of [`tilt_shift`]. The
/// image stays sharp inside an elliptical region centred at `(cx, cy)` (pixels)
/// with pixel radii `(rx, ry)` and blurs progressively outside it. The per-pixel
/// blur radius is `max_radius` scaled by [`iris_weight`] of the normalized
/// elliptical radius; each pixel runs a local 2D Gaussian over that radius —
/// mirroring the shader. `feather` is a normalized fraction of the ellipse
/// radius. Edge-clamped sampling.
#[allow(clippy::too_many_arguments)]
pub fn iris_blur(
    img: &[[f32; 4]],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    feather: f32,
    max_radius: f32,
) -> Vec<[f32; 4]> {
    let rx = rx.max(1e-3);
    let ry = ry.max(1e-3);
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 - cx) / rx;
            let dy = (y as f32 - cy) / ry;
            let e = (dx * dx + dy * dy).sqrt();
            let rad = max_radius * iris_weight(e, feather);
            let r = rad.clamp(0.0, 64.0) as i32;
            let p = y * w + x;
            if r <= 0 {
                out[p] = img[p];
                continue;
            }
            let sigma = (rad * 0.5).max(0.5);
            let mut sum = [0.0f32; 4];
            let mut wsum = 0.0f32;
            for j in -r..=r {
                for i in -r..=r {
                    let weight = (-0.5 * (i * i + j * j) as f32 / (sigma * sigma)).exp();
                    let sx = (x as i32 + i).clamp(0, w as i32 - 1) as usize;
                    let sy = (y as i32 + j).clamp(0, h as i32 - 1) as usize;
                    let s = img[sy * w + sx];
                    for c in 0..4 {
                        sum[c] += s[c] * weight;
                    }
                    wsum += weight;
                }
            }
            for c in 0..4 {
                out[p][c] = sum[c] / wsum.max(1e-5);
            }
        }
    }
    out
}

/// A Field Blur pin: a position in pixel coords and the blur amount (px) at that
/// position. `(x, y)` are absolute pixel coordinates on the canvas.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FieldPin {
    /// Pin x position in pixels.
    pub x: f32,
    /// Pin y position in pixels.
    pub y: f32,
    /// Blur radius (px) the field should reach at this pin.
    pub amount: f32,
}

/// Field Blur radius field: the per-pixel blur radius at `(x, y)` produced by
/// **inverse-distance-squared** interpolation of the pins' amounts (Shepard /
/// IDW weighting with power 2). With a single pin the field is uniform at that
/// pin's amount; with several pins each pin's amount is reproduced exactly at its
/// own location (the weight there is infinite, so it dominates) and the value
/// varies smoothly and monotonically in between. With no pins the field is 0
/// (identity). Kept as a standalone `fn` so the WGSL pass (kind 33) can mirror it
/// bit-for-bit and the falloff is unit-testable without a GPU adapter.
///
/// The epsilon floor on the squared distance (`1e-3`) matches the shader so an
/// exact pin hit is well-defined and the CPU/GPU agree.
pub fn field_blur_radius_at(pins: &[FieldPin], x: f32, y: f32) -> f32 {
    if pins.is_empty() {
        return 0.0;
    }
    let mut wsum = 0.0f32;
    let mut asum = 0.0f32;
    for pin in pins {
        let dx = x - pin.x;
        let dy = y - pin.y;
        let d2 = (dx * dx + dy * dy).max(1e-3);
        let w = 1.0 / d2;
        wsum += w;
        asum += w * pin.amount;
    }
    asum / wsum.max(1e-30)
}

/// Field Blur (Blur Gallery, kind 33): a multi-pin variable blur. The per-pixel
/// blur radius is [`field_blur_radius_at`] (inverse-distance interpolation of the
/// pins), and each pixel runs a local 2D Gaussian over that radius — the same
/// variable-radius kernel as Tilt-Shift / Iris Blur, but the radius field comes
/// from pin interpolation instead of a band/ellipse. A single pin is therefore a
/// uniform blur everywhere at that pin's amount; an empty pin list (or all-zero
/// amounts) is the identity. Edge-clamped sampling — mirrors the shader.
pub fn field_blur(img: &[[f32; 4]], w: usize, h: usize, pins: &[FieldPin]) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let rad = field_blur_radius_at(pins, x as f32, y as f32);
            let r = rad.clamp(0.0, 64.0) as i32;
            let p = y * w + x;
            if r <= 0 {
                out[p] = img[p];
                continue;
            }
            let sigma = (rad * 0.5).max(0.5);
            let mut sum = [0.0f32; 4];
            let mut wsum = 0.0f32;
            for j in -r..=r {
                for i in -r..=r {
                    let weight = (-0.5 * (i * i + j * j) as f32 / (sigma * sigma)).exp();
                    let sx = (x as i32 + i).clamp(0, w as i32 - 1) as usize;
                    let sy = (y as i32 + j).clamp(0, h as i32 - 1) as usize;
                    let s = img[sy * w + sx];
                    for c in 0..4 {
                        sum[c] += s[c] * weight;
                    }
                    wsum += weight;
                }
            }
            for c in 0..4 {
                out[p][c] = sum[c] / wsum.max(1e-5);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_math::test_support::{approx, vsplit};

    // ---- Tilt-Shift (Blur Gallery) ----------------------------------------

    #[test]
    fn focus_weight_is_zero_in_band_one_past_feather_and_monotonic() {
        let (hb, f) = (10.0f32, 8.0f32);
        // Inside the band (|dist| ≤ half-width) → fully sharp (0).
        assert_eq!(focus_weight(0.0, hb, f), 0.0);
        assert_eq!(focus_weight(hb, hb, f), 0.0);
        assert_eq!(focus_weight(-hb, hb, f), 0.0, "symmetric about the line");
        // At the far end of the feather and beyond → fully blurred (1).
        assert!(approx(focus_weight(hb + f, hb, f), 1.0, 1e-6));
        assert_eq!(focus_weight(hb + f + 50.0, hb, f), 1.0, "clamped past feather");
        // Mid-feather is a partial weight strictly between 0 and 1.
        let mid = focus_weight(hb + f * 0.5, hb, f);
        assert!(mid > 0.0 && mid < 1.0, "mid-feather is partial, got {mid}");
        // Monotonically non-decreasing with distance.
        let mut prev = 0.0;
        let mut d = 0.0;
        while d <= hb + f + 5.0 {
            let cur = focus_weight(d, hb, f);
            assert!(cur >= prev - 1e-7, "weight must not decrease ({prev} -> {cur} at d={d})");
            prev = cur;
            d += 0.5;
        }
    }

    #[test]
    fn tilt_shift_keeps_the_band_sharp_and_blurs_outside() {
        // 32×32 noisy image: a vertical hard edge (black left, white right) so
        // a blur visibly softens it. Focus band centred on the middle row.
        let n = 32usize;
        let img = vsplit(n);
        let center_y = (n / 2) as f32;
        let half_band = 3.0;
        let feather = 4.0;
        let out = tilt_shift(&img, n, n, center_y, half_band, feather, 8.0, 0.0);
        let col = n / 2; // the hard-edge column (x = n/2 is white, x-1 is black)
        // A row inside the focus band is untouched: the edge stays a hard step.
        let in_band = n / 2;
        let sharp_l = out[in_band * n + (col - 1)][0];
        let sharp_r = out[in_band * n + col][0];
        assert!(approx(sharp_l, 0.0, 1e-5), "in-band left of edge stays black");
        assert!(approx(sharp_r, 1.0, 1e-5), "in-band right of edge stays white");
        // A row far outside the band (top of image) is blurred: the edge bleeds,
        // so the column just left of the step is no longer pure black.
        let blurred = out[col - 1][0];
        assert!(blurred > 0.05, "far-from-band edge should blur (bleed), got {blurred}");
    }

    #[test]
    fn tilt_shift_zero_radius_is_identity() {
        let n = 16usize;
        let img = vsplit(n);
        let out = tilt_shift(&img, n, n, (n / 2) as f32, 2.0, 4.0, 0.0, 0.0);
        assert_eq!(out, img, "max radius 0 leaves the image untouched everywhere");
    }

    // ---- Iris Blur (Blur Gallery) -----------------------------------------

    #[test]
    fn iris_weight_is_zero_inside_one_at_full_feather_and_monotonic() {
        let f = 0.5f32;
        // Inside the ellipse (e ≤ 1) → fully sharp (0).
        assert_eq!(iris_weight(0.0, f), 0.0);
        assert_eq!(iris_weight(1.0, f), 0.0, "on the boundary is still sharp");
        // At the far end of the feather and beyond → fully blurred (1).
        assert!(approx(iris_weight(1.0 + f, f), 1.0, 1e-6));
        assert_eq!(iris_weight(5.0, f), 1.0, "clamped well past feather");
        // Mid-feather is a partial weight strictly between 0 and 1.
        let mid = iris_weight(1.0 + f * 0.5, f);
        assert!(mid > 0.0 && mid < 1.0, "mid-feather is partial, got {mid}");
        // Monotonically non-decreasing with normalized radius.
        let mut prev = 0.0;
        let mut e = 0.0;
        while e <= 1.0 + f + 0.5 {
            let cur = iris_weight(e, f);
            assert!(cur >= prev - 1e-7, "weight must not decrease ({prev} -> {cur} at e={e})");
            prev = cur;
            e += 0.05;
        }
    }

    #[test]
    fn iris_blur_keeps_the_center_sharp_and_blurs_outside() {
        // 41×41 image with a vertical hard edge (black left, white right) so a
        // blur visibly softens it. A small ellipse keeps the center crisp; the
        // corners (well outside the ellipse) blur and bleed across the edge.
        let n = 41usize;
        let img = vsplit(n);
        let c = (n / 2) as f32;
        let (rx, ry) = (5.0, 5.0); // small focus ellipse at the center
        let out = iris_blur(&img, n, n, c, c, rx, ry, 0.5, 8.0);
        let col = n / 2; // edge column: x = n/2 is white, x-1 is black
        // Center pixel (inside the ellipse) is untouched: hard step preserved.
        let mid = n / 2;
        assert!(approx(out[mid * n + (col - 1)][0], 0.0, 1e-5), "center-left stays black");
        assert!(approx(out[mid * n + col][0], 1.0, 1e-5), "center-right stays white");
        // Top-of-image row (far outside the ellipse) is blurred: edge bleeds, so
        // the column just left of the step is no longer pure black.
        let blurred = out[col - 1][0];
        assert!(blurred > 0.05, "far-from-center edge should blur (bleed), got {blurred}");
    }

    #[test]
    fn iris_blur_falloff_is_monotonic_with_normalized_radius() {
        // Along a horizontal line out from the center on a vertical edge image,
        // the effective blur radius (hence the bleed at the step) should grow
        // monotonically with the normalized elliptical radius. Probe the focus
        // weight directly across increasing distance.
        let f = 0.6f32;
        let (rx, ry) = (30.0f32, 20.0f32);
        let mut prev = 0.0;
        for k in 0..40 {
            let dx = k as f32 * 2.0;
            let e = ((dx / rx) * (dx / rx)).sqrt(); // dy = 0
            let w = iris_weight(e, f);
            assert!(w >= prev - 1e-7, "iris falloff must not decrease at dx={dx}");
            prev = w;
        }
        let _ = ry; // ry exercised by the elliptical-distance test above
    }

    #[test]
    fn iris_blur_zero_radius_is_identity() {
        let n = 16usize;
        let img = vsplit(n);
        let c = (n / 2) as f32;
        let out = iris_blur(&img, n, n, c, c, 4.0, 4.0, 0.5, 0.0);
        assert_eq!(out, img, "max radius 0 leaves the image untouched everywhere");
    }

    // ---- Field Blur (Blur Gallery, kind 33) -------------------------------

    #[test]
    fn field_blur_single_pin_is_uniform_everywhere() {
        // One pin → the radius field is that pin's amount at every pixel,
        // regardless of position.
        let pin = [FieldPin { x: 3.0, y: 7.0, amount: 9.5 }];
        for &(x, y) in &[(0.0, 0.0), (3.0, 7.0), (50.0, 1.0), (12.3, 44.7)] {
            let r = field_blur_radius_at(&pin, x, y);
            assert!(approx(r, 9.5, 1e-4), "single pin uniform: got {r} at ({x},{y})");
        }
    }

    #[test]
    fn field_blur_reproduces_each_pin_amount_at_its_location() {
        // Two pins with distinct amounts: at each pin's own position the field
        // must equal that pin's amount (its IDW weight there is overwhelming).
        let pins = [
            FieldPin { x: 5.0, y: 5.0, amount: 2.0 },
            FieldPin { x: 95.0, y: 5.0, amount: 20.0 },
        ];
        let a = field_blur_radius_at(&pins, 5.0, 5.0);
        let b = field_blur_radius_at(&pins, 95.0, 5.0);
        assert!(approx(a, 2.0, 1e-2), "field equals pin A amount at pin A, got {a}");
        assert!(approx(b, 20.0, 1e-2), "field equals pin B amount at pin B, got {b}");
    }

    #[test]
    fn field_blur_interpolates_monotonically_between_two_pins() {
        // Walking along the line between two pins, the radius rises monotonically
        // from the lower amount toward the higher and stays strictly between the
        // two amounts in the interior.
        let pins = [
            FieldPin { x: 0.0, y: 0.0, amount: 4.0 },
            FieldPin { x: 100.0, y: 0.0, amount: 16.0 },
        ];
        let mut prev = field_blur_radius_at(&pins, 1.0, 0.0);
        for k in 1..100 {
            let x = k as f32;
            let r = field_blur_radius_at(&pins, x, 0.0);
            assert!(r >= prev - 1e-4, "field must not decrease toward the larger pin at x={x}");
            assert!((4.0..=16.0).contains(&r), "interior stays within the pin amounts, got {r}");
            prev = r;
        }
        // The midpoint between two equal-distance pins is the mean of the amounts.
        let mid = field_blur_radius_at(&pins, 50.0, 0.0);
        assert!(approx(mid, 10.0, 1e-3), "equidistant midpoint is the mean, got {mid}");
    }

    #[test]
    fn field_blur_zero_amount_and_no_pins_are_identity() {
        let n = 16usize;
        let img = vsplit(n);
        // All-zero pin amounts → radius 0 everywhere → identity.
        let zero = [
            FieldPin { x: 2.0, y: 2.0, amount: 0.0 },
            FieldPin { x: 13.0, y: 13.0, amount: 0.0 },
        ];
        assert_eq!(field_blur(&img, n, n, &zero), img, "zero-amount pins are identity");
        // No pins at all → radius 0 everywhere → identity.
        assert_eq!(field_blur(&img, n, n, &[]), img, "no pins is identity");
    }

    #[test]
    fn field_blur_blurs_the_image_near_a_nonzero_pin() {
        // A single nonzero pin → uniform blur. On a vertical edge image the step
        // softens: the column just left of the edge is no longer pure black.
        let n = 21usize;
        let img = vsplit(n);
        let pin = [FieldPin { x: (n / 2) as f32, y: (n / 2) as f32, amount: 6.0 }];
        let out = field_blur(&img, n, n, &pin);
        let col = n / 2;
        let row = n / 2;
        let bled = out[row * n + (col - 1)][0];
        assert!(bled > 0.05, "uniform field blur should soften the edge, got {bled}");
    }
}
