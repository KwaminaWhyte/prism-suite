//! **Fractal Noise** + **Turbulent Displace** — two standard After Effects
//! effects, implemented as deterministic CPU passes.
//!
//! - **Fractal Noise** synthesises a grayscale field by summing several octaves
//!   of value noise (each octave doubles the frequency and halves the
//!   amplitude). A fixed integer `seed` makes the field fully reproducible:
//!   identical params always yield an identical buffer.
//! - **Turbulent Displace** warps an input by offsetting each pixel's sample
//!   coordinate by a vector read from a noise field (`amount` px, scaled by
//!   `size` / `complexity`), the classic "heat-haze / liquid" distortion.
//!
//! Both kernels are free functions so they can be unit-tested without an `App`;
//! the `App` impl holds per-layer config and the panel actions. Storage is
//! app-side (these don't touch the engine's effect stacks) to keep the engine
//! crate unchanged.

use std::collections::HashMap;

use super::{App, Action};

/// Per-layer **Fractal Noise** configuration (After Effects' Fractal Noise).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FractalNoiseConfig {
    /// Deterministic seed — identical seeds reproduce the field exactly.
    pub seed: u32,
    /// Octaves (detail layers) summed; each doubles frequency, halves amplitude.
    pub octaves: u32,
    /// Base spatial frequency (cells across the unit square). Larger = finer.
    pub frequency: f32,
    /// Per-octave amplitude falloff (After Effects' *Sub Influence*); `0.5` is
    /// the standard `1/f` fractal.
    pub persistence: f32,
    /// Output contrast multiplier applied to the `[0,1]` field.
    pub contrast: f32,
    /// Output brightness offset added after contrast.
    pub brightness: f32,
    /// Evolution (animates the field — fed in as a Z offset to the noise).
    pub evolution: f32,
}

impl Default for FractalNoiseConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            octaves: 5,
            frequency: 4.0,
            persistence: 0.5,
            contrast: 1.0,
            brightness: 0.0,
            evolution: 0.0,
        }
    }
}

/// Per-layer **Turbulent Displace** configuration (After Effects' Turbulent
/// Displace).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurbulentDisplaceConfig {
    /// Maximum displacement in pixels (the noise field's `[-1,1]` × `amount`).
    pub amount: f32,
    /// Spatial scale of the displacement noise (larger = broader swirls).
    pub size: f32,
    /// Octaves of detail in the displacement field.
    pub complexity: u32,
    /// Deterministic seed for the displacement noise.
    pub seed: u32,
    /// Evolution offset (animates the swirl).
    pub evolution: f32,
}

impl Default for TurbulentDisplaceConfig {
    fn default() -> Self {
        Self {
            amount: 20.0,
            size: 50.0,
            complexity: 3,
            seed: 0,
            evolution: 0.0,
        }
    }
}

impl App {
    /// Apply a Fractal-Noise / Turbulent-Displace [`Action`]. Dispatched from
    /// [`App::apply`].
    pub(super) fn apply_effects_noise(&mut self, action: Action) {
        match action {
            Action::AddFractalNoise { layer_id } => {
                self.fractal_noise.insert(layer_id, FractalNoiseConfig::default());
                self.host.mark_dirty();
            }
            Action::RemoveFractalNoise { layer_id } => {
                self.fractal_noise.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::SetFractalNoiseParam { layer_id, param, value } => {
                let cfg = self.fractal_noise.entry(layer_id).or_default();
                match param {
                    "frequency" => cfg.frequency = value.max(0.01),
                    "persistence" => cfg.persistence = value.clamp(0.0, 1.0),
                    "contrast" => cfg.contrast = value.max(0.0),
                    "brightness" => cfg.brightness = value,
                    "evolution" => cfg.evolution = value,
                    _ => {}
                }
                self.host.mark_dirty();
            }
            Action::SetFractalNoiseSeed { layer_id, seed } => {
                self.fractal_noise.entry(layer_id).or_default().seed = seed;
                self.host.mark_dirty();
            }
            Action::SetFractalNoiseOctaves { layer_id, octaves } => {
                self.fractal_noise.entry(layer_id).or_default().octaves = octaves.clamp(1, 10);
                self.host.mark_dirty();
            }
            Action::AddTurbulentDisplace { layer_id } => {
                self.turbulent_displace.insert(layer_id, TurbulentDisplaceConfig::default());
                self.host.mark_dirty();
            }
            Action::RemoveTurbulentDisplace { layer_id } => {
                self.turbulent_displace.remove(&layer_id);
                self.host.mark_dirty();
            }
            Action::SetTurbulentDisplaceParam { layer_id, param, value } => {
                let cfg = self.turbulent_displace.entry(layer_id).or_default();
                match param {
                    "amount" => cfg.amount = value,
                    "size" => cfg.size = value.max(0.01),
                    "evolution" => cfg.evolution = value,
                    _ => {}
                }
                self.host.mark_dirty();
            }
            Action::SetTurbulentDisplaceComplexity { layer_id, complexity } => {
                self.turbulent_displace.entry(layer_id).or_default().complexity = complexity.clamp(1, 10);
                self.host.mark_dirty();
            }
            _ => unreachable!("apply_effects_noise called with wrong action"),
        }
    }

    /// Render the `layer_id` fractal-noise field to a `width × height` grayscale
    /// buffer (`Vec<f32>`, `[0,1]`, row-major). `None` if the layer has no
    /// configured fractal noise.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_fractal_noise(&self, layer_id: usize, width: usize, height: usize) -> Option<Vec<f32>> {
        let cfg = self.fractal_noise.get(&layer_id)?;
        Some(fractal_noise_buffer(*cfg, width, height))
    }
}

