//! **Particle System** — a seeded, fully deterministic 2D particle simulator
//! (Pulse's analog of After Effects' *CC Particle World* / *Particle
//! Playground*).
//!
//! Given an [`EmitterConfig`] and a frame time `t`, [`simulate_particles`]
//! returns every **live** particle at that instant — position, velocity, age,
//! size, opacity and color. There is no global RNG and no wall-clock: each
//! particle's birth time is `index / emission_rate`, and every random draw for a
//! particle (cone angle, speed, lifetime) comes from a [`SplitMix64`] seeded by
//! `(emitter_seed, particle_index)`. The trajectory is obtained by **integrating
//! real physics** — gravity, air-resistance drag and optional value-noise
//! turbulence — from the particle's birth to `t` with a fixed sub-step `dt`. The
//! result is therefore bit-reproducible: sampling the same `t` twice (or
//! re-opening the project tomorrow) yields the identical particle set.
//!
//! The simulator is a set of free functions so it can be unit-tested without an
//! `App`; the `App` impl below just holds per-layer [`EmitterConfig`]s and the
//! panel actions (mirroring `effects_noise.rs` — app-side storage that doesn't
//! touch the engine's effect stacks, so the engine crate stays unchanged).

use std::collections::HashMap;

use super::effects_noise::fractal_noise;
use super::{Action, App};

// ── Configuration ────────────────────────────────────────────────────────────

/// A particle **emitter**: everything needed to deterministically reproduce its
/// particle stream. Identical configs + the same `t` always yield identical
/// particles (see [`simulate_particles`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitterConfig {
    /// Master seed — every particle derives its randomness from `(seed, index)`.
    pub seed: u64,
    /// Emitter origin in comp pixels (`+y` points down, matching comp space).
    pub position: (f32, f32),
    /// Emission rate in particles per second. Particle `i` is born at `i / rate`.
    pub emission_rate: f32,
    /// How long (seconds) the emitter stays active. No particle is born at or
    /// after this time; together with the lifetime this bounds the live set.
    pub emit_duration: f32,
    /// Emission direction in degrees (`0` = `+x`, `90` = `+y` / downward).
    pub direction: f32,
    /// Cone spread in degrees: each particle's angle is jittered by
    /// `±spread/2` around [`direction`](Self::direction).
    pub spread: f32,
    /// Initial speed in px/s along the (jittered) emission direction.
    pub speed: f32,
    /// Per-particle speed jitter (`speed ± speed_variance`, px/s).
    pub speed_variance: f32,
    /// Base particle lifetime in seconds.
    pub lifetime: f32,
    /// Per-particle lifetime jitter (`lifetime ± lifetime_variance`, seconds).
    pub lifetime_variance: f32,
    /// Constant gravity acceleration in px/s² (`+y` pulls particles down).
    pub gravity: (f32, f32),
    /// Air-resistance coefficient (1/s): each step damps velocity by `drag·dt`.
    pub drag: f32,
    /// Turbulence acceleration strength in px/s² (`0` disables it).
    pub turbulence: f32,
    /// Spatial scale of the turbulence field (cells per pixel; smaller = broader).
    pub turbulence_scale: f32,
    /// Particle size at birth (px).
    pub size_start: f32,
    /// Particle size at death (px) — linearly interpolated over life.
    pub size_end: f32,
    /// Peak opacity (`[0,1]`) before the fade-in/out envelope is applied.
    pub opacity: f32,
    /// Fraction of life spent fading **in** from 0 → `opacity` (`[0,1]`).
    pub fade_in: f32,
    /// Fraction of life spent fading **out** from `opacity` → 0 (`[0,1]`).
    pub fade_out: f32,
    /// RGBA color at birth.
    pub color_start: [f32; 4],
    /// RGBA color at death — linearly interpolated over life.
    pub color_end: [f32; 4],
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            position: (0.0, 0.0),
            emission_rate: 30.0,
            emit_duration: 3.0,
            direction: -90.0, // upward (comp space has +y down)
            spread: 30.0,
            speed: 200.0,
            speed_variance: 40.0,
            lifetime: 2.0,
            lifetime_variance: 0.5,
            gravity: (0.0, 120.0),
            drag: 0.1,
            turbulence: 0.0,
            turbulence_scale: 0.01,
            size_start: 8.0,
            size_end: 1.0,
            opacity: 1.0,
            fade_in: 0.1,
            fade_out: 0.3,
            color_start: [1.0, 0.8, 0.2, 1.0],
            color_end: [1.0, 0.1, 0.0, 0.0],
        }
    }
}

