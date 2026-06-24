use super::*;

// --- Footage layers -----------------------------------------------------

/// Write a `w`x`h` solid-color 8-bit RGBA PNG to a unique temp path and return
/// it, so footage tests have a real file for `prism-io` to decode.
pub(super) fn write_test_png(stem: &str, w: u32, h: u32, rgba: [u8; 4]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pulse_footage_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{stem}.png"));
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        buf.extend_from_slice(&rgba);
    }
    let img = image::RgbaImage::from_raw(w, h, buf).unwrap();
    img.save_with_format(&path, image::ImageFormat::Png).unwrap();
    path
}

#[test]
fn footage_still_rasterizes_into_the_quad() {
    use crate::comp::{FootageSource, LayerKind};
    // A solid-green still, decoded and sampled across the footage layer's quad:
    // the center is the footage color (opaque), a far corner is uncovered.
    let path = write_test_png("still_green", 8, 8, [0, 255, 0, 255]);
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Footage;
    c.layers[0].footage.source = Some(FootageSource::still(&path));

    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "footage center should be opaque");
    assert!(g > 250, "green channel high, got {g}");
    assert!(r < 8 && b < 8, "center should be green, got ({r},{g},{b})");
    // Outside the quad (a far corner) stays transparent.
    assert_eq!(f.pixel(0, 0)[3], 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn footage_honors_opacity() {
    use crate::comp::{FootageSource, LayerKind, Prop};
    let path = write_test_png("still_blue", 8, 8, [0, 0, 255, 255]);
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Footage;
    c.layers[0].footage.source = Some(FootageSource::still(&path));
    c.layers[0].track_mut(Prop::Opacity).set_key(0.0, 0.5);

    let f = render_frame(&c, 0.0);
    let a = f.pixel(32, 32)[3];
    assert!(a > 100 && a < 200, "half-opacity footage center, got a={a}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn unset_footage_renders_nothing() {
    use crate::comp::LayerKind;
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Footage; // no source set
    let f = render_frame(&c, 0.0);
    assert!(f.pixels.iter().all(|&b| b == 0), "no source => empty frame");
}
