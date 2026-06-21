# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Open-source creative suite: six desktop apps in Rust, each targeting ≥85–90% parity with industry-standard tools.

| App | Analog | Domain | Status |
|-----|--------|--------|--------|
| **Pigment** | Photoshop | GPU raster editor | ~87% |
| **Contour** | Illustrator | CPU vector editor | ~69% |
| **Pulse** | After Effects | CPU compositor / motion | ~64% |
| **Reel** | Premiere Pro | NLE / video editor | ~52% |
| **Drift** | Adobe Animate + Char. Animator | AI-first animation | ~5% (scaffold) |
| **Tone** | Logic Pro / GarageBand / Ableton | AI-first music creation | ~5% (scaffold) |

Architectural bet: raster, vector, video frames, comp layers all reduce to compositing tiles through a DAG in linear light, cached by what's dirty. See `SUITE.md` for vision, `RESEARCH.md` for suite-level research.

## Repository layout — single Cargo workspace

```
prism-suite/
  Cargo.toml              # workspace root — pins ALL versions
  apps/
    pigment/               # [[bin]] — Pigment (GPUI host + inline GPU compositor, 155+ tests)
    contour/               # [[bin]] — Contour (GPUI host + all vector logic inline, 667+ tests)
    pulse/                 # [[bin]] — Pulse (GPUI host + compositor + keyframe engine, 794+ tests)
    reel/                  # [[bin]] — Reel (GPUI host + NLE logic, 155+ tests)
    drift/                 # [[bin]] — Drift (GPUI host + animation engine + AI stubs, 101+ tests)
    tone/                  # [[bin]] — Tone (GPUI host + DAW engine + AI stubs, 115+ tests)
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
cargo run -p pigment
cargo run -p contour
cargo run -p pulse
cargo run -p reel
cargo run -p drift
cargo run -p tone

# Check all
cargo check --workspace

# Test all (GPU tests skip silently when no adapter)
cargo test --workspace

# Per-crate tests (most tests here)
cargo test -p pigment          # 155: filters, lens, perspective, smart-object, masking, generative-fill, 3d
cargo test -p contour          # 667: document, path, boolean ops, graph, trace, image-trace, perspective-grid, artboards
cargo test -p pulse            # 794: compositor, keyframes, render, rotobrush, puppet-pin, camera-tracker, text-animator, mogrt
cargo test -p reel             # 155: timeline, effects, lumetri, captions, audio-mixer, titles, project-manager, media-browser
cargo test -p drift            # 101: layers, keyframes, rig, state-machine, AI stubs
cargo test -p tone             # 115: tracks, clips, piano-roll, mixer, AI generation stubs

# Subset by name
cargo test flood_fill
```

Unit tests live inline (`#[cfg(test)]`) in the source files they cover. GPUI binary crates (`[[bin]]`) have no separate lib crate; tests compile into a separate test binary automatically.

## GPU model — Pigment is the exception

- **Pigment** uses **GPUI + wgpu**. WGSL shaders in `apps/pigment/src/shaders/` (`composite`, `display`, `dab`, `filter`, `selection`). GPU compositor lives entirely inside `pigment` — it has NOT been promoted to `prism-canvas`. Compositor passes run in GPUI's `prepare_frame` hook.
- **Contour / Pulse** use eframe/egui. Do not add wgpu pipelines without a strong reason.
- **Reel / Drift / Tone** use GPUI without custom GPU passes.
- **Drift**: GPUI host + planned wgpu animation canvas (frame compositing via `prism-canvas`). AI inference via `ort` crate (ONNX Runtime).
- **Tone**: GPUI host + custom piano roll canvas. Audio I/O via `prism-media` (FFmpeg) + CPAL for real-time playback. AI inference via `ort` crate.
- Compositing: linear-light, premultiplied, `Rgba16Float` working textures. sRGB↔linear boundary owned by `prism-color`; encode at display blit only.

## Engine boundaries

- `prism-core` knows nothing about wgpu — owns state only. Keep it that way.
- Shared crates stay app-agnostic. Pulse's time axis, Reel's clip model are layers ON TOP of shared code, not changes TO it.
- Code landing in `shared/` must be needed by ≥2 apps. Everything else belongs in `apps/`.
- Planned future shared crates (not yet promoted): `prism-vector` (paths/booleans), `prism-fx` (OpenFX effects), `prism-ai` (ort runtime — promote when ≥2 apps need ONNX inference). Coordinate before promoting.
- **Drift and Tone are AI-first**: their AI features stub ONNX calls now; real models land when `prism-ai` is promoted or inline `ort` wrappers are added in Phase 3+.

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
- `apps/<app>/PLAN.md` — per-app phased roadmap to ≥85–90% parity
- `apps/<app>/ARCHITECTURE.md` — per-app module/data-flow detail (all 6 apps have this)
- `apps/<app>/RESEARCH.md` — per-app competitor analysis, AI model choices, UX principles
