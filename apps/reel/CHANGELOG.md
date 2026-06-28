# Changelog

All notable changes to **Reel** (the Prism suite's video NLE) are documented in
this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.13.0] - 2026-06-28

### Added — Phase 3: built-in per-clip effects
- **Non-destructive effect stack** — each clip owns an ordered `Vec<BuiltinEffect>`
  (`clip_effects.rs`) with an `enabled` bypass toggle per entry.
- **Parametric effects with real math**:
  - **Transform** — position (x/y), separate scale x/y, rotation (deg), anchor
    pivot; stacked into a single 2D affine matrix (`effective_transform_matrix`).
  - **Crop** — left/right/top/bottom fractions (0..1), summed with opposing
    pairs capped below 1.0 (`effective_crop` / `effective_crop_area`).
  - **Opacity** — 0..1 multiplier folded with clip opacity
    (`effective_builtin_opacity`).
  - **Blend** — reuses `prism_core::BlendMode`; separable W3C blend math +
    Porter-Duff "over" compositing (`blend_channel` / `blend_sample`).
  - **Drop shadow** — offset x/y, blur radius, opacity, color
    (`effective_drop_shadow`).
  - **Gaussian blur / sharpen** — summed radius / amount, clamped.
- **Actions** — `AddBuiltinEffect`, `RemoveBuiltinEffect`,
  `ReorderBuiltinEffects`, `ToggleBuiltinEffect`, `ClearBuiltinEffects`,
  `SetBuiltinEffectParams` (all clamped/sanitized at the `App::apply` choke point).
- +29 tests (374 → 403).

## [0.12.0] - 2026-06-24

### Added — Multi-line + comprehensive text input
- **Typeable clip numerics** — opacity, speed, audio gain (dB), scale X/Y, position X/Y (single-axis edits preserve the other axis).
- **Multi-line captions** — selected cue text via `prism_ui::TextArea` → `SetCueText`.
- **Media-bin search box** — `TextField` filters the active bin by name → `SetBinQuery`.
- +18 tests (356 → 374).

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Title + Essential-Graphics text** — type real title-clip / MOGRT text → `SetTitleText` / `SetMogrParamText` (live program-preview update).
- **Rename** — clip/track/sequence/marker inline rename → `RenameClip`/`RenameTrack`/`RenameSequence`/`RenameMarker`.
- **Export output path** — typeable path → `SetExportPath`.
- +7 tests (349 → 356).

## [0.10.0] - 2026-06-24

### Added — UI (GPUI)
- **Workspaces switcher** (top bar) — Editing/Color/Audio/Effects/Graphics layouts
  driven by the existing panel-toggle actions.
- **Preferences** floating window — autosave + read-only keybinding list.
- **Essential Graphics** panel — list/select/edit/instantiate MOGRT templates.
- **Render bar + proxy status** — work-area render region + proxy toggle/queue.
- **Export** floating window — codec-matrix (container/video/audio/depth/bitrate/
  two-pass/hardware) preset editor.
- All child windows use `WindowKind::Floating`; UI-only (emits existing actions).

## [0.9.0] - 2026-06-24

### Added — Feature waves 1–3 (parity push)
- **Time-remap keyframes + freeze-frame** — `time_remap_speed_keys` on a clip,
  sampled by piecewise-linear integration of the speed curve (factor 0 = freeze)
  in `program_frame.rs`.
- **Transition geometry suite** — Slide, Spin, Zoom, Cube-fold, directional
  Push/Wipe as progress-driven A/B warps (`app_state/transitions.rs` +
  `timeline_transitions.rs`).
- **EDL + FCP-XML export** — CMX 3600 EDL and xmeml v5 FCP XML writers
  (`app_state/edl.rs`).
- **HSL secondary curves + 3DL LUT** — hue-vs-hue/sat/luma grading; `.3dl` import
  and `.cube` export (`app_state/color_curves.rs`).
- **Caption positions + styled inline tags** — per-cue position overrides and
  `<b>/<i>/<u>/<font>` inline styling on burn-in.
- **Export codec matrix** — containers (MP4/MOV/MKV/WebM/PNGseq/EXRseq) × video
  (H.264/H.265/ProRes/VP9/AV1/DNxHR) × audio (AAC/PCM/FLAC) with a pure,
  unit-tested ffmpeg-arg generator (`export_codecs.rs`).
- **Multicam sync + angle switch** — waveform cross-correlation + timecode sync,
  per-time angle selections sampled into cut segments (`app_state/multicam.rs`).
- **Project management** — media offline/online, relink (path remap), consolidate
  manifest (`app_state/project_mgmt.rs`).
- **Autosave / crash recovery** — count-driven versioned snapshot ring with
  restore (`app_state/autosave.rs`).
- **Essential Graphics templates** — text/shape template layers with exposed
  editable props + timeline instantiation (`app_state/graphics_templates.rs`).
- **Proxy workflow** — proxy job model (½/¼ res), attach/detach, proxy↔full
  playback/export toggle (`app_state/proxy_workflow.rs`).
- **Background render cache** — timeline segments keyed by clip-stack hash + range,
  edit invalidation, ring eviction, render-bar status (`app_state/render_cache.rs`).
- **Preferences + remappable keybindings** — prefs (autosave/scratch/playback res/
  default transition) + Premiere-like keymap with conflict detection + JSON
  (`app_state/prefs_keys.rs`).
- **Workspaces** — named panel layouts (Editing/Color/Audio/Effects/Graphics),
  switch/save/reset (`app_state/workspaces.rs`).

### Changed — File organization
- Split `timeline.rs` (2367 lines, over the ~1000-line limit) into `timeline.rs`
  core + `timeline_clips` / `timeline_selection` / `timeline_transitions` /
  `timeline_playback`, with `apply_timeline` delegating to the sub-routers.

### Fixed
- **GPU sprite-atlas memory leak during playback.** RAM grew unbounded and was never released while playing. gpui's sprite atlas only frees an image's GPU tile via an explicit `Window::drop_image`, and the host (`canvas_host::sample_and_bridge`) builds a brand-new `RenderImage` (new image id) on every re-sample/playback frame — so every frame leaked one atlas tile, unbounded. Fixed by holding the previously-painted preview image (`Reel::last_image`) and calling `window.drop_image(prev)` whenever a new-id frame replaces it, bounding the atlas to ~one preview frame. A cached/paused frame returns the same id and keeps its single tile.

### Batch 5 — Completed (2026-06-19)
Depth & polish on existing GPUI-host features — each made real end-to-end
(model + compositor/mix apply + GPUI wiring), not new stubs. All in `reel-gpui`
(self-contained; `reel-app` is legacy and not in the workspace). 60 tests pass
(was 52; +8 new). No new warnings.
- [x] **Nested-sequence recursive compositing** — `ClipSource::NestedClip` rendered for real instead of the dark-purple placeholder. `program_frame::sample_clip_raw` builds a transient sub-`Project` from the inline `tracks`/`clips` and renders it through the *same* compositor (`render_program_inner`) at the speed/reverse-aware nest-local time, composited as the clip's opaque pixels (black backdrop, Premiere-style). A `MAX_NEST_DEPTH = 8` guard degrades a self/cyclic nest to black rather than overflowing the stack; the inner render uses an identity `GlobalGrade` so the outer global grade applies once. The preview, scopes, and full-res export all see nested content now. Unit-tested (green nest shows through; cyclic nest terminates).
- [x] **Caption / subtitle burn-in onto the program frame** — captions were panel-only (SRT import/export, list) but never drawn. `render_program_inner` now burns the cue active at the playhead (`draw_active_caption`) into the composited frame after the grade, before flatten: the embedded 5×7 font in the cue's `CaptionStyle` color, a 60%-dark legibility box hugging the text, position (Bottom/Top/Custom normalized x,y), font size scaled to the comp height, multi-line on `\n`. Threaded through `program_frame` → `CanvasHost::image` → `Reel::preview_image` (`app.captions`) AND the export path (new `JobSpec.captions` snapshot → `with_progress_encode`/`render_program`), so the preview, scopes, and exported MP4 all show the captions. Unit-tested (active cue alters the frame; inactive cue is a no-op).
- [x] **HSL Secondary grade actually applied** — `Clip.hsl_secondary` was editable/stored but never applied in the compositor (only `clip.grade` ran). `sample_clip` now applies the HSL qualifier-based grade after the basic grade, in the documented order, for all clip types — preview, scopes, and export. Added an **HSL Secondary** inspector section (Hue° / Range / Sat / Lum steppers, `Action::SetHslSecondaryGrade`) so it's reachable. Unit-tested.
- [x] **Real audio FX DSP — Reverb + Delay + chain wiring** — the per-track `audio_effects` chain (Eq3/Compressor/Reverb/Delay) was stored but never applied in the mix. New real DSP in `export.rs`: a **Schroeder reverb** (4 parallel comb filters with damped feedback → 2 series allpass, room-size/damping/wet), a **feedback delay line** (time/feedback/wet), a **3-band EQ** (RBJ low-shelf + parametric peak + high-shelf via `apply_shelf`/`apply_eq_band`), and a chain `Compressor` reusing the peak compressor. `MixClip.fx` carries the parent track's chain; `render_program_audio` applies it after the track EQ/comp, so playback + scrub mix honor it (`build_mix_clips` gains an `audio_effects` param, threaded from `App.audio_effects`). Mixer panel gains **+Rev** / **+Dly** add buttons beside **+EQ**. Unit-tested (delay echo, reverb tail + wet=0 no-op, eq3 flat no-op vs boost, end-to-end chain in the mix).
- [x] **Proxy / optimized media used in the preview** — `Clip.proxy_path` was stored + badged in the timeline but the decoder always read the original. `program_frame`'s Video arm now decodes the proxy (when attached) at the same frame-index map, so scrubbing/playback on heavy footage uses the lighter file. Added an inspector **Proxy** section (Attach proxy… via `rfd` / click-to-clear, `Action::SetProxyPath`/`ClearProxy`) for video clips.

### Batch 4 — Added (2026-06-19)
- **RGB Curves color panel**: `RgbCurves { master, red, green, blue: Vec<[f32;2]> }` struct with linear interpolation between sorted control points. Applied per-pixel in `GlobalGrade::apply` after color wheels. Inspector shows per-channel (Master/R/G/B) tab selector with stepper controls for each point's input/output values. Add Point and Reset buttons per channel. `Action::SetRgbCurves`, `SetCurvesChannel`, `AddCurvePoint`, `MoveCurvePoint`.
- **Audio Effects chain per track**: `AudioEffect` enum (`Eq3(TrackEq3)`, `Compressor`, `Reverb`, `Delay`) stored in `app.audio_effects: Vec<Vec<AudioEffect>>`. Mixer panel columns gain FX toggle button; when open, shows effect chips with labels and ×-remove buttons, plus an +EQ add button. `Action::AddAudioEffect`, `RemoveAudioEffect`, `SetAudioEffect`, `ToggleTrackFx`, `ExpandTrackFx`.
- **Closed Captions / Subtitles panel**: `Caption { start_secs, end_secs, text, style }` with `CaptionStyle` and `CaptionPosition`. SRT import (`parse_srt`) and export (`serialize_srt`) functions. Captions panel (toggled by toolbar "CC" button) shows scrollable list with active-caption warm-yellow highlight, click-to-seek, Import SRT / Export SRT / Add buttons. `Action::AddCaption`, `RemoveCaption`, `EditCaption`, `ImportSrt`, `ExportSrt`, `ToggleCaptionsPanel`.
- **Export Presets UI panel**: `ExportPreset { name, format, width, height, fps, bitrate_kbps }` with 4 built-in defaults (1080p H.264, 4K H.264, 720p GIF, ProRes Proxy). Preset list panel (toggled by toolbar "Export▾" button) shows Apply/Del per row and a "Save Current Settings as Preset" button. `Action::AddExportPreset`, `DeleteExportPresetNew`, `ExportWithPreset`, `ToggleExportPresets`.
- **Markers panel**: `GpuiMarker { time_secs, name, color, kind: MarkerKind }` with `InPoint/OutPoint/Chapter/Comment` kinds. Markers panel (toggled by toolbar "Markers" button) shows Add kind buttons, Mark In/Out with current work area times, and a sorted scrollable list with colored dot + name + kind@time + ×-delete. Click any row to jump playhead. `Action::ToggleMarkersPanel`, `AddMarkerAt`, `RemoveMarker`, `RenameMarker`, `SetInPoint`, `SetOutPoint`.
- **Toolbar buttons**: "CC", "Markers", and "Export▾" buttons added to toolbar.

### Batch 3 — Added (2026-06-19)
- **RateStretch tool**: `Tool::RateStretch` variant added; dragging the right clip edge with this tool active changes the clip's playback speed (`clip.speed = orig_dur / new_dur`) rather than trimming source. `ClipDragKind::RateStretch` and `Action::RateStretchClip { index, new_duration }` wired end-to-end (mouse_down, mouse_move, apply).
- **3-way color wheels (Lift / Gamma / Gain)**: `ColorWheels { lift, gamma, gain: [f32;3] }` struct added to `app_state`. Applied globally per-pixel after the LUT in `GlobalGrade::apply`. Inspector shows nine R/G/B steppers (3 per wheel) when a visual clip is selected. `Action::SetColorWheels` triggers a dirty frame.
- **Sequence settings dialog**: `Action::ToggleSequenceSettings` / `SetSequenceSize` / `SetFrameRate` / `SetSampleRate` / `SetColorSpace` added. Floating overlay panel (`panels/sequence_settings.rs`) shows resolution, FPS presets, sample rate, and color space with stepper/preset buttons. "Seq" button in toolbar toggles it.
- **Bins toolbar button**: "Bins" text button added to toolbar to toggle `app.bins_open` via `Action::ToggleBins`.
- **Audio scrub burst**: `App::scrub_audio_burst` plays a 100ms audio burst at the seek position when scrubbing while paused (wired into `Action::Seek` else-branch).
- **InsertClipFromBin fix**: `Action::InsertClipFromBin` now calls `build_imported_clip` and pushes the clip onto the timeline (was a no-op log stub).
- **ColorSpace enum**: `ColorSpace { Rec709, Rec2020, SRGB }` added; stored on `App` as `sequence_color_space`.

### Batch 2 — Added (2026-06-18)
- **ProRes/GIF export format selector**: `ExportFormat` enum (`H264Mp4`, `ProResProxy`, `ProRes422`, `Gif`) added to `App`. Toolbar shows 4 format-selector buttons (MP4 / ProRes P / ProRes 422 / GIF) that set `app.export_format` via `Action::SetExportFormat`. New `Action::ExportProResProxy`, `Action::ExportProRes422`, `Action::ExportGif` variants. `export.rs` gains `ensure_mov_extension` and `ensure_gif_extension` helpers.
- **Speed ramp with Bezier curve**: `SpeedCurve` enum (`Constant`, `Bezier { p0, p1, p2, p3 }`) added to `Clip`. `bezier_source_time` helper in `program_frame.rs` evaluates a cubic bezier for non-linear retiming. Inspector shows "Bezier" toggle (ON/off) for non-audio clips; toggling emits `Action::SetSpeedCurve`.
- **3-band audio EQ per track**: `TrackEq3 { low_gain_db, mid_gain_db, high_gain_db, mid_freq }` struct added with ±12 dB shelves at 200 Hz / 8 kHz and a parametric mid. `App.track_eq3: Vec<TrackEq3>` (parallel to tracks). Mixer panel shows L/M/H increment/decrement buttons per track column via `Action::SetTrackEq3`.
- **Waveform scope individual pixel dots (256 cols)**: Scope column count raised from 64 → 256. `waveform_chart` replaced with individual 2×2 dot layout (absolute-positioned per sample) in a 256×56 relative canvas, giving a true oscilloscope-style scatter plot.
- **Multicam group management**: `MulticamGroup { clips: Vec<usize>, active_angle: usize }` struct added. `App.multicam_groups: Vec<MulticamGroup>`. Source Viewer panel shows a group list + "+ Create Group" button when `multicam_mode` is active. `Action::SwitchMulticamAngle` and `Action::CreateMulticamGroup` wired in `apply`.

### Batch 1 — Added (2026-06-18)
- **Linked audio/video joint-move**: `Clip.link_group: Option<u64>` field added; `Action::MoveClip` and `Action::TrimClipIn/Out` now propagate edits to all clips sharing the same `link_group`. `ToggleLinkClip` assigns/clears the group id.
- **Nest sequence**: `ClipSource::NestedClip { tracks, clips, duration_secs }` variant added. `Action::NestSelectedClips` wraps the selected clip into a nested sub-sequence in place. Program frame renderer shows a purple placeholder (recursive compositing stubbed — see `sample_clip_raw` in `program_frame.rs`). Inspector shows nested clip metadata.
- **Snap toggle button**: Timeline header now has a "Snap" / magnet button that toggles `snap_enabled` via `Action::ToggleSnap`, highlighted when active.
- **Dual viewer center layout**: When `app.dual_viewer` is true, the preview center splits into Source (left, black placeholder with clip name label) and Program (right, live composite) panes side by side, each labeled.
- **Lumetri Scopes improvements**: Tab row (Wave | Vector | Hist) added at top of scopes panel; `Action::SetScopeTab(u8)` + `App.scope_tab: u8` wired. Vectorscope now renders actual Cb/Cr dots (up to 512, green 2×2px divs) in a VS_SIZE×VS_SIZE canvas with a crosshair, replacing the dot-count badge.

### Changed — UI cleanup
- Toolbar stripped to essentials: app name, project name, import, mixer/scopes/dual toggles, export, timecode. Removed add-title, load-lut, multicam, ProRes/DNxHD buttons, and all dead tool/transition variable definitions.

### Added