/// A single live particle sampled at a frame time, as returned by
/// [`simulate_particles`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// The particle's emission index (`birth = index / emission_rate`).
    pub index: u64,
    /// Current position in comp pixels.
    pub position: (f32, f32),
    /// Current velocity in px/s.
    pub velocity: (f32, f32),
    /// Time alive in seconds (`t - birth`).
    pub age: f32,
    /// This particle's total lifetime in seconds (after variance).
    pub life: f32,
    /// Current size in pixels (size-over-life).
    pub size: f32,
    /// Current opacity in `[0,1]` (fade-in/out envelope applied).
    pub opacity: f32,
    /// Current RGBA color (color-over-life).
    pub color: [f32; 4],
}

// ── Deterministic RNG: SplitMix64 ────────────────────────────────────────────

/// A SplitMix64 pseudo-random generator. Tiny, fast and (critically) fully
/// deterministic: seeded purely from integers, it reproduces the same stream on
/// every run and platform. Each particle gets its own generator seeded by
/// `(emitter_seed, particle_index)`, so the simulation never depends on a global
/// RNG, `Instant`, or iteration order.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seed a generator for particle `index` of an emitter with `emitter_seed`.
    /// The two integers are mixed into the initial state so adjacent particles
    /// (and adjacent seeds) produce decorrelated streams.
    fn seeded(emitter_seed: u64, index: u64) -> Self {
        let mut state = emitter_seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(index.wrapping_add(1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        state ^= state >> 33;
        Self { state }
    }

    /// Advance the generator and return the next 64-bit value.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Next float in `[0, 1)` (top 24 bits → unit interval).
    fn next_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / ((1u64 << 24) as f32)
    }

    /// Next float in `[-1, 1)` — used for symmetric jitter around a base value.
    fn next_signed(&mut self) -> f32 {
        self.next_unit() * 2.0 - 1.0
    }
}

// ── Pure math helpers ────────────────────────────────────────────────────────

/// Linear interpolation `a + (b - a)·t`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Component-wise linear interpolation of two RGBA colors.
fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
        lerp(a[3], b[3], t),
    ]
}

/// Particle **size** at normalized life `u ∈ [0,1]` — a straight ramp from
/// `size_start` to `size_end`.
pub fn size_over_life(cfg: &EmitterConfig, u: f32) -> f32 {
    lerp(cfg.size_start, cfg.size_end, u.clamp(0.0, 1.0)).max(0.0)
}

/// Particle **opacity** at normalized life `u ∈ [0,1]`: `opacity`, modulated by a
/// fade-in over the first `fade_in` fraction of life and a fade-out over the last
/// `fade_out` fraction. With either fade > 0 the value reaches ~0 at that end.
pub fn opacity_over_life(cfg: &EmitterConfig, u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let mut o = cfg.opacity.clamp(0.0, 1.0);
    let fi = cfg.fade_in.clamp(0.0, 1.0);
    let fo = cfg.fade_out.clamp(0.0, 1.0);
    if fi > 0.0 && u < fi {
        o *= (u / fi).clamp(0.0, 1.0);
    }
    if fo > 0.0 && u > 1.0 - fo {
        o *= ((1.0 - u) / fo).clamp(0.0, 1.0);
    }
    o
}

/// Particle **color** at normalized life `u ∈ [0,1]` — a straight RGBA lerp from
/// `color_start` to `color_end`.
pub fn color_over_life(cfg: &EmitterConfig, u: f32) -> [f32; 4] {
    lerp4(cfg.color_start, cfg.color_end, u.clamp(0.0, 1.0))
}

// ── Physics integration ──────────────────────────────────────────────────────

/// Fixed simulation sub-step (seconds). The trajectory is integrated from a
/// particle's birth to the frame time in steps of this size (plus one shorter
/// remainder step) so the sampled state at any `t` is reproducible regardless of
/// the comp frame rate.
const SIM_DT: f32 = 1.0 / 120.0;

/// Hard cap on particles considered per frame — a safety net so a runaway rate ×
/// time can't allocate unbounded memory. Far above any realistic emitter.
const MAX_PARTICLES: u64 = 100_000;

/// Salt mixed into the emitter seed for the turbulence noise field, so a given
/// seed's turbulence is decorrelated from its per-particle jitter stream.
const TURB_SEED_SALT: u32 = 0x7193_2A1F;

