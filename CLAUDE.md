# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Open-source creative suite: four desktop apps in Rust, each targeting ≥85% parity with an Adobe app.

| App | Adobe analog | Domain | Status |
|-----|--------------|--------|--------|
| **Pigment** | Photoshop | GPU raster editor | ~65% |
| **Contour** | Illustrator | CPU vector editor | ~35% |
| **Pulse** | After Effects | CPU compositor / motion | ~25% |
| **Reel** | Premiere Pro | NLE / video editor | ~15% |

Architectural bet: raster, vector, video frames, comp layers all reduce to compositing tiles through a DAG in linear light, cached by what's dirty. See `SUITE.md` for vision, `RESEARCH.md` for suite-level research.

## Repository layout — single Cargo workspace

```
prism-suite/
  Cargo.toml              # workspace root — pins ALL versions
  apps/
    pigment-gpui/         # [[bin]] — Pigment (GPUI host + inline GPU compositor)
    contour-app/          # [lib] — Contour document + CPU rasterizer (94 tests)
    contour-gpui/         # [[bin]] — Contour GPUI host
    pulse-app/            # [lib] + [[bin]] — Pulse compositor + keyframe engine (633 tests)
    pulse-gpui/           # [[bin]] — Pulse GPUI host
    reel-gpui/            # [[bin]] — Reel GPUI host
  shared/
    prism-core/           # doc model, blend modes, adjustments, curves, shapes, histogram
    prism-canvas/         # wgpu GPU compositor: composite/display/dab/filter/selection passes
    prism-color/          # Rgba, sRGB↔linear
    prism-io/             # PNG/JPEG/PSD/EXR I/O, text raster, resize, .pigment doc
    prism-media/          # FFmpeg bridge: video decode/encode, audio mux
    prism-ui/             # GPUI design system: tokens, icons, components
  assets/branding/
  scripts/                # package-macos.sh / package-linux.sh / package-windows.ps1
  .github/workflows/      # ci.yml, release.yml
```

All crates share one `workspace.dependencies` block in root `Cargo.toml`. Per-crate `Cargo.toml` uses `workspace = true` — never hard-code a version that's already pinned at the workspace level.

## Build / run / test

Everything runs from the repo root:

```bash
# Run an app
cargo run -p pigment-gpui
cargo run -p contour-gpui
cargo run -p pulse-gpui
cargo run -p reel-gpui

# Check all
cargo check --workspace

# Test all (GPU tests skip silently when no adapter)
cargo test --workspace

# Per-crate tests (most tests here)
cargo test -p pigment-gpui       # 35: filters, lens, perspective
cargo test -p contour-app        # 94: document, path, boolean ops
cargo test -p pulse-app --no-default-features   # 633: compositor, keyframes, render
cargo test -p reel-gpui

# Subset by name
cargo test flood_fill
```

Unit tests live inline (`#[cfg(test)]`) in the source files they cover. GPUI binary crates (`[[bin]]`) have no separate lib crate; tests compile into a separate test binary automatically.

## GPU model — Pigment is the exception

- **Pigment** uses **GPUI + wgpu**. WGSL shaders in `apps/pigment-gpui/src/shaders/` (`composite`, `display`, `dab`, `filter`, `selection`). GPU compositor lives entirely inside `pigment-gpui` — it has NOT been promoted to `prism-canvas`. Compositor passes run in GPUI's `prepare_frame` hook.
- **Contour / Pulse / Reel** use eframe/egui (Contour, Pulse) or GPUI without custom GPU passes (Reel). Do not add wgpu pipelines to these without a strong reason.
- Compositing: linear-light, premultiplied, `Rgba16Float` working textures. sRGB↔linear boundary owned by `prism-color`; encode at display blit only.

## Engine boundaries

- `prism-core` knows nothing about wgpu — owns state only. Keep it that way.
- Shared crates stay app-agnostic. Pulse's time axis, Reel's clip model are layers ON TOP of shared code, not changes TO it.
- Code landing in `shared/` must be needed by ≥2 apps. Everything else belongs in `apps/`.
- Planned future shared crates (not yet promoted): `prism-vector` (paths/booleans), `prism-fx` (OpenFX effects), `prism-ai` (ort runtime). Coordinate before promoting.

## Conventions

- Workspace pins: `gpui = "0.2.2"`, `egui`/`eframe = "0.34"`. Never add a duplicate direct dep that conflicts with a workspace pin.
- Lint policy via `[workspace.lints]`: `clippy::needless_range_loop`, `too_many_arguments`, `field_reassign_with_default`, `rust::deprecated` all `allow`. Don't "fix" these.
- `dev.opt-level = 1`, all deps at `3` — keep app interactive during iteration.
- Commit messages: **never** add `Co-Authored-By` trailers.

## Doc map

- `SUITE.md` — four-app vision, interop mechanisms
- `RESEARCH.md` — suite-level shared-engine research, crate matrix
- `UI_SYSTEM.md` — GPUI design system implementation guide
- `UI_UX.md` — UX patterns and interaction design
- `VERSIONING.md` — SemVer policy, release process
- `apps/<app>/PLAN.md` — per-app phased roadmap to ≥85% Adobe parity
- `apps/pigment-gpui/ARCHITECTURE.md` — Pigment module/data-flow detail
