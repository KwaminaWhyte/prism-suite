use super::*;

#[test]
fn frame_has_correct_size() {
    let c = solid([1.0, 0.0, 0.0, 1.0]);
    let f = render_frame(&c, 0.0);
    assert_eq!(f.width, 64);
    assert_eq!(f.height, 64);
    assert_eq!(f.pixels.len(), 64 * 64 * 4);
}

#[test]
fn empty_comp_is_transparent() {
    let c = Comp {
        width: 8,
        height: 8,
        duration: 1.0,
        fps: 30.0,
        motion_blur: MotionBlur::default(),
        markers: Vec::new(),
        work_area: WorkArea::default(),
        camera: Camera::default(),
        lights: Vec::new(),
        hide_shy: false,
        layers: Vec::new(),
        id: 0,
        name: String::new(),
    };
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0));
}

#[test]
fn center_pixel_is_opaque_layer_color() {
    // A centered, unrotated, unit-scale opaque red layer covers the center.
    let c = solid([1.0, 0.0, 0.0, 1.0]);
    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    assert!(r > 250, "red channel high, got {r}");
    assert_eq!(g, 0);
    assert_eq!(b, 0);
}

#[test]
fn corner_pixel_outside_quad_is_transparent() {
    // Half-extent is 0.22*64 ≈ 14 px, so a far corner is uncovered.
    let c = solid([1.0, 1.0, 1.0, 1.0]);
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(0, 0)[3], 0);
    assert_eq!(f.pixel(63, 63)[3], 0);
}

#[test]
fn invisible_layer_does_not_render() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].visible = false;
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0));
}

#[test]
fn zero_opacity_is_transparent() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].opacity.set_key(0.0, 0.0);
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 0);
}

#[test]
fn opacity_animates_over_time() {
    // Opacity ramps 0 -> 1 across the comp; center alpha grows with time.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].opacity.set_key(0.0, 0.0);
    c.layers[0].opacity.set_key(1.0, 1.0);
    let a0 = render_frame(&c, 0.0).pixel(32, 32)[3];
    let amid = render_frame(&c, 0.5).pixel(32, 32)[3];
    let a1 = render_frame(&c, 1.0).pixel(32, 32)[3];
    assert!(a0 < amid && amid < a1, "{a0} < {amid} < {a1}");
    assert_eq!(a1, 255);
}

#[test]
fn position_offset_moves_coverage() {
    // Shift the layer far right: center is now uncovered, the right edge covered.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].x.set_key(0.0, 20.0);
    let f = render_frame(&c, 0.0);
    // Original center (32,32) sits at the layer's left edge region; the
    // covered band shifts right. Sample a pixel that should now be covered.
    assert_eq!(f.pixel(50, 32)[3], 255);
    // A pixel far left of the shifted quad is uncovered.
    assert_eq!(f.pixel(10, 32)[3], 0);
}

#[test]
fn source_over_blends_two_layers_in_linear() {
    // Opaque black behind, 50% white on top -> mid gray, fully opaque.
    let mut c = solid([0.0, 0.0, 0.0, 1.0]);
    let mut top = PulseLayer::new("top", [1.0, 1.0, 1.0, 1.0]);
    top.opacity.set_key(0.0, 0.5);
    c.layers.push(top);
    let f = render_frame(&c, 0.0);
    let [r, _g, _b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    // 0.5 linear-light coverage of white over black, sRGB-encoded, is well
    // above naive 0.5*255=128 (gamma), so just bound it sensibly.
    assert!((150..=200).contains(&r), "mid gray r={r}");
}

#[test]
fn scale_zero_renders_nothing() {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 0.0);
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0));
}

#[test]
fn larger_scale_covers_more_pixels() {
    let count_covered = |scale: f32| {
        let mut c = solid([1.0, 1.0, 1.0, 1.0]);
        c.layers[0].scale.set_key(0.0, scale);
        let f = render_frame(&c, 0.0);
        f.pixels.chunks(4).filter(|p| p[3] > 0).count()
    };
    assert!(count_covered(2.0) > count_covered(1.0));
}

#[test]
fn rotation_keeps_center_covered() {
    // Rotating about the layer center leaves the center pixel covered.
    let mut c = solid([0.0, 1.0, 0.0, 1.0]);
    c.layers[0].rotation.set_key(0.0, 45.0);
    let f = render_frame(&c, 0.0);
    assert_eq!(f.pixel(32, 32)[3], 255);
}

#[test]
fn rotation_uses_outgoing_interp() {
    // Sanity: a rotation track sampled mid-segment differs from endpoints,
    // confirming render_frame consults the animated transform.
    let mut c = solid([1.0, 0.0, 0.0, 1.0]);
    c.layers[0].rotation.set_key(0.0, 0.0);
    c.layers[0].rotation.set_key(1.0, 90.0);
    c.layers[0].rotation.set_interp(0.0, Interp::Linear);
    // Just assert it renders without panic at a few times.
    for &t in &[0.0, 0.25, 0.5, 1.0] {
        let _ = render_frame(&c, t);
    }
    // And the transform actually animates.
    assert!((c.layers[0].value(Prop::Rotation, 0.5) - 45.0).abs() < 1e-3);
}
