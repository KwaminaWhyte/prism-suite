use super::*;

// --- 3-D layers + camera (perspective projection + z-sort) -----------------

/// A 3-D-capable comp: a single solid layer, default camera sized to the comp.
fn comp_3d() -> Comp {
    let mut c = parented_comp();
    c.layers.truncate(1);
    c.layers[0].threed = true;
    // Match `Comp::new`'s default-camera placement for the comp height.
    c.camera = Camera::default();
    c.camera.position = [0.0, 0.0, -Camera::default_distance(c.height as f32)];
    c
}

#[test]
fn default_camera_projects_z0_plane_to_identity() {
    // A point on the z = 0 plane projects to itself at unit scale under the
    // default camera — the back-compat guarantee.
    let cam = Camera {
        position: [0.0, 0.0, -Camera::default_distance(100.0)],
        ..Camera::default()
    };
    let p = cam.project(20.0, -30.0, 0.0, 100.0);
    assert!(approx(p.screen, (20.0, -30.0)), "screen {:?}", p.screen);
    assert!((p.scale - 1.0).abs() < 1e-4, "scale {}", p.scale);
}

#[test]
fn pushing_z_farther_shrinks_projected_scale() {
    let cam = Camera {
        position: [0.0, 0.0, -Camera::default_distance(100.0)],
        ..Camera::default()
    };
    let near = cam.project(10.0, 0.0, 0.0, 100.0);
    let far = cam.project(10.0, 0.0, 200.0, 100.0);
    assert!(far.scale < near.scale, "far {} < near {}", far.scale, near.scale);
    // Projected x shrinks with scale (perspective foreshortening toward center).
    assert!(far.screen.0.abs() < near.screen.0.abs());
    // Pulling closer toward the camera (negative z, still in front of it)
    // enlarges it.
    let closer = cam.project(10.0, 0.0, -50.0, 100.0);
    assert!(closer.scale > near.scale, "closer {} > near {}", closer.scale, near.scale);
}

#[test]
fn layer_world_z0_equals_2d_world_matrix() {
    // A 3-D layer at Z = 0 with no orientation projects to exactly its 2-D
    // world matrix under the default camera (back-compat at the layer level).
    let mut c = comp_3d();
    c.layers[0].x.set_key(0.0, 30.0);
    c.layers[0].y.set_key(0.0, -15.0);
    c.layers[0].scale.set_key(0.0, 1.3);
    c.layers[0].rotation.set_key(0.0, 20.0);
    let world_2d = c.world_matrix(0, 0.0);
    let world_3d = c.layer_world(0, 0.0).expect("non-degenerate");
    // Corners must coincide.
    for (lx, ly) in [(0.0, 0.0), (50.0, 0.0), (0.0, 40.0), (-30.0, 25.0)] {
        assert!(
            approx(world_2d.apply(lx, ly), world_3d.apply(lx, ly)),
            "2d {:?} vs 3d {:?}",
            world_2d.apply(lx, ly),
            world_3d.apply(lx, ly),
        );
    }
}

#[test]
fn z_depth_shrinks_layer_world_scale() {
    // A 3-D layer pushed in Z is projected smaller than at Z = 0.
    let mut c = comp_3d();
    let at0 = c.layer_world(0, 0.0).unwrap();
    c.layers[0].z.set_key(0.0, 400.0);
    let atz = c.layer_world(0, 0.0).unwrap();
    let area = |m: &Affine2| (m.a * m.d - m.b * m.c).abs();
    assert!(area(&atz) < area(&at0), "z-pushed area must shrink");
}

#[test]
fn orientation_rotates_the_projected_quad() {
    // A Z orientation rolls the projected quad; the +x edge no longer points
    // straight along comp +x.
    let mut c = comp_3d();
    c.layers[0].orient_z.set_key(0.0, 90.0);
    let m = c.layer_world(0, 0.0).unwrap();
    let o = m.apply(0.0, 0.0);
    let ex = m.apply(1.0, 0.0);
    // After a 90° roll the local +x edge maps to (≈0, +something) in comp space.
    assert!((ex.0 - o.0).abs() < 1e-3, "x-edge x-comp ~ 0");
    assert!((ex.1 - o.1).abs() > 0.5, "x-edge gained y-comp");
}

