//! Unit tests for the Phase 6 Liquify forward-warp mesh in `app_state::liquify`.
//! Split out of `liquify.rs` to keep both files under the 1000-line rule (the same
//! `healing.rs` / `healing_tests.rs` convention); imports reach one module level
//! up via `crate::app_state::liquify::...`.

#[cfg(test)]
mod tests {
    use crate::app_state::liquify::{
        apply_warp, bloat_pucker, falloff, push, reconstruct, twirl, WarpField,
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

    /// A vertical-edge buffer: bright (red=1) for `x < edge`, dark elsewhere.
    fn vedge(w: u32, h: u32, edge: u32) -> Vec<f32> {
        let mut v = flat(w, h, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..h {
            for x in 0..edge.min(w) {
                v[idx(w, x, y)] = 1.0;
            }
        }
        v
    }

    // ---- Falloff -----------------------------------------------------------

    #[test]
    fn falloff_one_at_center() {
        assert!((falloff(0.0, 10.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn falloff_zero_at_and_outside_radius() {
        assert_eq!(falloff(10.0, 10.0), 0.0);
        assert_eq!(falloff(15.0, 10.0), 0.0);
        assert_eq!(falloff(5.0, 0.0), 0.0); // non-positive radius
    }

    #[test]
    fn falloff_monotonic_decreasing() {
        let a = falloff(2.0, 10.0);
        let b = falloff(5.0, 10.0);
        let c = falloff(8.0, 10.0);
        assert!(a > b && b > c, "falloff not decreasing: {a} {b} {c}");
        assert!((0.0..=1.0).contains(&b));
    }

    // ---- WarpField ---------------------------------------------------------

    #[test]
    fn new_field_is_identity() {
        let f = WarpField::new(8, 6);
        assert!(f.is_identity());
        assert_eq!(f.max_magnitude(), 0.0);
        assert_eq!(f.disp.len(), 48);
    }

    #[test]
    fn push_breaks_identity_and_sets_magnitude() {
        let mut f = WarpField::new(20, 20);
        push(&mut f, [10.0, 10.0], 6.0, [4.0, 0.0], 1.0);
        assert!(!f.is_identity());
        assert!(f.max_magnitude() > 0.0);
    }

    // ---- apply_warp: identity round-trip + clamping ------------------------

    #[test]
    fn apply_warp_zero_field_is_identity() {
        let (w, h) = (16u32, 12u32);
        // A non-uniform buffer so an accidental smear would show.
        let mut src = flat(w, h, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..h {
            for x in 0..w {
                src[idx(w, x, y)] = (x as f32) / (w as f32);
                src[idx(w, x, y) + 1] = (y as f32) / (h as f32);
            }
        }
        let f = WarpField::new(w, h);
        let out = apply_warp(&src, &f);
        assert_eq!(out, src, "zero field must be a bit-exact copy (round-trip)");
    }

    #[test]
    fn apply_warp_clamps_huge_displacement() {
        // Every pixel pulls from a far-off (-1e6,-1e6) source → clamps to (0,0).
        let (w, h) = (8u32, 8u32);
        let mut src = flat(w, h, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..h {
            for x in 0..w {
                src[idx(w, x, y)] = (x + y) as f32; // distinct per pixel
            }
        }
        let mut f = WarpField::new(w, h);
        for d in f.disp.iter_mut() {
            *d = [1e6, 1e6];
        }
        let out = apply_warp(&src, &f); // must not panic / index OOB
        assert_eq!(out.len(), src.len());
        let corner = src[idx(w, 0, 0)];
        assert!((out[idx(w, 7, 7)] - corner).abs() < 1e-4, "not edge-clamped");
        assert!((out[idx(w, 3, 5)] - corner).abs() < 1e-4);
    }

    #[test]
    fn apply_warp_size_mismatch_is_copy() {
        let f = WarpField::new(4, 4);
        let src = vec![0.5f32; 4 * 4 * 4 - 3]; // wrong length
        let out = apply_warp(&src, &f);
        assert_eq!(out, src);
    }

    // ---- Push: drags a feature along the drag direction --------------------

    #[test]
    fn push_moves_edge_in_drag_direction() {
        // Vertical edge at x=20; push the region right by +5 → the edge moves right,
        // so a pixel just right of the original edge becomes bright.
        let (w, h) = (40u32, 40u32);
        let src = vedge(w, h, 20);
        let mut f = WarpField::new(w, h);
        push(&mut f, [20.0, 20.0], 12.0, [5.0, 0.0], 1.0);
        let out = apply_warp(&src, &f);
        assert!(
            out[idx(w, 22, 20)] > 0.5,
            "edge did not move right: {}",
            out[idx(w, 22, 20)]
        );
    }

    #[test]
    fn push_outside_radius_leaves_field_zero() {
        let mut f = WarpField::new(40, 40);
        push(&mut f, [20.0, 20.0], 8.0, [5.0, 0.0], 1.0);
        // A corner is well outside the 8px brush → exactly zero displacement.
        let p = 0usize;
        assert_eq!(f.disp[p], [0.0, 0.0]);
        let far = (39 * 40 + 39) as usize;
        assert_eq!(f.disp[far], [0.0, 0.0]);
    }

    #[test]
    fn push_accumulates() {
        let mut f = WarpField::new(20, 20);
        push(&mut f, [10.0, 10.0], 6.0, [2.0, 0.0], 1.0);
        let once = f.disp[(10 * 20 + 10) as usize][0];
        push(&mut f, [10.0, 10.0], 6.0, [2.0, 0.0], 1.0);
        let twice = f.disp[(10 * 20 + 10) as usize][0];
        assert!((twice - 2.0 * once).abs() < 1e-5, "push not additive");
    }

    #[test]
    fn push_strength_zero_is_noop() {
        let mut f = WarpField::new(20, 20);
        push(&mut f, [10.0, 10.0], 6.0, [4.0, 4.0], 0.0);
        assert!(f.is_identity());
    }

    // ---- Bloat / pucker ----------------------------------------------------

    #[test]
    fn bloat_field_points_outward() {
        let mut f = WarpField::new(40, 40);
        let c = [20.0, 20.0];
        bloat_pucker(&mut f, c, 12.0, 0.3);
        // A pixel right of center: displacement has positive dot with (p-center).
        for &(x, y) in &[(26u32, 20u32), (20, 26), (24, 24)] {
            let p = (y * 40 + x) as usize;
            let rel = [x as f32 + 0.5 - c[0], y as f32 + 0.5 - c[1]];
            let dot = f.disp[p][0] * rel[0] + f.disp[p][1] * rel[1];
            assert!(dot > 0.0, "bloat not outward at {x},{y}: dot={dot}");
        }
    }

    #[test]
    fn pucker_field_points_inward() {
        let mut f = WarpField::new(40, 40);
        let c = [20.0, 20.0];
        bloat_pucker(&mut f, c, 12.0, -0.3);
        for &(x, y) in &[(26u32, 20u32), (20, 26), (16, 16)] {
            let p = (y * 40 + x) as usize;
            let rel = [x as f32 + 0.5 - c[0], y as f32 + 0.5 - c[1]];
            let dot = f.disp[p][0] * rel[0] + f.disp[p][1] * rel[1];
            assert!(dot < 0.0, "pucker not inward at {x},{y}: dot={dot}");
        }
    }

    #[test]
    fn bloat_increases_distance_pucker_decreases() {
        // The warped position of q is q + D[q] (forward displacement). Bloat pushes
        // it farther from center; pucker pulls it closer.
        let c = [20.0f32, 20.0];
        let q = (26u32, 20u32);
        let qp = [q.0 as f32 + 0.5, q.1 as f32 + 0.5];
        let r0 = ((qp[0] - c[0]).powi(2) + (qp[1] - c[1]).powi(2)).sqrt();
        let pidx = (q.1 * 40 + q.0) as usize;

        let mut fb = WarpField::new(40, 40);
        bloat_pucker(&mut fb, c, 12.0, 0.3);
        let nb = [qp[0] + fb.disp[pidx][0], qp[1] + fb.disp[pidx][1]];
        let rb = ((nb[0] - c[0]).powi(2) + (nb[1] - c[1]).powi(2)).sqrt();
        assert!(rb > r0, "bloat must increase distance: {rb} !> {r0}");

        let mut fp = WarpField::new(40, 40);
        bloat_pucker(&mut fp, c, 12.0, -0.3);
        let np = [qp[0] + fp.disp[pidx][0], qp[1] + fp.disp[pidx][1]];
        let rp = ((np[0] - c[0]).powi(2) + (np[1] - c[1]).powi(2)).sqrt();
        assert!(rp < r0, "pucker must decrease distance: {rp} !< {r0}");
    }

    #[test]
    fn bloat_amount_zero_is_noop() {
        let mut f = WarpField::new(20, 20);
        bloat_pucker(&mut f, [10.0, 10.0], 6.0, 0.0);
        assert!(f.is_identity());
    }

    // ---- Twirl -------------------------------------------------------------

    #[test]
    fn twirl_adds_tangential_component() {
        // At a pixel directly right of center (rel ~ (R,0)), twirl injects a
        // perpendicular (y) displacement; CW and CCW give opposite signs.
        let c = [20.0f32, 20.0];
        let p = (26u32, 20u32);
        let pidx = (p.1 * 40 + p.0) as usize;

        let mut ccw = WarpField::new(40, 40);
        twirl(&mut ccw, c, 14.0, 0.6, true);
        let mut cw = WarpField::new(40, 40);
        twirl(&mut cw, c, 14.0, 0.6, false);

        let yc = ccw.disp[pidx][1];
        let yw = cw.disp[pidx][1];
        assert!(yc.abs() > 1e-3, "twirl produced no rotation: {yc}");
        assert!(yc * yw < 0.0, "CW/CCW not opposite: {yc} vs {yw}");
    }

    #[test]
    fn twirl_rotates_feature_off_axis() {
        // A feature on the +x axis from center picks up a vertical shift after twirl
        // (its warped position rotates around the center).
        let c = [20.0f32, 20.0];
        let q = (28u32, 20u32);
        let qp = [q.0 as f32 + 0.5, q.1 as f32 + 0.5];
        let pidx = (q.1 * 40 + q.0) as usize;
        let mut f = WarpField::new(40, 40);
        twirl(&mut f, c, 14.0, 0.8, true);
        let new_y = qp[1] + f.disp[pidx][1];
        assert!((new_y - qp[1]).abs() > 0.5, "feature did not rotate off axis");
        // Distance from center is roughly preserved by a rotation.
        let new_x = qp[0] + f.disp[pidx][0];
        let r0 = (qp[0] - c[0]).abs();
        let r1 = (((new_x - c[0]).powi(2)) + ((new_y - c[1]).powi(2))).sqrt();
        assert!((r1 - r0).abs() < 0.5 * r0, "rotation grossly changed radius");
    }

    #[test]
    fn twirl_angle_zero_is_noop() {
        let mut f = WarpField::new(20, 20);
        twirl(&mut f, [10.0, 10.0], 8.0, 0.0, true);
        assert!(f.is_identity());
    }

    // ---- Reconstruct -------------------------------------------------------

    #[test]
    fn reconstruct_decays_field_magnitude() {
        let mut f = WarpField::new(40, 40);
        push(&mut f, [20.0, 20.0], 12.0, [6.0, 3.0], 1.0);
        let before = f.max_magnitude();
        reconstruct(&mut f, [20.0, 20.0], 14.0, 0.5);
        let after = f.max_magnitude();
        assert!(after < before, "reconstruct did not decay: {after} !< {before}");
        assert!(after > 0.0, "half reconstruct should not fully clear");
    }

    #[test]
    fn reconstruct_full_clears_center() {
        let mut f = WarpField::new(40, 40);
        push(&mut f, [20.0, 20.0], 12.0, [6.0, 3.0], 1.0);
        let pidx = (20 * 40 + 20) as usize;
        let before = f.disp[pidx][0].abs();
        assert!(before > 0.0);
        reconstruct(&mut f, [20.0, 20.0], 14.0, 1.0);
        // amount=1 at near-center falloff → scale ~0: dropped to <2% of original.
        let after = f.disp[pidx][0].abs();
        assert!(
            after < 0.02 * before,
            "center not cleared: {after} (before {before})"
        );
    }

    #[test]
    fn reconstruct_amount_zero_is_noop() {
        let mut f = WarpField::new(20, 20);
        push(&mut f, [10.0, 10.0], 6.0, [3.0, 0.0], 1.0);
        let snap = f.clone();
        reconstruct(&mut f, [10.0, 10.0], 8.0, 0.0);
        assert_eq!(f, snap);
    }

    #[test]
    fn reconstruct_repeated_drives_toward_zero() {
        let mut f = WarpField::new(40, 40);
        push(&mut f, [20.0, 20.0], 12.0, [6.0, 3.0], 1.0);
        for _ in 0..40 {
            reconstruct(&mut f, [20.0, 20.0], 18.0, 1.0);
        }
        assert!(f.max_magnitude() < 0.2, "did not converge toward identity");
    }

    // ---- Determinism -------------------------------------------------------

    #[test]
    fn push_apply_is_deterministic() {
        let (w, h) = (32u32, 32u32);
        let src = vedge(w, h, 16);
        let mut a = WarpField::new(w, h);
        push(&mut a, [16.0, 16.0], 10.0, [4.0, -2.0], 0.8);
        let mut b = WarpField::new(w, h);
        push(&mut b, [16.0, 16.0], 10.0, [4.0, -2.0], 0.8);
        assert_eq!(a, b);
        assert_eq!(apply_warp(&src, &a), apply_warp(&src, &b));
    }

    #[test]
    fn twirl_is_deterministic() {
        let mut a = WarpField::new(30, 30);
        twirl(&mut a, [15.0, 15.0], 12.0, 0.7, true);
        let mut b = WarpField::new(30, 30);
        twirl(&mut b, [15.0, 15.0], 12.0, 0.7, true);
        assert_eq!(a, b);
    }

    // ---- App dispatch (no panic with a fresh / headless document) ----------

    #[test]
    fn liquify_push_action_runs() {
        let mut app = App::new();
        app.apply(Action::LiquifyPush {
            center: [10.0, 10.0],
            radius: 8.0,
            drag: [4.0, 0.0],
            strength: 0.5,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn liquify_bloat_action_runs() {
        let mut app = App::new();
        app.apply(Action::LiquifyBloat {
            center: [10.0, 10.0],
            radius: 8.0,
            amount: 0.3,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn liquify_pucker_action_runs() {
        let mut app = App::new();
        app.apply(Action::LiquifyPucker {
            center: [10.0, 10.0],
            radius: 8.0,
            amount: 0.3,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn liquify_twirl_action_runs() {
        let mut app = App::new();
        app.apply(Action::LiquifyTwirl {
            center: [10.0, 10.0],
            radius: 8.0,
            angle: 0.5,
            ccw: true,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn liquify_reconstruct_action_runs() {
        let mut app = App::new();
        app.apply(Action::LiquifyReconstruct {
            center: [10.0, 10.0],
            radius: 8.0,
            amount: 0.5,
        });
        let _ = app.status_message.clone();
    }

    #[test]
    fn liquify_commit_and_reset_actions_run() {
        let mut app = App::new();
        app.apply(Action::LiquifyCommit);
        app.apply(Action::LiquifyReset);
        let _ = app.status_message.clone();
    }
}
