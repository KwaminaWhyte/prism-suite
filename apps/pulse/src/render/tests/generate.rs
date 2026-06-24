use super::*;

// --- Generate render-path ---------------------------------------------------

use crate::comp::{CellType, FractalType, GenerateEffect, Overflow, RampShape};

/// A full-opacity default Fractal Noise that always covers (opacity 1, no clip
/// killing the center). Scale tuned so several features fall inside the quad.
fn fractal_fill() -> GenerateEffect {
    GenerateEffect::FractalNoise {
        fractal_type: FractalType::Basic,
        contrast: 1.0,
        brightness: 0.3, // lift so the field is visible (avoids near-black center)
        scale: 20.0,
        scale_x: 1.0,
        scale_y: 1.0,
        complexity: 6,
        sub_influence: 0.6,
        sub_scaling: 2.0,
        evolution: 0.0,
        seed: 0,
        overflow: Overflow::AllowHdr,
        opacity: 1.0,
    }
}

#[test]
fn generate_fills_the_layer_quad() {
    // A generate fill replaces the layer's content: the quad's center is covered
    // (alpha > 0), and a far corner outside the (unit-scale) quad — half-extent
    // ≈ 0.22·64 ≈ 14 px about the center — stays transparent.
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].generate = Some(fractal_fill());
    let f = render_frame(&c, 0.0);
    assert!(f.pixel(32, 32)[3] > 0, "generate fill covers the quad center");
    assert_eq!(f.pixel(0, 0)[3], 0, "outside the quad stays transparent");
}

#[test]
fn generate_render_is_deterministic() {
    // Same comp, same time → byte-identical frame (the cache / MFR rely on this).
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0);
    c.layers[0].generate = Some(fractal_fill());
    let a = render_frame(&c, 0.0);
    let b = render_frame(&c, 0.0);
    assert_eq!(a.pixels, b.pixels, "generate render must be deterministic");
}

#[test]
fn generate_evolution_changes_the_frame() {
    // Animating evolution must change the rendered pixels (the motion knob).
    let mut evolved = fractal_fill();
    if let GenerateEffect::FractalNoise { evolution, .. } = &mut evolved {
        *evolution = 5.0;
    }

    let mut a = solid([1.0, 1.0, 1.0, 1.0]);
    a.layers[0].scale.set_key(0.0, 3.0);
    a.layers[0].generate = Some(fractal_fill());
    let mut b = a.clone();
    b.layers[0].generate = Some(evolved);
    let fa = render_frame(&a, 0.0);
    let fb = render_frame(&b, 0.0);
    assert_ne!(
        fa.pixels, fb.pixels,
        "evolution should change the rendered frame"
    );
}

#[test]
fn generate_evolution_track_drives_the_field_over_time() {
    // A keyframed evolution track flows the field over comp time: two different
    // times render different frames. (The static field is fixed, so the change
    // comes from the track overriding it per frame.)
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.duration = 2.0;
    c.layers[0].scale.set_key(0.0, 3.0);
    c.layers[0].generate = Some(fractal_fill());
    c.layers[0].generate_evolution.set_key(0.0, 0.0);
    c.layers[0].generate_evolution.set_key(2.0, 8.0);
    let f0 = render_frame(&c, 0.0);
    let f1 = render_frame(&c, 2.0);
    assert_ne!(
        f0.pixels, f1.pixels,
        "a keyframed evolution track should flow the field over time"
    );
    // And it's still deterministic at a fixed time.
    assert_eq!(render_frame(&c, 1.0).pixels, render_frame(&c, 1.0).pixels);
}

#[test]
fn generate_color_correction_applies_to_field() {
    // The layer's per-pixel effect stack runs on the generated grayscale: a Tint
    // mapping black→red, white→red drives the field red, so the green channel of
    // a covered pixel drops to ~0.
    use crate::comp::Effect;
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0);
    c.layers[0].generate = Some(fractal_fill());
    c.layers[0].effects.push(Effect::Tint {
        black: [1.0, 0.0, 0.0],
        white: [1.0, 0.0, 0.0],
        amount: 1.0,
    });
    let f = render_frame(&c, 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert!(a > 0, "covered");
    assert!(r > g && r > b, "tinted red: r={r} g={g} b={b}");
    assert_eq!(g, 0, "fully red tint zeroes green");
}

#[test]
fn demo_comp_with_fractal_noise_renders() {
    // The launch demo ships a keyframed-evolution Fractal Noise layer; render it
    // at a couple of times to confirm the full demo composites without panicking
    // and the noise contributes (the field flows, so two times differ).
    let c = Comp::new();
    let f0 = render_frame(&c, 0.0);
    let f2 = render_frame(&c, 2.5);
    assert_eq!(f0.pixels.len(), (c.width * c.height * 4) as usize);
    assert_ne!(f0.pixels, f2.pixels, "the demo evolves over time");
}

