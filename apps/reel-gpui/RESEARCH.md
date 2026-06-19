# Reel — Research Findings (June 2026)

Cited findings backing [PLAN.md](./PLAN.md). Verify all crate versions against crates.io at build time
— third-party version metadata is sometimes stale. Reel is **app #4 of the Prism suite** (the Premiere
analog); shared-engine decisions live in [SUITE.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/SUITE.md) and reuse the suite's compositor/color
research ([Pigment RESEARCH.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)) and the media engine co-designed with Pulse
([Pulse RESEARCH.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)).

---

## 1. The program monitor is the suite compositor + a time/clip layer

An NLE's program output is, structurally, **the suite compositor sampled at a playhead** with clips as
the layer sources. The render-graph/tile/blend machinery (`prism-core`, 18 blend modes, linear-light
float; see [Pigment RESEARCH.md §2](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)) and Pulse's *time-addressable* extension
(properties as functions of `t`; see [Pulse RESEARCH.md §1–2](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)) transfer directly.
Reel's distinct additions are **clips with source in/out ranges on tracks** and **A/V sync** — the editing
model, not the compositor. Reuse, don't reimplement; keep the compositor clip-agnostic.

Sources: [Pigment RESEARCH.md §2](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · [Pulse RESEARCH.md §1](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) · w3.org/TR/compositing-1

## 2. Video decode — FFmpeg, frame-accurate seek (shared `prism-media`)

- The headline gap (today every clip is a still). Decode runs through the suite **`prism-media`** crate
  wrapping FFmpeg, **co-owned with Pulse** so the NLE and the compositor decode identically. Options:
  **`ffmpeg-next`** (mature safe wrapper, broad API), **`rsmpeg`** (exposes more of FFmpeg's power —
  useful for precise seek/packet control), **`video-rs`** (higher-level read/write/encode), or the newer
  pure-Rust **`oximedia`** (FFmpeg+OpenCV reconstruction; DPX/EXR/TIFF + color science; v0.1.7 2026-05,
  young).
- **Frame-accurate seek** is the correctness crux: seek to the nearest prior keyframe, then decode forward
  to the target presentation timestamp (PTS); convert timeline time ↔ source PTS with integer frame math.
  Maintain a small **decode ring/cache** around the playhead and a **thumbnail** decode path for the
  timeline. Long-GOP codecs (H.264/265) make backward scrubbing expensive → hardware decode +
  preview/proxy media (Phase 8/10).
- **Hardware decode** (VideoToolbox on macOS, NVDEC, QuickSync) via FFmpeg hwaccel is the realtime lever
  for HD/4K playback.

Sources: crates.io/crates/ffmpeg-next · github.com/larksuite/rsmpeg · crates.io/crates/video-rs · github.com/cool-japan/oximedia · [Pulse RESEARCH.md §6](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 3. Audio — decode, playback, resample, mix (pure Rust)

- **`symphonia`** — pure-Rust decode/demux for AAC, ALAC, FLAC, MP3, OGG/Vorbis, WAV, MP4, MKV, CAF… at
  roughly ±15% of FFmpeg's speed; the decode foundation.
- **`cpal`** — cross-platform device output (CoreAudio/WASAPI/ALSA) via the standard callback model.
- **`rubato`** — real-time-safe sinc + FFT resamplers (no per-block allocation); resample every track to
  the project sample rate before mixing.
- **`symphonium`** / **`playback-rs`** — ready-made Symphonia+cpal+rubato wrappers to bootstrap playback,
  then specialize for multitrack mixing.
- **A/V sync**: treat audio as the master clock (steady device callback), slave video frame selection to
  the audio time; measure and bound drift in tests. **Waveforms** = decode min/max peaks per pixel column,
  cached per media item.

Sources: crates.io/crates/symphonia · lib.rs/crates/rubato · crates.io/crates/symphonium · lib.rs/crates/playback-rs · crates.io/crates/cpal

## 4. The editing model — ripple / roll / slip / slide, 3/4-point

Premiere's core trim tools are precise interval operations; model them as **pure functions** over
`(start, source_in, source_out, duration, track)` and property-test them before any UI:
- **Ripple** — change a clip's in/out and shift all downstream clips by the delta (no gap left).
- **Roll** — move the edit point between two adjacent clips: out of the left = in of the right; total
  duration unchanged.
- **Slip** — change a clip's source in/out together (content shifts) with timeline position/duration fixed.
- **Slide** — move a clip along the timeline; adjacent clips' adjoining edges absorb the move; the slid
  clip's content is unchanged.
- **3-point / 4-point edits** — choose source in/out + timeline in/out; insert (ripples downstream) vs
  overwrite (replaces). Backtiming when 4 points over-constrain.
Plus razor/split, lift (leave gap) vs extract (ripple-close), ripple-delete, J/K/L shuttle, snapping,
markers, and **nested sequences** (a sequence as a clip source). Premiere 25.5 (Sep 2025) added 90+
transitions/effects (Film Impact acquisition) — the long tail is `prism-fx` content, not new architecture.

Sources: nobledesktop.com (ripple/roll/slip/slide) · mediacollege.com (ripple edit) · blog.adobe.com (Premiere 25.5, 90+ effects/transitions) · helpx.adobe.com/premiere-pro (editing)

## 5. Transitions & effects (shared `prism-fx`)

Transitions are two-input `prism-fx` passes parameterized by a 0→1 progress (the OpenFX-style host shared
with Pigment/Pulse): cross-dissolve, dip-to-black/white, wipes, push/slide/zoom, plus **audio crossfades**
(constant-power/linear gain). Clip effects are single-input `prism-fx` passes stacked per clip with
**keyframable params** (full `Property<T>` + Bézier ease, reusing Pulse's interpolation model). Built-ins
(transform/crop/opacity/blend/mask/blur) reuse `prism-core` blend + `kurbo` masks. Authoring once means an
effect written for Pulse or Pigment runs in Reel too.

Sources: openfx.readthedocs.io · [Pulse RESEARCH.md §2,4](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) (Property<T>, prism-fx) · [Pigment RESEARCH.md §8](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 6. Color — Lumetri-grade, scopes, OCIO

Color is the suite's `prism-color` (linear-light, ICC via `lcms2`, OCIO target). A Lumetri-style grade is
an ordered stack: **basic correction** (WB/exposure/contrast/highlights-shadows), **curves** (RGB + per-
channel + hue-vs-hue/sat/luma), **color wheels** (lift/gamma/gain), **HSL secondaries** (key→refine→grade),
LUTs (`.cube`/`.3dl`). **Scopes** — waveform, vectorscope, histogram, RGB parade — are GPU reductions over
the program frame. **OpenColorIO** (ASWF Rust binding in progress; `exr` for scene-linear meanwhile)
supplies log→linear input transforms and display/output transforms; HDR (PQ/HLG) delivery is range +
transfer-function aware. Most of this is shared with Pulse's color-correction effects.

Sources: helpx.adobe.com/premiere-pro (Lumetri color, scopes) · aswf.io (OpenColorIO/OpenEXR Rust) · github.com/kornelski/rust-lcms2 · [Pigment RESEARCH.md §9](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)

## 7. Titles, captions, interchange

- **Titles / graphics** use `cosmic-text` (shaping, OpenType, variable fonts) + the suite vector
  primitives; Essential-Graphics-style **templates** are parameterized title comps (shareable). Real
  motion-design titles are authored in **Pulse** and placed via Dynamic Link.
- **Captions** — import/export **SRT/VTT**, styled caption track, burn-in vs sidecar; AI **transcription**
  (Whisper-class via `ort`) generates captions and enables **text-based editing** (edit video by editing
  the transcript) and **silence/filler removal**.
- **Interchange** — EDL / Final-Cut-XML / AAF and **OpenTimelineIO** (OTIO) for round-tripping timelines
  with other NLEs and the grading/finishing world; OTIO is the modern lingua franca.

Sources: helpx.adobe.com/premiere-pro (captions, Essential Graphics) · opentimeline.io · github.com/openai/whisper (via ort) · [Pulse RESEARCH.md §7](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) (text animators / Dynamic Link)

## 8. Export, render queue, performance

- **Export** re-uses `prism-media` encode: H.264/H.265/**ProRes**/VP9/AV1/DNxHR, MP4/MOV/MKV containers,
  image sequences (PNG/EXR/DPX/TIFF), audio (WAV/AAC), GIF/APNG/WebP; a **Media-Encoder-style** background
  **render queue** with presets (social/web/broadcast), range/in-out export, VBR/CBR, and **hardware
  encode** (NVENC/QuickSync/VideoToolbox). **Smart render** reuses matching preview files.
- **Performance & reliability** — the NLE-defining work: background decode threads + frame/decode cache,
  preview-file (render-and-replace) generation, **proxies/optimized media**, dropped-frame indicator,
  playback-resolution downscale, **multi-frame** export (`rayon`), plus **autosave/crash recovery** and a
  versioned `.reel` project with **relink-missing-media**. Multicam (sync N angles, live switching) and
  project-management (consolidate/transfer) round out pro workflows.

Sources: helpx.adobe.com/premiere-pro (export, Media Encoder, proxies, multicam) · [Pulse RESEARCH.md §9](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md) (caching, MFR) · github.com/rayon-rs/rayon

## 9. AI (feature-gated, shared `pigment-ai`/`ort`)

Reuse the suite `ort` (ONNX Runtime, CoreML/DirectML/CUDA EPs) runtime — not a separate stack. Video-
relevant uses: **Scene Edit Detection** (cut detection → split), **Auto Reframe** (subject track → aspect
reframe keyframes), **object masking/tracking** (SAM2/3, shared with Pulse roto), **transcription**
(Whisper-class → captions + text-based editing + silence removal), and **Generative Extend** (extend a
clip's tail) which stays **optional and pluggable — local model or BYO cloud key, never required**, per the
suite AI policy ([RESEARCH.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)). Models are fetched on demand behind a feature flag with
license surfacing; every tool degrades gracefully with no model/GPU.

Sources: blog.adobe.com (Premiere AI: scene-edit detect, auto-reframe, object masking, generative extend) · github.com/pykeio/ort · github.com/ZhengPeng7/BiRefNet · [Pigment RESEARCH.md §10](https://github.com/KwaminaWhyte/prism-suite/blob/main/RESEARCH.md)
