# Tone — Research Notes
*Updated June 2026 — deep competitive + AI model analysis (60+ sources)*

---

## 1. Competitor Matrix

| Feature | Suno AI | Udio | GarageBand | Logic Pro | FL Studio | Ableton Live | Adobe Audition |
|---------|---------|------|------------|-----------|-----------|--------------|----------------|
| Platform | Web/cloud | Web/cloud | Mac/iOS only | Mac only | Win/Mac | Win/Mac/Linux | Win/Mac |
| Price | $0–$30/mo | $0–$30/mo | Free | $199 one-time or $12.99/mo | $99–$449 one-time | $99–$749 one-time | $22.99/mo (CC) |
| Offline | No | No | Yes | Yes | Yes | Yes | Yes |
| AI full-song gen | Yes | Yes | No | No | No | No | No |
| AI Session Players | No | No | Basic Drummer | Drummer, Bass, Keys, Synth, Chord ID | No | No | No |
| Step sequencer | No | No | Basic | No | Yes (signature) | No | No |
| Piano roll | No | No | Basic | Excellent | Best-in-class | Good | None |
| Ghost notes | No | No | No | No | Yes (unique) | No | No |
| Scale lock | No | No | No | Limited | Yes (project-wide) | Yes (clip-level) | No |
| Clip launcher | No | No | No | No | No | Yes (Session View) | No |
| MIDI export | Premier only (credit-gated) | None | Yes | Yes | Yes | Yes | No |
| Stem export | 12 stems (Premier only) | Disabled (2025–26) | No | No | No | No | No |
| VST/AU hosting | No | No | AU only | AU only | Yes | Yes | Yes |
| Commercial rights | Pro+ only | Suspended (licensing transition) | Yes | Yes | Yes | Yes | Yes |
| Linux | No | No | No | No | No | Yes | No |
| One-time purchase | No | No | Free | Yes (or sub) | Yes, lifetime updates | Yes | No |
| Open source | No | No | No | No | No | No | No |
| Copyright lawsuits | Active (Sony, Jul 2026 ruling) | UMG/WMG settled | None | None | None | None | None |

---

## 2. UX Wins / Fails Per Competitor

### Suno AI

**Copy these:**
- Zero-friction entry: one text box, one button, full song in 30 seconds. Best onboarding funnel in the category.
- Song continuation ("extend") pattern: building longer pieces by extending from any point is natural.
- Stem view stacks tracks visually — intuitive even for non-producers.
- "Persona" custom voice model concept (train a vocal character).

**Avoid these:**
- Credit system creates anxiety before every click — users feel penalized for iteration.
- Failed/rejected generations still consume credits. Predatory.
- Commercial rights permanently bound to plan tier at generation time — users locked out retroactively.
- Post-Warner deal (November 2025): ownership shifted from "you own it" to "users are generally not considered the owner" with no user notification. Trust-destroying.
- No reliable way to specify key, BPM, chord progression, or time signature from text prompts.

### Udio

**Copy these:**
- **Inpainting (2-second segment regeneration)**: select any region, regenerate only that section. The most powerful editing primitive in AI music. Extend to bar-level in Tone.
- 48kHz stereo output — audiophile-grade, better than Suno on instrumentals.
- Style Blending: mix two sonic references into a hybrid.
- Pay-as-you-go credits that never expire.

**Avoid these:**
- Disabling downloads entirely during licensing transition destroyed user trust.
- No custom voice training — vocal inconsistency across tracks.
- No MIDI export at any tier.
- 2-minute single-generation cap.

### GarageBand

**Copy these:**
- **Smart Instruments**: tap a chord shape → realistic strummed guitar. Abstracting instrument technique behind a simple gesture.
- **Loop Browser**: filter by genre/instrument/mood, drag to track. Dead simple search-and-preview.
- **Auto-detect tempo and key** from any audio recording. Never asks user to set BPM manually.
- **One-click recording** with visual countdown.
- **Track templates** ("Hip Hop Beat," "Singer-Songwriter"): pre-configure routing and instruments.

