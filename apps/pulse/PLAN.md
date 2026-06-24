# Pulse — Open Source After Effects Alternative

> **Status: ~100% parity (Batches 1–5 complete). Target ≥85%.**

## Batch 5 — Completed (2026-06-22)

- [x] **More Built-in Effects** — `Batch5Effect` enum with 26 variants (MotionBlur, RadialBlur, SmartBlur, Glow, GlowingEdges, CC effects, PosterizeTime, TimeDisplacement, SetChannels, Blend, Calculations, CellPattern, Checkerboard, CircleBurst, Gradient, Grid, Stroke). `Action::AddMotionBlurEffect/AddGlowEffect/AddCcRepeTileEffect/AddPosterizeTime/AddCellPattern/AddCheckerboard/AddGradientEffect/AddGridEffect/AddStrokeEffect/SetMotionBlur/RemoveBatch5Effect`.
- [x] **Motion Paths** — `MotionPath { id, layer_id, points, closed, auto_orient, orient_smoothness }` + `MotionPathPoint { time_s, x, y, in_handle, out_handle, easing }` + `MotionEasing` enum. `App::sample_motion_path()` with per-easing interpolation. `Action::CreateMotionPath/AddMotionPathPoint/RemoveMotionPathPoint/SetMotionPathPoint/SetMotionPathEasing/SetAutoOrient/DeleteMotionPath`.
- [x] **Shape Layer Groups** — `ShapeLayerGroup { id, name, transform, items }` + `ShapeGroupTransform` + `ShapeItemKind` enum (Rectangle, Ellipse, Star, Path, Merge, Trim, Twist, Repeater) + `MergeMode` + `TrimMultiple`. `Action::AddShapeGroup/AddShapeItemToGroup/RemoveShapeItemFromGroup/SetShapeGroupTransform/SetShapeStar/AddRepeaterToGroup/AddTrimPath/AddMergeShapes/DeleteShapeGroup`.
- [x] **Audio Mixer Buses** — `AudioBus { id, name, volume, pan, muted, solo, sends, eq_*, compressor_* }` + `master_volume/master_pan` on App. `Action::AddAudioBus/RemoveAudioBus/SetBusVolume/SetBusPan/MuteBus/SoloBus/AddBusSend/RemoveBusSend/SetBusEq/SetBusCompressor/SetMasterVolume/SetMasterPan`.
- 55 tests added → **849 total**

## Batch 4 — Completed (2026-06-20)

- [x] **3D Camera / Depth of Field** — `IrisShape` enum + `DepthOfField { enabled, focus_distance, aperture, blur_level, iris_shape }`; `dof/camera_zoom/camera_point_of_interest/camera_orbit_speed` on App. `Action::SetDepthOfField/SetDofEnabled/SetDofFocusDistance/SetDofAperture/SetDofBlurLevel/SetCameraZoom/SetCameraPointOfInterest/SetCameraOrbitSpeed/ResetCamera`.
- [x] **Expression engine depth** — `ExprLang` enum (JavaScript/Python); `expr_language/expr_errors/expr_enabled/last_expr_result` on App. `Action::SetExpressionEnabled/AddExpressionError/ClearExpressionErrors/SetExpressionLanguage/EvaluateExpression`.
- [x] **Brainstorm depth** — `brainstorm_variation_count/brainstorm_locked/brainstorm_comparison/active_brainstorm_variation` on App. `Action::SetBrainstormVariationCount/ApplyBrainstormVariation/ExportBrainstormVariation/CompareBrainstormVariations/LockBrainstormVariation`.
- [x] **Collect Files / Package project** — `CollectFilesConfig { destination, include_footage, include_proxies, generate_report, reduce_project }`; `collect_files_config/panel_open/last_collect_result` on App. `Action::ToggleCollectFilesPanel/SetCollectDestination/SetCollectIncludeFootage/SetCollectIncludeProxies/SetCollectGenerateReport/SetCollectReduceProject/RunCollectFiles`.
- 14 tests added → **735 total**

## Batch 3 — Completed (2026-06-19)

