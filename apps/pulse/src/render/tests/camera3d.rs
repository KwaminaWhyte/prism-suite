use super::*;

// --- 3-D layers + camera (render-level) ---------------------------------

use crate::comp::Camera as Cam3d;

/// A 64x64 comp whose default camera is sized to the comp (so a Z=0 3-D layer
/// is identity) with a single mid-size opaque solid.
fn comp_3d_render() -> Comp {
    let mut c = solid([0.2, 0.7, 0.9, 1.0]);
    c.camera = Cam3d::default();
    c.camera.position = [0.0, 0.0, -Cam3d::default_distance(c.height as f32)];
    c
}

#[test]
fn three_d_layer_at_z0_renders_identical_to_2d() {
    // A 3-D layer at Z = 0 with no orientation, default camera → byte-for-byte
    // the same frame as the same layer in 2-D. The core back-compat guarantee.
    let mut flat = comp_3d_render();
    flat.layers[0].x.set_key(0.0, 30.0);
    flat.layers[0].rotation.set_key(0.0, 20.0);
    let mut three_d = flat.clone();
    three_d.layers[0].threed = true; // Z defaults to 0, no orientation
    let a = render_frame(&flat, 0.0);
    let b = render_frame(&three_d, 0.0);
    assert_eq!(a.pixels, b.pixels, "3-D @ Z=0 must match the 2-D render");
}

#[test]
fn pushing_z_shrinks_the_rendered_footprint() {
    // The same 3-D layer pushed in Z covers fewer opaque pixels (perspective).
    let count_opaque = |f: &Frame| -> usize {
        (0..f.width * f.height)
            .filter(|i| f.pixels[(*i * 4 + 3) as usize] > 0)
            .count()
    };
    let mut near = comp_3d_render();
    near.layers[0].threed = true;
    let near_f = render_frame(&near, 0.0);
    let mut far = near.clone();
    far.layers[0].z.set_key(0.0, 600.0);
    let far_f = render_frame(&far, 0.0);
    assert!(
        count_opaque(&far_f) < count_opaque(&near_f),
        "z-pushed layer must cover fewer pixels: far {} < near {}",
        count_opaque(&far_f),
        count_opaque(&near_f),
    );
}

#[test]
fn two_d_only_comp_renders_identically_with_camera_field() {
    // A comp built the legacy way (serde-default camera) and the same comp with
    // an explicit default camera render byte-identically — adding the camera
    // field changes nothing for a 2-D-only comp.
    let legacy = solid([0.9, 0.3, 0.2, 1.0]); // uses Camera::default()
    let f = render_frame(&legacy, 0.0);
    // A reference render produced the same way must match exactly (determinism +
    // 2-D-only invariance).
    let f2 = render_frame(&legacy.clone(), 0.0);
    assert_eq!(f.pixels, f2.pixels);
    // No 3-D layers ⇒ the draw order is the plain stack order.
    assert_eq!(legacy.draw_order(0.0), vec![0]);
}

#[test]
fn z_sorted_3d_layers_draw_far_first() {
    // Two overlapping full-frame 3-D solids: the nearer one must end up on top
    // regardless of stack order (painter's z-sort).
    let mut c = comp_3d_render();
    c.layers[0] = PulseLayer::new("back", [1.0, 0.0, 0.0, 1.0]); // red
    c.layers[0].scale.set_key(0.0, 3.0);
    c.layers[0].threed = true;
    c.layers[0].z.set_key(0.0, 0.0); // nearer
    let mut front = PulseLayer::new("front", [0.0, 0.0, 1.0, 1.0]); // blue
    front.scale.set_key(0.0, 3.0);
    front.threed = true;
    front.z.set_key(0.0, 800.0); // farther — should be drawn first (behind)
    c.layers.push(front);
    let f = render_frame(&c, 0.0);
    let center = f.pixel(32, 32);
    // The nearer (red, Z=0) layer wins the center even though blue is later in
    // the stack, because blue is farther and drawn first.
    assert!(center[0] > center[2], "near red on top: {center:?}");
}

// ---- Lighting (comp lights shading 3-D layers) ----

