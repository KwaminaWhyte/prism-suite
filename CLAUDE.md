# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Open-source creative suite: six desktop apps in Rust, each targeting ≥85–90% parity with industry-standard tools.

| App | Analog | Domain | Status |
|-----|--------|--------|--------|
| **Pigment** | Photoshop | GPU raster editor | ~91% |
| **Contour** | Illustrator | CPU vector editor | ~77% |
| **Pulse** | After Effects | CPU compositor / motion | ~72% |
| **Reel** | Premiere Pro | NLE / video editor | ~62% |
| **Drift** | Adobe Animate + Char. Animator | AI-first animation | ~100% |
| **Tone** | Logic Pro / GarageBand / Ableton | AI-first music creation | ~100% |

Architectural bet: raster, vector, video frames, comp layers all reduce to compositing tiles through a DAG in linear light, cached by what's dirty. See `SUITE.md` for vision, `RESEARCH.md` for suite-level research.

## Repository layout — single Cargo workspace

```
prism-suite/
  Cargo.toml              # workspace root — pins ALL versions
  apps/
    pigment/               # [[bin]] — Pigment (GPUI host + inline GPU compositor, 291+ tests)
    contour/               # [[bin]] — Contour (GPUI host + all vector logic inline, 712+ tests)
    pulse/                 # [[bin]] — Pulse (GPUI host + compositor + keyframe engine, 849+ tests)
    reel/                  # [[bin]] — Reel (GPUI host + NLE logic, 228+ tests)
    drift/                 # [[bin]] — Drift (GPUI host + animation engine + AI stubs, 427+ tests)
    tone/                  # [[bin]] — Tone (GPUI host + DAW engine + AI stubs, 513+ tests)
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
cargo test -p pigment          # 291: filters, lens, perspective, smart-object, masking, generative-fill, 3d, shapes, boolean ops, layer styles
cargo test -p contour          # 712: document, path, boolean ops, graph, trace, image-trace, perspective-grid, artboards, variable fonts, blend, 3D, PDF
cargo test -p pulse            # 849: compositor, keyframes, render, rotobrush, puppet-pin, camera-tracker, text-animator, mogrt, CC effects, motion paths, audio buses
cargo test -p reel             # 228: timeline, effects, lumetri, captions, audio-mixer, titles, project-manager, media-browser, transitions, export presets
cargo test -p drift            # 427: layers, keyframes, rig, IK/springs, scenes, frame-labels, library, swap-sets, vector, state-machine, 3D, camera, mocap, scripting, beat sync
cargo test -p tone             # 513: tracks, clips, piano-roll, mixer, AI stubs, undo/redo, MIDI ops, clip ops, track groups, automation, tempo map, step sequencer, chord tools, freeze

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

## File organization — MANDATORY

**Never let any source file exceed ~1000 lines.** When a file grows beyond that, split it by domain immediately. This is a hard rule, not a suggestion. AI models and humans both suffer with 5000–9000 line files.

### How to split `app_state.rs` (the pattern for all apps)

Convert `apps/<app>/src/app_state.rs` → `apps/<app>/src/app_state/` directory:

```
app_state/
  mod.rs          ← Action enum, App struct fields, new(), apply() dispatcher ONLY
  <domain>.rs     ← domain types + pub impl App { domain-specific apply helpers } + #[cfg(test)]
```

Each domain file:
- Defines the domain's types (structs, enums)
- Has `pub fn`s on `impl App` for that domain's apply arms
- Has its own `#[cfg(test)]` covering those actions
- Uses `use super::{App, Action};` to access the core types from mod.rs
- Is declared via `mod <domain>;` in mod.rs

`mod.rs` then has: `match action { Action::SomeDomainThing(..) => self.apply_domain_thing(..) }` dispatching to helpers in domain files.

Domain split per app (examples):
- **pigment**: canvas, layers, selections, painting, filters, text, transforms, smart_objects, ai, layer_3d, export
- **contour**: document, paths, text, colors, symbols, effects, perspective, export
- **pulse**: composition, keyframes, effects, render, tracking, expressions, puppeting, text_anim, precomp
- **reel**: timeline, effects, color, audio, captions, export, proxy, multicam, graphics, media
- **drift**: document, layers, keyframes, transforms, rig, ai, export, state_machine
- **tone**: project, tracks, clips, midi, mixer, transport, ai, export

**Shared crates** (`shared/*/src/`) follow the same rule: split by domain, never monolithic files.

**When adding a new batch of features:** add them to a new domain file, not to an existing oversized one. If mod.rs grows past 1000 lines, refactor.

## Worktree hygiene — MANDATORY

After merging a worktree branch into main, **immediately delete the worktree**:
```bash
git worktree remove --force .claude/worktrees/agent-<id>
```

At end of any batch wave (all 4 merges done), run:
```bash
for wt in .claude/worktrees/agent-*; do git worktree remove --force "$wt"; done
```

Never leave stale worktrees. Check with `git worktree list`.

## Child windows / multi-window UI

**GPUI supports multiple OS-level windows.** Use `cx.open_window(WindowOptions {...}, |win, cx| ...)` to open any number of independent windows. Each has its own render tree. Share state between windows via `Model<T>` / `Entity<T>`.

**egui (Contour, Pulse)** uses floating panels via `egui::Window::new("name").show(ctx, |ui| ...)` — not OS-level windows, but floating panels within the same OS window.

Use cases for child windows in each app:
- **Welcome / Home screen**: shown on launch if no document open; close on "New" or "Open Recent"
- **Export dialog**: separate focused window for export settings + progress
- **Preferences**: settings window (keyboard shortcuts, color profiles, etc.)
- **Color picker**: floating picker window (like Photoshop's detachable color picker)
- **Script editor**: floating code/expression editor (Pulse expressions, Reel effects)
- **Progress window**: async operations (AI generation, render queue, export)

Pattern for a welcome window in GPUI:
```rust
cx.open_window(
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(
            Bounds::centered(None, size(px(900.0), px(580.0)), cx)
        )),
        ..Default::default()
    },
    |win, cx| cx.new(|cx| WelcomeView::new(cx)),
)
```

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
