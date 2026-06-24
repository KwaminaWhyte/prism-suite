# Reel — Open Source Premiere Pro Alternative

> **Status: ~62% parity (Phases 0–8 done, Batch 5 wave 2 done). Target ≥85% by end of Phase 7.**

## Batch 5 (wave 2) — Completed (2026-06-22)

- [x] **Lumetri Scopes Config** — `LumetriScopesConfig` with 7 new enums; intensity clamped 10–300 (default 75); `scopes_config` on App; 10 new actions (`ToggleScopesPanel/SetScopeKind/SetScopeLayout/SetWaveformType/SetParadeType/SetVectorscopeType/SetHistogramChannel/SetScopeIntensity/SetScopeColorspace/SetScopeShowClipping`).
- [x] **Project Bins & Media Manager** — `ProjectBin` + `MediaItem` + `BinColor/MediaKind` enums; 16 actions (bin CRUD, import/remove/move/relink/label/log-note/offline/proxy attach-detach); `project_bins/media_items/next_bin_id/next_media_item_id` on App.
- [x] **Extended Transitions** — 15 new `TransitionKind` variants (Slide/Split/Swap/Zoom/SpinAway/PagePeel/PageTurn/Cube/Film/Luma/DipToBlack/DipToWhite/AdditiveDissolve/NonAdditiveDissolve/RandomInvert) + 7 direction enums in `timeline.rs`; `weights()` handles all new variants; 6 new transition actions.
- [x] **Export Presets B5** — `ExportPresetB5` with 7 built-in presets (YouTube 1080p/4K, Twitter/X, Vimeo, ProRes 422, GIF, MP3 Audio); full CRUD; `ExportCategory/ExportContainer/VideoCodecB5/AudioCodecB5` enums; `export_presets_b5/active_export_preset_b5/next_preset_id` on App.
- [x] **Sequence Settings B5 / Multi-sequence** — `SequenceSettingsB5` with id/name/size/frame_rate/field_order/timebase/working_color_space; `FieldOrder` enum; 6 actions (new/duplicate/delete/setActive/updateSettings/nest); delete guards last sequence; `sequences_b5/active_sequence_id/next_sequence_id` on App.
- 64 tests added → **228 total**
- New files: `reel_project.rs` (272 lines), `apply_batch5.rs` (338 lines), `tests_batch5.rs` (703 lines)

## Batch 8 — Completed (2026-06-20)

- [x] **Multicam depth** — `MulticamAngle { label, source_clip_idx, sync_offset, enabled }`; `MulticamSyncMode/MulticamDisplayMode` enums; `multicam_angles/active_angle/sync_mode/display_mode` on App. `Action::AddMulticamAngle/RemoveMulticamAngle/SetMulticamAngleLabel/SetMulticamAngleSyncOffset/ToggleMulticamAngle/SetMulticamSyncMode/SetMulticamDisplayMode/FlattenMulticam`.
- [x] **EDL / XML interchange** — `EdlFormat` (Cmx3600/FcpXml/Aaf/Otio) + `EdlConfig { format, frame_rate, reel_name, include_audio, include_video }`. `Action::SetEdlFormat/SetEdlFrameRate/SetEdlReelName/SetEdlIncludeAudio/SetEdlIncludeVideo/ExportEdl/ImportEdl/ExportFcpXml/ImportFcpXml/ExportOtio`.
- [x] **Audio Suite** — `AudioSuiteKind` (8 variants) + `AudioSuiteConfig { kind, gain_db, preserve_duration, process_in_place, clip_by_clip, target_level_db, pitch_semitones, stretch_ratio }`; `audio_suite_config/panel_open/preview` on App. 10 new actions with clamping.
- [x] **Auto Reframe** — `ReframeMotion` enum + `AutoReframeConfig { target_aspect_w/h, motion_preset, keep_scale, analyze_on_import }`; `auto_reframe_config/panel_open/reframe_results` on App. `Action::ToggleAutoReframePanel/SetReframeAspect/SetReframeMotion/SetReframeKeepScale/SetReframeAnalyzeOnImport/AnalyzeReframe/ApplyReframe/ClearReframeResults`.
- 16 tests added → **102 total**

## Batch 7 — Completed (2026-06-20)

- [x] **Per-clip video effects** — `ClipEffectKind` (GaussianBlur/Sharpen/Mosaic/DropShadow/Glow/ChromaticAberration) + `ClipEffect { kind, enabled, intensity, secondary, color }` on `Clip`. `Action::AddClipEffect/RemoveClipEffect/SetClipEffect/ToggleClipEffect/ReorderClipEffects/ClearClipEffects`.
- [x] **Clip Motion (Transform effect)** — `motion_x/y/scale_x/scale_y/rotation` fields on `Clip`. `Action::SetClipMotion/SetClipMotionScale/SetClipMotionRotation/ResetClipMotion`.
- [x] **Scene Edit Detection (stub)** — `SceneEditResult`, sensitivity param, `Action::SetSceneEditSensitivity/DetectSceneEdits/ApplySceneEditSplits`.
- [x] **Project Management** — `project_name/path/recent_project_paths/notes/auto_save_enabled/auto_save_interval_sec` on `App`. `Action::SetProjectName/SetProjectPath/AddRecentProject/SetProjectNotes/SetAutoSaveEnabled/SetAutoSaveInterval/TriggerAutoSave`.

## Batch 6 — Completed (2026-06-19)

- [x] **Clipboard copy/paste clips** — `clipboard_clips: Vec<Clip>` on App; `Action::CopySelectedClips/CutSelectedClips/PasteClips/DuplicateSelectedClips`.
- [x] **Clip transform** — `anchor_x/y`, `crop_left/right/top/bottom`, `ClipBlendMode` enum; `Action::SetClipAnchor/SetClipCrop/SetClipBlendMode/ResetClipTransform`.
- [x] **Time remap** — `time_remap_enabled/keys` on Clip; `Action::SetTimeRemapEnabled/AddTimeRemapKey/MoveTimeRemapKey/RemoveTimeRemapKey/SetFreezeFrame`.
- [x] **LUFS metering** — `lufs_short_term/integrated/power_history` on App; `Action::UpdateLufsMeters/ResetLufsIntegrated`.
- [x] **Group ripple trim** — `Action::GroupRippleTrimIn/GroupRippleTrimOut`.

## Batch 5 — Completed (2026-06-19, depth & polish)

Deepened five existing GPUI-host features so they work end-to-end (model + compositor/mix apply + UI), not stubs. All in `reel-gpui`; 60 tests green; no new warnings.

