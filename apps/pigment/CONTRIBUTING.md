# Contributing to Pigment

Thanks for your interest in contributing to Pigment — the open-source Photoshop
analog in the Prism Suite.

## Dev setup

1. **Rust stable** (1.88+). Install from [rustup.rs](https://rustup.rs/).
2. **macOS:** install the Metal Toolchain if prompted:
   ```bash
   xcodebuild -downloadComponent MetalToolchain
   ```
3. Clone this repo and the shared engine repo side by side:
   ```bash
   git clone https://github.com/KwaminaWhyte/prism-suite
   git clone https://github.com/KwaminaWhyte/prism-suite
   ```
   The shared crates (`prism-canvas`, `prism-core`, `prism-io`, etc.) are
   path-dep'd from `../shared/`.

## Running

```bash
cargo run
```

## Testing

```bash
cargo test --workspace
```

Headless GPU tests (those calling `skip_if_no_adapter()`) are skipped
automatically in CI environments without a GPU adapter.

## PR checklist

Before opening a pull request:

- [ ] `cargo build` passes with no errors.
- [ ] `cargo test --workspace` — all tests pass.
- [ ] `cargo clippy --workspace -- -D warnings` — no new warnings.
- [ ] `cargo fmt --check` — code is formatted.
- [ ] **CHANGELOG.md** updated: add an entry under `## [Unreleased]` describing
  what you added, changed, or fixed.
  `[ ]` to `[x]` or `[-]` as appropriate.
- [ ] Branch off `main` and PR back into `main`.

## Where does new code belong?

- **Pigment-specific raster logic** (tools, filters, UI panels, CanvasHost): in
  `crates/pigment-gpui/src/`.
- **Engine-level GPU logic** (compositor passes, brush engine, shader math): in
  `shared/prism-canvas/`. Keep it app-agnostic — Contour and
  Pulse share this crate.
- **Shared domain types** (document model, color, blend math, IO): in
  `shared/prism-core/` or `prism-io/`. Must not assume Pigment.
- **New filters**: add a GPU shader kind in `prism-canvas/src/shaders/filter.wgsl`,
  a CPU reference in `canvas::filter_math` (for tests), and a new `Action`/menu
  entry in `pigment-gpui`.

## Commit style

Conventional Commits (subject ≤50 chars):
- `feat: add tilt-shift blur filter`
- `fix: histogram channel mode off-by-one`

Do not include `Co-Authored-By:` lines.