**Avoid these:**
- No VST/AU plugin support — closed ecosystem.
- No stem export, no project format usable by other DAWs.

### Logic Pro

**Copy these:**
- **Session Players (Drummer, Bass Player, Keyboard Player, Synth Player)**: AI instruments that read the Chord Track and produce realistic MIDI performances. The Chord Track as single source of truth is the correct architecture — Tone adopts this.
- **Chord Track + Chord ID**: define harmony once, everything follows. MIDI regions, Session Players, transposition all adapt. Chord ID detects chords from audio or MIDI in one click.
- **Piano roll**: score-aligned, smart quantize, MIDI draw mode, region-based editing — industry reference quality.
- **Flex Pitch**: edit individual pitches in polyphonic audio non-destructively.

**Avoid these:**
- Mac-only permanently excludes the majority of global music producers.
- Feature depth creates extremely long learning curve.

### FL Studio

**Copy these:**
- **Piano roll is the industry benchmark**: ghost notes, scale highlighting (project-wide), brush tool for painting, per-note parameters (pan, velocity, pitch, release, filter cutoff, resonance), waveform display inside piano roll, slide/portamento note visualization.
- **Ghost notes specifically**: notes from other patterns as translucent overlays in the active editor. The single most envied FL Studio feature. Ableton forum petitions date to 2013 and remain unresolved. **Tone implements ghost notes on day one.**
- **Step sequencer + piano roll duality**: same MIDI data editable in either view. Beginners start in step grid; advanced users move to piano roll. Zero friction between views.
- **One-time purchase with lifetime free updates**: strongest customer loyalty driver in the category.
- **Pattern-based workflow**: each pattern is self-contained and reusable anywhere in arrangement.

**Avoid these:**
- Context-switching between Playlist (arrangement) and individual patterns is confusing.
- No MIDI hardware routing view — connecting a physical keyboard requires nested menus.

### Ableton Live

**Copy these:**
- **Session View (clip launcher)**: grid of clips triggerable independently or in "scenes" (rows). Every clip is a loop; scenes launch in sync; Follow Actions trigger next clip automatically.
- **Dual-view architecture**: Session View (improvisation) + Arrangement View (linear timeline). Record Session performance into Arrangement.
- **Warping**: time-stretch any audio to project BPM in real time. Any file becomes tempo-synced instantly.
- **Live 12 generative MIDI tools**: Recombine (pitch from one seq, rhythm from another), Euclidean generator (N hits across M steps), per-note probability, Strum tool.
- **MPE support**: 5 per-note dimensions with curve editors per note.

**Avoid these:**
- No ghost notes — #1 complaint from FL Studio users switching, unresolved since 2013.
- Scale lock is clip-level only (not project-wide).
- Keyboard shortcuts hardcoded, non-remappable.
- Suite edition at $749 extremely expensive.

### Adobe Audition

**Copy these:**
- **Spectral frequency display**: visualize audio as frequency-over-time heatmap, paint-erase noise with a brush. Include for Tone's audio cleanup features.

**Avoid these:**
- No MIDI support — not a music creation tool.
- Subscription-only, no one-time purchase.

---

## 3. AI Model Recommendations

### Music Generation (Full-Song / Long-Form Audio)

**Primary: ACE-Step 1.5**
- Most capable open-weight text-to-music model as of 2026. Apache 2.0 license.
- `fspecii/ace-step-ui` on GitHub provides a production UI.
- Runs on GPU (RTX 3080+ or Apple M-series); 24GB+ VRAM for full quality.
- ONNX export path being explored by community. Tone's Rust path: ONNX → `ort` crate.