// ── Deterministic value-noise kernels ────────────────────────────────────────

/// A reproducible hash of three integer lattice coords + a seed into `[0, 1)`.
/// Pure integer mixing (no float RNG) so the field is bit-stable across runs and
/// platforms — the determinism the spec requires.
fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((x as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add((y as u32).wrapping_mul(0xC2B2_AE35))
        .wrapping_add((z as u32).wrapping_mul(0x27D4_EB2F));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491_4F6C_DD1D_u64 as u32);
    h ^= h >> 13;
    (h as f32) / (u32::MAX as f32)
}

/// Smoothstep fade (Perlin's `6t⁵−15t⁴+10t³`) for C¹-continuous interpolation.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Trilinearly-interpolated value noise at `(x, y, z)` for an integer `seed`,
/// using [`hash3`] lattice values and a [`fade`] curve. Returns `[0, 1)`.
fn value_noise(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (xf, yf, zf) = (x - xi as f32, y - yi as f32, z - zi as f32);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));

    let c000 = hash3(xi, yi, zi, seed);
    let c100 = hash3(xi + 1, yi, zi, seed);
    let c010 = hash3(xi, yi + 1, zi, seed);
    let c110 = hash3(xi + 1, yi + 1, zi, seed);
    let c001 = hash3(xi, yi, zi + 1, seed);
    let c101 = hash3(xi + 1, yi, zi + 1, seed);
    let c011 = hash3(xi, yi + 1, zi + 1, seed);
    let c111 = hash3(xi + 1, yi + 1, zi + 1, seed);

    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(c000, c100, u);
    let x10 = lerp(c010, c110, u);
    let x01 = lerp(c001, c101, u);
    let x11 = lerp(c011, c111, u);
    let y0 = lerp(x00, x10, v);
    let y1 = lerp(x01, x11, v);
    lerp(y0, y1, w)
}

