# Tone — Research Notes

## 1. Competitor Analysis

### GarageBand / Logic Pro (Apple)

**Strengths:**
- Tightest hardware integration of any DAW (Apple Silicon, AirPods spatial audio, iPad Remote)
- Best-in-class live loops / session view in GarageBand
- Smart Tempo: automatically detects and conforms audio to the project tempo
- Drummer: AI-generated drum tracks with real drummer humanisation (velocity + timing swing)
- Beat detection: audio follows changes in project BPM without time-stretch artefacts
- Flex Time / Flex Pitch: audio-to-MIDI and timing/pitch correction without bouncing
- Logic Pro's Step Sequencer and Arpeggiator are deep and musical
- Excellent reverb / delay algorithms (ChromaVerb, Space Designer convolution)

**Weaknesses:**
- macOS / iOS only — zero Linux / Windows reach
- No text-to-music generation
- AI features are deterministic (Drummer = pre-recorded samples, not neural)
- No stem separation
- Cannot export as stems from GarageBand (Logic only, with Stem Splitter which is Spleeter-level)
- Plugin format: AU only in GarageBand, VST3 added in Logic 10.7 only

**Parity targets for Tone:**
- Smart Tempo equivalent: auto-detect BPM from audio clip
- Drummer equivalent: `GenerateDrumPattern` → real AI model (Groove Transformer)
- Step Sequencer: dedicated step-grid view for percussion programming
- Flex Time / Pitch: time-stretch and formant-preserving pitch shifting on audio clips

---

### Ableton Live

**Strengths:**
- Best session / clip launcher UX (industry standard for live performance)
- Max for Live: a Turing-complete patch environment inside the DAW
- Warping: audio time-stretch with transient-based modes (Beats, Complex, Tones)
- Excellent MIDI follow actions for generative music
- Push hardware controller integration is best-in-class

**Weaknesses:**
- No AI text-to-audio generation
- Session view clips can't hold MIDI sequences longer than 8 bars without awkward workarounds
- Expensive ($749 Suite)
- Closed plugin ecosystem for mobile/live extensions

---

### Suno / Udio (AI music generation)

**Strengths:**
- Impressive audio quality for text prompts
- Handles vocals + instruments end-to-end
- Very low barrier: paste a prompt, get a track in 30 s

