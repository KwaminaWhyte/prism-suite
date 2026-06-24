//! PNG / raster export assertions. Split out of `export.rs` (mechanical).

use super::*;

    #[test]
    fn png_encodes_nonempty() {
        let bytes = to_png(&sample_doc(), 200.0, 200.0).expect("png should encode");
        assert!(bytes.len() > 8);
        // PNG magic signature.
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    /// A cropped PNG still encodes to a valid image at the artboard pixel size.
    #[test]
    fn png_artboard_crop_encodes() {
        let bytes =
            to_png_artboard(&sample_doc(), [50.0, 25.0, 64.0, 48.0]).expect("png should encode");
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    #[test]
    fn png_encodes_dashed_stroke() {
        // Dashed/round-cap stroking must not crash the rasterizer.
        let bytes = to_png(&dashed_rect(), 120.0, 100.0).expect("png should encode");
        assert!(bytes.len() > 8);
    }

    // --- Align stroke + arrowheads ------------------------------------------

    #[test]
    fn png_encodes_arrowheads() {
        let bytes = to_png(&arrowed_line(), 200.0, 100.0).expect("png should encode");
        assert!(bytes.len() > 8);
    }

    #[test]
    fn png_align_inside_vs_outside_differ() {
        use crate::document::StrokeAlign;
        // Inside vs outside place the stroke band on opposite sides of the path,
        // so the rasterized output must differ.
        let inside = to_png(&aligned_rect(StrokeAlign::Inside), 200.0, 200.0).unwrap();
        let outside = to_png(&aligned_rect(StrokeAlign::Outside), 200.0, 200.0).unwrap();
        assert_ne!(inside, outside, "inside/outside align should render differently");
    }

    #[test]
    fn ts_stops_maps_and_expands_correctly() {
        // The pure mapping converts every Contour stop into one tiny-skia stop;
        // its colour-space behaviour is the gradient's `render_stops` expansion
        // (verified per-colour in gradient.rs). Here we lock the cardinality and
        // the empty-gradient guard, since tiny-skia's stop fields are private.
        let srgb = Gradient {
            interpolation: crate::gradient::Interpolation::Srgb,
            ..Gradient::two_stop(GradientKind::Linear, [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.5])
        };
        assert_eq!(ts_stops(&srgb, 16).len(), 2, "sRGB maps 1:1");
        assert_eq!(
            ts_stops(&srgb, 16).len(),
            srgb.render_stops(16).len(),
            "one tiny-skia stop per Contour stop"
        );

        // Perceptual gradient: expanded to samples+1 tiny-skia stops.
        let perc = Gradient {
            interpolation: crate::gradient::Interpolation::Perceptual,
            ..Gradient::two_stop(GradientKind::Linear, [0.0; 4], [1.0, 1.0, 1.0, 1.0])
        };
        assert_eq!(ts_stops(&perc, 8).len(), 9, "perceptual expands");

        // An empty gradient yields no stops (the shader builder then bails out).
        let empty = Gradient {
            stops: vec![],
            ..Gradient::default()
        };
        assert!(ts_stops(&empty, 16).is_empty());
    }

    #[test]
    fn png_renders_angle_gradient_conic_sweep() {
        // An Angle (conic) gradient sweeps around the centre, so opposite radial
        // directions land at different parameters → different colours. Compare a
        // point above the centre with one to the right.
        let doc = gradient_doc(GradientKind::Angle);
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &doc.shapes[0], Transform::identity(), None);
        let px = |x: u32, y: u32| {
            let p = pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        // Right of centre and below centre sit at different sweep angles.
        let right = px(95, 50);
        let down = px(50, 95);
        assert_ne!(right, down, "conic sweep should vary by angle: {right:?} {down:?}");
    }

    #[test]
    fn png_renders_gradient_fill() {
        // A linear red→blue gradient should leave the left edge reddish and the
        // right edge bluish in the rasterized output.
        let doc = gradient_doc(GradientKind::Linear);
        let bytes = to_png(&doc, 100.0, 100.0).expect("png should encode");
        assert!(bytes.len() > 8);

        // Re-rasterize to a pixmap directly so we can sample pixels.
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &doc.shapes[0], Transform::identity(), None);
        let px = |x: u32, y: u32| {
            let p = pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        let (lr, _, lb) = px(2, 50);
        let (rr, _, rb) = px(97, 50);
        // Left is more red than blue; right is more blue than red.
        assert!(lr > lb, "left should be reddish: {lr},{lb}");
        assert!(rb > rr, "right should be bluish: {rr},{rb}");
    }

    /// A stacked PNG paints the top fill over the bottom one (last-on-top), so the
    /// centre samples the topmost opaque fill's colour.
    #[test]
    fn png_renders_top_of_fill_stack() {
        use crate::appearance::{Appearance, Fill};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 100.0, 100.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        };
        // Bottom red, top opaque green → centre reads green.
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([1.0, 0.0, 0.0, 1.0]),
                Fill::solid([0.0, 1.0, 0.0, 1.0]),
            ],
            strokes: vec![],
            effects: vec![],
        }));
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        let p = pixmap.pixel(50, 50).unwrap();
        assert!(
            p.green() > p.red() && p.green() > p.blue(),
            "top green fill wins: {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    // --- Blend-mode compositing ---------------------------------------------

    /// A stacked PNG with a Multiply top fill must darken where it overlaps the
    /// bottom fill (Multiply composites against the backdrop, not source-over).
    #[test]
    fn png_blend_multiply_darkens_against_backdrop() {
        use crate::appearance::{Appearance, BlendMode, Fill};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 100.0, 100.0],
            fill: [1.0, 1.0, 1.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        };
        // Bottom 60% grey, top 60% grey Multiply → 0.36 grey (much darker than
        // either layer alone, which a source-over top fill could never produce).
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([0.6, 0.6, 0.6, 1.0]),
                Fill {
                    paint: Paint::Solid([0.6, 0.6, 0.6, 1.0]),
                    opacity: 1.0,
                    blend: BlendMode::Multiply,
                    visible: true,
                },
            ],
            strokes: vec![],
            effects: vec![],
        }));
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        let p = pixmap.pixel(50, 50).unwrap();
        let expected = (0.36_f32 * 255.0).round() as i32;
        assert!(
            (p.red() as i32 - expected).abs() <= 6,
            "multiply should darken to ~{expected}, got {}",
            p.red()
        );
    }

    /// PNG: a content shape masked by a half-covering white rect is opaque under
    /// the mask and erased outside it (luminance·coverage drives alpha).
    #[test]
    fn png_opacity_mask_reveals_under_mask_hides_outside() {
        let mut doc = Document::new();
        doc.shapes.clear();
        // Red content fills the left 100×100; a white mask covers only its left
        // half (0..50). Under the mask → red shows; right of it → erased to white.
        doc.shapes
            .push(omask_rect([0.0, 0.0, 100.0, 100.0], [1.0, 0.0, 0.0, 1.0], Some(0), false));
        doc.shapes
            .push(omask_rect([0.0, 0.0, 50.0, 100.0], [1.0, 1.0, 1.0, 1.0], Some(0), true));

        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        for (i, shape) in doc.render_shapes() {
            let mask = doc.opacity_mask_of(i);
            draw_shape_skia(&mut pixmap, &shape, Transform::identity(), mask.as_ref());
        }
        // Under the white mask (x=25): red shows through.
        let under = pixmap.pixel(25, 50).unwrap();
        assert!(
            under.red() > 200 && under.green() < 80 && under.blue() < 80,
            "under mask should be red: {},{},{}",
            under.red(),
            under.green(),
            under.blue()
        );
        // Outside the mask (x=75): content hidden → page white shows.
        let outside = pixmap.pixel(75, 50).unwrap();
        assert!(
            outside.red() > 240 && outside.green() > 240 && outside.blue() > 240,
            "outside mask should be white page: {},{},{}",
            outside.red(),
            outside.green(),
            outside.blue()
        );
    }

    /// A drop-shadow PNG still encodes, and the shadow paints pixels *outside*
    /// the shape's tight bounds (down-right of it), proving the effect raster is
    /// composited onto the page.
    #[test]
    fn png_drop_shadow_paints_outside_bounds() {
        let doc = Document {
            shapes: vec![effect_shape(vec![Effect::DropShadow {
                dx: 8.0,
                dy: 8.0,
                blur: 3.0,
                color: [0.0, 0.0, 0.0, 1.0],
                opacity: 1.0,
            }])],
            ..Default::default()
        };
        let bytes = to_png(&doc, 200.0, 200.0).expect("png should encode");
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
        // Re-rasterize to sample. Shape spans doc (40,40)-(80,80) on a white page;
        // a point just past the bottom-right corner should be darkened by the
        // shadow (not pure white).
        let mut pixmap = Pixmap::new(200, 200).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &doc.shapes[0], Transform::identity(), None);
        let p = pixmap.pixel(85, 85).unwrap();
        assert!(
            p.red() < 250 && p.green() < 250 && p.blue() < 250,
            "shadow should darken just outside the shape: {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    /// An effect layer's placement: the rasterized layer's `doc_origin` sits at
    /// the padded top-left of the shape (bbox minus the effect padding).
    #[test]
    fn render_shape_layer_pads_bounds() {
        let s = effect_shape(vec![Effect::GaussianBlur { radius: 4.0 }]);
        let (path, fillable) = skia_path_of(&s).unwrap();
        let bbox = s.bounds().map(|b| [b.x, b.y, b.w, b.h]).unwrap();
        let ap = s.effective_appearance();
        let layer = render_shape_layer(&path, fillable, &bbox, &ap, 1.0).unwrap();
        // pad = 3·radius = 12, so the origin is shifted up-left by 12 from (40,40).
        assert!((layer.doc_origin.0 - 28.0).abs() < 1.0, "origin x");
        assert!((layer.doc_origin.1 - 28.0).abs() < 1.0, "origin y");
        // The layer is the padded shape (40 + 2·12 = 64 units) → ~64 px at 1×.
        assert!(layer.pixmap.width() >= 64);
    }

    // --- Compound paths -----------------------------------------------------

    /// PNG export rasterizes the even-odd hole: the frame is filled (red) but the
    /// centre (inside the hole) shows the white page.
    #[test]
    fn png_compound_path_renders_the_hole() {
        let s = donut_compound();
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        // On the frame (5,5): red fill.
        let frame = pixmap.pixel(5, 5).unwrap();
        assert!(
            frame.red() > 200 && frame.green() < 80 && frame.blue() < 80,
            "frame is red: {},{},{}",
            frame.red(),
            frame.green(),
            frame.blue()
        );
        // In the hole centre (15,15): even-odd → empty → white page shows.
        let hole = pixmap.pixel(15, 15).unwrap();
        assert!(
            hole.red() > 240 && hole.green() > 240 && hole.blue() > 240,
            "hole shows the white page: {},{},{}",
            hole.red(),
            hole.green(),
            hole.blue()
        );
    }

    /// A non-zero compound (same-wound rings) fills the centre (no hole carved).
    #[test]
    fn png_compound_non_zero_fills_centre() {
        use crate::document::FillRule;
        let mut s = donut_compound();
        if let Shape::Compound { fill_rule, .. } = &mut s {
            *fill_rule = FillRule::NonZero;
        }
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        // Same-wound rings + non-zero → the centre is filled red, not a hole.
        let centre = pixmap.pixel(15, 15).unwrap();
        assert!(
            centre.red() > 200 && centre.green() < 80,
            "non-zero fills the centre: {},{},{}",
            centre.red(),
            centre.green(),
            centre.blue()
        );
    }

    /// A text object exports as an even-odd `<path>` of glyph outlines in SVG, and
    /// rasterizes to non-blank pixels in PNG (composing / exporting like any
    /// vector).
    #[test]
    fn text_exports_to_svg_and_png() {
        let params = crate::text::TextParams {
            text: "Ag".to_string(),
            font_size: 60.0,
            align: crate::text::TextAlign::Left,
            font_family: None,
            font_axes: Default::default(),
        };
        let glyphs = crate::text::layout(&params, (20.0, 20.0)).0;
        let text = Shape::Text {
            params,
            origin: (20.0, 20.0),
            glyphs,
            fill: [0.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 1.0],
            stroke_w: 0.0,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        };
        let doc = Document {
            shapes: vec![text.clone()],
            ..Default::default()
        };
        // SVG: a single even-odd path carrying the glyph contours.
        let svg = to_svg(&doc, 200.0, 120.0);
        assert!(svg.contains("<path"), "text emits a path element");
        assert!(
            svg.contains("fill-rule=\"evenodd\""),
            "text fills even-odd so counters carve"
        );
        // PNG: the text rasterizes to at least some non-white ink.
        let mut pixmap = Pixmap::new(200, 120).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &text, Transform::identity(), None);
        let inked = pixmap
            .pixels()
            .iter()
            .any(|p| p.red() < 200 || p.green() < 200 || p.blue() < 200);
        assert!(inked, "rasterized text should leave ink on the page");
    }

    // --- Placed / linked images --------------------------------------------

    /// PNG export composites a placed image into the right pixels: a red 40×40
    /// image placed at (30,30) paints red there and leaves the rest white.
    #[test]
    fn png_bakes_placed_image_into_pixels() {
        let doc = placed_doc(solid_embedded(40, 40, [255, 0, 0, 255]), 30.0, 30.0);
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        for img in &doc.placed_images.list {
            draw_placed_image_skia(&mut pixmap, img, Transform::identity());
        }
        // Inside the image (centre at 50,50): red.
        let inside = pixmap.pixel(50, 50).unwrap();
        assert!(
            inside.red() > 200 && inside.green() < 60 && inside.blue() < 60,
            "image pixel is red: {},{},{}",
            inside.red(),
            inside.green(),
            inside.blue()
        );
        // Outside the image (5,5): the white page.
        let outside = pixmap.pixel(5, 5).unwrap();
        assert!(
            outside.red() > 240 && outside.green() > 240 && outside.blue() > 240,
            "outside is white page: {},{},{}",
            outside.red(),
            outside.green(),
            outside.blue()
        );
        // The full export still encodes a valid PNG.
        let bytes = to_png(&doc, 100.0, 100.0).expect("png encodes");
        assert_eq!(&bytes[0..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }

    /// PNG export composites the image in z-order *over* the vector artwork: an
    /// opaque image atop a shape shows the image's colour where they overlap.
    #[test]
    fn png_image_composites_over_shapes_in_z_order() {
        let mut doc = placed_doc(solid_embedded(100, 100, [0, 0, 255, 255]), 0.0, 0.0);
        // A green rect *under* the image (placed images draw last / on top).
        doc.shapes.push(Shape::Rect {
            rect: [0.0, 0.0, 100.0, 100.0],
            fill: [0.0, 1.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        });
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        let base = Transform::identity();
        for (_, shape) in doc.render_shapes() {
            draw_shape_skia(&mut pixmap, &shape, base, None);
        }
        for img in &doc.placed_images.list {
            draw_placed_image_skia(&mut pixmap, img, base);
        }
        // The opaque blue image wins over the green rect beneath it.
        let p = pixmap.pixel(50, 50).unwrap();
        assert!(
            p.blue() > 200 && p.green() < 60,
            "image (blue) on top of shape (green): {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    /// PNG export honours a clip ring: pixels outside the clip stay the page
    /// colour even though the image's quad covers them.
    #[test]
    fn png_clipped_image_only_paints_inside_clip() {
        let mut doc = placed_doc(solid_embedded(100, 100, [255, 0, 0, 255]), 0.0, 0.0);
        // Clip to a 40×40 box at (10,10)..(50,50).
        doc.placed_images.list[0].set_clip(vec![
            (10.0, 10.0),
            (50.0, 10.0),
            (50.0, 50.0),
            (10.0, 50.0),
        ]);
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        for img in &doc.placed_images.list {
            draw_placed_image_skia(&mut pixmap, img, Transform::identity());
        }
        // Inside the clip (30,30): red.
        let inside = pixmap.pixel(30, 30).unwrap();
        assert!(inside.red() > 200 && inside.green() < 60, "inside clip is red");
        // Outside the clip but inside the image quad (80,80): page white.
        let outside = pixmap.pixel(80, 80).unwrap();
        assert!(
            outside.red() > 240 && outside.green() > 240 && outside.blue() > 240,
            "outside clip is white page: {},{},{}",
            outside.red(),
            outside.green(),
            outside.blue()
        );
    }

    /// A document with no placed images rasterizes a byte-identical PNG to before
    /// (the placed-image bake loop is a no-op when the list is empty).
    #[test]
    fn png_without_placed_images_is_unchanged() {
        let doc = sample_doc();
        // Same doc, but force-clear the (already empty) placed-image list to make
        // the "no images" path explicit.
        let mut bare = doc.clone();
        bare.placed_images.list.clear();
        assert_eq!(
            to_png(&doc, 200.0, 200.0),
            to_png(&bare, 200.0, 200.0),
            "an empty placed-image list does not change the raster"
        );
    }

    /// base64 encodes the RFC 4648 test vectors (alphabet + padding).
    #[test]
    fn base64_matches_rfc_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    /// The href selection is by source kind: Embedded → data URI, Linked → path.
    #[test]
    fn placed_image_href_selects_by_source_kind() {
        let embedded = PlacedImage::new(0, "a", solid_embedded(2, 2, [1, 2, 3, 255]), 0.0, 0.0);
        let href = placed_image_href(&embedded).unwrap();
        assert!(href.starts_with("data:image/png;base64,"), "embedded → data");

        let linked = PlacedImage::new(
            1,
            "b",
            ImageSource::Linked {
                path: std::path::PathBuf::from("/a/b.png"),
                width: 2,
                height: 2,
            },
            0.0,
            0.0,
        );
        assert_eq!(placed_image_href(&linked).unwrap(), "/a/b.png", "linked → path");
    }