/// Sum `octaves` of value noise (fractal Brownian motion) at `(x, y, z)`,
/// doubling frequency and scaling amplitude by `persistence` each octave, then
/// normalising back to `[0, 1]`. The fractal field the Fractal-Noise effect
/// produces.
pub fn fractal_noise(x: f32, y: f32, z: f32, octaves: u32, persistence: f32, seed: u32) -> f32 {
    let octaves = octaves.clamp(1, 12);
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut max_amp = 0.0;
    for o in 0..octaves {
        // Vary the seed per octave so octaves don't align into visible grids.
        total += value_noise(x * frequency, y * frequency, z * frequency, seed.wrapping_add(o * 1013))
            * amplitude;
        max_amp += amplitude;
        amplitude *= persistence;
        frequency *= 2.0;
    }
    if max_amp > 0.0 {
        (total / max_amp).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Render a full **Fractal Noise** buffer: a `width × height` grayscale field
/// (`[0,1]`, row-major) sampled from [`fractal_noise`] over the unit square at
/// the config's frequency / octaves / evolution, then contrast/brightness
/// graded. Deterministic for a fixed [`FractalNoiseConfig`].
pub fn fractal_noise_buffer(cfg: FractalNoiseConfig, width: usize, height: usize) -> Vec<f32> {
    let mut buf = vec![0.0_f32; width.max(1) * height.max(1)];
    if width == 0 || height == 0 {
        return buf;
    }
    let freq = cfg.frequency.max(0.01);
    let z = cfg.evolution;
    for py in 0..height {
        let v = (py as f32 + 0.5) / height as f32;
        for px in 0..width {
            let u = (px as f32 + 0.5) / width as f32;
            let n = fractal_noise(u * freq, v * freq, z, cfg.octaves, cfg.persistence, cfg.seed);
            // Grade: contrast about 0.5 then brightness offset.
            let graded = ((n - 0.5) * cfg.contrast + 0.5 + cfg.brightness).clamp(0.0, 1.0);
            buf[py * width + px] = graded;
        }
    }
    buf
}

/// The **displacement vector** `(dx, dy)` (pixels) at unit coords `(u, v)` for a
/// Turbulent-Displace config: two decorrelated fractal-noise fields (one per
/// axis, via offset seeds) remapped to `[-1, 1]` and scaled by `amount`.
pub fn turbulent_displacement(cfg: TurbulentDisplaceConfig, u: f32, v: f32) -> (f32, f32) {
    let scale = (100.0 / cfg.size.max(0.01)).max(0.01);
    let z = cfg.evolution;
    let nx = fractal_noise(u * scale, v * scale, z, cfg.complexity, 0.5, cfg.seed);
    let ny = fractal_noise(u * scale, v * scale, z + 100.0, cfg.complexity, 0.5, cfg.seed ^ 0x5BD1_E995);
    let dx = (nx * 2.0 - 1.0) * cfg.amount;
    let dy = (ny * 2.0 - 1.0) * cfg.amount;
    (dx, dy)
}

/// Apply **Turbulent Displace** to a single-channel `width × height` buffer:
/// each output pixel reads the input at its position **offset** by the noise
/// displacement (nearest-sample, edge-clamped). Returns a new buffer; the input
/// is unchanged. Deterministic for a fixed config + input.
pub fn turbulent_displace_buffer(
    input: &[f32],
    width: usize,
    height: usize,
    cfg: TurbulentDisplaceConfig,
) -> Vec<f32> {
    let mut out = vec![0.0_f32; input.len()];
    if width == 0 || height == 0 {
        return out;
    }
    for py in 0..height {
        let v = (py as f32 + 0.5) / height as f32;
        for px in 0..width {
            let u = (px as f32 + 0.5) / width as f32;
            let (dx, dy) = turbulent_displacement(cfg, u, v);
            let sx = ((px as f32 + dx).round() as i32).clamp(0, width as i32 - 1) as usize;
            let sy = ((py as f32 + dy).round() as i32).clamp(0, height as i32 - 1) as usize;
            out[py * width + px] = input[sy * width + sx];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractal_noise_is_deterministic() {
        // Identical params + seed must produce a byte-identical buffer.
        let cfg = FractalNoiseConfig { seed: 42, ..FractalNoiseConfig::default() };
        let a = fractal_noise_buffer(cfg, 32, 32);
        let b = fractal_noise_buffer(cfg, 32, 32);
        assert_eq!(a, b, "same seed/params ⇒ reproducible field");
    }

    #[test]
    fn fractal_noise_seed_changes_field() {
        let a = fractal_noise_buffer(FractalNoiseConfig { seed: 1, ..Default::default() }, 32, 32);
        let b = fractal_noise_buffer(FractalNoiseConfig { seed: 2, ..Default::default() }, 32, 32);
        assert_ne!(a, b, "different seeds ⇒ different fields");
    }

    #[test]
    fn fractal_noise_in_range() {
        let buf = fractal_noise_buffer(FractalNoiseConfig::default(), 24, 24);
        assert!(buf.iter().all(|&v| (0.0..=1.0).contains(&v)), "field stays in [0,1]");
    }

    #[test]
    fn fractal_noise_brightness_offset() {
        // A large brightness pushes the whole field toward white.
        let dark = fractal_noise_buffer(FractalNoiseConfig { brightness: 0.0, ..Default::default() }, 16, 16);
        let bright = fractal_noise_buffer(FractalNoiseConfig { brightness: 0.6, ..Default::default() }, 16, 16);
        let avg = |b: &[f32]| b.iter().sum::<f32>() / b.len() as f32;
        assert!(avg(&bright) > avg(&dark), "brightness raises the field mean");
    }

    #[test]
    fn fractal_noise_evolution_animates() {
        let a = fractal_noise_buffer(FractalNoiseConfig { evolution: 0.0, ..Default::default() }, 32, 32);
        let b = fractal_noise_buffer(FractalNoiseConfig { evolution: 5.0, ..Default::default() }, 32, 32);
        assert_ne!(a, b, "evolution flows the field over time");
    }

    #[test]
    fn value_noise_is_smooth_at_lattice() {
        // Value noise equals its lattice hash exactly at integer coords.
        let s = 7;
        let n = value_noise(3.0, 4.0, 0.0, s);
        let h = hash3(3, 4, 0, s);
        assert!((n - h).abs() < 1e-5, "value noise interpolates lattice values");
    }

    #[test]
    fn turbulent_displacement_bounded_by_amount() {
        let cfg = TurbulentDisplaceConfig { amount: 30.0, ..Default::default() };
        for i in 0..50 {
            let u = i as f32 / 50.0;
            let (dx, dy) = turbulent_displacement(cfg, u, u * 0.7);
            assert!(dx.abs() <= 30.0 + 1e-3 && dy.abs() <= 30.0 + 1e-3, "|disp| <= amount");
        }
    }

    #[test]
    fn turbulent_displacement_is_deterministic() {
        let cfg = TurbulentDisplaceConfig { seed: 9, ..Default::default() };
        assert_eq!(turbulent_displacement(cfg, 0.3, 0.6), turbulent_displacement(cfg, 0.3, 0.6));
    }

    #[test]
    fn turbulent_displace_zero_amount_is_identity() {
        // amount = 0 ⇒ no offset ⇒ the buffer is unchanged.
        let w = 16;
        let h = 16;
        let input: Vec<f32> = (0..w * h).map(|i| (i % 7) as f32 / 6.0).collect();
        let cfg = TurbulentDisplaceConfig { amount: 0.0, ..Default::default() };
        let out = turbulent_displace_buffer(&input, w, h, cfg);
        assert_eq!(out, input, "zero displacement is a no-op");
    }

    #[test]
    fn turbulent_displace_warps_with_amount() {
        let w = 32;
        let h = 32;
        // A vertical gradient so displacement changes pixel values.
        let input: Vec<f32> = (0..w * h).map(|i| (i / w) as f32 / h as f32).collect();
        let cfg = TurbulentDisplaceConfig { amount: 12.0, ..Default::default() };
        let out = turbulent_displace_buffer(&input, w, h, cfg);
        assert_ne!(out, input, "non-zero displacement warps the buffer");
        assert!(out.iter().all(|&v| (0.0..=1.0).contains(&v)), "samples stay in range");
    }

    #[test]
    fn add_remove_fractal_noise_action() {
        let mut app = App::new();
        app.apply(Action::AddFractalNoise { layer_id: 0 });
        assert!(app.fractal_noise.contains_key(&0));
        app.apply(Action::SetFractalNoiseSeed { layer_id: 0, seed: 99 });
        assert_eq!(app.fractal_noise[&0].seed, 99);
        app.apply(Action::RemoveFractalNoise { layer_id: 0 });
        assert!(!app.fractal_noise.contains_key(&0));
    }

    #[test]
    fn fractal_noise_params_clamp() {
        let mut app = App::new();
        app.apply(Action::AddFractalNoise { layer_id: 0 });
        app.apply(Action::SetFractalNoiseParam { layer_id: 0, param: "frequency", value: -5.0 });
        app.apply(Action::SetFractalNoiseParam { layer_id: 0, param: "persistence", value: 2.0 });
        app.apply(Action::SetFractalNoiseOctaves { layer_id: 0, octaves: 50 });
        let cfg = app.fractal_noise[&0];
        assert!(cfg.frequency >= 0.01);
        assert!((cfg.persistence - 1.0).abs() < 1e-4);
        assert_eq!(cfg.octaves, 10);
    }

    #[test]
    fn render_fractal_noise_through_app() {
        let mut app = App::new();
        assert!(app.render_fractal_noise(0, 8, 8).is_none());
        app.apply(Action::AddFractalNoise { layer_id: 0 });
        let buf = app.render_fractal_noise(0, 8, 8).expect("buffer");
        assert_eq!(buf.len(), 64);
    }

    #[test]
    fn turbulent_displace_actions() {
        let mut app = App::new();
        app.apply(Action::AddTurbulentDisplace { layer_id: 0 });
        app.apply(Action::SetTurbulentDisplaceParam { layer_id: 0, param: "size", value: -10.0 });
        app.apply(Action::SetTurbulentDisplaceComplexity { layer_id: 0, complexity: 99 });
        let cfg = app.turbulent_displace[&0];
        assert!(cfg.size >= 0.01, "size clamps to > 0");
        assert_eq!(cfg.complexity, 10);
        app.apply(Action::RemoveTurbulentDisplace { layer_id: 0 });
        assert!(!app.turbulent_displace.contains_key(&0));
    }
}

/// Type aliases used by the `App` field declarations in `mod.rs`.
pub type FractalNoiseMap = HashMap<usize, FractalNoiseConfig>;
pub type TurbulentDisplaceMap = HashMap<usize, TurbulentDisplaceConfig>;
