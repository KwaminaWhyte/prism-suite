# Changelog — Tone

All notable changes to Tone will be documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Typeable MusicGen prompt** — type the AI-generation prompt (live `SetAiPrompt`); Generate Music consumes it. Style-tag chips append to the real field.
- **Project name** — typeable → `SetProjectName`.
- **BPM / tempo** — typeable field (pure `parse_bpm`, clamp 20–999) → `SetBpm`; steppers re-seed it.
- **Track + clip rename** — double-click inline rename → `RenameTrack` / `RenameClip`.
- +8 tests (734 → 742).

## [0.10.0] - 2026-06-24

### Added — Real ONNX inference layer
- `inference.rs` — `InferenceBackend` (`Stub` | `Ort`) + `ModelRegistry` over Tone's
  AI families (MusicGen, Demucs, MelodyRnn, MusicTransformer, AudioSr, AiMasterNet,
  VocalAutoTune, VocalIsolate). `run_inference()` prefers the real `ort` path only
  when backend=`Ort` + feature on + a registered model exists, and never errors
  (falls back to the deterministic stub).
- Real `ort` 2.x session load/run/tensor-IO under the optional `onnx` feature
  (`apps/tone/Cargo.toml`: `ort = { version = "=2.0.0-rc.10", optional = true }`,
  `[features] onnx = ["dep:ort"]`). Default build pulls no `ort`. +31 tests.

## [0.9.0] - 2026-06-24

### Added

- **Project model** — `ToneProject` with BPM (20–999), time signature, sample rate (44100/48000/88200/96000), bit depth (16/24/32), key, and scale. Defaults: 120 BPM, 4/4, 44100 Hz, 24-bit, C Major.
- **Track model** — `ToneTrack` with `TrackKind` (Audio / MIDI / Instrument / Bus / Master), mute, solo, arm, volume (0–2), pan (-1–1), sends to reverb and delay buses, optional instrument name. App starts with a Master track pre-populated.
- **Clip model** — `ToneClip` with `ClipKind` (Audio / MIDI / AiGenerated), start beat, duration in beats, gain (0–4), pitch shift (-24–24 semitones), time-stretch ratio (0.5–2.0), loop, mute, colour, and optional AI prompt string.
- **Piano roll state** — `MidiNote` (pitch 0–127, velocity 0–127, beat position/duration) and `MidiController` (CC number, beat position, value). Piano roll clip focus, zoom, and scroll tracked in `App`.
- **Mixer model** — `MixerChannel` per track: 3-band EQ (low/mid/high, ±12 dB), compressor (threshold -60–0 dB, ratio 1–20), compressor enable toggle, ordered insert-effect chain, pre/post-fader and peak metering fields.
- **Master bus** — master volume (0–2) and master limiter toggle on `App`.
- **Transport state** — playhead beat, playing, recording, loop enabled, loop start/end, metronome enabled.
- **AI generation** — `AiGenerationJob` with prompt, style, duration in bars, `AiGenerationStatus` (Idle / Generating / Done / Failed), output clip id, and requested stem list.
- **Bounce config** — `BounceConfig` with `BounceFormat` (WAV / MP3 / FLAC / OGG / Stems), sample rate, bit depth, normalise, dither, export path, master FX flag, stems-per-track flag.
- **Action enum** — 120+ actions covering all feature areas; `App::apply` is the single mutation choke point.
- **Actions: Project** — `SetBpm`, `SetTimeSignature`, `SetSampleRate`, `SetBitDepth`, `SetProjectKey`, `SetProjectScale`, `SetProjectName`.
- **Actions: Tracks** — `AddTrack`, `DeleteTrack`, `RenameTrack`, `SetTrackMute`, `SetTrackSolo`, `SetTrackArm`, `SetTrackVolume`, `SetTrackPan`, `SetTrackColor`, `SetTrackInstrument`, `DuplicateTrack`, `ReorderTracks`, `SetActiveTrack`.
- **Actions: Clips** — `AddClip`, `DeleteClip`, `MoveClip`, `ResizeClip`, `SetClipGain`, `SetClipPitchShift`, `SetClipTimeStretch`, `SetClipLoop`, `SetClipMute`, `SplitClip`, `MergeClips`.
- **Actions: Piano roll / MIDI** — `OpenPianoRoll`, `ClosePianoRoll`, `AddMidiNote`, `DeleteMidiNote`, `MoveMidiNote`, `ResizeMidiNote`, `SetMidiNoteVelocity`, `SelectAllNotesInClip`, `QuantizeMidiNotes`, `TransposeMidiNotes`, `SetPianoRollZoom`, `SetPianoRollScroll`.
- **Actions: Mixer** — `SetChannelEqLow`, `SetChannelEqMid`, `SetChannelEqHigh`, `SetChannelCompThreshold`, `SetChannelCompRatio`, `ToggleChannelComp`, `AddChannelEffect`, `RemoveChannelEffect`, `SetMasterVolume`, `ToggleMasterLimiter`.
- **Actions: Transport** — `Play`, `Stop`, `Pause`, `Record`, `SetPlayheadBeat`, `ToggleLoop`, `SetLoopRange`, `ToggleMetronome`, `Rewind`, `FastForward`.
- **Actions: AI** — `SetAiPrompt`, `SetAiStyle`, `SetAiDurationBars`, `GenerateTrack`, `CompleteAiGeneration`, `GenerateChordProgression`, `GenerateDrumPattern`, `HarmonizeMelody`, `AiMasterTrack`, `SuggestChords`.
- **Actions: Bounce** — `SetBounceFormat`, `SetBounceNormalize`, `SetBounceDither`, `SetBounceExportPath`, `SetBounceStemsPerTrack`, `StartBounce`, `CancelBounce`.
- **GPUI host** — `main.rs` initialises the app window with the same entry-point pattern as Reel (1600×1000, full display bounds, `PrismAssets`). Renders a minimal placeholder toolbar and timeline strip.
- **120+ unit tests** covering every action, all clamp boundaries, compound workflows, and AI stub behaviours.
- **PLAN.md** — 6-phase roadmap to 90% GarageBand/Logic Pro parity and AI feature leadership.
- **ARCHITECTURE.md** — GPUI host design, piano roll canvas plan, audio engine and AI inference pipeline design.
- **RESEARCH.md** — competitor analysis (GarageBand, Ableton, Suno/Udio), AI model choices (MusicGen, Demucs, Magenta), audio format decisions, 10 UX principles.
