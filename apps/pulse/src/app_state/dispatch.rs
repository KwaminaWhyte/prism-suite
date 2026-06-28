//! The [`App::dispatch`] action router — the big `match` that sends each
//! [`Action`] to its domain `apply_*` helper. Split out of `mod.rs` to keep
//! that file under the 1000-line limit; `App::apply` calls this after taking the
//! undo snapshot. Same `App` impl, same single-choke-point contract.

use super::{App, Action};

impl App {
    /// Route an [`Action`] to its domain handler. Called by [`App::apply`] after
    /// the undo snapshot is taken; never call this directly (it skips the undo
    /// guard).
    pub(super) fn dispatch(&mut self, action: Action) {
        match action {
            // --- Keyframes ---
            a @ (Action::MoveKeyframe { .. }
            | Action::ToggleGraph
            | Action::ToggleGraphProp(_)
            | Action::ClearGraphProps
            | Action::SetInterp { .. }
            | Action::MoveKeyframeXY { .. }
            | Action::GizmoKeys { .. }) => self.apply_keyframes(a),

            // --- Effects chain ---
            a @ (Action::ToggleEffectBrowser
            | Action::SetEffectQuery(_)
            | Action::AddEffect(_)
            | Action::RemoveEffect { .. }
            | Action::SetEffectParam { .. }
            | Action::AddGpuiEffect(_)
            | Action::RemoveGpuiEffect(_)
            | Action::SetMosaicBlock { .. }
            | Action::SetChromaOffset { .. }
            | Action::SetEffectIntensity { .. }
            | Action::SetVignetteRadius { .. }
            | Action::ToggleGpuiEffectExpand(_)
            | Action::SetColorBalanceShadows { .. }
            | Action::SetColorBalanceMidtones { .. }
            | Action::SetColorBalanceHighlights { .. }
            | Action::SetLevelsInBlack { .. }
            | Action::SetLevelsInWhite { .. }
            | Action::SetLevelsGamma { .. }
            | Action::SetLevelsOutBlack { .. }
            | Action::SetLevelsOutWhite { .. }
            | Action::SetHueShift { .. }
            | Action::SetSaturation { .. }
            | Action::SetLightness { .. }
            | Action::SetNoiseFrequency { .. }
            | Action::SetNoiseEvolution { .. }
            | Action::AddDisplacementMap { .. }
            | Action::SetDisplaceScale { .. }
            | Action::AddTextAnimator(_)
            | Action::RemoveTextAnimator(_)
            | Action::SetTextAnimatorRange { .. }
            | Action::SetTextAnimatorOffsetX { .. }
            | Action::SetTextAnimatorOffsetY { .. }
            | Action::SetTextAnimatorRotation { .. }
            | Action::SetTextAnimatorScale { .. }
            | Action::SetTextAnimatorOpacity { .. }
            | Action::SetShapeTrimPaths { .. }
            | Action::ClearShapeTrimPaths(_)
            | Action::AddShapeRepeater(_)
            | Action::RemoveShapeRepeater(_)
            | Action::SetRepeaterCopies { .. }
            | Action::SetRepeaterOffset { .. }
            | Action::SetRepeaterRotation { .. }
            | Action::SetRepeaterScale { .. }
            | Action::SetRepeaterOpacity { .. }
            | Action::AddLumetriColor(_)
            | Action::RemoveLumetriColor(_)
            | Action::SetLumetriParam { .. }
            | Action::ToggleLumetriEnabled(_)
            | Action::ResetLumetriColor(_)
            | Action::AddColorFinesse(_)
            | Action::RemoveColorFinesse(_)
            | Action::SetColorFinesseEnabled { .. }
            | Action::SetColorFinesseParam { .. }
            | Action::ResetColorFinesse(_)) => self.apply_effects_chain(a),

            // --- Render / export ---
            a @ (Action::ExportMp4(_)
            | Action::ExportGif(_)
            | Action::ToggleAudioPreview
            | Action::SetAudioVolume(_)
            | Action::AddToRenderQueue
            | Action::RenderAll
            | Action::RemoveFromRenderQueue(_)
            | Action::ToggleCompSettings
            | Action::SetPendingCompWidth(_)
            | Action::SetPendingCompHeight(_)
            | Action::SetPendingCompFps(_)
            | Action::SetPendingCompDuration(_)
            | Action::SetPendingCompBgColor(_)
            | Action::ApplyCompSettings
            | Action::BuildRamPreview
            | Action::PlayRamPreview
            | Action::PurgeRamPreview
            | Action::ClearRamPreview
            | Action::ExportProRes(_)
            | Action::ExportDnxHD(_)
            | Action::SaveOutputPreset(_)
            | Action::LoadOutputPreset(_)
            | Action::DeleteOutputPreset(_)
            | Action::AddAllCompsToQueue
            | Action::ToggleLiveOutput
            | Action::StartPreRender
            | Action::CancelPreRender
            | Action::SetPreRenderProgress { .. }
            | Action::PreRenderComplete { .. }
            | Action::PreRenderFailed(_)
            | Action::ClearPreRenderCache
            | Action::ToggleUsePreRender
            | Action::ToggleRenderQueue
            | Action::AddRenderQueueItem(_)
            | Action::RemoveRenderQueueItem(_)
            | Action::SetRenderItemFormat { .. }
            | Action::SetRenderItemOutput { .. }
            | Action::SetRenderOutputPath(_)
            | Action::SetRenderItemRange { .. }
            | Action::SetRenderItemProxy { .. }
            | Action::StartRenderQueue
            | Action::StopRenderQueue
            | Action::RenderQueueItemComplete { .. }
            | Action::SkipRenderItem(_)
            | Action::DuplicateRenderItem(_)
            | Action::ToggleBrainstorm
            | Action::GenerateBrainstormVariations { .. }
            | Action::SelectBrainstormVariation(_)
            | Action::ApplyBrainstormVariation(_)
            | Action::SetBrainstormGrid { .. }
            | Action::SetBrainstormVariationCount(_)
            | Action::ExportBrainstormVariation { .. }
            | Action::CompareBrainstormVariations { .. }
            | Action::LockBrainstormVariation(_)
            | Action::ToggleCollectFilesPanel
            | Action::SetCollectDestination(_)
            | Action::SetCollectIncludeFootage(_)
            | Action::SetCollectIncludeProxies(_)
            | Action::SetCollectGenerateReport(_)
            | Action::SetCollectReduceProject(_)
            | Action::RunCollectFiles) => self.apply_render(a),

            // --- Tracking / rotobrush / motion sketch ---
            a @ (Action::ToggleCameraTracker
            | Action::AddTrackPoint { .. }
            | Action::RemoveTrackPoint(_)
            | Action::MoveTrackPoint { .. }
            | Action::SolveCameraTrack
            | Action::CreateCameraFromTrack
            | Action::SetCameraTrackerProgress(_)
            | Action::ClearCameraTrack
            | Action::SetRotobrushMode { .. }
            | Action::SetRotobrushRadius(_)
            | Action::AddRotobrushStroke { .. }
            | Action::ClearRotobrushStrokes { .. }
            | Action::PropagateRotobrush { .. }
            | Action::SetWarpStabResult(_)
            | Action::SetWarpStabSmoothness(_)
            | Action::SetWarpStabMethod(_)
            | Action::SetWarpStabFraming(_)
            | Action::SetWarpStabCropSmooth(_)
            | Action::SetWarpStabDetailedAnalysis(_)
            | Action::SetWarpStabRollingShutter(_)
            | Action::AnalyzeWarpStab { .. }
            | Action::WarpStabAnalysisComplete
            | Action::SetMotionSketchCaptureSpeed(_)
            | Action::SetMotionSketchSmoothing(_)
            | Action::SetMotionSketchShowWireframe(_)
            | Action::ToggleMotionSketchRecord
            | Action::ApplyMotionSketchStroke(_)
            | Action::ClearMotionSketchStrokes
            | Action::ApplyMotionSketchToLayer { .. }
            | Action::StartCameraTrackSolve { .. }
            | Action::SolveCameraTrackExt { .. }
            | Action::SelectTrackPoints { .. }
            | Action::CreateSolvedCamera { .. }
            | Action::DeleteCameraTrackSolve { .. }) => self.apply_tracking(a),

            // --- Expressions ---
            a @ (Action::AddExpressionControl(_)
            | Action::SetExprControlValue { .. }
            | Action::SetExpressionEnabled { .. }
            | Action::AddExpressionError { .. }
            | Action::ClearExpressionErrors { .. }
            | Action::SetExpressionLanguage(_)
            | Action::EvaluateExpression { .. }) => self.apply_expressions(a),

            // --- Precomp / track matte / 3D layers ---
            a @ (Action::SetTrackMatte { .. }
            | Action::SetTrackMatteMode { .. }
            | Action::SetTrackMatteSource { .. }
            | Action::ToggleTrackMatteInvert { .. }
            | Action::SetTrackMattePreserveTransparency { .. }
            | Action::ClearTrackMatte { .. }
            | Action::ToggleTrackMattePanel
            | Action::SetPrecompName(_)
            | Action::SetPrecompMoveAttribs(_)
            | Action::SetPrecompAdjustDuration(_)
            | Action::PrecomposeSelected
            | Action::OpenPrecomp(_)
            | Action::ClosePrecomp
            | Action::ReturnToMain
            | Action::RenamePrecomp { .. }
            | Action::DeletePrecomp(_)
            | Action::CollapseTransformations { .. }
            | Action::Enable3DLayer { .. }
            | Action::Set3DPosition { .. }
            | Action::Set3DLayerRotation { .. }
            | Action::Set3DOrientation { .. }
            | Action::Set3DScale { .. }
            | Action::Set3DAnchor { .. }
            | Action::Set3DShadows { .. }
            | Action::Set3DMaterial { .. }
            | Action::Reset3DLayer { .. }) => self.apply_precomp(a),

            // --- Puppeting / morphing ---
            a @ (Action::AddPuppetPin { .. }
            | Action::MovePuppetPin { .. }
            | Action::RemovePuppetPin { .. }
            | Action::SetPuppetPinStiffness { .. }
            | Action::TogglePuppetPinStiff { .. }
            | Action::SetPuppetMeshDensity { .. }
            | Action::SetShapeMorphEnabled(_)
            | Action::AddMorphKeyframe(_)
            | Action::RemoveMorphKeyframe(_)
            | Action::SetMorphMode { .. }
            | Action::SetCorrespondenceMode(_)
            | Action::SetMorphPreviewTime(_)
            | Action::PreviewMorphAtTime(_)
            | Action::ClearMorphKeyframes
            | Action::ActivatePuppetTool(_)
            | Action::AddPuppetPinExt { .. }
            | Action::MovePuppetPinExt { .. }
            | Action::SetPuppetPinStiffnessExt { .. }
            | Action::DeletePuppetPin(_)
            | Action::SetPuppetMeshDensityExt { .. }
            | Action::SetPuppetMeshExpansion { .. }) => self.apply_puppeting(a),

            // --- Text animation / MoGrt / audio spectrum ---
            a @ (Action::ToggleMoGrtPanel
            | Action::AddMoGrtTemplate(_)
            | Action::RemoveMoGrtTemplate(_)
            | Action::SelectMoGrtTemplate(_)
            | Action::AddMoGrtControl { .. }
            | Action::RemoveMoGrtControl { .. }
            | Action::SetMoGrtTextValue { .. }
            | Action::SetMoGrtColorValue { .. }
            | Action::SetMoGrtSliderValue { .. }
            | Action::ExportMoGrt { .. }
            | Action::SetAudioVisMode(_)
            | Action::SetAudioVisLayer(_)
            | Action::SetAudioStartFreq(_)
            | Action::SetAudioEndFreq(_)
            | Action::SetAudioMaxHeight(_)
            | Action::SetAudioVisSide(_)
            | Action::SetAudioSoftness(_)
            | Action::SetAudioMirror(_)
            | Action::SetAudioDisplayedSamples(_)
            | Action::SetAudioFrequencyBands(_)
            | Action::SetAudioThickness(_)
            | Action::SetAudioDigital(_)
            | Action::ApplyAudioSpectrumEffect { .. }
            | Action::AddTextAnimatorExt { .. }
            | Action::RemoveTextAnimatorExt(_)
            | Action::ApplyTextAnimPreset { .. }
            | Action::SetTextAnimRange { .. }
            | Action::SetTextAnimRangeUnits { .. }
            | Action::SetTextAnimBasedOn { .. }
            | Action::OpenEssentialGraphics
            | Action::CloseEssentialGraphics
            | Action::CreateMogrTemplate { .. }
            | Action::AddMogrParam { .. }
            | Action::SetMogrParamValue { .. }
            | Action::ExportMogrt { .. }
            | Action::DeleteMogrTemplate(_)) => self.apply_text_anim(a),

            // --- Batch 5 new features ---
            a @ (Action::AddMotionBlurEffect { .. }
            | Action::AddGlowEffect { .. }
            | Action::AddCcRepeTileEffect { .. }
            | Action::AddPosterizeTime { .. }
            | Action::AddCellPattern { .. }
            | Action::AddCheckerboard { .. }
            | Action::AddGradientEffect { .. }
            | Action::AddGridEffect { .. }
            | Action::AddStrokeEffect { .. }
            | Action::SetMotionBlur { .. }
            | Action::RemoveBatch5Effect { .. }
            | Action::CreateMotionPath { .. }
            | Action::AddMotionPathPoint { .. }
            | Action::RemoveMotionPathPoint { .. }
            | Action::SetMotionPathPoint { .. }
            | Action::SetMotionPathEasing { .. }
            | Action::SetAutoOrient { .. }
            | Action::DeleteMotionPath { .. }
            | Action::AddShapeGroup { .. }
            | Action::AddShapeItemToGroup { .. }
            | Action::RemoveShapeItemFromGroup { .. }
            | Action::SetShapeGroupTransform { .. }
            | Action::SetShapeStar { .. }
            | Action::AddRepeaterToGroup { .. }
            | Action::AddTrimPath { .. }
            | Action::AddMergeShapes { .. }
            | Action::DeleteShapeGroup { .. }
            | Action::AddAudioBus { .. }
            | Action::RemoveAudioBus { .. }
            | Action::SetBusVolume { .. }
            | Action::SetBusPan { .. }
            | Action::MuteBus { .. }
            | Action::SoloBus { .. }
            | Action::AddBusSend { .. }
            | Action::RemoveBusSend { .. }
            | Action::SetBusEq { .. }
            | Action::SetBusCompressor { .. }
            | Action::SetMasterVolume(_)
            | Action::SetMasterPan(_)) => self.apply_batch5(a),

            // --- Mask editor ---
            a @ (Action::AddRectMask { .. }
            | Action::AddEllipseMask { .. }
            | Action::RemoveMask { .. }
            | Action::AddMaskVertex { .. }
            | Action::InsertMaskVertex { .. }
            | Action::RemoveMaskVertex { .. }
            | Action::SetMaskVertex { .. }
            | Action::SetMaskVertexHandles { .. }
            | Action::SetMaskMode { .. }
            | Action::SetMaskFeather { .. }
            | Action::SetMaskOpacity { .. }
            | Action::SetMaskExpansion { .. }
            | Action::SetMaskInverted { .. }) => self.apply_masks(a),

            // --- Fractal Noise / Turbulent Displace ---
            a @ (Action::AddFractalNoise { .. }
            | Action::RemoveFractalNoise { .. }
            | Action::SetFractalNoiseParam { .. }
            | Action::SetFractalNoiseSeed { .. }
            | Action::SetFractalNoiseOctaves { .. }
            | Action::AddTurbulentDisplace { .. }
            | Action::RemoveTurbulentDisplace { .. }
            | Action::SetTurbulentDisplaceParam { .. }
            | Action::SetTurbulentDisplaceComplexity { .. }) => self.apply_effects_noise(a),

            // --- Per-layer motion blur ---
            a @ (Action::SetLayerShutterAngle { .. }
            | Action::SetLayerShutterPhase { .. }
            | Action::SetLayerMotionBlurSamples { .. }
            | Action::SetLayerMotionBlurOverride { .. }
            | Action::ClearLayerMotionBlurOverride { .. }) => self.apply_motion_blur(a),

            // --- Time stretch / time remap ---
            a @ (Action::SetLayerStretchFromDuration { .. }
            | Action::ReverseLayerTime { .. }
            | Action::FreezeFrameAt { .. }
            | Action::RemoveTimeRemapKey { .. }
            | Action::ClearTimeRemap { .. }) => self.apply_time_stretch(a),

            // --- AE feature pass: Output Modules ---
            a @ (Action::AddOutputModule(_)
            | Action::RemoveOutputModule(_)
            | Action::SetOutputModuleFormat { .. }
            | Action::SetOutputModuleCodec { .. }
            | Action::SetOutputModuleDepth { .. }
            | Action::SetOutputModuleScale { .. }
            | Action::SetOutputModuleResolution { .. }
            | Action::SetOutputModuleRange { .. }
            | Action::SetOutputModuleAudio { .. }) => self.apply_output_module(a),

            // --- AE feature pass: Preferences ---
            a @ (Action::TogglePreferences
            | Action::SetPrefUndoLevels(_)
            | Action::SetPrefAutosaveMinutes(_)
            | Action::SetPrefShowTooltips(_)
            | Action::SetPrefUiScale(_)
            | Action::SetPrefDarkTheme(_)
            | Action::SetPrefMotionPathKeyframes(_)
            | Action::SetPrefDiskCacheDir(_)
            | Action::SetPrefDiskCacheMaxGb(_)
            | Action::SetPrefRamReserve(_)
            | Action::SetPrefConformFps(_)
            | Action::SetPrefPreviewQuality(_)
            | Action::SetPrefFastDraft(_)
            | Action::SavePreferences(_)
            | Action::LoadPreferences(_)
            | Action::ResetPreferences) => self.apply_preferences(a),

            // --- AE feature pass: Disk Cache Manager ---
            a @ (Action::SetCacheDir(_)
            | Action::SetCacheMaxGb(_)
            | Action::CacheInsert { .. }
            | Action::CacheTouch(_)
            | Action::PurgeDiskCache
            | Action::EvictDiskCache) => self.apply_cache_manager(a),

            // --- AE feature pass: Audio mixer expansion ---
            a @ (Action::AddMixerTrack { .. }
            | Action::RemoveMixerTrack { .. }
            | Action::SetTrackGainDb { .. }
            | Action::SetTrackPan { .. }
            | Action::SetTrackMute { .. }
            | Action::SetTrackSolo { .. }
            | Action::SetTrackOutputBus { .. }
            | Action::SetMasterGainDb(_)
            | Action::SetMasterBusPan(_)
            | Action::SetMasterBusMute(_)) => self.apply_audio_mixer(a),

            // --- Keying suite (keying.rs) ---
            a @ (Action::AddKeyer { .. }
            | Action::RemoveKeyer { .. }
            | Action::SetKeyKind { .. }
            | Action::SetKeyColor { .. }
            | Action::SetKeyParam { .. }) => self.apply_keying(a),

            // --- Distortion effects (effects_distort.rs) ---
            a @ (Action::AddCornerPin { .. }
            | Action::SetCornerPinCorner { .. }
            | Action::RemoveCornerPin { .. }
            | Action::AddBezierWarp { .. }
            | Action::SetBezierWarpCorner { .. }
            | Action::RemoveBezierWarp { .. }
            | Action::AddWaveWarp { .. }
            | Action::SetWaveWarpParam { .. }
            | Action::RemoveWaveWarp { .. }
            | Action::AddRoughenEdges { .. }
            | Action::SetRoughenEdgesParam { .. }
            | Action::SetRoughenEdgesSeed { .. }
            | Action::RemoveRoughenEdges { .. }) => self.apply_effects_distort(a),

            // --- 3D lights & materials (lighting3d.rs) ---
            a @ (Action::AddLight3D { .. }
            | Action::RemoveLight3D { .. }
            | Action::SetLight3DPosition { .. }
            | Action::SetLight3DColor { .. }
            | Action::SetLight3DIntensity { .. }
            | Action::SetLight3DCone { .. }
            | Action::SetMaterial3D { .. }) => self.apply_lighting3d(a),

            // --- Shape repeater + trim paths (shape_repeater.rs) ---
            a @ (Action::AddRepeater { .. }
            | Action::RemoveRepeater { .. }
            | Action::SetRepeaterCount { .. }
            | Action::SetRepeaterTransform { .. }
            | Action::SetRepeaterOpacityRamp { .. }
            | Action::SetTrimPaths { .. }
            | Action::AddTrimKey { .. }
            | Action::ClearTrimPaths { .. }) => self.apply_shape_repeater(a),

            // --- Render output formats (render_formats.rs) ---
            a @ (Action::SetRenderCodec(_)
            | Action::SetRenderFps(_)
            | Action::SetRenderCrf(_)
            | Action::SetRenderAudio(_)) => self.apply_render_formats(a),

            // --- Particle system (particles.rs) ---
            a @ Action::Particles(_) => self.apply_particles(a),

            // --- Everything else: composition, transport, layer management, 3D camera, history ---
            a => self.apply_composition(a),
        }
    }
}