- [x] **Nested-sequence recursive compositing** — `ClipSource::NestedClip` rendered through the same compositor at the speed/reverse-aware nest-local time (was a purple placeholder); `MAX_NEST_DEPTH=8` cycle guard; identity inner grade. `program_frame::render_program_inner` / `sample_clip_raw`.
- [x] **Caption burn-in onto the program frame** — the active cue is drawn (5×7 font, style color, legibility box, Bottom/Top/Custom position, comp-relative size, multi-line) in the preview, scopes, AND export. Threaded via `program_frame` → `CanvasHost::image` → `Reel::preview_image` + `JobSpec.captions`/`with_progress_encode`. `draw_active_caption`.
- [x] **HSL Secondary grade applied** — `Clip.hsl_secondary` (stored/editable but never applied) now runs in `sample_clip` after the basic grade; inspector **HSL Secondary** section (`Action::SetHslSecondaryGrade`).
- [x] **Real audio FX DSP (Reverb/Delay/EQ3) + chain wiring** — Schroeder reverb (comb→allpass), feedback delay, 3-band RBJ EQ; per-track `audio_effects` chain now applied in `render_program_audio` (`MixClip.fx`, `build_mix_clips` audio_effects param); mixer **+Rev**/**+Dly** buttons.
- [x] **Proxy media used in preview** — the Video decode path reads `clip.proxy_path` when attached; inspector **Proxy** attach/clear (`Action::SetProxyPath`/`ClearProxy`).

## Batch 4 — Completed (2026-06-19)

- [x] **RGB Curves color panel** — `RgbCurves { master, red, green, blue }` per-channel tone curves; linear interpolation; applied in `GlobalGrade::apply` after color wheels; inspector tab selector + stepper controls; `Action::SetRgbCurves/SetCurvesChannel/AddCurvePoint/MoveCurvePoint`.
- [x] **Audio Effects chain per track** — `AudioEffect` enum (Eq3/Compressor/Reverb/Delay) in `app.audio_effects`; FX button in mixer columns; effect chips with remove; +EQ add; `Action::AddAudioEffect/RemoveAudioEffect/ToggleTrackFx`.
- [x] **Closed Captions / Subtitles panel** — `Caption/CaptionStyle/CaptionPosition`; SRT parse/serialize; captions panel with toolbar (Add/Import/Export SRT); scrollable list with active highlight and click-to-seek; toolbar "CC" button.
- [x] **Export Presets UI panel** — `ExportPreset` with 4 built-in defaults; panel with Apply/Del + "Save Current Settings" button; toolbar "Export▾" toggle.
- [x] **Markers panel** — `GpuiMarker/MarkerKind (In/Out/Chapter/Comment)`; sorted list with colored dots; Add kind buttons; Mark In/Out; click-to-jump; toolbar "Markers" toggle.

## Batch 3 — Completed (2026-06-19)

- [x] **RateStretch tool** — `Tool::RateStretch` + `ClipDragKind::RateStretch`; dragging right clip edge changes speed (duration / speed ratio). `Action::RateStretchClip` apply handler.
- [x] **3-way color wheels** — `ColorWheels { lift, gamma, gain }` global struct; `GlobalGrade` carries it; applied after LUT in `program_frame`; inspector panel shows 9 R/G/B steppers; `Action::SetColorWheels`.
- [x] **Sequence settings dialog** — floating overlay; resolution, FPS presets, sample rate, color space; `Action::Toggle/Set* sequence settings`; "Seq" toolbar button.
- [x] **Bins toolbar button** — "Bins" text button in toolbar; `Action::ToggleBins`.
- [x] **Audio scrub burst** — 100ms audio burst on seek while paused via `scrub_audio_burst`.
- [x] **InsertClipFromBin fix** — stub replaced with real `build_imported_clip` + push to timeline.

## Batch 2 — Completed (2026-06-18)

- [x] **ProRes/GIF export format selector** — `ExportFormat` enum + toolbar format buttons (MP4 / ProRes P / ProRes 422 / GIF). `export.rs` gains `ensure_mov_extension` / `ensure_gif_extension`.
- [x] **Speed ramp with Bezier curve** — `SpeedCurve::Bezier { p0..p3 }` field on `Clip`; cubic-bezier helper in `program_frame.rs`; inspector "Bezier" toggle.
- [x] **3-band audio EQ per track** — `TrackEq3 { low/mid/high_gain_db, mid_freq }` per track; mixer L/M/H ±1 dB buttons; `Action::SetTrackEq3`.
- [x] **Waveform scope 256-col pixel dots** — columns raised 64 → 256; `waveform_chart` rewritten as individual 2×2 absolute dots for true scatter-plot display.
- [x] **Multicam group management** — `MulticamGroup { clips, active_angle }`; source viewer "Create Group" button; `Action::SwitchMulticamAngle` / `CreateMulticamGroup`.

Professional video non-linear editor (NLE) in Rust, and **app #4 of the Prism suite** (the Premiere Pro
analog; siblings: [Pigment](https://github.com/KwaminaWhyte/prism-suite) raster, [Contour](https://github.com/KwaminaWhyte/prism-suite) vector, [Pulse](https://github.com/KwaminaWhyte/prism-suite) motion).
**Goal: reach ≥85% of Adobe Premiere Pro's real-world capability** — features, reliability, and
ease-of-use — in staged milestones, on the suite's shared engine: `prism-core` (compositor/render graph),
`prism-color` (Lumetri-grade color + OCIO), `prism-media` (FFmpeg decode/encode + audio, shared with
Pulse), and `prism-fx` (effects/transitions).

> Companion docs: [RESEARCH.md](./RESEARCH.md) (cited findings + crate matrix), [README.md](./README.md) (what runs today), [SUITE.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/SUITE.md) (four-app vision + interop). This PLAN expands the README's v0 scaffold into a parity roadmap.

---

## 0. Why this can work

- The hard parts are solved and free: FFmpeg decode/encode + frame-accurate seek (`prism-media`), a
  tile/render-graph compositor for the program monitor (shared with Pigment/Pulse), color science +
  OpenColorIO (`prism-color`), pure-Rust audio (`symphonia`/`cpal`/`rubato`).
- An NLE is, at its core, **a timeline of clips composited and mixed at the playhead**. The program
  monitor is the *same* compositor Pigment and Pulse already run; Reel adds **clips on tracks with
  source in/out points**, **A/V sync**, **transitions**, and **export**. That shared engine is the
  suite's whole architectural bet.
- Premiere's real moat is interop (Dynamic Link), performance/reliability (caching, background render,
  proxies), and the edit-tool ergonomics. We target those deliberately.

**Non-negotiable principle:** the timeline is **time-addressable and non-destructive** — a clip
references source media + an in/out range + a timeline placement; the program frame at time `t` is
*derived* (decode → effect stack → composite in linear light → mix audio), cached, never baked until
export. Frame-accurate timecode (incl. drop-frame) and A/V sync from day one.

---

## 0a. Suite boundaries — what belongs in Reel vs Pulse / Pigment / Contour

Reel shares `prism-core` / `prism-io` today and will share `prism-media` / `prism-color` / `prism-fx`.
Every feature is filed against three rules so we never overwrite a sibling app's work:

- **Reel-owned (NLE / editing):** the multitrack timeline, clip trim/ripple/roll/slip/slide, 3-/4-point
  edits, insert/overwrite, the source & program monitors, transitions between clips, the audio mixer,
  multicam, proxies/optimized media, sequences/nesting, captions/subtitles, the export/render queue.
  Lives in `reel-app` or NLE-only modules.
- **Shared-crate, app-agnostic:** the compositor/render-graph & blend modes (`prism-core`), color &
  OCIO (`prism-color`), the **`prism-media`** FFmpeg engine + audio (**co-defined with Pulse** — both
  decode/encode identically), and the **`prism-fx`** effects/transitions host. These **must not** assume
  Reel; the timeline (clips, ripple, tracks) is Reel's layer on top.
- **Out of scope — a sibling app's domain (do not build here):**
  - *Deep motion graphics / keyframe compositing / advanced VFX* → **Pulse** (the After Effects analog).
    Reel does clip transforms, speed, basic effects and keyframes; for real motion design it **places a
    Pulse comp via Dynamic Link** (live, re-rendered on edit) rather than re-implementing AE.
  - *Primary raster painting* → **Pigment**; *deep vector authoring* → **Contour**. Reel consumes their
    documents as media/titles via suite interop.
  - *Cross-app interop glue* (Dynamic Link host, `prism-doc` container, shared clipboard/assets) is
    **suite-level**. Reel is the canonical Dynamic-Link **consumer** (Pulse comps drop into its
    timeline), but the mechanism is defined with the suite, not unilaterally.

---

## 0b. GPUI migration — complete

Reel's UI has been fully migrated from eframe/egui to **GPUI** (Zed's
retained-mode framework). The `reel-gpui` crate on `main` is the sole production

**Architecture backbone:**

- **`app_state::App`** is the single shared state every panel reads/mutates and
  the **one mutation choke point `App::apply(Action)`**. Panels emit an `Action`;
  the root view routes it through `apply`, which mutates state and marks the
  preview host dirty when the composited frame changes.
- **CPU preview bridge.** `program_frame.rs` is a headless CPU sampler that
  reproduces the program content at the playhead into an RGBA8 buffer;
  `canvas_host::CanvasHost` converts RGBA8 → BGRA8 and bridges it in as a GPUI
  `RenderImage`, dirty-cached.
- **Panels** are `render(app: &App, cx) -> impl IntoElement` free functions
  emitting `Action`s via `cx.listener`.

## Batch 3 — In Progress

The following features are currently under development on `main`:

- **3-way color wheels** — lift/gamma/gain interactive trackball widgets replacing
  the numeric controls; real-time preview update.
- **Audio scrub** — play audio while scrubbing the timeline; pitch-preserved
  scrub preview via rubato resampling.
- **Media bin enhancements** — folder browsing, metadata columns, smart bin
  filters, drag-to-timeline from bin.
- **Rate stretch tool** — drag a clip edge with the Rate Stretch tool to change
  speed instead of trimming source.
- **Sequence settings dialog** — edit canvas size, frame rate, sample rate, and
  color space for the active sequence.

---

## 1. Current state (what runs today)

Grounded in `reel/crates/reel-app/src/`. A real, editable NLE scaffold (stills only):

- **Model** (`project.rs`) — `Project { width, height, fps, duration, tracks, clips, bin }`;
  `Clip { name, source, track, start, duration }`; `ClipSource = Image(path) | Color(rgba)`;
  `MediaItem` bin entries. `covers(t)`, `topmost_at(t)` (highest track wins) drive the preview.
- **Timeline** (`timeline.rs`) — seconds ruler, N stacked track lanes (V1, V2…), draggable clip blocks:
  move (horizontal = `start`, vertical = track), **trim edges** (left/right, no ripple yet), draggable
  **playhead**, ruler/lane **scrub**, snap to clip edges + playhead.
- **Transport** (`app.rs`) — play/pause (Spacebar), real-`dt` advance, loop at duration, jump-to-
  start/end, live **timecode + frame number**.
- **Preview** (`preview.rs`) — topmost active clip at the playhead, aspect-fit into the comp frame;
  `Color` fills, `Image` drawn letterboxed from a cached decoded texture (`textures.rs`); black when empty.
- **Media bin** (left) — import image (`rfd` + `prism_io::load_image`), add color clip, `+` drops to top
  track at the playhead.
- **Menus** — File (New, Import media…, Save `.reel` JSON), Edit (Delete clip). Prism dark theme (amber
  NLE accent) + phosphor icons. Depends on `prism-core` (`Size`, color) + `prism-io` (`load_image`,
  `SUPPORTED_EXTENSIONS`).

**Reality check:** since the scaffold, source in/out, ripple/roll/slip/slide, transitions, per-clip
effects/keyframes, nested sequences, **undo/redo** (a bounded snapshot history over the document), and
now **real video decode** (`ClipSource::Video` via the shared `prism-media` ffmpeg-CLI bridge — frames
decoded + scrubbed at the playhead) have landed. Still missing: **audio playback + A/V sync** (decode
exists; no output engine yet), a GPU program monitor, and export. These are the road below.

---

## 2. Validated tech stack (verify with `cargo add` at build)

| Concern | Crate | Notes |
|---|---|---|
| Video decode/encode | `prism-media` → `ffmpeg-next` / `rsmpeg` (or `video-rs`) | **Frame-accurate seek** + decode at playhead; encode for export. **Co-owned with Pulse.** |
| Audio decode | `symphonia` | Pure-Rust: AAC/ALAC/FLAC/MP3/OGG/Vorbis/WAV/MP4… within ±15% of FFmpeg. |
| Audio playback | `cpal` | Cross-platform device output (CoreAudio/WASAPI/ALSA) via callback. |
| Resample / SR convert | `rubato` | Real-time-safe sinc/FFT resamplers (mix tracks at the project rate). |
| Audio glue | `symphonium` / `playback-rs` | Symphonia+cpal+rubato wrappers to bootstrap fast. |
| Compositor / blend | `prism-core` + wgpu (shared) | Program-monitor render graph; 18 blend modes; linear-light float. |
| Color / grade / OCIO | `prism-color` + OpenColorIO (ASWF Rust) | Lumetri-style grade, scopes, LUTs (`.cube`), display transforms. |
| Effects / transitions | `prism-fx` (OpenFX-style, suite-shared) | Dissolves/wipes, blur/color/distort; authored once for the suite. |
| Keyframe easing | `kurbo` + `keyframe`/`bezier_easing` | Clip transform/opacity/effect keyframes with Bézier ease. |
| Text / captions | `cosmic-text` + `swash` | Titles, lower-thirds, caption rendering; OpenType + variable fonts. |
| Subtitle / transcription | `srt`/custom + `whisper`-class via `ort` | Import/export SRT/VTT; AI transcription (feature-gated). |
| Undo / project | `undo`/custom + `serde` | Command stack over the edit model; versioned `.reel` IO + relink. |
| Util | `glam`, `bytemuck`, `rayon` | Math, GPU casts, parallel decode/render. |

---

## 3. Architecture (target)

```
┌──────────────────────────────────────────────────────────┐
│  reel-gpui  (GPUI)                                       │
│  panels: project/bin · source monitor · program monitor ·│
│          timeline (V/A tracks) · effect controls · audio │
│          mixer · Lumetri/scopes · export queue           │
├──────────────────────────────────────────────────────────┤
│  edit model                                              │
│  Sequence · Track(V/A) · Clip{src,in,out,start,effects}  │
│  Transition · Keyframe · Marker · CommandStack(undo)     │
├──────────────────────────────────────────────────────────┤
│  prism-media  FFmpeg decode/encode + frame-accurate seek │
│               + audio (symphonia/cpal/rubato)  (w/ Pulse)│
│  prism-core   compositor/render graph, 18 blend modes    │
│  prism-color  Lumetri grade · OCIO · scopes · LUTs       │
│  prism-fx     transitions + effects (suite-shared)       │
├──────────────────────────────────────────────────────────┤
│  Dynamic Link: a Pulse comp / Contour artboard /         │
│  Pigment doc as a live clip, re-rendered on source edit  │
└──────────────────────────────────────────────────────────┘
```

### Core data model (target)
- **Sequence** = canvas spec (size/fps/color) + ordered **Video tracks** and **Audio tracks**; nestable
  (a sequence is a valid clip source = nested sequence). Generalize today's flat `tracks: usize` into
  typed V/A tracks with lock/sync/target/mute/solo.
- **Clip** = source ref + **source in/out** (the headline addition — decouple source range from timeline
  placement) + timeline `start` + linked A/V + an ordered **effect stack** + **opacity/transform**
  keyframes + speed/time-remap. `ClipSource` grows `Video(path)`, `Audio(path)`, `Sequence(id)`,
  `Linked(prism-doc)` (Pulse/Contour/Pigment via Dynamic Link).
- **Program frame at `t`** = topmost-visible composite of active video clips (through effects + opacity +
  transitions) in linear light, plus the summed/mixed audio of active audio clips — derived & cached.

---

## 4. Phased backlog (toward ≥85% parity)

Effort tags **S/M/L**. "shared?" = touches/promotes a `prism-*` crate → keep app-agnostic. Phase 0 is
**done** (see §1); the rest is the road to parity.

### Phase 0 — Skeleton: timeline, clips, preview  *(DONE)*
- [x] `Project` model, N tracks, clip blocks (move/trim, no ripple), playhead, scrub, snap
- [x] Transport (play/loop), timecode+frame, stills/color preview, media bin, `.reel` save

### Phase 1 — A/V engine: video decode + audio  *(the headline gap)*
- [x] **Video decode** (L, shared `prism-media`): `ClipSource::Video` — **DONE**. The new shared **`prism-media`** crate decodes via the **ffmpeg/ffprobe CLI** (`std::process::Command` — version-tolerant, no `-sys`/`pkg-config` linking; FFmpeg 8.x works; behind a small surface so libav can replace it later). At the playhead the compositor maps timeline → clip-local → source time (in/out + speed/reverse aware) → source frame index, decodes that frame (`decode_frame_at`, scaled to a ≤960px preview), runs it through the **existing per-clip effects rack + texture-upload path**, and **caches by `(path, frame-index, effects-hash)`** so scrubbing/holding a frame is free. Import detects `mp4/mov/mkv/webm/avi/m4v` (probed up front; re-probed on `.reel` load; probe is `serde(skip)`); a decode error / missing ffmpeg draws the existing placeholder (never crashes); inspector shows duration/res/fps/codec/audio. Unit-tested (model + gated decode tests). *Still open:* **frame-accurate** (vs keyframe) seek, a decode **ring buffer / prefetch**, and **timeline thumbnails**.
- [x] **Source in/out** (M): decouple source range from timeline placement (the core NLE concept) — `Clip { source_in, duration }` over a `ClipSource::source_len()` bound; reflected in the transport readout (`src in→out`) and kept consistent by every edit op. *(done)*
- [~] **Audio** (L, shared `prism-media`): decode is **DONE** (`prism_media::decode_audio` → interleaved `f32` `AudioBuffer`, whole-file, gated test). **Still open (the next `prism-media` follow-on):** `ClipSource::Audio`, **playback** (`cpal` output stream), resample (`rubato`), audio tracks, **A/V sync** to the playhead, and **waveform** display. The video slice intentionally does not block on audio playback.
- [~] **GPU program monitor** (M, shared `prism-core`): composite active video clips through the render graph (linear-light, 18 blend modes) instead of "topmost only". *(Done as a **CPU composite** first: the program monitor + the scopes' CPU program-frame sampler now **alpha-composite every visible, non-muted video track bottom-up** via `Project::effective_clips_at` — each clip's effective frame (adjustment layers folded on) drawn **over** the accumulated result honoring its **opacity** and **per-pixel alpha**, instead of drawing only the topmost clip over black. So a 21%-opacity V2 clip over a V1 clip now reads as the lower clip showing through, and an off-background Text/Title clip is transparent off its glyphs instead of an opaque black plate. The preview path composites through egui's straight-alpha painter; the sampler shares a pure, unit-tested source-over fold (`program_frame::fold_tracks` / `over_in_place` / `flatten_over_black`, premultiplied internally to avoid dark fringing). Per-clip effects / transitions / transforms / crop are preserved. **Still open:** the real GPU render graph on `prism-core` (linear-light, 18 blend modes) — this pass is a straight-sRGB CPU/painter source-over only.)*
- [x] **Real timecode** (S): drop-frame/non-drop, project-rate frames, J/K/L shuttle. *(Done: a pure, headlessly-tested `project::timecode` converts frame / seconds ↔ an SMPTE `HH:MM:SS:FF` string at the project rate in both **non-drop** (`:`) and **drop-frame** (`;`) flavours — the NTSC renumbering (29.97 skips `00`/`01` every minute but the tenth; 59.94 skips four) with an exact frame↔TC round-trip and string parse-back. A `Project.drop_frame` flag (`serde(default)` → non-drop; `.reel` back-compat) + a transport **DF** toggle (offered only at the fractional 29.97/59.94 rates) pick the flavour; the playhead readout now shows timecode + the raw frame. **J/K/L shuttle** (`app::shuttle`, a pure `1×→2×→4×→8×` speed-ladder state machine): **L** fwd / step up, **J** rev / step up, **K** pause, **K**+**L**/**K**+**J** slow-scrub; opposite direction restarts at `1×`. Coherent with the spacebar/transport buttons; live audio tracks only `1×` fwd, other rates advance the visual playhead by `rate * dt` (fwd loops, rev stops at head). 23 unit tests.)*
- [~] Tests: seek frame-accuracy *(open — current seek is input-`-ss`, keyframe-accurate)*, A/V sync drift bound *(open — no audio engine)*, composite ordering

