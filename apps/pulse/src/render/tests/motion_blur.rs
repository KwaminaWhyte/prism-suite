use super::*;

// --- Motion blur --------------------------------------------------------

/// A 64x64 comp whose single solid slides fast left→right across the frame,
/// with comp motion blur on and the layer opted in (toggled by `layer_mb`).
fn moving_solid(layer_mb: bool) -> Comp {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].motion_blur = layer_mb;
    c.layers[0].x.set_key(0.0, -24.0);
    c.layers[0].x.set_key(1.0, 24.0);
    c.motion_blur.enabled = true;
    c.motion_blur.angle = 360.0; // a whole frame of blur for a clear effect
    c.motion_blur.samples = 16;
    c
}

#[test]
fn motion_blur_softens_the_moving_edge() {
    // With motion blur the leading/trailing edge spans several partly-covered
    // (0 < a < 255) pixels; without it the edge is a hard 0/255 step. Count
    // the partial-alpha pixels along the center row at mid-travel.
    let partial_count = |mb: bool| {
        let c = moving_solid(mb);
        let f = render_frame(&c, 0.5);
        (0..f.width)
            .filter(|&x| {
                let a = f.pixel(x, 32)[3];
                a > 0 && a < 255
            })
            .count()
    };
    let blurred = partial_count(true);
    let crisp = partial_count(false);
    assert!(
        blurred > crisp,
        "motion blur should add partial-coverage edge pixels: blurred={blurred} crisp={crisp}"
    );
}

#[test]
fn motion_blur_preserves_color_no_bleed() {
    // A fully-covered pixel near the center of the swept band keeps the
    // layer's pure-white color (premultiplied averaging must not bleed it
    // toward black through the transparent samples).
    let c = moving_solid(true);
    let f = render_frame(&c, 0.5);
    // The layer center at t=0.5 sits at comp x=0 -> pixel 32; fully covered
    // across the sweep, so still opaque white.
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "center stays fully covered through the sweep");
    assert!(
        r > 250 && g > 250 && b > 250,
        "color preserved, got {r},{g},{b}"
    );
}

#[test]
fn comp_master_switch_gates_motion_blur() {
    // Layer opted in but comp master off -> identical to no motion blur.
    let mut c = moving_solid(true);
    c.motion_blur.enabled = false;
    let off = render_frame(&c, 0.5);
    let mut crisp = solid([1.0, 1.0, 1.0, 1.0]);
    crisp.layers[0].x.set_key(0.0, -24.0);
    crisp.layers[0].x.set_key(1.0, 24.0);
    let baseline = render_frame(&crisp, 0.5);
    assert_eq!(off.pixels, baseline.pixels);
}

#[test]
fn unblurred_layer_unaffected_by_comp_motion_blur() {
    // Comp MB on but the layer didn't opt in -> crisp render unchanged.
    let blurred_off = render_frame(&moving_solid(false), 0.5);
    let mut crisp = solid([1.0, 1.0, 1.0, 1.0]);
    crisp.layers[0].x.set_key(0.0, -24.0);
    crisp.layers[0].x.set_key(1.0, 24.0);
    let baseline = render_frame(&crisp, 0.5);
    assert_eq!(blurred_off.pixels, baseline.pixels);
}

#[test]
fn motion_blur_respects_track_matte() {
    // A motion-blurred base clipped by a small static alpha matte: the matte
    // still bounds coverage (no blurred pixels leak past the matte edge far
    // from the source quad).
    let mut c = matte_pair([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0], 1.0);
    c.layers[0].matte = MatteMode::Alpha;
    c.layers[0].motion_blur = true;
    c.layers[0].x.set_key(0.0, -24.0);
    c.layers[0].x.set_key(1.0, 24.0);
    c.motion_blur.enabled = true;
    let f = render_frame(&c, 0.5);
    // A far corner is outside the small matte source -> matted out even with
    // motion blur on.
    assert_eq!(f.pixel(2, 2)[3], 0, "matte must still clip the blur");
}
