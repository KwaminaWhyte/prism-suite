use super::*;
use super::footage::write_test_png;

// --- Frame blending -----------------------------------------------------

/// A 2-frame sequence (black `<tag>_0001.png`, white `<tag>_0002.png`) on a
/// footage layer playing at 10 fps in a 30 fps comp. Source frame 0 = black,
/// 1 = white. `tag` keeps each test's files distinct so parallel tests don't
/// delete each other's frames.
fn frame_blend_comp(
    tag: &str,
    blend: crate::comp::FrameBlend,
) -> (Comp, std::path::PathBuf, std::path::PathBuf) {
    use crate::comp::{FootageSource, LayerKind};
    let p0 = write_test_png(&format!("{tag}_0001"), 8, 8, [0, 0, 0, 255]);
    let p1 = write_test_png(&format!("{tag}_0002"), 8, 8, [255, 255, 255, 255]);
    let pattern = p0
        .parent()
        .unwrap()
        .join(format!("{tag}_{{}}.png"))
        .to_string_lossy()
        .into_owned();
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].kind = LayerKind::Footage;
    c.layers[0].footage.source = Some(FootageSource::Sequence {
        pattern,
        pad: 4,
        start: 1,
        count: 2,
    });
    c.layers[0].footage.fps = Some(10.0); // 10fps in a 30fps comp
    c.layers[0].footage.frame_blend = blend;
    (c, p0, p1)
}

#[test]
fn frame_blend_off_steps_to_floored_frame() {
    use crate::comp::FrameBlend;
    // At t=0.05s @ 10fps the source time is exactly between frame 0 (black) and
    // frame 1 (white). With blending OFF the floored frame (0, black) shows.
    let (c, p0, p1) = frame_blend_comp("fboff", FrameBlend::Off);
    let f = render_frame(&c, 0.05);
    let [r, _g, _b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "footage center opaque");
    assert!(r < 8, "stepped to the black frame, got r={r}");
    let _ = std::fs::remove_file(&p0);
    let _ = std::fs::remove_file(&p1);
}

#[test]
fn frame_blend_mix_cross_dissolves_neighbors() {
    use crate::comp::FrameBlend;
    // Same setup with Frame Mix: the half-way source time blends black and white
    // into a mid-gray (strictly between the two endpoints) — not a hard step.
    let (c, p0, p1) = frame_blend_comp("fbmix", FrameBlend::Mix);
    let f = render_frame(&c, 0.05);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "footage center opaque");
    assert!((20..235).contains(&r), "mid blend, not a step, got r={r}");
    assert_eq!(r, g);
    assert_eq!(g, b, "neutral gray (black<->white mix)");
    let _ = std::fs::remove_file(&p0);
    let _ = std::fs::remove_file(&p1);
}

#[test]
fn frame_blend_mix_on_exact_frame_matches_step() {
    use crate::comp::FrameBlend;
    // On an exact source frame (t=0.0 -> frame 0) Frame Mix has nothing to blend,
    // so it renders identically to the stepped black frame.
    let (c, p0, p1) = frame_blend_comp("fbexact", FrameBlend::Mix);
    let f = render_frame(&c, 0.0);
    let [r, _g, _b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    assert!(r < 8, "exact frame 0 is black with no blend, got r={r}");
    let _ = std::fs::remove_file(&p0);
    let _ = std::fs::remove_file(&p1);
}
