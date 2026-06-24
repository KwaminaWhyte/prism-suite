use super::path::{
    anchors_in_rect, delete_anchor, handle_endpoints, insert_anchor, is_corner, make_corner,
    make_smooth, segment_count, toggle_anchor_smooth,
};
use super::*;
use crate::text::{TextAlign, TextParams};
use crate::transform::Affine;

mod artboards;
mod clip_mask;
mod compound;
mod gradient_group;
mod layers;
mod path_edit;
mod serde_compat;
mod stroke_style;
mod swatches;
mod text;
mod transforms;

// --- Shared test fixtures ---------------------------------------------
//
// These helpers build the canonical shapes used across the split test
// modules; each sub-module reaches them via `use super::*;`.

/// Build a live polygon / star `Shape::Path` centred at the origin (the form the
/// Polygon / Star tools create). Mirrors `ContourApp::live_shape_at`.
fn live_path(live: crate::liveshape::LiveShape) -> Shape {
    let (points, handles) = live.outline((0.0, 0.0));
    Shape::Path {
        points,
        closed: true,
        fill: [0.0; 4],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
        handles,
        live: Some(live),
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

/// A simple three-anchor open corner path for the Direct-Select shape tests.
fn open_path() -> Shape {
    Shape::Path {
        points: vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)],
        closed: false,
        fill: [0.0; 4],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
        appearance: None,
        handles: vec![(0.0, 0.0); 3],
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
    }
}

/// Helper: a styled rect with explicit clip tagging, for clip-set tests.
fn clip_rect(rect: [f32; 4], clip: Option<u64>, mask: bool) -> Shape {
    Shape::Rect {
        rect,
        fill: [0.5, 0.5, 0.5, 1.0],
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 1.0,
        stroke_style: StrokeStyle::default(),
        appearance: None,
        visible: true,
        group: None,
        clip,
        mask,
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

/// A styled rect with explicit opacity-mask tagging, for opacity-mask tests.
fn omask_rect(rect: [f32; 4], fill: [f32; 4], omask: Option<u64>, mask: bool) -> Shape {
    Shape::Rect {
        rect,
        fill,
        fill_gradient: None,
        stroke: [0.0, 0.0, 0.0, 1.0],
        stroke_w: 1.0,
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

/// A test rect with the given fill / stroke colours.
fn swatch_rect(fill: [f32; 4], stroke: [f32; 4]) -> Shape {
    Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
        fill,
        fill_gradient: None,
        stroke,
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
    }
}

/// A compound path: a 30×30 outer ring with a 10×10 inner hole sub-contour, with
/// the given fill rule.
fn donut(fill_rule: FillRule) -> Shape {
    let outer = SubPath::ring(vec![
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 30.0),
        (0.0, 30.0),
    ]);
    let inner = SubPath::ring(vec![
        (10.0, 10.0),
        (20.0, 10.0),
        (20.0, 20.0),
        (10.0, 20.0),
    ]);
    Shape::Compound {
        subpaths: vec![outer, inner],
        fill_rule,
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
    }
}

/// Build a plain 10×10 red rectangle at the origin for the Layers-panel tests.
fn layer_rect() -> Shape {
    Shape::Rect {
        rect: [0.0, 0.0, 10.0, 10.0],
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
    }
}

/// A point-type text object with the given string at `origin`, glyphs laid out.
fn text_shape(text: &str, origin: (f32, f32)) -> Shape {
    let params = TextParams {
        text: text.to_string(),
        font_size: 72.0,
        align: TextAlign::Left,
        font_family: None,
        font_axes: Default::default(),
    };
    let glyphs = crate::text::layout(&params, origin).0;
    Shape::Text {
        params,
        origin,
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
    }
}

/// Build a placed point-type [`Shape::Text`] at `origin` for the text-placement
/// regression tests below: glyph cache laid out immediately, every additive
/// field at its default, so it matches what the Type tool produces.
fn placed_text(text: &str, font_size: f32, origin: (f32, f32)) -> Shape {
    use crate::text::TextParams;
    let params = TextParams {
        text: text.to_string(),
        font_size,
        align: crate::text::TextAlign::Left,
        font_family: None,
        font_axes: Default::default(),
    };
    let glyphs = crate::text::layout(&params, origin).0;
    Shape::Text {
        params,
        origin,
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
    }
}
