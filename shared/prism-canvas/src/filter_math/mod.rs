//! CPU reference implementations of the blur-family filters that
//! [`filter.wgsl`](../shaders/filter.wgsl) runs on the GPU.
//!
//! These are *not* used at runtime (the live path is the GPU shader pass in
//! [`compositor.rs`](super::compositor)); they exist so the averaging math
//! (motion-blur direction/length, separable box-blur normalization, radial
//! spin vs zoom) can be unit-tested deterministically without a GPU adapter,
//! mirroring the kernel the shader implements.
//!
//! Pixels are RGBA stored as `[f32; 4]` per texel in **linear premultiplied**
//! space (the working space of the compositor), so a plain component average is
//! a correct blur. Sampling uses edge-clamped bilinear, matching the WGSL
//! `textureSample` with a clamp-to-edge sampler.
//!
//! ## Module map
//!
//! The math is split by filter family; this root re-exports every public item so
//! callers keep using `filter_math::box_blur`, `filter_math::twirl`, etc.:
//! - `blur` — motion / box / Gaussian / High Pass / radial blur
//! - `gallery_blur` — Blur Gallery: Tilt-Shift / Iris / Field (variable radius)
//! - `distort` — Twirl / Pinch / Ripple / Polar coordinate remaps
//! - `stylize` — Find Edges / Emboss / Glowing Edges / Oil Paint / Diffuse
//! - `noise` — Add Noise / Median / Dust & Scratches
//! - `pixelate` — Mosaic / Crystallize / Color Halftone / Mezzotint
//! - `render` — Clouds / Difference Clouds (fBm generators)
//! - `tonal` — Posterize / Threshold (display-space quantizers)

mod blur;
mod distort;
mod gallery_blur;
mod noise;
mod pixelate;
mod render;
mod stylize;
mod tonal;

// `filter_math` is a private, test-only module: these re-exports preserve the
// `filter_math::<fn>` paths the CPU references are addressed by (and that the GPU
// compositor docs reference) without any non-test caller, hence the allow.
#[allow(unused_imports)]
mod reexports {
    pub use super::blur::{
        box_blur, box_blur_axis, gaussian_blur, gaussian_blur_axis, high_pass, motion_blur,
        radial_blur, RadialMode,
    };
    pub use super::distort::{pinch, polar, ripple, twirl, PolarMode};
    pub use super::gallery_blur::{
        field_blur, field_blur_radius_at, focus_weight, iris_blur, iris_weight, tilt_shift,
        FieldPin,
    };
    pub use super::noise::{add_noise, dust_and_scratches, median};
    pub use super::pixelate::{color_halftone, crystallize, mezzotint, mosaic};
    pub use super::render::{clouds, difference_clouds, fbm};
    pub use super::stylize::{diffuse, emboss, find_edges, glowing_edges, oil_paint};
    pub use super::tonal::{posterize, threshold};
}
#[allow(unused_imports)]
pub use reexports::*;

/// `2π` — shared by the distort, stylize and noise families' angular math.
pub(crate) const TAU: f32 = std::f32::consts::TAU;

/// Edge-clamped bilinear sample at floating-point pixel coords `(x, y)`.
/// `img` is row-major `w*h` RGBA. Coordinates are in pixel space where integer
/// `i` addresses the *center* of pixel `i` at `i + 0.0` (texel centers handled
/// by the caller adding 0.5 where it matches the shader's uv convention).
pub fn sample_bilinear(img: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    if w == 0 || h == 0 {
        return [0.0; 4];
    }
    let xc = x.clamp(0.0, (w - 1) as f32);
    let yc = y.clamp(0.0, (h - 1) as f32);
    let x0 = xc.floor() as usize;
    let y0 = yc.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = xc - x0 as f32;
    let fy = yc - y0 as f32;
    let p00 = img[y0 * w + x0];
    let p10 = img[y0 * w + x1];
    let p01 = img[y1 * w + x0];
    let p11 = img[y1 * w + x1];
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        let top = p00[c] * (1.0 - fx) + p10[c] * fx;
        let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
        out[c] = top * (1.0 - fy) + bot * fy;
    }
    out
}

/// Canonical `fract(sin(..))` hash → a stable scalar in [0,1) for integer pixel
/// `(ix, iy)` and `seed`. Bit-for-bit the WGSL `hash21` (and the diffuse hash),
/// so the CPU reference reproduces the shader's noise. Shared by the noise,
/// pixelate and render families.
// The multipliers mirror the canonical GLSL/WGSL hash; keep them verbatim.
// NB: use the floor-based fractional part (`x - floor(x)`) — WGSL's `fract`
// always returns a value in [0,1), whereas Rust's `f32::fract` keeps the sign of
// the input (so negatives would fall outside [0,1) and diverge from the shader).
#[allow(clippy::excessive_precision)]
pub(crate) fn hash21(ix: f32, iy: f32, seed: f32) -> f32 {
    let v = (ix * 12.9898 + iy * 78.233 + seed).sin() * 43758.5453;
    v - v.floor()
}

/// Shared deterministic test fixtures used across the family test modules.
#[cfg(test)]
pub(crate) mod test_support {
    pub fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    /// A 9x9 image: a single white opaque pixel at the center, else transparent.
    pub fn impulse(n: usize) -> Vec<[f32; 4]> {
        let mut img = vec![[0.0f32; 4]; n * n];
        img[(n / 2) * n + n / 2] = [1.0, 1.0, 1.0, 1.0];
        img
    }

    /// An `n×n` opaque image split by a vertical edge at column `n/2`: black on
    /// the left, white on the right. A clean vertical gradient (gx≠0, gy≈0).
    pub fn vsplit(n: usize) -> Vec<[f32; 4]> {
        let mut img = vec![[0.0, 0.0, 0.0, 1.0]; n * n];
        for y in 0..n {
            for x in (n / 2)..n {
                img[y * n + x] = [1.0, 1.0, 1.0, 1.0];
            }
        }
        img
    }

    /// A horizontal black→white ramp; sampling its color reveals which source x
    /// a remap pulled from (color encodes the x coordinate).
    pub fn ramp_x(n: usize) -> Vec<[f32; 4]> {
        let mut img = vec![[0.0f32; 4]; n * n];
        for y in 0..n {
            for x in 0..n {
                let v = x as f32 / (n - 1) as f32;
                img[y * n + x] = [v, v, v, 1.0];
            }
        }
        img
    }

    /// A flat opaque mid-gray field.
    pub fn flat_gray(n: usize, v: f32) -> Vec<[f32; 4]> {
        vec![[v, v, v, 1.0]; n * n]
    }

    pub fn channel_mean(img: &[[f32; 4]], ch: usize) -> f32 {
        img.iter().map(|p| p[ch]).sum::<f32>() / img.len() as f32
    }
}