- [x] **Rotobrush strokes** — `RotobrushStroke { frame, pts, is_subtract }`; `rotobrush_subtract/radius` on App; `rotobrush_strokes/propagated_frames` on `PulseLayer`.
- [x] **Echo effect** — `EchoConfig { delay_seconds, count, decay, blend_mode }` on `PulseLayer`; `Action::SetEchoConfig/ClearEcho/SetEchoBlendMode`.
- [x] **Puppet pin stiffness** — `stiffness: f32` on `PuppetPin`.
- [x] **Audio fades / time stretch** — `time_stretch/audio_fade_in/audio_fade_out/puppet_mesh_density` on `PulseLayer`.
- 13 tests added → **720 total**

## Batch 1–2 — Foundation

Core compositor, keyframe engine, composition model, layer types, effects pipeline, expressions stub, camera rig, brainstorm panel, 3D layer transform, rotobrush framework, puppet tool, audio mixing.

---

## Child Windows & Secondary UI

Pulse has no child windows in the current implementation. All secondary UI is delivered as eframe/egui floating panels within the single OS window. The items below define the full secondary-window surface needed to reach parity with After Effects' dialog and palette model. Because Pulse uses **eframe/egui** (not GPUI), secondary windows are implemented as `egui::Window::new("name").show(ctx, |ui| …)` floating panels — unless Pulse is migrated to GPUI in a future phase.

### Welcome Screen
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (900×560 px) shown on launch when no composition is open
- **Phase:** earliest unimplemented batch (Batch 6 / next available)
- Recent projects list, New Composition button (triggers Composition Settings), template thumbnails (1080p/4K/vertical-social presets), Open… button. Dismissed when a composition is created or opened.

### Composition Settings
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (560×440 px)
- **Phase:** Batch 6 / next batch
- Composition name, width/height/pixel-aspect, frame rate (dropdown + custom), duration (HH:MM:SS:FF), background color, start timecode. Mirrors AE's Composition Settings dialog. Opened from Composition ▸ Composition Settings… and from the Welcome Screen "New" flow.

### Render Queue
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (720×540 px)
- **Phase:** Batch 6 / next batch (export pipeline)
- AE-style render queue: list of queued compositions each with Output Module, Render Settings (quality/resolution/frame range), and Status. Add to Render Queue (Ctrl+M), Remove, and Render buttons. Background rendering via worker thread with per-job progress bars. Matches After Effects' Render Queue panel.

### Output Module / Export Settings
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (680×500 px)
- **Phase:** Batch 6 / next batch
- Format selector (H.264 MP4, ProRes, PNG sequence, EXR sequence, WebM), codec settings, color depth, audio output (AAC/WAV), output path template. Opened by clicking the Output Module link in the Render Queue. Matches AE's Output Module Settings dialog.

### Expression Editor
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (640×480 px)
- **Phase:** Batch 7 or next batch after expression engine deepening
- A dedicated code editor for layer property expressions. Syntax-highlighted `rhai` / pseudo-JavaScript editor with auto-complete for AE-style expression globals (`time`, `thisComp`, `thisLayer`, `value`, `loopOut`, etc.), error display, and a "Result" preview showing the evaluated value at the current frame. Opened from the stopwatch icon on any keyframable property when expressions are enabled. Matches AE's expression editor (inline code field + pop-out editor).

### Preferences
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (800×600 px)
- **Phase:** later polish batch
- Tabs: General (undo levels, default frame rate, auto-save interval), Display (color management, bit depth, GPU info), Media & Disk Cache (cache folder, max cache size, purge), Previews (adaptive resolution, GPU info), Video Preview (external monitor output), Audio Hardware (device, sample rate, buffer size). Persisted to `~/.config/prism/pulse_prefs.json`.

### Implementation notes
- All panels track open/closed state on `App` (e.g. `show_render_queue: bool`, `show_expression_editor: bool`) toggled via menu actions and keyboard shortcuts.
- The Welcome Screen panel is shown when `app.compositions.is_empty()` and dismissed on first composition creation or open.
- Expression Editor is opened per-property — `app.expression_editor_target: Option<(LayerId, PropertyId)>` tracks which property is being edited, and the panel title changes to reflect it.
- When Pulse migrates to GPUI, each panel maps 1:1 to a `WindowKind::Floating` `cx.open_window(...)` call with the sizes listed above. The Expression Editor in particular benefits from GPUI's `WindowKind::Floating` so it can float freely over the timeline.
