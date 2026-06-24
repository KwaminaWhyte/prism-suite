//! Export the document to external formats: SVG (vector) and PNG (raster).
//!
//! Both exporters iterate the document in paint order (bottom-up) and skip
//! hidden shapes. SVG emits standard `rect`/`ellipse`/`line`/`path` elements;
//! PNG rasterizes via `tiny-skia` into a `Pixmap` sized to the artboard.
//!
//! Mechanically split into [`svg`] (vector export) and [`raster`] (PNG/skia
//! rasterizer) submodules; a couple of helpers shared by both live here.

use crate::appearance::{Appearance, BlendMode, Effect, Paint};
// NOTE: `Paint` here is the Appearance paint enum; tiny-skia's `Paint` is used
// fully-qualified as `tiny_skia::Paint` in the rasterizer to avoid the clash.
use crate::document::{self, Document, LineCap, LineJoin, Shape, StrokeStyle};
use crate::gradient::{Gradient, GradientKind, SpreadMode};
use tiny_skia::{
    Color as TsColor, FillRule as TsFillRule, GradientStop as TsStop, LineCap as TsCap,
    LineJoin as TsJoin, LinearGradient, Paint as TsPaint, PathBuilder, Pixmap, Point as TsPoint,
    RadialGradient, Rect as TsRect, Shader, SpreadMode as TsSpread, Stroke, StrokeDash, Transform,
};

mod svg;
mod raster;
#[cfg(test)]
mod tests;

pub use svg::*;
pub use raster::*;

// --- Helpers shared by both the SVG and raster exporters ----------------------

/// Normalize a possibly-negative-extent rect to `(x, y, w, h)` with the
/// top-left origin and non-negative width/height.
fn norm_rect(rect: &[f32; 4]) -> (f32, f32, f32, f32) {
    let x = rect[0].min(rect[0] + rect[2]);
    let y = rect[1].min(rect[1] + rect[3]);
    (x, y, rect[2].abs(), rect[3].abs())
}

/// Clone a gradient with every stop alpha scaled by `opacity`.
fn scale_grad(g: &Gradient, opacity: f32) -> Gradient {
    let mut g = g.clone();
    for s in g.stops.iter_mut() {
        s.color[3] = (s.color[3] * opacity).clamp(0.0, 1.0);
    }
    g
}