### Phase 2 — Editing model & tools  *(the daily-driver edit surface)*
- [x] **Ripple / Roll / Slip / Slide** (L): the four trim tools; ripple-delete; close gap — pure `Project` methods (`roll`/`slip`/`slide`/`ripple_trim_in`/`ripple_trim_out`/`ripple_delete`/`close_gap_after`), wired to edge/body drags (Alt = roll/slide, Ripple toggle = ripple-trim) + Edit menu. *(done; unit-tested)*
- [x] **3-/4-point editing** (M): insert / overwrite from source to timeline; backtiming — pure `compute_three_point` + `Project::three_point_edit` (derive the 4th point; Insert ripples downstream, Overwrite in place), wired to the selected clip's source In/Out + the work-area timeline In/Out, Edit menu + `,`/`.` keys. *(done; unit-tested)*
- [x] **Razor / split** (S), **lift / extract** (S) — `split_clip`/`razor_all_at` (source-continuous), `lift` (keep gap), `ripple_delete` (extract). Copy/paste/duplicate clips still TODO.
- [~] **Track controls** (M): add/delete V/A tracks, lock, sync-lock, target, mute/solo, height; link/unlink A/V; group clips. *(Done: per-track controls live in `project::track` (`Track { name, enabled, locked, solo, target, height, collapsed }`, one entry per track kept in sync by `sync_track_meta`). **Add / delete** tracks (delete drops the track's clips + their transitions and shifts higher tracks down, refusing the last track); **lock** rejects clip edits (move/trim/razor/delete/markers); **mute-as-hide** (the "eye" — a disabled track is hidden from the program) + **solo** (solo isolates the visible set, enable gates first); **target** routes new clips / pastes to the chosen track (exclusive, top-track fallback, survives add, falls back on remove); **height** is per-track (clamped expanded height + a collapsed compact skim row) with variable timeline lane drawing. Track-header UI: name + eye / lock / solo / target / collapse toggles + a height drag handle + add/remove buttons. Every mutation routes through the labeled undo system ("Add Track" / "Remove Track" / "Lock Track" / "Solo Track" / "Target Track" / "Resize Track") and round-trips through `.reel` with `serde(default)` back-compat; unit-tested. **Still open:** true audio **mute** + **sync-lock** + **link/unlink A/V** (await the audio engine / typed V/A tracks); clip **grouping** beyond the existing multi-select group edits.)*
- [x] **Markers** (S): clip + sequence markers, color/comment, marker navigation; work-area / in-out range. *(Done: **sequence markers** (`Project.markers`) + **work-area in/out** (`in_point`/`out_point` with in≤out clamp, a shaded timeline band + bracket handles) shipped first; this pass adds **clip markers** (`Clip.markers`, clip-local times that travel with the clip and split across a razor) and a **color/comment editor** in the inspector (7-swatch `Marker::PALETTE`). `M` is DWIM — a clip marker on the selected clip under the playhead, else a sequence marker; `⇧M` removes the nearest; `↑`/`↓` navigate the merged sequence + selected-clip set; `I`/`O` mark in/out. Every mutation routes through the labeled undo system ("Add Marker" / "Add Clip Marker" / "Edit Marker" / "Mark In" …). `serde(default)` back-compat; unit-tested.)*
- [x] **Selection & snapping polish** (M): multi-select, marquee, snap to edges/markers/playhead, magnet toggle. *(Done: Snapping (`Project::snap_candidates`/`snap_time`) snaps a dragged clip/edge to clip edges / playhead / origin / markers / work-area in-out, gated by a **Snap** magnet toggle (`S`, on by default) with a guide line. **Multi-select + marquee** (pure `project::select`): a `Selection` (ordered set + primary), `clips_in_rect` band query, and group `move_clips`/`lift_clips`/`ripple_delete_clips`; Ctrl/Cmd/Shift-click toggles, empty-lane drag marquees, group body-drag moves the set, `Cmd+A`/`Esc` select-all/clear. All unit-tested.)*
- [x] **Undo/redo** (M): a snapshot-based history over the edit model. *(Done: a
  pure, generic, **bounded** `History<T>` (`app::history`) — a pre-edit undo
  stack + a redo stack; `record` pushes a pre-edit snapshot and clears redo,
  `undo`/`redo` swap the live state with a stored snapshot, depth bounded to 100
  (oldest dropped). `ReelApp` holds a `History<String>` of serialized-`Document`
  snapshots: each frame baselines the pre-edit document and, at frame end,
  commits a checkpoint iff the document's serialized state changed — coalescing a
  whole pointer drag (clip move / trim / keyframe drag) into one step and
  ignoring transport-only changes (the playhead never records). **`Cmd/Ctrl+Z`**
  / **`Cmd/Ctrl+Shift+Z`** (+ `Cmd/Ctrl+Y`) + an Edit-menu Undo/Redo pair;
  New/Open reset the history. Restoring re-probes video metadata and clamps the
  transport/selection. **Both originally-documented gaps now closed:**
  **(1) per-keystroke text edits coalesce** — an in-field text-editing session
  (title / caption text, a numeric param typed as text) is held against its
  pre-edit baseline while the field keeps focus and committed as **one** step
  when focus leaves (or moves to a different field), exactly like a pointer drag;
  switching fields makes two distinct steps. **(2) named-command labels** — the
  Edit-menu Undo/Redo items (and tooltips) name the action ("Undo Split", "Undo
  Trim", "Undo Move", "Undo Delete", "Undo Effect Change", "Undo Rename", "Redo
  …"); `History<T>` carries a short per-entry label that travels across
  undo↔redo, derived from the command / drag / inspector edit that produced the
  checkpoint. 20 unit tests (the pure stack discipline + five real `Document`
  round-trip tests: split/move/delete/effect-change undo→exact prior state, redo
  re-applies, a new edit clears redo, depth bounded; **plus** command-label
  exposure/travel and the text-edit coalescing flow — one field → one step, two
  fields → two steps).)*
- [x] **Nested sequences / subsequences** (M): a sequence as a clip. *(Done: a
  multi-sequence `Document` (id-keyed `Vec<Project>` sequences, one active) with
  serde back-compat that loads a legacy single-sequence `.reel` as the one
  sequence (`Document::from_json` + `normalize`); a `ClipSource::Sequence(id)`
  whose duration = the referenced sequence's `content_len`, rendered
  **recursively** through the existing preview/compose path at the mapped local
  time (`project::nest`, threaded `RenderCtx`); a **cycle guard** (visited-set)
  so A↔B / self-nest renders nothing and never stack-overflows; a **Nest
  selection** command (`Document::nest_clips`) that wraps the selected clips into
  a new sequence and replaces them with one nested clip; and a bin **Sequences
  switcher** (open/edit, add, place-as-nested-clip) + a **File ▸ Open .reel…**
  load path. Unit-tested (20 tests). Gaps: nested **audio submix** (no audio
  engine yet — Phase 1/5), **open-nest-in-place** / double-click to dive in (the
  switcher opens a nest as a top-level edit, not in-context), per-nest
  **opacity/rotation** on the recursive frame (transform position/scale + crop
  apply; rotation/opacity on a nest are approximate), and re-targeting an
  existing nest from the inspector.)*
- [x] Tests: ripple math (downstream shift), split/source-continuity, roll length-preservation, slip/slide clamping — 28 unit tests in `project.rs` (incl. `.reel` JSON + legacy round-trip); **undo round-trip done** (split/move/delete/effect-change undo→exact prior document, redo re-applies — see `app::history`). *(insert/overwrite still TODO)*

### Phase 3 — Transitions, effects, speed  *(make cuts cinematic)*
- [~] **Transitions** (M, shared `prism-fx`): video (cross-dissolve, dip-to-black/white, wipes, push, slide, zoom) + **audio crossfades**; duration/alignment handles; default transition. *(cross-dissolve + dip-to-color + directional **wipe** and **push** (4 directions each) computed in the preview compositor — `Transition`/`TransitionKind`/`WipeDir`, a pure `Transition::geometry` (Blend / Wipe reveal-rect / Push offsets) the compositor renders, cut-centered with clamp-to-neighbour, `.reel`-persisted, unit-tested; slide/zoom + audio crossfades + drag handles still TODO)*
- [~] **Effect stack** (M): per-clip ordered effects; **Effect Controls** panel; params are keyframable (`Property<T>` + Bézier ease). *(Done: a per-clip **color-grade effects rack** (pure `project::effect`) — each `Clip` carries an ordered `Vec<ClipEffect>` (a shared `prism_core::Adjustment` + an `enabled` flag), 12 adjustment kinds (Brightness/Contrast, Levels, Exposure, Hue/Saturation, Vibrance, Invert, Threshold, Black&White, Posterize, Photo Filter, Gradient Map, Curves) applied **CPU-side** in straight-sRGB via `apply_stack_rgba8`/`apply_stack_color` (Curves/Gradient-Map LUTs via `prism_core::curve`), the processed frame cached by `(source, effects-params hash)` so it reprocesses only on change; an inspector **Effects** section (add / remove / clear / reorder / enable + per-kind params); `.reel`-persisted with `serde(default)` back-compat; unit-tested. **Distinct from adjustment LAYER clips.** **Keyframable effect params done**: each scalar param (`FxParam`) reuses the transform/opacity keyframe infra (`Channel` + linear/hold/Bézier `Ease`) via `ClipEffect.params` — sampled at the clip-local playhead before the rack is applied (`effects_sampled`), the processed-frame cache re-keyed by the **sampled** params hash so animated grades update per frame (static racks stay cached); inspector stopwatch / diamond / interp per scalar param; `serde(default)` back-compat; unit-tested. **Keyframe-lane / curve editor done** (closing the deferred follow-on): a `curve_editor` panel under the inspector graphs the selected clip's animated parameters over its local time — a generic `project::param::ParamRef` addresses *every* animatable scalar (transform position/scale/rotation, opacity, **and** the keyframable effect-rack scalars) uniformly. It stacks a **dope-sheet lane list** (one row per animated param, keyframes as draggable diamonds — drag to retime) over a **value-over-time curve view** for the selected param: keyframes are draggable points (drag = retime + revalue), an eased segment shows draggable Bézier ease handles (drag = reshape easing, promoting a linear/hold segment to an editable ease), **double-click** the plot adds a key on the curve, **Delete** removes the selected key; the playhead is a guide in both views and clicking empty plot space scrubs it. Edits mutate the real `Channel` keyframes (kept sorted via `Channel::move_key`), so the preview's sampled values update immediately. Pure helpers (`Channel::move_key`/`value_bounds`, `Ease::with_handle1`/`with_handle2`, the screen-x↔clip-local-time / y↔value maps, `ParamRef`) are unit-tested. Still TODO: **keyframable color & curve params** (RGB triplets + Curves knots are not yet animatable, so the curve view only graphs scalars), **multi-param simultaneous editing** (the lane list shows every animated param but the curve view edits one at a time), and a **full per-channel Curves UI** (the model already carries per-channel knots; the inspector edits a single midpoint).)*
- [ ] **Built-in effects** (M): transform (position/scale/rotation/anchor), crop, opacity, blend modes (`prism-core`), drop shadow, blur/sharpen, mask (Bézier, track-able)
- [~] **Speed / duration** (M): clip speed %, reverse, **time remapping** (speed keyframes), freeze-frame, frame-blend/optical-flow (later). *(per-clip constant speed + reverse done: `Clip.speed` decouples timeline duration from source consumption — `source_consumed`/`source_out`/`source_time` are speed-aware, `set_speed` preserves the source window and recomputes duration (Premiere Speed/Duration), split/trim/roll/slide stay source-continuous under speed; inspector Speed section + Edit menu presets + `R` reverse + timeline speed pill; unit-tested. Time-remap keyframes, freeze-frame, frame-blend still TODO.)*
- [x] **Adjustment layers** (S): an effect-carrying clip affecting tracks beneath — a `ClipSource::Adjustment` (media-less, with an enable flag) whose transform/opacity/crop fold onto the topmost content clip below it via pure `Clip::with_adjustment` + `Project::program_at`/`effective_clip_at` (sampled at the playhead so keyframed layers animate; stacks compose); wired into the preview compositor, a Title-menu / bin add + inspector section. *(done; unit-tested)*
- [ ] Tests: transition blend at the cut, time-remap sampling, effect re-order

### Phase 4 — Color: Lumetri-grade + scopes  *(shared `prism-color`)*
- [x] **Basic correction** (M): white balance, exposure/contrast/highlights/shadows/whites/blacks, saturation. *(Done: a per-clip **Basic Correction** primary grade (`project::effect::BasicCorrection`) — WB temperature/tint, exposure (stops), contrast (about mid-grey), highlights/shadows/whites/blacks (smooth per-range weighted pushes), and saturation (lerp around Rec.601 luma; 0 = grey, >1 boosts) in a fixed documented order, in the rack's straight-sRGB space, clamped NaN-safe, identity = a true no-op. Integrated **app-locally** via an additive `basic: Option<BasicCorrection>` on `ClipEffect` (`#[serde(default)]`), flowing through the same `apply_stack_rgba8`/`apply_stack_color`/`effects_hash` paths so both the preview and the scope sampler reflect it; ordered/reorderable/enable-toggle, cached. Effect Controls gets an "Add effect ▸ Basic Correction" entry + the nine control rows + reset. `.reel` round-trips unchanged (legacy racks load `basic: None`). Static (not keyframable) this pass. 10 unit tests.)*
- [~] **Curves** (M): RGB + per-channel, **hue-vs-hue/sat/luma** curves *(Done: **RGB master + per-channel R/G/B** curves as a clip-rack grade — an interactive `inspector` canvas (channel selector + drag/double-click-add/right-click-remove knots) over the existing `Adjustment::Curves` monotone-cubic LUT apply; pure knot helpers (`curve_insert_knot`/`curve_move_knot`/`curve_delete_knot` + `CurveChannel`) are unit-tested and the only mutation path. Still TODO: **hue-vs-hue / hue-vs-sat / hue-vs-luma** curves.)*
- [x] **Color wheels** (M): lift/gamma/gain (shadows/mids/highlights), 3-way. *(Done: a per-clip **3-way color corrector** (`project::effect::ColorWheels`) — per-channel RGB lift (shadow offset) / gamma (midtone power) / gain (highlight multiplier) plus a master per range, `out = ((in + lift*(1-in))^(1/gamma)) * gain` in the rack's straight-sRGB space, identity = a true no-op, clamp/floor-safe against NaN. Integrated **app-locally** into the existing `ClipEffect` rack via an additive `wheels: Option<ColorWheels>` (`#[serde(default)]`), so it flows through the same `apply_stack_rgba8`/`apply_stack_color`/`effects_hash` paths as every other effect — ordered/reorderable/enable-toggle, cached, and rendered identically by the preview compositor AND the scopes' CPU program-frame sampler. Effect Controls gets an "Add effect ▸ Color Wheels (3-way)" entry + Lift/Gamma/Gain rows (per-channel + master) + reset. `.reel` round-trips unchanged (legacy racks load `wheels: None`). Static (not keyframable) this pass. 9 unit tests. **Still open:** keyframable wheel params, and a graphical wheel/trackball widget (numeric controls only).)*
- [x] **HSL secondaries** (M): key a color range, refine, grade it. *(Done: a per-clip **HSL Secondary** corrector (`project::effect::HslSecondary`) — a pure **qualifier** keys the image by **Hue** (circular band, center ± width with a feathered shoulder) ∧ **Saturation** ∧ **Luma** (trapezoidal `lo..hi` + soft shoulders), the three memberships multiplying into a per-pixel `q ∈ 0..1`; the **grade** (hue rotate + saturation in HSL, then gain + exposure in straight-sRGB) is blended toward the original by `q`, so only the keyed region is graded with smooth soft edges. All in the rack's documented **straight-sRGB 0..1** space, clamped + NaN-safe; **identity** = full-range key + identity grade is a true no-op (and an identity grade is a no-op at every `q`). Integrated **app-locally** via an additive `hsl: Option<HslSecondary>` on `ClipEffect` (`#[serde(default)]`), flowing through the same `apply_stack_rgba8`/`apply_stack_color`/`effects_hash` paths the preview + scope sampler use; ordered/reorderable/enable-toggle, cached. Effect Controls gets "Add effect ▸ HSL Secondary" + a Key group + a Grade group + reset. `.reel` round-trips unchanged (legacy racks load `hsl: None`). Static (not keyframable) this pass. 11 unit tests. **Still open:** keyframable secondary params, a graphical qualifier picker + key-matte overlay, and multiple keys per clip.)*
- [x] **Scopes** (M): waveform, vectorscope, histogram, RGB parade. *(Done: a workspace **Scopes** panel (`app::scopes_panel`, toggleable right-side panel — Window menu + `Cmd/Ctrl+5`) with a Premiere-style selector (Histogram / Waveform / RGB Parade / Vectorscope) that analyzes the composited **program frame at the playhead**. The scope math is **pure + unit-tested headlessly** (`scopes`): `Histogram` (luma + per-channel R/G/B, 256 bins), `Waveform` (brightness vs. image column, column-binned + capped), `Parade` (three column-aligned R/G/B waveforms), `Vectorscope` (BT.601 U/V chroma on a 128×128 disc + the six 75%-bar color targets). The program frame is produced by a CPU sampler (`program_frame`) since the preview composites only via egui's painter: it resolves `effective_clip_at`, samples the clip's pixels (color/still/sequence/video), applies the same pure grade rack, honors crop + opacity over a black backdrop, and aspect-fits at a 256-px scope resolution (cached per `(path, frame-index, effects-hash, size)`). 27 new tests (bin counts on known frames, waveform column mapping, parade channel separation, a known primary at the right vectorscope angle, the CPU sampler reproducing + grading a frame). **Still open:** scoping geometric transforms / transitions / nested sequences (the sampler analyzes graded color content only this pass), plus a full GPU program monitor.)*
- [~] **LUTs** (S): input + creative `.cube`/`.3dl`; export LUT *(Done: **`.cube` import + apply** — a pure `.cube` parser (1D + 3D LUTs; `LUT_1D_SIZE`/`LUT_3D_SIZE`, `DOMAIN_MIN`/`MAX`, `TITLE`, comment/whitespace tolerant, malformed → `Err`, never panics) → an in-memory `project::effect::CubeLut`, plus a pure apply (trilinear 3D / linear 1D, per-channel domain remap, intensity/mix) in the rack's straight-sRGB 0..1 space. Integrated as an additive `Option<CubeLut>` on `ClipEffect` (`#[serde(default)]`, table serialized with the project so `.reel` round-trips; legacy files load unchanged), flowing through the same `apply_stack_color`/`apply_stack_rgba8`/`effects_hash` paths the preview + scope sampler use; "Add effect ▸ LUT (.cube)…" `rfd` picker with intensity slider, Replace, and Reset; 21 unit tests. **Still open:** `.3dl` import and **LUT export**.)*
- [ ] **Color match / auto** (M): match-shot; auto-balance
- [ ] **OCIO / log** (M, shared): log→linear input transforms, display/output transforms, scene-linear
- [ ] Tests: scope accuracy, LUT round-trip, grade determinism

### Phase 5 — Audio: mixer, levels, repair  *(broadcast-usable sound)*
- [~] **Track mixer** (M): per-track + master, faders, pan, mute/solo, meters (peak/RMS/LUFS). *(Done: a workspace **Audio Mixer** panel (`app::mixer`, toggleable right-side panel — Window menu + `Cmd/Ctrl+4`) with one **channel strip per track** (volume fader + pan slider + mute + solo + peak/RMS **level meter** + rubber-band gain keyframe / reset) plus a **master** strip. Per-track `gain`/`pan` + a `Master` (gain+pan) fold into the **pure mixer**: clip gain × track fader, a **constant-power pan law** (`pan_gains` — center equal at −3 dB, hard L/R, power conserved) per L/R for stereo, then the master folded over the summed mix (`apply_master`). Metering is pure (`meter_level` = peak + windowed RMS, `amplitude_db` dBFS) fed the live mix at the playhead (per-track in isolation + master). Every edit is a `MixerAction` routed through the **labeled undo** ("Track Volume" / "Track Pan" / "Mute Track" / "Solo Track" / "Master Volume" / "Master Pan" / …); track gain/pan/`gain_anim` + `Master` round-trip through `.reel` with serde-default back-compat. Unit-tested (pan law, gain fold, meter, round-trip, labeled undo). **Still open:** **LUFS** loudness metering (peak/RMS only today) and audio sub-mix buses.)*
- [~] **Clip & track volume keyframes** (S): rubber-band gain; audio gain/normalize. *(Done: **rubber-band gain keyframes** for both **track faders** (keyed in sequence time, pinned from the mixer strip) and **clip volume** (keyed in clip-local time, with a stopwatch + diamond in the inspector **Audio** section), reusing the existing keyframe `Channel` / linear-hold-Bézier `Ease` infra. Both are sampled **per output frame** by the pure mixer (`MixClip::gain_at` → `Track::sampled_gain`/`Project::track_gain_at` and `AudioClip::sampled_gain`), so a ramp / dip changes the mix over time; `.reel`-persisted, unit-tested (a keyframe sampled per output time changes the mix; clip-local vs sequence-time sampling). **Still open:** audio **normalize** (target-level gain) and a dedicated audio rubber-band overlay on the timeline.)*
- [~] **Audio effects** (M, shared `prism-fx`): EQ, compressor/limiter, de-noise, de-reverb, de-ess, gain. *(Done: a per-clip **audio effect chain** (`project::audio_fx`) of pure, deterministic, headlessly-tested DSP processors applied in the mixer path **before** the gain/pan fold, mirroring the color-grade effects rack. **Gain / Normalize** (fixed dB + peak-normalize to a target dBFS), a **parametric EQ** (a cascade of stateful per-channel **RBJ-cookbook biquads** — low shelf + peak + high shelf, plus low/high-pass), and a **compressor / limiter** (peak envelope-follower → threshold/ratio/attack/release/make-up; a limiter is a high-ratio compressor). The chain is applied once to a clip's whole decoded/resampled buffer and cached by `(path, rate, chain-hash)`, so the pure mixer reads the processed samples 1:1; `AudioClip.fx` round-trips through `.reel` with serde-default back-compat. Inspector **Audio Effects** rack (add/edit/remove/reorder/enable, per-effect params) routed through labeled undo. 23 unit tests. **Still open:** **de-noise / de-reverb / de-ess** (spectral repair — deferred), per-*track* audio FX (per-clip this pass), and keyframable audio-effect params.)*
- [x] **Ducking** (S): auto-duck music under dialogue. *(Done: a pure, headlessly-tested **auto-ducker** (`project::duck`) — `duck_channel(envelope, t0, dt, params)` turns a **key** (dialogue) track's energy envelope into a gain-keyframe `Channel` of linear multipliers for a **target** (music) track: the music ducks toward a **duck amount** (dB) while the key energy exceeds a **threshold** and restores to 0 dB during silence, with **attack** / **release** linear ramps and a **hold** that extends each speech run so brief inter-word gaps don't pump (silence / empty envelope / a no-op duck amount → no automation). The key track's envelope is derived from the **existing decoded-buffer + pure-mixer path** (`energy_envelope` reduces the peak of a short `mix_window_owned` per envelope sample — no new decode path). An **Auto-Duck…** action (Audio menu + the mixer panel) opens a dialog to pick the key / target tracks + params, runs the ducker, and writes the curve onto the target's `gain_anim` through the **labeled-undo** system (label "Auto-Duck"), reusing the rubber-band track-gain plumbing the mixer samples per output frame. 14 unit tests.)*
- [ ] **Sync** (M): merge A/V by waveform or timecode; multicam audio
- [ ] Tests: mix sum correctness, loudness metering, sync alignment

### Phase 6 — Titles, graphics, captions  *(text on screen)*
- [~] **Titles / graphics** (M): text tool, lower-thirds, shapes; transform/style; motion presets; **Essential-Graphics-style templates** (parameterized). *(Title clips exist with text/size/color/align/background and now **font-family selection**: an additive `Title.font_family: Option<String>` (`#[serde(default)]` → `None` = egui's proportional default; round-trips `.reel`) chosen from a **Font** dropdown in Effect Controls, populated by enumerating installed families with the pure-Rust **`fontdb`** crate (`fonts::families`, sorted + deduped). On selection the preview reads that family's bytes on demand (cached; egui is never bulk-loaded) and registers them into the egui ctx under a named family once (`fonts::ensure_registered`), laying the galley out with `FontId::new(size, FontFamily::Name(...))`; absent/unknown/unloadable families fall back to proportional via the pure `fonts::resolve_family` seam (unit-tested headlessly). **Title transform fixed:** the title now **honors its clip Transform Position X/Y** correctly — the double-counted alignment (layout `halign` *and* a manual top-left subtraction) is gone; a single pure `preview::title_top_left` anchors the left-laid-out galley by alignment then applies the Position offset, so a centered/right-aligned title sits where it should and the numeric Position controls translate it predictably (unit-tested). **Move clips on-canvas:** the selected clip (title or any source) can be **dragged directly in the program monitor** to reposition it, the screen delta mapped 1:1 into Position units (pure `preview::screen_delta_to_position`, the inverse of the compositor offset) and committed keyframe-aware via `Clip::nudge_position`, as one labeled "Move Clip" undo step (unit-tested). **Export-side glyph rasterization done:** the CPU program-frame sampler (the scopes' analysis path + the foundation for the export engine) now rasterizes a title's **real glyph outlines** in the chosen family — a pure `title_raster` module lays the string into glyph contours from a resolved TrueType face (`ttf-parser`; family via `fonts::resolve_face_for_raster`, falling back to the system default sans for `None`/unknown) and fills them with an antialiased **even-odd** polygon fill (holes carved), framed to match the preview (size = fraction of comp height, per-`align`, vertically centered); the background box now hugs the text block, and both colors grade through the effect rack. So the scopes (and a future export) see the actual text, not just the background box. Unit-tested headlessly (font-dependent tests skip on a font-less box). *Fidelity gaps:* the clip transform isn't applied on this CPU path and long lines aren't wrapped (the egui preview does both). Shapes, motion presets, and Essential-Graphics templates still TODO.)*
- [~] **Captions / subtitles** (M): create/import/export **SRT/VTT**, style, burn-in or sidecar; caption track. *(Done: a sequence-level `Captions` track (pure `project::caption`) of `[start,end)` `Caption` cues + a shared `CaptionStyle` (size/color/box/position); `add_caption`/`remove_caption_at`/`caption_at`/`active_at`, SRT & VTT serialize and a tolerant one-parser `load_str`, burned into the preview at the playhead, a Captions menu (add ⇧C / nav / import / export), an inspector cue+style editor, and a timeline cue strip; unit-tested. Per-cue positioning overrides + styled inline tags still TODO.)*
- [ ] **Dynamic Link graphics** (M, suite): place a **Pulse comp** as a live clip; place Contour artboards / Pigment docs; re-render on source edit
- [ ] Tests: caption timing round-trip, title layout, linked-clip re-render

### Phase 7 — Export & render  *(deliver everything)*
- [~] **Export engine** (L, shared `prism-media`): render the sequence → encode; formats H.264/H.265/**ProRes**/VP9/AV1, DNxHR; container MP4/MOV/MKV; image sequences (PNG/EXR/DPX/TIFF); audio (WAV/AAC); GIF/APNG/WebP *(Done: **image-sequence + still export to PNG** — a pure full-resolution CPU program renderer (`export::render_program`) composites every visible track bottom-up at the comp's native size for a time `t`, reusing the scope sampler's `fold_tracks`/`flatten_over_black` compositing + per-clip content sampling (color/image/sequence/video/title-via-glyph-raster), with grades/opacity/crop, and — the key upgrade over the scope sampler — **honoring the per-clip Transform (position/scale/rotation)** via a pure inverse-warp (`warp_layer`) that mirrors the preview compositor, so the export matches the program monitor. **Export Frame** (playhead → one PNG) and **Export Image Sequence** (work-area or whole-sequence range → numbered `name_000123.png`) via `rfd`, written through `prism_io::export::save_rgba8`; pure frame-range/filename helpers + the renderer are unit-tested without disk. **H.264 MP4 video export done** (Export slice 2): the *same* per-frame renderer feeds a lazy `rgba` frame iterator to the suite-shared, app-agnostic encoder **`prism_media::encode_h264`** (additive, co-owned with Pulse) which pipes frames to ffmpeg over stdin → `libx264`/`yuv420p` MP4; the **ffmpeg arg construction is a pure unit-tested fn** (`encode_h264_args`), the actual encode is **gated on `prism_media::ffmpeg_available()`** (a missing binary / empty range shows an `rfd` message dialog, never panics), wired via **File ▸ Export video (MP4)…** as an acknowledged **blocking render** (background queue = follow-up). `export::write_video` + the pure `video_frame_plan` (rejects a zero-length range) + `ensure_mp4_extension` are unit-tested; a gated end-to-end render→encode→probe runs only when ffmpeg is present. **Audio mux done** (Export slice 3): the **program audio mix** over the export range is rendered to interleaved `f32` PCM (48 kHz stereo) via the *same pure mixer* playback uses (`project::mix_window_owned` + `apply_master`, fed from the decode-once audio cache) and muxed into the MP4 as **AAC** — exported videos now have sound. The shared encoder gains an **additive** audio path **`prism_media::encode_h264_with_audio`** (no signature changes; `encode_h264` stays silent) that writes the mix to a temp WAV (dependency-free `WAVE_FORMAT_IEEE_FLOAT` serializer) and feeds it as ffmpeg's second input; its **arg construction is a pure unit-tested fn** (`encode_h264_args_with_audio` → `-i audio.wav -map 0:v:0 -map 1:a:0 -c:a aac -b:a 192k -shortest`). Reel's `export::render_program_audio` (pure, unit-tested: `samples ≈ duration × rate × channels`) + `export::write_video_with_audio` wire it; **Export video** muxes audio when the sequence has decodable, non-silent audio and **falls back to the silent encode** otherwise. A gated render→encode→mux→probe asserts the export carries an audio stream. **Fidelity gaps (shared with the CPU path):** transitions, nested sequences, and title rotation aren't applied; compositing is straight sRGB not linear light. **Still open:** other codecs/containers (H.265/ProRes/VP9/AV1, MOV/MKV), EXR/DPX/TIFF/audio-only (WAV/AAC) /GIF outputs, LUFS-normalized export loudness, render **presets** (the **background render queue is now done** — File ▸ Export video enqueues a non-blocking worker-thread render with a live progress panel; see "Render queue / presets" below), and CRF/preset controls (fixed at CRF 18 / `medium`).)*
- [~] **Render queue / presets** (M): Media-Encoder-style queue, social/web/broadcast presets, range/in-out export, bitrate/VBR/CBR, hardware encode (NVENC/QuickSync/VideoToolbox) *(Done: **background render queue** — video export (File ▸ Export video) is now **non-blocking**: it enqueues a job that renders + encodes on a `std::thread` worker while the UI stays interactive. New `render_queue` module — a **pure, thread-free job/queue/progress state machine** (`JobSpec` thread-safe snapshot taken at enqueue: cloned `Project` + rendered `AudioMix` + `FramePlan` + path; `JobStatus` Queued→Rendering{done,total}→Done|Failed with clamped `fraction()`; `RenderQueue` with monotonic never-reused `JobId`s, FIFO `next_pending`, `apply_update` that ignores late ticks on terminal jobs, `is_rendering`/`is_idle` enforcing **one-job-at-a-time**, and a per-state `summary()`), **all unit-tested without threads/ffmpeg**. The worker bridge `run_job` streams a `JobUpdate` per frame over an `mpsc` channel and returns the terminal status; the progress-streaming encode `export::with_progress_encode` (queue twin of `write_video`, same lazy per-frame render, fires `on_frame` per frame) keeps ffmpeg **gated** (missing binary → a `Failed` queue entry, never a panic). A **render-queue panel** (File ▸ Render queue, auto-opened on enqueue) shows live per-job progress bars / status + Clear finished. **Still open:** social/web/broadcast presets, bitrate/VBR/CBR controls, hardware encode (NVENC/QuickSync/VideoToolbox), parallel/priority jobs, persisted queue.)*
- [ ] **Smart render / preview files** (M): render-and-replace timeline previews; reuse on export when codecs match
- [ ] **Color-managed delivery** (S, shared): embed/convert color, range (full/limited), HDR (PQ/HLG)
- [ ] Tests: encode round-trip, A/V mux sync, preset fidelity

### Phase 8 — Pro workflows: multicam, proxies, project mgmt
- [ ] **Multicam** (L): sync N angles, multicam source sequence, live angle switching, flatten
- [ ] **Proxies / optimized media** (M): attach/toggle proxies, background proxy generation, full-res on export
- [ ] **Project management** (M): relink/consolidate/transfer (collect files), media offline/online, multiple sequences, bins/labels/search
- [ ] **Interchange** (M): EDL/XML/AAF in-out (round-trip with other NLEs), OpenTimelineIO
- [ ] Tests: multicam sync, proxy↔full swap, project relink

### Phase 9 — AI tools  *(feature-gated, shared `pigment-ai`/`ort`)*
Models fetched on demand behind a feature flag; graceful no-model path.
- [ ] **Scene Edit Detection** (M): detect cuts in a flattened file → split clips
- [ ] **Auto Reframe** (M): track subject, reframe to target aspect with position keyframes
- [ ] **Object masking / tracking** (M): SAM2/3 → tracked mask for effects/grade (video roto, shared with Pulse)
- [ ] **Text-based editing** (L): transcribe audio (Whisper-class via `ort`), edit video by editing the transcript; **silence removal**, filler-word cut
- [ ] **Generative Extend** (L, optional + pluggable): extend a clip's tail with generated frames/audio — **local model or BYO cloud key**, never required
- [ ] **Audio enhance** (S): AI speech enhance / de-noise

### Phase 10 — Reliability, performance & ease-of-use
- [ ] **Playback performance** (L): hardware decode (VideoToolbox/NVDEC/QuickSync), background decode threads, frame/decode cache, dropped-frame indicator, playback resolution (full/½/¼)
- [ ] **Background render & cache** (M): render preview files in the background; persistent media/decode cache
- [ ] **Autosave + crash recovery** (M); versioned `.reel` project, relink-missing-media, auto-save versions
- [ ] **Prefs / shortcuts / workspaces** (M): full remappable map (Premiere muscle-memory + J/K/L), dockable panels, saved workspaces, command palette
- [ ] **Editing ergonomics** (M): trim monitor, dynamic trimming, gang/sync, replace edit, match frame, add-edit-to-all-tracks, keyboard-driven editing
- [ ] Tests: cache correctness, dropped-frame gate on long timelines, autosave/relink round-trip

---

## 4b. Parity coverage matrix (vs Premiere Pro surface)

| Category | Premiere surface | Status | Phase |
|---|---|---|---|
| Multitrack timeline / clips | tracks, move, trim | **Done** basic (no ripple) | 0,2 |
| Video decode / playback | frame-accurate, all codecs | **Planned** (`prism-media`) | 1 |
| Source/program monitors + in/out | full | **Partial** (program-only; source in/out **done** in the model) | 1 |
| Audio decode / playback / sync | full | **Planned** (symphonia/cpal/rubato) | 1,5 |
| Ripple/roll/slip/slide + 3/4-pt edit | full | **Done** (4 trim tools + razor/lift/ripple-delete); 3/4-pt **Planned** | 2 |
| Markers / nesting / track controls | full | **Done** markers (sequence + clip, color/comment editor, navigation, work-area in/out); track controls (add/delete, lock, mute-as-hide, solo, **target + height** done) + **nested sequences + undo done** | 2 |
| Undo/redo | full | **Done** (bounded snapshot history; ⌘Z / ⌘⇧Z + Edit menu) | 2 |
| Transitions (video + audio) | 90+ | **Partial** (cross-dissolve + dip-to-color + directional wipe/push in the preview; slide/zoom/audio planned) | 3 |
| Effects + keyframes + masks | full | **Partial** (per-clip color-grade effects rack — 12 adjustment kinds, reorder/enable, CPU-applied + cached; transform/opacity keyframes done; keyframable *effect* scalar params **done** (sampled per frame, cache re-keyed); **keyframe-lane / curve editor done** (dope-sheet + draggable value curve with Bézier ease handles, add/delete/retime/revalue); keyframable color/curve params + multi-param curve view + masks planned) | 3 |
| Speed / time-remap / reverse | full | **Partial** (constant clip speed + reverse done; time-remap planned) | 3 |
| Lumetri color + scopes + LUTs | full | **Partial** (scopes done; **color wheels — 3-way lift/gamma/gain — done**; **basic correction — WB + tone + saturation — done**; **LUTs (.cube) — done**; **HSL secondaries — key Hue/Sat/Luma + grade — done**; **curves — RGB master + per-channel R/G/B, interactive knot canvas — done** (hue-vs curves planned), `prism-color`) | 4 |
| Audio mixer / effects / loudness | full | **Partial** (mixer + per-clip/track volume done; audio effects — gain/normalize + parametric EQ + compressor/limiter — done; auto-duck done; LUFS / de-noise planned) | 5 |
| Titles / Essential Graphics | full | **Planned** | 6 |
| Captions / subtitles | full | **Done** (SRT/VTT track + burn-in + import/export) | 6 |
| Export / Media Encoder / presets | full | **Partial** (PNG still + image sequence + **H.264 MP4 video with muxed AAC audio** via `prism-media`/ffmpeg, now via a **non-blocking background render queue** with a live progress panel; other codecs / presets planned) | 7 |
| Multicam / proxies / project mgmt | full | **Planned** | 8 |
| Interchange (XML/AAF/EDL/OTIO) | full | **Planned** | 8 |
| AI (scene-detect/reframe/text-edit/generative) | Sensei/Firefly | **Planned** (feature-gated) | 9 |
| Perf (HW decode/cache) / autosave / prefs / workspaces | full | **Planned** | 10 |
| Dynamic Link (Pulse comps) | live | **Planned** (suite) | 6 |
| Motion graphics / advanced VFX | — | **Won't** (Pulse; place comps) | — |
| Raster paint / vector authoring | — | **Won't** (Pigment / Contour) | — |

---

## 5. Milestones

| Milestone | Phases | Capability | Approx parity |
|---|---|---|---|
| **Scaffold** | 0 | Multitrack timeline, clip move/trim, stills preview, save | ~8% |
| **Real NLE** | 1–2 | + video decode, audio + sync, source in/out, ripple/roll/slip/slide, undo | ~40% *(partial — **here today** (~17%): video decode + source in/out + ripple/roll/slip/slide + nested sequences + **undo/redo** land; audio playback/sync still out)* |
| **Cinematic** | 3–4 | + transitions, effects/keyframes, speed, Lumetri color + scopes | ~65% |
| **Deliverable** | 5–7 | + audio mixer/effects, titles/captions, export/render queue | **~85%** |
| **Parity+** | 8–10 | + multicam/proxies/project-mgmt, AI tools, HW-decode/cache, autosave/prefs/workspaces | **≥90%** |

**The ≥85% line lands at the end of Phase 7.** Highest felt-parity-per-effort first: **the Phase-1 A/V
engine gates everything** (video decode + audio + source in/out) — without it Reel isn't a video editor.
Then editing tools (Ph2), then transitions/color/audio/export.

---

## 6. Hard problems (mitigations)

1. **No video/audio today** → Phase 1 is the make-or-break: `prism-media` (FFmpeg) frame-accurate seek + a decode cache feeding the existing texture path; `symphonia`/`cpal`/`rubato` for audio with playhead-locked A/V sync.
2. **Playback performance** → hardware decode (VideoToolbox/NVDEC/QuickSync), background decode threads, a frame/decode ring cache, playback-resolution downscale, dropped-frame indicator; render preview files for heavy stacks.
3. **Frame accuracy & sync** → integer frame math + drop-frame timecode; resample audio to project rate (`rubato`); lock audio clock, slave video to it; measure drift in tests.
4. **Editing-model correctness** → ripple/roll/slip/slide are precise interval operations on (start, in/out, duration); model them as pure functions with property tests before wiring UI.
5. **Color/render correctness** → composite the program in linear light through `prism-core`; OCIO/log input transforms via `prism-color`; never bake until export; HDR (PQ/HLG) aware.
6. **Shared-crate discipline & Pulse overlap** → `prism-media` is **co-owned with Pulse** (both decode/encode); the compositor/color/fx crates stay time/clip-agnostic. The timeline (clips, ripple, tracks) is Reel's layer; motion-design depth stays in Pulse and arrives via Dynamic Link.
7. **Interop loop** → Reel is the Dynamic-Link *consumer* that closes the suite: a Pulse comp is a clip whose frames are evaluated on demand and cached — same render-graph-node mechanism as a nested sequence.

---

## 7. Immediate next steps

1. [ ] **Phase 1 A/V engine** — `ClipSource::Video` + FFmpeg frame-accurate seek/decode at the playhead (the headline gap), then **audio** (`symphonia`/`cpal`/`rubato`) with A/V sync + waveforms.
2. [ ] **Source in/out points** — decouple source range from timeline placement (the core NLE concept).
3. [~] **GPU program monitor** on `prism-core` (composite, not topmost-only). *(CPU composite **done**: the monitor + scopes sampler now alpha-composite every visible video track bottom-up honoring opacity + per-pixel alpha — see the Phase-1 item above; the real GPU render graph is still open. Undo/redo: **done** — a bounded snapshot history.)*
4. [ ] **Ripple/roll/slip/slide** + 3/4-point editing (Phase 2).
5. [ ] Co-design **`prism-media`** with the Pulse owner (shared FFmpeg/audio engine) and the **Dynamic Link** container with the suite before building export/interop on them.

*Foundations are free. The product is the polish — and the glue between apps.*

---

## Child Windows & Secondary UI

Reel uses GPUI's `cx.open_window(...)` for all secondary windows. The welcome screen is already implemented as a floating child window wired to dispatch `NewSequence`, `OpenFile`, and recent-project actions. The windows below are the remaining secondary UI surface needed to reach parity with Premiere Pro's dialog and workspace model.

### Welcome / Home Screen (already implemented)
- **Kind:** `WindowKind::Floating` (900×560 px)
- **Phase:** done — shown on launch when no project is open; recent projects list, New Sequence, and Open buttons; dismissed on project load.

### Sequence Settings
- **Kind:** `WindowKind::Floating` (560×440 px)
- **Phase:** Phase 2 / Batch 3 (already partially done as an overlay; promote to a true child window)
- Sequence name, resolution presets (1080p/4K/vertical) + custom width/height, frame rate (dropdown + custom), pixel aspect ratio, field order, audio sample rate, working color space. Opened from Sequence ▸ Sequence Settings… and during new-sequence creation. Replaces the current inline floating overlay (`Action::ToggleSequenceSettings`) with a proper OS-level child window that persists position and can coexist with the main workspace.

### Export / Render Queue
- **Kind:** `WindowKind::Floating` (800×560 px)
- **Phase:** Phase 7 (Export & render) — currently a side panel; promote to a dedicated child window for the full Media-Encoder-style queue
- Per-job rows showing composition name, output format, progress bar, and status (Queued / Rendering / Done / Failed). Add to Queue (Ctrl+M), format preset selector (YouTube 1080p/4K, Twitter/X, Vimeo, ProRes 422, GIF, MP3 — already in `ExportPresetB5`), Remove, and Start Queue buttons. Wraps the existing `render_queue` module's `RenderQueue` state. Opening it from File ▸ Export Video… auto-enqueues the current sequence and brings this window forward.

### Preferences
- **Kind:** `WindowKind::Floating` (800×600 px)
- **Phase:** Phase 10 (Reliability, performance & ease-of-use)
- Tabs: General (auto-save interval, undo levels, default frame rate), Playback (hardware decode, playback resolution, dropped-frame threshold), Media (scratch disk, media cache folder, optimized media path), Audio Hardware (device, buffer size, sample rate), Capture (device, format, scratch), Collaboration (future). Persisted to `~/.config/prism/reel_prefs.json`.

### Title / Motion Graphics Editor
- **Kind:** `WindowKind::Floating` (960×640 px)
- **Phase:** Phase 6 (Titles, graphics, captions)
- Essential-Graphics-style editor for creating and editing title clips and `.mogrt` templates. Left sidebar: text / shape / image tool palette. Center: canvas preview with snap/guides. Right: property inspector (font, size, color, alignment, motion presets). Opened by double-clicking a Title clip on the timeline. Shares state with the main app via `Model<App>` so edits are live-previewed in the program monitor.

### Command Palette
- **Kind:** `WindowKind::PopUp` (560×400 px)
- **Phase:** Phase 10 (Prefs / shortcuts / workspaces)
- Keyboard-driven command launcher (Cmd+Shift+P). Fuzzy-search over all registered actions (trim tools, effect adds, workspace switches, sequence settings, export presets). Arrow keys to navigate, Enter to execute. Dismissed on Escape or focus loss. Each result row shows the command name, its current keyboard shortcut (if any), and a category badge. Implemented as a `WindowKind::PopUp` so it floats above all Reel windows including any floating panels.

### Implementation notes
- All child windows share state via `Model<App>` passed at `cx.open_window(...)` time; no duplicated state.
- The Sequence Settings and Export windows remember their last position in `AppPrefs.window_positions: HashMap<String, (f32,f32)>`.
- `WindowKind::PopUp` (Command Palette) is opened with `appears_transparent: false`, no traffic-light buttons, and `is_movable: false`; it is centered on the main window using `Bounds::centered(Some(main_window_handle), ...)`.
- Title bar for Floating windows: `TitlebarOptions { title: Some("Sequence Settings".into()), appears_transparent: false, traffic_light_position: None }`.

---

## 8. UI/UX & workspace

A pro NLE lives or dies by its workspace ergonomics — panel management, monitoring,
and muscle-memory input. Reel's GPUI panels (menu bar · media bin · inspector · audio mixer ·
scopes · timeline · preview) now show/hide via the Window menu, drag-resize, and default to a focused
*Editing* workspace — but there is still no true docking (re-dock / float / tear-off) or named saved
workspaces. The list below is what a Premiere/Resolve-grade editor needs here. *(Partly broadens
Phase 10's "Prefs / shortcuts / workspaces" line into concrete UI work.)*

- [x] **Scrollable panels** — vertical `ScrollArea` (`auto_shrink([false,false])`) on the inspector and media bin so a tall body never overflows the window with no way to reach it. *(done this pass)*
- [x] **Collapsible inspector sections** — each effect-controls group (Transform / Opacity / Crop / Speed / Keyframes / Title / Image-sequence) is a collapsible header (Affinity-style stacked Studio panels). *(done)*
- [x] **Timeline zoom (out / Fit / in)** — pixels-per-second is now adjustable from the transport (`−` / **Fit** / `+`) instead of a fixed constant: **Fit** frames the whole sequence in the visible width, `−`/`+` step by 1.25× over a `6…240 px/s` range, the ruler's labeled-tick interval adapts to the zoom (1·2·5·×10 s), and edge-snapping stays a constant *pixel* distance. The fit math (`timeline::fit_zoom`) is pure + unit-tested. *(done this pass. Still TODO: scroll-wheel / pinch zoom-to-cursor, and a frame-the-selection / zoom-to-work-area shortcut.)*
- [~] **Dockable panels** (M): drag panels to re-dock / float / resize; tear-off into separate windows. *(Resize done: the media bin, inspector, audio mixer, and scopes side panels are drag-resizable with width ranges; the central preview takes the remainder. Re-dock / float / tear-off still TODO.)*
- [~] **Saveable workspaces** (M): named layouts (Editing / Color / Audio / Effects) persisted to prefs and switchable, à la Premiere workspaces / Resolve pages; reset-to-default. *(First step this pass: the **default launch workspace is now the *Editing* layout** — bin + inspector + timeline around the preview, with the audio mixer + scopes hidden until summoned (`⌘4`/`⌘5`), so four parallel side panels no longer crush the program monitor (`PanelVisibility::EDITING`). Named/persisted Color/Audio/Effects layouts still TODO.)*
- [x] **Panel show/hide via a Window menu** (S): toggle each panel's visibility; remember per-workspace. *(done: a pure `app::layout::PanelVisibility` over the three toggleable panels — media bin / inspector / timeline (the central program preview always stays) — driven by a **Window** menu (a checkbox per panel + Show-all / Reset) and `⌘1..3` / `⌘0` shortcuts; hidden panels hand their space to the neighbours and a title-bar badge restores the layout. Unit-tested. Per-workspace memory lands with saveable workspaces.)*
- [ ] **Tabbed panel groups** (S): stack panels as tabs in one dock region (e.g. Effect Controls / Lumetri / Audio mixer share a frame).
- [x] **Source + program dual monitors** (M): a Source monitor (clip with its own in/out + transport) beside the Program monitor; toggle single/dual; gang playheads. *(done: ToggleDualViewer action)*
- [x] **Proper Effect Controls panel** (M): the inspector grows into a real per-clip effect stack — add/remove/reorder effects, per-effect enable, keyframe lanes with a mini-curve editor (ties into Phase 3's effect stack). *(Done: the inspector's **Effects** section is a real per-clip color-grade rack — add (12 kinds) / remove / clear / reorder (up/down) / per-effect enable + per-kind params, and each scalar param now has a keyframe **stopwatch / diamond / interp** (animate over the clip's local time, reusing the transform/opacity keyframe model) — see Phase 3. The deferred **keyframe-lane / mini-curve editor** now ships below the inspector: a `curve_editor` panel with a dope-sheet lane list (drag diamonds to retime) over a draggable value-over-time curve (retime + revalue points, Bézier ease handles, double-click to add a key, Delete to remove), addressing every animatable scalar — transform/opacity + effect-rack params — via `ParamRef`. Remaining gaps: keyframable color/curve params and a multi-param (simultaneous) curve view.)*
- [ ] **Keyboard shortcuts** (M): a remappable keymap (Premiere/Resolve presets + custom), a searchable command palette, and an on-screen shortcut/help overlay. *(Extends Phase 10; today's keys are hard-coded in `app/mod.rs`.)*
- [ ] **Theme toggle** (S): dark / light (and high-contrast) theme switch; today the amber-on-dark theme in `theme.rs` is fixed at startup.
- [ ] **Status / notifications surface** (S): non-blocking toasts for import/save/relink/export-progress instead of silent state changes.

## 9. Parity gaps vs Premiere Pro + DaVinci Resolve

A skim of the plan against both NLEs surfaced these important capabilities not yet (or only thinly)
tracked above. One line each — flagged for the roadmap, **not** implemented this pass; nothing here
pulls in FFmpeg (real video decode stays deferred to Phase 1, `prism-media`).

- [ ] **Audio mixing / levels / waveforms** — clip-waveform draw on the timeline + a track mixer with faders/meters. *(Tracked: waveforms in Phase 1, mixer/levels in Phase 5 — surfacing the UI is the gap.)*
- [ ] **Real video decode (FFmpeg)** — `ClipSource::Video` + frame-accurate seek. *(Already the headline Phase 1 item; **deferred**, no FFmpeg this pass — noted for completeness.)*
- [ ] **Color grading + scopes** — Lumetri-style correction and waveform/vectorscope/parade/histogram. *(Tracked in Phase 4; needs the GPU program monitor first.)*
- [ ] **Multicam editing** — sync N angles, live angle switching, flatten. *(Tracked in Phase 8.)*
- [ ] **Proxy / optimized media** — attach/toggle proxies, full-res on export (Resolve "optimized media"). *(Tracked in Phase 8.)*
- [x] **Nested sequences** — a sequence usable as a clip source. *(Done: a
  multi-sequence `Document`, a `ClipSource::Sequence` that renders the referenced
  sequence recursively with a cycle guard, a Nest-selection command, a bin
  sequence switcher, and serde back-compat for legacy single-sequence files —
  Phase 2. Nested-audio submix + open-in-place noted as gaps.)*
- [~] **Export / render queue with presets** — Media-Encoder/Deliver-page queue, social/web/broadcast presets, in-out range export. *(Done: the **background render queue** — video export runs off the UI thread on a worker (one job at a time) with a live progress panel; pure queue/job/progress state machine unit-tested. Presets / bitrate controls / hardware encode still tracked in Phase 7.)*
- [~] **Titles / captions** — text titles, lower-thirds, SRT/VTT captions, burn-in. *(Title clips exist; **captions done** — a sequence-level SRT/VTT caption track with preview burn-in, import/export, and an inspector editor (Phase 6); Essential-Graphics-style title templates still tracked there.)*
- [~] **J/K/L + JKL trim** — J/K/L shuttle transport and trim-mode dynamic trimming (Resolve's JKL trim). *(Done: the **J/K/L shuttle transport** — a pure `1×→2×→4×→8×` speed-ladder state machine (`app::shuttle`) wired to L/J/K + K-held slow-scrub, coherent with the spacebar transport (see Phase 1 "Real timecode"). **Still open:** the dedicated trim monitor + JKL-*while-trimming* (dynamic trim).)*
- [~] **Snapping / magnetic timeline** — snap to clip edges/markers/playhead with a magnet toggle, and a Resolve-Cut-page-style magnetic mode that closes gaps automatically. *(Done: a pure `Project::snap_candidates` + `snap_time` model snapping a dragged clip/edge to clip edges / playhead / origin / markers / work-area in-out, gated by a **Snap** magnet toggle (`S` key, on by default) with an on-screen guide line; unit-tested. Resolve-style auto-gap-closing magnetic mode still tracked in Phase 2.)*
- [~] **Multi-select + marquee + ripple-aware drag** — band-select clips and move/trim them together. *(Multi-select + marquee + group move/lift/ripple-delete done — pure `project::select` (`Selection`, `clips_in_rect`, `move_clips`/`lift_clips`/`ripple_delete_clips`), Ctrl/Cmd/Shift-click + empty-lane marquee + group body-drag, unit-tested. Ripple-aware *group trim* still TODO.)*
- [ ] **Render cache / background render** — Resolve smart cache: cache heavy stacks so playback stays real-time. *(Tracked in Phase 10.)*
