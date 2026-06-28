//! Unit tests for the Phase 6 retouching algorithms in `app_state::healing`.
//! Split out of `healing.rs` (mechanical move to keep that file under the
//! 1000-line rule); the only change is rewriting the `use super::*` imports to
//! `use crate::app_state::healing::...` since the tests now sit one module
//! level deeper.

#[cfg(test)]
mod tests {
    use crate::app_state::healing::{
        bilinear_sample, brush_coverage, clone_stamp, heal_region, patch_fill, poisson_blend,
        red_eye_correct,
    };
    use crate::app_state::{Action, App};

    /// Premultiply a straight RGBA color.
    fn premul(c: [f32; 4]) -> [f32; 4] {
        [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
    }

    /// A flat `w×h` premultiplied buffer of straight color `c`.
    fn flat(w: u32, h: u32, c: [f32; 4]) -> Vec<f32> {
        let p = premul(c);
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&p);
        }
        v
    }

    fn idx(w: u32, x: u32, y: u32) -> usize {
        ((y * w + x) * 4) as usize
    }

    // ---- Healing brush / Poisson ------------------------------------------

    #[test]
    fn poisson_constant_offset_matches_dest_tone() {
        // A uniformly brighter source must heal back to the destination tone:
        // the membrane absorbs the constant offset — the signature heal property.
        let (w, h) = (12u32, 12u32);
        let dest = flat(w, h, [0.5, 0.5, 0.5, 1.0]);
        let src = flat(w, h, [0.85, 0.85, 0.85, 1.0]); // +0.35 everywhere
        let mut mask = vec![false; (w * h) as usize];
        for y in 3..9 {
            for x in 3..9 {
                mask[(y * w + x) as usize] = true;
            }
        }
        let out = poisson_blend(&dest, &src, &mask, w, h, 500);
        let c = idx(w, 6, 6);
        assert!(
            (out[c] - 0.5).abs() < 0.03,
            "healed center should match dest tone 0.5, got {}",
            out[c]
        );
    }

    #[test]
    fn poisson_interior_continuity_at_seam() {
        // No hard edge: a masked pixel adjacent to the boundary stays close to the
        // neighbouring destination value (seam continuity).
        let (w, h) = (12u32, 12u32);
        let dest = flat(w, h, [0.4, 0.4, 0.4, 1.0]);
        let src = flat(w, h, [0.9, 0.9, 0.9, 1.0]);
        let mut mask = vec![false; (w * h) as usize];
        for y in 3..9 {
            for x in 3..9 {
                mask[(y * w + x) as usize] = true;
            }
        }
        let out = poisson_blend(&dest, &src, &mask, w, h, 500);
        // First masked pixel (3,5) vs its unmasked neighbour (2,5).
        let inside = out[idx(w, 3, 5)];
        let neighbour = out[idx(w, 2, 5)];
        assert!(
            (inside - neighbour).abs() < 0.08,
            "seam discontinuity: inside {inside} vs neighbour {neighbour}"
        );
    }

    #[test]
    fn poisson_outside_mask_untouched() {
        let (w, h) = (8u32, 8u32);
        let dest = flat(w, h, [0.3, 0.3, 0.3, 1.0]);
        let src = flat(w, h, [0.9, 0.1, 0.1, 1.0]);
        let mut mask = vec![false; (w * h) as usize];
        mask[(4 * w + 4) as usize] = true;
        let out = poisson_blend(&dest, &src, &mask, w, h, 50);
        // A corner far from the mask must equal dest exactly.
        assert_eq!(out[0], dest[0]);
        assert_eq!(out[1], dest[1]);
    }

    #[test]
    fn poisson_empty_mask_is_identity() {
        let (w, h) = (6u32, 6u32);
        let dest = flat(w, h, [0.2, 0.6, 0.9, 1.0]);
        let src = flat(w, h, [0.0, 0.0, 0.0, 1.0]);
        let mask = vec![false; (w * h) as usize];
        let out = poisson_blend(&dest, &src, &mask, w, h, 30);
        assert_eq!(out, dest);
    }

    #[test]
    fn poisson_alpha_preserved() {
        let (w, h) = (8u32, 8u32);
        let dest = flat(w, h, [0.5, 0.5, 0.5, 0.5]);
        let src = flat(w, h, [0.9, 0.9, 0.9, 0.5]);
        let mut mask = vec![false; (w * h) as usize];
        mask[(4 * w + 4) as usize] = true;
        let out = poisson_blend(&dest, &src, &mask, w, h, 40);
        assert!((out[idx(w, 4, 4) + 3] - 0.5).abs() < 1e-6, "alpha changed");
    }

    #[test]
    fn poisson_is_deterministic() {
        let (w, h) = (10u32, 10u32);
        let dest = flat(w, h, [0.5, 0.4, 0.3, 1.0]);
        let src = flat(w, h, [0.7, 0.6, 0.5, 1.0]);
        let mut mask = vec![false; (w * h) as usize];
        for y in 2..8 {
            for x in 2..8 {
                mask[(y * w + x) as usize] = true;
            }
        }
        let a = poisson_blend(&dest, &src, &mask, w, h, 120);
        let b = poisson_blend(&dest, &src, &mask, w, h, 120);
        assert_eq!(a, b);
    }

    #[test]
    fn heal_region_preserves_gradient_direction() {
        // Flat dest; the source area carries a left→right gradient. After heal the
        // interior keeps the relative variation but the tone tracks the dest.
        let (w, h) = (24u32, 8u32);
        let mut px = flat(w, h, [0.5, 0.5, 0.5, 1.0]);
        // Paint a L→R ramp into the LEFT half (the heal source).
        for y in 0..h {
            for x in 0..12 {
                let g = x as f32 / 11.0;
                let p = idx(w, x, y);
                px[p] = g;
                px[p + 1] = g;
                px[p + 2] = g;
            }
        }
        // Heal a circle in the (flat 0.5) right half, sourced from the ramp left.
        let out = heal_region(&px, w, h, [6.0, 4.0], [18.0, 4.0], 3.0, 600);
        let left = out[idx(w, 16, 4)];
        let right = out[idx(w, 20, 4)];
        assert!(right > left, "gradient direction lost: {left} !< {right}");
    }

    #[test]
    fn heal_region_zero_radius_is_identity() {
        let (w, h) = (8u32, 8u32);
        let px = flat(w, h, [0.6, 0.2, 0.1, 1.0]);
        let out = heal_region(&px, w, h, [2.0, 2.0], [5.0, 5.0], 0.0, 50);
        assert_eq!(out, px);
    }

    #[test]
    fn heal_region_zero_offset_is_identity() {
        let (w, h) = (8u32, 8u32);
        let px = flat(w, h, [0.6, 0.2, 0.1, 1.0]);
        let out = heal_region(&px, w, h, [4.0, 4.0], [4.0, 4.0], 3.0, 50);
        assert_eq!(out, px);
    }

    // ---- Clone stamp -------------------------------------------------------

    #[test]
    fn clone_copies_source_at_center() {
        let (w, h) = (24u32, 24u32);
        let a = [0.8, 0.2, 0.1, 1.0];
        let b = [0.1, 0.1, 0.1, 1.0];
        let src = flat(w, h, a);
        let dst = flat(w, h, b);
        let out = clone_stamp(&dst, &src, w, h, [0.0, 0.0], [8.5, 8.5], 8.0, 1.0, 1.0);
        let c = idx(w, 8, 8);
        let ap = premul(a);
        assert!((out[c] - ap[0]).abs() < 1e-4, "center not cloned: {}", out[c]);
        assert!((out[c + 1] - ap[1]).abs() < 1e-4);
    }

    #[test]
    fn clone_outside_radius_untouched() {
        let (w, h) = (24u32, 24u32);
        let src = flat(w, h, [0.8, 0.2, 0.1, 1.0]);
        let dst = flat(w, h, [0.1, 0.1, 0.1, 1.0]);
        let out = clone_stamp(&dst, &src, w, h, [0.0, 0.0], [8.5, 8.5], 5.0, 1.0, 1.0);
        // A far corner is well outside the brush → identical to dst.
        assert_eq!(out[idx(w, 23, 23)], dst[idx(w, 23, 23)]);
    }

    #[test]
    fn clone_opacity_half_blends_halfway() {
        let (w, h) = (24u32, 24u32);
        let a = [0.8, 0.8, 0.8, 1.0];
        let b = [0.2, 0.2, 0.2, 1.0];
        let src = flat(w, h, a);
        let dst = flat(w, h, b);
        // Hard brush so center coverage is exactly 1 → blend = 0.5.
        let out = clone_stamp(&dst, &src, w, h, [0.0, 0.0], [8.5, 8.5], 8.0, 1.0, 0.5);
        let c = idx(w, 8, 8);
        assert!((out[c] - 0.5).abs() < 1e-4, "expected 0.5 blend, got {}", out[c]);
    }

    #[test]
    fn clone_opacity_zero_is_identity() {
        let (w, h) = (16u32, 16u32);
        let src = flat(w, h, [0.9, 0.1, 0.1, 1.0]);
        let dst = flat(w, h, [0.2, 0.3, 0.4, 1.0]);
        let out = clone_stamp(&dst, &src, w, h, [0.0, 0.0], [8.5, 8.5], 5.0, 1.0, 0.0);
        assert_eq!(out, dst);
    }

    #[test]
    fn clone_soft_falloff_is_partial_mid_ring() {
        // hardness 0 → coverage at half-radius is exactly smoothstep(0.5)=0.5.
        let (w, h) = (32u32, 32u32);
        let a = [1.0, 1.0, 1.0, 1.0];
        let b = [0.0, 0.0, 0.0, 1.0];
        let src = flat(w, h, a);
        let dst = flat(w, h, b);
        let out = clone_stamp(&dst, &src, w, h, [0.0, 0.0], [8.5, 8.5], 8.0, 0.0, 1.0);
        // Pixel (12,8): dx = 4 px from center → d/r = 0.5 → coverage 0.5.
        let p = idx(w, 12, 8);
        assert!(
            (out[p] - 0.5).abs() < 0.02,
            "mid-ring coverage should be ~0.5, got {}",
            out[p]
        );
        // Center is full coverage → equals source.
        assert!((out[idx(w, 8, 8)] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn clone_is_deterministic() {
        let (w, h) = (20u32, 20u32);
        let src = flat(w, h, [0.7, 0.5, 0.3, 1.0]);
        let dst = flat(w, h, [0.1, 0.2, 0.3, 1.0]);
        let o1 = clone_stamp(&dst, &src, w, h, [3.0, -2.0], [10.5, 10.5], 6.0, 0.4, 0.8);
        let o2 = clone_stamp(&dst, &src, w, h, [3.0, -2.0], [10.5, 10.5], 6.0, 0.4, 0.8);
        assert_eq!(o1, o2);
    }

    // ---- Red-eye -----------------------------------------------------------

    #[test]
    fn redeye_reduces_pupil_red() {
        let (w, h) = (8u32, 8u32);
        let mut px = flat(w, h, [0.1, 0.1, 0.1, 1.0]);
        let p = idx(w, 4, 4);
        // Strong pupil red.
        px[p] = 0.7;
        px[p + 1] = 0.08;
        px[p + 2] = 0.08;
        let out = red_eye_correct(&px, w, h, 4.5, 4.5, 3.0, 1.0);
        assert!(out[p] < 0.4, "pupil red not reduced: {}", out[p]);
        // Roughly neutralized (r no longer dominates).
        assert!((out[p] - out[p + 1]).abs() < 0.2, "still red-dominant");
    }

    #[test]
    fn redeye_leaves_skin_untouched() {
        // Skin: red leads only mildly → fails the pupil ratio gate → unchanged.
        let (w, h) = (8u32, 8u32);
        let mut px = flat(w, h, [0.2, 0.2, 0.2, 1.0]);
        let p = idx(w, 4, 4);
        px[p] = 0.8;
        px[p + 1] = 0.55;
        px[p + 2] = 0.45;
        let out = red_eye_correct(&px, w, h, 4.5, 4.5, 3.0, 1.0);
        assert_eq!(out[p], 0.8, "skin red changed");
        assert_eq!(out[p + 1], 0.55);
        assert_eq!(out[p + 2], 0.45);
    }

    #[test]
    fn redeye_neutral_untouched() {
        let (w, h) = (8u32, 8u32);
        let px = flat(w, h, [0.5, 0.5, 0.5, 1.0]);
        let out = red_eye_correct(&px, w, h, 4.0, 4.0, 4.0, 1.0);
        for (a, b) in out.iter().zip(px.iter()) {
            assert!((a - b).abs() < 1e-6, "neutral changed");
        }
    }

    #[test]
    fn redeye_strength_zero_is_identity() {
        let (w, h) = (8u32, 8u32);
        let mut px = flat(w, h, [0.1, 0.1, 0.1, 1.0]);
        let p = idx(w, 4, 4);
        px[p] = 0.7;
        px[p + 1] = 0.05;
        px[p + 2] = 0.05;
        let out = red_eye_correct(&px, w, h, 4.5, 4.5, 3.0, 0.0);
        assert_eq!(out, px);
    }

    #[test]
    fn redeye_outside_radius_untouched() {
        let (w, h) = (16u32, 16u32);
        let mut px = flat(w, h, [0.1, 0.1, 0.1, 1.0]);
        let p = 0usize; // corner pupil-red pixel
        px[p] = 0.7;
        px[p + 1] = 0.05;
        px[p + 2] = 0.05;
        let out = red_eye_correct(&px, w, h, 12.0, 12.0, 3.0, 1.0);
        assert_eq!(out[p], 0.7, "outside radius changed");
    }

    #[test]
    fn redeye_preserves_premultiplied_alpha() {
        let (w, h) = (8u32, 8u32);
        let mut px = flat(w, h, [0.1, 0.1, 0.1, 1.0]);
        let p = idx(w, 4, 4);
        // Half-transparent pupil red, stored premultiplied.
        px[p] = 0.7 * 0.5;
        px[p + 1] = 0.05 * 0.5;
        px[p + 2] = 0.05 * 0.5;
        px[p + 3] = 0.5;
        let out = red_eye_correct(&px, w, h, 4.5, 4.5, 3.0, 1.0);
        assert!((out[p + 3] - 0.5).abs() < 1e-6, "alpha changed");
        assert!(out[p] < 0.35, "premult red not reduced: {}", out[p]);
    }

    // ---- Content-aware patch ----------------------------------------------

    #[test]
    fn patch_fill_zero_radius_is_identity() {
        let (w, h) = (8u32, 8u32);
        let px = flat(w, h, [0.3, 0.7, 0.2, 1.0]);
        let out = patch_fill(&px, w, h, 4.0, 4.0, 0.0, 1);
        assert_eq!(out, px);
    }

    #[test]
    fn patch_fill_samples_local_texture() {
        // Two-tone image: left half A, right half B. A hole inside the LEFT half
        // must be filled from A (the surrounding texture), not B.
        let (w, h) = (24u32, 24u32);
        let a = [0.85, 0.15, 0.10, 1.0];
        let b = [0.10, 0.20, 0.85, 1.0];
        let mut px = flat(w, h, a);
        for y in 0..h {
            for x in 12..w {
                let p = idx(w, x, y);
                let bp = premul(b);
                px[p] = bp[0];
                px[p + 1] = bp[1];
                px[p + 2] = bp[2];
            }
        }
        let out = patch_fill(&px, w, h, 6.0, 12.0, 3.0, 7);
        let c = idx(w, 6, 12);
        let ap = premul(a);
        assert!((out[c] - ap[0]).abs() < 0.05, "hole not filled from A: r={}", out[c]);
        assert!((out[c + 2] - ap[2]).abs() < 0.05, "picked up B's blue: b={}", out[c + 2]);
    }

    #[test]
    fn patch_fill_changes_masked_pixels() {
        // A distinct blemish over flat background should be replaced.
        let (w, h) = (24u32, 24u32);
        let mut px = flat(w, h, [0.5, 0.5, 0.5, 1.0]);
        let r2 = 3.0f32 * 3.0;
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 + 0.5 - 12.0;
                let dy = y as f32 + 0.5 - 12.0;
                if dx * dx + dy * dy <= r2 {
                    let p = idx(w, x, y);
                    px[p] = 0.95;
                    px[p + 1] = 0.05;
                    px[p + 2] = 0.05;
                }
            }
        }
        let out = patch_fill(&px, w, h, 12.0, 12.0, 3.0, 3);
        let c = idx(w, 12, 12);
        assert!(out[c] < 0.7, "blemish center not healed toward bg: {}", out[c]);
    }

    #[test]
    fn patch_fill_is_deterministic() {
        let (w, h) = (20u32, 20u32);
        let mut px = flat(w, h, [0.4, 0.6, 0.3, 1.0]);
        // A little texture so candidates differ.
        for y in 0..h {
            for x in 0..w {
                if (x + y) % 3 == 0 {
                    let p = idx(w, x, y);
                    px[p] += 0.05;
                }
            }
        }
        let o1 = patch_fill(&px, w, h, 10.0, 10.0, 3.0, 42);
        let o2 = patch_fill(&px, w, h, 10.0, 10.0, 3.0, 42);
        assert_eq!(o1, o2);
    }

    // ---- Pure helpers ------------------------------------------------------

    #[test]
    fn brush_coverage_endpoints() {
        assert!((brush_coverage(0.0, 10.0, 0.5) - 1.0).abs() < 1e-6);
        assert_eq!(brush_coverage(10.0, 10.0, 0.5), 0.0);
        // hardness 0, half radius → smoothstep(0.5) = 0.5 coverage.
        assert!((brush_coverage(5.0, 10.0, 0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bilinear_sample_midpoint() {
        // 2×1 buffer: left = 0, right = 1; sampling x=0.5 → 0.5.
        let buf = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let s = bilinear_sample(&buf, 2, 1, 0.5, 0.0);
        assert!((s[0] - 0.5).abs() < 1e-6, "got {}", s[0]);
    }

    // ---- App dispatch (no panic with a fresh document) ---------------------

    #[test]
    fn heal_brush_action_runs() {
        let mut app = App::new();
        app.apply(Action::HealBrush {
            src_center: [4.0, 4.0],
            dst_center: [10.0, 10.0],
            radius: 4.0,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn clone_stamp_action_runs() {
        let mut app = App::new();
        app.apply(Action::CloneStampDab {
            src_center: [4.0, 4.0],
            dst_center: [10.0, 10.0],
            radius: 4.0,
            opacity: 0.8,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn remove_red_eye_action_runs() {
        let mut app = App::new();
        app.apply(Action::RemoveRedEye {
            center: [10.0, 10.0],
            radius: 4.0,
            strength: 1.0,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn content_aware_patch_action_runs() {
        let mut app = App::new();
        app.apply(Action::ContentAwarePatch {
            center: [10.0, 10.0],
            radius: 4.0,
            seed: 1,
        });
        let _ = app.status_message.clone();
    }
}
