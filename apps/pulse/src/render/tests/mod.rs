//! Render tests split by domain (see CLAUDE.md ~1000-line rule).
//!
//! Shared imports and cross-domain fixtures live here; each submodule
//! pulls them in via `use super::*;`.

#![allow(unused_imports)]

use super::export::{frame_count, frame_path, frame_range, frame_time, range_frame_count};
use super::*;
use crate::comp::{
    BlendMode, Camera, Interp, LayerBlend, MatteMode, MotionBlur, Prop, PulseLayer, WorkArea,
};
use crate::render::RenderRange;
use std::path::Path;
use crate::comp::{Mask, MaskMode};
use crate::comp::{Fill, ShapeItem, ShapePrimitive, Stroke};

mod basic;
mod sequence;
mod layer_kinds;
mod mattes;
mod render_range;
mod motion_blur;
mod masks;
mod spatial;
mod distort;
mod keying;
mod shapes;
mod text;
mod blend;
mod footage;
mod frame_blend;
mod precomp;
mod generate;
mod stylize;
mod camera3d;

// --- Shared test fixtures (used across multiple domain modules) ---------

pub(super) fn solid(color: [f32; 4]) -> Comp {
    let mut c = Comp {
        width: 64,
        height: 64,
        duration: 1.0,
        fps: 30.0,
        motion_blur: MotionBlur::default(),
        markers: Vec::new(),
        work_area: WorkArea::default(),
        camera: Camera::default(),
        lights: Vec::new(),
        hide_shy: false,
        layers: Vec::new(),
        id: 0,
        name: String::new(),
    };
    c.layers.push(PulseLayer::new("L", color));
    c
}

/// A 64x64 comp: a full-frame opaque solid (`base`) with a smaller solid on
/// top to serve as the matte source. Index 0 = matted base, index 1 = source.
pub(super) fn matte_pair(base: [f32; 4], source: [f32; 4], src_scale: f32) -> Comp {
    let mut c = Comp {
        width: 64,
        height: 64,
        duration: 1.0,
        fps: 30.0,
        motion_blur: MotionBlur::default(),
        markers: Vec::new(),
        work_area: WorkArea::default(),
        camera: Camera::default(),
        lights: Vec::new(),
        hide_shy: false,
        layers: Vec::new(),
        id: 0,
        name: String::new(),
    };
    let mut b = PulseLayer::new("base", base);
    b.scale.set_key(0.0, 3.0); // cover the whole frame
    c.layers.push(b); // index 0
    let mut s = PulseLayer::new("source", source);
    s.scale.set_key(0.0, src_scale);
    c.layers.push(s); // index 1
    c
}

/// A 64x64 comp with a single full-frame opaque white solid (index 0).
pub(super) fn full_frame_solid() -> Comp {
    let mut c = solid([1.0, 1.0, 1.0, 1.0]);
    c.layers[0].scale.set_key(0.0, 3.0); // cover the whole frame
    c
}