/// The turbulence **acceleration** (px/s²) at world position `pos` and the
/// per-step simulation time, read from two decorrelated value-noise fields
/// (reusing `effects_noise::fractal_noise`) remapped to `[-1, 1]` and scaled by
/// the configured strength. Deterministic in the emitter seed. `(0,0)` when
/// turbulence is disabled.
fn turbulence_accel(cfg: &EmitterConfig, pos: (f32, f32), sim_time: f32) -> (f32, f32) {
    if cfg.turbulence == 0.0 {
        return (0.0, 0.0);
    }
    let scale = cfg.turbulence_scale.abs().max(1e-6);
    let x = pos.0 * scale;
    let y = pos.1 * scale;
    let seed = (cfg.seed as u32) ^ TURB_SEED_SALT;
    let nx = fractal_noise(x, y, sim_time, 3, 0.5, seed);
    let ny = fractal_noise(x, y, sim_time + 37.0, 3, 0.5, seed ^ 0x5BD1_E995);
    (
        (nx * 2.0 - 1.0) * cfg.turbulence,
        (ny * 2.0 - 1.0) * cfg.turbulence,
    )
}

/// Advance `pos` / `vel` by one sub-step `dt` under the config's forces:
/// gravity, then air-resistance drag, then turbulence, then position. Semi-
/// implicit (symplectic) Euler — velocity is updated before it integrates the
/// position — which is stable and what game/VFX particle systems use.
fn step(cfg: &EmitterConfig, pos: &mut (f32, f32), vel: &mut (f32, f32), dt: f32, sim_time: f32) {
    // Gravity.
    vel.0 += cfg.gravity.0 * dt;
    vel.1 += cfg.gravity.1 * dt;
    // Air resistance: exponential-style velocity damping.
    let damp = (1.0 - cfg.drag.max(0.0) * dt).clamp(0.0, 1.0);
    vel.0 *= damp;
    vel.1 *= damp;
    // Turbulence.
    let (ax, ay) = turbulence_accel(cfg, *pos, sim_time);
    vel.0 += ax * dt;
    vel.1 += ay * dt;
    // Integrate position with the updated velocity.
    pos.0 += vel.0 * dt;
    pos.1 += vel.1 * dt;
}

/// Integrate a particle from birth (at `cfg.position`, velocity `vel0`) forward
/// by `age` seconds, returning its `(position, velocity)`. Uses whole [`SIM_DT`]
/// steps plus a final shorter remainder step so the result is exact and
/// reproducible for any `age`.
fn integrate(cfg: &EmitterConfig, vel0: (f32, f32), age: f32) -> ((f32, f32), (f32, f32)) {
    let mut pos = cfg.position;
    let mut vel = vel0;
    if age <= 0.0 {
        return (pos, vel);
    }
    let steps = (age / SIM_DT).floor() as u64;
    let mut sim_time = 0.0_f32;
    for _ in 0..steps {
        step(cfg, &mut pos, &mut vel, SIM_DT, sim_time);
        sim_time += SIM_DT;
    }
    let rem = age - steps as f32 * SIM_DT;
    if rem > 1e-7 {
        step(cfg, &mut pos, &mut vel, rem, sim_time);
    }
    (pos, vel)
}

// ── The simulator ────────────────────────────────────────────────────────────

/// Simulate the emitter and return every **live** particle at frame time `t`.
///
/// Deterministic: the returned `Vec` depends only on `cfg` and `t` (no RNG
/// state, clock, or threads). Particles are returned in ascending birth order.
/// A particle is live iff it has been born (`birth ≤ t`) and not yet died
/// (`t - birth < life`). Each particle's cone angle, speed and lifetime are
/// drawn — in a fixed order — from a [`SplitMix64`] seeded by `(seed, index)`,
/// then its trajectory is integrated from birth to `t`.
pub fn simulate_particles(cfg: &EmitterConfig, t: f32) -> Vec<Particle> {
    let mut out = Vec::new();
    if !t.is_finite() || t < 0.0 {
        return out;
    }
    let rate = cfg.emission_rate;
    if rate <= 0.0 {
        return out;
    }
    let emit_window = cfg.emit_duration.max(0.0);
    // The latest birth time we need to consider: no later than the emit window,
    // and no later than the frame time itself (nothing born after `t` is live).
    let last_birth = emit_window.min(t);
    if last_birth < 0.0 {
        return out;
    }
    let max_index = ((last_birth * rate).floor() as i64).max(0) as u64;
    let max_index = max_index.min(MAX_PARTICLES);

    let dir = cfg.direction.to_radians();
    let half_spread = cfg.spread.to_radians() * 0.5;

    for i in 0..=max_index {
        let birth = i as f32 / rate;
        // Particles are only emitted strictly within the active window, and
        // never after the current frame time.
        if birth >= emit_window || birth > t {
            break;
        }
        // Per-particle deterministic randomness. Draw in a FIXED order so the
        // stream is stable no matter which fields the caller actually varies.
        let mut rng = SplitMix64::seeded(cfg.seed, i);
        let angle = dir + rng.next_signed() * half_spread;
        let speed = (cfg.speed + rng.next_signed() * cfg.speed_variance).max(0.0);
        let life = (cfg.lifetime + rng.next_signed() * cfg.lifetime_variance).max(1e-4);

        let age = t - birth;
        if age >= life {
            continue; // already dead at this frame
        }

        let vel0 = (angle.cos() * speed, angle.sin() * speed);
        let (position, velocity) = integrate(cfg, vel0, age);
        let u = (age / life).clamp(0.0, 1.0);
        out.push(Particle {
            index: i,
            position,
            velocity,
            age,
            life,
            size: size_over_life(cfg, u),
            opacity: opacity_over_life(cfg, u),
            color: color_over_life(cfg, u),
        });
    }
    out
}

