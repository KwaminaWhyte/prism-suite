use super::*;

// --- Spatial effects (Gaussian Blur / Drop Shadow / Glow) ---------------

/// A `w×h` premultiplied buffer with a single fully-opaque white pixel at
/// `(cx, cy)` (a unit impulse) and transparent everywhere else.
fn impulse(w: usize, h: usize, cx: usize, cy: usize) -> Vec<[f32; 4]> {
    let mut buf = vec![[0.0f32; 4]; w * h];
    buf[cy * w + cx] = [1.0, 1.0, 1.0, 1.0];
    buf
}

#[test]
fn gaussian_kernel_is_normalized_and_symmetric() {
    let k = gaussian_kernel(2.0);
    assert!(k.len() % 2 == 1, "kernel must be odd-length");
    let sum: f32 = k.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "kernel sums to {sum}");
    // Symmetric about the center.
    let n = k.len();
    for i in 0..n / 2 {
        assert!((k[i] - k[n - 1 - i]).abs() < 1e-6, "asymmetric at {i}");
    }
    // The center weight is the largest.
    assert!(k[n / 2] >= k[0]);
}

#[test]
fn gaussian_kernel_zero_sigma_is_identity() {
    assert_eq!(gaussian_kernel(0.0), vec![1.0]);
    assert_eq!(gaussian_kernel(-3.0), vec![1.0]);
}

#[test]
fn gaussian_blur_conserves_alpha_mass() {
    // A blur redistributes coverage but, with no edge loss for a centered
    // impulse well inside the buffer, conserves total alpha.
    let mut buf = impulse(21, 21, 10, 10);
    let before: f32 = buf.iter().map(|p| p[3]).sum();
    gaussian_blur(&mut buf, 21, 21, 3.0, 3.0, false);
    let after: f32 = buf.iter().map(|p| p[3]).sum();
    assert!((before - after).abs() < 1e-3, "{before} vs {after}");
    // The center is no longer a hard 1.0 (energy spread to neighbours)...
    assert!(buf[10 * 21 + 10][3] < 1.0);
    // ...and a neighbour now carries some coverage.
    assert!(buf[10 * 21 + 11][3] > 0.0);
}

#[test]
fn gaussian_blur_zero_sigma_is_noop() {
    let mut buf = impulse(9, 9, 4, 4);
    let orig = buf.clone();
    gaussian_blur(&mut buf, 9, 9, 0.0, 0.0, false);
    assert_eq!(buf, orig);
}

#[test]
fn gaussian_blur_premultiplied_no_color_bleed() {
    // A blurred opaque white impulse stays white where it has coverage: in
    // premultiplied space color == rgb/alpha must remain ~white (no bleed
    // toward black from the transparent neighbours).
    let mut buf = impulse(21, 21, 10, 10);
    gaussian_blur(&mut buf, 21, 21, 2.0, 2.0, false);
    let p = buf[10 * 21 + 11];
    assert!(p[3] > 0.0);
    let (r, g, b) = (p[0] / p[3], p[1] / p[3], p[2] / p[3]);
    assert!(
        (r - 1.0).abs() < 1e-3 && (g - 1.0).abs() < 1e-3 && (b - 1.0).abs() < 1e-3,
        "color bled: {r},{g},{b}"
    );
}

#[test]
fn drop_shadow_offsets_coverage_behind_the_layer() {
    // A small opaque square; a 0-softness shadow offset right/down should put
    // shadow coverage where the layer is transparent, down-right of it.
    let w = 32;
    let mut buf = vec![[0.0f32; 4]; w * w];
    for y in 12..16 {
        for x in 12..16 {
            buf[y * w + x] = [1.0, 1.0, 1.0, 1.0];
        }
    }
    SpatialEffect::DropShadow {
        color: [0.0, 0.0, 0.0],
        opacity: 1.0,
        angle: 0.0, // straight right (+x)
        distance: 6.0,
        softness: 0.0,
        shadow_only: false,
    }
    .apply(&mut buf, w, w);
    // The layer pixel is still opaque white (shadow is behind it).
    let layer_px = buf[13 * w + 13];
    assert!((layer_px[0] / layer_px[3] - 1.0).abs() < 1e-3);
    // A pixel 6px right of the square (previously transparent) now carries
    // dark shadow coverage.
    let shadow_px = buf[13 * w + 19];
    assert!(shadow_px[3] > 0.5, "shadow alpha {}", shadow_px[3]);
    // It's dark (black tint) — rgb ~0 in premultiplied space.
    assert!(shadow_px[0] < 0.05 && shadow_px[1] < 0.05 && shadow_px[2] < 0.05);
}