- **Wave 15: parity complete — custom transitions, proxy media, log-to-Rec709, ProRes/DNxHD**
  - Custom transition plugin system: `DiagonalWipe` and `PixelDissolve` built-in
    plugins implemented with per-pixel math in `program_frame`. Toolbar gains
    **Diag** and **PxDiss** transition buttons.
  - Proxy media: `Clip.proxy_path` field; `[P]` badge in timeline when active;
    `Action::SetProxyPath` / `ClearProxy` wired.
  - Log-to-Rec709: `apply_slog2_rec709` S-Log2 approximation applied per-track;
    `track_log_transform: Vec<bool>` on `App`; **Log** toggle button per track in
    mixer panel; `ToggleLogTransform` action.
  - ProRes/DNxHD export: `Action::ExportProRes` / `ExportDNxHD`; **ProRes** and
    **DNxHD** buttons in toolbar; ffmpeg pipe ready; stub for missing ffmpeg.
  - egui references scrubbed from `reel-gpui` source and Cargo.toml comments.

- **GPUI host — more transitions, per-clip audio volume, basic color grade
  (`reel-gpui`)** — the GPUI host's program sampler (preview + export) now shows
  dip-to-black/white and a wipe alongside the cross-dissolve, audio clips carry a
  per-clip volume folded into the mix, and visual clips carry a basic exposure /
  contrast / saturation grade. The egui `reel` binary is unchanged and stays
  green (536 tests); `reel-gpui` is green (52 tests).
  - **More transitions.** `TransitionKind` (`CrossDissolve` / `DipToColor` /
    `Wipe(WipeDir)`) extends the `Transition` model, mirroring the egui app's
    `project/transition.rs`. `Transition::weights` now returns a `dip` overlay and
    `wipe_reveal` returns the swept reveal-rect. The shared sampler
    (`program_frame::render_program_at`) overlays a flat dip color over the single
    visible clip (peaking opaque at the midpoint) and, for a wipe, alpha-knocks
    the incoming clip to the reveal sub-rect — so preview and the full-res export
    match. Toolbar gains **Dip Blk** / **Dip Wht** / **Wipe** buttons (each adds
    the kind at the cut nearest the playhead on the selected clip's track via
    `Action::AddTransition`).
  - **Per-clip audio volume.** `AudioSource` gains a linear `gain` (unity default,
    capped at +6 dB / 2.0×), surfaced as a **Volume** stepper in the inspector
    (`Action::SetClipGain`). The export audio mix (`export::render_program_audio`)
    multiplies each clip's samples by its gain before summing (a 0 gain is muted),
    so playback + export honor the level.
  - **Basic color grade.** `ColorGrade` (exposure / contrast / saturation) mirrors
    the subset of the egui app's `BasicCorrection::apply` math and is applied
    per-clip in `program_frame::sample_clip` (a true no-op when identity, so an
    ungraded clip pays nothing). Visual clips get **Exposure** / **Contrast** /
    **Saturation** steppers in the inspector (`Action::SetClipGrade`).
- **GPUI host — MP4 export + cross-dissolve transition (`reel-gpui`)** — the
  GPUI host can now render the program to an H.264 MP4 with audio, and the
  preview + export show a cross-dissolve between adjacent clips. The egui `reel`
  binary is unchanged and stays green (536 tests).
  - **Export engine (`reel-gpui/src/export.rs`).** A new module mirrors the egui
    app's `export.rs` + `render_queue.rs`: `render_program` renders the program
    at **full comp resolution** through the shared CPU sampler (so the export
    matches the program monitor — same track fold, opacity, and transitions);
    `with_progress_encode` renders frames **lazily** (one at a time, decode cached
    across frames) and pipes them to the read-only `prism_media` encoder
    (`encode_h264` / `encode_h264_with_audio`). The **program audio mix** is built
    by decoding each audio clip's PCM (`prism_media::decode_audio`, mono @ 48 kHz)
    and summing it into the export span at its timeline position; a non-empty mix
    is muxed as an AAC track, a silent program falls back to a silent encode.
  - **Background render thread + progress.** `JobSpec` is a `Send` snapshot
    (cloned project + pre-rendered audio mix + frame plan + path) captured at
    enqueue time; `run_job` renders→encodes→muxes on a worker thread, streaming
    `JobUpdate` progress over an `mpsc` channel. The root view parks the receiver
    in `Reel::export`, polls it each animation frame, and shows the
    `Exporting NN% (done/total)` / `Exported` / `Export failed` label in the
    toolbar. ffmpeg is gated up-front — a missing binary surfaces as an immediate
    `Failed` status, never a panic. The **Export** toolbar button opens a native
    save dialog (`rfd`) and is greyed/disabled while an export runs.
  - **Cross-dissolve transition.** `app_state` gains a minimal `Transition`
    (cross-dissolve only) plus `Project::find_cut` / `add_transition` /
    `active_transition`, mirroring the egui app's transition model. The shared
    program sampler (`program_frame::render_program_at`, now resolution-agnostic
    so the preview and the full-res export share one compositor) defers the
    transition track's clip and blends the outgoing + incoming clips by the
    transition's opacity ramp over the overlap, so **both the preview and the
    exported MP4** show the dissolve. A **Dissolve** toolbar button emits
    `Action::AddCrossDissolve` for the cut nearest the playhead on the selected
    clip's track.
  - **Tests (`reel-gpui`).** 18 new unit tests cover the frame-plan / mp4-path /
    stem-sanitizer helpers, the program audio mix (sample count, range, silence),
    `build_full_export`, the job-status state machine, an ffmpeg-gated end-to-end
    `run_job` render→encode→probe, the transition model (cut finding, span +
    track-visibility gating, weight ramp), and the dissolve blend in the
    full-res render (plus a clean single-clip render outside the span).

- **GPUI host — timeline scrub-to-seek + live video preview:**
  - **Timeline scrub-to-seek.** Clicking or dragging anywhere in the timeline's
    lane body now seeks the playhead: the pointer x is mapped to a timeline time
    and emitted as `Action::Seek`, which re-samples the program frame (marks the
    host dirty) and slides the red playhead marker with the pointer.
  - **Real decoded video frames in the preview.** The headless CPU sampler
    (`program_frame.rs`) now handles `ClipSource::Video(VideoSource)`:
    it maps the playhead → clip-local time, quantizes to a source frame index,
    and decodes that frame via `prism_media::decode_frame_at` (the ffmpeg CLI
    bridge), scaled to an aspect-preserving preview size (longest side ≤ 960px),
    then aspect-fits it into the comp and composites it like an image clip.
    Decoded frames are **cached by `(path, frame-index)`** so a redraw or
    scrubbing back never re-decodes. A decode failure / missing ffmpeg caches an
    empty buffer and falls back to the black backdrop (logged, never a panic).
  - **First `reel-gpui` unit tests (4)** — video frame-index quantization, the
    unprobed fps fallback, the scrub x→time mapping (incl. clamp outside the
    region), and the `Seek` clamp/dirty contract.

- **Background render queue (non-blocking video export)** (Phase 7 "Export
  engine" — render-queue slice, the top follow-up flagged by the v0.6.0 H.264
  export) — **File ▸ Export video** no longer freezes the UI. The export now
  **enqueues a render job** that runs on a `std::thread` worker; the editor stays
  fully interactive (scrub, edit, even queue more exports) while the movie
  renders in the background, one job at a time.
  - **New `render_queue` module** — a pure, thread-free **job/queue/progress
    state machine** plus a thin threading bridge:
    - `JobSpec` is the **thread-safe snapshot** captured at *enqueue* time — a
      cloned `Project`, the already-rendered program `AudioMix`, the `FramePlan`,
      and the output path — so the worker is fully decoupled from the live editor
      (later edits never affect an in-flight render). The frame renderer
      (`export::render_program`) is already a pure CPU function, so the worker
      drives it directly with no UI/GPU dependency.
    - `JobStatus` (`Queued` → `Rendering { done, total }` → `Done { frames }` |
      `Failed { message }`) with `fraction()` (clamped progress, zero-total safe),
      `is_terminal` / `is_active`, and a panel `label()`.
    - `RenderQueue` — ordered jobs + the pure ops: `enqueue` (monotonic, never-
      reused `JobId`s so a stale worker update can't alias a cleared job),
      `next_pending` (FIFO), `mark_started`, `apply_update` (**ignores a late tick
      on an already-terminal job**), `clear_finished`, `is_rendering` /
      `is_idle` (the **one-job-at-a-time** invariant), and a `summary()` of
      per-state counts for the panel header. **All unit-tested without spawning a
      thread or invoking ffmpeg** (14 new tests).
    - `run_job` is the side-effecting worker bridge: renders + encodes one spec to
      completion on the calling thread, **streaming a `JobUpdate` per frame** over
      an `mpsc` channel, and returns the terminal status. ffmpeg stays gated — a
      missing binary / encode error becomes a `JobStatus::Failed` entry, never a
      panic.
  - **Progress-streaming encode** (`export::with_progress_encode`) — the
    queue-facing twin of `write_video` / `write_video_with_audio`: same lazy
    per-frame render (decode cached across frames, never holding every frame in
    memory) but fires an `on_frame(index)` callback per encoded frame so the UI's
    progress bar advances live. The frame renderer is taken as a parameter so the
    worker and the tests share one wiring; muxes the audio mix when present, else
    encodes silently. A gated end-to-end test asserts one in-order callback per
    frame; another exercises `run_job`'s channel-streamed terminal outcome.
  - **Render-queue panel** (`File ▸ Render queue…`, auto-opened on enqueue) — a
    floating window listing every job with a live per-job progress bar (queued /
    rendering N% / done / failed), a per-state summary header, and **Clear
    finished**. The UI requests a repaint while any job is outstanding so progress
    keeps ticking even when otherwise idle; the worker requests a repaint on
    completion. Closing the window only hides it (jobs keep running).
  - **Kept intact:** the pure helpers (`video_frame_plan`, `render_program`,
    `render_program_audio`, `encode_h264*`) are unchanged; the queue *wraps* them.
    `write_video` / `write_video_with_audio` are retained as unit-tested one-shot
    helpers (now test-only in the app build, which uses the progress path). No
    shared-crate (`prism-*`) change. Existing `.reel` projects and exports are
    unchanged (purely additive).

## [0.6.0] - 2026-06-17

### Added

- **Export: H.264 MP4 video** (Phase 7 "Export engine" — second slice, building
  directly on the v0.5.0 PNG / image-sequence renderer) — encode the program to
  a playable H.264 MP4 via **ffmpeg**, reusing the existing full-resolution CPU
  program renderer (`export::render_program`) for every frame.
  - **Shared encoder in `prism-media`** (separate commit in `prism-suite-prism`,
    additive + app-agnostic, co-owned with Pulse) — `prism_media::encode_h264`
    pipes a lazy iterator of straight-alpha **`rgba`** frames to `ffmpeg` via
    stdin (`-f rawvideo -pix_fmt rgba -s WxH -r FPS -i - -c:v libx264 -pix_fmt
    yuv420p -preset … -crf … -movflags +faststart out.mp4`), so a long export
    never holds every frame in memory. The **ffmpeg argument construction is a
    pure, unit-tested function** (`prism_media::encode_h264_args`); a missing
    binary surfaces as `MediaError::BinaryNotFound` (never a panic), and
    `prism_media::ffmpeg_available()` probes the binary so callers gate the
    encode. A wrong-size frame, a zero-size geometry, an empty stream, or a
    non-zero ffmpeg exit are clear `MediaError::Decode`s.
  - **Reel video export** (`export::write_video`) — renders each frame of the
    chosen range at the comp's native `width`x`height` and comp `fps` (decode
    cached across frames) and feeds them to `prism_media::encode_h264`. The
    range is the work-area (`in`/`out`) when set, else the whole sequence. Pure
    helpers are unit-tested without ffmpeg: `export::video_frame_plan` (the
    inclusive `[first, last]` + frame `count`, **rejecting a zero-length /
    inverted range** so a video always has ≥1 frame and a real duration),
    `export::ensure_mp4_extension` (normalizes the output name to `.mp4`), and
    the re-exported pure arg builder. A gated end-to-end test renders + encodes +
    re-probes a tiny MP4 when ffmpeg is present, skipping with a printed note
    otherwise (the GPU/decode skip convention).
  - **Wired** via **File ▸ Export video (MP4)…** (`rfd` save dialog). The encode
    is **gated on ffmpeg** at runtime — a missing binary (or an empty range)
    shows a clear `rfd` message dialog rather than failing silently / panicking;
    image (PNG) export still needs no ffmpeg. The menu / hover text **acknowledge
    it is a blocking render** (a background render queue is a tracked follow-up).
  - **Encoded vs follow-up (honest):** the **visual program** is encoded with the
    same fidelity as the PNG export — multi-track composite, grades, opacity,
    crop, titles (glyph-rasterized), and the per-clip transform. Other codecs
    (H.265 / ProRes / VP9 / AV1), containers, a render queue / presets, and the
    export-path fidelity gaps shared with the CPU renderer (transitions, nested
    sequences, title rotation, linear-light blending) remain follow-ups.
    CRF/preset are fixed (CRF 18, `medium`) this pass.
- **Export: mux the program audio mix into the exported MP4** (Phase 7 "Export
  engine" — audio slice, building on the H.264 video export) — exported videos
  now have **sound**. The **program audio mix** over the export range is rendered
  to interleaved `f32` PCM (48 kHz stereo) via the *same pure mixer* playback
  uses (`project::mix_window_owned` + the master fold `apply_master`, fed from the
  decode-once audio cache), and muxed into the MP4 as an AAC track.
  - **Additive shared encoder in `prism-media`** (separate commit in
    `prism-suite-prism`, no signature changes — `encode_h264` stays silent) —
    `prism_media::encode_h264_with_audio` muxes an `AudioMix` (interleaved `f32` +
    rate + channels) by writing it to a temp WAV (a dependency-free canonical
    `WAVE_FORMAT_IEEE_FLOAT` serializer) and feeding it to ffmpeg as a second
    input. The **mux ffmpeg-arg construction is a pure, unit-tested function**
    (`encode_h264_args_with_audio` → `-i audio.wav -map 0:v:0 -map 1:a:0 -c:a aac
    -b:a 192k -shortest`); `AudioMix::is_empty()` lets callers fall back to the
    silent encode.
  - **Reel audio render + wiring** — `export::render_program_audio` (pure,
    unit-tested) renders the mix for a `FramePlan` span (`samples ≈ duration ×
    rate × channels`, range start honored), and `export::write_video_with_audio`
    feeds the lazy `rgba` frame iterator + the mix to `encode_h264_with_audio`.
    **File ▸ Export video (MP4)…** now renders + muxes the audio when the sequence
    has decodable, non-silent audio, and **falls back to the silent encode** when
    it doesn't. A gated render→encode→mux→probe test asserts the exported MP4
    carries an audio stream (skips when ffmpeg is absent).
  - **Still open:** LUFS-normalized loudness on export, audio-only outputs
    (WAV/AAC), and the per-format codec/container/queue follow-ups above.

## [0.5.0] - 2026-06-13

### Added

- **Export: PNG still + image sequence** (Phase 7 "Export engine" — first slice;
  no video codec yet) — render the program to PNG files at the sequence's full
  comp resolution via `prism_io::export::save_rgba8`.
  - **Full-resolution CPU program renderer** (`export::render_program`, pure +
    unit-tested, no disk) — composites every visible track bottom-up at the
    comp's native `width`x`height` for a time `t`, reusing the scope sampler's
    compositing primitives (`fold_tracks` / `flatten_over_black`) and per-clip
    content sampling (color / image / image-sequence / video / title via the CPU
    glyph rasterizer), with grades, opacity, and crop. **Key upgrade over the
    scope sampler: the per-clip Transform (position / scale / rotation) is now
    honored**, applied by a pure inverse-warp (`export::warp_layer`) that mirrors
    the preview compositor (scale + rotate about the comp center, then translate
    by the position offset), so the exported frame matches the program monitor.
    An empty / past-the-end program is a black comp of the right size, never
    empty.
  - **Export Frame** (File ▸ Export frame) — render the current playhead to one
    PNG at full res via an `rfd` save dialog (`export::write_still`).
  - **Export Image Sequence** (File ▸ Export image sequence) — render the
    work-area range (`in`/`out` points) or the whole sequence to numbered PNGs
    (`{name}_{index:06}.png`) into an `rfd`-picked folder
    (`export::write_image_sequence`), decoding cached across frames. Pure helpers
    for the frame range (`export::frame_range`), the zero-padded filename
    (`export::sequence_frame_name`), and a filesystem-safe name stem are
    unit-tested without touching disk.
  - Tests cover: full-res render of the default doc (dimensions + sampled teal
    pixel), a grade and an opacity reflected in the output, a position-shifted
    clip landing at the shifted location (transform honored), a past-the-end
    frame being black, the identity-warp passthrough, frame-range indexing, and
    filename padding. **Fidelity gaps (honest):** transitions, nested sequences,
    and title rotation are not applied on this CPU export path; compositing is in
    straight sRGB (not linear light). Encode-to-video is the tracked follow-up.
- **3-/4-point editing** (Phase 2 "insert / overwrite from source to timeline;
  backtiming") — place a source clip's In/Out range onto the timeline via
  **Insert** (ripple — push every downstream clip on the target track later by
  the inserted duration) or **Overwrite** (replace whatever is under the target
  range, no ripple), driven by the source In/Out points + the timeline (work-
  area) In/Out points.
  - **Pure placement math** (`project::edits::compute_three_point`, unit-tested)
    — given the four edit points (`EditPoints`, each `Option<f32>`) and an
    `EditMode`, it resolves the placement (`start` / `source_in` / `duration` /
    `ripple_shift`). It supports **3-point** edits by deriving the one missing
    point — timeline-out from the source window length, source-out from the
    timeline span, **backtiming** (align by the *out* points) when source-in is
    unknown, and timeline-in from timeline-out — and **4-point** edits (all four
    set → fit the source to the timeline span). A zero-length range, an under-
    specified set (< 3 points), or a backtime past the source head is a no-op;
    the consumed source is clamped to the bounded media length.
  - **Project mutation** (`Project::three_point_edit`) applies a resolved
    placement as one labeled undo step ("Insert Edit" / "Overwrite Edit"):
    Insert opens the gap by rippling downstream clips and leaves upstream clips
    untouched; Overwrite drops the clip in place with no ripple.
  - **Wired** through the **Edit menu** (Insert / Overwrite, gated on a selected
    source clip + a timeline in-point) and the **`,` / `.`** keys (Premiere's
    convention). The source is the primary-selected clip's source + source
    In/Out window; the timeline In/Out is the work-area range; the clip lands on
    the target track. Tests cover the 3-point derivations, backtiming, the
    4-point fit, source-length clamping, the zero-length no-op, determinism, and
    the Insert-ripples-downstream-only / Overwrite-no-ripple project mutations.

## [0.4.0] - 2026-06-13

### Added

- **Curves (RGB master + per-channel R/G/B)** (Phase 4 "Curves (M): RGB +
  per-channel") — a Lumetri/Photoshop-style **interactive curves editor** in a
  clip's effect rack. The `Adjustment::Curves` grade (shared `prism-core`
  descriptor; monotone-cubic LUT apply + per-channel chain already existed) now
  has a real **point-editing UI**, replacing the previous single master-midpoint
  slider — per-channel curves were stubbed at their defaults before this.
  - **Channel selector + canvas** (`inspector::curve_editor_rows` /
    `curve_canvas`) — pick **RGB** (the composite/master curve, applied to every
    channel first) or one of **R** / **G** / **B**, then edit a square canvas that
    plots that channel's response (sampled through the *same*
    `prism_core::curve::build_lut` the compositor uses) over an identity-diagonal
    reference grid. **Drag** a knot to reshape it (endpoints move only in `y`),
    **double-click** empty canvas to add a knot, **right-click** a knot to delete
    it (the two endpoints are protected). The active channel and the in-flight
    drag are remembered per-effect in egui memory; a **Reset** button restores all
    curves to identity.
  - **Pure knot helpers** (`project::effect`, unit-tested) — `curve_insert_knot`
    (sorted insert, clamps to `[0,1]`, overwrites a near-coincident x instead of
    duplicating), `curve_move_knot` (clamps `y`; pins endpoint `x`; keeps interior
    knots strictly between their neighbors so the list stays sorted/unambiguous),
    and `curve_delete_knot` (removes any interior knot, never an endpoint). A
    `CurveChannel` enum maps the selector to the right knot list. All UI mutation
    flows through these, so the canvas carries no editing logic of its own.
  - It flows through the rack's existing pure path (`apply_stack_color` /
    `apply_stack_rgba8` / `curve_luts` / `effects_hash`) and serde
    (`.reel`-persisted, legacy identity descriptors load unchanged), so the
    preview compositor and the scopes' CPU program-frame sampler both reflect it
    and the processed-frame cache re-keys on any knot edit. Tests cover identity
    no-op, insert/move/delete invariants, a master knot bending all channels
    equally, per-channel independence, and the hash + serde (incl. legacy)
    round-trip.