/// Clamp/store a single scalar emitter parameter by name. Shared by the
/// `SetParam` action and tests.
fn set_param(cfg: &mut EmitterConfig, param: &str, value: f32) {
    match param {
        "emission_rate" => cfg.emission_rate = value.max(0.0),
        "emit_duration" => cfg.emit_duration = value.max(0.0),
        "direction" => cfg.direction = value,
        "spread" => cfg.spread = value.clamp(0.0, 360.0),
        "speed" => cfg.speed = value,
        "speed_variance" => cfg.speed_variance = value.max(0.0),
        "lifetime" => cfg.lifetime = value.max(1e-4),
        "lifetime_variance" => cfg.lifetime_variance = value.max(0.0),
        "drag" => cfg.drag = value.max(0.0),
        "turbulence" => cfg.turbulence = value,
        "turbulence_scale" => cfg.turbulence_scale = value,
        "size_start" => cfg.size_start = value.max(0.0),
        "size_end" => cfg.size_end = value.max(0.0),
        "opacity" => cfg.opacity = value.clamp(0.0, 1.0),
        "fade_in" => cfg.fade_in = value.clamp(0.0, 1.0),
        "fade_out" => cfg.fade_out = value.clamp(0.0, 1.0),
        _ => {}
    }
}

// ── Action layer ─────────────────────────────────────────────────────────────

/// A particle-emitter sub-action. Defined here (the particle *domain file*) and
/// wrapped by the single [`Action::Particles`](super::Action::Particles) variant
/// so the central `Action` enum stays small. All variants are app-side state
/// edits (they configure the per-layer emitter map, not the `Project`), so —
/// like Fractal Noise — they are **not** undoable.
#[derive(Clone, Debug)]
pub enum ParticleAction {
    /// Add a default emitter to the layer (no-op if one already exists).
    Add { layer_id: usize },
    /// Remove the layer's emitter.
    Remove { layer_id: usize },
    /// Set a scalar param by name (see [`set_param`]).
    SetParam {
        layer_id: usize,
        param: &'static str,
        value: f32,
    },
    /// Set the deterministic master seed.
    SetSeed { layer_id: usize, seed: u64 },
    /// Set the emitter origin (comp px).
    SetPosition { layer_id: usize, x: f32, y: f32 },
    /// Set the gravity vector (px/s²).
    SetGravity { layer_id: usize, gx: f32, gy: f32 },
    /// Set the start/end colors (RGBA, lerped over life).
    SetColors {
        layer_id: usize,
        start: [f32; 4],
        end: [f32; 4],
    },
}

impl App {
    /// Apply a particle [`Action`] (dispatched from [`App::apply`] via the
    /// [`Action::Particles`](super::Action::Particles) wrapper).
    pub(super) fn apply_particles(&mut self, action: Action) {
        let Action::Particles(pa) = action else {
            unreachable!("apply_particles called with wrong action");
        };
        match pa {
            ParticleAction::Add { layer_id } => {
                self.particle_emitters
                    .entry(layer_id)
                    .or_insert_with(EmitterConfig::default);
                self.host.mark_dirty();
            }
            ParticleAction::Remove { layer_id } => {
                self.particle_emitters.remove(&layer_id);
                self.host.mark_dirty();
            }
            ParticleAction::SetParam { layer_id, param, value } => {
                set_param(self.particle_emitters.entry(layer_id).or_default(), param, value);
                self.host.mark_dirty();
            }
            ParticleAction::SetSeed { layer_id, seed } => {
                self.particle_emitters.entry(layer_id).or_default().seed = seed;
                self.host.mark_dirty();
            }
            ParticleAction::SetPosition { layer_id, x, y } => {
                self.particle_emitters.entry(layer_id).or_default().position = (x, y);
                self.host.mark_dirty();
            }
            ParticleAction::SetGravity { layer_id, gx, gy } => {
                self.particle_emitters.entry(layer_id).or_default().gravity = (gx, gy);
                self.host.mark_dirty();
            }
            ParticleAction::SetColors { layer_id, start, end } => {
                let cfg = self.particle_emitters.entry(layer_id).or_default();
                cfg.color_start = start;
                cfg.color_end = end;
                self.host.mark_dirty();
            }
        }
    }