#[test]
fn drop_shadow_shadow_only_drops_the_layer() {
    let w = 24;
    let mut buf = vec![[0.0f32; 4]; w * w];
    buf[10 * w + 10] = [1.0, 1.0, 1.0, 1.0];
    SpatialEffect::DropShadow {
        color: [0.0, 0.0, 0.0],
        opacity: 1.0,
        angle: 0.0,
        distance: 4.0,
        softness: 0.0,
        shadow_only: true,
    }
    .apply(&mut buf, w, w);
    // The original layer pixel is gone (replaced by shadow buffer, which is
    // transparent there since the shadow moved right).
    assert_eq!(buf[10 * w + 10][3], 0.0, "layer should be dropped");
    // The shadow lives 4px to the right.
    assert!(buf[10 * w + 14][3] > 0.5, "shadow present at the offset");
}

#[test]
fn glow_brightens_a_bright_region() {
    // A bright (but sub-white) opaque blob; glow should screen a bloom over
    // it, raising its luminance toward white, and extend coverage outward.
    let w = 32;
    let mut buf = vec![[0.0f32; 4]; w * w];
    for y in 13..19 {
        for x in 13..19 {
            buf[y * w + x] = [0.9, 0.9, 0.9, 1.0];
        }
    }
    let before = buf[15 * w + 15][0];
    SpatialEffect::Glow {
        threshold: 0.5,
        radius: 4.0,
        intensity: 2.0,
    }
    .apply(&mut buf, w, w);
    // The center got brighter (bloom screened on top).
    assert!(buf[15 * w + 15][0] > before, "glow should brighten");
    // The glow bled outside the original blob (a previously-empty pixel near
    // the edge now has some coverage).
    assert!(
        buf[15 * w + 21][3] > 0.0,
        "glow should extend past the edge"
    );
}

#[test]
fn glow_below_threshold_is_inert() {
    // A dim blob below the threshold produces no bloom -> buffer unchanged.
    let w = 16;
    let mut buf = vec![[0.0f32; 4]; w * w];
    for y in 6..10 {
        for x in 6..10 {
            buf[y * w + x] = [0.2, 0.2, 0.2, 1.0];
        }
    }
    let orig = buf.clone();
    SpatialEffect::Glow {
        threshold: 0.8,
        radius: 4.0,
        intensity: 2.0,
    }
    .apply(&mut buf, w, w);
    assert_eq!(buf, orig, "below-threshold glow must be inert");
}

#[test]
fn spatial_effect_apply_ignores_empty_buffer() {
    // Degenerate sizes are a no-op (no panic).
    let mut empty: Vec<[f32; 4]> = Vec::new();
    SpatialEffect::GaussianBlur {
        sigma_x: 3.0,
        sigma_y: 3.0,
        repeat_edge: false,
    }
    .apply(&mut empty, 0, 0);
    assert!(empty.is_empty());
}

#[test]
fn apply_spatial_effects_stacks_in_order() {
    // Two blurs spread more than one: stacking runs both passes.
    let mut one = impulse(21, 21, 10, 10);
    gaussian_blur(&mut one, 21, 21, 2.0, 2.0, false);
    let mut two = impulse(21, 21, 10, 10);
    apply_spatial_effects(
        &[
            SpatialEffect::GaussianBlur {
                sigma_x: 2.0,
                sigma_y: 2.0,
                repeat_edge: false,
            },
            SpatialEffect::GaussianBlur {
                sigma_x: 2.0,
                sigma_y: 2.0,
                repeat_edge: false,
            },
        ],
        &mut two,
        21,
        21,
    );
    // The twice-blurred center is lower (more spread) than the once-blurred.
    assert!(two[10 * 21 + 10][3] < one[10 * 21 + 10][3]);
}