- **HSL Secondaries** (Phase 4 "HSL secondaries (M): key a color range, refine,
  grade it") — a Lumetri/Resolve-style **secondary color corrector** in a clip's
  effect rack: **key** (qualify) a range of the image by **Hue**, **Saturation**
  and **Luma** (each with a soft, feathered falloff), then apply a **grade** (hue
  rotate, saturation multiply, gain, exposure) **only inside the keyed region**,
  blended by the qualifier so soft key edges give a smooth transition. It flows
  through the *same* pure functions every effect uses (`apply_stack_color` /
  `apply_stack_rgba8` / `effects_hash`), so the preview compositor and the scopes'
  CPU program-frame sampler both reflect it.
  - **Pure qualifier** (`project::effect::HslSecondary::qualifier`) — converts the
    straight-sRGB pixel to **HSL** and returns a per-pixel membership `q ∈ 0..1`:
    a circular **hue** band (`center ± width/2` with a feathered shoulder via
    smoothstep), and trapezoidal **saturation** / **luma** bands (`lo..hi` + soft
    shoulders). The three memberships **multiply** (a pixel must satisfy all
    keys); a full-range axis (hue width ≥ 0.5, or `lo=0,hi=1`) contributes 1
    everywhere, disabling that axis. NaN-safe (hue wrap via `rem_euclid`, softness
    floored), clamped to 0..1.
  - **Pure grade + apply** (`HslSecondary::apply`) — hue rotate + saturation
    multiply in HSL, then gain + exposure in straight-sRGB, all in a documented
    order; the fully-graded pixel is blended toward the original by `q`. Runs in
    the rack's existing **straight-sRGB 0..1** space (documented — the same space
    every other effect, the preview, and the scope sampler use); every output is
    clamped to 0..1. **Identity** two ways over: the default is a full-range key
    *and* an identity grade (a true per-pixel no-op), and `q` only ever scales the
    *difference* the grade makes, so an identity grade is a no-op at every `q` and
    a pixel with `q = 0` is always unchanged.
  - **Rack integration** (additive, mirroring Color Wheels / Basic Correction /
    LUT): a `ClipEffect` can now carry an optional app-local
    `hsl: Option<HslSecondary>` (`#[serde(default)]`), applied in place of its
    (placeholder) `Adjustment`. It is ordered, reorderable, enable-toggleable, and
    cached — the processed-frame cache re-keys (a distinct hash tag folding every
    key + grade control) when a param changes. `.reel` files round-trip unchanged:
    legacy racks load `hsl: None`.
  - **Effect Controls UI** — "Add effect ▸ HSL Secondary" plus a **Key** group
    (hue center/width/softness, sat low/high/softness, luma low/high/softness) and
    a **Grade** group (hue rotate, saturation, gain, exposure) with a reset to
    identity. Static (not keyframable this pass, consistent with the rack's other
    app-local grades).
  - **Tests** — 11 unit tests: identity is a no-op everywhere (incl. through the
    rack); a pixel inside the keyed hue range is graded, one outside is unchanged;
    soft falloff gives a partial qualifier at the band edge; the saturation and
    luma keys each gate correctly; a non-identity key with an identity grade is a
    no-op; qualifier/output are finite + clamped on extreme inputs; the cache hash
    changes on a key/grade edit and matches for identity; serde round-trips
    (incl. a legacy absent-field `.reel`); alpha is preserved.
  - App-local only — **no `prism-*` crate change**. *Still open:* keyframable
    secondary params, a graphical hue/sat/luma qualifier picker + key-overlay
    (show the matte), and multiple secondary keys per clip.

- **`.cube` LUT import + apply** (Phase 4 "LUTs (S): input + creative
  `.cube`/`.3dl`; export LUT") — load an Adobe/IRIDAS/Resolve `.cube` color
  lookup table and apply it as a creative/input grade in a clip's effect rack.
  It flows through the *same* pure functions every effect uses
  (`apply_stack_color` / `apply_stack_rgba8` / `effects_hash`), so the preview
  compositor and the scopes' CPU program-frame sampler both reflect it.
  - **Pure `.cube` parser** (`project::effect::CubeLut::parse`) — handles both
    **1D** (`LUT_1D_SIZE`, a per-channel transfer curve) and **3D**
    (`LUT_3D_SIZE`, a full cube) LUTs, plus `TITLE`, `DOMAIN_MIN` / `DOMAIN_MAX`
    (defaulting to 0..1), `#` comments, blank lines, and arbitrary whitespace.
    Returns a `Result` — a malformed file (wrong size, bad numbers, missing/extra
    rows, both/neither size keyword, inverted domain, non-finite entry) surfaces a
    `CubeParseError` and **never panics**.
  - **Pure apply** (`CubeLut::apply`) — **trilinear** interpolation between the
    eight surrounding nodes for 3D LUTs, **linear** per-channel for 1D LUTs, with
    the `.cube` red-fastest node ordering and per-channel domain remapping. Runs
    in the rack's existing **straight-sRGB 0..1** space (documented — the same
    space every other effect, the preview, and the scope sampler use); every
    output is clamped to 0..1 so a wild table can't escape range. An **intensity**
    (mix) 0..1 lerps from the original (0) to the fully-mapped image (1); an
    identity table at full intensity, or any table at intensity 0, is a true
    per-pixel no-op (within interpolation tolerance).
  - **Rack integration** (additive, mirroring Color Wheels / Basic Correction): a
    `ClipEffect` can now carry an optional app-local `lut: Option<CubeLut>`
    (`#[serde(default)]`), applied in place of its (placeholder) `Adjustment`. It
    is ordered, reorderable, enable-toggleable, and cached — the processed-frame
    cache re-keys (a distinct hash tag, folding the table shape, domain, intensity
    and every entry) when the table or mix changes. The parsed table **serializes
    with the project**, so `.reel` files round-trip self-contained; legacy files
    (no `lut` field) load unchanged.
  - **UI** — an **Add effect ▸ LUT (.cube)…** menu entry opens an `rfd` file
    picker; a valid file becomes a LUT effect, a bad file is logged and shown in a
    message dialog (no crash). The effect's controls show a read-only
    title/type/size summary, an **intensity** slider, a **Replace…** button (load
    a different `.cube`, preserving the current intensity), and a **Reset**
    (intensity back to 1).
  - **Tests** (21 new): parse a small 2×2×2 3D cube; identity LUT is a no-op (per
    pixel + through the rack); a known invert LUT maps a known input to its
    complement; trilinear interpolation between nodes; 1D LUT parse + linear
    interp; intensity mix; domain remap; comment / whitespace tolerance; a battery
    of malformed inputs all returning `Err`; clamping safety on extreme tables;
    hash changes/stability vs. adjustment / wheels / basic entries; serde
    round-trip incl. the legacy (absent-field) `.reel` case and intensity default.
  All app-local (no `prism-core` / `prism-color` change), consistent with the
  Color Wheels / Basic Correction grades.

- **Basic Correction — Lumetri-"Basic"-style primary grade** (Phase 4 "Basic
  correction (M): white balance, exposure/contrast/highlights/shadows/whites/
  blacks, saturation") — a one-stop primary corrector you add to a clip's effect
  rack, complementing the Color Wheels. Applied CPU-side through the *same* pure
  functions the preview compositor and the scopes' CPU program-frame sampler both
  flow through, so the picture and the scopes agree.
  - **Pure grade math** (`project::effect::BasicCorrection`): white-balance
    **temperature** (warm/cool: R vs B) and **tint** (green↔magenta), **exposure**
    (photographic stops, ×`2^exposure`), **contrast** (linear stretch about
    mid-grey), four tonal sliders — **highlights** / **shadows** / **whites** /
    **blacks** (each an additive push weighted by a smooth function of the current
    level so it only moves the range it names: highlights/shadows use `x²` /
    `(1−x)²`, whites/blacks concentrate at the extremes with `x⁴` / `(1−x)⁴`) —
    and **saturation** (lerp toward/past the pixel's Rec.601 luma; 0 = greyscale,
    1 = unchanged, >1 boosts). Controls apply in a fixed, documented order (WB →
    exposure → contrast → tonal → saturation) in the rack's existing straight-sRGB
    0..1 space, every output clamped to 0..1 so extreme controls never NaN /
    escape range; alpha untouched. **Identity** (temp/tint/exposure/contrast +
    the four tonal sliders 0, saturation 1) is a true per-pixel no-op.
  - **Rack integration** (additive, mirroring Color Wheels exactly): a
    `ClipEffect` can now carry an optional app-local `basic: Option<BasicCorrection>`
    (`#[serde(default)]`) — when set the entry is a Basic-Correction effect applied
    in place of its (placeholder) `Adjustment`. It flows through the *exact same*
    `apply_stack_rgba8` / `apply_stack_color` / `effects_hash` paths as every other
    effect, so it is ordered, reorderable, enable-toggleable, cached (the
    processed-frame cache re-keys when a control changes — distinct hash tag from
    adjustment / wheels entries), and rendered identically by the preview and the
    scope sampler. App-local on purpose (the shared `prism-core` / `prism-color`
    crates stay app-agnostic — see suite CLAUDE.md).
  - **Effect Controls UI**: an **Add effect ▸ Basic Correction** entry, then a
    per-effect panel with Temperature / Tint / Exposure / Contrast / Highlights /
    Shadows / Whites / Blacks / Saturation rows and a *Reset to identity*. Static
    this pass (not keyframable — consistent with the rack's other app-local grades;
    `FxParam` exposes no Basic scalars, so the curve editor / param plumbing
    ignore it).
  - `.reel` **round-trips unchanged**: legacy racks (no `basic` key) load with
    `basic: None`; a fresh adjustment / wheels effect still serializes as before.
    10 new unit tests (identity no-op through the rack, exposure brightens,
    contrast pivots around mid, the four tonal sliders affect the right ranges,
    saturation 0 = greyscale / >1 boosts, temperature warms (R) / cools (B) +
    tint lifts G, extreme-control clamping safety, the cache hash changing on edit
    / stable / distinct from adjustment + wheels, the serde round-trip +
    legacy-absent default, and alpha preservation). Tests 450 → 460 (+10).

- **Color Wheels — Lift / Gamma / Gain (3-way color corrector)** (Phase 4
  "Color wheels (M): lift/gamma/gain (shadows/mids/highlights), 3-way") — a
  Resolve/Lumetri-style 3-way grade you can add to a clip's effect rack, applied
  CPU-side in the same pure functions the preview compositor and the scopes' CPU
  program-frame sampler both flow through, so the picture and the scopes agree.
  - **Pure grade math** (`project::effect::ColorWheels`): per-channel RGB **lift**
    (shadow offset), **gamma** (midtone power), and **gain** (highlight
    multiplier), each with a **master** that folds onto all three channels
    (master lift adds, master gamma / gain multiply, so the identity composes).
    The per-channel formula is the conventional ordering
    `out = ((in + lift*(1 - in))^(1/gamma)) * gain`, operating in the rack's
    existing straight-sRGB 0..1 space with results clamped to 0..1 and alpha
    untouched. **Identity** (lift 0, gamma 1, gain 1 — master included) is a true
    per-pixel no-op. Gamma is floored away from zero and gain from negatives, so
    extreme controls never divide-by-zero / NaN.
  - **Rack integration** (additive): a `ClipEffect` can now carry an optional
    app-local `wheels: Option<ColorWheels>` (`#[serde(default)]`) — when set, the
    entry is a Color-Wheels effect applied in place of its (placeholder)
    `Adjustment`. It flows through the *exact same* `apply_stack_rgba8` /
    `apply_stack_color` / `effects_hash` paths as every other effect, so it is
    ordered, reorderable, enable-toggleable, cached (the processed-frame cache
    re-keys when a wheel changes), and rendered identically by the preview and
    the scope sampler. App-local on purpose (the shared `prism-core`/`prism-color`
    crates stay app-agnostic — see suite CLAUDE.md).
  - **Effect Controls UI**: an **Add effect ▸ Color Wheels (3-way)** entry, then
    a per-effect panel with a Lift / Gamma / Gain row each (three per-channel R G
    B drag-values plus a master) and a *Reset to identity*. Static this pass (not
    keyframable — consistent with the rack's other non-scalar grades; `FxParam`
    exposes no wheels scalars, so the curve editor / param plumbing ignore it).
  - `.reel` **round-trips unchanged**: legacy racks (all adjustment effects, no
    `wheels` key) load with `wheels: None`; a fresh adjustment effect still
    serializes as before. 9 new unit tests (identity no-op through the rack, lift
    raises shadows more than highlights, gain scales highlights / clamps, gamma
    bends mids with endpoints pinned, per-channel tint, extreme-control clamping
    safety, the cache hash changing on edit / stable / distinct from an
    adjustment, the serde round-trip + legacy-absent default, and alpha
    preservation). Tests 441 → 450 (+9).

- **Export-side glyph rasterization for Title clips.** The CPU program-frame
  sampler — the path the scopes analyze, and the foundation the
  future export engine will render through — now rasterizes a title's **real glyph
  outlines** in the chosen font family, not just its background box. Previously
  the chosen family was preview-only: off the glyphs a title sampled to nothing
  and a backed title sampled to a flat plate, so the scopes never saw the text.
  - A new pure `title_raster` module lays the string out into real glyph
    **contours** from a resolved TrueType face (advances + outlines via
    `ttf-parser`, the family resolved by `fonts`) and fills them with an
    antialiased **even-odd** polygon fill — the same recipe the Pulse text path
    uses — so glyph holes (the inside of `o`, `A`, `8`) are carved out. The block
    is laid out to **match the preview's framing**: font size is a fraction of the
    comp height, lines are aligned per the title's `align`, and the block is
    vertically centered, so the scopes register with the picture.
  - Font resolution for the no-egui CPU path is forgiving (`fonts::
    resolve_face_for_raster`): the requested family when available (cached), else
    the **system default sans** (`fonts::default_sans`) standing in for egui's
    proportional family. A font-less environment simply draws no glyphs (the
    sampler degrades to transparent / the background box) rather than panicking.
  - The background box now **hugs the text block** (padded like the preview) when
    glyphs resolve, rather than always flooding the whole frame — so a lower-third
    composites over the tracks below as a strip, with the rest of the frame
    transparent. Both the text and background colors are graded through the clip's
    effect rack first, matching the rest of the sampler.
  - **Fidelity gaps** (shared with the CPU sampler's documented scope): the clip's
    geometric transform (position / scale / rotation) isn't applied on this path,
    and long lines aren't wrapped to the frame width (the egui preview wraps).
  - Pure and unit-tested headlessly (glyph ink with a transparent surround, holes
    carved by the even-odd fill, alignment shifting the ink, the background box
    fill, source-over compositing, and the default-sans fallback), with the
    font-dependent tests skipping on a font-less box. No model / serde change, so
    `.reel` files round-trip unchanged.

## [0.3.0] - 2026-06-13

### Added

- **Drag clips to reposition them in the program monitor** — the selected clip
  (a Title, a video / image, a color, any source) can now be **dragged directly
  in the program monitor** to move it, writing the result into the clip's
  **Transform → Position X/Y**. The screen-space drag delta is mapped 1:1 into
  the clip's normalized position units (x in frame-widths, y in frame-heights,
  up-positive — the exact inverse of the compositor's `offset = [x·w, −y·h]`), so
  the clip tracks the cursor. The move is **keyframe-aware**: when a position
  channel is animated the drag re-keys the playhead frame, otherwise it updates
  the static field — identical to editing the inspector's Position drag-values.
  The whole gesture coalesces into **one labeled undo step ("Move Clip")**, and
  a grab / grabbing cursor hints the affordance. The pure mapping (screen px ↔
  position units, its inverse of the draw offset) and the keyframe-aware
  `Clip::nudge_position` are unit-tested headlessly.
- **Font-family selection for Title clips (preview)** — a Title's text can now be
  rendered in a chosen **system font family** instead of egui's built-in
  proportional default. A new **Font** dropdown in the Title's Effect Controls
  (alongside Size / Color / Align / Background) lists every installed family,
  enumerated with the lightweight pure-Rust **`fontdb`** crate (mirroring the
  Contour app), sorted + de-duplicated, with a **Default** entry at the top. The
  selection persists on the title as an additive `font_family: Option<String>`
  field (`#[serde(default)]` → `None` = the proportional default), so existing
  `.reel` files round-trip and a freshly created title is unchanged. When a family
  is picked, the preview reads **that family's font bytes on demand** (cached, so
  each file is read at most once — egui is never bulk-loaded with every system
  font) and registers them into the egui context under a named family exactly
  once, then lays the title galley out with `FontId::new(size,
  FontFamily::Name(...))`. An absent (`None`), unknown, or unloadable family
  **falls back to the proportional default** rather than disappearing. The pure
  font-resolution seam (enumeration, availability, `None`/unknown → default) is
  unit-tested headlessly; the title's serde round-trip (incl. legacy titles with
  no `font_family`) is covered. **Scope:** this affects the **preview only** — the
  CPU export / scope sampler still can't rasterize glyphs (it paints only the
  title's background box), so export-side glyph rasterization (honoring the chosen
  family in exported frames) remains a follow-up.

