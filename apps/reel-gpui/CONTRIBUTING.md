# Contributing to Reel

Reel is the video NLE of the Prism suite (the Premiere Pro analog). Contributions
are welcome. Please follow this guide to keep the codebase healthy.

## Dev setup

**Requirements:**
- Rust stable (1.82+). Install via [rustup](https://rustup.rs/).
- ffmpeg on your PATH for video decode/encode (the tests that need ffmpeg skip
  gracefully if it is absent; everything else passes without it).

**Clone and run:**

```sh
git clone https://github.com/KwaminaWhyte/prism-suite
cd prism-suite-reel

# The workspace default-members points at reel-gpui, so no --bin flag needed.
cargo run           # launch reel-gpui in debug mode
cargo build         # build without launching
cargo build --release   # optimised build
```

**Shared engine crates** live in the sibling repo `prism-suite-prism` and are
path-depended. Clone it alongside this repo if you need to modify shared code:

```sh
# Sibling layout expected by Cargo.toml path deps:
# ~/Desktop/solo/ prism/prism-suite-prism/   ← shared engine
# ~/Desktop/solo/ prism/prism-suite-reel/    ← this repo
```

## Running tests

```sh
cargo test --workspace       # all tests (ffmpeg-gated tests skip if ffmpeg absent)
cargo test -p reel-gpui      # reel-gpui only
```

All pure logic (timeline model, edit ops, scopes, export math) is unit-tested
headlessly — no GPU, window, or audio hardware needed. Tests that require ffmpeg
are gated with `#[ignore]` / a runtime skip and print a note when skipped.

## Making a change

1. **Fork** the repository on GitHub.
2. **Branch off `main`:**
   ```sh
   git checkout main
   git pull
   git checkout -b feat/my-feature
   ```
3. Make your changes and add tests for new logic.
4. Run the full test suite and confirm it passes: `cargo test --workspace`.
5. Confirm `cargo build -p reel-gpui` succeeds.
6. **Open a PR against `main`.**

Do not open PRs against other branches — all work merges to `main`.

## PR checklist

Before submitting your pull request, verify:

- [ ] `cargo build -p reel-gpui` passes with no errors.
- [ ] `cargo test --workspace` passes (all non-ffmpeg-gated tests green).
- [ ] `cargo clippy -p reel-gpui -- -D warnings` produces no new warnings.
- [ ] **CHANGELOG.md updated** — add a bullet under `## [Unreleased]` describing
  what you added/changed/fixed.
  the parity matrix, flip its checkbox from `[ ]` to `[x]`.
- [ ] No shared-crate (`prism-*`) changes unless the change is truly app-agnostic.
  Shared-crate work should be PRed to `prism-suite-prism` first and then the
  version bumped here.

## Code conventions

- **Actions, not direct mutation.** Panels emit `Action` variants; `App::apply`
  is the only place that mutates `App` fields. Do not add direct field writes in
  panel render functions.
- **Pure, unit-tested model code.** Edit operations (`split`, `ripple_trim`,
  `roll`, …) live in `project/` as pure functions with unit tests. Keep UI and
  model cleanly separated.
- **No egui dependencies.** The production binary is `reel-gpui` (GPUI). Do not
  add `egui` / `eframe` dependencies.
- **Shared crates stay app-agnostic.** `prism-core`, `prism-io`, `prism-media`,
  and `prism-ui` must not import or reference Reel's timeline model.
- **Additive serde changes.** New fields on serialized types must use
  `#[serde(default)]` so existing `.reel` project files load unchanged.

## Questions

Open a GitHub issue or discussion. For shared-engine questions, check
`prism-suite-prism` first — the suite's architecture is documented in its
[SUITE.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/SUITE.md).
