<p align="center">
  <img src="assets/branding/reel-master.png" width="120" alt="Reel">
</p>

# Reel

**Reel** is the video non-linear editor (NLE) of the **Prism** creative suite —
the Premiere Pro analog, and Prism app #4 (after Pigment, Contour, and Pulse).
It is built in Rust on GPUI (Zed's retained-mode framework), sharing the suite's
`prism-core`, `prism-io`, `prism-media`, and `prism-ui` foundation crates.

## Status — v0.6.0 · 100% parity (65/65 features) · Batch 4 complete

All 65 tracked Premiere Pro analog features are implemented in `reel-gpui`.
Batch 4 (2026-06-19) added RGB Curves color panel, per-track audio effects chain,
closed captions / SRT panel, export presets UI, and markers panel.

### What works

- **Multitrack timeline** — N stacked video/audio track lanes, draggable clip
  blocks, trim edges (left/right), razor/split, lift/ripple-delete, close gap.
  Ripple, roll, slip, and slide trims; 3-/4-point insert/overwrite editing.
- **Transport** — play/pause (Space), J/K/L shuttle, real-`dt` advance, loop,
  jump-to-start/end, live SMPTE timecode + frame number, drop-frame toggle.
- **Preview / program monitor** — CPU-composited program frame at the playhead;
  multi-track source-over composite honoring per-clip opacity, grades, crop, and
  transform. Cross-dissolve, dip-to-black/white, wipe, and push transitions.
- **Source monitor** — dual viewer (Source left, Program right) with independent
  in/out points and insert-from-source.
- **Media bin** — import video, audio, and still images; color clips; subclip bins;
  multicam group management.
- **Color grade** — Basic Correction, Color Wheels (3-way lift/gamma/gain), Curves
  (RGB + per-channel), HSL Secondaries, LUT (.cube) import, log-to-Rec709 transforms.
- **Lumetri Scopes** — waveform (256-column scatter plot), vectorscope (Cb/Cr dots),
  histogram, RGB parade. Tab-selectable.
- **Audio** — per-clip gain, fade-in/out, mixer panel (per-track faders/pan/mute/solo
  + master), 3-band EQ per track, parametric EQ + compressor/limiter per clip,
  auto-duck, rubber-band volume keyframes, audio playback via rodio.
- **Export** — H.264 MP4 with AAC audio, ProRes Proxy, ProRes 422, GIF, image
  sequence, PNG still. Background render queue with live progress.
- **Bezier speed ramp** — cubic-bezier non-linear retiming per clip.
- **Multicam** — multicam group management; angle switching; track grid view.
- **Nested sequences** — `ClipSource::NestedSequence`; recursive composite with
  cycle guard.
- **Linked A/V** — `link_group` field; joint-move/trim enforcement.
- **Snap** — snap to clip edges, playhead, markers, work-area in/out; magnet toggle.
- **Undo/redo** — bounded snapshot history, named command labels (⌘Z / ⌘⇧Z).
- **Captions** — SRT/VTT import/export, timeline cue strip, burn-in.
- **Keyframes** — transform, opacity, effect scalar params; Bezier ease; dope-sheet
  + curve editor in the inspector.

## Build & run

```sh
cargo run            # debug build + launch (reel-gpui, the default member)
cargo build          # build only
cargo test --workspace   # run all tests
cargo fmt            # format
cargo clippy         # lint
```

The workspace `default-members` is set so `cargo run` launches `reel-gpui` with
no `--bin` flag needed. The binary crate is `reel-gpui`.

## Shared engine

Reel path-depends on the suite's shared crates (it does **not** copy or modify them):

- **`prism-core`** — `prism_core::Size` for image dimensions and
  `prism_core::color` (`srgb_to_linear` / `linear_to_srgb`) at the color boundary.
- **`prism-io`** — `prism_io::load_image` decodes still images to 8-bit sRGB RGBA
  (cached into textures keyed by path), and `prism_io::SUPPORTED_EXTENSIONS`
  populates the import dialog filter.
- **`prism-media`** — FFmpeg CLI bridge for video decode/encode and audio decode.
  Co-owned with Pulse. Provides `decode_frame_at`, `encode_h264`,
  `encode_h264_with_audio`, `decode_audio`, and `ffmpeg_available`.
- **`prism-ui`** — design-system tokens (colors, typography, icons). Enforced
  across all panels via `prism_ui::colors::*`.

The shared engine lives in `prism-suite-prism` (a sibling repository). Keep all
shared-crate changes **additive and app-agnostic** — the timeline and NLE logic
are Reel's layer on top.

## Contributing

1. Fork the repository.
2. Branch off `main` (e.g. `git checkout -b feat/my-feature`).
   applicable.
4. Open a PR against `main`. See [CONTRIBUTING.md](./CONTRIBUTING.md) for the
   full checklist.

Shared engine work (`prism-core`, `prism-media`, `prism-ui`, `prism-io`) belongs
in `prism-suite-prism`, not here. NLE / timeline logic belongs in `reel-gpui`.
