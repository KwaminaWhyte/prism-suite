# Contributing to prism-suite

Thank you for contributing to the Prism suite. This repo contains six apps and the shared crates they all depend on. Changes to shared crates affect every app, so the bar for quality is higher than for app-specific work.

## Setting up the dev environment

You need:

- **Rust stable** (latest stable; install via [rustup](https://rustup.rs/))
- **FFmpeg** in your `PATH` — required for `prism-media` tests (install via `brew install ffmpeg` on macOS)
- A macOS machine with a GPU adapter for `prism-canvas` GPU tests (tests skip silently without one)

Clone the repo and build:

```bash
git clone https://github.com/KwaminaWhyte/prism-suite.git
cd prism-suite
cargo build
```

## Running tests

```bash
cargo test --workspace
```

To test a single crate:

```bash
cargo test -p prism-core
cargo test -p prism-media
```

`prism-media` tests skip silently when FFmpeg is absent. `prism-canvas` GPU tests skip when no GPU adapter is present — both behaviors are intentional.

## Code style

Format with `rustfmt` before committing:

```bash
cargo fmt --all
```

Lint with Clippy:

```bash
cargo clippy --workspace
```

The workspace `Cargo.toml` allows a small set of Clippy lints (`needless_range_loop`, `too_many_arguments`, `field_reassign_with_default`) — do not add suppressions beyond those without discussion.

## PR checklist

Before opening a pull request:

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace` reports no new warnings
- [ ] `cargo fmt --all` has been run
- [ ] `CHANGELOG.md` updated under `## [Unreleased]` with a brief entry
- [ ] Shared crates remain **app-agnostic**: no UI framework imports, no app-specific types, no time/clip coupling in non-media crates

## What belongs here vs in an app repo

Promote code to a shared crate only when it is **genuinely needed by multiple apps**. Features that only one app needs belong in that app's repository. Before promoting code, coordinate with the owners of the other consuming apps to agree on an API shape that works for all.

Planned promotions (not yet done — coordinate before starting): `prism-vector`, `prism-fx`, `prism-ai`, `prism-doc`.

## Engine boundaries

- `prism-core` must not import `wgpu`, any UI framework, or any app crate.
- `prism-canvas` owns GPU rendering; `prism-core` owns state. Keep them separate.
- `prism-media` is the only crate that may shell out to external binaries (ffmpeg/ffprobe).
- `prism-ui` may import GPUI but must not import any app crate.
