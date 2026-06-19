# Contributing to Pulse

Pulse is the After Effects analog in the Prism creative suite. Contributions are
welcome — please read this guide before opening a pull request.

## Prerequisites

- **Rust** (stable toolchain, 1.95.0 or later). Install via [rustup](https://rustup.rs/).
- **FFmpeg** (optional). Required only to test ProRes / H.265 export and video
  footage import. If absent, those paths degrade gracefully and all other tests pass.
- **macOS** is the primary development platform (GPUI uses Metal). Linux (Vulkan) is
  supported by GPUI but not actively tested here.

## Dev setup

```sh
# Clone the suite — Pulse depends on sibling crates by path.
git clone https://github.com/KwaminaWhyte/prism-suite   # prism-core, prism-io, prism-media, prism-ui
git clone https://github.com/KwaminaWhyte/prism-suite

cd prism-suite-pulse
cargo build                   # builds pulse-gpui (the active binary)
cargo run                     # launches the Pulse window
```

The sibling `prism-suite-prism` repo must be checked out at `../prism-suite-prism`
relative to this repo (the default when cloning side-by-side). The path deps in
`Cargo.toml` resolve from there.

## Running tests

```sh
cargo test --workspace        # all unit tests (engine + render path)
cargo clippy --workspace      # lints (must pass clean)
cargo fmt --check             # formatting check
```

Tests are pure and headless — no GPU, no display, no FFmpeg required for the core
suite. The render-path integration tests spin up the CPU compositor directly.

## Project structure

The workspace has two crates:

- **`crates/pulse-app`** — the composition engine (library). All comp model types
  (`Comp`, `PulseLayer`, `Track`, `Keyframe`), the CPU software compositor
  (`render/`), effect stacks, export logic, and pure math modules (`gizmo`, `graph`,
  `light`). The `egui-ui` feature gates the legacy egui binary (`src/main.rs`); it
  is **disabled by default** and not part of normal development.
- **`crates/pulse-gpui`** — the GPUI host (the active binary). Owns `App`,
  `Action`, the undo stack, `CanvasHost` (CPU compositor → GPUI image bridge), and
  all panels. Depends on `pulse-app` with `default-features = false`.

The shared engine crates (`prism-core`, `prism-io`, `prism-media`, `prism-ui`) live
in `prism-suite-prism` and are time-agnostic. Keep additions to those crates free of
any Pulse-specific concepts — time (keyframes, expressions) is Pulse's layer on top.

## Making changes

1. **Fork** the repo on GitHub.
2. Create a branch off `main`: `git checkout -b my-feature`.
3. Make your changes. Keep additions additive — use `#[serde(default)]` on new
   fields so existing `.pulse` project files load unchanged.
4. Run the full test + lint suite (see above). All tests must pass.
5. Open a pull request targeting `main`.

## PR checklist

- [ ] `cargo test --workspace` passes.
- [ ] `cargo clippy --workspace` is clean (no new warnings).
- [ ] `cargo fmt --check` passes.
- [ ] `cargo build -p pulse-gpui` succeeds.
- [ ] New `.pulse` project fields have `#[serde(default)]` (back-compat).
- [ ] New engine logic is covered by at least one unit test.
- [ ] CHANGELOG.md `## [Unreleased]` section updated with a summary.

## What goes where

- **Pulse-owned** — timeline, keyframes, expressions, layer types, masks, effects
  UI, render queue, and the GPUI panels. Lives in `pulse-app` or `pulse-gpui`.
- **Shared crate** — compositor math, blend modes, color transforms, effect passes,
  media decode/encode. These belong in `prism-suite-prism` crates and must stay
  time-agnostic.
- **Out of scope** — clip-based NLE editing (that is Reel), primary raster painting
  (Pigment), or deep vector authoring (Contour).

## Questions

Open an issue or discussion on GitHub. The [PLAN.md](./PLAN.md) parity roadmap and
[RESEARCH.md](./RESEARCH.md) crate matrix are good background reading.
