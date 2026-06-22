# Tone — Phased Roadmap to 90% GarageBand/Logic Pro Parity + AI Supremacy

Target: surpass GarageBand/Logic Pro on AI features while matching them on DAW essentials.
Current status: Batch 4 complete (~70% parity, 513 tests). Beat detection, clip slot launching, chord progression tools, track freeze/stem export, MIDI output routing, virtual instruments (8 kinds), and arpeggiator engine all implemented.

---

## Parity Tracking

| Batch | Feature area | Status | Tests |
|-------|-------------|--------|-------|
| 1 | Foundation scaffold (project, tracks, clips, MIDI, mixer, transport, AI stubs, export) | Done | 120+ |
| 2 | Phase 2 extensions (CC lanes, note ops, quantize, sends, groups, history, loop bars, automation, tempo, scenes, plugins, MIDI control) | Done | 170+ |
| 3 | Phase 3 (audio engine, recording, step sequencer, score view, bus routing) | Done | 448+ |
| 4 | Beat detection + warp markers, clip slot launching, chord tools, freeze/stems, MIDI routing/VI/arp | Done | 513+ |
| Phase 2 UI | Core DAW UI (GPUI piano roll, mixer, timeline) | Not started | — |
| Phase 3 AI | AI Generation (ONNX, MusicGen, Demucs, Magenta) | Not started | — |
| Phase 4 AI | AI Power Tools (mastering, vocal, arrangement, smart mix) | Not started | — |
| Phase 5 | Live Performance (session view, MIDI controllers, hardware sync) | Not started | — |
| Phase 6 | Advanced (VST3, surround, spectral, notation) | Not started | — |

Overall parity: **~70%** (comprehensive DAW state machine with beat detection, clip launching, chord tools, freeze/stem export, MIDI routing).

---

## Phase 1 — Foundation (months 1-3)

Goal: a real, testable DAW state machine with complete MIDI editing logic.

- [x] Project model: BPM, time signature, key/scale, sample rate, bit depth
- [x] Track model: Audio / MIDI / Instrument / Bus / Master kinds; mute / solo / arm / pan / volume
- [x] Clip model: audio + MIDI clips; gain / pitch shift / time stretch / loop / mute / split / merge
- [x] MIDI note model: pitch / velocity / position / duration; quantize / transpose
- [x] Piano roll state: clip focus, zoom, scroll, note selection
- [x] Mixer channel model: 3-band EQ, compressor, insert chain
- [x] Master bus: volume fader + limiter toggle
- [x] Transport: play / pause / stop / record / rewind / fast-forward / loop / metronome
- [x] Bounce config: format (WAV/MP3/FLAC/OGG/Stems), normalise, dither, path
- [x] AI stubs: GenerateTrack / GenerateChordProgression / GenerateDrumPattern / HarmonizeMelody / AiMasterTrack / SuggestChords
- [x] 120+ unit tests covering all actions

**Milestone:** `cargo test -p tone` passes cleanly.

---

## Phase 2 — Core DAW (months 4-6)

Goal: real-time audio playback and a functional GPUI piano roll + mixer UI.