#[test]
fn preview_is_downscaled_full_frame_not_center_crop() {
    // Regression: `render_preview_frame` must render the comp at native size and
    // uniformly downscale, NOT shrink `comp.width/height` (which center-crops the
    // layout — the demo Title, positioned above the comp center, would fall off
    // the top of the preview). Lock the preview to exactly the downscaled native
    // frame.
    let mut c = Comp::new();
    // Disable comp motion blur so the preview path doesn't clamp sub-frame
    // samples (it does for performance) — this test isolates the geometry /
    // downscale property, which is independent of motion blur.
    c.motion_blur.enabled = false;
    let mut cache = crate::comp::FrameCache::new();
    let prev = render_preview_frame(&[c.clone()], c.id, 1.0, 640, &mut cache);
    assert_eq!(
        (prev.width, prev.height),
        preview_dims(c.width, c.height, 640),
        "preview keeps the comp aspect at the capped long edge"
    );
    let native = render_frame_in_project(&[c.clone()], c.id, 1.0, &mut cache);
    assert_eq!((native.width, native.height), (c.width, c.height));
    let expected = downscale_frame(native, prev.width, prev.height);
    assert_eq!(
        prev.pixels, expected.pixels,
        "preview must be the downscaled full frame, not a center crop"
    );
}

#[test]
fn generate_on_non_generate_layer_is_inert_when_none() {
    // A solid without a generate fill renders exactly as before (no regression).
    let mut with_none = solid([0.2, 0.6, 0.9, 1.0]);
    with_none.layers[0].scale.set_key(0.0, 2.0);
    let baseline = render_frame(&with_none, 0.0);
    // Setting then clearing the generate slot returns to the baseline.
    with_none.layers[0].generate = Some(fractal_fill());
    with_none.layers[0].generate = None;
    let cleared = render_frame(&with_none, 0.0);
    assert_eq!(baseline.pixels, cleared.pixels, "cleared generate is inert");
}

// --- Generate (colour generators) render-path -------------------------------

/// A solid scaled 3× so its quad covers the whole 64×64 frame, with `gen` filling
/// it. The center pixel (32,32) maps to layer-local (0,0).
fn generated(gen: GenerateEffect) -> Comp {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0);
    c.layers[0].generate = Some(gen);
    c
}

#[test]
fn ramp_render_fills_quad_with_gradient() {
    // A vertical black→white ramp across the layer: the top of the quad is dark,
    // the bottom bright, and the whole quad is covered.
    let ramp = GenerateEffect::Ramp {
        shape: RampShape::Linear,
        start: [0.0, -40.0],
        end: [0.0, 40.0],
        radius: 40.0,
        start_color: [0.0, 0.0, 0.0],
        end_color: [1.0, 1.0, 1.0],
        scatter: 0.0,
        opacity: 1.0,
    };
    let f = render_frame(&generated(ramp), 0.0);
    let top = f.pixel(32, 18); // near the top of the quad
    let bot = f.pixel(32, 46); // near the bottom
    assert!(top[3] > 0 && bot[3] > 0, "ramp covers the quad");
    assert!(
        bot[0] > top[0] + 20,
        "ramp gets brighter top→bottom: top={} bot={}",
        top[0],
        bot[0]
    );
}

#[test]
fn checkerboard_render_alternates_cells() {
    // A checkerboard whose cells are small enough that several fall inside the
    // (scale-3, ±14 local-px) quad: adjacent cells differ. Cell size 8 local px →
    // cell 0 spans local [0,8), cell 1 [8,16). The layer is scaled 3×, so a comp
    // pixel maps to local = (comp - 32) / 3.
    let checker = GenerateEffect::Checkerboard {
        anchor: [0.0, 0.0],
        size_w: 8.0,
        size_h: 8.0,
        color1: [0.0, 0.0, 0.0],
        color2: [1.0, 1.0, 1.0],
        opacity: 1.0,
    };
    let f = render_frame(&generated(checker), 0.0);
    // comp 34 → local ~0.67 (cell 0, black); comp 60 → local ~9.3 (cell 1, white).
    let a = f.pixel(34, 32)[0];
    let b = f.pixel(60, 32)[0];
    assert_ne!(a, b, "adjacent checker cells differ: {a} vs {b}");
    assert!(a.max(b) > 200 && a.min(b) < 60, "one cell white, one black");
}

#[test]
fn four_color_render_blends_corners() {
    // Distinct corner colours blend across the quad; the four corners read close
    // to their colours and the centre is a mix (not equal to any single corner).
    let g = GenerateEffect::FourColorGradient {
        tl: [1.0, 0.0, 0.0],
        tr: [0.0, 1.0, 0.0],
        bl: [0.0, 0.0, 1.0],
        br: [1.0, 1.0, 0.0],
        blend: 1.0,
        jitter: 0.0,
        opacity: 1.0,
    };
    let f = render_frame(&generated(g), 0.0);
    // Quad spans ~[-42,42] local → comp ~[-10,74], clamped to the frame. Sample
    // inside each quadrant; the dominant channel reflects that corner's colour.
    let tl = f.pixel(20, 20); // toward top-left → red dominant
    let tr = f.pixel(44, 20); // toward top-right → green dominant
    assert!(tl[0] > tl[1] && tl[0] > tl[2], "top-left reds: {tl:?}");
    assert!(tr[1] > tr[0] && tr[1] > tr[2], "top-right greens: {tr:?}");
}