    /// Simulate the emitter on `layer_id` at time `t`, or `None` if the layer has
    /// no emitter. The compositor/preview calls this to draw the layer's
    /// particles. Deterministic — see [`simulate_particles`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn simulate_layer_particles(&self, layer_id: usize, t: f32) -> Option<Vec<Particle>> {
        self.particle_emitters
            .get(&layer_id)
            .map(|cfg| simulate_particles(cfg, t))
    }
}

/// Per-layer emitter storage (app-side; keyed by layer index). Mirrors
/// `FractalNoiseMap` in `effects_noise.rs`.
pub type ParticleEmitterMap = HashMap<usize, EmitterConfig>;

#[cfg(test)]
mod tests {
    use super::*;

    /// A force-free config so a particle's velocity stays at its emission value
    /// (handy for isolating emission/jitter behavior from integration).
    fn forceless() -> EmitterConfig {
        EmitterConfig {
            emission_rate: 10.0,
            emit_duration: 100.0,
            spread: 0.0,
            speed: 100.0,
            speed_variance: 0.0,
            lifetime: 100.0,
            lifetime_variance: 0.0,
            gravity: (0.0, 0.0),
            drag: 0.0,
            turbulence: 0.0,
            direction: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            ..EmitterConfig::default()
        }
    }

    fn mag(v: (f32, f32)) -> f32 {
        (v.0 * v.0 + v.1 * v.1).sqrt()
    }

    // ── Determinism ──────────────────────────────────────────────────────────

    #[test]
    fn deterministic_same_seed_same_t() {
        let cfg = EmitterConfig { seed: 1234, ..EmitterConfig::default() };
        let a = simulate_particles(&cfg, 1.5);
        let b = simulate_particles(&cfg, 1.5);
        assert_eq!(a, b, "same config + t ⇒ identical particle set");
        assert!(!a.is_empty(), "sanity: emitter produced particles");
    }

    #[test]
    fn deterministic_with_all_forces() {
        // Turbulence + gravity + drag + variance: still bit-reproducible.
        let cfg = EmitterConfig {
            seed: 77,
            turbulence: 300.0,
            turbulence_scale: 0.02,
            gravity: (10.0, 200.0),
            drag: 0.5,
            spread: 60.0,
            speed_variance: 80.0,
            lifetime_variance: 0.4,
            ..EmitterConfig::default()
        };
        assert_eq!(simulate_particles(&cfg, 2.0), simulate_particles(&cfg, 2.0));
    }

    #[test]
    fn different_seed_differs() {
        let a = simulate_particles(&EmitterConfig { seed: 1, spread: 45.0, ..EmitterConfig::default() }, 1.0);
        let b = simulate_particles(&EmitterConfig { seed: 2, spread: 45.0, ..EmitterConfig::default() }, 1.0);
        assert_ne!(a, b, "different seeds ⇒ different particles");
    }

    // ── Emission ───────────────────────────────────────────────────────────────

    #[test]
    fn emission_count_grows_with_rate() {
        let base = EmitterConfig { emit_duration: 100.0, lifetime: 100.0, lifetime_variance: 0.0, ..EmitterConfig::default() };
        let slow = simulate_particles(&EmitterConfig { emission_rate: 10.0, ..base }, 1.0);
        let fast = simulate_particles(&EmitterConfig { emission_rate: 40.0, ..base }, 1.0);
        assert!(fast.len() > slow.len(), "higher rate ⇒ more live particles ({} vs {})", fast.len(), slow.len());
    }

    #[test]
    fn emission_count_matches_rate_times_time() {
        let cfg = EmitterConfig {
            emission_rate: 20.0,
            emit_duration: 100.0,
            lifetime: 100.0,
            lifetime_variance: 0.0,
            ..EmitterConfig::default()
        };
        // Born at 0, 0.05, 0.10, ... ≤ 0.5  ⇒ 11 particles (i = 0..=10).
        let p = simulate_particles(&cfg, 0.5);
        assert_eq!(p.len(), 11);
    }