### 2a — Audio Engine
- Integrate CPAL for real-time audio output
- PCM mixing pipeline: read clips → apply gain/pan/mute → mix to stereo
- Track send routing (reverb / delay bus)
- Waveform peak cache (mirroring Reel's `WaveformCache`) for timeline display

### 2b — Piano Roll Canvas
- GPUI custom draw surface for note grid
- Note drag-to-add, click-to-delete, drag-to-move/resize
- Velocity lane below the note grid
- Zoom (horizontal = time, vertical = pitch) with scroll
- Quantize panel (1/4, 1/8, 1/16, 1/32, triplets, swing)
- MIDI CC lane (mod wheel, expression, pitch bend)

### 2c — Mixer UI
- GPUI channel strip components: fader, knob, label, level meter
- 3-band EQ visualiser (simple frequency response curve)
- Compressor gain-reduction meter
- Insert slot list with drag reorder

### 2d — Timeline
- Beat-grid ruler with BPM snap
- Clip blocks: waveform thumbnail for audio, note preview for MIDI
- Drag to move, edge drag to resize, razor split
- Loop region drag handles

**Milestone:** a real session can be built and heard in the app.

---

## Phase 3 — AI Generation (months 7-10)

Goal: text-to-track, stem generation, chord suggestion — all local (ONNX).

### 3a — ONNX Inference Pipeline
- `prism-ai` crate (planned shared crate): wraps `ort` (OnnxRuntime Rust binding)
- Model asset management: download + cache models in `~/.local/share/tone/models/`
- Async inference worker thread → channel → main thread progress update

### 3b — Text-to-Track (MusicGen / AudioCraft)
- Wire `GenerateTrack` action to real MusicGen-small ONNX model
- Encode the prompt + style tag → tokens → audio tokens → decode to PCM
- Write output to a tmp `.wav` file, attach as an `Audio` clip on the target track
- Streaming progress: model reports generation bars as they complete

### 3c — Stem Separation (Demucs)
- `SeparateStems { clip_id }` action: run Demucs HTDemucs-4 model on an audio clip
- Output 4 stems (drums / bass / other / vocals) as separate `Audio` clips on new tracks
- Runs in background; progress updated via `ai_jobs`

### 3d — MIDI Generation (Magenta)
- `GenerateMelody { track_id, bars, temperature }`: Melody RNN / Music Transformer
- `ContinueMelody { clip_id, bars }`: prime model with existing notes, extend
- `GenerateChordProgression` (real): chord2melody model → realistic voice-led chords

### 3e — Style Transfer
- `ApplyStyle { clip_id, style_prompt }`: encode audio, shift timbre toward target style
- Powered by AudioSR or a fine-tuned MusicGen model checkpoint

**Milestone:** generate a 30-second lo-fi track from a text prompt in under 60 s on CPU.

---

## Phase 4 — AI Power Tools (months 11-14)

Goal: surpass Suno/Udio on control — features they can never offer.

### 4a — AI Mastering
- `AiMasterTrack` (real): run loudness normalisation → multiband compression → limiting
- LUFS target selection (Spotify -14, YouTube -14, CD -9)
- Before/after A/B comparison

### 4b — Vocal Isolation / Harmony
- `IsolateVocals { clip_id }`: Demucs vocals stem (already in 3c, surfaced as separate action)
- `AutoTune { clip_id, key, strength }`: pitch-to-scale quantise in real time
- `GenerateVocalHarmony { clip_id, voices: u8 }`: clone + pitch-shift + formant-shift

### 4c — Melody Harmoniser (real)
- Replace the semitone-shift stub with a chord-aware harmoniser
- Key analysis of existing MIDI → suggest compatible chord tones
- User picks interval (3rd, 5th, octave) and voice count

### 4d — Intelligent Arrangement
- `SuggestArrangement { total_bars }`: AI proposes intro / verse / chorus / bridge structure
- Places generated AI clips into the suggested blocks
- User can accept/reject per section

### 4e — Smart Mixing
- `AutoMix`: analyses frequency content of all tracks, applies complementary EQ cuts
- Level balancing: adjusts faders so RMS levels sit in target range
- Panning law: widens the mix by distributing instruments in the stereo field

**Milestone:** upload a recorded vocal + beat, press "AI Mix + Master", get a release-ready track.

---

## Phase 5 — Live Performance (months 15-18)

Goal: Ableton-style session view + hardware MIDI controller integration.

### 5a — Session View (Clip Launcher)
- Grid: tracks as columns, scenes as rows
- Tap a cell to queue, tap scene to launch all tracks in sync
- Record new clips directly into session cells
- Loop record mode (keep overdubbing until stopped)

### 5b — MIDI Controller Support
- MIDI in/out via `midir` crate
- Auto-map: learn controller knobs/pads to Tone actions
- Preset maps for popular controllers (Akai MPC, Novation Launchpad, Push 2)
- Velocity-sensitive pads: direct MIDI input to armed instrument tracks

### 5c — Loop Recording
- Record multiple takes in loop mode, keep all as stacked clips
- Comp editor: pick the best region from each take, merge to master comp

### 5d — Hardware Sync
- MIDI clock send/receive (sync Tone tempo to external hardware)
- Ableton Link for wireless multi-device sync

**Milestone:** a live set can be performed with a hardware controller, fully in sync.

---

## Phase 6 — Advanced (months 19-24)

Goal: pro-level features matching Logic Pro.

### 6a — Plugin System (VST3 / AU)
- `vst3-sys` or `nih-plug` for VST3 host
- AudioUnit host on macOS
- Plugin scanner + blacklist
- GUI sandboxing (run plugin UI in child process)

### 6b — Surround Sound
- 5.1 / 7.1 channel strip routing
- Spatial audio panner (binaural head tracking via AirPods API on macOS)
- Dolby Atmos object-based export

### 6c — Spectral Editing
- FFT display on audio clips (spectrogram view)
- Paint-to-remove specific frequencies (spectral repair)
- Spectral stretch: change duration without pitch artefacts

### 6d — Notation View
- Render MIDI clips as standard notation (treble/bass clef)
- Edit notes in notation view (bidirectional sync with piano roll)
- PDF export for sheet music

### 6e — Score + Sync
- Tempo map editor (tempo changes, time signature changes per bar)
- Video import: lock tempo map to video frames (Film Scoring mode)

**Milestone:** 90% GarageBand/Logic Pro parity score.

---

## Competitive Benchmark vs. Suno / Udio

| Feature | Suno/Udio | Tone target |
|---------|-----------|-------------|
| Text prompt → track | Yes | Yes (Phase 3) |
| Editing generated audio | No | Yes (Phase 1+) |
| Local / offline | No (cloud) | Yes (ONNX) |
| MIDI piano roll | No | Yes (Phase 2) |
| Stem separation | No | Yes (Phase 3) |
| Real-time audio engine | No | Yes (Phase 2) |
| Plugin host (VST3) | No | Yes (Phase 6) |
| Surround audio | No | Yes (Phase 6) |
| MIDI controller | No | Yes (Phase 5) |
| Session/clip launcher | No | Yes (Phase 5) |
| AI mastering | Limited | Yes (Phase 4) |
| Vocal harmoniser | No | Yes (Phase 4) |