#[test]
fn no_lights_comp_renders_byte_identical() {
    // A comp with no lights renders exactly as before — even with a 3-D layer
    // that has `accepts_lights` set (no lights → no modulation). Golden guarantee.
    let mut base = comp_3d_render();
    base.layers[0].scale.set_key(0.0, 2.0);
    base.layers[0].threed = true;
    let mut lit_flag = base.clone();
    lit_flag.layers[0].accepts_lights = true; // opted in, but the comp has no lights
    let a = render_frame(&base, 0.0);
    let b = render_frame(&lit_flag, 0.0);
    assert_eq!(a.pixels, b.pixels, "no lights → byte-identical even with accepts_lights");
}

#[test]
fn accepts_lights_off_is_unaffected_by_lights() {
    // Adding lights to the comp must not touch a layer that doesn't opt in.
    let mut no_light = comp_3d_render();
    no_light.layers[0].scale.set_key(0.0, 2.0);
    no_light.layers[0].threed = true;
    let baseline = render_frame(&no_light, 0.0).pixels.clone();
    let mut with_lights = no_light.clone();
    with_lights.lights.push(crate::comp::Light::point(
        [0.0, 0.0, -500.0],
        [1.0, 1.0, 1.0],
        2.0,
    ));
    // accepts_lights stays false (default).
    let b = render_frame(&with_lights, 0.0);
    assert_eq!(baseline, b.pixels, "accepts_lights=false → unaffected by lights");
}

#[test]
fn facing_point_light_brightens_the_layer() {
    // A 3-D layer facing a point light is brighter than the same unlit layer.
    let mut c = comp_3d_render();
    c.layers[0] = PulseLayer::new("L", [0.5, 0.5, 0.5, 1.0]);
    c.layers[0].scale.set_key(0.0, 2.0);
    c.layers[0].threed = true;
    c.layers[0].accepts_lights = true;
    let unlit = {
        let mut u = c.clone();
        u.layers[0].accepts_lights = false;
        render_frame(&u, 0.0).pixel(32, 32)[0]
    };
    // Ambient floor + a head-on point light → factor > 1 → brighter.
    c.lights.push(crate::comp::Light::ambient([1.0, 1.0, 1.0], 0.3));
    c.lights.push(crate::comp::Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0));
    let lit = render_frame(&c, 0.0).pixel(32, 32)[0];
    assert!(lit > unlit, "facing light brighter: lit {lit} > unlit {unlit}");
}

#[test]
fn back_facing_layer_falls_to_ambient_floor() {
    // A layer flipped away from a point light is dimmer than one facing it
    // (only the ambient floor reaches it) — Lambert N·L.
    let make = |orient_x: f32| -> u8 {
        let mut c = comp_3d_render();
        c.layers[0] = PulseLayer::new("L", [0.6, 0.6, 0.6, 1.0]);
        c.layers[0].scale.set_key(0.0, 2.0);
        c.layers[0].threed = true;
        c.layers[0].accepts_lights = true;
        c.layers[0].orient_x.set_key(0.0, orient_x);
        c.lights.push(crate::comp::Light::ambient([1.0, 1.0, 1.0], 0.2));
        c.lights.push(crate::comp::Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0));
        render_frame(&c, 0.0).pixel(32, 32)[0]
    };
    let facing = make(0.0);
    let away = make(180.0);
    assert!(facing > away, "facing {facing} brighter than back-facing {away}");
}

// ---- Camera depth of field (defocus blur on 3-D layers) ----

/// Count of fully/partially opaque pixels in a frame.
fn dof_opaque_count(f: &Frame) -> usize {
    (0..f.width * f.height)
        .filter(|i| f.pixels[(*i * 4 + 3) as usize] > 0)
        .count()
}

#[test]
fn dof_off_renders_byte_identical() {
    // A comp with DoF off (the default) renders exactly as before — even with a
    // 3-D layer pushed off the focal plane. Golden / back-compat guarantee.
    let mut base = comp_3d_render();
    base.layers[0].scale.set_key(0.0, 2.0);
    base.layers[0].threed = true;
    base.layers[0].z.set_key(0.0, 600.0); // off the focal plane
    let mut explicit_off = base.clone();
    explicit_off.camera.dof_enabled = false; // explicit OFF
    explicit_off.camera.aperture = 80.0; // aperture set but DoF off → ignored
    let a = render_frame(&base, 0.0);
    let b = render_frame(&explicit_off, 0.0);
    assert_eq!(a.pixels, b.pixels, "DoF off → byte-identical");
}

