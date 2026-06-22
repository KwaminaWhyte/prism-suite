//! Dispatcher — the single mutation choke-point for Tone's App state.
//!
//! Contains only `impl App {{ pub fn apply(...) }}`.
//! All other `App` impl blocks live in domain files.

use super::{App, Action};

impl App {
// ── Dispatcher ───────────────────────────────────────────────────────────

    /// Apply a single action, mutating state in place. This is the only place
    /// state is mutated — panels never touch fields directly.
    pub fn apply(&mut self, action: Action) {
        match &action {
            // ── Project ──────────────────────────────────────────────────────
            Action::SetBpm(..)
            | Action::SetTimeSignature { .. }
            | Action::SetSampleRate(..)
            | Action::SetBitDepth(..)
            | Action::SetProjectKey(..)
            | Action::SetProjectScale(..)
            | Action::SetProjectName(..) => self.apply_project(action),

            // ── Tracks ───────────────────────────────────────────────────────
            Action::AddTrack(..)
            | Action::DeleteTrack(..)
            | Action::RenameTrack { .. }
            | Action::SetTrackMute { .. }
            | Action::SetTrackSolo { .. }
            | Action::SetTrackArm { .. }
            | Action::SetTrackVolume { .. }
            | Action::SetTrackPan { .. }
            | Action::SetTrackColor { .. }
            | Action::SetTrackInstrument { .. }
            | Action::DuplicateTrack(..)
            | Action::ReorderTracks { .. }
            | Action::SetActiveTrack(..)
            | Action::AddTrackSend { .. }
            | Action::RemoveTrackSend { .. }
            | Action::SetSendLevel { .. }
            | Action::ToggleSend { .. }
            | Action::AddInsertEffect { .. }
            | Action::RemoveInsertEffect { .. }
            | Action::ToggleInsertEffect { .. }
            | Action::ReorderInsertEffect { .. }
            | Action::SetInsertGain { .. } => self.apply_tracks(action),

            // ── Clips ────────────────────────────────────────────────────────
            Action::AddClip { .. }
            | Action::DeleteClip(..)
            | Action::MoveClip { .. }
            | Action::ResizeClip { .. }
            | Action::SetClipGain { .. }
            | Action::SetClipPitchShift { .. }
            | Action::SetClipTimeStretch { .. }
            | Action::SetClipLoop { .. }
            | Action::SetClipMute { .. }
            | Action::SplitClip { .. }
            | Action::MergeClips { .. }
            | Action::InvalidatePeakCache { .. }
            | Action::DuplicateClip { .. }
            | Action::ConsolidateClips { .. }
            | Action::SetClipColor { .. }
            | Action::TrimClipStart { .. }
            | Action::TrimClipEnd { .. }
            | Action::SetClipPitchF32 { .. }
            | Action::SetClipGainDb { .. } => self.apply_clips(action),

            // ── MIDI / Piano Roll ─────────────────────────────────────────────
            Action::OpenPianoRoll(..)
            | Action::ClosePianoRoll
            | Action::AddMidiNote { .. }
            | Action::DeleteMidiNote(..)
            | Action::MoveMidiNote { .. }
            | Action::ResizeMidiNote { .. }
            | Action::SetMidiNoteVelocity { .. }
            | Action::SelectAllNotesInClip(..)
            | Action::QuantizeMidiNotes { .. }
            | Action::TransposeMidiNotes { .. }
            | Action::SetPianoRollZoom { .. }
            | Action::SetPianoRollScroll { .. }
            | Action::AddMidiCC { .. }
            | Action::RemoveMidiCC { .. }
            | Action::SetMidiCCValue { .. }
            | Action::MoveMidiCC { .. }
            | Action::SelectNote { .. }
            | Action::DeselectNote { .. }
            | Action::DeselectAllNotes
            | Action::CopySelectedNotes
            | Action::PasteNotes { .. }
            | Action::NudgeNotes { .. }
            | Action::DeleteSelectedNotes
            | Action::MoveSelectedNotes { .. }
            | Action::ResizeSelectedNotes { .. }
            | Action::SetSelectedNotesVelocity { .. }
            | Action::QuantizeSelectedNotes { .. } => self.apply_midi(action),

            // ── Tool selection ────────────────────────────────────────────────
            Action::SetActiveTool(tool) => {
                self.active_tool = *tool;
            }

            // ── Quantize ─────────────────────────────────────────────────────
            Action::SetQuantize { .. } => {
                if let Action::SetQuantize { grid, swing, strength } = action {
                    self.quantize_config.grid = grid;
                    self.quantize_config.swing = swing.clamp(0.0, 1.0);
                    self.quantize_config.strength = strength.clamp(0.0, 1.0);
                }
            }

            // ── Mixer ─────────────────────────────────────────────────────────
            Action::SetChannelEqLow { .. }
            | Action::SetChannelEqMid { .. }
            | Action::SetChannelEqHigh { .. }
            | Action::SetChannelCompThreshold { .. }
            | Action::SetChannelCompRatio { .. }
            | Action::ToggleChannelComp(..)
            | Action::AddChannelEffect { .. }
            | Action::RemoveChannelEffect { .. }
            | Action::SetMasterVolume(..)
            | Action::ToggleMasterLimiter
            | Action::AddChannelSend { .. }
            | Action::RemoveChannelSend { .. }
            | Action::SetChannelSendLevel { .. }
            | Action::SetChannelPfl { .. }
            | Action::SetChannelPhaseInvert { .. }
            | Action::SetStereoWidth { .. }
            | Action::SetChannelTrim { .. }
            | Action::ResetChannel { .. } => self.apply_mixer(action),

            // ── Transport ────────────────────────────────────────────────────
            Action::Play
            | Action::Stop
            | Action::Pause
            | Action::Record
            | Action::SetPlayheadBeat(..)
            | Action::ToggleLoop
            | Action::SetLoopRange { .. }
            | Action::ToggleMetronome
            | Action::Rewind
            | Action::FastForward
            | Action::SetLoopRegion { .. }
            | Action::SetLoopBarEnabled(..)
            | Action::MoveLoopRegion { .. }
            | Action::ResizeLoopStart { .. }
            | Action::ResizeLoopEnd { .. } => self.apply_transport(action),

            // ── History ──────────────────────────────────────────────────────
            Action::Undo | Action::Redo | Action::PushUndoCheckpoint { .. } => {
                self.apply_history(action)
            }

            // ── Track groups ─────────────────────────────────────────────────
            Action::CreateTrackGroup { .. }
            | Action::AddTrackToGroup { .. }
            | Action::RemoveTrackFromGroup { .. }
            | Action::CollapseGroup { .. }
            | Action::DeleteTrackGroup { .. }
            | Action::SetGroupColor { .. }
            | Action::RenameTrackGroup { .. } => self.apply_track_groups(action),

            // ── AI ────────────────────────────────────────────────────────────
            Action::SetAiPrompt(..)
            | Action::SetAiStyle(..)
            | Action::SetAiDurationBars(..)
            | Action::GenerateTrack { .. }
            | Action::CompleteAiGeneration { .. }
            | Action::GenerateChordProgression { .. }
            | Action::GenerateDrumPattern { .. }
            | Action::HarmonizeMelody { .. }
            | Action::AiMasterTrack
            | Action::SuggestChords { .. } => self.apply_ai(action),

            // ── Export ────────────────────────────────────────────────────────
            Action::SetBounceFormat(..)
            | Action::SetBounceNormalize(..)
            | Action::SetBounceDither(..)
            | Action::SetBounceExportPath(..)
            | Action::SetBounceStemsPerTrack(..)
            | Action::StartBounce
            | Action::CancelBounce => self.apply_export(action),

            // ── Automation ────────────────────────────────────────────────────
            Action::CreateAutomationLane { .. }
            | Action::DeleteAutomationLane { .. }
            | Action::AddAutomationPoint { .. }
            | Action::RemoveAutomationPoint { .. }
            | Action::MoveAutomationPoint { .. }
            | Action::SetAutomationCurve { .. }
            | Action::SetAutomationLaneEnabled { .. }
            | Action::SetAutomationLaneVisible { .. }
            | Action::SetAutomationMode(..)
            | Action::ClearAutomationLane { .. } => self.apply_automation(action),

            // ── Tempo Map ─────────────────────────────────────────────────────
            Action::AddTempoEvent { .. }
            | Action::RemoveTempoEvent { .. }
            | Action::MoveTempoEvent { .. }
            | Action::AddTimeSigEvent { .. }
            | Action::RemoveTimeSigEvent { .. }
            | Action::SetGlobalBpm(..) => self.apply_tempo_map(action),

            // ── Scenes ────────────────────────────────────────────────────────
            Action::AddScene { .. }
            | Action::DeleteScene { .. }
            | Action::RenameScene { .. }
            | Action::SetSceneColor { .. }
            | Action::SetSceneBpmOverride { .. }
            | Action::AssignClipToScene { .. }
            | Action::SetFollowAction { .. }
            | Action::LaunchScene { .. }
            | Action::SetArrangementMode(..)
            | Action::DuplicateScene { .. } => self.apply_scenes(action),

            // ── Plugins ───────────────────────────────────────────────────────
            Action::AddPlugin { .. }
            | Action::RemovePlugin { .. }
            | Action::TogglePlugin { .. }
            | Action::SetPluginPreset { .. }
            | Action::SetPluginParam { .. }
            | Action::OpenPluginWindow { .. }
            | Action::ClosePluginWindow { .. }
            | Action::AddBuiltinSynth { .. }
            | Action::RemoveBuiltinSynth { .. }
            | Action::SetBuiltinSynthOctave { .. } => self.apply_plugins(action),

            // ── MIDI Control ──────────────────────────────────────────────────
            Action::AddMidiMapping { .. }
            | Action::RemoveMidiMapping { .. }
            | Action::SetMappingRange { .. }
            | Action::AddMidiDevice { .. }
            | Action::RemoveMidiDevice { .. }
            | Action::EnableMidiDevice { .. }
            | Action::StartMidiLearn { .. }
            | Action::StopMidiLearn
            | Action::CompleteMidiLearn { .. } => self.apply_midi_control(action),

            // ── Audio Engine ──────────────────────────────────────────────────
            Action::SetAudioDevice { .. }
            | Action::SetBufferSize(..)
            | Action::SetAudioSampleRate(..)
            | Action::SetLowLatencyMode(..)
            | Action::SetExclusiveMode(..)
            | Action::StartAudioEngine
            | Action::StopAudioEngine
            | Action::ReportAudioOverload
            | Action::ResetOverloadCount
            | Action::AddInputDevice { .. }
            | Action::AddOutputDevice { .. }
            | Action::SelectInputDevice { .. }
            | Action::SelectOutputDevice { .. } => self.apply_audio_engine(action),

            // ── Recording ─────────────────────────────────────────────────────
            Action::StartRecording { .. }
            | Action::StopRecording
            | Action::SetPunchIn { .. }
            | Action::SetPunchOut { .. }
            | Action::SetAutoPunch(..)
            | Action::SetCountIn { .. }
            | Action::AddTake { .. }
            | Action::SetActiveTake { .. }
            | Action::MuteTake { .. }
            | Action::DeleteTake { .. }
            | Action::EnableCompMode { .. }
            | Action::FlattenTakes { .. } => self.apply_recording(action),

            // ── Step Sequencer ────────────────────────────────────────────────
            Action::AddPattern { .. }
            | Action::DeletePattern { .. }
            | Action::RenamePattern { .. }
            | Action::SetPatternSteps { .. }
            | Action::SetPatternStepLength { .. }
            | Action::SetPatternSwing { .. }
            | Action::SetStep { .. }
            | Action::SetStepVelocity { .. }
            | Action::SetStepProbability { .. }
            | Action::SetStepAccent { .. }
            | Action::SetStepSkip { .. }
            | Action::SetStepRetrigger { .. }
            | Action::ClearPattern { .. }
            | Action::FillPattern { .. }
            | Action::RandomizePattern { .. }
            | Action::SetActivePattern { .. }
            | Action::AddChainPattern { .. }
            | Action::DeleteChainPattern { .. } => self.apply_step_sequencer(action),

            // ── Score View ────────────────────────────────────────────────────
            Action::ToggleScoreView
            | Action::AddTrackToScore { .. }
            | Action::RemoveTrackFromScore { .. }
            | Action::SetScoreZoom(..)
            | Action::SetScoreScroll(..)
            | Action::SetScoreClef(..)
            | Action::SetScoreKeySignature(..)
            | Action::SetScoreStemDirection(..)
            | Action::SetScoreQuantizeDisplay(..)
            | Action::ToggleChordSymbols
            | Action::AddChordSymbol { .. }
            | Action::RemoveChordSymbol { .. }
            | Action::SetPrintLayout(..) => self.apply_score_view(action),

            // ── Bus Routing ───────────────────────────────────────────────────
            Action::AddBusTrack { .. }
            | Action::DeleteBusTrack { .. }
            | Action::RenameBusTrack { .. }
            | Action::SetBusVolume { .. }
            | Action::SetBusPan { .. }
            | Action::MuteBusTrack { .. }
            | Action::AddBusInsertEffect { .. }
            | Action::RemoveBusInsertEffect { .. }
            | Action::SetSendToBus { .. }
            | Action::SetMasterBusVolume(..)
            | Action::SetMasterLimiterThreshold(..)
            | Action::SetMasterLimiterRelease(..)
            | Action::SetMasterDither { .. }
            | Action::SetMasterAutoNormalize(..)
            | Action::AddMasterInsertEffect(..)
            | Action::RemoveMasterInsertEffect { .. } => self.apply_bus_routing(action),
            // ── Beat Detection ────────────────────────────────────────────────
            Action::AnalyzeBeatDetection { .. }
            | Action::UpdateBeatDetectionStatus { .. }
            | Action::ApplyDetectedBpm { .. }
            | Action::AddWarpMarker { .. }
            | Action::RemoveWarpMarker { .. }
            | Action::MoveWarpMarker { .. }
            | Action::LockWarpMarker { .. }
            | Action::SetStretchAlgorithm { .. }
            | Action::SetFormantPreservation { .. }
            | Action::SetTransientSensitivity { .. } => self.apply_beat_detection(action),

            // ── Clip Launch ───────────────────────────────────────────────────
            Action::AddClipSlot { .. }
            | Action::AssignClipToSlot { .. }
            | Action::LaunchSlot { .. }
            | Action::StopSlot { .. }
            | Action::QueueSlotAction { .. }
            | Action::ClearSlotQueue { .. }
            | Action::SetSlotFollowAction { .. }
            | Action::SetSlotFollowTime { .. }
            | Action::SetLaunchQuantize(..)
            | Action::SetLinkEnabled(..)
            | Action::StopAllSlots
            | Action::LaunchSceneSlots { .. } => self.apply_clip_launch(action),

            // ── Chord Tools ───────────────────────────────────────────────────
            Action::AddChordProgression { .. }
            | Action::DeleteChordProgression { .. }
            | Action::RenameChordProgression { .. }
            | Action::AddChordToProgression { .. }
            | Action::RemoveChordFromProgression { .. }
            | Action::SetProgressionRoot { .. }
            | Action::SetProgressionScale { .. }
            | Action::SetActiveProgression(..)
            | Action::SetScaleLock { .. }
            | Action::ClearChordSuggestions
            | Action::AddChordSuggestion { .. } => self.apply_chord_tools(action),

            // ── Freeze / Stems ─────────────────────────────────────────────────
            Action::FreezeTrack { .. }
            | Action::UnfreezeTrack { .. }
            | Action::SetFreezeState { .. }
            | Action::LockFrozenTrack { .. }
            | Action::FlattenTrack { .. }
            | Action::AddStem { .. }
            | Action::RemoveStem { .. }
            | Action::SetStemTracks { .. }
            | Action::SetStemOutputPath { .. }
            | Action::ExportStems
            | Action::UpdateStemStatus { .. } => self.apply_freeze(action),

            // ── MIDI Routing / VI / Arp ───────────────────────────────────────
            Action::AddMidiOutputPort { .. }
            | Action::RemoveMidiOutputPort { .. }
            | Action::SetMidiPortEnabled { .. }
            | Action::SetMidiPortTranspose { .. }
            | Action::SetMidiPortVelocityScale { .. }
            | Action::AddVirtualInstrument { .. }
            | Action::RemoveVirtualInstrument { .. }
            | Action::SetVirtualInstrumentPreset { .. }
            | Action::SetVirtualInstrumentPolyphony { .. }
            | Action::ToggleVirtualInstrument { .. }
            | Action::AddArpeggiator { .. }
            | Action::RemoveArpeggiator { .. }
            | Action::SetArpPattern { .. }
            | Action::SetArpRate { .. }
            | Action::SetArpOctaveRange { .. }
            | Action::ToggleArpeggiator { .. } => self.apply_midi_routing(action),

            // ── MusicGen ──────────────────────────────────────────────────────
            Action::QueueMusicGen { .. }
            | Action::StartMusicGen { .. }
            | Action::UpdateMusicGenProgress { .. }
            | Action::CompleteMusicGen { .. }
            | Action::CancelMusicGen { .. }
            | Action::RetryMusicGen { .. } => self.apply_musicgen(action),

            // ── Demucs stem splitting ─────────────────────────────────────────
            Action::QueueStemSplit { .. }
            | Action::StartStemSplit { .. }
            | Action::UpdateStemSplitProgress { .. }
            | Action::CompleteStemSplit { .. }
            | Action::CancelStemSplit { .. } => self.apply_demucs(action),

            // ── Magenta melody / continuation / chord voicing ─────────────────
            Action::QueueMelodyGen { .. }
            | Action::StartMelodyGen { .. }
            | Action::CompleteMelodyGen { .. }
            | Action::CancelMelodyGen { .. }
            | Action::QueueMelodyContinue { .. }
            | Action::CompleteMelodyContinue { .. }
            | Action::QueueChordVoicing { .. }
            | Action::CompleteChordVoicing { .. } => self.apply_magenta(action),
            // ── AI Mastering ──────────────────────────────────────────────────
            Action::QueueAiMaster { .. }
            | Action::StartAiMaster { .. }
            | Action::UpdateAiMasterProgress { .. }
            | Action::CompleteAiMaster { .. }
            | Action::ToggleAiMasterAB { .. }
            | Action::SetAiMasterTarget { .. }
            | Action::CancelAiMaster { .. } => self.apply_ai_mastering(&action),

            // ── Vocal Tools ───────────────────────────────────────────────────
            Action::ApplyAutoTune { .. }
            | Action::GenVocalHarmony { .. }
            | Action::IsolateVocals { .. }
            | Action::StartVocalJob { .. }
            | Action::CompleteVocalJob { .. }
            | Action::CancelVocalJob { .. } => self.apply_vocal_tools(&action),

            // ── Smart Mix ─────────────────────────────────────────────────────
            Action::StartAutoMix
            | Action::UpdateAutoMixAnalysis { .. }
            | Action::ApplyMixSuggestions { .. }
            | Action::SetAutoMixTarget { .. }
            | Action::ResetAutoMix { .. }
            | Action::DiscardAutoMix { .. } => self.apply_smart_mix(&action),
            // ── MIDI Controllers (Batch 5) ────────────────────────────────────
            Action::RegisterController { .. }
            | Action::UnregisterController { .. }
            | Action::SetControllerPreset { .. }
            | Action::ActivateController { .. }
            | Action::DeactivateController { .. }
            | Action::StartPadLearn { .. }
            | Action::CompletePadLearn { .. }
            | Action::StartKnobLearn { .. }
            | Action::CompleteKnobLearn { .. }
            | Action::CancelCtrlLearn => self.apply_midi_controllers(action),

            // ── Loop Recording (Batch 5) ──────────────────────────────────────
            Action::StartLoopRecord { .. }
            | Action::EndLoopRecord { .. }
            | Action::DiscardTake { .. }
            | Action::SetLoopActiveTake { .. }
            | Action::SetCompRegion { .. }
            | Action::BakeComp { .. }
            | Action::DeleteTakeStack { .. } => self.apply_loop_recording(action),

            // ── Hardware Sync (Batch 5) ───────────────────────────────────────
            Action::SetMidiClockSource { .. }
            | Action::SetMidiClockBpm { .. }
            | Action::ToggleSendClock
            | Action::ToggleReceiveClock
            | Action::StartMidiClock
            | Action::StopMidiClock
            | Action::SetHardwareLinkEnabled { .. }
            | Action::SetLinkBpm { .. }
            | Action::UpdateLinkPeers { .. }
            | Action::SetLinkQuantum { .. }
            | Action::ToggleLinkStartStop => self.apply_hardware_sync(action),
            // ── VST Host ─────────────────────────────────────────────────────
            Action::ScanVstDirectory { .. }
            | Action::CompletVstScan { .. }
            | Action::LoadVst { .. }
            | Action::UnloadVst { .. }
            | Action::BypassVst { .. }
            | Action::SetVstPreset { .. }
            | Action::BlacklistVst { .. } => self.apply_vst_host(action),

            // ── Surround ─────────────────────────────────────────────────────
            Action::SetSurroundFormat { .. }
            | Action::ToggleSurroundBus
            | Action::ToggleBinauralMonitor
            | Action::SetTrackSurroundPan { .. }
            | Action::CreateAtmosObject { .. }
            | Action::SetAtmosPan { .. }
            | Action::RemoveAtmosObject { .. } => self.apply_surround(action),

            // ── Spectral ─────────────────────────────────────────────────────
            Action::ToggleSpectralView
            | Action::SetSpectralFftSize { .. }
            | Action::SetSpectralColorMap { .. }
            | Action::ApplySpectralBrush { .. }
            | Action::UndoLastSpectralBrush
            | Action::QueueSpectralStretch { .. }
            | Action::CompleteSpectralStretch { .. } => self.apply_spectral(action),

            // ── Notation ─────────────────────────────────────────────────────
            Action::OpenNotationView { .. }
            | Action::CloseNotationView { .. }
            | Action::SetStaffClef { .. }
            | Action::SetStaffKeySig { .. }
            | Action::SetStaffTranspose { .. }
            | Action::ExportNotationPdf { .. }
            | Action::CompleteNotationExport { .. } => self.apply_notation(action),

            // ── Tempo Film ────────────────────────────────────────────────────
            Action::AddTempoChange2 { .. }
            | Action::RemoveTempoChange { .. }
            | Action::SetTempoChangeBpm { .. }
            | Action::AddTimeSigChange2 { .. }
            | Action::RemoveTimeSigChange { .. }
            | Action::LinkVideo { .. }
            | Action::UnlinkVideo
            | Action::SetVideoOffset { .. }
            | Action::ToggleVideoPlaybackLink => self.apply_tempo_film(action),

            // ── ONNX Model Management ─────────────────────────────────────────
            Action::RegisterOnnxModel { .. }
            | Action::StartModelDownload { .. }
            | Action::UpdateModelDownload { .. }
            | Action::CompleteModelDownload { .. }
            | Action::RemoveOnnxModel { .. }
            | Action::QueueOnnxInference { .. }
            | Action::CompleteOnnxInference { .. }
            | Action::FailOnnxInference { .. } => self.apply_onnx_runtime(&action),

            // ── Waveform Peak Cache ───────────────────────────────────────────
            Action::InvalidateWaveform { .. }
            | Action::SetWaveformPeaks { .. }
            | Action::ClearWaveformCache => self.apply_waveform_cache(&action),

            // ── Welcome screen actions ────────────────────────────────────────
            Action::NewProject => {
                // Reset to a blank project: clear tracks, clips, MIDI notes,
                // mixer channels, and transport state.
                self.tracks.clear();
                self.clips.clear();
                self.midi_notes.clear();
                self.mixer_channels.clear();
                self.playhead_beat = 0.0;
                self.playing = false;
                self.project = super::ToneProject::new();
                self.undo_stack.clear();
                self.redo_stack.clear();
            }
            Action::OpenFile => {
                // Stub: actual file-picker I/O wired in a later wave.
                log::info!("tone: OpenFile requested from welcome screen");
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