    #[test]
    fn zero_rate_emits_nothing() {
        let cfg = EmitterConfig { emission_rate: 0.0, ..EmitterConfig::default() };
        assert!(simulate_particles(&cfg, 5.0).is_empty());
    }

    #[test]
    fn negative_time_emits_nothing() {
        assert!(simulate_particles(&EmitterConfig::default(), -1.0).is_empty());
    }

    #[test]
    fn particles_born_in_ascending_index_order() {
        let p = simulate_particles(&EmitterConfig { emit_duration: 100.0, lifetime: 100.0, ..EmitterConfig::default() }, 1.0);
        assert!(p.windows(2).all(|w| w[0].index < w[1].index), "indices strictly ascending");
        assert_eq!(p[0].index, 0, "first live particle is index 0");
    }

    #[test]
    fn emission_stops_after_window() {
        // Window of 1s @ 10/s ⇒ exactly 10 particles ever emitted (i = 0..=9).
        let cfg = EmitterConfig {
            emission_rate: 10.0,
            emit_duration: 1.0,
            lifetime: 100.0,
            lifetime_variance: 0.0,
            ..EmitterConfig::default()
        };
        let during = simulate_particles(&cfg, 1.0);
        let after = simulate_particles(&cfg, 2.0); // emitter idle, all still alive
        assert_eq!(during.len(), 10, "rate × window particles emitted");
        assert_eq!(after.len(), 10, "no new births after the window closes");
    }

    // ── Lifetime / death ─────────────────────────────────────────────────────

    #[test]
    fn particles_die_after_lifetime() {
        // Emit for 1s, each lives 1s ⇒ all dead well before t = 10.
        let cfg = EmitterConfig {
            emission_rate: 10.0,
            emit_duration: 1.0,
            lifetime: 1.0,
            lifetime_variance: 0.0,
            ..EmitterConfig::default()
        };
        assert!(!simulate_particles(&cfg, 0.5).is_empty(), "alive mid-emission");
        assert!(simulate_particles(&cfg, 10.0).is_empty(), "all dead long after lifetime");
    }

    #[test]
    fn age_never_exceeds_life() {
        let cfg = EmitterConfig { lifetime: 2.0, lifetime_variance: 0.5, ..EmitterConfig::default() };
        for p in simulate_particles(&cfg, 1.7) {
            assert!(p.age < p.life, "live particle: age {} < life {}", p.age, p.life);
            assert!(p.age >= 0.0);
        }
    }

    // ── Physics: gravity ───────────────────────────────────────────────────────

    #[test]
    fn gravity_pulls_particles_down() {
        // Emit horizontally (+x) so any downward motion is purely gravity.
        let cfg = EmitterConfig {
            emission_rate: 1.0,
            emit_duration: 100.0,
            direction: 0.0,
            spread: 0.0,
            speed: 50.0,
            speed_variance: 0.0,
            gravity: (0.0, 200.0),
            drag: 0.0,
            turbulence: 0.0,
            lifetime: 100.0,
            lifetime_variance: 0.0,
            ..EmitterConfig::default()
        };
        // Particle 0 (born at t=0) at two later times.
        let early = simulate_particles(&cfg, 0.2)[0];
        let late = simulate_particles(&cfg, 0.8)[0];
        assert!(late.position.1 > early.position.1, "y increases (falls) over time");
        assert!(early.position.1 > 0.0, "already below the origin");
        assert!(late.velocity.1 > early.velocity.1, "downward velocity accelerates");
        assert!(late.velocity.1 > 0.0, "downward velocity is positive");
    }

    #[test]
    fn no_gravity_no_vertical_drift() {
        let cfg = forceless(); // direction +x, no gravity/drag/turbulence
        let p = simulate_particles(&cfg, 1.0)[0];
        assert!(p.position.1.abs() < 1e-3, "no vertical motion without gravity");
        assert!(p.position.0 > 0.0, "moves along +x at constant speed");
    }

    // ── Physics: drag ──────────────────────────────────────────────────────────