#[test]
fn draw_order_sorts_3d_layers_by_depth_regardless_of_stack() {
    // Two 3-D layers; the one with larger Z (farther) must be drawn first
    // regardless of stack order.
    let mut c = comp_3d();
    c.layers.push(PulseLayer::new("B", [1.0; 4]));
    c.layers[1].threed = true;
    // Layer 0 near (small Z), layer 1 far (large Z).
    c.layers[0].z.set_key(0.0, -100.0);
    c.layers[1].z.set_key(0.0, 500.0);
    let order = c.draw_order(0.0);
    // Far layer (1) drawn before near layer (0).
    let pos0 = order.iter().position(|&i| i == 0).unwrap();
    let pos1 = order.iter().position(|&i| i == 1).unwrap();
    assert!(pos1 < pos0, "far layer drawn first: {order:?}");
    // Swapping depths flips the order.
    c.layers[0].z.set_key(0.0, 500.0);
    c.layers[1].z.set_key(0.0, -100.0);
    let order2 = c.draw_order(0.0);
    let p0 = order2.iter().position(|&i| i == 0).unwrap();
    let p1 = order2.iter().position(|&i| i == 1).unwrap();
    assert!(p0 < p1, "depths swapped → order flips: {order2:?}");
}

#[test]
fn draw_order_identity_with_no_3d_layers() {
    // No 3-D layers → identity order (back-compat: the draw loop is unchanged).
    let c = parented_comp();
    assert_eq!(c.draw_order(0.0), vec![0, 1]);
}

#[test]
fn draw_order_keeps_2d_layers_in_place() {
    // 2-D layers keep their exact stack slots; only the 3-D slots are re-sorted.
    let mut c = comp_3d(); // layer 0 is 3-D
    c.layers.push(PulseLayer::new("two_d", [1.0; 4])); // 1: 2-D
    c.layers.push(PulseLayer::new("three_d", [1.0; 4])); // 2: 3-D
    c.layers[2].threed = true;
    c.layers[0].z.set_key(0.0, 0.0);
    c.layers[2].z.set_key(0.0, 800.0); // far → should come first among 3-D slots
    let order = c.draw_order(0.0);
    // The 2-D layer (index 1) stays in slot 1.
    assert_eq!(order[1], 1, "2-D layer stays put: {order:?}");
    // The 3-D slots {0, 2} hold the depth-sorted 3-D layers (far=2 first).
    assert_eq!(order[0], 2, "far 3-D layer fills the first 3-D slot: {order:?}");
    assert_eq!(order[2], 0);
}

#[test]
fn focal_length_round_trips_through_fov() {
    let mut cam = Camera::default();
    let (w, h) = (1280.0, 720.0);
    cam.set_focal_length(50.0, w, h);
    let back = cam.focal_length(w, h);
    assert!((back - 50.0).abs() < 0.5, "focal round-trip: {back}");
}

#[test]
fn rotate_orientation_identity_when_zero() {
    let p = rotate_orientation(3.0, -4.0, 0.0, 0.0, 0.0, 0.0);
    assert!((p.0 - 3.0).abs() < 1e-6 && (p.1 + 4.0).abs() < 1e-6 && p.2.abs() < 1e-6);
}

#[test]
fn camera_serde_round_trip_and_legacy_default() {
    // A comp with a camera + 3-D layer round-trips through JSON.
    let mut c = comp_3d();
    c.layers[0].z.set_key(0.0, 120.0);
    c.layers[0].orient_y.set_key(0.0, 30.0);
    c.camera.fov_deg = 40.0;
    let json = serde_json::to_string(&c).unwrap();
    let back: Comp = serde_json::from_str(&json).unwrap();
    assert!((back.camera.fov_deg - 40.0).abs() < 1e-4);
    assert!(back.layers[0].threed);
    assert!((back.layer_z(0, 0.0) - 120.0).abs() < 1e-4);

    // A legacy comp JSON with no `camera` / 3-D keys loads with the default
    // camera and a 2-D layer (renders unchanged).
    let legacy = r#"{
        "id":0,"name":"L","width":64,"height":64,"duration":1.0,"fps":30.0,
        "layers":[{"name":"S","color":[1.0,1.0,1.0,1.0],"visible":true,
            "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
            "rotation":{"keys":[]},"opacity":{"keys":[]}}]
    }"#;
    let lc: Comp = serde_json::from_str(legacy).unwrap();
    assert_eq!(lc.camera, Camera::default());
    assert!(!lc.layers[0].threed);
    assert!(!lc.layer_is_3d(0));
}

