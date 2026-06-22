# Pulse — Open Source After Effects Alternative

> **Status: ~72% parity (Batches 1–5 complete). Target ≥85%.**

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