### Fixed

- **Title text is centered correctly and honors its Transform** — a centered (or
  right-aligned) Title was anchored with **two competing mechanisms** at once: the
  layout job's `halign` *and* a manual top-left subtraction, double-counting the
  alignment so the text block sat off its intended anchor (a centered title drew
  half its width to the left of center). The title galley is now laid out
  **left-internally** and its block top-left is placed by a single pure helper
  (`title_top_left`) that anchors by alignment and then applies the Transform's
  **Position X/Y** offset, so the numeric Position controls (and the new
  drag-to-move) translate the title exactly and predictably for every alignment.
  The placement math is unit-tested headlessly.
- **Program monitor composites video tracks (not "topmost over black")** — the
  preview now **alpha-composites every visible, non-muted video track from the
  bottom track upward**, each clip drawn *over* the accumulated result honoring
  its **opacity** and **per-pixel alpha**, instead of drawing only the topmost
  active clip over black. A clip on V2 at 21% opacity over a clip on V1 now
  shows 21% of the upper clip blended over the lower track (you mostly see the
  lower clip) rather than a near-black frame (`upper × 0.21 + black × 0.79`). A
  **Text/Title clip with "Background" OFF** is now transparent off its glyphs,
  so the tracks below show through rather than being hidden by an opaque black
  plate (a backed title still draws its background box as the legitimately-opaque
  part of a lower-third). Per-clip effects, transitions, transforms, and crop
  are preserved — the already-processed clip frames are composited. The CPU
  program-frame sampler the scopes analyze got the same multi-track fold
  (`fold_tracks` / `over_in_place` / `flatten_over_black`, a Porter-Duff
  source-over computed on premultiplied values to avoid dark fringing), so the
  scopes match the picture. Pure compositing math is unit-tested headlessly.

## [0.2.0] - 2026-06-09

### Added

- **Real SMPTE timecode (drop-frame) + J/K/L shuttle** — the transport now reads
  out proper **SMPTE timecode** at the project frame rate instead of a bare
  seconds figure. A **pure, headlessly-tested** timecode module
  (`project::timecode`) converts a frame number / seconds ↔ an `HH:MM:SS:FF`
  string in both **non-drop** (`:` separator, counts every frame) and
  **drop-frame** (`;` separator) flavours, parsing a stamp back to a continuous
  frame number for an exact round-trip. The drop-frame math renumbers the NTSC
  rates the canonical way — 29.97 skips frame numbers `00`/`01` at the top of
  every minute **except every tenth minute** (`2 * 9 = 18` labels per ten
  minutes), 59.94 skips four — so the clock tracks wall-time to within a frame
  while the underlying count stays continuous. A `Project.drop_frame` flag
  (additive, `#[serde(default)]` → non-drop; round-trips through `.reel`) and a
  **DF** toggle in the transport (offered only at the fractional 29.97 / 59.94
  rates where drop-frame is meaningful) pick the displayed flavour; the raw frame
  index follows as a secondary cue. **J/K/L shuttle** transport (Premiere /
  Resolve muscle memory) drives the playhead through a pure speed-ladder state
  machine (`app::shuttle`): **L** plays forward and steps the speed up
  `1× → 2× → 4× → 8×` (sticking at the top), **J** mirrors it in reverse, **K**
  pauses, and **K**+**L** / **K**+**J** engage a fixed-fraction slow/scrub;
  pressing the opposite direction drops back to `1×` that way. The shuttle stays
  coherent with the spacebar / transport buttons (one transport); live audio
  tracks the playhead only at `1×` forward (the engine has no resampling
  shuttle), and off-`1×` rates advance the visual playhead by `rate * dt`
  (forward loops at the end, reverse stops at the head) with a speed badge in the
  readout. 23 new tests (17 timecode: the 29.97 minute-boundary skip, the
  tenth-minute non-drop, one-hour reference, 59.94's four-frame skip, frame↔TC
  round-trips across the seams for non-drop 24/25/30/60 and drop-frame
  29.97/59.94, separator authority, dropped-label rejection, sub-frame rounding,
  and the NTSC-only drop-frame gate; 6 shuttle: the forward/reverse ladders and
  stick, pause, opposite-direction restart, the fixed slow fraction, and
  spacebar-coherence).

## [0.1.0] - 2026-06-09

### Added

- **Auto-Duck — duck music under dialogue** — a one-click side-chain ducker that
  dips a **music** (target) track under a **dialogue** (key) track: the target's
  fader ramps down while the key track has sound and restores to 0 dB during
  silence. The core is a **pure, headlessly-tested** function (`project::duck`):
  given the key track's energy envelope (sampled in sequence time) plus params —
  **threshold**, **duck amount** (dB), **attack**, **release**, and a **hold**
  time to bridge inter-word gaps without chatter — `duck_channel` emits a gain
  keyframe `Channel` of linear multipliers (attack/release linear ramps; the hold
  extends each speech run forward; silence / empty envelope / a no-op duck amount
  produce no automation). The key track's envelope is derived from the **existing
  decoded-buffer + pure-mixer path** (peak of a short mix window per envelope
  sample — no new decode path). An **Auto-Duck…** action (Audio menu + the mixer
  panel) opens a dialog to pick the key / target tracks and the params, runs the
  ducker, and writes the curve onto the target track's `gain_anim` through the
  **labeled-undo** system (label "Auto-Duck"), reusing the existing rubber-band
  track-gain plumbing the mixer already samples per output frame. 14 new tests
  (silence → no duck; sustained dialogue → held duck; a gap shorter than the hold
  stays ducked while a longer one releases; attack/release ramp shape; empty
  envelope → no keys; the envelope reducer; and the labeled-undo round-trip).

### Fixed

- **Inspector sections rendered empty / stretched** — the Effect Controls panel
  had two compounding layout bugs that left the collapsible sections (Transform /
  Opacity / Crop / …) broken once the panel had real width:
  - It nested two vertical `ScrollArea`s (the panel wrapped the body in one and
    `inspector::show` opened another inside it), handing the inner one bogus
    geometry so section bodies drew as an empty full-height indent line.
  - Each section's right-aligned reset row used `Layout::right_to_left(Center)`,
    which *vertically centered* the button across the full viewport height that
    the first section is handed — pushing the section's reset row to mid-panel and
    its grid to the bottom, leaving an ~800 px empty gap (only the first section,
    which absorbs the slack, was affected).

  Fixed by removing the inner scroll area (the panel owns the single scroll) and
  top-aligning the reset rows (`Align::Min`); the panel scroll also shrinks to
  content height. Added a headless `egui_kittest` regression test asserting the
  sections stack compactly.

### Changed

- **Workspace layout — the program monitor gets real room** — the default launch
  workspace is now the **editing** layout (media bin + inspector + timeline
  around the preview), with the **audio mixer** and **video scopes** hidden until
  summoned (Window ▸ or `⌘4` / `⌘5`). Four simultaneous fixed-width side panels
  (bin + inspector + mixer + scopes ≈ 1180 px) crushed the central program
  monitor to a sliver on any normal window; mirroring Premiere's *Editing*
  workspace — where the mixer and scopes are separate workspaces, not always-on
  columns — hands that width back to the preview. *Show all panels* (`⌘0`) and
  *Reset layout* still work; the reset target is now the editing workspace.
- **Resizable side panels** — the media bin, inspector, audio mixer, and scopes
  panels are now drag-resizable with sensible width ranges (so the mixer no
  longer clips its 3rd track and the scopes no longer wrap "Vectorscope"
  mid-word when narrow). The central preview always takes the remaining width.

### Added