#[test]
fn grid_render_lines_over_transparent_background() {
    // A grid with a transparent background: the line at the quad centre is opaque,
    // a cell interior is transparent.
    let grid = GenerateEffect::Grid {
        anchor: [0.0, 0.0],
        size_w: 20.0,
        size_h: 20.0,
        border: 4.0,
        color: [1.0, 1.0, 1.0],
        background: [0.0, 0.0, 0.0],
        background_opacity: 0.0,
        opacity: 1.0,
    };
    let f = render_frame(&generated(grid), 0.0);
    // Local (0,0) (comp 32,32) sits on a grid line → opaque.
    assert_eq!(f.pixel(32, 32)[3], 255, "grid line is opaque");
    // Local (10,10) (comp 42,42) is a cell interior → transparent.
    assert_eq!(f.pixel(42, 42)[3], 0, "cell interior is transparent");
}

#[test]
fn color_generator_render_is_deterministic() {
    // Each colour generator renders byte-identically across passes (cache / MFR).
    for i in 1..GenerateEffect::defaults().len() {
        let c = generated(GenerateEffect::defaults()[i]);
        let a = render_frame(&c, 0.0);
        let b = render_frame(&c, 0.0);
        assert_eq!(
            a.pixels,
            b.pixels,
            "{} render must be deterministic",
            GenerateEffect::defaults()[i].label()
        );
    }
}

#[test]
fn checkerboard_color_decodes_through_srgb() {
    // A checkerboard with a known sRGB colour 1 round-trips through the
    // sRGB→linear compositor decode and back to ~the same 8-bit value on output.
    let checker = GenerateEffect::Checkerboard {
        anchor: [0.0, 0.0],
        size_w: 128.0, // one big cell over the quad → cell (0,0) = color1
        size_h: 128.0,
        color1: [0.5, 0.25, 0.75],
        color2: [0.0, 0.0, 0.0],
        opacity: 1.0,
    };
    let f = render_frame(&generated(checker), 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert_eq!(a, 255, "opaque cell");
    // Output 8-bit ≈ the sRGB input (within rounding of the decode/encode trip).
    assert!((r as i32 - 128).abs() <= 3, "r ~128, got {r}");
    assert!((g as i32 - 64).abs() <= 3, "g ~64, got {g}");
    assert!((b as i32 - 191).abs() <= 3, "b ~191, got {b}");
}

#[test]
fn cell_pattern_render_fills_quad_grayscale() {
    // Cell Pattern is grayscale-linear (like Fractal Noise): it fills the quad, and
    // a covered pixel is grey (R ≈ G ≈ B) rather than a tinted colour.
    let cells = GenerateEffect::CellPattern {
        cell_type: CellType::Crystals,
        size: 8.0,
        disorder: 1.0,
        contrast: 1.0,
        brightness: 0.3, // lift so the centre is visibly covered
        invert: false,
        evolution: 0.0,
        seed: 0,
        opacity: 1.0,
    };
    let f = render_frame(&generated(cells), 0.0);
    let [r, g, b, a] = f.pixel(32, 32);
    assert!(a > 0, "cell pattern covers the quad centre");
    assert!(
        (r as i32 - g as i32).abs() <= 1 && (g as i32 - b as i32).abs() <= 1,
        "grayscale fill: r={r} g={g} b={b}"
    );
}

#[test]
fn cell_pattern_evolution_track_drives_the_field_over_time() {
    // A keyframed evolution track flows the cells over comp time (Cell Pattern
    // shares the keyframable evolution infra with Fractal Noise): two times differ.
    let mut c = generated(GenerateEffect::CellPattern {
        cell_type: CellType::Bubbles,
        size: 8.0,
        disorder: 1.0,
        contrast: 1.0,
        brightness: 0.0,
        invert: false,
        evolution: 0.0,
        seed: 0,
        opacity: 1.0,
    });
    c.duration = 2.0;
    c.layers[0].generate_evolution.set_key(0.0, 0.0);
    c.layers[0].generate_evolution.set_key(2.0, 8.0);
    let f0 = render_frame(&c, 0.0);
    let f1 = render_frame(&c, 2.0);
    assert_ne!(
        f0.pixels, f1.pixels,
        "a keyframed evolution track should flow the cells over time"
    );
    // And it's still deterministic at a fixed time.
    assert_eq!(render_frame(&c, 1.0).pixels, render_frame(&c, 1.0).pixels);
}