#[test]
fn layer_world_is_deterministic() {
    let mut c = comp_3d();
    c.layers[0].z.set_key(0.0, 250.0);
    c.layers[0].orient_x.set_key(0.0, 25.0);
    let a = c.layer_world(0, 0.0).unwrap();
    let b = c.layer_world(0, 0.0).unwrap();
    assert_eq!(a, b);
}

#[test]
fn layer_light_factor_none_when_unlit() {
    // No lights, a 2-D layer, and a 3-D layer that doesn't opt in all return
    // None (no modulation → render unchanged).
    let mut c = Comp::new();
    c.layers.clear();
    c.layers.push(PulseLayer::new("L", [1.0, 1.0, 1.0, 1.0]));
    // No lights at all.
    assert_eq!(c.layer_light_factor(0, 0.0), None);
    // With a light but the layer is 2-D.
    c.lights.push(Light::point([0.0, 0.0, -100.0], [1.0, 1.0, 1.0], 1.0));
    assert_eq!(c.layer_light_factor(0, 0.0), None, "2-D layer is never lit");
    // 3-D but not accepting lights.
    c.layers[0].threed = true;
    assert_eq!(c.layer_light_factor(0, 0.0), None, "accepts_lights=false → None");
    // 3-D + opted in → Some factor.
    c.layers[0].accepts_lights = true;
    assert!(c.layer_light_factor(0, 0.0).is_some(), "lit layer yields a factor");
}

#[test]
fn comp_with_lights_serde_round_trips_and_legacy_defaults_empty() {
    let mut c = Comp::new();
    c.lights.push(Light::ambient([0.2, 0.3, 0.4], 0.5));
    c.lights.push(Light::point([1.0, 2.0, 3.0], [1.0, 0.5, 0.0], 1.5));
    c.layers[0].accepts_lights = true;
    let json = serde_json::to_string(&c).unwrap();
    let back: Comp = serde_json::from_str(&json).unwrap();
    assert_eq!(back.lights, c.lights);
    assert!(back.layers[0].accepts_lights);
    // A legacy comp JSON (no `lights` key, no `accepts_lights`) loads empty/false.
    let legacy = r#"{"width":64,"height":64,"duration":1.0,"fps":30.0,"layers":[]}"#;
    let lc: Comp = serde_json::from_str(legacy).unwrap();
    assert!(lc.lights.is_empty(), "legacy comp has no lights");
}

// --- Camera depth of field (circle-of-confusion blur) ----------------------

/// A DoF-enabled camera with an explicit focus distance + aperture.
fn dof_cam(focus: f32, aperture: f32) -> Camera {
    Camera {
        dof_enabled: true,
        focus_distance: focus,
        aperture,
        ..Camera::default()
    }
}

#[test]
fn coc_zero_at_focus_distance() {
    // A layer exactly at the focus distance is perfectly sharp.
    let cam = dof_cam(500.0, 40.0);
    assert_eq!(cam.coc_blur_radius(500.0, 720.0), 0.0);
}

#[test]
fn coc_grows_with_distance_from_focus() {
    let cam = dof_cam(500.0, 40.0);
    let near = cam.coc_blur_radius(600.0, 720.0); // 100 from focus
    let far = cam.coc_blur_radius(900.0, 720.0); // 400 from focus
    assert!(near > 0.0, "off-focus layer blurs");
    assert!(far > near, "farther from focus blurs more ({far} > {near})");
    // Symmetric: same |error| in front of focus blurs the same amount.
    let front = cam.coc_blur_radius(400.0, 720.0); // 100 from focus (nearer)
    assert!((front - near).abs() < 1e-4, "blur symmetric about focus");
}

