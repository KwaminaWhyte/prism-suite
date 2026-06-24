# Tone — Phased Roadmap to 90% GarageBand/Logic Pro Parity + AI Supremacy

Target: surpass GarageBand/Logic Pro on AI features while matching them on DAW essentials.
Current status: Batch 4 complete (~100% parity, 513 tests). Beat detection, clip slot launching, chord progression tools, track freeze/stem export, MIDI output routing, virtual instruments (8 kinds), and arpeggiator engine all implemented.

---

## Parity Tracking

| Batch | Feature area | Status | Tests |
|-------|-------------|--------|-------|
| 1 | Foundation scaffold (project, tracks, clips, MIDI, mixer, transport, AI stubs, export) | Done | 120+ |
| 2 | Phase 2 extensions (CC lanes, note ops, quantize, sends, groups, history, loop bars, automation, tempo, scenes, plugins, MIDI control) | Done | 170+ |
| 3 | Phase 3 (audio engine, recording, step sequencer, score view, bus routing) | Done | 448+ |
| 4 | Beat detection + warp markers, clip slot launching, chord tools, freeze/stems, MIDI routing/VI/arp | Done | 513+ |
| 5 | MusicGen, Demucs, Magenta, AI mastering, vocal tools, smart mix, MIDI controllers, loop recording, hardware sync, VST host, surround, spectral, notation, tempo film | Done | 681+ |
| Phase 2 UI | Core DAW UI (GPUI piano roll, mixer, timeline) | In Progress | — |
| Phase 3 AI | Real ONNX model inference (promote prism-ai) | Planned | — |
| Phase 4 | Further polish, VST3 GUI, real-time audio I/O | Planned | — |

Overall parity: **~90%** (comprehensive DAW state machine with beat detection, clip launching, chord tools, freeze/stem export, MIDI routing, MusicGen, Demucs, Magenta, AI mastering, vocal tools, smart mix, MIDI controllers, loop recording, hardware sync, VST host, surround, spectral, notation, tempo film).

### Batch 5 (done — 681 tests total)
- [x] MusicGen AI — queue/start/progress/complete/cancel/retry job pipeline (14 tests)
- [x] Demucs stem splitting — queue/start/progress/complete/cancel (10 tests)
- [x] Magenta melody/continuation/chord voicing — 3 job pipelines (14 tests)
- [x] AI Mastering — LUFS targeting, A/B toggle, multiband comp state (14 tests)
- [x] Vocal Tools — autotune config, harmony config, isolation job pipeline (12 tests)
- [x] Smart Mix — auto-mix session, frequency analysis, suggestions (12 tests)
- [x] MIDI Controllers — hardware controller registration, pad/knob learn, presets (14 tests)
- [x] Loop Recording — take stacks, comp regions, bake-to-clip (12 tests)
- [x] Hardware Sync — MIDI clock source/BPM, Ableton Link peers/quantum/start-stop (14 tests)
- [x] VST Host — scan, load, unload, bypass, preset, blacklist (12 tests)
- [x] Surround / Atmos — format, binaural monitor, surround pan, Atmos objects (12 tests)
- [x] Spectral Editing — FFT size, color map, brush strokes, stretch jobs (12 tests)
- [x] Notation View — staves, clef/key/transpose, PDF export job (12 tests)
- [x] Tempo Film / Video Lock — tempo changes, time-sig changes, video lock/offset (12 tests)
| Phase 2 UI | Core DAW UI (GPUI piano roll, mixer, timeline) | Complete | — |
| Phase 3 AI | AI Generation (ONNX, MusicGen, Demucs, Magenta) | Complete (ONNX stubs) | — |
| Phase 4 AI | AI Power Tools (mastering, vocal, arrangement, smart mix) | Complete (stubs) | — |
| Phase 5 | Live Performance (session view, MIDI controllers, hardware sync) | Not started | — |
| Phase 6 | Advanced (VST3, surround, spectral, notation) | Not started | — |

Overall parity: **~100%** (comprehensive DAW state machine with beat detection, clip launching, chord tools, freeze/stem export, MIDI routing).

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

---

## Child Windows & Secondary UI

Tone uses GPUI's `cx.open_window(...)` for all secondary windows. The welcome screen is already a floating child window wired to dispatch `NewProject`, `OpenFile`, and template preset actions. The windows below are the remaining secondary UI surface needed to reach parity with Logic Pro / GarageBand's multi-window model.

### Welcome / Home Screen (already implemented)
- **Kind:** `WindowKind::Floating` (900×560 px)
- **Phase:** done — shown on launch when no project is open; recent projects list, New Project (with BPM / time-sig / key defaults), template grid (Electronic / Hip-Hop / Acoustic / Film Score), Open…; dismissed on project load.

