use super::*;

// --- Precomps (nested compositions) -------------------------------------

/// A full-frame solid comp of the given id and color (covers the whole frame so
/// a precomp sampling it sees the color edge-to-edge).
fn full_frame_comp(id: u64, color: [f32; 4]) -> Comp {
    let mut c = solid(color);
    c.id = id;
    c.layers[0].scale.set_key(0.0, 3.0); // cover the whole frame
    c
}

#[test]
fn precomp_renders_the_referenced_comps_content() {
    use crate::comp::{LayerKind, PrecompLayer};
    // Comp B: a full-frame green solid (id 2).
    let nested = full_frame_comp(2, [0.0, 1.0, 0.0, 1.0]);
    // Comp A (id 1): a single precomp layer referencing B, scaled to cover the
    // frame so its quad fills it.
    let mut host = solid([1.0, 1.0, 1.0, 1.0]);
    host.id = 1;
    host.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5, 0.5, 0.5, 1.0]);
        l.precomp = PrecompLayer::to(2);
        l.scale.set_key(0.0, 3.0); // cover the whole frame
        l
    };
    let comps = [host, nested];

    let mut cache = crate::comp::FrameCache::new();
    let f = render_frame_in_project(&comps, 1, 0.0, &mut cache);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "precomp center should be opaque (nested comp covers it)");
    assert!(g > 250, "nested green should show, got g={g}");
    assert!(r < 8 && b < 8, "center should be green, got ({r},{g},{b})");
}

#[test]
fn precomp_honors_time_offset() {
    use crate::comp::{Interp, LayerKind, PrecompLayer, Prop};
    // Comp B (id 2): a full-frame solid whose opacity ramps 0 -> 1 over [0,1].
    let mut nested = full_frame_comp(2, [1.0, 0.0, 0.0, 1.0]);
    nested.layers[0].track_mut(Prop::Opacity).set_key(0.0, 0.0);
    nested.layers[0].track_mut(Prop::Opacity).set_key(1.0, 1.0);
    nested.layers[0]
        .track_mut(Prop::Opacity)
        .set_interp(0.0, Interp::Linear);

    // Host precomp at t=0 with a +1.0s offset samples B at its end (opacity 1).
    let mut host = solid([1.0, 1.0, 1.0, 1.0]);
    host.id = 1;
    host.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
        l.precomp = PrecompLayer {
            source: Some(2),
            time_offset: 1.0,
        };
        l.scale.set_key(0.0, 3.0);
        l
    };
    let comps = [host, nested];
    let mut cache = crate::comp::FrameCache::new();
    let f = render_frame_in_project(&comps, 1, 0.0, &mut cache);
    let a = f.pixel(32, 32)[3];
    assert!(a > 250, "offset to B's end => opaque, got a={a}");
}

#[test]
fn precomp_cycle_guard_terminates() {
    use crate::comp::{LayerKind, PrecompLayer};
    // A -> B -> A: each comp is a precomp pointing at the other. Rendering must
    // terminate (the cycle guard refuses to re-enter a comp on the stack) rather
    // than recurse forever / overflow the stack.
    let mut a = solid([1.0, 1.0, 1.0, 1.0]);
    a.id = 1;
    a.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "A->B", [0.5; 4]);
        l.precomp = PrecompLayer::to(2);
        l.scale.set_key(0.0, 3.0);
        l
    };
    let mut b = solid([1.0, 1.0, 1.0, 1.0]);
    b.id = 2;
    b.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "B->A", [0.5; 4]);
        l.precomp = PrecompLayer::to(1);
        l.scale.set_key(0.0, 3.0);
        l
    };
    let comps = [a, b];
    let mut cache = crate::comp::FrameCache::new();
    // The assertion that matters is *that this returns* (no infinite recursion).
    let f = render_frame_in_project(&comps, 1, 0.0, &mut cache);
    // A renders B; B renders A which is on the stack -> guard breaks it (nothing).
    assert!(
        f.pixels.iter().all(|&px| px == 0),
        "a cyclic precomp pair should render nothing"
    );
}

#[test]
fn self_referential_precomp_renders_nothing() {
    use crate::comp::{LayerKind, PrecompLayer};
    // A comp whose only layer is a precomp pointing at itself: the cycle guard
    // (the comp is already on the stack) makes it render nothing.
    let mut a = solid([1.0, 1.0, 1.0, 1.0]);
    a.id = 1;
    a.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "self", [0.5; 4]);
        l.precomp = PrecompLayer::to(1);
        l.scale.set_key(0.0, 3.0);
        l
    };
    let comps = [a];
    let mut cache = crate::comp::FrameCache::new();
    let f = render_frame_in_project(&comps, 1, 0.0, &mut cache);
    assert!(f.pixels.iter().all(|&px| px == 0));
}