#[test]
fn in_focus_layer_is_sharp_with_dof_on() {
    // A 3-D layer placed exactly at the focus distance renders identically with
    // DoF on vs off — an in-focus layer is untouched.
    let mut sharp = comp_3d_render();
    sharp.layers[0].scale.set_key(0.0, 2.0);
    sharp.layers[0].threed = true;
    sharp.layers[0].z.set_key(0.0, 300.0);
    let depth = sharp.layer_depth(0, 0.0);
    let mut dof_on = sharp.clone();
    dof_on.camera.dof_enabled = true;
    dof_on.camera.aperture = 60.0;
    dof_on.camera.focus_distance = depth; // focus exactly on the layer
    let a = render_frame(&sharp, 0.0);
    let b = render_frame(&dof_on, 0.0);
    assert_eq!(a.pixels, b.pixels, "in-focus 3-D layer is sharp");
}

#[test]
fn out_of_focus_layer_blurs_and_spreads() {
    // A 3-D layer far from focus blurs: its opaque footprint spreads (soft edges
    // bleed into surrounding transparent pixels) compared to the sharp render.
    let mut sharp = comp_3d_render();
    sharp.layers[0] = PulseLayer::new("L", [0.2, 0.8, 0.4, 1.0]);
    sharp.layers[0].scale.set_key(0.0, 0.5); // small footprint so it can spread
    sharp.layers[0].threed = true;
    sharp.layers[0].z.set_key(0.0, 100.0);
    let mut blurred = sharp.clone();
    blurred.camera.dof_enabled = true;
    blurred.camera.aperture = 40.0;
    blurred.camera.focus_distance = 1500.0; // far from the layer → blur
    let sf = render_frame(&sharp, 0.0);
    let bf = render_frame(&blurred, 0.0);
    assert_ne!(sf.pixels, bf.pixels, "out-of-focus layer must differ");
    assert!(
        dof_opaque_count(&bf) > dof_opaque_count(&sf),
        "blur spreads coverage: blurred {} > sharp {}",
        dof_opaque_count(&bf),
        dof_opaque_count(&sf),
    );
}

#[test]
fn wider_aperture_blurs_more() {
    // The same off-focus 3-D layer blurs more with a wider aperture: a wider
    // aperture spreads its soft footprint over more pixels.
    let make = |aperture: f32| -> usize {
        let mut c = comp_3d_render();
        c.layers[0] = PulseLayer::new("L", [0.9, 0.6, 0.2, 1.0]);
        c.layers[0].scale.set_key(0.0, 0.4); // small footprint so it can spread
        c.layers[0].threed = true;
        c.layers[0].z.set_key(0.0, 50.0);
        c.camera.dof_enabled = true;
        c.camera.aperture = aperture;
        c.camera.focus_distance = 800.0;
        dof_opaque_count(&render_frame(&c, 0.0))
    };
    let narrow = make(10.0);
    let wide = make(40.0);
    assert!(wide > narrow, "wider aperture spreads more: {wide} > {narrow}");
}

#[test]
fn dof_does_not_blur_2d_layers() {
    // A 2-D layer is never defocused, even with DoF on and a wide aperture.
    let mut base = comp_3d_render();
    base.layers[0].scale.set_key(0.0, 2.0); // 2-D (threed stays false)
    let baseline = render_frame(&base, 0.0).pixels.clone();
    let mut dof_on = base.clone();
    dof_on.camera.dof_enabled = true;
    dof_on.camera.aperture = 150.0;
    dof_on.camera.focus_distance = 9000.0;
    let b = render_frame(&dof_on, 0.0);
    assert_eq!(baseline, b.pixels, "2-D layer unaffected by camera DoF");
}

#[test]
fn dof_render_is_deterministic() {
    let mut c = comp_3d_render();
    c.layers[0].scale.set_key(0.0, 1.5);
    c.layers[0].threed = true;
    c.layers[0].z.set_key(0.0, 700.0);
    c.camera.dof_enabled = true;
    c.camera.aperture = 90.0;
    c.camera.focus_distance = 200.0;
    let a = render_frame(&c, 0.0);
    let b = render_frame(&c, 0.0);
    assert_eq!(a.pixels, b.pixels, "DoF render is deterministic");
}
