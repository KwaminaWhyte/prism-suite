//! GPU-backed tests for the canvas compositor.
//!
//! These run against the real wgpu device when an adapter is present and
//! skip (returning early after an `eprintln!`) when none is available, so
//! they are safe in headless CI. Tests are grouped by pass/feature into the
//! submodules below; the shared fixtures/helpers live here.

use super::*;

mod compositor;
mod adjustments;
mod blend;
mod layer_styles;
mod filters_blur;
mod filters_distort;
mod filters_noise_pixelate;
mod filters_render_tonal;
mod smart_filters;
mod blur_gallery;

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// A solid `n`x`n` buffer of linear-premultiplied RGBA16F bytes.
fn solid(n: u32, r: f32, g: f32, b: f32, a: f32) -> Vec<u8> {
    let px = [r * a, g * a, b * a, a];
    let mut o = Vec::new();
    for _ in 0..(n * n) {
        for &c in &px {
            o.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
        }
    }
    o
}

/// A plain, no-style LayerDraw for `id` — new style tests fill only the fields
/// they exercise via struct-update syntax.
fn base_draw(id: LayerId) -> LayerDraw {
    LayerDraw {
        id,
        opacity: 1.0,
        blend: 0,
        visible: true,
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

/// A 16x16 layer with a 4x4 opaque white square at x6..10, y6..10; rest empty.
#[cfg(test)]
fn square_16() -> Vec<u8> {
    let mut buf = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            let px = if (6..10).contains(&x) && (6..10).contains(&y) {
                [1.0f32, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            for &c in &px {
                buf.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
            }
        }
    }
    buf
}

/// Build an `n`x`n` RGBA16F layer buffer from a per-pixel closure returning
/// straight-linear `[r,g,b,a]`; stored premultiplied (the working space).
#[cfg(test)]
fn layer_from(n: u32, f: impl Fn(u32, u32) -> [f32; 4]) -> Vec<u8> {
    let mut buf = Vec::with_capacity((n * n * 8) as usize);
    for y in 0..n {
        for x in 0..n {
            let p = f(x, y);
            let a = p[3];
            for c in &[p[0] * a, p[1] * a, p[2] * a, a] {
                buf.extend_from_slice(&half::f16::from_f32(*c).to_le_bytes());
            }
        }
    }
    buf
}


// A horizontal black→white opaque ramp: the sampled red value encodes which
// source column a coordinate remap pulled from.
#[cfg(test)]
fn ramp_layer(n: u32) -> Vec<u8> {
    layer_from(n, |x, _y| {
        let v = x as f32 / (n - 1) as f32;
        [v, v, v, 1.0]
    })
}