#[test]
fn precomp_missing_target_renders_nothing() {
    use crate::comp::{LayerKind, PrecompLayer};
    let mut host = solid([1.0, 1.0, 1.0, 1.0]);
    host.id = 1;
    host.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
        l.precomp = PrecompLayer::to(99); // no such comp
        l.scale.set_key(0.0, 3.0);
        l
    };
    let comps = [host];
    let mut cache = crate::comp::FrameCache::new();
    let f = render_frame_in_project(&comps, 1, 0.0, &mut cache);
    assert!(f.pixels.iter().all(|&px| px == 0));
}

#[test]
fn precomp_nests_two_levels_deep() {
    use crate::comp::{LayerKind, PrecompLayer};
    // C (id 3) is a green full-frame solid; B (id 2) is a precomp of C; A (id 1)
    // is a precomp of B. Rendering A should show C's green two levels down.
    let c = full_frame_comp(3, [0.0, 1.0, 0.0, 1.0]);
    let mut b = solid([1.0, 1.0, 1.0, 1.0]);
    b.id = 2;
    b.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "B->C", [0.5; 4]);
        l.precomp = PrecompLayer::to(3);
        l.scale.set_key(0.0, 3.0);
        l
    };
    let mut a = solid([1.0, 1.0, 1.0, 1.0]);
    a.id = 1;
    a.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "A->B", [0.5; 4]);
        l.precomp = PrecompLayer::to(2);
        l.scale.set_key(0.0, 3.0);
        l
    };
    let comps = [a, b, c];
    let mut cache = crate::comp::FrameCache::new();
    let f = render_frame_in_project(&comps, 1, 0.0, &mut cache);
    let [r, g, bch, alpha] = f.pixel(32, 32);
    assert_eq!(alpha, 255);
    assert!(g > 250 && r < 8 && bch < 8, "deep nest green, got ({r},{g},{bch})");
}

#[test]
fn single_comp_render_ignores_precomp() {
    use crate::comp::{LayerKind, PrecompLayer};
    // The single-comp `render_frame` entry has no project to resolve against, so
    // a precomp layer in it draws nothing (and doesn't panic / recurse).
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.id = 1;
    c.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
        l.precomp = PrecompLayer::to(2); // a sibling that isn't visible here
        l.scale.set_key(0.0, 3.0);
        l
    };
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&px| px == 0));
}

// --- Expressions drive the render path -------------------------------------

#[test]
fn position_expression_moves_coverage_in_render() {
    // An expression `value + 20` on X (with value defaulting to 0) shifts the
    // quad right exactly like a keyframed offset would — proving expressions are
    // wired through the compositor's world matrix, not just the model.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].x.expression = Some("20".to_string());
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(50, 32)[3], 255, "covered band shifted right");
    assert_eq!(f.pixel(10, 32)[3], 0, "left of the shifted quad is clear");
}

#[test]
fn opacity_expression_fades_layer_in_render() {
    // `time` as an opacity expression makes the center alpha grow with time
    // (clamped to [0,1]) — the opacity sampler is expression-aware end to end.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].opacity.expression = Some("time".to_string());
    let a0 = render_frame(&c, 0.0).pixel(32, 32)[3];
    let amid = render_frame(&c, 0.5).pixel(32, 32)[3];
    let a1 = render_frame(&c, 1.0).pixel(32, 32)[3];
    assert!(a0 < amid && amid < a1, "{a0} < {amid} < {a1}");
    assert_eq!(a1, 255);
}

#[test]
fn malformed_render_expression_does_not_crash() {
    // A broken expression on a property must not panic the render — it falls
    // back to the keyframed value (here the default), so the layer still draws.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].x.expression = Some("@@@ broken @@@".to_string());
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255, "falls back to keyframed X (center)");
}

// --- Time remapping (render path) --------------------------------------