**Critical weaknesses (Tone's opportunity):**
- No editing: you get a flat audio file. If one part is wrong, regenerate from scratch
- No stems: can't isolate or replace individual elements
- Cloud-only: internet required, data sent to third-party servers
- No MIDI: impossible to feed the output into a traditional DAW
- No tempo / key control: BPM and key emerge from the model, can't be specified reliably
- No arrangement control: song structure is decided by the model
- No mixing: all stems are baked into one stereo file
- No plugin support: can't process the output with EQ, comp, effects
- No live performance: can't launch clips, sync to MIDI clock, or use a controller

**Tone's differentiator:** generate like Suno, then edit like Logic.

---

## 2. AI Model Choices

### Text-to-Audio: MusicGen (Meta)

- Paper: "Simple and Controllable Music Generation" (Copet et al., 2023)
- Model sizes: small (300M), medium (1.5B), large (3.3B)
- ONNX-exportable via `audiocraft` + `onnxruntime`
- Tone default: MusicGen-small (fits in 4 GB RAM, runs on CPU in ~25 s for 10 s)
- Conditioning: text prompt + melody conditioning (existing audio as melodic reference)
- Licence: CC-BY-NC 4.0 (non-commercial initially; watch for community variants)

**Alternatives:**
- Stable Audio Open (Stability AI, Apache 2.0) — shorter training window, good for effects
- AudioCraft's AudioGen (sound effects, not music)

### Stem Separation: Demucs HTDemucs-4 (Meta)

- Paper: "Hybrid Transformers for Music Source Separation" (Défossez et al., 2023)
- 4 stems: drums / bass / other / vocals
- ONNX-exportable. CPU inference: ~3× real-time for 4-minute track
- Licence: MIT

**Alternatives:**
- Spleeter (Deezer, MIT) — faster, lower quality
- Open-Unmix (Fraunhofer, MIT) — balanced quality / speed

### MIDI Generation: Magenta / Music Transformer

- Melody RNN: sequence LSTM for melody continuation, tiny model (~10 MB)
- Music Transformer (Huang et al., 2018): attention-based long-range musical structure
- Both available as TFLite → ONNX via unofficial converters
- Google Magenta is Apache 2.0

### Chord Suggestion: Chord-Conditioned Generation

- Hooktheory progression database (reference, not a model)
- ChatMusician (LLM fine-tuned on ABC notation): can be prompted for chord progressions
- Practical stub: embed a lookup table of 50 common progressions per key + mood, then upgrade to neural in Phase 3

### Vocal Harmony / Harmoniser

- Pitch shifter: formant-preserving pitch shift via PSOLA or phase vocoder (Phase-Vocoder in `rubberband` crate)
- Neural option: Matchering (style transfer for mixing reference)

---

## 3. Audio Format Decisions

| Format | Use case | Codec |
|--------|----------|-------|
| WAV PCM 24-bit | Mastering / archive | `pcm_s24le` |
| WAV PCM 16-bit | CD distribution | `pcm_s16le` |
| FLAC | Lossless distribution | `flac` |
| MP3 320 kbps CBR | Streaming / share | `libmp3lame` |
| OGG Vorbis q6 | Web / game audio | `libvorbis` |
| AIFF | Logic / Pro Tools interop | `pcm_s24be` |

Internal working format: f32 interleaved PCM at the project sample rate.
sRGB↔linear boundary does not apply to audio — but audio uses linear amplitude
throughout (same as Tone's linear-light model for pixels in Pigment).

---

## 4. UI / UX Principles for Tone

1. **Generate-first, edit-second.** The AI prompt bar is always visible in the
   toolbar — not buried in a menu. Generation is one keystroke away at all times.

2. **Nothing is locked.** Every AI-generated clip is immediately editable in the
   piano roll or as audio. The user is never stuck with a flat file.

3. **Beat is king.** All positions in Tone are expressed in beats, not seconds.
   Zooming in the timeline reveals more beat subdivisions; zooming out shows bars.
   This matches how musicians think.

4. **Stems are first-class.** Every clip can be exploded into stems. Stems appear
   as new tracks, not as a separate view. You can mute the drums stem and keep
   everything else.

5. **Colour communicates kind.** Audio clips are amber. MIDI clips are blue.
   AI-generated clips are violet. Colour is consistent across the app and cannot
   be overridden for the kind-indicator portion.

6. **One undo step per action.** The `Action` enum is the undo log. Every action
   is reversible. Undo/redo is implemented by replaying the inverse action or
   storing the prior state snapshot.

7. **The piano roll is a first-class view, not a modal.** Selecting a MIDI clip
   expands the timeline into a split view with the piano roll below. Closing the
   piano roll returns to the arrangement, but the note state is retained.

8. **AI feedback is immediate and honest.** A generating clip shows a progress
   bar with an estimated time remaining. A failed generation shows a clear error
   message with a Retry button. The status is never silent.

9. **Local-first privacy.** No audio data ever leaves the device without
   explicit user action (export). AI models run locally. Tone has no accounts,
   no sign-in, no telemetry.

10. **Less chrome, more music.** Panels are collapsible. The minimum viable view
    is the timeline + transport. Everything else is opt-in. Keyboard shortcuts
    cover 95% of actions.

---

## 5. Lessons from Existing Prism Apps

- **Follow the Reel pattern:** GPUI host + `App` / `Action` / `apply` works well;
  avoid the egui split that Contour and Pulse have inherited.
- **Serde-free state:** the `Action` enum doubles as a serialisable undo log
  (when/if serialisation is added later, add serde then — don't bake it in early).
- **prism-media for I/O:** already handles decode/encode via FFmpeg; Tone reuses
  it rather than adding another audio I/O crate at the shared level.
- **Don't promote to shared prematurely:** `prism-ai` should only become a shared
  crate when a second app (e.g. Pigment's neural upscale) actually needs it.
