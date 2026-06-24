//! Public value types of the canvas engine: the brush [`Dab`], the per-frame
//! [`LayerDraw`] compositing description, a smart-filter [`SmartPass`], a
//! [`SelectionOp`] request, and the [`ViewTransform`] pan/zoom state. These are
//! the host-facing inputs the engine consumes each frame; the GPU-side uniform
//! mirrors live in [`crate::uniforms`]. Re-exported from the crate root.

use prism_core::LayerId;

/// One instanced brush dab, in document pixel space; color is straight linear.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Dab {
    pub center: [f32; 2],
    pub radius: f32,
    pub hardness: f32,
    pub color: [f32; 4],
}

/// Per-frame layer compositing parameters supplied by the app.
#[derive(Clone, Copy)]
pub struct LayerDraw {
    pub id: LayerId,
    pub opacity: f32,
    pub blend: u32,
    pub visible: bool,
    /// 0 = raster layer; else an adjustment kind applied to the backdrop.
    pub adjust_kind: u32,
    pub adjust: [f32; 4],
    /// Channel-Mixer matrix (adjust_kind 14): per-output [from_r,from_g,from_b,const].
    pub mix_r: [f32; 4],
    pub mix_g: [f32; 4],
    pub mix_b: [f32; 4],
    /// Blend-If: gate the layer by its own + the backdrop's luma.
    pub has_blend_if: bool,
    pub blend_if: [f32; 4], // [this_black, this_white, under_black, under_white]
    /// Clip to the layer directly below (its alpha gates this layer).
    pub clipped: bool,
    /// Outer-stroke layer style.
    pub has_stroke: bool,
    pub stroke_color: [f32; 4],
    pub stroke_width: f32, // uv units
    /// Drop-shadow layer style.
    pub has_shadow: bool,
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2], // uv units
    pub shadow_blur: f32,        // uv units
    /// Color-overlay layer style (a = strength).
    pub has_overlay: bool,
    pub overlay_color: [f32; 4],
    /// Inner-shadow layer style.
    pub has_inner_shadow: bool,
    pub inner_shadow_color: [f32; 4],
    pub inner_shadow_offset: [f32; 2], // uv units
    pub inner_shadow_blur: f32,        // uv units
    /// Outer-glow layer style.
    pub has_outer_glow: bool,
    pub outer_glow_color: [f32; 4],
    pub outer_glow_size: f32, // uv units
    /// Inner-glow layer style.
    pub has_inner_glow: bool,
    pub inner_glow_color: [f32; 4],
    pub inner_glow_size: f32, // uv units
    /// Gradient-overlay layer style.
    pub has_grad_overlay: bool,
    pub grad_color0: [f32; 4],
    pub grad_color1: [f32; 4],
    pub grad_angle: f32,   // radians
    pub grad_opacity: f32, // 0..1
    /// Bevel-&-emboss layer style (Inner Bevel).
    pub has_bevel: bool,
    pub bevel_highlight: [f32; 4], // straight rgba (a = opacity)
    pub bevel_shadow: [f32; 4],    // straight rgba (a = opacity)
    pub bevel_size: f32,           // edge width, uv units
    pub bevel_soften: f32,         // extra blur of the normal field, uv units
    pub bevel_angle: f32,          // light azimuth, radians
    pub bevel_altitude: f32,       // light altitude, radians
}