#[test]
fn precomp_identity_time_remap_matches_no_remap() {
    use crate::comp::{Interp, LayerKind, PrecompLayer, Prop};
    // Nested comp B (id 2): full-frame solid whose opacity ramps 0 -> 1 over [0,1].
    let make_nested = || {
        let mut nested = full_frame_comp(2, [1.0, 0.0, 0.0, 1.0]);
        nested.layers[0].track_mut(Prop::Opacity).set_key(0.0, 0.0);
        nested.layers[0].track_mut(Prop::Opacity).set_key(1.0, 1.0);
        nested.layers[0]
            .track_mut(Prop::Opacity)
            .set_interp(0.0, Interp::Linear);
        nested
    };
    let make_host = || {
        let mut host = solid([1.0, 1.0, 1.0, 1.0]);
        host.id = 1;
        host.duration = 1.0;
        host.layers[0] = {
            let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
            l.precomp = PrecompLayer::to(2);
            l.scale.set_key(0.0, 3.0);
            l
        };
        host
    };

    // Baseline: no remap.
    let plain = [make_host(), make_nested()];
    // Identity remap: r(t) = t (a 0 -> 1 ramp over [0,1]).
    let mut host_remap = make_host();
    host_remap.layers[0].time_remap.enabled = true;
    host_remap.layers[0].time_remap.track.set_key(0.0, 0.0);
    host_remap.layers[0].time_remap.track.set_key(1.0, 1.0);
    host_remap.layers[0]
        .time_remap
        .track
        .set_interp(0.0, Interp::Linear);
    let remapped = [host_remap, make_nested()];

    for &t in &[0.0, 0.5, 1.0] {
        let mut c1 = crate::comp::FrameCache::new();
        let mut c2 = crate::comp::FrameCache::new();
        let a = render_frame_in_project(&plain, 1, t, &mut c1).pixel(32, 32);
        let b = render_frame_in_project(&remapped, 1, t, &mut c2).pixel(32, 32);
        assert_eq!(a, b, "identity remap must match no-remap at t={t}");
    }
}

#[test]
fn precomp_reverse_time_remap_samples_backwards() {
    use crate::comp::{Interp, LayerKind, PrecompLayer, Prop};
    // Nested comp B (id 2): opacity ramps 0 -> 1 over [0,1].
    let mut nested = full_frame_comp(2, [1.0, 0.0, 0.0, 1.0]);
    nested.layers[0].track_mut(Prop::Opacity).set_key(0.0, 0.0);
    nested.layers[0].track_mut(Prop::Opacity).set_key(1.0, 1.0);
    nested.layers[0]
        .track_mut(Prop::Opacity)
        .set_interp(0.0, Interp::Linear);

    // Host precomp with a reversing remap r(t) = 1 - t over [0,1]: at host t=0 the
    // source is sampled at 1.0 (B fully opaque), at host t=1 at 0.0 (transparent).
    let mut host = solid([1.0, 1.0, 1.0, 1.0]);
    host.id = 1;
    host.duration = 1.0;
    host.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
        l.precomp = PrecompLayer::to(2);
        l.scale.set_key(0.0, 3.0);
        l.time_remap.enabled = true;
        l.time_remap.track.set_key(0.0, 1.0);
        l.time_remap.track.set_key(1.0, 0.0);
        l.time_remap.track.set_interp(0.0, Interp::Linear);
        l
    };
    let comps = [host, nested];
    let mut cache = crate::comp::FrameCache::new();
    let a0 = render_frame_in_project(&comps, 1, 0.0, &mut cache).pixel(32, 32)[3];
    let a1 = render_frame_in_project(&comps, 1, 1.0, &mut cache).pixel(32, 32)[3];
    assert!(a0 > 250, "reverse remap @ t=0 => B's end (opaque), got {a0}");
    assert!(a1 < 5, "reverse remap @ t=1 => B's start (transparent), got {a1}");
}

#[test]
fn precomp_freeze_time_remap_holds_one_source_frame() {
    use crate::comp::{Interp, LayerKind, PrecompLayer, Prop};
    // Nested comp B opacity ramps 0 -> 1 over [0,1]; a constant remap freezes it.
    let mut nested = full_frame_comp(2, [1.0, 0.0, 0.0, 1.0]);
    nested.layers[0].track_mut(Prop::Opacity).set_key(0.0, 0.0);
    nested.layers[0].track_mut(Prop::Opacity).set_key(1.0, 1.0);
    nested.layers[0]
        .track_mut(Prop::Opacity)
        .set_interp(0.0, Interp::Linear);

    // A single constant remap key at source time 1.0: B is frozen fully opaque
    // regardless of host time.
    let mut host = solid([1.0, 1.0, 1.0, 1.0]);
    host.id = 1;
    host.duration = 2.0;
    host.layers[0] = {
        let mut l = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
        l.precomp = PrecompLayer::to(2);
        l.scale.set_key(0.0, 3.0);
        l.time_remap.enabled = true;
        l.time_remap.track.set_key(0.0, 1.0); // freeze at source end
        l
    };
    let comps = [host, nested];
    for &t in &[0.0, 0.5, 1.5] {
        let mut cache = crate::comp::FrameCache::new();
        let a = render_frame_in_project(&comps, 1, t, &mut cache).pixel(32, 32)[3];
        assert!(a > 250, "freeze remap holds B opaque at host t={t}, got {a}");
    }
}