// --- Box / Directional / Radial blur ------------------------------------

/// A `w×h` premultiplied buffer with a fully-opaque white square centred at
/// `(cx, cy)` of half-extent `r`, transparent elsewhere.
fn square(w: usize, h: usize, cx: usize, cy: usize, r: usize) -> Vec<[f32; 4]> {
    let mut buf = vec![[0.0f32; 4]; w * h];
    for y in cy.saturating_sub(r)..=(cy + r).min(h - 1) {
        for x in cx.saturating_sub(r)..=(cx + r).min(w - 1) {
            buf[y * w + x] = [1.0, 1.0, 1.0, 1.0];
        }
    }
    buf
}

#[test]
fn spatial_label_and_defaults_are_consistent() {
    // Defaults are one per variant, Blur family first, labels stable.
    let d = SpatialEffect::defaults();
    assert_eq!(d.len(), 6);
    assert_eq!(d[0].label(), "Gaussian Blur");
    assert_eq!(d[1].label(), "Box Blur");
    assert_eq!(d[2].label(), "Directional Blur");
    assert_eq!(d[3].label(), "Radial Blur");
    assert_eq!(d[4].label(), "Drop Shadow");
    assert_eq!(d[5].label(), "Glow");
}

#[test]
fn box_blur_conserves_alpha_mass() {
    // A box blur spreads coverage but conserves total alpha for an impulse well
    // inside the buffer (no edge loss).
    let mut buf = impulse(21, 21, 10, 10);
    let before: f32 = buf.iter().map(|p| p[3]).sum();
    box_blur(&mut buf, 21, 21, 3.0, 1, false);
    let after: f32 = buf.iter().map(|p| p[3]).sum();
    assert!((before - after).abs() < 1e-3, "{before} vs {after}");
    // The center spread to its neighbours.
    assert!(buf[10 * 21 + 10][3] < 1.0);
    assert!(buf[10 * 21 + 11][3] > 0.0);
}

#[test]
fn box_blur_one_pass_is_a_flat_average() {
    // A single box pass over an impulse is a uniform average: every covered pixel
    // in the box window carries the *same* coverage (1 / window_area), unlike a
    // Gaussian whose centre is the peak.
    let mut buf = impulse(21, 21, 10, 10);
    box_blur(&mut buf, 21, 21, 2.0, 1, false);
    // Separable 2-pass box of radius 2 → 5×5 flat window: each pixel = 1/25.
    let expect = 1.0 / 25.0;
    for (dy, dx) in [(0, 0), (1, 0), (0, 2), (-2, -1)] {
        let v = buf[(10 + dy) as usize * 21 + (10 + dx) as usize][3];
        assert!((v - expect).abs() < 1e-5, "flat box tap {dx},{dy} = {v}");
    }
}

#[test]
fn box_blur_iterations_smooth_toward_gaussian() {
    // More box iterations spread the impulse wider (the central peak drops): the
    // central-limit smoothing. 3 passes is lower-peaked than 1.
    let mut one = impulse(31, 31, 15, 15);
    box_blur(&mut one, 31, 31, 3.0, 1, false);
    let mut three = impulse(31, 31, 15, 15);
    box_blur(&mut three, 31, 31, 3.0, 3, false);
    assert!(
        three[15 * 31 + 15][3] < one[15 * 31 + 15][3],
        "3 box passes should be smoother (lower peak) than 1"
    );
    // Alpha mass is still conserved across iterations (impulse stays interior).
    let mass: f32 = three.iter().map(|p| p[3]).sum();
    assert!((mass - 1.0).abs() < 1e-2, "box iterations conserve mass: {mass}");
}

