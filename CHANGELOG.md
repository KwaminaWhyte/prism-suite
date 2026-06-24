# Changelog — Prism suite

Suite-level changes (shared engine crates + cross-app interop). Each app keeps
its own changelog: [pigment](https://github.com/KwaminaWhyte/prism-suite/blob/main/CHANGELOG.md) ·
[contour](https://github.com/KwaminaWhyte/prism-suite/blob/main/CHANGELOG.md) · [pulse](https://github.com/KwaminaWhyte/prism-suite/blob/main/CHANGELOG.md) ·
[reel](https://github.com/KwaminaWhyte/prism-suite/blob/main/CHANGELOG.md).

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); pre-1.0.

## [Unreleased]

## [0.12.0] - 2026-06-24

### Added — Multi-line text + comprehensive text-input coverage
- New shared **`prism_ui::TextArea`** — a multi-line editor (plain Enter = newline,
  Cmd/Ctrl+Enter = submit, vertical line motion, selection, line-granular scroll)
  built on a line-aware extension of `TextInputState` (`move_up`/`move_down`,
  `line_start`/`line_end`, `insert_newline`); prism-ui now 51 tests.
- A second, **comprehensive** text-input pass across all six apps (the 0.11.0 pass
  added the first spots; this makes typing pervasive) — multi-line editors + typeable
  numeric fields + search/filter boxes:
  - **Pigment** — runnable multi-line script editor (`TextArea`), new-doc/image-size dialog, typeable brush size/hardness/opacity + transform rotation/skew.
  - **Contour** — typeable inspector numerics (X/Y/W/H, stroke, opacity, rotation, font size), multi-line text objects, layers filter box.
  - **Pulse** — multi-line expression editor (`TextArea`), effect-browser search, typeable comp-settings + layer-transform numerics.
  - **Reel** — typeable clip opacity/speed/gain/scale/position, multi-line captions, media-bin search.
  - **Drift** — multi-line script editor + AI prompts, typeable inspector numerics (pos/scale/rotation/opacity/keyframe/doc).
  - **Tone** — multi-line MusicGen prompt, typeable clip gain/pitch/length + track volume/pan.
- Full workspace suite green: Pigment 459, Contour 833, Pulse 1047, Reel 374, Drift 672, Tone 759, prism-ui 51 (+ other shared crates).
- Still deferred in `TextArea`: IME/marked-text and mouse caret placement (the element retains shaped-line geometry to add them without an API change).

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField` across all six apps)
- New shared **`prism_ui::TextField`** — a focusable GPUI single-line editor (cursor,
  selection, Shift-select, word motion, clipboard cut/copy/paste, UTF-8-safe) backed by
  a pure, fully-unit-tested `TextInputState` (34 tests). This closes the suite's biggest
  real gap: GPUI 0.2.2 had no usable text-input widget, so prior UIs fell back to
  steppers/preset-chips. Now wired into every app, replacing those with genuine typing:
  - **Pigment** — layer rename, hex color entry, PSD export path + native Browse dialog, editable text-tool content (type real text onto the canvas).
  - **Contour** — Document Setup dimensions, color-picker hex, export path, layer/symbol/artboard rename, editable text-object content (live glyph re-shaping).
  - **Pulse** — typeable expression editor (type ANY rhai expression → evaluate, no longer chip-only), layer/comp rename, render output path.
  - **Reel** — title + Essential-Graphics text, clip/track/sequence/marker rename, export path.
  - **Drift** — typeable AnimateDiff motion prompt, AI-script prompt + script editor, layer rename.
  - **Tone** — typeable MusicGen prompt, project name, BPM/tempo, track/clip rename.
- Per-app test totals after this work: Pigment 452, Contour 821, Pulse 1034, Reel 356, Drift 655, Tone 742, prism-ui 34.
- Deferred (documented): multi-line `TextArea`, IME/marked-text, and mouse caret placement — single-line `TextField` covers Latin typing fully; the element already retains shaped-line geometry to add these without an API change.

### Changed — File organization (no behavior change)
- Split every oversized non-test source file to satisfy the MANDATORY ~1000-line
  rule. Apps: pigment `app_state/mod.rs` (5460→983) + `canvas_host.rs`→dir; contour
  `export.rs`/`panels/inspector.rs`→dirs, `app_state/mod.rs`, `document/mod.rs`,
  `main.rs`; pulse 7 files (`comp/generate`/`mod`/`text`/`effect_browser`,
  `app_state/render`/`mod`, `render/passes`); reel `app_state/mod.rs` +
  `program_frame.rs`/`export.rs`→dirs; drift `main.rs`. Shared: prism-canvas
  `filter_math`/`compositor`/`lib` + prism-media `lib` → submodules with all public
  APIs re-exported (unchanged). Only the two `Action` enums (pigment ~1101, contour
  ~1168) remain over the line — a single Rust `enum` is atomic and can't be split.
  Full workspace test suite green throughout.

## [0.10.0] - 2026-06-24

UI wiring for the 0.9.0 engine features + real ONNX inference scaffolding. All
app-local; shared engine crates unchanged. Full workspace suite green: Pigment
439, Contour 813, Pulse 1027, Reel 349, Drift 651, Tone 734 (+ shared-crate tests).

### Added — UI panels
- **Pigment** (GPUI) — Preferences + Keyboard-Shortcuts floating windows, Navigator
  panel + multi-doc tab bar, guides/rulers rendering + View menu, Color-management
  menu.
- **Contour** (GPUI) — Document Setup, Export, Color Picker, Preferences floating
  windows.
- **Pulse** (GPUI) — Expression editor, Output Module dialog, Preferences,
  Keying+Lights panels.
- **Reel** (GPUI) — Workspaces switcher, Preferences window, Essential Graphics
  panel, render-bar + proxy status, Export (codec-matrix) window.
- Every child window uses the mandatory `WindowKind::Floating`; all panels emit
  pre-existing actions and read existing state (no dead UI, no new state added).

### Added — ONNX inference (Drift, Tone)
- Inference abstraction (`InferenceBackend`: `Stub` | `Ort`) + a `ModelRegistry`
  over each app's AI model families (Drift: AnimateDiff/FILM-RIFE/Whisper/scripting;
  Tone: MusicGen/Demucs/Magenta/mastering/vocal). Real `ort` 2.x session load + run +
  tensor I/O behind an **optional `onnx` cargo feature** (off by default, so the
  default build/tests pull no native runtime); the deterministic stub remains the
  default backend and the fallback when the feature is off or a model file is
  absent.

### Notes
- Contour and Pulse are GPUI hosts — the suite table's "eframe/egui" labels for them
  are stale; child windows follow the `WindowKind::Floating` rule.

## [0.9.0] - 2026-06-24

Three parallel feature waves across the four lower-parity apps (Pigment, Contour,
Pulse, Reel). All new logic lives in per-app domain modules — the shared engine
crates (`prism-core`, `prism-io`, `prism-media`, `prism-color`, `prism-canvas`,
`prism-ui`) are unchanged. Full workspace test suite green: Pigment 439, Contour
813, Pulse 1019, Reel 349, Drift 635, Tone 703.

### Added — Per-app features (see each app's changelog)
- **Pigment** — non-destructive Smart Objects; actions/batch automation; rhai
  scripting sandbox; rich preferences; per-artboard export; native PSD serializer;
  guides/rulers/smart-guides; color management (CMYK/Lab); remappable keyboard
  shortcuts; navigator + multi-doc tabs; surface/path blur, satin + pattern-overlay
  bake, free-transform rotate/skew, red-eye.
- **Contour** — outline-text; full Pathfinder (Divide/Trim/Merge/Crop/Outline/
  Minus-Back); perspective + envelope distort; Live Paint; real raster→vector
  Image Trace; pattern brush; chart/graph generation; document setup; symbol-edit
  mode; CPU Extrude/Revolve 3D; gradient mesh; PNG/SVG/EPS/PDF export; color picker
  + preferences.
- **Pulse** — full After-Effects expression API (`loopOut/In/InOut`, `valueAtTime`,
  `wiggle`, `ease`, `posterizeTime`); output modules; preferences; disk-cache
  manager; ML-free rotobrush; audio mixer; mask editor; fractal-noise + turbulent-
  displace; per-layer motion blur; time-stretch/remap; keying suite (chroma/color/
  luma + spill); distortion fx (corner-pin/bezier/wave/roughen); 3D lights &
  materials; shape repeater + trim paths; render-format module.
- **Reel** — time-remap keyframes + freeze-frame; transition suite; EDL/FCP-XML
  export; HSL secondary curves; 3DL LUT; caption positions; export codec matrix
  (ProRes/H.26x/VP9/AV1/DNxHR ffmpeg-arg builder); multicam waveform/timecode sync;
  project relink/consolidate; autosave/crash recovery; Essential-Graphics
  templates; proxy workflow; background render cache; preferences + remappable
  keybindings; workspaces.

### Changed — File organization
- Split oversized source files to respect the ~1000-line rule. **Pulse**:
  `composition.rs`, `effects_chain.rs`, `mod.rs`, `actions.rs` → focused sibling
  modules + a new `dispatch.rs` action router. **Reel**: `timeline.rs` (2367 lines)
  → `timeline.rs` core + `timeline_clips` / `timeline_selection` /
  `timeline_transitions` / `timeline_playback`.

### Fixed — Drift
- `test_queue_animatediff` asserted a `Queued` status the synchronous stub never
  produces (queuing completes immediately — adds motion keyframes and marks the
  job `Done`); test aligned with the intended behaviour.

## [0.8.0] - 2026-06-19

### Changed — Repository
- **Monorepo migration**: the 5 sibling repos (`prism-suite-prism`, `prism-suite-pigment`, `-contour`, `-pulse`, `-reel`) merged into one Cargo workspace at `prism-suite/`. Layout: `apps/` for binaries + logic libs, `shared/` for engine crates. Single root `Cargo.toml` with `[workspace.dependencies]` pins all third-party deps.
- **Scripts unified**: `package-macos.sh`, `package-linux.sh`, `package-windows.ps1` now accept `AppName / bin / bundle-id` args — one script per platform, not one per app.
- **CI unified**: single `release.yml` matrix (4 apps × 3 platforms = 12 jobs triggered on `vX.Y.Z` tag); single `ci.yml` that runs `cargo check --workspace` on every push/PR.

### Fixed — Pulse
- **Preview center-crop**: `render_preview_frame` scaled comp dimensions without scaling layer geometry, cropping to the centre band. Fix: render at native resolution, then box-downsample (`downscale_frame`).
- **Preview tiny/small**: preview fits the work area via `window.viewport_size()` minus fixed panel widths — fills the canvas instead of sitting at the capped pixel size.
- **Laggy playback**: motion blur preview clamped to `PREVIEW_MB_SAMPLES_CAP = 4` sub-frames for interactive use; export/render queue uses the full sample count.
- **Timeline not scrollable**: lanes wrapped in `overflow_y_scroll`; ruler stays pinned; `TIMELINE_H` increased 180 → 240.

### Fixed — Pigment
- **Marquee not visible**: selection overlay lacked explicit top/left positioning — Taffy placed it below the image. Fix: `.inset_0()` + white dashes added for visibility on dark backgrounds.
- **Canvas overlay order**: overlay moved before `img()` child so ants/guides render on top.
- **Clone/Heal auto-source**: first click without Alt now sets source anchor automatically.
- **Tool options bar**: added missing options for Dodge/Burn, Smudge, Liquify, Transform, Crop.
- **Layer menu**: New Layer, Duplicate Layer, Merge Down actions wired to `app_state`.
- Removed debug log writes (`clone_debug.log`, `pigment_clicks.log`) left by test agent.

### Added — Tests
- `lens_correction`: 6 tests (zero-params identity, flat-field barrel/pincushion, vignette corner vs centre, alpha unchanged, corners dimmed at vignette=1).
- `perspective_warp`: 5 tests (identity warp, flat-field under projective quad, homography round-trip, Gaussian elimination, output size).
- `pigment-gpui` total: 35 tests (was 24).

## [0.4.0] - 2026-06-17

### Added
- **`prism-media` — H.264 MP4 encode (`encode_h264`).** Pipes a lazy iterator of
  straight-alpha `rgba` frames to `ffmpeg` over stdin (`-f rawvideo -pix_fmt rgba
  … -c:v libx264 -pix_fmt yuv420p -preset … -crf … -movflags +faststart`) so a
  long export never holds every frame in memory. The ffmpeg argument construction
  is a pure, unit-tested function (`encode_h264_args`); `ffmpeg_available()`
  probes the binary so callers gate the encode. A missing binary surfaces as
  `MediaError::BinaryNotFound` (never a panic); wrong-size frames, zero geometry,
  an empty stream, or a non-zero ffmpeg exit are clear `MediaError::Decode`s.
  App-agnostic, co-owned by Reel + Pulse.
- **`prism-media` — mux audio into MP4 (`encode_h264_with_audio`).** Additive
  sibling of `encode_h264` (no signature change to the silent path): muxes an
  `AudioMix` (interleaved `f32` + rate + channels) by writing it to a temp WAV
  (dependency-free `WAVE_FORMAT_IEEE_FLOAT` serializer) and feeding it to ffmpeg
  as a second input. The mux ffmpeg-arg construction is a pure, unit-tested
  function (`encode_h264_args_with_audio` → `-map 0:v:0 -map 1:a:0 -c:a aac -b:a
  192k -shortest`); `AudioMix::is_empty()` lets callers fall back to the silent
  encode.
- **`prism-io::document_file` — `SmartFilterMeta.params_ext` overflow slot
  (additive).** `SmartFilterMeta` gains a `params_ext: Option<[f32; 12]>` field
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`) so re-editable
  filter kinds needing more than the base four params (e.g. Camera Raw) round-trip
  in the `.pigment` container. Legacy four-param kinds and older documents
  serialize byte-for-byte unchanged.

## [0.3.0] - 2026-06-13

### Added
- **`prism-io::document_file` — smart-filter stack in `LayerMeta` (additive).**
  `LayerMeta` gains a `smart_filters: Vec<SmartFilterMeta>` field (a new pure-data
  `SmartFilterMeta { kind: u32, params: [f32; 4], enabled: bool }`) so a layer's
  non-destructive, re-editable filter stack round-trips in the `.pigment`
  container. The field is `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  — old documents (and layers with no smart filters) load and serialize exactly
  as before, keeping the format byte-compatible. App-agnostic: the `kind`/`params`
  encoding is owned by the consuming app (Pigment); the shared crate stores the
  values verbatim.

## [0.2.0] - 2026-06-13

### Added
- **`prism-io::text` — optional font family + family enumeration.**
  `render_text` gains a trailing `family: Option<&str>` argument: when
  `Some(name)` (and non-empty) it requests that face via
  `Attrs::family(Family::Name(name))`, otherwise it keeps cosmic-text's default
  sans-serif. Unknown/empty names degrade gracefully through cosmic-text's font
  matching, so rendering never fails. New `available_families() -> Vec<String>`
  enumerates the system font database (sorted, de-duplicated) so apps can
  populate a font chooser; any name it returns is valid input to `render_text`.
  `prism-core::layer::TextDef` gains a `family: Option<String>` field
  (`#[serde(default)]` → `None`) so existing serialized text defs round-trip
  unchanged (absent key deserializes to `None`). Additive and app-agnostic:
  contour/pulse/reel still build (they don't construct `TextDef` literals). New
  tests cover family enumeration, set-vs-default/empty-family rendering, and the
  `TextDef` serde round-trip incl. the legacy (no-`family`) case.

## [0.1.0] - 2026-06-09

### Added
- **`prism-core::gradient`** — new shared, app-agnostic multi-stop gradient
  primitive (used first by Pigment's gradient editor/fill; reusable by Contour's
  gradient meshes and Pulse's ramp generators). Additive and back-compatible —
  no existing API changed, so contour/pulse/reel still build. A `Gradient` has an
  independent **color rail** (`ColorStop` = position + straight RGB in the
  caller's working/linear space) and **opacity rail** (`OpacityStop` = position +
  alpha), interpolated independently and combined (Photoshop's two-rail model).
  Five geometries (`GradientType`: Linear, Radial, Angle, Reflected, Diamond),
  each mapping a pixel to the gradient parameter `t` from a single `start→end`
  drag via `GradientType::param`. `Gradient::sample(t)` returns straight RGBA;
  `Gradient::render(start, end, w, h)` rasterizes to interleaved **premultiplied**
  linear RGBA f32 (matching `shape.rs`), with optional **ordered (Bayer 8×8)
  dithering** to suppress 8-bit banding — fully deterministic (no RNG),
  toggleable, and mean-preserving. Stops need not be pre-sorted (lookups sort
  internally) and clamp to the end stops outside the range. Helper constructors
  (`two_color`, `foreground_to_transparent`, `Default` = black→white) and stable
  `GradientType` id round-trip. All types derive serde, so gradients can be
  embedded in the `.pigment` container later. 18 unit tests cover stop
  interpolation in the working space (incl. multi-stop + unsorted), the
  independent opacity rail, each geometry's parameterization, dither
  determinism/presence/average-preservation, premultiplied render, and
  ids/edge-cases; `prism-io` adds a serde round-trip test.
- **`prism-media`** — new shared A/V decode crate (the planned `prism-media`
  promotion; co-owned with Pulse/Reel). App-agnostic (no wgpu / egui / app
  types): probes media metadata and decodes video frames + whole audio tracks.
  - **Backend: the ffmpeg / ffprobe CLI** (not a `-sys`/`-next` binding). Shells
    out via `std::process::Command`, so it is version-tolerant (works with the
    installed FFmpeg 8.x), needs no `pkg-config` / linking, and stays behind a
    small surface so an in-process libav backend can replace it later. Binary
    paths default to `ffmpeg`/`ffprobe` and are overridable via `PRISM_FFMPEG` /
    `PRISM_FFPROBE`.
  - **API**: `probe(path) -> MediaInfo` (duration / first video stream's
    `width`×`height` + `avg_frame_rate`→fps / audio presence + `AudioInfo`) via
    `ffprobe -print_format json -show_format -show_streams`; `decode_frame_at(path,
    t, scale) -> VideoFrame` (8-bit straight-alpha sRGB RGBA, exactly
    `w*h*4` bytes) via `ffmpeg -ss <t> -frames:v 1 -f rawvideo -pix_fmt rgba
    [-vf scale]`; `decode_audio(path, sample_rate, channels) -> AudioBuffer`
    (interleaved `f32le`, whole-file). `MediaError` (thiserror) with a dedicated
    `BinaryNotFound` so callers degrade gracefully (never panic) when FFmpeg is
    absent.
  - Tests gate on FFmpeg presence (skip silently when absent, mirroring the
    suite's GPU-test-skip convention) — they generate a `testsrc` lavfi clip in
    a temp dir, assert probe geometry/duration/fps and native + scaled frame
    byte counts, decode audio of a sine clip, and assert the missing-binary path
    surfaces `BinaryNotFound`.
- Per-app `CHANGELOG.md` files (this file + one per app) to track work over time.
- `prism-core` — retouch primitives behind Pigment's Phase-6 tools:
  `heal::seamless_clone` (gradient-domain Poisson cloning), `heal::spot_heal`
  (auto-source blemish repair), `inpaint::content_aware_fill` (PatchMatch
  synthesis), `tone::dodge_burn` (local lighten/darken), `tone::sponge`
  (saturation), `detail::blur_sharpen` (local blur/sharpen), and `warp`
  (displacement-field mesh warp + brush stamps, for Liquify).
- `prism-core` — `adjust::Curves` / `CurvePoints` (tone-curve adjustment data);
  `adjust::{Vibrance, PhotoFilter, Posterize}` adjustment variants.
- `prism-io` — `.pigment` doc model gains an optional per-layer `styles` payload
  (`LayerMeta.styles: Option<LayerStyles>`) holding the 8 non-destructive layer
  styles (stroke, drop/inner shadow, color/gradient overlay, outer/inner glow,
  bevel & emboss) as plain serde data (colors `[f32;4]`/`[f32;3]`, sizes/offsets
  in px, angles in deg — no GPU/app types). Serialized with `#[serde(default)]` +
  `skip_serializing_if` so old docs (no `styles` key) still load and style-less
  layers stay byte-compact; full-payload round-trip + old-doc back-compat tested.
- `prism-io` — `.pigment` doc model gains an optional per-layer `adjustment`
  payload (`LayerMeta.adjustment: Option<prism_core::Adjustment>`) that stores an
  adjustment layer's full descriptor (kind + every param, all 14 kinds) by
  reusing the shared `Adjustment` enum's own serde derive verbatim — so the
  variable-length Curves control points and the Channel Mixer matrix serialize
  unchanged, and adding adjustment kinds needs no `prism-io` change. App-agnostic
  (just the already-shared `prism-core` type). Serialized with `#[serde(default)]`
  + `skip_serializing_if` so old docs (no `adjustment` key) still load and
  non-adjustment layers stay byte-compact; full-payload round-trip + old-doc
  back-compat tested. Closes the data-loss gap where reopening a saved Pigment
  document dropped every adjustment layer's parameters.

### Per-app progress (see each app's changelog)
- **Pigment** — Curves adjustment (GPU LUT); **Phase-6 retouch core**: Clone Stamp,
  Healing Brush, Spot Healing, Content-Aware Fill, Dodge & Burn, Liquify,
  Detail brush (sponge/blur/sharpen); **Phase-7 adjustments**: Vibrance, Photo
  Filter, Posterize.
- **Contour** — undo/redo; direct-select path editing; stroke options
  (caps/joins/dashes); **multi-select + Align & Distribute**.
- **Pulse** — keyframe interpolation; graph editor; PNG image-sequence export +
  software compositor; **anchor point + layer parenting**.
- **Reel** — source in/out + ripple/roll/slip/slide editing; transitions; per-clip
  transform/opacity/crop + inspector; sequence markers + work-area; nested
  sequences; **real video decode** (`ClipSource::Video` via the new `prism-media`
  ffmpeg-CLI bridge — frames decoded + scrubbed on the timeline).

## [0.0.1] - 2026-06-06

The suite established: one shared GPU-agnostic engine, four interoperating apps.

### Added
- **Shared engine crates** (root Cargo workspace):
  - `prism-color` — color science: `Rgba`, the sRGB↔linear boundary.
  - `prism-core` — GPU-agnostic document/scene model: layer tree, blend, tiles,
    adjust, curve, raster, shape, histogram.
  - `prism-io` — file↔pixels: image/PSD/EXR decode, text, resize, export,
    `.pigment` doc file.
- **Four apps**, each its own git repo + Cargo workspace, path-depending on the
  shared crates:
  - **Pigment** (Photoshop / raster) — most built; the only app with a custom
    wgpu compositor.
  - **Contour** (Illustrator / vector).
  - **Pulse** (After Effects / motion).
  - **Reel** (Premiere / video NLE).
- **Cross-app interop** — Pigment Dynamic-Links a Contour `.contour` artboard as a
  rasterized layer that re-renders when the source file changes (the suite's
  signature glue; `.contour` JSON is the cross-app contract).
- Suite docs: `README.md`, `SUITE.md` (vision), `RESEARCH.md` (shared-engine +
  interop research), per-app `PLAN.md`/`RESEARCH.md`.
