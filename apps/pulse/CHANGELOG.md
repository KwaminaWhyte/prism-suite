# Changelog

All notable changes to **Pulse** (the Prism suite's motion/compositing app) are
documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project is pre-1.0, so versions are `0.x` milestones and track the workspace
`version` in the root `Cargo.toml`. History before 0.9.0 lives in the git log.

## [Unreleased]

## [0.13.0] - 2026-06-28

### Added — Particle system (CC Particle World / Particle Playground analog)
- **Deterministic 2D particle simulator** (`app_state/particles.rs`) — `simulate_particles(&EmitterConfig, t)` returns every live particle (position/velocity/age/size/opacity/color) at a frame. Seeded, reproducible: each particle's birth + randomness (cone angle / speed / lifetime) comes from a **SplitMix64** keyed by `(emitter_seed, particle_index)` — no global RNG, no `Instant`. Real physics integrated from birth to `t` with a fixed sub-step: **gravity**, **air-resistance drag**, and optional **value-noise turbulence** (reuses `effects_noise::fractal_noise`).
- **Per-particle envelopes** — size-over-life, opacity fade-in/out, and start→end color lerp.
- **App action layer** — `Action::Particles(ParticleAction::{Add,Remove,SetParam,SetSeed,SetPosition,SetGravity,SetColors})` manage a per-layer emitter map (app-side, like Fractal Noise — not undoable); `App::simulate_layer_particles(layer, t)`.
- +28 tests, 1047 → 1075 (determinism, gravity, drag, turbulence, cone/variance, lifetime death, emission-vs-rate, fade-out, color lerp). (`actions.rs` kept under 1000 via a nested `ParticleAction` enum + minor variant compaction.)

## [0.12.0] - 2026-06-24

### Added — Multi-line + comprehensive text input
- **Expression editor → multi-line `prism_ui::TextArea`** — type AE-style multi-line rhai expressions (Cmd+Enter → `SetExpression` + `EvaluateExpression`); chips still quick-insert.
- **Effect-browser search box** — live `SetEffectQuery` filtering as you type.
- **Typeable numerics** — comp settings (W/H/FPS/Duration) + layer transform (X/Y/scale/rotation/opacity); also fixed comp-settings seeding so the panel shows before first Apply.
- +13 tests (1034 → 1047). (`mod.rs` kept under 1000 via `geom_util.rs` extraction.)

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Typeable expression editor** — type ANY rhai expression (no longer preset-chip-only); Enter / Evaluate → `SetExpression` + `EvaluateExpression`, with result/error readout. Chips kept as quick-insert.
- **Layer + comp rename** — `RenameLayer` / `SetCompName` via inline fields.
- **Render output path** — typeable path → `SetRenderOutputPath`.
- +7 tests (1027 → 1034).

## [0.10.0] - 2026-06-24

### Added — UI (GPUI panels)
- **Expression editor** — property selector + preset-expression chips →
  `SetExpression`, "Evaluate" → `EvaluateExpression`, shows result/errors.
- **Output Module dialog** → `AddOutputModule` / `SetOutputModule*`, lists modules.
- **Preferences** panel → `SetPref*` + disk-cache dir/purge controls with a live
  usage bar.
- **Keying + Lights** inspector — per-layer keyers + comp 3D lights.
- UI-only (emits existing 0.9.0 actions, no new state); 8 helper/round-trip tests.

## [0.9.0] - 2026-06-24

### Added — Feature waves 1–3 (parity push)
- **After-Effects expression API** — deterministic Rust implementations of
  `loopOut/loopIn/loopInOut/loopOutDuration` (cycle/pingpong/offset/continue),
  `valueAtTime`, `velocityAtTime` (segment-slope), `wiggle` (SplitMix64-seeded, no
  global RNG), `linear`/`ease`/`easeIn`/`easeOut`/`clamp`, `posterizeTime`;
  `EvaluateExpression` runs the real string through the rhai engine
  (`app_state/expressions.rs`).
- **Output modules** — render-queue output config: format (MP4/MOV/PNGseq/EXRseq),
  codec, color depth, scale/resolution, frame range, audio (`output_module.rs`).
- **Preferences** — general/display/media/previews with serde JSON load/save
  (`preferences.rs`).
- **Disk-cache manager** — cache dir, max size, usage tracking, purge, LRU
  eviction (`cache_manager.rs`).
- **Rotobrush (ML-free)** — luma+chroma threshold segmentation + BFS flood-fill
  mask propagation across frames (`tracking.rs`).
- **Audio mixer** — per-track gain(dB)/pan/solo/mute/bus routing, master bus,
  equal-power pan law, solo-aware mixdown (`audio_mixer.rs`).
- **Mask editor** — bezier masks with Add/Subtract/Intersect/Difference modes,
  feather, opacity, expansion, inversion; rasterize the stack to an alpha matte
  (`masks.rs`).
- **Fractal Noise + Turbulent Displace** — deterministic multi-octave value noise
  and a noise-field pixel displacer (`effects_noise.rs`).
- **Per-layer motion blur** — shutter-angle/phase/samples override, transform
  sampled and averaged across the shutter (`motion_blur.rs`).
- **Time-stretch / time-remap** — layer stretch factor composed with a remap curve
  (`time_stretch.rs`).
- **Keying suite** — chroma (brightness-independent chromaticity distance), color,
  and luma keyers + Keylight-style spill suppression (`keying.rs`).
- **Distortion effects** — Corner Pin (homography), Bezier Warp, Wave Warp,
  Roughen Edges (`effects_distort.rs`).
- **3D lights & materials** — point/spot/ambient lights + per-layer Blinn-Phong
  material options (`lighting3d.rs`).
- **Shape repeater + trim paths** — N-copy repeater with per-copy transform/opacity
  ramp; static + keyed trim-paths (`shape_repeater.rs`).
- **Render formats** — codec/fps/CRF/audio output spec with an ffmpeg-arg builder
  (`render_formats.rs`).

### Changed — File organization
- Split files over the ~1000-line limit: `composition.rs` → `composition.rs` +
  `composition_layers.rs`; `effects_chain.rs` → `effects_chain.rs` +
  `effects_apply.rs`; `mod.rs` → `mod.rs` + a new `dispatch.rs` action router;
  `actions.rs` → `actions.rs` + `actions_undoable.rs`.