impl LayerDraw {
    /// A plain raster draw: no adjustment, identity channel mixer, no layer
    /// styles, no blend-if, not clipped. Only id/opacity/blend/visible vary.
    pub fn basic(id: LayerId, opacity: f32, blend: u32, visible: bool) -> Self {
        Self {
            id,
            opacity,
            blend,
            visible,
            adjust_kind: 0,
            adjust: [0.0; 4],
            mix_r: [1.0, 0.0, 0.0, 0.0],
            mix_g: [0.0, 1.0, 0.0, 0.0],
            mix_b: [0.0, 0.0, 1.0, 0.0],
            has_blend_if: false,
            blend_if: [0.0, 1.0, 0.0, 1.0],
            clipped: false,
            has_stroke: false,
            stroke_color: [0.0; 4],
            stroke_width: 0.0,
            has_shadow: false,
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_blur: 0.0,
            has_overlay: false,
            overlay_color: [0.0; 4],
            has_inner_shadow: false,
            inner_shadow_color: [0.0; 4],
            inner_shadow_offset: [0.0; 2],
            inner_shadow_blur: 0.0,
            has_outer_glow: false,
            outer_glow_color: [0.0; 4],
            outer_glow_size: 0.0,
            has_inner_glow: false,
            inner_glow_color: [0.0; 4],
            inner_glow_size: 0.0,
            has_grad_overlay: false,
            grad_color0: [0.0; 4],
            grad_color1: [0.0; 4],
            grad_angle: 0.0,
            grad_opacity: 0.0,
            has_bevel: false,
            bevel_highlight: [0.0; 4],
            bevel_shadow: [0.0; 4],
            bevel_size: 0.0,
            bevel_soften: 0.0,
            bevel_angle: 0.0,
            bevel_altitude: 0.0,
        }
    }
}

#[cfg(test)]
mod layer_draw_tests {
    use super::LayerDraw;
    use prism_core::LayerId;

    #[test]
    fn basic_builds() {
        let d = LayerDraw::basic(LayerId(7), 0.5, 3, true);
        assert_eq!(d.id, LayerId(7));
        assert_eq!(d.opacity, 0.5);
        assert_eq!(d.blend, 3);
        assert!(d.visible);
        assert_eq!(d.adjust_kind, 0);
        assert!(!d.has_blend_if);
        assert!(!d.clipped);
        assert_eq!(d.mix_r, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(d.mix_g, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(d.mix_b, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(d.blend_if, [0.0, 1.0, 0.0, 1.0]);
    }
}

/// One enabled smart filter's GPU pass parameters, as consumed by
/// [`CanvasGpu::reapply_smart_filters`]: the shader `kind`, the scalar `radius` /
/// `amount` the simple kinds use, and the `cr` overflow payload that the
/// multi-parameter kinds (Camera Raw, shader kind 32) read instead. `cr` is all
/// zeros for the scalar kinds — and all-zero Camera Raw params are an exact
/// no-op, so an unset payload never disturbs a pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmartPass {
    pub kind: u32,
    pub radius: f32,
    pub amount: f32,
    pub cr: [f32; 12],
}

impl SmartPass {
    /// A scalar (radius/amount) pass with an empty Camera Raw payload.
    pub fn scalar(kind: u32, radius: f32, amount: f32) -> Self {
        Self {
            kind,
            radius,
            amount,
            cr: [0.0; 12],
        }
    }
}

/// A selection operation requested by the app for this frame.
#[derive(Clone, Copy)]
pub enum SelectionOp {
    /// Replace the selection with a rectangle/ellipse marquee (doc px).
    #[allow(dead_code)]
    Marquee {
        rect: [f32; 4],
        ellipse: bool,
    },
    All,
    None,
    Invert,
}

/// Pan/zoom state for the viewport. `pan` is in egui points, but the engine is
/// egui-agnostic, so it is stored as a `glam::Vec2`; the app converts at the
/// egui boundary.
#[derive(Clone, Copy, Debug)]
pub struct ViewTransform {
    pub pan: glam::Vec2,
    pub zoom: f32,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            pan: glam::Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl ViewTransform {
    pub fn zoom_to(&mut self, factor: f32, anchor_from_center: glam::Vec2) {
        let new_zoom = (self.zoom * factor).clamp(0.02, 64.0);
        let real = new_zoom / self.zoom;
        self.pan = (self.pan - anchor_from_center) * real + anchor_from_center;
        self.zoom = new_zoom;
    }
}
