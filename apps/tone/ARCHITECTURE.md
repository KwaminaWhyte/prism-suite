# Tone — Architecture

## Overview

Tone is a GPUI desktop app — same chassis as Reel. GPUI owns the window,
composites the chrome, and dispatches events. All audio computation happens on
worker threads that communicate back to the UI via channels. The AI inference
pipeline is decoupled from the audio engine and from the UI.

```
┌──────────────────────────────────────────────────────────┐
│  GPUI host (main thread)                                 │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Tone root view                                  │   │
│  │   ↓ apply(Action)          ↑ read &App           │   │
│  │  App  (app_state::App)                           │   │
│  └────────────┬──────────────────────────────────────┘  │
│               │ Action queue / channel                   │
└───────────────┼──────────────────────────────────────────┘
                │
     ┌──────────┴───────────────────────────┐
     │                                      │
     ▼                                      ▼
┌─────────────────┐              ┌──────────────────────┐
│  Audio Engine   │              │  AI Inference Worker │
│  (CPAL thread)  │              │  (ort / ONNX thread) │
│                 │              │                      │
│  mixer graph    │              │  MusicGen, Demucs,   │
│  → PCM frames   │              │  Magenta, AudioSR    │
│  → device out   │              │  → PCM / MIDI → disk │
└─────────────────┘              └──────────────────────┘
        ↑                                   ↑
   prism-media                        prism-ai (planned)
   (file decode)                      (ort wrapper)
```

---

## GPUI Host

The `Tone` root view in `main.rs` owns the `App` struct (from `app_state.rs`).
Panels borrow `&App` for rendering and emit `Action`s through `cx.listener`.
The root view routes every action through `App::apply`, the single mutation
choke point. This is the identical pattern used by Reel and Pigment.

Panel layout (planned):
- Toolbar (top, 44 px): transport, BPM display, key/scale, AI prompt entry
- Piano roll / arrangement view (centre): switchable via tab
- Mixer dock (right, 280 px): channel strips
- Bottom timeline (120 px): arrangement clips + playhead

---

## Piano Roll Canvas

The piano roll is a custom GPUI draw surface — no egui. It is pixel-addressed
directly in `Render`, drawing:

1. Black/white key grid (vertical axis = MIDI pitch 0-127)
2. Beat/bar ruler (horizontal axis = time in beats)
3. Note rectangles coloured by velocity
4. Velocity lane (bar chart below the note grid)
5. MIDI CC lane (for mod wheel, pitch bend, etc.)

Interaction is handled via GPUI's `on_mouse_down` / `on_mouse_move` / `on_mouse_up`
on a stateful element. Gesture state (drag type: add / move / resize) is stored
in a transient `PianoRollGesture` field on the root view, not in `App`.

---

## Audio Engine (Phase 2)

Built on CPAL (Cross-Platform Audio Library) for real-time output. The engine
runs on a dedicated CPAL callback thread — it never blocks.

```
CPAL callback (audio thread, hard real-time)
  ← reads atomic playhead beat from App
  ← reads clip/note data from a snapshot Arc<AppSnapshot>
  → pulls PCM from prism-media decode cache
  → mixes tracks (apply gain, pan, mute, EQ, comp)
  → writes to CPAL output buffer
```

The audio thread never allocates. It reads from a lock-free snapshot. When the
user changes state (volume, mute, etc.), the main thread builds a new
`Arc<AppSnapshot>` and atomically swaps it. The audio thread picks it up on
the next callback.

For file-based audio, `prism-media::decode_audio` (FFmpeg bridge) decodes
audio files to interleaved f32 PCM on a background thread and stores the
result in a shared decoded-sample cache keyed by file path + range.

---

## AI Inference Pipeline (Phase 3)

Planned via `prism-ai` (not yet promoted from apps/tone). The inference
pipeline is always async / off-thread:

1. `App::apply(Action::GenerateTrack { track_id })` records an
   `AiGenerationJob` with `status = Generating`.
2. The root view's render loop sees a Generating job and spawns an `ort`
   inference task on a Rayon thread pool.
