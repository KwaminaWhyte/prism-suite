use super::*;

// --- Track mattes -------------------------------------------------------


#[test]
fn matte_source_is_not_composited_on_its_own() {
    // A red base under a green source; with an alpha matte the green source
    // must NOT appear in the output — it only shapes the base's alpha.
    let mut c = matte_pair([1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], 1.0);
    c.layers[0].matte = MatteMode::Alpha;
    let f = render_frame(&c, 0.0);
    let [r, g, _b, a] = f.pixel(32, 32);
    assert_eq!(a, 255);
    // The center shows the base's red, not the source's green.
    assert!(r > 250, "expected red base, got r={r}");
    assert_eq!(g, 0, "matte source leaked into the composite");
}

#[test]
fn alpha_matte_clips_to_source_coverage() {
    // Full-frame base, small (unit-scale) source. With an alpha matte the
    // base is visible only inside the small source quad: center covered, a
    // far edge (inside the base but outside the source) is now transparent.
    let mut c = matte_pair([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0], 1.0);
    c.layers[0].matte = MatteMode::Alpha;
    let f = render_frame(&c, 0.0);
    assert_eq!(
        f.pixel(32, 32)[3],
        255,
        "center should pass the alpha matte"
    );
    // A pixel far from center is inside the full-frame base but outside the
    // small source quad -> matted away.
    assert_eq!(f.pixel(2, 2)[3], 0, "edge should be matted out");
}

#[test]
fn inverted_alpha_matte_is_the_complement() {
    // Inverted alpha: the base shows where the source is *transparent*, so the
    // center (under the opaque source) is hidden and the surrounding base
    // (full-frame) stays. Compare against the non-inverted case.
    let mut c = matte_pair([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0], 1.0);
    c.layers[0].matte = MatteMode::AlphaInverted;
    let f = render_frame(&c, 0.0);
    // Center (under the opaque source) is punched out.
    assert_eq!(f.pixel(32, 32)[3], 0, "center should be inverted-out");
    // An edge pixel (base present, source absent) survives.
    assert_eq!(f.pixel(2, 2)[3], 255, "edge should survive inversion");
}

#[test]
fn luma_matte_scales_alpha_by_source_brightness() {
    // White base; the luma of a darker source scales the base's alpha. A gray
    // (0.5 sRGB) source yields a partial matte: center alpha between 0 and 255.
    let mut c = matte_pair([1.0, 1.0, 1.0, 1.0], [0.5, 0.5, 0.5, 1.0], 1.0);
    c.layers[0].matte = MatteMode::Luma;
    let gray = render_frame(&c, 0.0).pixel(32, 32)[3];
    assert!((1..255).contains(&gray), "partial luma matte, got a={gray}");
    // A white source passes the base through fully.
    c.layers[1].color = [1.0, 1.0, 1.0, 1.0];
    let white = render_frame(&c, 0.0).pixel(32, 32)[3];
    assert_eq!(white, 255, "white luma should fully pass");
    // A black source mattes the base completely away.
    c.layers[1].color = [0.0, 0.0, 0.0, 1.0];
    let black = render_frame(&c, 0.0).pixel(32, 32)[3];
    assert_eq!(black, 0, "black luma should fully matte out");
}

#[test]
fn matte_preserves_base_color() {
    // The matte changes coverage only, never color: a blue base under a
    // partial luma matte still reads blue (just dimmer in alpha).
    let mut c = matte_pair([0.0, 0.0, 1.0, 1.0], [0.6, 0.6, 0.6, 1.0], 1.0);
    c.layers[0].matte = MatteMode::Luma;
    let [r, g, b, a] = render_frame(&c, 0.0).pixel(32, 32);
    assert!(a > 0, "some coverage expected");
    assert!(
        b > r && b > g,
        "base color should stay blue, got {r},{g},{b}"
    );
}

#[test]
fn export_sequence_writes_all_frames() {
    let mut c = solid([0.2, 0.6, 0.9, 1.0]);
    c.width = 16;
    c.height = 16;
    c.duration = 0.1; // 0.1s * 30fps = 3 frames
    c.fps = 30.0;
    let dir = std::env::temp_dir().join(format!("pulse_export_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let summary = export_sequence(&c, &dir, "seq", RenderRange::Full).expect("export");
    assert_eq!(summary.frames, 3);
    for i in 0..3 {
        let p = frame_path(&dir, "seq", i, 3);
        assert!(p.exists(), "missing frame {}", p.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