- **Timeline zoom — out / Fit / in** — the timeline's pixels-per-second is no
  longer a fixed constant: the transport row gains zoom controls (`−` / **Fit** /
  `+`). **Fit** frames the whole sequence in the visible width (so a 30 s
  sequence isn't stuck showing only the first few seconds); `−` / `+` step by
  1.25× within a `6…240 px/s` range. The ruler's labeled-tick interval adapts to
  the zoom (1 · 2 · 5 · ×10 s series) so it stays legible instead of smearing,
  and edge-snapping stays a constant *pixel* distance as you zoom. The fit math
  (`timeline::fit_zoom`) is pure and unit-tested.

- **Audio effects — gain / normalize, parametric EQ, compressor / limiter**
  (Phase 5 "Audio effects: EQ, compressor/limiter, de-noise, gain") — a per-clip
  **audio effect chain** of pure, deterministic DSP processors applied in the
  mixer path **before** the gain / pan fold, mirroring the per-clip color-grade
  effects rack (model + inspector UI):
  - **Pure DSP module** (`project::audio_fx`): an ordered `AudioFxChain` of
    `AudioEffect`s, each a device-free function over an interleaved `f32` buffer,
    unit-tested headlessly (no `cpal` / hardware):
    - **Gain / Normalize** (`AudioEffect::Gain`) — a fixed dB gain, with an
      optional *normalize* that scans the buffer's peak and scales it to a target
      dBFS (then the fixed dB on top). `db_to_linear` / `linear_to_db` are the
      shared dB⇄linear helpers.
    - **Parametric EQ** (`AudioEffect::Eq`) — a cascade of `EqBand`s (low shelf +
      peak + high shelf by default; low/high-pass also available), each a
      stateful per-channel `Biquad` whose coefficients come from the **RBJ
      Audio-EQ cookbook** (`EqBand::coeffs`). The filter state carries across the
      buffer so block processing is seamless; a band above Nyquist / degenerate
      returns the identity filter (never explodes).
    - **Compressor / Limiter** (`AudioEffect::Compressor`) — a peak
      envelope-follower (shared across channels so the stereo image stays intact)
      drives downward gain reduction above `threshold_db` by `ratio:1`, with
      `attack_ms` / `release_ms` smoothing and `makeup_db`. A *limiter* is just a
      high-ratio compressor (`ratio >= LIMITER_RATIO`), labelled as such.
  - **Statefulness handled by the cache, not the mixer**: biquads + the
    compressor envelope must see a clip's samples contiguously, so the chain is
    applied to a clip's whole decoded/resampled buffer **once** and cached by
    `(path, rate, chain-hash)` (`AudioCache::get_processed`); the pure mixer then
    reads the already-processed samples 1:1. A chain edit re-keys (the hash
    changes) and reprocesses. An empty (no-op) chain returns the plain resampled
    buffer with no copy.
  - **Per-clip model** (`AudioClip.fx: AudioFxChain`, `#[serde(default)]`) — an
    `FxEntry` wraps one `AudioEffect` plus an `enabled` flag (mute in place
    without losing params), so a chain round-trips through `.reel` with
    serde-default back-compat (legacy audio clips load with an empty chain).
  - **Inspector "Audio Effects" section** — an **Add effect** menu (Gain /
    Normalize · Parametric EQ · Compressor · Limiter), a **Clear** button, and
    per-entry an enable checkbox, up / down reorder, remove, and the effect's own
    parameter widgets (gain + normalize target; per-band kind / freq / gain / Q
    with add-band / remove-band; compressor threshold / ratio / attack / release /
    make-up). Mirrors the color-grade rack exactly; edits route through the
    labeled undo system ("Audio Effect").
  - 23 new unit tests: gain scales amplitude by the right dB→linear factor,
    normalize hits the target peak (and is a no-op on silence), a low-shelf /
    high-shelf boosts the right band measured with two known sines (and leaves the
    other ~unchanged), a low-pass attenuates above its corner, a peak band boosts
    its center, a flat band is a no-op, biquad state is continuous across split
    blocks, the compressor reduces gain above threshold by ~the ratio (and leaves
    a below-threshold signal untouched), a high-ratio limiter holds near its
    ceiling, make-up lifts the output, the chain is deterministic / applies in
    order / is a no-op when empty, the hash changes with params + on disable,
    degenerate inputs never panic, plus the `.reel` round-trip of a 3-effect chain
    (with a disabled middle entry) + the legacy empty-chain default + an undo
    round-trip restoring an audio-fx edit under the "Audio Effect" label. Tests
    341 → 364 (+23). **Still open (Phase 5 follow-ons):** de-noise / de-reverb /
    de-ess (spectral repair, deferred), per-*track* audio FX (the chain is
    per-clip this pass), and keyframable audio-effect params.
- **Video scopes — histogram, waveform, RGB parade, vectorscope** (Phase 4
  "Scopes: waveform, vectorscope, histogram, RGB parade") — a workspace
  **Scopes** panel that analyzes the composited program frame at the playhead,
  with the scope math kept pure and unit-tested headlessly and the drawing a thin
  egui layer on top:
  - **Scopes panel** (`app::scopes_panel`, a toggleable right-side panel —
    Window menu + `Cmd/Ctrl+5`) with a Premiere-style selector (Histogram /
    Waveform / RGB Parade / Vectorscope). Each scope recomputes every frame from
    the live program, so it tracks edits, scrubbing, and grades exactly as the
    preview does. Read-only (never mutates the document).
  - **Pure scope computation** (`scopes`): a `ScopeFrame` (straight-sRGB RGBA)
    feeds `Histogram` (luma + per-channel R/G/B distribution over 256 bins),
    `Waveform` (brightness vs. image column — a column-binned brightness density,
    column count capped for wide frames), `Parade` (three column-aligned R/G/B
    waveforms sharing one intensity scale), and `Vectorscope` (BT.601 U/V chroma
    binned on a `128×128` disc, with the six standard 75%-bar color targets
    R/Yl/G/Cy/B/Mg at fixed angles). All deterministic and pixel-pure (no egui /
    I/O).
  - **CPU program-frame sampler** (`program_frame`): because Reel previews
    straight through egui's painter (no readable framebuffer), the scopes get
    their own buffer — a `FrameCache` resolves the effective program clip at the
    playhead (`Project::effective_clip_at`, topmost visible content + adjustment
    layers folded on), samples its pixels (color fill / still / image-sequence
    frame / decoded video frame), runs the same pure color-grade rack the preview
    applies, honors crop + opacity (composited over the black comp backdrop), and
    aspect-fits the result at a scope resolution (longest side 256). Decodes are
    cached keyed by `(path, frame-index, effects-hash, size)`; a failed decode /
    missing ffmpeg shows an empty scope rather than crashing. *(Fidelity note:
    this pass analyzes the graded color content — geometric transforms,
    transitions, and nested sequences are not reproduced in the scope buffer
    yet.)*
  - **Panel plumbing**: a new `Panel::Scopes` variant + `PanelVisibility.scopes`
    (Window menu, `Cmd/Ctrl+5`, layout show/hide/reset), mirroring the Mixer
    panel exactly. 27 new unit tests (histogram bin counts on all-black /
    all-white / gradient / known-color frames, waveform column→brightness
    mapping + column capping + pixel-count conservation, parade channel
    separation vs. neutral alignment, vectorscope neutral-at-center + a known
    primary at the right disc angle, six distinct color-target angles, the CPU
    sampler reproducing a color clip + reflecting a grade, crop / letterbox /
    composite math). Tests 314 → 341 (+27).
- **Audio mixer + volume controls** (Phase 5 "Track mixer" + "Clip & track
  volume keyframes") — a workspace **Audio Mixer** panel plus the model that
  drives it, all pure / unit-tested and `.reel`-persisted with serde-default
  back-compat:
  - **Mixer panel** (`app::mixer`, a toggleable right-side panel — Window menu +
    `Cmd/Ctrl+4`): one **channel strip per track** (volume fader + pan slider +
    **mute** + **solo** + a peak/RMS **level meter** + a rubber-band gain
    keyframe / reset row) plus a **master** strip (output gain + pan). Faders and
    pans drive the model live; meters read from the **pure mixer** over a short
    window at the playhead (per-track in isolation + the master with the master
    bus folded), so they show exactly what plays.
  - **Per-track + master gain / pan** (`project::track`, `project::audio::Master`):
    each `Track` carries a `gain` (fader), `pan`, and a keyframable `gain_anim`
    `Channel`; the sequence carries a `Master` strip (gain + pan). The mixer folds
    each clip's gain × track fader, applies a **constant-power pan law**
    (`pan_gains` — center equal at −3 dB, hard L/R full on one side, power
    conserved) per L/R for stereo output, then folds the **master** gain + pan
    over the summed mix (`apply_master`) as the final stage.
  - **Rubber-band volume keyframes** (clip *and* track): track-fader automation
    is keyed in sequence time and clip-volume automation in clip-local time, both
    sampled **per output frame** by the pure mixer (`MixClip::gain_at`), so a
    ramp / dip changes the mix over time. Track keyframes are pinned from the
    mixer strip; clip-volume keyframes get a stopwatch + diamond in the inspector
    **Audio** section (reusing the existing keyframe `Channel` infra).
  - **Level metering** (`project::audio`): `meter_level` (peak + windowed RMS),
    `Level` (+ `peak_db` / `rms_db`), and `amplitude_db` (dBFS with a −120 dB
    floor) — fed the live mix to drive the strip meters (green / amber / red).
  - **Routing + undo + persistence**: every mixer edit is a `MixerAction` the app
    applies through the **labeled undo** system ("Track Volume", "Track Pan",
    "Mute Track", "Solo Track", "Master Volume", "Master Pan", "Track Volume
    Keyframe", …); track gain/pan/`gain_anim`, the clip `gain_anim`, and the
    `Master` all round-trip through `.reel` (old projects load at unity / center).
    17 new unit tests (constant-power pan law, per-track + master gain fold,
    a gain keyframe sampled per output time, peak/RMS meter on known signals,
    `.reel` round-trip of gain/pan/keyframes, undo restores a mixer edit with the
    right label). **Still open:** audio effects (EQ / compressor / limiter),
    ducking, and LUFS loudness metering — Phase 5 follow-ons.
- **Audio engine — `ClipSource::Audio`, a pure A/V-synced mixer, resample, a
  best-effort cpal output stream, and timeline waveforms** (Phase 1 "Audio",
  building on the already-shipped `prism_media::decode_audio`). Audio clips are a
  real, bounded, time-varying source decoded once and mixed at the playhead; the
  mixing / resampling / sync / waveform math is pure and unit-tested headlessly,
  with the device stream a thin shell on top:
  - **`ClipSource::Audio(AudioClip)`** (`project::audio`) — mirrors
    `ClipSource::Video`: an `AudioClip` holds the file `path` plus a cached probe
    (`MediaInfo`, `#[serde(skip)]` so it never bloats / stales the `.reel`; an
    unprobed clip falls back to a default length and is re-probed on load) and a
    per-clip linear `gain` (serde-default 1.0). Its `source_len` is the decoded
    media duration, so every source-aware edit op (in/out, trim, slip, slide,
    roll, speed/retime) constrains against the real audio length. Audio clips are
    excluded from the topmost-*visible*-clip queries (`topmost_at` /
    `topmost_content_at`) since they contribute sound, not a frame; they *do*
    count toward a sequence's `content_len`. The new variant is additive — old
    `.reel` files load unchanged.
  - **The pure A/V-synced mixer** (`project::audio::mix_window`) — *the testable
    heart.* Given the playhead time, an output sample rate + channel count, and
    the set of active audio clips with their decoded buffers, it produces the
    mixed interleaved `f32` output for a time range: each audible clip that
    covers a sample time contributes the sample at its mapped source time (in/out
    + speed/reverse aware), scaled by gain, up/down-mixed to the output channels,
    summed additively (no hard clip — a float mix bus). **Sample-accurate clip
    boundaries** (half-open `[start, end)`), **A/V offset alignment** to the
    playhead, **mute/solo** (via a per-clip `audible` flag the app resolves from
    track enable/solo), and **per-clip gain** are all unit-tested. Device-free,
    deterministic — no audio hardware required for the tests.
  - **Resample** (`project::audio::resample_linear`) — a pure linear resampler
    converting an interleaved buffer between sample rates (source rate → device
    rate), used by the decode cache to convert each clip to the device rate once
    (resample-once-then-mix). Tested for length scaling (halve / double) and tone
    preservation (a 1 kHz tone keeps its zero-crossing structure across a rate
    change). The mixer additionally resamples *speed* per-sample at the playhead.
  - **Waveform peaks** (`project::audio::waveform_peaks`) — pure per-pixel-column
    min/max downsampling of a mono mixdown of an `AudioBuffer` window, drawn on
    each audio clip on the timeline (tracking trims / slips / speed via the clip's
    source-time window). Tested against ramps, a full-scale tone, silence, and a
    stereo cancel-to-mono case.
  - **cpal output stream** (`audio_stream`, *best-effort*) — a thin device shell
    that pulls from the same pure mixer at playback: it opens the default output
    device **lazily** (only on the first play, never at startup or in tests) and
    **degrades to a silent, `dt`-driven transport** when no device can be opened,
    so headless runs / `cargo test` never require audio hardware. The audio
    callback is the master clock (advancing a frame cursor, looping at the
    sequence duration); the visual playhead is slaved to it while playing, keeping
    A/V locked. Seeks / clip-set / loop-bound changes are pushed under a lock each
    frame.
  - **Decode-once audio cache** (`audio_cache`) — the audio analogue of the video
    frame cache: each clip's whole-file buffer is decoded once (`Arc`-shared so it
    crosses to the cpal thread without a copy) and a device-rate-resampled buffer
    cached by `(path, rate)`. A decode failure / missing ffmpeg is remembered so
    it isn't retried; the waveform overlay peeks the cache (never decodes on the
    UI thread).
  - **Import + bin + inspector** — File ▸ Import audio… and a bin **Audio**
    button import `wav/mp3/aac/m4a/flac/ogg/opus/aiff`, probed up front (via the
    new `prism_media::probe_audio`, see below) so the placed clip's duration /
    sample rate are right immediately; a probe failure imports an unprobed clip
    rather than dropping it. The inspector shows read-only audio metadata
    (duration / sample rate / channels / codec) plus an editable per-clip **gain**
    (with a dB readout), routed through the labeled undo system ("Audio Gain").
    Audio clips get a warm-green timeline block with a waveform glyph; the bin
    shows a speaker icon.
  - **Persistence + undo**: `AudioClip` (path + gain) round-trips through `.reel`
    with serde-default back-compat (the probe is re-attached on load via
    `Document::reprobe_videos`, which now probes audio clips too); gain edits and
    placement route through the existing labeled-undo system. 30 new unit tests
    (the bulk on the pure mixer / resample / waveform), plus a gated end-to-end
    test (decode a real ffmpeg tone → mix → waveform) that skips when ffmpeg is
    absent. **Solid:** the pure mixer + resample + waveform + `ClipSource::Audio`
    + clip source + decode cache. **Best-effort:** live cpal device playback (the
    stream is wired and lazy/guarded, but device behaviour is environment-
    dependent and not exercised by the deterministic tests). **Still open:** a
    track mixer UI (faders / pan / meters), audio effects (EQ / compressor), and
    ducking — all Phase 5.
  - *Shared crate (separate commit in `prism-suite-prism`):* `prism-media` gained
    an additive, app-agnostic `probe_audio` (the audio analogue of `probe`, which
    requires a video stream and so rejected pure audio files like `.mp3` / `.wav`)
    + a gated test.
- **Track controls — target track + per-track height** (Phase 2 "Track
  controls", finishing the line alongside the already-shipped add/delete tracks,
  lock, mute-as-hide/eye, and solo) — pure model state routed through the labeled
  undo system, persisted to `.reel` with serde-default back-compat:
  - **Target track** (`project::track`): each `Track` gains a `target` flag kept
    mutually exclusive across tracks (`Project::set_target_track` /
    `target_track` / `is_track_target`). `sync_track_meta` normalizes it so
    exactly one track is always the target — the top track when none is set (a
    legacy / fresh project), the highest-indexed when several are (a hand-edit).
    **New clips now route to the target track** instead of always stacking on the
    top: `place_on_timeline`, `add_title_clip`, `add_adjustment_clip`, and
    `place_sequence_on_timeline` all place onto `target_track()`. Adding a track
    keeps the current target; removing the target track falls back to the top.
  - **Per-track height** (`project::track`): each `Track` gains a `height` (px,
    clamped to `MIN_HEIGHT..=MAX_HEIGHT`, NaN-repaired to `DEFAULT_HEIGHT`) and a
    `collapsed` flag (a compact `COLLAPSED_HEIGHT` skim row). Methods
    `set_track_height` (clamps + expands), `toggle_track_collapsed`, and
    `track_lane_height` / `Track::lane_height`. The timeline now draws **variable
    lane heights** — every y-coordinate (lanes, clip blocks, transitions, drag
    target-track + marquee row hit-testing) derives from the per-track heights
    instead of a fixed lane height.
  - **Track header UI**: two new header toggles per track — a **target**
    crosshair (the target track's name draws in the accent color) and a
    **collapse/expand** button — plus a **height drag handle** along each header's
    bottom edge (live-resizes the row, coalesced into one undo step). The
    `TrackAction` enum gains `SetTarget` / `ToggleCollapsed` / `SetHeight`.
  - **Persistence + undo**: `target`, `height`, and `collapsed` round-trip
    through `.reel`; older files default them cleanly (target false → top track
    on load, default height, expanded). Mutations are tagged "Target Track" /
    "Resize Track" for the Edit menu, alongside the existing "Add Track" /
    "Remove Track" / "Lock Track" / "Solo Track" / "Toggle Track". A loaded
    document's `normalize` now runs `sync_track_meta` per sequence so legacy /
    hand-edited track metadata is repaired (one entry per track + one target).
- **Markers complete — clip markers + a color/comment editor** (Phase 2
  "Markers", finishing the line: sequence markers + work-area in/out already
  shipped) — markers now exist at *both* scopes with an editable color + comment,
  all pure model data routed through the labeled undo system (no FFmpeg / decode):
  - **Clip markers** (`project::marker`): each `Clip` gains a
    `markers: Vec<Marker>` (`#[serde(default)]` = none) of cues at **clip-local**
    times (`0..=duration`) that travel with the clip. Pure `Clip` methods mirror
    the sequence-marker API — `add_marker` (sorted insert, clamped to the clip
    span), `remove_marker_near` (closest within `MARKER_PICK_DIST`),
    `next_marker_after` / `prev_marker_before` navigation, and `clamp_markers`
    (re-sort + re-clamp after a trim). **`split_clip` distributes clip markers
    across the cut** — markers before the cut stay on the left half, the rest move
    to the new right half rebased to its head — so a razored clip keeps its cues.
  - **`M` is now DWIM**: `M` adds a **clip** marker on the primary selected clip
    when the playhead is over it (else a **sequence** marker); `⇧M` removes the
    nearest one in the same scope — Premiere's convention. Marker navigation
    (`↑` / `↓` and the Markers menu) jumps across the **merged** set of sequence
    markers + the selected clip's clip markers (mapped to timeline time).
  - **Marker color + comment editor** (inspector): a new **Markers** section edits
    the marker nearest the playhead (the selected clip's clip marker when one is
    in range, else the nearest sequence marker) — a single-line **comment** field
    and an RGBA **color** picker plus a row of quick-pick swatches from a 7-color
    marker palette (`Marker::PALETTE`). The edit is tagged "Edit Marker" for undo.
  - **Timeline draw**: clip markers render as small colored pennants on the top
    edge of their clip block (distinct from the ruler's sequence-marker pins), in
    each marker's own color.
  - **Undo**: every marker mutation routes through the existing labeled snapshot
    history — "Add Marker" / "Remove Marker" (sequence), "Add Clip Marker" /
    "Remove Clip Marker" (clip), "Edit Marker" (recolor / recomment), "Mark In" /
    "Mark Out" — so each is a single, named, reversible step.
  - `markers` persists in the `.reel` JSON (`#[serde(default)]`, so older projects
    load with no clip markers); the project round-trip now carries a colored clip
    marker, and the legacy-file test asserts the empty-default.
  - 7 new unit tests: clip `add_marker` sort + clip-span clamp (both ends),
    `remove_marker_near` closest-pick, clip-marker next/prev navigation,
    `split_clip` distributing markers across the cut (left stays / right rebased),
    `clamp_markers` re-sort + clamp, plus two `app::history` document round-trips
    (a labelled sequence-marker add→edit chain undo→exact / redo→exact with the
    right label travelling onto redo, and a clip-marker add round-trip).

- **Undo / redo polish — text-edit coalescing + named-command labels** (closes
  the two documented gaps left by the initial undo/redo pass) — no FFmpeg / GPU:
  - **Per-keystroke text edits now coalesce into one undo step** (gap 1): typing
    into a text field (title text, caption text, a numeric param typed as text)
    used to record a separate history checkpoint per keystroke, so one Undo
    removed a single character. The frame-level commit machinery now treats an
    in-field text-editing session exactly like an in-progress pointer drag — the
    pre-edit baseline is **held** while a text widget keeps focus (no per-frame
    commit), and a **single** checkpoint is committed when focus leaves the field
    (or moves to a *different* field). Switching between two fields produces two
    distinct, correct steps. The focused widget is detected as a text edit via
    its egui `TextEditState`; `ReelApp` tracks the focused-text-field `Id`
    (`text_edit_session`) and commits the coalesced edit at the session boundary
    (`commit_held_edit`). Undo/redo and New/Open reset the session.
  - **Named-command labels in the Edit menu** (gap 2): the Undo / Redo menu items
    (and their tooltips) now name the action — "Undo Split", "Undo Trim", "Undo
    Move", "Undo Delete", "Undo Effect Change", "Undo Rename"/"Edit Title"/"Edit
    Caption", "Redo …", etc. — instead of a generic label. `History<T>` gained a
    short per-entry label (`record_labeled`, `undo_label`, `redo_label`) that
    travels with the entry across the undo↔redo swap (so "Redo Split" matches
    "Undo Split"). `ReelApp` derives the label from the action that produced the
    checkpoint: each editing command / timeline drag / inspector edit tags a
    `pending_label` (Split / Trim / Move / Roll / Slide / Delete / Ripple Delete /
    Add Transition / Speed Change / Reverse / markers / captions / track ops /
    nest / transform / opacity / crop / effects / keyframes / title / caption …),
    consumed when the frame's checkpoint commits; unlabelled edits fall back to a
    generic "Undo" / "Redo".
  - 6 new unit tests (24 → in `app::history`): label exposure for the next undo
    then travel onto redo, each checkpoint keeping its own label, a new labelled
    edit dropping a pending redo label, an unlabelled record reporting no label,
    **plus** the text-edit coalescing flow driven through the app's exact
    baseline/commit discipline — a multi-keystroke single-field edit yields
    **one** undo step (and undo restores the pre-edit document, not one
    character), and **two different fields yield two** distinct, correctly
    labelled steps.

- **Undo / redo** (Phase 2 "Undo/redo" — *the highest-value missing feature*) —
  a bounded, snapshot-based history over the whole edit document, so every
  mutating action (split, trim, move, delete, transition, speed, effect change,
  caption, marker, track op, nest, …) round-trips. Pure history model + a
  frame-level commit/coalesce wiring, no FFmpeg / GPU:
  - **History model** (`app::history`): a new pure, generic `History<T>` — a
    bounded undo stack of pre-edit snapshots plus a redo stack. `record(snapshot)`
    pushes a pre-edit state (and **clears the redo stack**, so a new edit
    abandons the redoable branch); `undo(current)` / `redo(current)` swap the
    live state with a stored snapshot (parking `current` on the opposite stack);
    the undo depth is **bounded** to `DEFAULT_LIMIT` (100), dropping the oldest
    snapshot on overflow (redo keeps the undo stack bounded too); `can_undo` /
    `can_redo` / `undo_depth` / `redo_depth` / `clear`. Generic over the snapshot
    type so the stack discipline is unit-tested independently of the document and
    the UI.
  - **Wiring** (`app::ReelApp`): the app holds a `History<String>` of serialized-
    document snapshots (`Document::to_json` is the source of truth for both the
    snapshot *and* the change-detection diff, so an edit is recorded exactly when
    it alters the persisted document — transport-only state like the playhead
    never records). Each frame baselines the document's pre-edit state and, at
    frame end, commits a checkpoint **iff** the document changed vs. that
    baseline. Continuous edits **coalesce**: while a pointer drag is in progress
    (and on the drag-release frame) the baseline is held, so a multi-frame clip
    drag / trim / keyframe drag is a **single** undo step; a no-op frame records
    nothing (trivial-repeat coalescing falls out of the diff). Undo / redo
    restore a snapshot (re-attaching un-serialized video probe metadata and
    clamping the transport / selection to the restored edit) and rebaseline so
    the restore itself is never re-recorded. **New** / **Open** reset the history
    (a fresh document has no past).
  - **UI**: an **Edit ▸ Undo / Redo** pair at the top of the Edit menu
    (enabled-state and a step-count tooltip from the live history depth) and the
    standard shortcuts — **`Cmd/Ctrl+Z`** undo, **`Cmd/Ctrl+Shift+Z`** (and
    `Cmd/Ctrl+Y`) redo — ignored while a text field has focus (so typing keeps
    egui's in-field text undo).
  - 14 unit tests: the pure stack discipline on trivial values (empty no-op,
    record→undo→redo round-trip, an exact undo/redo chain, a new edit clears
    redo, depth bounding drops the oldest + redo keeps the undo stack bounded,
    limit clamp, clear), **plus** five document round-trip tests that drive a
    real `Document` exactly as `ReelApp` does (serialized snapshots): a **split**
    edit undo restores the byte-for-byte prior document then redo re-applies, and
    the same exact round-trip for a **move**, a **delete**, an **effect change**,
    and a "new edit discards the pending redo branch" case.
- **Color Balance & Channel Mixer grades** (shared-crate follow-through) — the
  per-clip effects rack now handles `prism_core::Adjustment`'s two newest
  variants (added suite-side): **Color Balance** (per-tonal-range shadows /
  midtones / highlights RGB pushes + preserve-luminosity) and **Channel Mixer**
  (per-output-channel linear mix of the input RGB + constant, + monochrome).
  Both already appear in the rack's **Add effect** menu (driven by
  `Adjustment::defaults()`); this pass gives them CPU pixel kernels (mirroring
  the shared compositor's reference math — Color Balance's tonal-weighted shift
  LUT, Channel Mixer via `prism_core::ChannelMixerMatrix::apply`), folds their
  array params into the processed-frame cache hash so editing a push / mix
  re-keys the cache, and adds inspector parameter widgets for each (their params
  are arrays / bools, so they remain non-keyframable — consistent with the
  documented non-scalar gap). This unblocks the build against the current shared
  `prism-core`.

- **Real video decode** (Phase 1 A/V engine — *the headline gap*): a clip can now
  be a real movie file whose frames are decoded and **scrubbed/played on the
  timeline**, via the new shared **`prism-media`** crate (an ffmpeg/ffprobe-CLI
  A/V bridge — version-tolerant, no `-sys` linking; see the suite changelog):
  - **`ClipSource::Video(VideoClip)`** (`project::video`): a new bounded source.
    `VideoClip` holds the file `path` plus a cached probe (`MediaInfo`); the cache
    is `#[serde(skip)]` so it never bloats / stales the `.reel` (a loaded clip
    comes back *unprobed* and is re-probed on load). Its `source_len` is the
    decoded media duration (a default until probed), so every source-aware edit op
    (source in/out, trim/slip/slide/roll, speed/retime) constrains against the
    real footage length. `frame_index_at` / `frame_time` quantize a source time to
    a source frame (the decode-cache key). New `id`/probe fields are additive +
    `serde(default)`/skip, so **old `.reel` files without the `Video` variant
    still load** unchanged.
  - **Preview decode + cache**: drawing a Video clip maps the playhead →
    clip-local → source time (in/out + speed/reverse aware via `Clip::source_time`),
    quantizes to a source frame index, decodes that frame **scaled to a preview
    size** (longest side ≤ 960px, aspect preserved) through
    `prism_media::decode_frame_at`, runs it through the **existing per-clip effects
    rack** (the same `get_processed` grading path), uploads it as a texture, and
    **caches by `(path, frame-index, effects-hash)`** like the image / processed
    caches — so scrubbing within a frame, or holding a paused frame, costs nothing.
    A decode error / **missing ffmpeg draws the existing muted placeholder** —
    never a crash.
  - **Import** (`File ▸ Import video…` + a bin **Video** button): detects
    `mp4/mov/mkv/webm/avi/m4v`, probes up front so the placed clip's duration
    matches the footage, and falls back to importing the clip *unprobed* (default
    length, re-probed later) if the probe fails (e.g. ffmpeg absent) — graceful,
    no crash. On opening a `.reel`, video clips are **re-probed**
    (`Document::reprobe_videos`) to re-attach metadata.
  - **Inspector**: a Video clip shows read-only metadata — duration, resolution,
    fps, codec, audio (codec / sample-rate / channels), and the current source
    frame index — or an "unprobed" hint.
  - Distinct saturated blue-violet timeline block + a film glyph in the bin.
  - Tests: probed `source_len` = media duration, bounded-trim clamps to length,
    clip-local→source-frame mapping (through `source_time`), Video-clip serde
    round-trip (probe dropped, path kept), legacy `.reel` without the variant
    loads, video-extension detection; plus `prism-media`'s own gated decode tests.
    Decode-dependent assertions gate on ffmpeg presence (skip if absent).
  - **Audio**: `prism-media` ships `decode_audio` (+ a gated test), but a full
    audio **playback** engine (output stream, A/V sync, waveforms) is still open —
    the next `prism-media` follow-on (see PLAN Phase 1 / Phase 5).
- **Nested sequences** (Phase 2 "Nested sequences / subsequences" — Premiere's
  nesting) — a sequence can be placed as a clip inside another sequence and
  renders recursively through the existing preview/compose path, exactly like
  Pulse's precomps in Reel's clip/timeline model:
  - **Multi-sequence document** (`project::document`): a new `Document` is the
    top-level `.reel` container — an id-keyed `Vec<Project>` (each `Project` is a
    sequence, now carrying a stable `id` + `name`) with one marked **active** for
    editing and a monotonic id minter (`mint_id`). The app holds a `Document`;
    the timeline / inspector / preview operate on the active sequence (via
    `Document::active`/`active_mut`), and the others are referenceable as nested
    clips. `add_empty_sequence` / `push_sequence` / `set_active_id` manage the
    set.
  - **serde back-compat** (`Document::from_json`): the single load entry point
    accepts **either** the new multi-sequence document **or** a legacy
    single-sequence `Project` (an old `.reel` file, no `sequences`/`id`), wrapping
    the latter into a one-sequence document and minting it a real id; the result
    is always `normalize`d (unique non-zero ids, in-range `active`, leading
    `next_id`). A **File ▸ Open .reel…** command loads through it. New `id`/`name`
    fields are `serde(default)` so legacy clip/project JSON still loads.
  - **Nested-sequence clip source** (`ClipSource::Sequence(seq_id)`): a clip that
    draws another sequence by id. Its natural duration is the referenced
    sequence's **content length** (`Project::content_len` — the right edge of its
    last non-adjustment clip; `Document::sequence_source_len` resolves it,
    `new_nested_clip` builds a clip already sized to it). At the playhead the
    referenced sequence is rendered **recursively** through the same compose path
    as a top-level program (its clips, tracks, adjustment layers, transitions,
    captions) at the mapped local time (speed-aware via `Clip::source_time`), and
    composited into the clip's (cropped / scaled / positioned) frame.
  - **Cycle guard** (`project::nest`, `RenderCtx`): the recursive renderer carries
    a visited-set of sequence ids and *refuses* to re-enter one already on the
    stack, so a cycle (A → B → A) or a self-nest renders **nothing** instead of
    infinite-looping / overflowing the stack. The direct self-nest is also
    refused up front when placing a sequence as a clip.
  - **Nest command** (`Document::nest_clips`, **Edit ▸ Nest selection…**): wraps
    the selected clips into a new sequence (rebased so the group starts at t=0,
    relative timing + stacking preserved, canvas inherited) and replaces them on
    the timeline with a single nested-sequence clip at the earliest clip's start
    — Premiere's classic Nest workflow.
  - **Sequence switcher UI**: a **Sequences** list at the top of the bin — a row
    per sequence (active one highlighted), click to open/edit, a `+` to add a
    fresh sequence, and, for any non-active sequence, a nest button that places it
    onto the current timeline as a nested clip at the playhead. The nested clip
    shows its sequence name (gold block on the timeline; an inspector "Nested
    sequence" reference section).
  - All model logic is pure and unit-tested (20 new tests): nested-clip duration
    = referenced content length, mapped-time sampling, cycle-guard termination
    (A↔B), self-nest → nothing, dangling-id → nothing, multi-sequence serde
    round-trip (incl. a `Sequence` source), legacy single-sequence back-compat
    (with and without the new `id` field), `normalize` repair, and the Nest
    wrap+reference workflow.

- **Keyframe-lane / curve editor** (Phase 3 "Effect stack" follow-on — closes the
  explicitly-deferred follow-on after keyframable effect params) — a graph editor
  for the selected clip's animatable parameters, so keyframes can be viewed and
  reshaped directly instead of only set at the playhead. Reuses Reel's *exact*
  keyframe model (`Channel` / `Keyframe` / linear-hold-Bézier `Ease`) — no new
  animation type:
  - **Generic param addressing** (`project::param`): a new `ParamRef` names any
    keyframable scalar of a clip uniformly — a transform/opacity `AnimProp` on
    the clip's `ClipAnim`, or an effect-rack `FxParam` on a `ClipEffect` — with
    `channel`/`channel_mut` (the shared/mutable keyframe `Channel`, creating an
    effect channel on demand), `label`, `static_value`, `value_at`, `is_animated`,
    and `animated_on(clip)` (every param that currently carries a key, in a
    stable transform-then-effect order). The editor's pure addressing layer.
  - **Editing-logic helpers on the model** (`project::anim`): `Channel::move_key`
    (retime + revalue a key keeping the list sorted, returning its new index — the
    pure drag behavior, interp preserved), `Channel::value_bounds` (frame the
    value axis), and `Ease::with_handle1`/`with_handle2` (replace one Bézier
    control handle, clamping so the two time handles can't cross). All pure and
    unit-tested.
  - **The editor** (`curve_editor`): stacked below the inspector's effect
    controls in a collapsible "Graph editor" section. A compact **dope-sheet lane
    list** shows one row per animated parameter with its keyframes as draggable
    diamonds along the clip's local time (drag a diamond to **retime**); clicking
    a lane label picks it for the curve view. A **value-over-time curve view**
    plots the selected parameter: keyframes are draggable points (drag =
    **retime + revalue**), an eased leaving-segment shows draggable Bézier ease
    handles (drag = **reshape easing**; a linear/hold segment is promoted to an
    editable ease, seeded straight so the conversion is value-neutral),
    **double-click** the plot **adds a key on the curve** at that time, and
    **Delete** (or the Delete-key button) **removes the selected key**. The
    playhead is a vertical guide in both views; clicking empty plot space scrubs
    it. Edits mutate the real `Channel` keyframes, so the preview's sampled values
    (and the animated processed-frame cache) update immediately.
  - 17 new unit tests for the editing *logic* (not pixels): `move_key` keeps the
    channel sorted + returns the landed index + preserves interp + clamps time;
    revalue updates the sample; `value_bounds`; an ease-handle change alters the
    interpolation between two keys and handles can't cross in time; the
    screen-x↔clip-local-time and y↔value maps round-trip; handle screen-position
    maps normalized ease space onto the segment rect; `pick` prefers the nearest
    element (handles before keys); and `ParamRef` addressing (`animated_on`
    ordering, on-demand effect channel, stale-index safety, sampled-vs-static
    `value_at`, effect label carries its stack position).
  - Known gaps (documented): the curve view graphs **scalars only** — keyframable
    color & curve params (RGB triplets + Curves knots) remain non-animatable; the
    curve view edits **one parameter at a time** (the lane list shows all but
    multi-param simultaneous curve editing is future work).

- **Keyframable effect parameters** (Phase 3 "Effect stack" follow-on) — the
  per-clip color-grade effects rack's scalar parameters can now be **animated
  over the clip's local time**, reusing the *exact* transform/opacity keyframe
  infrastructure (`Channel` / `Keyframe` / `Interp` linear-hold-Bézier `Ease`).
  Pure model + sampling in the preview + cache-key fix + inspector controls, no
  FFmpeg / GPU shader:
  - Model (`project::effect`): a new `FxParam` enum names each keyframable scalar
    of a `prism_core::Adjustment` (Brightness, Contrast, In black/white, Gamma,
    Hue, Saturation, Lightness, Stops, Amount, Threshold Level, Density), with
    `FxParam::for_adjustment` (the scalars a kind exposes, in order),
    `get`/`set` (read/write the scalar on the shared descriptor, with a
    kind-guard so a stale channel after a kind change is ignored), and `label`.
    Each `ClipEffect` gains `params: Vec<FxParamAnim>` (`#[serde(default)]` =
    none) — one keyframe `Channel` per animated param — plus `is_animated`,
    `channel`/`channel_mut`/`clear_channel`, and `sampled(local_t)` (the effect
    with every animated param sampled and baked into a clone of its
    `Adjustment`, carrying no animation). Free helpers `effects_sampled(rack,
    local_t)` (the per-time *effective* rack) and `rack_is_animated`. The
    animation layer is a thin overlay on the shared descriptor — no parallel
    param storage to drift. (Non-scalar params — the RGB triplets of Photo
    Filter / Gradient Map and the Curves knots — are **not** keyframable this
    pass; noted as a gap.)
  - **Sample-at-time in preview**: `draw_clip` now samples the rack at the
    clip-local playhead time (`effects_sampled`) *before* applying it, so an
    animated grade updates per frame for image, image-sequence, and color clips
    alike (the color path grades the sampled fill).
  - **Cache key fix**: the processed-frame cache (`TextureCache::get_processed`)
    is keyed by `effects_hash` of the rack it's handed — now the *sampled*
    (effective) rack, so the params hash reflects the sampled values, not just
    the static ones. An animated effect therefore re-keys (and reprocesses) per
    playhead time, while a static rack hashes identically at every time so its
    processed texture stays cached during playback.
  - **Inspector**: each scalar parameter in the Effects section now carries a
    keyframe **stopwatch** (enable animation — captures the current value as a
    key at the playhead; disable — freezes the sampled value back into the
    descriptor and clears the channel) and a **diamond** (add / update / remove
    the key at the playhead), plus the same interp toggle + custom-ease handle
    drag-values as the transform/opacity rows. The slider shows the *sampled*
    value at the playhead and writes a key there when animated. An animated
    effect rack now counts toward `Clip::is_animated`, so the timeline shows its
    keyframe diamond badge.
  - The animation persists in the `.reel` JSON (`#[serde(default)]` on `params`,
    so pre-feature racks load static); the project round-trip now carries an
    eased animated Brightness param, and a dedicated test loads a legacy effect
    with no `params` key as static.
  - 12 new unit tests (param enumeration per kind, `get`/`set` round-trip +
    kind-guard, animated-param sampling with linear / hold / ease, static-param
    time-invariance, `effects_hash` differing across time when animated and
    stable when static, `clear_channel` revert, `rack_is_animated`, plus the
    serde round-trip + legacy-default tests).
  - **Gap**: a full keyframe-*lane* editor (a per-param timeline with draggable
    keys / a mini-curve canvas) and keyframable color/curve params remain TODO.

- **Per-clip color-grade effects rack** (Phase 3 "Effect stack") — a Lumetri-style
  ordered effect rack that every clip now carries on *its own* pixels, distinct
  from an *adjustment LAYER* (the clip type that grades the tracks beneath it).
  Pure model + a CPU pixel kernel, no FFmpeg / GPU shader:
  - Model: a new pure `project::effect` module. Each `Clip` gains an ordered
    `effects: Vec<ClipEffect>` (`#[serde(default)]` = empty). A `ClipEffect`
    wraps one shared `prism_core::adjust::Adjustment` color correction plus an
    `enabled` flag (`#[serde(default)]` = `true`) so a single effect can be muted
    in place without losing its params or its position in the rack. Twelve
    adjustment kinds are exposed via `available_adjustments()` (the shared
    `Adjustment::defaults` list): Brightness/Contrast, Levels, Exposure,
    Hue/Saturation, Vibrance, Invert, Threshold, Black & White, Posterize, Photo
    Filter, Gradient Map, and Curves. `Clip::has_grade` reports any *enabled*
    effect and now counts toward `has_effects`.
  - **CPU application**: Reel previews through egui's painter with uploaded
    textures (no custom shader), so the rack is applied **CPU-side** in
    straight-sRGB `0..1` space (alpha preserved). `apply_stack_rgba8` folds the
    enabled effects in order over an 8-bit RGBA buffer; `apply_stack_color`
    does the same for a `Color` clip's flat fill. Each kind has a pure per-pixel
    kernel (`apply_pixel`); Curves / Gradient Map prebuild a 256-entry LUT once
    via `prism_core::curve` then map per pixel (master curve then per-channel).
  - **Processed-frame cache**: a new `TextureCache::get_processed` keys the
    graded texture by `(source path, effects-params hash)` — `effects_hash`
    folds every kind's discriminant, params (floats by bit pattern), enabled
    flag, and (for Curves / Gradient Map) the control knots — so a clip is
    reprocessed only when the source or a parameter actually changes, not every
    paint. Ungraded clips skip the processed path entirely. The preview
    compositor draws the processed texture for image clips and the graded fill
    for color clips, composing with transform / opacity / crop / transitions
    exactly as the ungraded path did.
  - **Inspector**: a collapsing **Effects** section (titled with the rack count)
    above the appearance controls — an **Add effect** menu offering every
    adjustment kind, a **Clear** button, and per-entry an **enable** checkbox,
    **up / down** reorder, **remove**, and the kind's own parameter widgets
    (sliders / drag-values / color pickers per kind).
  - The rack persists in the `.reel` JSON (`#[serde(default)]` on both the
    `effects` field and each entry's `enabled`), so pre-rack projects load with
    an empty rack; the project round-trip now carries a three-effect rack with
    a disabled middle entry.
  - 18 new unit tests (empty / all-disabled rack no-op, disabled-skip, in-order
    application, known-pixel kernels for Invert / Exposure / Brightness / Levels
    / Black&White / saturation-zero desaturate, identity-curve no-op, HSL
    round-trip, color-stack alpha preservation, and `effects_hash` changing with
    params / disabled flag / curve knots while staying stable for equal racks;
    plus the project-level `effects` serde round-trip, the legacy-JSON empty-rack
    default, and the `has_grade` / `has_effects` flag wiring).

- **Adjustment layers** (Phase 3 "Adjustment layers") — an effect-carrying clip
  with no media of its own that applies its transform / opacity / crop to the
  clips on the tracks *beneath* it (Premiere's *Adjustment Layer* / Resolve's
  adjustment clip). Pure model + compositor wiring, no FFmpeg / decode:
  - Model: a new pure `project::adjustment` module. A `ClipSource::Adjustment`
    variant carries an `Adjustment { enabled }` (an `enabled` flag so a layer can
    be muted in place without losing its effects / keyframes; `#[serde(default)]`
    = `true`). It is media-less, so `ClipSource::source_len()` returns `None`
    (unbounded, like stills / colors / titles) and it gets a distinct desaturated
    slate timeline block. `Clip::is_adjustment` / `is_active_adjustment` report
    its kind.
  - **Effect composition**: a pure `Clip::with_adjustment(adj, t)` folds an
    adjustment layer's effect stack onto a content clip, returning an *effective*
    clip the existing `draw_clip` path renders unchanged. Both stacks are sampled
    at the playhead (so a keyframed layer animates) and composed about the shared
    frame center: scale multiplies, rotation adds, position offsets add, opacity
    multiplies, and crop fractions add per side (clamped to keep a sliver). The
    result carries static combined fields and no animation. A disabled (or
    non-adjustment) layer is a pass-through.
  - **Program resolution**: `Project::topmost_content_at` (topmost visible clip
    skipping adjustment layers), `program_at` → a `ProgramFrame { base,
    adjustments }` (the base content clip plus the active adjustment layers above
    it, lowest-track-first), and `effective_clip_at` (the base with every
    adjustment folded on). An adjustment over an empty frame applies to nothing
    (the frame stays empty), matching Premiere; layers below the base, on hidden
    tracks, or disabled don't apply.
  - **Compositor**: the preview's non-transition path now draws
    `effective_clip_at` instead of the topmost-only clip, so toggling or
    keyframing an adjustment layer changes the program monitor immediately.
    (Adjustment layers are not applied over an active transition this pass.)
  - **UI**: a **Title → New adjustment layer** menu item and an **Adjust** button
    in the media bin add an adjustment clip on the top track at the playhead and
    select it; the inspector gains an **Adjustment layer** section (an Enabled
    toggle + a note that the Transform / Opacity / Crop sections below apply to
    the clips beneath), reusing the existing effect-controls / keyframe surface.
  - The variant persists in the `.reel` JSON, and the project round-trip now
    carries a disabled adjustment layer with a non-trivial effect stack.
  - 10 new unit tests (unbounded source-len + distinct block color, active /
    disabled reporting, content-topmost skipping adjustments, `program_at`
    collecting above-base layers lowest-first / excluding below-base + disabled,
    adjustment-over-empty-frame, effect-stack combination math, disabled
    pass-through, stacked-adjustment fold, and the legacy default for `enabled`).

- **Captions / subtitles (SRT / VTT)** (Phase 6 "Captions / subtitles" / §9
  parity gap) — a sequence-level timed-text track burned into the program
  monitor and round-tripped as SubRip / WebVTT. Pure model + string IO, no
  FFmpeg / decode:
  - Model: a new pure `project::caption` module. A `Caption` cue is a
    `[start, end)` window (sanitized: `start >= 0`, non-collapsed) of text;
    `covers`/`duration`. A `Captions` track holds the ordered cue list plus a
    shared `CaptionStyle` (size as a fraction of frame height, text color,
    optional background plate, and a `CaptionPosition` of Top / Middle / Bottom);
    `active_at` returns the single cue showing at a time, `sort` keeps cues
    ordered. New `captions: Captions` field on `Project` (`#[serde(default)]`,
    so pre-caption `.reel` files load with an empty track).
  - **SRT / VTT IO**: `Captions::to_srt` / `to_vtt` serialize the cues
    (`HH:MM:SS,mmm` for SRT, `HH:MM:SS.mmm` for VTT); `load_str` parses *either*
    format with one tolerant parser — CRLF-agnostic, skipping a `WEBVTT` header,
    `NOTE`/`STYLE`/`REGION` blocks, SRT index lines, and malformed cue blocks.
    `parse_time` accepts `HH:MM:SS`, `MM:SS`, comma or dot millis.
  - `Project` ops: `add_caption` (sorted insert), `remove_caption` /
    `remove_caption_at` (cue under the playhead), `caption_at`, `has_captions`,
    `load_captions_str`.
  - **Preview burn-in**: the cue active at the playhead renders on top of the
    program frame — centered, wrapped to the title-safe width, anchored top /
    middle / bottom over an optional plate — exactly as it would on export.
  - **Captions menu**: add cue at the playhead (`⇧C`), remove the cue at the
    playhead, jump prev / next cue, and import / export SRT / VTT via file
    dialogs (export format chosen by the saved extension).
  - **Inspector**: a Captions section edits the cue under the playhead — its
    text (multi-line), start / end — plus the shared style (size, text color,
    position, background box). A timeline ruler strip shows a band per cue with
    the active one brightened; hover reads the window + text.
  - Tests: 11 unit tests — window sanitize, half-open `covers`, sorted insert /
    `active_at` / gap, remove-at-playhead, SRT & VTT timecode format, `parse_time`
    variants, SRT and VTT round-trips, a tolerant-parse case (CRLF + NOTE +
    garbage block), serde round-trip, and the legacy-JSON default.

- **Window menu — panel show/hide** (§8 UI/UX "Panel show/hide via a Window
  menu") — hide the workspace panels you aren't using to give the program monitor
  or timeline more room, à la Premiere's *Window* menu. No FFmpeg / decode:
  - Model: a new pure `app::layout` module. `PanelVisibility` tracks which of the
    three toggleable panels (`Panel::Bin` / `Inspector` / `Timeline`) are shown —
    the central program preview is always visible and isn't listed. Methods:
    `is_shown`, `set`, `toggle` (returns the new state), `show_all`, `reset`,
    `shown_count`, `all_shown`, `none_shown`; `Panel::ALL` drives the menu and
    shortcut map so the field set and UI never drift. Defaults to the full layout.
  - **Window menu**: a labelled checkbox per panel (with its `⌘1..3` shortcut),
    plus *Show all panels* (`⌘0`, disabled when already full) and *Reset layout*.
  - **Shortcuts**: `Cmd/Ctrl+1/2/3` toggle the bin / inspector / timeline;
    `Cmd/Ctrl+0` restores every panel (ignored while a text field has focus).
  - Hidden panels keep their state and hand their space to the neighbours (hiding
    the timeline grows the preview; the central preview always fills the rest).
    A title-bar badge appears when any panel is hidden — one click (or `⌘0`)
    restores the full workspace.
  - Tests: 7 unit tests for the visibility model (default-all-shown,
    per-panel-independent toggle, idempotent set, hide-everything, show-all /
    reset restore, unique labels).

- **Multi-select + marquee band-select + group edits** (Phase 2 "Selection &
  snapping polish" / §9 parity gap) — select and edit a *set* of clips together,
  all pure model logic with no FFmpeg required:
  - Model: a new pure `project::select` module. A `Selection` type is an
    ordered, de-duplicated set of clip indices with a distinguished **primary**
    (the last one touched — what the single-clip inspector edits): `single`,
    `select_one`, `set` (de-dup, order-preserving), `add` (insert / promote to
    primary), `toggle` (Ctrl/Cmd-click in/out), `contains`, `primary`,
    `indices`, `clear`, and index-churn re-mappers `remap_after_remove_many`
    (drop removed indices, shift survivors down by the count removed below each)
    and `clamp_to` so the selection survives lifts / ripple-deletes / razors.
  - Marquee query: `Project::clips_in_rect(t0, t1, track_lo, track_hi)` returns
    every clip whose timeline span (half-open overlap) and track fall inside a
    (time × track) band, with the time/track order normalized — a pure geometry
    test independent of any pixel mapping.
  - Group operations on `Project`: `move_clips(indices, dt, d_track)` shifts a
    set together as one collision-checked edit (the group is clamped as a whole
    so the earliest clip can't cross the origin and the set stays within
    `0..tracks`; the whole move is rejected if it would land any moved clip on an
    *unmoved* clip — moved clips never block each other; locked-track members are
    skipped). `lift_clips` / `ripple_delete_clips` remove a set (leaving gaps /
    closing them per track), returning the removed original indices for selection
    re-mapping; ripple-delete processes latest-first so each gap-close sees the
    already-rippled positions.
  - Timeline: **Ctrl/Cmd-click** or **Shift-click** toggles a clip in/out of the
    selection (plain click replaces); a **drag on empty lane space** draws a
    marquee rectangle that band-selects every clip it covers on release (ruler
    drags still scrub the playhead); dragging the body of an already-multi-
    selected clip **moves the whole group together** (snapping the grabbed clip's
    leading edge, promoting it to primary). Every selected block is outlined in
    the accent color (the primary bolder, secondary members thinner).
  - App: the single `selected: Option<usize>` is now a `Selection`; **Lift**
    (`Del`) and **Ripple delete** (`⇧Del`) act on the whole selection, the
    inspector edits the primary (with an "N clips selected · editing the primary"
    banner), and the transport readout follows the primary. **Cmd/Ctrl+A**
    selects all clips; **Escape** clears the selection. (`C` razor now ignores
    the Cmd/Ctrl chord so `Cmd+A` doesn't also razor.)
  - 13 new unit tests (select/toggle/primary, `set` de-dup + order, `add`
    promotion, `remap_after_remove_many`, `clamp_to`, `clips_in_rect` band +
    track-range + order-normalize + half-open edge, group move shift /
    origin+track clamp / unmoved-collision reject / intra-group non-collision /
    locked-skip, and `lift_clips` / `ripple_delete_clips` set removal + gap
    handling + reported indices).

- **Snapping / magnetic timeline** (Phase 2 / §9 parity gap) — a real snap model
  with a magnet toggle, no FFmpeg required:
  - Model: a new pure `project::snap` module. `Project::snap_candidates(time,
    exclude_clip)` gathers every snap target — all clips' starts/ends (excluding
    the dragged clip so it never snaps to itself), the playhead, the timeline
    origin, every sequence marker, and the work-area in/out points when set —
    returned sorted, de-duplicated, and stripped of negative/non-finite values.
    A free `project::snap_time(t, tol, candidates)` is the nearest-within-
    tolerance core (ties resolve to the closest, equal distances keep the first;
    a negative tolerance or empty list never snaps). Both are unit-tested
    independently of the UI.
  - Timeline: dragging a clip body or trimming an edge now snaps onto those
    targets when the magnet is on — the body drag snaps whichever of its leading
    or trailing edge lands closer, so a clip can butt its tail against the next
    cut. The pixel snap distance is converted to a seconds tolerance once at the
    current zoom and fed to the pure core. A bright accent **guide line** is
    drawn through the lanes at the point an edge snapped to this frame, so the
    pull is visible (matching Premiere / Resolve).
  - UI: a **Snap** toggle (magnet icon) in the timeline transport row beside
    Ripple, plus an **`S`** keyboard shortcut to toggle it; on by default. (Razor
    is now **`C`** only — `S` was freed for the magnet, the Premiere/Resolve
    binding.) The toggle is app UI state (not persisted to `.reel`).
- **Wipe & push transitions** (Phase 3) — directional geometric transitions
  alongside the existing cross-dissolve / dip-to-color, computed as pure model
  geometry rendered by the preview compositor (no FFmpeg required):
  - Model: a new `WipeDir` (`Left` / `Right` / `Up` / `Down` — the edge the
    *incoming* clip enters from, Premiere's convention) and two new
    `TransitionKind` variants, `Wipe(WipeDir)` and `Push(WipeDir)`. A new pure
    `Transition::geometry(t)` returns a `TransitionGeometry` describing how the
    two clips are arranged at the playhead: `Blend` (the existing
    alpha-stacked dissolve / dip, carrying the same `weights`), `Wipe`
    (the fraction-rect of the incoming clip revealed so far, growing from its
    entering edge), or `Push` (the fractional `(dx, dy)` offsets both clips
    slide by, the incoming entering as the outgoing leaves the opposite edge).
    `weights` reports a hard mid-span cut for the geometric kinds so any
    weight-only caller still hands off; `WipeDir::is_horizontal`/`label` are
    pure helpers.
  - Preview compositor: `paint` now switches on `geometry` instead of `weights`.
    A wipe draws the outgoing clip in full, then the incoming clip through a
    painter clipped to the revealed sub-rect (a new `frac_rect` maps the
    fraction-rect to screen space); a push slides both clips by passing an
    `extra_offset` screen translation into `draw_clip` (the shared clip-drawing
    path gained that parameter, `Vec2::ZERO` everywhere else). Both compose with
    each clip's own transform / opacity / crop exactly as the blend path does.
  - UI: the **Edit → Add transition** menu gains **Wipe** and **Push** submenus
    (one entry per direction), reusing the existing cut-nearest-the-playhead
    insertion (clamped to the adjoining clips, playhead parked on the cut). The
    timeline transition overlay now reads the direction: a single sweep line
    along the travel axis for wipe / push (vs the crossing diagonals for a
    dissolve / dip), with the kind shown in the hover tooltip. Two new icons
    (split-square for wipe, arrow-square for push).
  - The variants persist in the `.reel` JSON (`TransitionKind` already carried
    `#[serde(default)]` semantics through the transition list), and the project
    round-trip now exercises a right wipe and an up push on adjacent cuts.
  - 8 new unit tests (left-wipe reveal growing from the edge, all four wipe
    directions anchored to their entering edge at the midpoint, push offsets
    sliding both clips one frame apart, vertical push travelling on y, geometric
    kinds reporting `Wipe`/`Push` geometry + a hard-cut `weights`, dissolve / dip
    geometry still matching `weights`, `WipeDir` metadata, and a wipe/push JSON
    round-trip).

- **Image-sequence clip source** (Phase 1) — a numbered run of still frames
  (e.g. `frame_0001.png`, `frame_0002.png`, …) imported and played back as one
  time-varying clip, with no FFmpeg / codec dependency (each frame is an
  ordinary still decoded through the existing texture cache):
  - Model: a new `ClipSource::ImageSequence(ImageSequence)` variant carrying an
    `ImageSequence { dir, prefix, suffix, pad, start_index, count, fps }`. This
    is the **first bounded source** in Reel — `ClipSource::source_len()` now
    returns `Some(count / fps)` for it (stills/colors/titles stay unbounded), so
    the whole source-aware edit machinery (source in/out, trim, slip, slide,
    roll, speed/retime) constrains against a *real* finite source for the first
    time. Pure helpers: `ImageSequence::new`/`sanitize` (count `>= 1`, fps
    clamped to `MIN_SEQ_FPS..=MAX_SEQ_FPS`), `source_len`,
    `frame_ordinal_at`/`frame_index_at` (the frame the playhead lands on,
    clamped to the last frame at/after the tail), `path_for_index`/`path_at`
    (formats `dir/{prefix}{index:0pad}{suffix}`), `pattern_label`, and
    `from_paths` (recognizes a clean contiguous numbered run from a multi-select
    — shared directory + prefix/suffix + equal digit width + no gaps — and
    rejects singletons, gaps, mismatched padding, and split directories).
  - Preview compositor: an image-sequence clip samples the frame the playhead
    lands on via the clip's speed-aware `source_time` mapping, decodes that
    frame through the same texture cache as a still, and draws it with the
    identical aspect-fit / crop / transform / opacity / transition path (the
    image-drawing code is now a shared `draw_image_path` helper used by both
    `Image` and `ImageSequence`), so a sequence slows / speeds / reverses and
    composites exactly like a still does — only the sampled frame changes.
  - Import: **File → Import image sequence…** and a **Seq** button in the media
    bin open a multi-select dialog and add the recognized run as a single bin
    item (named with its pattern + frame count), played at the project's frame
    rate; a selection that isn't a clean numbered run is dropped (logged) rather
    than guessed. The bin shows a distinct stacked-images glyph, and the
    timeline draws a teal-green block distinct from stills / colors / titles.
  - Inspector: image-sequence clips get an **Image sequence** section — the
    pattern label, the frame index range + count, an editable **Rate** (fps)
    drag-value, the recomputed source **Length**, and the **Current** frame
    index the playhead samples. Changing the rate rescales the source length, so
    the section re-clamps the clip's duration to never read past its source.
  - The variant persists in the `.reel` JSON (`fps` is `#[serde(default)]` =
    30fps), and the project round-trip now carries a non-trivial image-sequence
    clip (offset source-in, trimmed duration, 24fps).
  - 7 new unit tests (bounded `source_len` + clamped default duration + distinct
    block color, count/fps sanitize incl. NaN, frame-ordinal/index mapping +
    end/negative clamp, padded + bare path formatting, `from_paths` recognition,
    `from_paths` rejection of singletons/gaps/padding/dirs, and a bounded-source
    `trim_out` clamp + source-continuous split).

- **Track controls: enable / lock / solo + add / remove tracks** (Phase 2) —
  per-track state generalizing the flat `tracks: usize` into addressable video
  tracks, all pure model data honoured by the compositor and edit ops (no
  FFmpeg required):
  - Model: a new `Track { name, enabled, locked, solo }` and a
    `Project.track_meta: Vec<Track>` kept exactly one-per-track by
    `sync_track_meta` (truncate extras / append `V{n}` defaults), called from
    `Project::new` and after any track-count change. Pure operations —
    `track`, `any_solo`, `is_track_visible` (enabled *and* (nothing soloed or
    itself soloed)), `is_track_locked` / `is_clip_locked`, `toggle_track_*`
    (enabled / locked / solo), `add_track` (append on top), and `remove_track`
    (delete the track's clips + their transitions, shift higher clips/meta
    down, refuse the last track).
  - Compositor: `topmost_at` now skips clips on hidden tracks, and the preview's
    transition path falls through to the topmost visible clip when the
    transition's track is hidden — so toggling a track's eye or soloing changes
    what the program monitor shows immediately.
  - Edit-op lock honouring: `razor_all_at` skips clips on locked tracks, and the
    app drops any move / trim / roll / slide / lift / ripple-delete / slip /
    speed / reverse that targets a locked-track clip (selection still updates so
    the inspector can show it), so a finished track can't be disturbed.
  - Timeline: each track header (in a widened label strip) shows the track name
    above an eye / lock / solo / remove glyph button row (accent = active,
    dimmed name on a disabled track), plus an "add track" button in the ruler
    corner; clicks report a `TrackAction` the app applies.
  - `track_meta` persists in the `.reel` JSON (all `#[serde(default)]`, with
    `enabled` defaulting to `true`), so older projects load all-enabled and
    unlocked; the round-trip now exercises a renamed/locked, a disabled, and a
    soloed track, and a legacy-file test backfills defaults via
    `sync_track_meta`.
  - 12 new unit tests (meta sync grow/shrink, defaults, disabled + solo
    visibility, `topmost_at` skipping hidden tracks, per-clip lock reporting,
    lock-aware razor, add-on-top, remove-shifts-down + last-track/out-of-range
    refusal + transition cleanup, and JSON round-trip + legacy default-fill).

- **Bézier ease on clip keyframes** (Phase 3) — keyframe segments can now ease
  along a cubic-Bézier curve instead of only ramping linearly or holding, all
  pure model math sampled by the preview compositor (no FFmpeg required):
  - Model: a new `Ease { x1, y1, x2, y2 }` type — two CSS `cubic-bezier()`-style
    control handles between the implied endpoints `(0,0)`→`(1,1)`. `Ease::new`
    sanitizes only the *time* (x) handles into `0..=1` (so the curve stays a
    well-defined function of time) while leaving the *value* (y) handles free
    for overshoot / anticipation; `EASE_IN` / `EASE_OUT` / `EASE_IN_OUT` presets
    match the CSS curves and the default is symmetric ease-in-out. `Ease::eval`
    maps a normalized segment time to eased progress by solving `bezier_x(u)=x`
    (Newton-Raphson with a bisection fallback) then evaluating `bezier_y(u)`.
  - `Interp` gains an `Ease(Ease)` variant alongside `Linear` / `Hold`, and a
    single `Interp::shape(f)` now defines the segment blend factor (identity for
    linear, `0` for hold, the Bézier curve for ease). `Channel::sample` routes
    every segment through `shape`, so any keyframed property (position x/y,
    scale, rotation, opacity) eases automatically wherever its leaving key is an
    `Ease` — the preview animation, scrubbing, and all existing sampling paths
    pick it up unchanged.
  - Inspector: the per-key interpolation button now cycles
    Linear → Hold → Ease In → Ease Out → Ease In-Out → Linear (distinct labels
    per ease preset), and an `Ease` key reveals four compact drag-values for its
    cubic-Bézier handles (custom curves route through `Ease::new`, clamping the
    time handles).
  - Eased keys persist in the `.reel` JSON (`Interp` is `#[serde(default)]` per
    keyframe, so older projects load as `Linear`), and the project round-trip
    now carries a custom-eased opacity segment.
  - 6 new unit tests (endpoint pinning + monotonicity + range for every preset,
    ease-in lag / ease-out lead / symmetric default, Newton-Raphson inversion of
    the Bézier x-component, `Ease::new` clamping time-only, a channel segment
    shaped by an ease, and `Interp::shape` dispatch).

- **Clip speed / retime** (Phase 3) — per-clip playback speed (slow / fast /
  reverse), decoupling timeline duration from source consumption, modeled as
  pure interval math with no FFmpeg required:
  - Model: each `Clip` gains a `speed` factor (`1.0` = 100% normal; `< 1` slow
    motion, `> 1` fast, *negative* = reverse), clamped to
    `MIN_SPEED_MAG..=MAX_SPEED_MAG` by `sanitize_speed` (sign preserved). The
    source a clip consumes is now `duration * |speed|` (`source_consumed`), so
    `source_out` and every source-bound edit account for speed; `source_time`
    maps a clip-local timeline time onto the source time the playhead samples
    (forward or reversed, clamped to the window — the hook for the future
    decode path). `set_speed` retimes a clip the Premiere "Speed/Duration" way:
    it preserves the source window (`source_in` + consumed source) and
    recomputes the timeline `duration`, so speeding a clip up shortens it and
    slowing it down lengthens it; flipping only the sign reverses in place
    without changing length. `is_reversed`/`is_retimed` report state, and a
    retime now counts toward `has_effects` (while `reset_appearance` leaves
    speed alone, since it changes clip length).
  - Edit-op correctness under speed: `split_clip` advances the right half's
    `source_in` by the source the left half consumed (`left_dur * |speed|`), so
    a cut stays frame-continuous at any speed (forward or reverse); `clamp_to_source`
    and the source bounds in `trim_in`/`trim_out`/`ripple_trim_in`/
    `ripple_trim_out`/`roll`/`slide` all scale by `|speed|` (timeline seconds =
    source seconds / |speed|), so they never run a clip past its source.
  - Inspector: a new **Speed** section — a logarithmic speed-magnitude slider
    (% of normal), a **Reverse** toggle, a section reset (back to 100%), and a
    read-only recomputed clip **Duration**; edits route through `set_speed` so
    the source window is preserved.
  - Transport readout: the selected clip now shows the source time the playhead
    samples (`@{t}s`, speed-aware) and its speed factor (`rev 200%`) when
    retimed, alongside the existing `src in→out`.
  - **Edit → Speed / duration** submenu (50% / 100% / 200% / 400% presets +
    Reverse) and an `R` keyboard shortcut reverse the selected clip; the
    timeline draws a small speed pill (e.g. `200%` / `-50%`) on a retimed clip's
    block.
  - `speed` persists in the `.reel` JSON (`#[serde(default)]` = 1.0, so older
    projects load at normal speed), and the project round-trip now exercises a
    reversed double-speed clip.
  - 8 new unit tests (speed defaults, consumed-source scaling, set-speed window
    preservation + duration recompute, reverse keeps duration, magnitude clamp
    with sign, forward/reversed `source_time` mapping, retime/has-effects flags,
    and source-continuous split under speed).

- **Keyframe animation of transform / opacity** (Phase 3) — the per-clip
  transform and opacity can now animate over the clip's life, sampled in the
  preview compositor with no FFmpeg required:
  - Model: a generic, pure animation layer — `Interp` (`Linear`/`Hold`),
    `Keyframe { time, value, interp }` (clip-relative seconds), and a `Channel`
    (a time-sorted keyframe list) with `sample` (clamp-hold before the first /
    after the last key; linear or step between), `set_key` (sorted insert,
    in-place replace within `KEY_PICK_DIST` preserving interp, negative-time
    clamp), `key_index_near`/`remove_key_near`, and `next_key_after`/
    `prev_key_before` navigation. Each `Clip` gains a `ClipAnim` carrying one
    channel per animatable scalar (position x/y, scale, rotation, opacity) plus
    `is_animated`, generic `channel`/`channel_mut` over an `AnimProp`, and
    cross-channel `next/prev_key_*`. `Clip::local_time` maps a timeline time to
    clip-relative time (clamped to the clip span); `sampled_transform`/
    `sampled_opacity` return the per-component animated value where keyed and
    the static field otherwise.
  - Preview compositor: `draw_clip` now samples the transform/opacity at the
    clip-relative playhead time instead of reading the static fields, so an
    animated clip moves/scales/rotates/fades during playback and scrubbing.
    Animation composes with crop, transitions, and per-clip opacity exactly as
    the static path did.
  - Inspector: each animatable property gets a keyframe **stopwatch** (enable
    animation — captures the current value as a key at the playhead; disable —
    freezes the sampled value back to the static field and clears the keys), a
    **diamond** to add/update or remove the key on the playhead, and an interp
    (`Linear`/`Hold`) toggle for the key under the playhead; the value
    slider/drag shows the *sampled* value and writes a key at the playhead when
    the channel is animated. The header gains prev/next-keyframe arrows that jump
    the playhead across all of the clip's channels, and "Reset all" /
    per-section resets clear keyframes too. Position X/Y are now separate rows so
    each animates independently.
  - Timeline: an animated clip shows a small accent diamond badge (vs the dot
    for a static override).
  - Keyframes persist in the `.reel` JSON (all `#[serde(default)]`, so older
    projects load un-animated), and the project round-trip now exercises keyed
    scale (with a `Hold` key) and opacity channels.
  - 13 new unit tests (empty/single-key/linear/hold sampling, sorted insert +
    negative clamp, in-place replace keeping interp, remove + strict
    next/prev navigation, per-component `sampled_transform`, opacity fallback,
    `local_time` clamping, animation flags + reset, cross-channel navigation,
    and static-value reads).

- **Title / text clips** (Phase 6) — the first vector text clip type, rendered
  in the preview compositor through egui's text layout (no FFmpeg/fonts beyond
  the bundled UI font):
  - Model: a new `ClipSource::Title(Title)` variant carrying a `Title { text,
    size, color, align, background }` — `size` is a fraction of the comp frame
    height (so titles scale with the canvas), `align` is a `TitleAlign`
    (`Left`/`Center`/`Right`), and `background` is an optional flat box for
    lower-thirds. `Title::new`/`sanitize` (size clamp) are pure; titles report
    `source_len() == None` (unbounded, like stills/colors) and get a distinct
    violet timeline-block color.
  - Preview compositor: a title lays out its (multi-line, frame-wrapped) text at
    the scaled font size, horizontally aligned and vertically centered, with the
    transform's position offset and the clip's opacity applied (and the optional
    background box drawn behind it). Titles compose with the existing
    transform/opacity and transition cross-fade paths; rotation is not applied to
    text in this pass.
  - Inspector: title clips get a dedicated **Title** section above the
    transform/opacity/crop controls — a multiline text editor, a logarithmic
    size slider (% of frame height), an RGBA text color picker, an
    `Left/Center/Right` alignment toggle, and a background-box toggle + color.
  - Creation: a **Title** menu (New title…) and a **Title** button in the media
    bin both add a fresh "Title N" clip on the top track at the playhead and
    select it for immediate editing; titles need no media import.
  - Titles persist in the `.reel` JSON; the styling fields are
    `#[serde(default)]`, so a title saved with only its `text` reloads white,
    centered, at the default size with no background.
  - 5 new unit tests (title defaults, size-clamp sanitize, unbounded
    source-len + distinct block color, alignment labels/default, and a
    text-only legacy-title deserialization), and the project round-trip now
    carries a non-trivial title clip (left-aligned, colored, with a background).

- **Sequence markers + work-area in/out range** (Phase 2) — annotation and
  navigation cues plus a ranged work area, all pure model data (no FFmpeg):
  - Model: a `Marker { time, label, color }` and a sorted `Project.markers`
    list, plus `in_point`/`out_point: Option<f32>` work-area points. Pure
    operations — `add_marker` (insert sorted, clamp `time >= 0`),
    `remove_marker_near` (closest within `MARKER_PICK_DIST`),
    `next_marker_after`/`prev_marker_before` (strict-after/-before navigation),
    `set_in`/`set_out` (clamp to `0..=duration`, keep `in <= out` by pushing the
    other endpoint), `clear_in_out`, `work_range` (defaults to the full
    timeline), and `has_work_range`.
  - Timeline overlay: a colored pin on the ruler at each marker with a faint
    guide line down the lanes, clickable to jump the playhead and hover for the
    label; the work area shades the out-of-range lanes and draws bracket handles
    plus an accent band on the ruler.
  - Transport + **Markers menu**: add/remove marker, go to previous/next marker,
    mark in/out, and clear the range; ruler readout of the active in→out range.
    Keyboard: `M` add marker / `⇧M` remove nearest, `I`/`O` mark in/out,
    `↑`/`↓` jump between markers.
  - Markers and the in/out points persist in the `.reel` JSON (all
    `#[serde(default)]`, so older projects load with no markers and the full
    range), with the marker color defaulting to the suite amber.
  - 8 new unit tests (sorted insert + negative-time clamp, nearest-pick remove,
    next/prev navigation, work-range default, in/out clamp + ordering both ways,
    clear), and the project round-trip now exercises markers + in/out points.

- **Per-clip transform / opacity / crop + Effect Controls inspector**
  (Phase 3) — the first non-destructive effect stack every clip carries,
  computed in the preview compositor with no FFmpeg required:
  - Model: each `Clip` gains a `Transform` (normalized `position` offset,
    uniform `scale`, clockwise `rotation`), an `opacity` (`0..=1`), and a
    `Crop` (per-side fractions). All are pure types with `sanitize`
    (scale/rotation clamp + wrap, crop sliver guarantee), `uv`, and
    `is_identity`/`is_none` helpers, plus `Clip::effective_opacity`,
    `has_effects`, and `reset_appearance`.
  - Preview compositor applies the stack: crop cuts the source in source
    space, scale/position rotate the result inside the comp frame (images
    rotate through an `egui::Mesh` textured quad with cropped UVs; colors use a
    rotated/scaled quad), and the clip's alpha is multiplied by its opacity.
    Transition cross-fade alpha now composes with each clip's own opacity.
  - New right-hand **Effect Controls** inspector panel edits the selected
    clip's Transform (x/y drag-values, log scale slider in %, rotation slider),
    Opacity (% slider), and Crop (per-side % sliders), each with a group reset
    and a global "Reset all"; empty when nothing is selected.
  - Timeline shows a small accent dot on any clip carrying an override.
  - All fields persist in the `.reel` JSON (`#[serde(default)]`, so older
    projects load with the identity transform / full opacity / no crop).
  - 7 new unit tests (transform identity + sanitize clamp/wrap, crop
    uv/sanitize sliver, effective-opacity clamp, effects flag + reset, plus the
    project round-trip now exercises a non-trivial transform/opacity/crop).

- **Transitions at cuts** (Phase 3) — cross-dissolve and dip-to-color
  (black/white) computed in the preview compositor, no FFmpeg required:
  - A `Transition` model centered on the cut between two edge-adjacent clips on
    a track, with a `0→1` progress across its span and pure `weights(t)` blend
    math (`Transition`, `TransitionKind`, `Project::add_transition`,
    `find_cut`, `active_transition`, `drop_transitions_for_clip`).
  - The program preview now blends the two clips through a shared `draw_clip`
    alpha path: cross-dissolve stacks the incoming clip over the outgoing by
    weight; dip-to-color shows one clip with a flat color dipped over it
    (peaking at the midpoint).
  - **Edit → Add transition** submenu (Cross dissolve / Dip to black / Dip to
    white) inserts a transition at the cut nearest the playhead on the selected
    clip's track, clamped so it never overruns either clip; the playhead parks
    on the cut so the blend is immediately visible.
  - Timeline overlay: each transition draws a translucent accent band with
    crossing diagonals at its cut, with a hover tooltip showing kind + duration.
  - Transitions persist in the `.reel` JSON (`#[serde(default)]`, so older
    projects load unchanged), and are dropped/rebased automatically when a
    referenced clip is lifted or ripple-deleted.
  - 9 new unit tests (cross-dissolve weight sum, dip color peak, span/progress,
    add-transition clamping/replacement, active-transition lookup, dangling-
    transition cleanup + index rebasing).

## [0.0.1] - 2026-06-06

### Added

- **Multitrack timeline NLE scaffold** (Phase 0) — a real, editable
  stills-only non-linear editor on `eframe`/`egui` (glow), sharing the suite's
  `prism-core` and `prism-io` crates:
  - Edit model: `Project { width, height, fps, duration, tracks, clips, bin }`,
    `Clip { name, source, track, start, duration }`, and
    `ClipSource = Image(path) | Color(rgba)`, with `covers(t)` and
    `topmost_at(t)` (highest track wins) driving the preview.
  - Timeline: a seconds ruler, N stacked track lanes (V1, V2, …), draggable clip
    blocks (move horizontally = `start`, vertically = track), edge trims, a
    draggable playhead, ruler/lane scrubbing, and snap to clip edges + playhead.
  - Transport: play/pause (Spacebar), real-`dt` advance, loop at duration,
    jump-to-start/end, and a live timecode + frame readout.
  - Preview: the topmost active clip at the playhead, aspect-fit into the comp
    frame (color fills; images letterboxed from a cached decoded texture; black
    when empty).
  - Media bin: import images (`rfd` + `prism_io::load_image`), add color clips,
    and drop bin items onto the top track at the playhead.
  - Menus, the Prism dark theme (warm amber NLE accent), and Phosphor icons;
    `.reel` project save as `serde_json`.

- **Source in/out + ripple/roll/slip/slide editing tools** (Phases 1–2) — the
  daily-driver edit surface, modeled as pure, unit-tested operations on the
  project:
  - Source in/out: `Clip` gains `source_in`, decoupling the source window from
    timeline placement, bounded by `ClipSource::source_len`; the transport shows
    the selected clip's `src in→out` and `.reel` files round-trip (with legacy
    files defaulting `source_in` to 0).
  - The four trim tools — `roll`, `slip`, `slide`, `ripple_trim_in`,
    `ripple_trim_out` — plus `split_clip`/`razor_all_at` (source-continuous),
    `lift`, `ripple_delete`, and `close_gap_after`, wired to edge/body drags
    (Alt = roll/slide, Ripple toggle = ripple-trim) and the Edit menu.
  - Keyboard: razor (C/S), lift (Del), ripple-delete (⇧Del), slip one frame
    (`[`/`]`).
  - 28 unit tests covering ripple math, split source-continuity, roll
    length-preservation, slip/slide clamping, and `.reel` JSON round-trips.

[Unreleased]: https://github.com/prism-suite/reel/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/prism-suite/reel/releases/tag/v0.5.0
[0.4.0]: https://github.com/prism-suite/reel/releases/tag/v0.4.0
[0.3.0]: https://github.com/prism-suite/reel/releases/tag/v0.3.0
[0.2.0]: https://github.com/prism-suite/reel/releases/tag/v0.2.0
[0.1.0]: https://github.com/prism-suite/reel/releases/tag/v0.1.0
[0.0.1]: https://github.com/prism-suite/reel/releases/tag/v0.0.1