3. The task loads the ONNX model from disk (cached in RAM after first load),
   encodes the prompt, runs inference, decodes PCM, writes a temp `.wav` file.
4. When done, it sends `Action::CompleteAiGeneration { job_id }` over an mpsc
   channel back to the main thread.
5. The main thread calls `App::apply(CompleteAiGeneration)`, which updates the
   stub clip's `source_path` to the temp file, marking it as real audio.

Model choices (local, ONNX-exportable):
- **MusicGen-small** (Meta): text + audio → audio tokens → decode. 300M params,
  runs on CPU in ~30 s for 10 s of audio.
- **Demucs HTDemucs-4** (Meta): audio → 4 stems. ~80M params, runs on CPU.
- **Melody RNN / Music Transformer** (Magenta): MIDI generation and continuation.
- **AudioSR** (ByteDance): audio style transfer / super-resolution.

All models are downloaded once to `~/.local/share/tone/models/` and never
phoned home again. No internet required after the first download.

---

## MIDI Processing

MIDI notes live in `app_state::MidiNote` and are never sent to a real MIDI
device in Phase 1 (stubbed). In Phase 2:

- `midir` crate for MIDI in/out on all platforms.
- Armed Instrument tracks receive live MIDI input and record notes into clips
  when `recording = true`.
- Instrument tracks route to a built-in synthesis engine (Phase 2: simple
  wavetable synth; Phase 3+: SoundFont player via `oxisynth`).

---

## Mixer Routing Graph

```
Track 1 (Audio)  ─────────────────────────────────────┐
Track 2 (MIDI/Instrument) → synth → PCM ──────────────┤
Track 3 (Bus: Reverb)  ← sends from tracks 1,2 ───────┤  → Master out
Track 4 (Bus: Delay)   ← sends from tracks 1,2 ───────┤
Master track (Vol 0.9, Limiter ON) ────────────────────┘
```

Each channel runs its insert chain in order (EQ → Comp → plugin slots).
Bus tracks collect pre-fader sends. The master channel applies the master
limiter as the last stage.

In Phase 1 this routing is modelled in `app_state` (EQ/comp fields) but not
executed. Phase 2 implements real-time execution.

---

## Bounce / Export Pipeline

1. `StartBounce` sets `bounce_in_progress = true`.
2. A worker thread reads the `BounceConfig` + all clips from an `Arc<AppSnapshot>`.
3. For each frame position: mix all non-muted tracks at the configured sample
   rate, applying EQ + compression + master FX chain.
4. Write interleaved f32 PCM to a temp file.
5. Encode to target format via `prism-media` (FFmpeg):
   - WAV: `pcm_s24le` or `pcm_s16le`
   - MP3: `libmp3lame`, VBR quality 2
   - FLAC: `flac`, compression level 8
   - OGG: `libvorbis`, quality 6
6. Send `CancelBounce` / completion action on finish. Report output path.

---

## Crate Dependencies

| Crate | Role |
|-------|------|
| `gpui 0.2.2` | Window, event loop, custom draw surface, text rendering |
| `prism-core` | Shared blend modes, curves, colour model |
| `prism-color` | `Rgba` type, sRGB↔linear conversion |
| `prism-media` | FFmpeg bridge: audio decode (PCM peaks), encode (export) |
| `prism-ui` | Design tokens, icon set, GPUI component library |
| `log` / `env_logger` | Structured logging |
| `anyhow` | Error handling |
| `rfd` | Native file-open/save dialogs |
| `cpal` *(Phase 2)* | Cross-platform real-time audio I/O |
| `midir` *(Phase 2)* | Cross-platform MIDI I/O |
| `ort` *(Phase 3, via prism-ai)* | ONNX Runtime: AI model inference |
| `rayon` *(Phase 3)* | Thread pool for inference tasks |

Tone does NOT use `egui` / `eframe`. GPUI is the sole UI framework.
Tone does NOT use `wgpu` custom pipelines. Piano roll is CPU-rasterised.
