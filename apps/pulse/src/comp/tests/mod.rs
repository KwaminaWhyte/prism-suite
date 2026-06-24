//! Tests for `comp` split by domain (see CLAUDE.md ~1000-line rule).
//!
//! Shared imports and cross-domain test fixtures live here; each
//! submodule pulls them in via `use super::*;`.

#![allow(unused_imports)]

use super::distort::sample_bilinear;
use super::effect::{curve_eval, hsl_to_rgb, rgb_to_hsl, smoothstep};
use super::keyframe::{cubic_bezier, solve_bezier_x, Keyframe};
use super::mask::{dist_to_polygon, point_in_polygon};
use super::spatial::{box_blur, directional_blur, gaussian_blur, gaussian_kernel, radial_blur};
use super::*;

mod keyframes;
mod transform;
mod layers;
mod effects;
mod mattes_blur;
mod masks;
mod spatial;
mod distort;
mod keying;
mod precomp;
mod motion_path;
mod presets;
mod roving;
mod camera3d;

// --- Shared test fixtures (used across multiple domain modules) ---------

pub(super) fn approx(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
}

pub(super) fn parented_comp() -> Comp {
    let mut c = Comp {
        width: 100,
        height: 100,
        duration: 1.0,
        fps: 30.0,
        motion_blur: MotionBlur::default(),
        markers: Vec::new(),
        work_area: WorkArea::default(),
        camera: Camera::default(),
        lights: Vec::new(),
        layers: Vec::new(),
        hide_shy: false,
        id: 0,
        name: String::new(),
    };
    c.layers.push(PulseLayer::new("parent", [1.0; 4])); // 0
    c.layers.push(PulseLayer::new("child", [1.0; 4])); // 1
    c
}