#[test]
fn box_blur_separable_matches_2d_average() {
    // A single separable box pass (H then V) equals a true 2-D mean over the
    // (2r+1)² window. Verify on an interior pixel of a constant patch where the
    // window is fully covered (so the average is exact and obvious).
    let w = 16;
    let mut buf = square(w, w, 8, 8, 4); // opaque 9×9 block centred at (8,8)
    box_blur(&mut buf, w, w, 1.0, 1, false);
    // The deep interior of the block (window entirely inside the block) stays a
    // full 1.0 — averaging all-ones is one.
    let p = buf[8 * w + 8];
    assert!((p[3] - 1.0).abs() < 1e-5, "interior box average = {}", p[3]);
}

#[test]
fn box_blur_radius_zero_is_noop() {
    let mut buf = impulse(9, 9, 4, 4);
    let orig = buf.clone();
    box_blur(&mut buf, 9, 9, 0.0, 3, false);
    assert_eq!(buf, orig);
}

#[test]
fn box_blur_premultiplied_no_color_bleed() {
    // A blurred opaque white impulse stays white where it has coverage (no bleed
    // toward black from transparent neighbours) — premultiplied averaging.
    let mut buf = impulse(21, 21, 10, 10);
    box_blur(&mut buf, 21, 21, 2.0, 2, false);
    let p = buf[10 * 21 + 11];
    assert!(p[3] > 0.0);
    let (r, g, b) = (p[0] / p[3], p[1] / p[3], p[2] / p[3]);
    assert!(
        (r - 1.0).abs() < 1e-3 && (g - 1.0).abs() < 1e-3 && (b - 1.0).abs() < 1e-3,
        "color bled: {r},{g},{b}"
    );
}

#[test]
fn directional_blur_smears_along_the_angle_only() {
    // A horizontal (0°) directional blur smears an impulse left↔right but leaves
    // the column above/below it crisp (the perpendicular axis is untouched).
    let w = 41;
    let mut buf = impulse(w, w, 20, 20);
    directional_blur(&mut buf, w, w, 0.0, 8.0);
    // Coverage spread horizontally: a pixel several px left/right now has alpha.
    assert!(buf[20 * w + 26][3] > 0.0, "smear reached right along the axis");
    assert!(buf[20 * w + 14][3] > 0.0, "smear reached left along the axis");
    // The perpendicular (vertical) neighbours stay empty — no smear off-axis.
    assert_eq!(buf[26 * w + 20][3], 0.0, "no smear vertically");
    assert_eq!(buf[14 * w + 20][3], 0.0, "no smear vertically");
}

#[test]
fn directional_blur_vertical_smears_vertically() {
    // The 90° case smears the other axis — proving the angle drives the direction.
    let w = 41;
    let mut buf = impulse(w, w, 20, 20);
    directional_blur(&mut buf, w, w, 90.0, 8.0);
    assert!(buf[26 * w + 20][3] > 0.0, "smear reached down along the axis");
    assert!(buf[14 * w + 20][3] > 0.0, "smear reached up along the axis");
    assert_eq!(buf[20 * w + 26][3], 0.0, "no smear horizontally");
    assert_eq!(buf[20 * w + 14][3], 0.0, "no smear horizontally");
}

#[test]
fn directional_blur_zero_length_is_noop() {
    let w = 16;
    let mut buf = square(w, w, 8, 8, 2);
    let orig = buf.clone();
    directional_blur(&mut buf, w, w, 30.0, 0.0);
    assert_eq!(buf, orig);
}

#[test]
fn radial_spin_blurs_tangentially_not_radially() {
    // A spin blur about the centre sweeps samples *around* the centre: an off-axis
    // impulse smears along an arc (tangential), so a tangential neighbour gains
    // coverage while a point further out along the same radius does not.
    let w = 41;
    let cx = 20usize;
    let cy = 20usize;
    // Impulse directly to the right of the centre (on the +x axis, radius 12).
    let mut buf = impulse(w, w, cx + 12, cy);
    radial_blur(&mut buf, w, w, [0.5, 0.5], RadialKind::Spin, 40.0);
    // Spin sweeps the point up/down (tangent to the circle) — a pixel above the
    // original lands on the swept arc.
    assert!(
        buf[(cy - 3) * w + (cx + 12)][3] > 0.0 || buf[(cy + 3) * w + (cx + 12)][3] > 0.0,
        "spin should smear tangentially (around the centre)"
    );
    // Radius is ~preserved: a point much further out along the +x ray stays empty.
    assert_eq!(
        buf[cy * w + (cx + 18)][3],
        0.0,
        "spin should not push coverage radially outward"
    );
}