    #[test]
    fn drag_slows_particles() {
        let base = EmitterConfig {
            emission_rate: 1.0,
            emit_duration: 100.0,
            direction: 0.0,
            spread: 0.0,
            speed: 300.0,
            speed_variance: 0.0,
            gravity: (0.0, 0.0),
            turbulence: 0.0,
            lifetime: 100.0,
            lifetime_variance: 0.0,
            ..EmitterConfig::default()
        };
        let free = simulate_particles(&EmitterConfig { drag: 0.0, ..base }, 1.0)[0];
        let dragged = simulate_particles(&EmitterConfig { drag: 1.5, ..base }, 1.0)[0];
        assert!(dragged.position.0 < free.position.0, "drag ⇒ less distance travelled");
        assert!(mag(dragged.velocity) < mag(free.velocity), "drag ⇒ lower speed");
    }

    // ── Physics: turbulence ──────────────────────────────────────────────────

    #[test]
    fn turbulence_perturbs_trajectory() {
        let base = forceless();
        let calm = simulate_particles(&base, 1.0)[0];
        let stormy = simulate_particles(&EmitterConfig { turbulence: 400.0, turbulence_scale: 0.05, ..base }, 1.0)[0];
        assert_ne!(calm.position, stormy.position, "turbulence bends the path");
    }

    // ── Physics: emission cone / variance ────────────────────────────────────

    #[test]
    fn zero_spread_uniform_direction() {
        let p = simulate_particles(&forceless(), 1.0);
        let v0 = p[0].velocity;
        assert!(p.iter().all(|q| (q.velocity.0 - v0.0).abs() < 1e-3 && (q.velocity.1 - v0.1).abs() < 1e-3),
            "no spread / variance ⇒ identical velocities");
    }

    #[test]
    fn spread_creates_angular_variation() {
        let cfg = EmitterConfig { spread: 90.0, ..forceless() };
        let p = simulate_particles(&cfg, 1.0);
        let v0 = p[0].velocity;
        assert!(p.iter().any(|q| (q.velocity.1 - v0.1).abs() > 1.0),
            "spread fans particles into different directions");
    }

    #[test]
    fn speed_variance_creates_speed_variation() {
        let cfg = EmitterConfig { speed_variance: 60.0, ..forceless() };
        let p = simulate_particles(&cfg, 1.0);
        let m0 = mag(p[0].velocity);
        assert!(p.iter().any(|q| (mag(q.velocity) - m0).abs() > 1.0),
            "speed variance ⇒ particles at different speeds");
    }

    // ── Size / opacity / color over life ─────────────────────────────────────

    #[test]
    fn size_ramps_over_life() {
        let cfg = EmitterConfig { size_start: 10.0, size_end: 2.0, ..EmitterConfig::default() };
        assert!((size_over_life(&cfg, 0.0) - 10.0).abs() < 1e-4);
        assert!((size_over_life(&cfg, 1.0) - 2.0).abs() < 1e-4);
        assert!(size_over_life(&cfg, 0.5) < size_over_life(&cfg, 0.0), "shrinks over life");
    }

    #[test]
    fn opacity_fades_in_and_out() {
        let cfg = EmitterConfig { opacity: 1.0, fade_in: 0.2, fade_out: 0.3, ..EmitterConfig::default() };
        assert!(opacity_over_life(&cfg, 0.0) < 0.01, "starts transparent (fade in)");
        assert!((opacity_over_life(&cfg, 0.5) - 1.0).abs() < 1e-4, "full in the middle");
        assert!(opacity_over_life(&cfg, 1.0) < 0.01, "ends transparent (fade out)");
    }

    #[test]
    fn opacity_fades_to_zero_at_end_in_sim() {
        // A particle sampled near the end of its life is nearly transparent.
        let cfg = EmitterConfig {
            emission_rate: 1.0,
            emit_duration: 100.0,
            lifetime: 2.0,
            lifetime_variance: 0.0,
            fade_in: 0.0,
            fade_out: 0.3,
            opacity: 1.0,
            ..EmitterConfig::default()
        };
        // Particle 0 born at t=0; sample at t=1.98 ⇒ u = 0.99.
        let p = simulate_particles(&cfg, 1.98)[0];
        assert!(p.opacity < 0.1, "opacity ≈ 0 at end of life, got {}", p.opacity);
    }