### Detachable Piano Roll
- **Kind:** `WindowKind::Floating` (1100×600 px)
- **Phase:** Phase 2 (Core DAW — Piano Roll Canvas)
- The piano roll is the primary MIDI editing surface. When detached it becomes a full floating window: note grid with horizontal-time / vertical-pitch axes, velocity lane below, MIDI CC lane drawer, zoom controls, quantize toolbar (1/4 / 1/8 / 1/16 / triplets / swing), selection/pencil/eraser tools, chord-highlight overlay, and scale-assist mask. Opened by double-clicking a MIDI clip on the timeline or from Window ▸ Piano Roll. Syncs bidirectionally with the timeline clip via `Model<App>` — edits in the piano roll immediately reflect in the timeline view and vice versa. Logic Pro / Ableton both support a detached piano roll as a separate OS window; this matches that pattern.

### Detachable Mixer
- **Kind:** `WindowKind::Floating` (900×360 px)
- **Phase:** Phase 2 (Core DAW — Mixer UI)
- Full floating mixer window with one channel strip per track plus master: fader, pan knob, mute/solo/arm buttons, 3-band EQ curve visualizer, compressor gain-reduction meter, insert-slot list with drag reorder, send-level knobs to bus tracks. Opened from Window ▸ Mixer (Cmd+2). Stays open alongside the timeline so producers can mix while arranging, mirroring Logic Pro's separate Mixer window. Shares state via `Model<App>`.

### VST Plugin GUI Windows
- **Kind:** `WindowKind::Floating` (plugin-defined size, default 640×400 px)
- **Phase:** Phase 6 (Advanced — Plugin System VST3/AU)
- One child window per loaded VST3/AU plugin instance whose GUI is activated. Plugin GUIs are embedded inside a Tone-hosted OS window (the plugin draws into a native view; Tone wraps it in a `WindowKind::Floating` GPUI window with a thin title bar showing the plugin name and a bypass toggle). Multiple plugin windows can be open simultaneously — one per instance. Each window remembers its position in `AppPrefs.plugin_window_positions: HashMap<PluginInstanceId, (f32,f32)>`. Matches Logic Pro's floating plugin window model.

### Export / Bounce
- **Kind:** `WindowKind::Floating` (640×480 px)
- **Phase:** Phase 2 / early Phase 3 (the bounce config already exists in `BounceConfig`)
- Promote the existing `BounceConfig` (WAV/MP3/FLAC/OGG/Stems, normalize, dither, path) from a data struct to a proper Export window. Format tabs: Stereo Mix / Stems / MIDI. Stereo Mix: format selector, bit depth, sample rate, normalize toggle, dither type, loudness target (LUFS for streaming presets: Spotify -14 / YouTube -14 / CD -9), output path. Stems: per-track toggle list (which tracks to include), same format options. MIDI: clip selector, SMF format (0/1). Opened from File ▸ Bounce Project to Disk… (Cmd+B). Matches Logic Pro's Bounce dialog.

### Preferences
- **Kind:** `WindowKind::Floating` (720×560 px)
- **Phase:** Phase 2 polish / Phase 4
- Tabs: General (undo levels, auto-save interval, default BPM/time-sig/key, metronome volume), Audio (device, buffer size, sample rate, latency display, ASIO/CoreAudio), MIDI (controller devices, clock source, program-change behavior), Plug-ins (VST/AU scan paths, blacklist management, scan on startup), AI (model cache directory, backend CPU/CoreML/CUDA, bandwidth cap for model downloads), User Interface (theme, waveform color, UI scale). Persisted to `~/.config/prism/tone_prefs.json`.

### AI Progress
- **Kind:** `WindowKind::PopUp` (440×200 px)
- **Phase:** Phase 3 (AI Generation) and Phase 4 (AI Power Tools)
- Shown above all windows during ONNX inference: MusicGen text-to-track generation, Demucs stem separation, Magenta melody/continuation, AI Mastering pipeline, vocal isolation/autotune. Displays job name (e.g. "MusicGen: generating 30 s"), animated progress bar, elapsed time, and a Cancel button that sends a cancellation token to the worker thread. Uses `WindowKind::PopUp` so it appears above open plugin windows and the floating mixer.

### Implementation notes
- All child windows share state via `Model<App>` passed at `cx.open_window(...)` time — the Piano Roll and Mixer windows read the same `Project` / `Tracks` state the timeline panel reads.
- Piano Roll and Mixer are "workspace windows" — they persist across document sessions and their open/closed states are saved in `AppPrefs.workspace: WorkspaceLayout { piano_roll_open, mixer_open, piano_roll_pos, mixer_pos }`.
- VST plugin windows are managed by a `PluginWindowRegistry` that maps `PluginInstanceId → WindowHandle` and closes a plugin's window when the plugin is unloaded.
- `WindowKind::PopUp` (AI Progress) is centered on the main window via `Bounds::centered(Some(main_window_handle), ...)`, has `is_movable: false`, and no traffic-light buttons.
- Title bar for Floating windows: `TitlebarOptions { title: Some("Piano Roll".into()), appears_transparent: false, traffic_light_position: None }`.
