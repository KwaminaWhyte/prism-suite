# Contributing to Contour

Contour is the Illustrator-analog vector editor in the Prism suite. Contributions are welcome.

## Dev setup

**Requirements:**
- Rust stable (install via [rustup](https://rustup.rs/))
- macOS with the Metal toolchain (Xcode Command Line Tools)

**Clone and build:**

```sh
git clone https://github.com/KwaminaWhyte/prism-suite
cd prism-suite-contour
cargo build -p contour-gpui
```

**Run the editor:**

```sh
cargo run
```

**Tests:**

```sh
cargo test --workspace
```

**Lint and format:**

```sh
cargo clippy --workspace -- -D warnings
cargo fmt --all
```

## Making changes

1. Fork the repo on GitHub.
2. Create a feature branch from `main`:
   ```sh
   git checkout -b my-feature
   ```
3. Make your changes. Follow the existing code style.
4. Run `cargo test --workspace` and `cargo clippy --workspace` — both must pass.
5. Push your branch and open a PR against `main`.

## PR checklist

- [ ] `cargo build -p contour-gpui` passes with no errors
- [ ] `cargo test --workspace` passes (0 failures)
- [ ] `cargo clippy --workspace` passes with no new warnings
- [ ] New features have unit tests in the relevant module
- [ ] CHANGELOG.md `## [Unreleased]` section updated with a summary of the change

## Architecture notes

- **Engine/host split.** `contour-app` is the shared library (document model, tools, export,
  boolean ops). `contour-gpui` is the thin GPUI host that drives it. New features belong in
  `contour-app` unless they are strictly UI-layout concerns.
- **Single mutation point.** All state changes go through `App::apply(Action)` in `app_state.rs`.
  Add an `Action` variant + an `apply` arm; never mutate `App` directly from a panel.
- **Additive model changes.** Every new field on a document type must have `#[serde(default)]` so
  existing `.contour` files continue to load without errors.
- **Suite boundaries.** Do not add Pigment/Pulse-domain features here. Do not couple
  `contour-app` to any UI framework — it must remain headless-testable.

feature tracker.