#[test]
fn radial_zoom_blurs_radially_not_tangentially() {
    // A zoom blur about the centre sweeps samples *along the ray*: an off-axis
    // impulse smears toward/away from the centre (radial), so a point further out
    // along the same +x ray gains coverage while a tangential neighbour does not.
    let w = 41;
    let cx = 20usize;
    let cy = 20usize;
    let mut buf = impulse(w, w, cx + 12, cy);
    radial_blur(&mut buf, w, w, [0.5, 0.5], RadialKind::Zoom, 0.3);
    // Radial smear: a pixel further out along +x (away from centre) gains alpha.
    assert!(
        buf[cy * w + (cx + 15)][3] > 0.0 || buf[cy * w + (cx + 9)][3] > 0.0,
        "zoom should smear radially (along the ray)"
    );
    // Tangential neighbours (same radius, rotated) stay empty.
    assert_eq!(
        buf[(cy - 4) * w + (cx + 12)][3],
        0.0,
        "zoom should not smear tangentially"
    );
}

#[test]
fn radial_blur_zero_amount_is_noop() {
    let w = 16;
    let mut buf = square(w, w, 8, 8, 2);
    let orig = buf.clone();
    radial_blur(&mut buf, w, w, [0.5, 0.5], RadialKind::Spin, 0.0);
    assert_eq!(buf, orig);
    radial_blur(&mut buf, w, w, [0.5, 0.5], RadialKind::Zoom, 0.0);
    assert_eq!(buf, orig);
}

#[test]
fn blur_passes_are_deterministic() {
    // Re-running each blur on identical input yields byte-identical output (pure,
    // no time / IO / RNG).
    let mk = || square(24, 24, 12, 12, 4);
    let run = |mut b: Vec<[f32; 4]>, f: &dyn Fn(&mut Vec<[f32; 4]>)| {
        f(&mut b);
        b
    };
    let a = run(mk(), &|b| box_blur(b, 24, 24, 3.0, 2, false));
    let b = run(mk(), &|b| box_blur(b, 24, 24, 3.0, 2, false));
    assert_eq!(a, b, "box blur deterministic");
    let a = run(mk(), &|b| directional_blur(b, 24, 24, 35.0, 10.0));
    let b = run(mk(), &|b| directional_blur(b, 24, 24, 35.0, 10.0));
    assert_eq!(a, b, "directional blur deterministic");
    let a = run(mk(), &|b| radial_blur(b, 24, 24, [0.5, 0.5], RadialKind::Spin, 30.0));
    let b = run(mk(), &|b| radial_blur(b, 24, 24, [0.5, 0.5], RadialKind::Spin, 30.0));
    assert_eq!(a, b, "radial blur deterministic");
}

#[test]
fn new_blur_apply_ignores_empty_buffer() {
    // Degenerate sizes are a no-op (no panic) for every new blur.
    let mut empty: Vec<[f32; 4]> = Vec::new();
    SpatialEffect::BoxBlur {
        radius: 3.0,
        iterations: 2,
        repeat_edge: false,
    }
    .apply(&mut empty, 0, 0);
    SpatialEffect::DirectionalBlur {
        angle: 30.0,
        length: 10.0,
    }
    .apply(&mut empty, 0, 0);
    SpatialEffect::RadialBlur {
        center: [0.5, 0.5],
        kind: RadialKind::Zoom,
        amount: 0.2,
    }
    .apply(&mut empty, 0, 0);
    assert!(empty.is_empty());
}