**Secondary: Stable Audio 3.0 Small** (Stability AI)
- Apache 2.0 / Stability AI Community License. **Trained on fully licensed data** — cleanest legal position of any open music generation model. Directly addresses the Suno/Udio lawsuit risk.
- `stabilityai/stable-audio-open-1.0` on Hugging Face.
- Audio latent diffusion model; supports text conditioning and audio reference conditioning.
- ONNX export feasible via standard diffusion pipeline tooling.

**Also evaluate: MusicGen Small** (Meta AudioCraft)
- ONNX export confirmed: produces `text_encoder.onnx`, `encodec_decode.onnx`, `decoder_model.onnx`.
- 300M parameter small variant is feasible on consumer GPU. AudioCraft GitHub issue #297 documents the ONNX export path.
- Outputs audio waveforms (not MIDI) — use for conditioned audio clip generation.

### Stem Separation

**Recommendation: HT-Demucs FT via ONNX**
- Best open-source stem separator on MUSDB18-HQ benchmark. MIT license.
- ONNX export fully solved as of 2025 (Google Summer of Code 2025 project + StemSplit's `demucs-onnx`).
- Pre-built ONNX models on Hugging Face: `StemSplitio/htdemucs-ft-other-onnx`
- Runs via `onnxruntime` on CPU/CoreML/CUDA/DirectML — no PyTorch at inference.
- 1.31x faster than PyTorch on CPU; numerically equivalent output (<0.1 dB difference).
- **Rust path**: download pre-built ONNX → load via `ort` crate → run inference directly. Clearest model-to-Rust path of any audio AI model.
- Separates: vocals, drums, bass, other (4-stem); htdemucs_ft variant for higher quality.

### MIDI Generation

**Primary: Magenta suite** (Google, open source, Apache 2.0)
- `MusicVAE`: interpolation between two MIDI sequences, MIDI-space style transfer.
- `Melody RNN`: melody continuation in a given style.
- `GrooVAE`: (a) humanize a rigid drum grid with micro-timing + velocity variation; (b) generate expressive drums to match a melody.
- `DrumsRNN`: generate drum patterns in a given style; trained on real drummer data.
- `PianoGenie`: real-time piano generation from 8-button input.
- **Rust path**: export TF SavedModel → ONNX via `tf2onnx`, load via `ort`. Documented.

### Chord Suggestion

**Hybrid approach:**
- Magenta chord RNN (ONNX) for contextual chord following.
- Traditional music theory rules for hard constraints (diatonic chord sets, voice leading, avoid parallel fifths).
- `chord2vec` embeddings (small, fast) for nearest-neighbor chord suggestion.
- **For key/chord detection from audio**: Essentia (MIT license, C++ with Rust FFI bindings). Has production-ready chord and key detection.

### AI Mastering

**Recommendation: Matchering 2.0** (Apache 2.0)
- Reference-based: provide TARGET track + REFERENCE track → Matchering matches RMS, frequency response, peak amplitude, stereo width.
- DSP-based (not neural) — portable to Rust or wrapped via PyO3. No ONNX required.
- Available as Python library and ComfyUI node.
- **Tone UX**: "Master to Reference" as top-level export step. User drags in any commercial reference track, Tone matches master characteristics.

### Beat / Drum Generation

**Primary: GrooVAE** (Magenta)
- Two modes: humanize rigid drum grid; generate groove to match melody.
- Trained on Groove MIDI Dataset (real drummer recordings) — outputs feel genuinely human.
- TF/TFLite, exportable to ONNX via `tf2onnx`.

**Secondary: MaskBeat** (2025, arxiv 2507.03395)
- Specifically designed for loopable drum beat generation. Generates 4/8-bar loops that repeat without seams.
- Not yet production-packaged; paper is public.

### Audio to MIDI

**Recommendation: BasicPitch** (Spotify, Apache 2.0)
- Converts audio (including vocals) to MIDI with pitch, onset, and note duration detection.
- CoreML model available for Apple Silicon. ONNX export done by community.
- `spotify/basic-pitch` on GitHub.

### Rust Inference Runtime

**Recommendation: `ort` crate (pykeio/ort)**
- Rust interface for ONNX Runtime (Microsoft). Wraps production-grade ORT C++ library.
- Supports CUDA, TensorRT, CoreML, DirectML, OpenVINO execution providers.
- Actively maintained; used in production for audio ML.
- Strategy: export every model to ONNX → load via `ort` → handle preprocessing/postprocessing in Rust.

---

## 4. Key Differentiators for Tone

1. **Local-first, private, no cloud required.** Suno/Udio are 100% cloud-locked. No local option at any price. Tone runs everything locally. User data never leaves the machine. Positioning: "Your music. Your machine. Your rights."

2. **AI that outputs MIDI, not just audio blobs.** Suno/Udio output stereo audio. Tone's AI generates into the piano roll as editable MIDI. Every AI suggestion lands in the arrangement as standard MIDI, immediately editable and exportable.

3. **Ghost notes — the feature every Ableton and Logic user has been waiting for.** Implemented on day one. Notes from all other tracks visible as translucent overlays in the active editor. Per-track toggle.

4. **Chord Track as the harmonic source of truth.** Logic's architecture: define chord progression once, everything follows — AI generation, Session Players, transposition, chord-aware quantize. Tone extends this: the Chord Track constrains AI generation. Specify "Am-F-C-G" → every AI output (melody, bass, drums, pads) generated within that context.

5. **Bar-level AI inpainting in the arrangement timeline.** Udio offers 2-second region regeneration. Tone offers bar-level and phrase-level regeneration: select bars 9–16, press Regenerate, keep chord track and tempo context, replace only the selected region.

6. **Dual view — Session (clip launcher) + Arrangement (timeline), AI-native.** Ableton's dual-view is the best compositional architecture but has no AI features. Logic has AI but no clip launcher. Tone combines both.

7. **First-class stem export, every tier, always.** Suno/Udio treat stems as premium or broken. Tone exports 4-stem or 6-stem 24-bit/48kHz WAV from any project, at any tier. Standard: Vocals, Drums, Bass, Other. All stems same length, starting at sample 0.

8. **Reference mastering built in.** No DAW ships with this. Matchering 2.0 is open source. Tone exposes "Master to Reference" as a top-level export step: drag reference track → Tone matches RMS, frequency response, peak amplitude, stereo width.

9. **No subscription, no credits, one purchase forever.** FL Studio's lifetime purchase model is the strongest customer loyalty driver in desktop software. Tone matches it. The credit model is the most complained-about aspect of Suno/Udio.

10. **Cross-platform: Windows, Mac, Linux from day one.** Logic is Mac-only. GarageBand is Mac/iOS only. Tone in Rust/GPUI targets all three platforms.

11. **Open source with auditable AI.** No AI music creation tool is open source. Tone being open source under the prism-suite umbrella creates a trust signal neither Suno nor Udio can match.

---

## 5. Format and Protocol Strategy

### MIDI
- Standard MIDI File Type 1 (multi-track), 480 PPQN resolution.
- Embed tempo, time signature, key signature in MIDI header.
- Support MIDI 2.0 UMP (Universal MIDI Packet) for high-resolution velocity, pitch, timing — future-proof.
- Platform MIDI APIs: CoreMIDI (Mac), WinMM/UMP (Windows), ALSA (Linux).

### Audio (Internal)
- Working format: 32-bit float PCM, 48kHz.
- Never downconvert internally; only encode on export.

### Stems Export (Professional Standard)
- All stems: 24-bit WAV, 48kHz, same duration, starting at sample 0.
- Standard 4-stem: Vocals, Drums, Bass, Other.
- Extended 6-stem: add Guitar, Keys.
- Extended 12-stem (matching Suno Premier): per-instrument sub-stems.
- Filename convention: `ProjectName_Vocals.wav`, `ProjectName_Drums.wav` etc.

### Consumer Export
- MP3 (320kbps), AAC (256kbps), OGG Vorbis (open/patent-free, for Linux credibility).
- FLAC for lossless archival distribution.
- WAV only for production interchange.

### Project Format
- `.tone` format: ZIP container with JSON manifest + embedded WAV/MIDI assets.
- JSON manifest includes: tempo map, key, time signature, chord track, plugin state, AI generation history (model, prompt, parameters, chord track context active at generation time).
- AI generation history enables: re-generation from same parameters, text export of generation parameters, full provenance audit.
- Document format publicly; make it open for DAW interoperability.

---

## 6. 10 UI/UX Principles for Tone

1. **AI is a sidekick in the margin, not the product.** Surface AI as "Generate" buttons inline in piano roll, chord track, clip launcher — never as a separate app mode. Model: GitHub Copilot's inline ghost-text pattern, adapted for music. Pressing Escape or continuing to edit dismisses AI suggestions silently.

2. **The chord track is always visible and always in charge.** Pinned to top of arrangement view, always visible, never scrolled away. Every musical action — AI generation, transposition, Session Player performance, scale lock — reads from the chord track. Chord ID is a one-click button.

3. **Zero latency setup for beginners — hide the plumbing.** Auto-detect connected audio interface. Set buffer size to 512 samples by default. Show plain-English latency indicator: "Monitoring delay: 12ms — Good for recording." Never surface "ASIO," "sample rate," "bit depth," "buffer size" to a user who hasn't opened audio settings. GarageBand does this; no other DAW does it as well.

4. **Step sequencer and piano roll are the same data, two views.** FL Studio's model is correct. Beginners tap beats in step grid; advanced users edit in piano roll. Same underlying MIDI data, toggle with a single keyboard shortcut. No copy-paste between views.

5. **Dual view — Session (clip launcher) and Arrangement (timeline).** Ableton's session/arrangement dual-view is the correct compositional architecture. Session View for improvising; Arrangement View for the final linear timeline. Record from Session to Arrangement with one button.

6. **Ghost notes everywhere, project-wide.** Implement FL Studio-style ghost notes across all tracks in piano roll. Notes from all other tracks visible as translucent overlays in the active editor. Per-track toggle controls which tracks show ghost notes.

7. **Non-destructive everything, with AI generation history as a first-class sidebar.** Every edit is non-destructive. Every AI generation creates a new history entry, never overwrites. Generation history panel shows: timestamp, model used, prompt, parameters, waveform/MIDI preview thumbnail. Click any entry to preview or restore.

8. **Stem export is a top-level action, not buried in menus.** "Export Stems" in main toolbar and top-level File menu. Export dialog shows per-stem preview with play button. One-click "Export All Stems" with sane defaults (WAV 24-bit/48kHz).

9. **Reference-quality metering on every channel strip.** Real-time RMS and peak metering on every channel. Master bus shows LUFS integrated loudness (the streaming standard) in real time. Never require a separate plugin for this. Green/yellow/red LUFS readout on master = "is this track ready to upload."

10. **AI provenance badge on every generated clip.** Every AI-generated clip carries a subtle "AI" badge. Hovering shows: which model, what prompt/parameters, when generated, what chord track context was active. "Re-generate with same parameters" button inline. Users can export generation parameters as text. Transparent, trustworthy alternative to the ownership-language ambiguity that damaged Suno.

---

## 7. Lessons from Existing Prism Apps

- **Follow the Reel pattern**: GPUI host + `App` / `Action` / `apply` works well; avoid egui split that Contour and Pulse inherited.
- **Serde-free state**: the `Action` enum doubles as a serializable undo log (add serde later when/if needed).
- **prism-media for I/O**: already handles decode/encode via FFmpeg; Tone reuses it rather than adding another audio I/O crate.
- **Don't promote to shared prematurely**: `prism-ai` becomes a shared crate only when a second app (e.g. Pigment's neural upscale) actually needs it.
- **`ort` crate throughout**: all ONNX inference via pykeio/ort — one inference runtime across all AI features.
