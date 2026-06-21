# Tone

AI-first music creation for the Prism Suite.
GarageBand / Logic Pro / Ableton analog — with AI that beats Suno and Udio on control.

---

## What makes Tone different

| Feature | Suno / Udio | GarageBand | Tone |
|---------|------------|------------|------|
| Text-to-track generation | Yes | No | Yes |
| Edit generated audio | No | Yes (manual) | Yes (AI-aware) |
| Stem separation | No | No | Yes (Demucs) |
| MIDI piano roll | No | Yes | Yes |
| Local / offline | No (cloud) | macOS only | Yes (ONNX) |
| Privacy | Cloud | Local | Local |
| Cross-platform | Web | macOS / iOS | macOS + Linux + Windows |

Generate like Suno, edit like Logic.

---

## Key AI features

- **Text-to-track**: type a prompt, get a full audio clip placed on a new track
- **Chord progression generator**: AI suggests and places chord voicings in the key of your project
- **Drum pattern AI**: generate a kick/snare/hi-hat pattern and place it as editable MIDI
- **Melody harmoniser**: automatically adds harmony voices (major 3rd, 5th, etc.) to a MIDI melody
- **AI mastering**: one-click loudness normalisation + compression + limiting
- **Stem separation**: split any audio clip into drums / bass / melody / harmony tracks
- **Chord suggestion**: AI recommends chords that fit the project key and a mood descriptor

All AI runs locally via ONNX models. No internet required after the first model download.

---

## Build

```bash
# From the repo root
cargo run -p tone

# Tests (no GPU required)
cargo test -p tone

# Release build
cargo build -p tone --release
```

Requires Rust 1.79+ and the Prism Suite workspace (see root `Cargo.toml`).

---

## Architecture

- **Host**: GPUI 0.2.2 (same as Reel)
- **State**: single `App` struct, mutated only via `Action` enum through `App::apply`
- **Audio engine** (Phase 2): CPAL for real-time output, prism-media for file decode/encode
- **AI inference** (Phase 3): ONNX Runtime via `prism-ai` (planned shared crate), worker threads

See `ARCHITECTURE.md` for full details, `PLAN.md` for the roadmap.

---

## Status

Phase 1 scaffold — state machine complete, UI chrome placeholder.
See `PLAN.md` for the full phased roadmap.
