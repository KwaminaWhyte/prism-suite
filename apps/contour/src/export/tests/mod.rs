//! Tests for the export module. Split out of `export.rs` (mechanical).
//!
//! Shared fixture builders live here; the SVG and raster assertions live in
//! the [`svg_tests`] / [`raster_tests`] submodules.

use super::*;
use crate::document::Shape;
use crate::placed_image::{ImageSource, PlacedImage};

mod svg_tests;
mod raster_tests;

    fn sample_doc() -> Document {
        Document {
            shapes: vec![
                Shape::Rect {
                    rect: [10.0, 10.0, 40.0, 30.0],
                    fill: [1.0, 0.0, 0.0, 1.0],
                    fill_gradient: None,
                    stroke: [0.0, 0.0, 0.0, 1.0],
                    stroke_w: 2.0,
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
                },
                Shape::Path {
                    points: vec![(60.0, 60.0), (90.0, 60.0), (90.0, 90.0)],
                    closed: true,
                    fill: [0.0, 0.0, 1.0, 1.0],
                    fill_gradient: None,
                    stroke: [0.0, 0.0, 0.0, 1.0],
                    stroke_w: 1.0,
                    stroke_style: StrokeStyle::default(),
                    appearance: None,
                    handles: vec![(10.0, 0.0), (0.0, 0.0), (0.0, 0.0)],
                    live: None,
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
                },
            ],
            ..Default::default()
        }
    }

    fn dashed_rect() -> Document {
        Document {
            shapes: vec![Shape::Rect {
                rect: [10.0, 10.0, 80.0, 60.0],
                fill: [0.0, 0.0, 0.0, 0.0],
                fill_gradient: None,
                stroke: [0.0, 0.0, 0.0, 1.0],
                stroke_w: 4.0,
                stroke_style: StrokeStyle {
                    cap: LineCap::Round,
                    join: LineJoin::Round,
                    miter_limit: 4.0,
                    dash: vec![12.0, 6.0],
                    dash_offset: 3.0,
                    ..StrokeStyle::default()
                },
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
            }],
            ..Default::default()
        }
    }

    /// An open line with an end triangle arrowhead and a start circle.
    fn arrowed_line() -> Document {
        Document {
            shapes: vec![Shape::Line {
                p0: (20.0, 50.0),
                p1: (180.0, 50.0),
                stroke: [0.0, 0.0, 0.0, 1.0],
                stroke_w: 6.0,
                stroke_style: StrokeStyle {
                    start_arrow: crate::document::Arrowhead::Circle,
                    end_arrow: crate::document::Arrowhead::Triangle,
                    arrow_scale: 1.5,
                    ..StrokeStyle::default()
                },
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
            }],
            ..Default::default()
        }
    }

    /// A rect stroked with the requested align.
    fn aligned_rect(align: crate::document::StrokeAlign) -> Document {
        Document {
            shapes: vec![Shape::Rect {
                rect: [40.0, 40.0, 100.0, 80.0],
                fill: [0.0, 0.0, 0.0, 0.0],
                fill_gradient: None,
                stroke: [0.0, 0.0, 0.0, 1.0],
                stroke_w: 12.0,
                stroke_style: StrokeStyle {
                    align,
                    ..StrokeStyle::default()
                },
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
            }],
            ..Default::default()
        }
    }

    fn gradient_doc(kind: GradientKind) -> Document {
        Document {
            shapes: vec![Shape::Rect {
                rect: [0.0, 0.0, 100.0, 100.0],
                fill: [0.5, 0.5, 0.5, 1.0],
                fill_gradient: Some(Gradient::two_stop(
                    kind,
                    [1.0, 0.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0, 1.0],
                )),
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
            }],
            ..Default::default()
        }
    }

    fn omask_rect(rect: [f32; 4], fill: [f32; 4], omask: Option<u64>, mask: bool) -> Shape {
        Shape::Rect {
            rect,
            fill,
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask,
            omask_path: mask,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        }
    }

    fn effect_shape(effects: Vec<Effect>) -> Shape {
        use crate::appearance::{Appearance, Fill};
        let mut s = Shape::Rect {
            rect: [40.0, 40.0, 40.0, 40.0],
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
            fills: vec![Fill::solid([1.0, 0.0, 0.0, 1.0])],
            strokes: vec![],
            effects,
        }));
        s
    }

    /// A compound path: a 30×30 outer ring with a 10×10 inner hole, even-odd.
    fn donut_compound() -> Shape {
        use crate::document::{FillRule, SubPath};
        Shape::Compound {
            subpaths: vec![
                SubPath::ring(vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]),
                SubPath::ring(vec![
                    (10.0, 10.0),
                    (20.0, 10.0),
                    (20.0, 20.0),
                    (10.0, 20.0),
                ]),
            ],
            fill_rule: FillRule::EvenOdd,
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
        }
    }

    /// A solid-colour embedded source `w×h` (straight RGBA8).
    fn solid_embedded(w: u32, h: u32, rgba: [u8; 4]) -> ImageSource {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            px.extend_from_slice(&rgba);
        }
        ImageSource::Embedded {
            width: w,
            height: h,
            rgba: px,
        }
    }

    /// A document with one embedded placed image at `(x, y)`, no shapes.
    fn placed_doc(source: ImageSource, x: f32, y: f32) -> Document {
        let mut doc = Document {
            shapes: vec![],
            ..Default::default()
        };
        doc.placed_images.place("img", source, x, y);
        doc
    }
