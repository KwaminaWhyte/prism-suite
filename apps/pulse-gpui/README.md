<p align="center">
  <img src="assets/branding/pulse-master.png" width="120" alt="Pulse">
</p>

# Pulse

Motion-graphics and compositing app — the After Effects analog and **app #3 of the
Prism creative suite** (sibling to [Pigment](https://github.com/KwaminaWhyte/prism-suite), the raster editor, and
[Contour](https://github.com/KwaminaWhyte/prism-suite), the vector editor).

Built in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (Zed's framework). The composition engine runs as a
CPU software compositor; the GPUI host bridges rendered frames to the GPU for display.
State serializes with `serde` (`.pulse` JSON project files).

> `pulse-app` also has an `egui-ui` feature (disabled by default). The default build
> produces only the `pulse-gpui` binary. The egui binary is a legacy build path and
> is not the active UI.

## Status — v0 (≥85% After Effects parity)

Pulse is feature-complete for most real-world After Effects workflows. The compositor
runs fully on the CPU; a GPU compositor path is planned but not yet wired.

**Core capabilities**

- **Comp model** — `Comp` (width, height, duration, fps) holding an ordered stack of
  `PulseLayer`s. Each layer carries animatable transform tracks (X, Y, Z, scale,
  rotation, orientation, opacity, anchor), a 3D toggle, parenting pick-whip, blend
  mode, masks, track matte, and six effect stacks.
- **Keyframe interpolation** — linear, hold, and Bézier ease (Easy Ease / Ease In /
  Ease Out, Newton-solved `cubic-bezier`); roving keyframes for constant-velocity
  spatial paths.
- **Graph editor** — value-curve editor with draggable keyframes and Bézier ease
  handles, per-property show/hide chips.
- **Effects** — color correction (Levels, Curves, Hue/Sat, Channel Mixer, etc.),
  Blur & Sharpen (Gaussian, Box, Directional, Radial), Generate (Fractal Noise, Cell
  Pattern, Ramp, Checkerboard, 4-Color Gradient, Grid), Distort (Corner Pin,
  Transform, Mirror, Polar), Keying (Chroma Key, Color Key, Luma Key, Spill
  Suppression, Matte Choke), Stylize (Find Edges, Mosaic, Stroke, Glow), Drop Shadow.
- **Expressions** — per-property `rhai` engine; `time`, `value`, `wiggle`, `linear`,
  `clamp`, and standard math helpers.
- **3D layers and camera** — per-layer Z / orientation, perspective projection,
  painter's z-sort, depth of field, Ambient / Point / Spot / Parallel lights.
- **Export** — PNG sequence, H.264 / H.265 MP4, ProRes (FFmpeg), animated GIF, render
  queue with format badges.
- **Layer types** — Solid, Text (built-in stroke font + real TrueType families),
  Shape (rect/ellipse/polygon/star), Footage (stills + image sequences + video via
  `prism-media`), Precomp (nested comps, recursive render, cycle guard), Adjustment,
  Null, Guide, Expression Controls.
- **Undo/redo**, RAM-preview cache, work area loop, markers, animation presets,
  time remapping, frame blending, motion blur, masks, track mattes.

## Shared foundation

Pulse depends on the suite's shared crate **`prism-core`** (path dep
`../shared/prism-core`) plus **`prism-io`** (stills / image
sequences) and **`prism-media`** (FFmpeg A/V). These crates are time-agnostic — time
(keyframes, expressions) is Pulse's layer on top.

## Build and run

```sh
# from prism-suite-pulse/
cargo run                        # launches the Pulse window (pulse-gpui binary)
cargo build                      # debug build
cargo build -p pulse-gpui        # build only the GPUI host
cargo test --workspace           # all unit tests
cargo fmt                        # formatting
cargo clippy                     # lints

# Egui binary (disabled by default — not the active UI):
cargo run -p pulse-app --features egui-ui
```

Binary name: `pulse-gpui` (crate `pulse-gpui`).

## Workspace layout

```
prism-suite-pulse/
├── Cargo.toml                      # workspace + shared lint config + path deps
├── CHANGELOG.md
├── PLAN.md                         # parity roadmap + phased backlog
├── RESEARCH.md                     # cited findings + crate matrix
├── VERSIONING.md
└── crates/
    ├── pulse-app/                  # comp model + CPU compositor (library)
    │   └── src/
    │       ├── lib.rs              # pub re-exports for pulse-gpui
    │       ├── main.rs             # egui entry point (egui-ui feature, not default)
    │       ├── comp/               # Keyframe/Track/PulseLayer/Comp + sampling
    │       ├── render/             # CPU compositor, export, render queue
    │       ├── gizmo.rs            # transform gizmo math (pure)
    │       └── graph.rs            # value-curve graph math (pure)
    └── pulse-gpui/                 # GPUI host (the active binary)
        └── src/
            ├── main.rs             # GPUI Application entry point
            ├── app_state.rs        # App + Action choke point + undo stack
            ├── canvas_host.rs      # CPU-compositor → GPUI RenderImage bridge
            └── panels/             # layers, properties, timeline, effects, …
```

## Contributing

Fork the repo, create a branch off `main`, and open a pull request to `main`.

The shared engine lives in `prism-suite-prism` (the `prism-core`, `prism-io`, and
`prism-media` crates). Keep additions to those crates time-agnostic — time is Pulse's
layer on top, not the shared crates' concern.

See [CONTRIBUTING.md](./CONTRIBUTING.md) for dev setup, test checklist, and PR
guidelines.