    #[test]
    fn color_lerps_start_to_end() {
        let cfg = EmitterConfig {
            color_start: [1.0, 0.0, 0.0, 1.0],
            color_end: [0.0, 0.0, 1.0, 0.0],
            ..EmitterConfig::default()
        };
        assert_eq!(color_over_life(&cfg, 0.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(color_over_life(&cfg, 1.0), [0.0, 0.0, 1.0, 0.0]);
        let mid = color_over_life(&cfg, 0.5);
        assert!((mid[0] - 0.5).abs() < 1e-4 && (mid[2] - 0.5).abs() < 1e-4 && (mid[3] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn color_lerps_in_sim() {
        let cfg = EmitterConfig {
            emission_rate: 1.0,
            emit_duration: 100.0,
            lifetime: 2.0,
            lifetime_variance: 0.0,
            color_start: [1.0, 0.0, 0.0, 1.0],
            color_end: [0.0, 1.0, 0.0, 1.0],
            ..EmitterConfig::default()
        };
        // Particle 0 at t=1.0 ⇒ u = 0.5 ⇒ midpoint color.
        let p = simulate_particles(&cfg, 1.0)[0];
        assert!((p.color[0] - 0.5).abs() < 1e-4 && (p.color[1] - 0.5).abs() < 1e-4,
            "particle color is the midpoint, got {:?}", p.color);
    }

    // ── App action layer ─────────────────────────────────────────────────────

    #[test]
    fn add_configure_remove_emitter() {
        let mut app = App::new();
        assert!(app.simulate_layer_particles(0, 1.0).is_none(), "no emitter yet");
        app.apply(Action::Particles(ParticleAction::Add { layer_id: 0 }));
        assert!(app.particle_emitters.contains_key(&0));
        assert!(app.simulate_layer_particles(0, 1.0).is_some());

        app.apply(Action::Particles(ParticleAction::SetSeed { layer_id: 0, seed: 99 }));
        app.apply(Action::Particles(ParticleAction::SetPosition { layer_id: 0, x: 50.0, y: -25.0 }));
        app.apply(Action::Particles(ParticleAction::SetGravity { layer_id: 0, gx: 5.0, gy: 300.0 }));
        let cfg = app.particle_emitters[&0];
        assert_eq!(cfg.seed, 99);
        assert_eq!(cfg.position, (50.0, -25.0));
        assert_eq!(cfg.gravity, (5.0, 300.0));

        app.apply(Action::Particles(ParticleAction::Remove { layer_id: 0 }));
        assert!(!app.particle_emitters.contains_key(&0));
    }

    #[test]
    fn set_param_clamps_values() {
        let mut app = App::new();
        app.apply(Action::Particles(ParticleAction::Add { layer_id: 1 }));
        app.apply(Action::Particles(ParticleAction::SetParam { layer_id: 1, param: "emission_rate", value: -5.0 }));
        app.apply(Action::Particles(ParticleAction::SetParam { layer_id: 1, param: "opacity", value: 4.0 }));
        app.apply(Action::Particles(ParticleAction::SetParam { layer_id: 1, param: "lifetime", value: -2.0 }));
        app.apply(Action::Particles(ParticleAction::SetParam { layer_id: 1, param: "spread", value: 999.0 }));
        let cfg = app.particle_emitters[&1];
        assert_eq!(cfg.emission_rate, 0.0);
        assert!((cfg.opacity - 1.0).abs() < 1e-6);
        assert!(cfg.lifetime > 0.0);
        assert!((cfg.spread - 360.0).abs() < 1e-6);
    }

    #[test]
    fn set_colors_action() {
        let mut app = App::new();
        app.apply(Action::Particles(ParticleAction::Add { layer_id: 2 }));
        app.apply(Action::Particles(ParticleAction::SetColors {
            layer_id: 2,
            start: [0.1, 0.2, 0.3, 1.0],
            end: [0.4, 0.5, 0.6, 0.0],
        }));
        let cfg = app.particle_emitters[&2];
        assert_eq!(cfg.color_start, [0.1, 0.2, 0.3, 1.0]);
        assert_eq!(cfg.color_end, [0.4, 0.5, 0.6, 0.0]);
    }

    #[test]
    fn emitter_actions_are_not_undoable() {
        // App-side state (like Fractal Noise) ⇒ no project snapshot taken.
        let mut app = App::new();
        assert!(!app.can_undo());
        app.apply(Action::Particles(ParticleAction::Add { layer_id: 0 }));
        assert!(!app.can_undo(), "particle edits don't push undo history");
    }

    #[test]
    fn set_param_changes_simulation() {
        let mut app = App::new();
        app.apply(Action::Particles(ParticleAction::Add { layer_id: 0 }));
        app.apply(Action::Particles(ParticleAction::SetParam { layer_id: 0, param: "emission_rate", value: 5.0 }));
        let few = app.simulate_layer_particles(0, 1.0).unwrap().len();
        app.apply(Action::Particles(ParticleAction::SetParam { layer_id: 0, param: "emission_rate", value: 50.0 }));
        let many = app.simulate_layer_particles(0, 1.0).unwrap().len();
        assert!(many > few, "raising the rate yields more particles");
    }
}
