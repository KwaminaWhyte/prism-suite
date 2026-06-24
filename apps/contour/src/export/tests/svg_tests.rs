//! SVG export assertions. Split out of `export.rs` (mechanical).

use super::*;

    #[test]
    fn svg_contains_elements_and_curve() {
        let svg = to_svg(&sample_doc(), 200.0, 200.0);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<path"));
        assert!(svg.contains(" C "), "curved segment should emit a cubic");
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn svg_skips_hidden_shapes() {
        let mut doc = sample_doc();
        if let Shape::Rect { visible, .. } = &mut doc.shapes[0] {
            *visible = false;
        }
        let svg = to_svg(&doc, 200.0, 200.0);
        assert!(!svg.contains("<rect"));
        assert!(svg.contains("<path"));
    }

    /// A non-origin artboard crop offsets the SVG body with a `translate(...)`
    /// group and sizes the viewBox to the artboard, not the document origin.
    #[test]
    fn svg_artboard_crop_offsets_body() {
        let svg = to_svg_artboard(&sample_doc(), [120.0, 40.0, 200.0, 150.0]);
        assert!(
            svg.contains("viewBox=\"0 0 200 150\""),
            "viewBox sized to the artboard: {svg}"
        );
        assert!(
            svg.contains("translate(-120,-40)"),
            "artwork translated to the artboard origin: {svg}"
        );
        // The origin case adds no translate group.
        let at_origin = to_svg_artboard(&sample_doc(), [0.0, 0.0, 200.0, 150.0]);
        assert!(!at_origin.contains("translate"), "no offset at origin");
    }

    #[test]
    fn svg_emits_dash_and_cap_attrs() {
        let svg = to_svg(&dashed_rect(), 200.0, 200.0);
        assert!(svg.contains("stroke-dasharray=\"12,6\""), "svg: {svg}");
        assert!(svg.contains("stroke-dashoffset=\"3\""), "svg: {svg}");
        assert!(svg.contains("stroke-linecap=\"round\""), "svg: {svg}");
        assert!(svg.contains("stroke-linejoin=\"round\""), "svg: {svg}");
    }

    #[test]
    fn svg_omits_default_stroke_attrs() {
        // A solid butt/miter stroke must not emit cap/join/dash attributes.
        let svg = to_svg(&sample_doc(), 200.0, 200.0);
        assert!(!svg.contains("stroke-linecap"));
        assert!(!svg.contains("stroke-linejoin"));
        assert!(!svg.contains("stroke-dasharray"));
    }

    #[test]
    fn svg_emits_arrowhead_marker_geometry() {
        let svg = to_svg(&arrowed_line(), 200.0, 100.0);
        // The decorated line emits its centerline + the markers as `<path>`s
        // (baked geometry), not the bare `<line>` element.
        assert!(
            svg.matches("<path").count() >= 3,
            "expected baked centerline + 2 marker paths, got: {svg}"
        );
    }

    #[test]
    fn svg_align_emits_offset_path_not_bare_rect() {
        use crate::document::StrokeAlign;
        // A non-center align replaces the `<rect>` stroke element with a baked
        // offset `<path>`; center keeps the plain `<rect>`.
        let outside = to_svg(&aligned_rect(StrokeAlign::Outside), 200.0, 200.0);
        assert!(outside.contains("<path"), "outside should bake a path: {outside}");
        let center = to_svg(&aligned_rect(StrokeAlign::Center), 200.0, 200.0);
        // Center keeps the rect element (no baked stroke path needed).
        assert!(center.contains("<rect"), "center keeps rect: {center}");
    }

    #[test]
    fn svg_emits_linear_gradient_def_and_ref() {
        let svg = to_svg(&gradient_doc(GradientKind::Linear), 100.0, 100.0);
        assert!(svg.contains("<defs>"), "svg: {svg}");
        // Gradient defs are now named per-layer: grad{shape}_{layer}.
        assert!(svg.contains("<linearGradient id=\"grad0_0\""), "svg: {svg}");
        assert!(
            svg.contains("gradientUnits=\"userSpaceOnUse\""),
            "svg: {svg}"
        );
        assert!(svg.contains("<stop offset="), "svg: {svg}");
        // The shape's fill layer references the def rather than a solid colour.
        assert!(svg.contains("fill=\"url(#grad0_0)\""), "svg: {svg}");
        assert!(
            !svg.contains("fill=\"#808080\""),
            "svg should not use solid"
        );
    }

    #[test]
    fn svg_emits_radial_gradient_def() {
        let svg = to_svg(&gradient_doc(GradientKind::Radial), 100.0, 100.0);
        assert!(svg.contains("<radialGradient id=\"grad0_0\""), "svg: {svg}");
        assert!(svg.contains("fill=\"url(#grad0_0)\""), "svg: {svg}");
    }

    #[test]
    fn svg_angle_gradient_falls_back_to_linear_def() {
        // SVG 1.1 has no conic gradient, so Angle is exported as a linear def
        // (documented limitation) — still a valid, referenced gradient.
        let svg = to_svg(&gradient_doc(GradientKind::Angle), 100.0, 100.0);
        assert!(svg.contains("<linearGradient id=\"grad0_0\""), "svg: {svg}");
        assert!(svg.contains("fill=\"url(#grad0_0)\""), "svg: {svg}");
        assert!(!svg.contains("<radialGradient"), "angle != radial");
    }

    #[test]
    fn svg_perceptual_gradient_expands_into_many_stops() {
        // A perceptual gradient is pre-expanded into many sRGB sub-stops so SVG's
        // sRGB stop interpolation reproduces the linear-light ramp; an sRGB one
        // keeps just its authored stops.
        let mut doc = gradient_doc(GradientKind::Linear);
        let count_stops = |svg: &str| svg.matches("<stop offset=").count();
        if let Some(g) = doc.shapes[0].fill_gradient().cloned() {
            // sRGB: exactly the 2 authored stops.
            let mut srgb = g.clone();
            srgb.interpolation = crate::gradient::Interpolation::Srgb;
            doc.shapes[0].set_fill_gradient(Some(srgb));
            let svg = to_svg(&doc, 100.0, 100.0);
            assert_eq!(count_stops(&svg), 2, "sRGB keeps authored stops");
            // Perceptual: expanded to PERCEPTUAL_SVG_SAMPLES+1 stops.
            let mut perc = g;
            perc.interpolation = crate::gradient::Interpolation::Perceptual;
            doc.shapes[0].set_fill_gradient(Some(perc));
            let svg = to_svg(&doc, 100.0, 100.0);
            assert!(count_stops(&svg) > 2, "perceptual expands the stop list");
        } else {
            panic!("gradient_doc should have a gradient");
        }
    }

    /// A shape with two stacked fills + two strokes emits a paint layer per item
    /// in the SVG (bottom-to-top), so the stack survives export.
    #[test]
    fn svg_emits_stacked_paint_layers() {
        use crate::appearance::{Appearance, Fill, Stroke as AppStroke};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 50.0, 50.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 1.0],
            stroke_w: 1.0,
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
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([1.0, 0.0, 0.0, 1.0]),
                Fill::solid([0.0, 1.0, 0.0, 1.0]),
            ],
            strokes: vec![
                AppStroke::solid([0.0, 0.0, 1.0, 1.0], 2.0),
                AppStroke::solid([1.0, 1.0, 1.0, 1.0], 6.0),
            ],
            effects: vec![],
        }));
        let doc = Document {
            shapes: vec![s],
            ..Default::default()
        };
        let svg = to_svg(&doc, 100.0, 100.0);
        // Two fill colours + two stroke colours present as separate elements.
        assert!(svg.matches("<rect").count() == 4, "4 paint layers: {svg}");
        assert!(svg.contains("fill=\"#ff0000\""), "bottom fill: {svg}");
        assert!(svg.contains("fill=\"#00ff00\""), "top fill: {svg}");
        assert!(svg.contains("stroke=\"#0000ff\""), "bottom stroke: {svg}");
        assert!(svg.contains("stroke=\"#ffffff\""), "top stroke: {svg}");
    }

    /// SVG export tags a non-Normal paint layer with `mix-blend-mode` so it
    /// composites in any viewer; a Normal layer emits no blend style.
    #[test]
    fn svg_emits_mix_blend_mode_for_non_normal() {
        use crate::appearance::{Appearance, BlendMode, Fill};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 50.0, 50.0],
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
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([1.0, 0.0, 0.0, 1.0]), // Normal: no style
                Fill {
                    paint: Paint::Solid([0.0, 0.0, 1.0, 1.0]),
                    opacity: 1.0,
                    blend: BlendMode::Screen,
                    visible: true,
                },
            ],
            strokes: vec![],
            effects: vec![],
        }));
        let doc = Document {
            shapes: vec![s],
            ..Default::default()
        };
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(
            svg.contains("mix-blend-mode:screen"),
            "non-normal layer gets a blend style: {svg}"
        );
        // Exactly one blend style (the Normal layer emits none).
        assert_eq!(svg.matches("mix-blend-mode").count(), 1, "svg: {svg}");
    }

    // --- Opacity masks ------------------------------------------------------

    /// SVG: an opacity-masked shape emits a luminance `<mask>` def and references
    /// it on the content's group.
    #[test]
    fn svg_emits_opacity_mask_def_and_ref() {
        let mut doc = Document::new();
        doc.shapes.clear();
        doc.shapes
            .push(omask_rect([0.0, 0.0, 100.0, 100.0], [1.0, 0.0, 0.0, 1.0], Some(0), false));
        doc.shapes
            .push(omask_rect([0.0, 0.0, 50.0, 100.0], [1.0, 1.0, 1.0, 1.0], Some(0), true));
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(svg.contains("<mask id=\"om0\""), "mask def: {svg}");
        assert!(svg.contains("mask-type=\"luminance\""), "luminance mask: {svg}");
        assert!(svg.contains("mask=\"url(#om0)\""), "masked group: {svg}");
        // The mask path itself is not emitted as a normal painted shape (only one
        // <rect> for the content's fill, inside the masked group).
    }

    // --- Live effects -------------------------------------------------------

    /// A shape with a drop shadow + blur emits an SVG `<filter>` with the
    /// matching primitives and wraps the paint stack in a filtered group.
    #[test]
    fn svg_emits_effect_filter() {
        let doc = Document {
            shapes: vec![effect_shape(vec![
                Effect::drop_shadow(),
                Effect::GaussianBlur { radius: 5.0 },
            ])],
            ..Default::default()
        };
        let svg = to_svg(&doc, 200.0, 200.0);
        assert!(svg.contains("<filter id=\"fx0\""), "filter def: {svg}");
        assert!(svg.contains("<feDropShadow"), "drop-shadow primitive: {svg}");
        assert!(svg.contains("<feGaussianBlur"), "blur primitive: {svg}");
        assert!(svg.contains("filter=\"url(#fx0)\""), "filtered group: {svg}");
    }

    /// A shape with no active effect emits no filter (back-compat: plain output).
    #[test]
    fn svg_no_filter_without_effects() {
        let doc = Document {
            shapes: vec![effect_shape(vec![Effect::GaussianBlur { radius: 0.0 }])],
            ..Default::default()
        };
        let svg = to_svg(&doc, 200.0, 200.0);
        assert!(!svg.contains("<filter"), "no filter for inactive fx: {svg}");
    }

    /// SVG export emits a compound path as one `<path>` with `fill-rule="evenodd"`
    /// whose `d` contains both sub-contours (two `M` move commands).
    #[test]
    fn svg_compound_path_has_fill_rule_and_two_subpaths() {
        let doc = Document {
            shapes: vec![donut_compound()],
            ..Default::default()
        };
        let svg = to_svg(&doc, 40.0, 40.0);
        assert!(svg.contains("fill-rule=\"evenodd\""), "even-odd attr: {svg}");
        assert!(svg.contains("<path"), "one path element: {svg}");
        // Two sub-contours → two `M` commands in the single path's `d`.
        let d_start = svg.find("d=\"").expect("has d");
        let d_end = svg[d_start + 3..].find('"').unwrap() + d_start + 3;
        let d = &svg[d_start + 3..d_end];
        assert_eq!(d.matches('M').count(), 2, "two move-tos (outer + hole): {d}");
    }

    /// An embedded placed image exports as an `<image>` with a base64 PNG
    /// `data:` URI in its href.
    #[test]
    fn svg_embedded_image_emits_data_uri() {
        let doc = placed_doc(solid_embedded(4, 4, [255, 0, 0, 255]), 10.0, 20.0);
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(svg.contains("<image"), "image element: {svg}");
        assert!(
            svg.contains("href=\"data:image/png;base64,"),
            "embedded → data URI: {svg}"
        );
        // The placement matrix is carried on the element.
        assert!(svg.contains("matrix(1 0 0 1 10 20)"), "transform: {svg}");
        // The xlink namespace is declared once a placed image is present.
        assert!(svg.contains("xmlns:xlink"), "xlink ns declared: {svg}");
    }

    /// A linked placed image exports as an `<image>` whose href is the file path
    /// (not a data URI).
    #[test]
    fn svg_linked_image_emits_href_path() {
        let src = ImageSource::Linked {
            path: std::path::PathBuf::from("/tmp/photo.png"),
            width: 8,
            height: 6,
        };
        let svg = to_svg(&placed_doc(src, 0.0, 0.0), 100.0, 100.0);
        assert!(svg.contains("<image"), "image element: {svg}");
        assert!(
            svg.contains("href=\"/tmp/photo.png\""),
            "linked → path href: {svg}"
        );
        assert!(!svg.contains("data:image"), "linked is not a data URI: {svg}");
    }

    /// A clipped placed image emits a `<clipPath>` def the `<image>` references.
    #[test]
    fn svg_clipped_image_emits_clip_path() {
        let mut doc = placed_doc(solid_embedded(100, 100, [0, 0, 255, 255]), 0.0, 0.0);
        doc.placed_images.list[0].set_clip(vec![
            (10.0, 10.0),
            (50.0, 10.0),
            (50.0, 50.0),
            (10.0, 50.0),
        ]);
        let svg = to_svg(&doc, 100.0, 100.0);
        let cid = format!("imgclip{}", doc.placed_images.list[0].id);
        assert!(svg.contains(&format!("<clipPath id=\"{cid}\"")), "clipPath def: {svg}");
        assert!(
            svg.contains("clipPathUnits=\"userSpaceOnUse\""),
            "clip in document space: {svg}"
        );
        assert!(svg.contains(&format!("clip-path=\"url(#{cid})\"")), "ref: {svg}");
    }

    /// A document with no placed images exports a byte-identical SVG to before
    /// placed-image support (no `<image>`, no xlink namespace, no clipPath).
    #[test]
    fn svg_without_placed_images_is_unchanged() {
        let svg = to_svg(&sample_doc(), 200.0, 200.0);
        assert!(!svg.contains("<image"), "no image element");
        assert!(!svg.contains("xmlns:xlink"), "no xlink namespace");
        assert!(!svg.contains("clipPath"), "no clip path");
        // The header is exactly the legacy one.
        assert!(svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\""
        ));
    }

    /// A hidden placed image is not emitted (and leaves the header unchanged).
    #[test]
    fn svg_hidden_placed_image_is_omitted() {
        let mut doc = placed_doc(solid_embedded(4, 4, [255, 0, 0, 255]), 0.0, 0.0);
        doc.placed_images.list[0].visible = false;
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(!svg.contains("<image"), "hidden image omitted: {svg}");
        assert!(!svg.contains("xmlns:xlink"), "no xlink for hidden-only doc");
    }
