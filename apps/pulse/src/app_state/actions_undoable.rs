//! The [`Action::is_undoable`] classifier — which actions snapshot the project
//! for undo. Split out of `actions.rs` to keep that file under the 1000-line
//! limit; it's a pure `impl Action` over the same enum (defined in
//! `actions.rs`).

use super::Action;

impl Action {
    /// Whether applying this action mutates the [`Project`] document (so the
    /// undo stack should snapshot the pre-state before it runs). Pure transport /
    /// selection / UI-toggle actions — and the history actions themselves — are
    /// NOT snapshotted (they don't change the document, or they manage the stack
    /// directly).
    pub(super) fn is_undoable(&self) -> bool {
        matches!(
            self,
            Action::ToggleLayerVisible(_)
                | Action::SetTransform(_, _)
                | Action::ToggleKeyframe(_)
                | Action::MoveKeyframe { .. }
                | Action::AddEffect(_)
                | Action::RemoveEffect { .. }
                | Action::SetEffectParam { .. }
                | Action::SetWorkAreaStart(_)
                | Action::SetWorkAreaEnd(_)
                | Action::ResetWorkArea
                | Action::SetInterp { .. }
                | Action::MoveKeyframeXY { .. }
                | Action::GizmoKeys { .. }
                | Action::SetLayer3D(_, _)
                | Action::SetPositionZ(_, _)
                | Action::Set3DRotation(_, _, _, _)
                | Action::DuplicateLayer(_)
                | Action::SetExpression { .. }
                | Action::RenameLayer { .. }
                | Action::SetCompName(_)
                | Action::AddGpuiEffect(_)
                | Action::RemoveGpuiEffect(_)
                | Action::SetMosaicBlock { .. }
                | Action::SetChromaOffset { .. }
                | Action::SetEffectIntensity { .. }
                | Action::SetVignetteRadius { .. }
                | Action::ApplyCompSettings
                | Action::SetParent(_, _)
                | Action::ClearParent(_)
                | Action::PreCompose(_, _)
                | Action::AddNullLayer
                | Action::AddGuideLayer
                | Action::AddSolidLayer(_)
                | Action::AddAdjustmentLayer
                | Action::SetFootageVideo { .. }
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
                | Action::AddCompMarker { .. }
                | Action::RemoveCompMarker(_)
                | Action::AddLayerMarker { .. }
                | Action::RemoveLayerMarker { .. }
                | Action::SetCameraPosition(_)
                | Action::SetCameraFov(_)
                | Action::SetParentLayer { .. }
                | Action::SetTimeRemapEnabled { .. }
                | Action::SetTimeRemapKey { .. }
                | Action::SetLayerMatte { .. }
                | Action::AddPuppetPin { .. }
                | Action::MovePuppetPin { .. }
                | Action::RemovePuppetPin { .. }
                | Action::ToggleSolo(_)
                | Action::AddLight(_)
                | Action::RemoveLight(_)
                | Action::UpdateLight { .. }
                | Action::SetMotionBlurEnabled(_)
                | Action::SetMotionBlurAngle(_)
                | Action::SetMotionBlurPhase(_)
                | Action::SetMotionBlurSamples(_)
                | Action::ToggleLayerMotionBlur(_)
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
                | Action::SplitLayer(_)
                | Action::SplitLayerAt { .. }
                | Action::AddColorFinesse(_)
                | Action::RemoveColorFinesse(_)
                | Action::SetColorFinesseEnabled { .. }
                | Action::SetColorFinesseParam { .. }
                | Action::ResetColorFinesse(_)
                | Action::AddRotobrushStroke { .. }
                | Action::ClearRotobrushStrokes { .. }
                | Action::PropagateRotobrush { .. }
                | Action::SetLayerTimeStretch { .. }
                | Action::SetLayerAudioFade { .. }
                | Action::SetPuppetPinStiffness { .. }
                | Action::TogglePuppetPinStiff { .. }
                | Action::SetPuppetMeshDensity { .. }
                | Action::SetLayerEcho { .. }
                | Action::ClearLayerEcho { .. }
                | Action::AddMotionBlurEffect { .. }
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
                | Action::SetMasterPan(_)
                // Mask editor — these edit `project.comps[].layers[].masks`.
                | Action::AddRectMask { .. }
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
                | Action::SetMaskInverted { .. }
                // Time stretch / remap — these edit `project` (stretch + remap).
                | Action::SetLayerStretchFromDuration { .. }
                | Action::ReverseLayerTime { .. }
                | Action::FreezeFrameAt { .. }
                | Action::RemoveTimeRemapKey { .. }
                | Action::ClearTimeRemap { .. }
        )
    }
}