#[test]
fn coc_grows_with_aperture() {
    let narrow = dof_cam(500.0, 20.0).coc_blur_radius(800.0, 720.0);
    let wide = dof_cam(500.0, 80.0).coc_blur_radius(800.0, 720.0);
    assert!(wide > narrow, "wider aperture blurs more ({wide} > {narrow})");
    // Aperture is the linear scale: 4x aperture → 4x radius.
    assert!((wide - 4.0 * narrow).abs() < 1e-3, "radius linear in aperture");
}

#[test]
fn coc_zero_when_dof_off_or_aperture_zero() {
    // DoF disabled: no blur regardless of depth.
    let mut cam = dof_cam(500.0, 40.0);
    cam.dof_enabled = false;
    assert_eq!(cam.coc_blur_radius(2000.0, 720.0), 0.0, "DoF off = sharp");
    // DoF on but aperture zero: still sharp (the default-aperture case).
    let cam = dof_cam(500.0, 0.0);
    assert_eq!(cam.coc_blur_radius(2000.0, 720.0), 0.0, "aperture 0 = sharp");
}

#[test]
fn coc_radius_is_clamped() {
    // A pathological aperture + huge depth error can't exceed MAX_DOF_RADIUS.
    let cam = dof_cam(1.0, 1e6);
    assert_eq!(cam.coc_blur_radius(1e6, 720.0), Camera::MAX_DOF_RADIUS);
}

#[test]
fn coc_default_focus_is_the_focal_plane() {
    // focus_distance == 0 means "use the camera-to-look-at focal plane", so a
    // layer on the z = 0 plane (the default poi) is sharp.
    let mut cam = Camera {
        position: [0.0, 0.0, -Camera::default_distance(720.0)],
        dof_enabled: true,
        aperture: 50.0,
        ..Camera::default()
    };
    cam.focus_distance = 0.0;
    let focal = cam.effective_focus(720.0);
    assert!(focal > 0.0);
    assert_eq!(cam.coc_blur_radius(focal, 720.0), 0.0, "focal plane is sharp");
    assert!(cam.coc_blur_radius(focal + 300.0, 720.0) > 0.0, "off it blurs");
}

#[test]
fn layer_dof_blur_none_when_off_or_2d() {
    let mut c = comp_3d();
    c.layers[0].z.set_key(0.0, 800.0);
    // DoF off (default): None even for a 3-D layer.
    assert_eq!(c.layer_dof_blur(0, 0.0), None, "DoF off → None");
    // DoF on, 3-D layer off the focal plane: Some positive radius.
    c.camera.dof_enabled = true;
    c.camera.aperture = 60.0;
    let r = c.layer_dof_blur(0, 0.0).expect("3-D + DoF on yields a radius");
    assert!(r > 0.0, "off-focus 3-D layer blurs ({r})");
    // A 2-D layer is never defocused, even with DoF on.
    c.layers[0].threed = false;
    assert_eq!(c.layer_dof_blur(0, 0.0), None, "2-D layer never blurs");
}

#[test]
fn camera_dof_serde_round_trip_and_legacy_default() {
    let mut c = comp_3d();
    c.camera.dof_enabled = true;
    c.camera.focus_distance = 750.0;
    c.camera.aperture = 35.0;
    let json = serde_json::to_string(&c).unwrap();
    let back: Comp = serde_json::from_str(&json).unwrap();
    assert!(back.camera.dof_enabled);
    assert!((back.camera.focus_distance - 750.0).abs() < 1e-4);
    assert!((back.camera.aperture - 35.0).abs() < 1e-4);

    // A legacy camera JSON without any `dof_*` keys loads DoF off (sharp).
    let legacy = r#"{"position":[0.0,0.0,-100.0],"poi":[0.0,0.0,0.0],"fov_deg":54.0}"#;
    let cam: Camera = serde_json::from_str(legacy).unwrap();
    assert!(!cam.dof_enabled, "legacy camera defaults DoF off");
    assert_eq!(cam.focus_distance, 0.0);
    assert_eq!(cam.aperture, 0.0);
}
