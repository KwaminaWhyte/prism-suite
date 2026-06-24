//! Real ONNX inference scaffolding for Tone.
//!
//! This module turns Tone's previously pure-state AI stubs into a real
//! inference *layer* with a pluggable backend, **without** forcing the native
//! ONNX Runtime into the default build.
//!
//! ## Design
//!
//! * [`InferenceBackend`] — the backend selector. The default is
//!   [`InferenceBackend::Stub`], a deterministic, dependency-free backend that
//!   reproduces the exact behaviour Tone's stub modules already rely on. The
//!   [`InferenceBackend::Ort`] variant is only constructable behind the
//!   `onnx` cargo feature and loads a real `.onnx` session via the `ort` crate.
//!
//! * [`ModelRegistry`] — maps a [`ModelId`] to an optional local `.onnx` path
//!   plus a [`LoadState`]. It is the source of truth for "is this model
//!   available?". When a model has no registered path (or the `onnx` feature
//!   is off), inference falls back to the stub backend.
//!
//! * [`run_inference`] — the single entry point. Given a [`ModelRegistry`], a
//!   chosen [`InferenceBackend`], and an [`InferenceRequest`], it attempts the
//!   real `ort` path when (and only when) the `onnx` feature is on AND the
//!   model is loadable; otherwise it falls back to the deterministic stub and
//!   *never fails the build or the call*.
//!
//! The `App` state in `onnx_runtime.rs` keeps tracking model download/registry
//! actions; this module is the compute layer those actions feed into.

use std::collections::HashMap;

use super::{Action, App};
use super::onnx_runtime::OnnxModelKind;

// ─── Model identity ─────────────────────────────────────────────────────────

/// Every AI model family Tone can run inference against. This is the union of
/// the model kinds used across Tone's AI stub domains (MusicGen, Demucs,
/// Magenta melody/continuation/chord-voicing, AI mastering, vocal tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelId {
    /// Prompt-conditioned full-track generation (`musicgen.rs`).
    MusicGen,
    /// Source separation into drums/bass/other/vocals (`demucs.rs`).
    Demucs,
    /// Magenta melody generation from scratch (`magenta.rs`).
    MelodyRnn,
    /// Magenta transformer melody continuation (`magenta.rs`).
    MusicTransformer,
    /// Audio super-resolution / upsampling.
    AudioSr,
    /// Loudness + multiband mastering network (`ai_mastering.rs`).
    AiMasterNet,
    /// Pitch-correction / auto-tune network (`vocal_tools.rs`).
    VocalAutoTune,
    /// Vocal isolation network (`vocal_tools.rs`).
    VocalIsolate,
}

impl ModelId {
    /// Canonical on-disk file name for this model's ONNX graph.
    pub fn filename(&self) -> &'static str {
        match self {
            Self::MusicGen => "musicgen-small-decoder.onnx",
            Self::Demucs => "demucs-hybrid.onnx",
            Self::MelodyRnn => "melody-rnn.onnx",
            Self::MusicTransformer => "music-transformer.onnx",
            Self::AudioSr => "audio-sr.onnx",
            Self::AiMasterNet => "ai-master-net.onnx",
            Self::VocalAutoTune => "vocal-autotune.onnx",
            Self::VocalIsolate => "vocal-isolate.onnx",
        }
    }

    /// Human-readable label for UI / logging.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MusicGen => "MusicGen",
            Self::Demucs => "Demucs",
            Self::MelodyRnn => "Melody RNN",
            Self::MusicTransformer => "Music Transformer",
            Self::AudioSr => "Audio Super-Resolution",
            Self::AiMasterNet => "AI Master Net",
            Self::VocalAutoTune => "Vocal Auto-Tune",
            Self::VocalIsolate => "Vocal Isolate",
        }
    }
}

// ─── Load state ───────────────────────────────────────────────────────────────

/// Tracks whether a model's session is loadable / has been loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadState {
    /// No local path registered yet.
    Unregistered,
    /// A path is registered but the session has not been opened.
    Registered,
    /// The session is loaded and ready (real backend only).
    Loaded,
    /// Loading or inference failed; carries the error message.
    Failed(String),
}

/// One entry in the registry: the file path (if any) + its load state.
#[derive(Clone, Debug)]
pub struct ModelSlot {
    pub path: Option<String>,
    pub state: LoadState,
}

impl Default for ModelSlot {
    fn default() -> Self {
        Self { path: None, state: LoadState::Unregistered }
    }
}

impl ModelSlot {
    /// True when there's a registered, non-empty path on disk to load from.
    pub fn has_path(&self) -> bool {
        self.path.as_deref().is_some_and(|p| !p.is_empty())
    }
}

// ─── Registry ─────────────────────────────────────────────────────────────────

/// Maps each [`ModelId`] to its [`ModelSlot`]. The compute layer consults this
/// to decide whether a real session can be loaded or the stub must be used.
#[derive(Clone, Debug, Default)]
pub struct ModelRegistry {
    slots: HashMap<ModelId, ModelSlot>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the local path for `model`, marking it
    /// [`LoadState::Registered`].
    pub fn register(&mut self, model: ModelId, path: impl Into<String>) {
        let path = path.into();
        let state = if path.is_empty() {
            LoadState::Unregistered
        } else {
            LoadState::Registered
        };
        self.slots.insert(model, ModelSlot { path: Some(path), state });
    }

    /// Remove a model's path and reset it to [`LoadState::Unregistered`].
    pub fn clear(&mut self, model: ModelId) {
        self.slots.insert(model, ModelSlot::default());
    }

    /// Record a load/inference failure for `model`.
    pub fn mark_failed(&mut self, model: ModelId, err: impl Into<String>) {
        let slot = self.slots.entry(model).or_default();
        slot.state = LoadState::Failed(err.into());
    }

    /// Mark a model's session as loaded (real backend success).
    pub fn mark_loaded(&mut self, model: ModelId) {
        let slot = self.slots.entry(model).or_default();
        slot.state = LoadState::Loaded;
    }

    pub fn slot(&self, model: ModelId) -> Option<&ModelSlot> {
        self.slots.get(&model)
    }

    pub fn load_state(&self, model: ModelId) -> LoadState {
        self.slots
            .get(&model)
            .map(|s| s.state.clone())
            .unwrap_or(LoadState::Unregistered)
    }

    /// The registered path for `model`, if any non-empty path exists.
    pub fn path(&self, model: ModelId) -> Option<&str> {
        self.slots
            .get(&model)
            .and_then(|s| s.path.as_deref())
            .filter(|p| !p.is_empty())
    }

    /// True when a real session *could* be loaded for `model` (a path is
    /// registered). This does not check the filesystem — that happens in the
    /// `ort` backend at load time.
    pub fn is_loadable(&self, model: ModelId) -> bool {
        self.slot(model).is_some_and(|s| s.has_path())
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

// ─── Backend selector ───────────────────────────────────────────────────────

/// Which inference implementation to use.
///
/// `Stub` is always available and deterministic. `Ort` is only constructable
/// when the `onnx` feature is enabled; with the feature off it is uninhabited
/// in practice (the constructor [`InferenceBackend::ort`] is feature-gated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferenceBackend {
    /// Deterministic, dependency-free fallback. Reproduces Tone's existing
    /// stub behaviour exactly.
    Stub,
    /// Real ONNX Runtime session via the `ort` crate. Only meaningful with the
    /// `onnx` cargo feature; without it, selecting this still safely falls back
    /// to the stub at run time.
    Ort,
}

impl Default for InferenceBackend {
    fn default() -> Self {
        // Default build ships with the stub — no native runtime required.
        InferenceBackend::Stub
    }
}

impl InferenceBackend {
    /// Construct the real ORT backend. Only available with `--features onnx`;
    /// in the default build the `Ort` variant can still be named but the
    /// inference path will fall back to the stub.
    #[cfg(feature = "onnx")]
    pub fn ort() -> Self {
        InferenceBackend::Ort
    }

    /// Whether this backend can actually run native ONNX in the current build.
    pub fn supports_native(&self) -> bool {
        match self {
            InferenceBackend::Stub => false,
            InferenceBackend::Ort => cfg!(feature = "onnx"),
        }
    }
}

// ─── Request / response ─────────────────────────────────────────────────────

/// A single inference request. The payload describes the AI task; the audio /
/// conditioning buffers are carried alongside.
#[derive(Clone, Debug)]
pub struct InferenceRequest {
    pub model: ModelId,
    /// Conditioning text (prompts for MusicGen / Magenta), empty otherwise.
    pub prompt: String,
    /// Sampling temperature where applicable.
    pub temperature: f32,
    /// Requested output length in bars (generation tasks).
    pub bars: u8,
    /// Input audio, interleaved f32 samples (separation / mastering / vocal).
    pub audio_in: Vec<f32>,
    /// Sample rate of `audio_in`.
    pub sample_rate: u32,
}

impl InferenceRequest {
    pub fn generation(model: ModelId, prompt: impl Into<String>, bars: u8, temperature: f32) -> Self {
        Self {
            model,
            prompt: prompt.into(),
            temperature,
            bars,
            audio_in: Vec::new(),
            sample_rate: 48_000,
        }
    }

    pub fn audio(model: ModelId, audio_in: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            model,
            prompt: String::new(),
            temperature: 1.0,
            bars: 0,
            audio_in,
            sample_rate,
        }
    }
}

/// Result of running inference.
#[derive(Clone, Debug, PartialEq)]
pub struct InferenceOutput {
    /// Generated / processed audio (interleaved f32). For separation, the
    /// stems are concatenated and `stem_lengths` records each stem's length.
    pub audio_out: Vec<f32>,
    /// Per-stem sample counts for separation outputs; empty otherwise.
    pub stem_lengths: Vec<usize>,
    /// True when this came from the real ORT session, false for the stub.
    pub from_native: bool,
}

impl InferenceOutput {
    /// Number of stems represented in `audio_out`.
    pub fn stem_count(&self) -> usize {
        self.stem_lengths.len()
    }
}

/// Errors the inference layer surfaces. Callers may choose to fall back to the
/// stub on any of these — [`run_inference`] already does so automatically.
#[derive(Clone, Debug, PartialEq)]
pub enum InferenceError {
    /// No path registered / model not loadable.
    ModelUnavailable(ModelId),
    /// The `ort` session failed to load or run.
    Backend(String),
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceError::ModelUnavailable(m) => {
                write!(f, "model unavailable: {}", m.display_name())
            }
            InferenceError::Backend(msg) => write!(f, "ort backend error: {msg}"),
        }
    }
}

// ─── Top-level dispatch ─────────────────────────────────────────────────────

/// Run inference, preferring the real backend when possible and falling back
/// to the deterministic stub otherwise. **This never returns an error** — the
/// stub is always a valid fallback so callers always get a usable result.
///
/// Selection logic:
/// 1. If `backend == Ort` AND the `onnx` feature is on AND the model has a
///    registered path → attempt the real session. On success return its
///    output (`from_native = true`). On failure mark the registry slot failed
///    and fall through to the stub.
/// 2. Otherwise → run the stub (`from_native = false`).
pub fn run_inference(
    registry: &mut ModelRegistry,
    backend: InferenceBackend,
    req: &InferenceRequest,
) -> InferenceOutput {
    if backend == InferenceBackend::Ort && registry.is_loadable(req.model) {
        match run_native(registry, req) {
            Ok(out) => {
                registry.mark_loaded(req.model);
                return out;
            }
            Err(e) => {
                registry.mark_failed(req.model, e.to_string());
                // fall through to stub
            }
        }
    }
    stub_inference(req)
}

/// Attempt to run the request through the real ORT backend.
///
/// With the `onnx` feature OFF this is a no-op that reports the model as
/// unavailable, so [`run_inference`] cleanly falls back to the stub. The build
/// never pulls `ort` unless the feature is enabled.
#[cfg(not(feature = "onnx"))]
fn run_native(
    _registry: &mut ModelRegistry,
    req: &InferenceRequest,
) -> Result<InferenceOutput, InferenceError> {
    Err(InferenceError::ModelUnavailable(req.model))
}

/// Real ORT path. Loads the registered `.onnx` session, builds the input
/// tensor(s) from the request, runs the graph, and maps outputs back to audio.
#[cfg(feature = "onnx")]
fn run_native(
    registry: &mut ModelRegistry,
    req: &InferenceRequest,
) -> Result<InferenceOutput, InferenceError> {
    use ort::session::Session;
    use ort::value::Value;

    let path = registry
        .path(req.model)
        .ok_or(InferenceError::ModelUnavailable(req.model))?
        .to_string();

    // Build (or rebuild) the session from the registered path.
    let mut session = Session::builder()
        .and_then(|b| b.commit_from_file(&path))
        .map_err(|e| InferenceError::Backend(format!("load {path}: {e}")))?;

    // Build the primary input tensor. Generation tasks have no audio input, so
    // we synthesise a conditioning vector from prompt/temperature; audio tasks
    // feed the raw buffer. Real model graphs vary in their exact input names
    // and shapes, so we read the first input's name from the loaded graph.
    let input_name = session
        .inputs
        .first()
        .map(|i| i.name.clone())
        .ok_or_else(|| InferenceError::Backend("model has no inputs".into()))?;

    let (data, shape): (Vec<f32>, [usize; 2]) = if req.audio_in.is_empty() {
        let cond = conditioning_vector(req);
        let len = cond.len();
        (cond, [1, len])
    } else {
        let len = req.audio_in.len();
        (req.audio_in.clone(), [1, len])
    };

    let tensor = Value::from_array((shape, data))
        .map_err(|e| InferenceError::Backend(format!("build input tensor: {e}")))?;

    let outputs = session
        .run(ort::inputs![input_name.as_str() => tensor])
        .map_err(|e| InferenceError::Backend(format!("run: {e}")))?;

    // Map the first output back to an audio buffer.
    let first = outputs
        .iter()
        .next()
        .ok_or_else(|| InferenceError::Backend("model produced no outputs".into()))?;

    let (_shape, slice) = first
        .1
        .try_extract_tensor::<f32>()
        .map_err(|e| InferenceError::Backend(format!("extract output: {e}")))?;

    let audio_out = slice.to_vec();
    // For separation models we'd parse the output shape into per-stem lengths;
    // a single-output graph yields one buffer.
    Ok(InferenceOutput {
        audio_out,
        stem_lengths: Vec::new(),
        from_native: true,
    })
}

/// Build a deterministic conditioning vector from a generation request — used
/// only by the real backend to feed prompt-conditioned graphs a stable input.
#[cfg(feature = "onnx")]
fn conditioning_vector(req: &InferenceRequest) -> Vec<f32> {
    let mut v = Vec::with_capacity(64);
    let seed = prompt_seed(&req.prompt);
    for i in 0..64u32 {
        let x = ((seed.wrapping_add(i).wrapping_mul(2654435761)) >> 8) as f32;
        v.push(((x % 1000.0) / 1000.0) * req.temperature.max(0.01));
    }
    v
}

// ─── Deterministic stub backend ─────────────────────────────────────────────

/// Hash a prompt into a stable seed so stub output is reproducible per-prompt.
fn prompt_seed(prompt: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in prompt.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Deterministic, dependency-free inference. Produces a stable pseudo-audio
/// buffer keyed off the request so tests and the default build behave
/// identically regardless of whether `ort` is present.
pub fn stub_inference(req: &InferenceRequest) -> InferenceOutput {
    match req.model {
        ModelId::Demucs | ModelId::VocalIsolate => {
            // Source separation: split the input into 4 (or 2) equal stems.
            let stems = if req.model == ModelId::Demucs { 4 } else { 2 };
            let total = req.audio_in.len().max(stems);
            let per = total / stems;
            let mut audio_out = Vec::with_capacity(per * stems);
            let mut stem_lengths = Vec::with_capacity(stems);
            for s in 0..stems {
                for i in 0..per {
                    let src = req.audio_in.get(i).copied().unwrap_or(0.0);
                    // Deterministic per-stem attenuation so stems differ.
                    audio_out.push(src * (1.0 - s as f32 * 0.15));
                }
                stem_lengths.push(per);
            }
            InferenceOutput { audio_out, stem_lengths, from_native: false }
        }
        ModelId::MusicGen | ModelId::MelodyRnn | ModelId::MusicTransformer => {
            // Generation: synthesise a deterministic tone buffer whose content
            // is keyed off the prompt + bars so output is reproducible.
            let seed = prompt_seed(&req.prompt);
            let samples = (req.bars.max(1) as usize) * 1024;
            let freq = 110.0 + (seed % 880) as f32;
            let mut audio_out = Vec::with_capacity(samples);
            for i in 0..samples {
                let t = i as f32 / req.sample_rate.max(1) as f32;
                let v = (t * freq * std::f32::consts::TAU).sin() * req.temperature.clamp(0.1, 2.0) * 0.25;
                audio_out.push(v);
            }
            InferenceOutput { audio_out, stem_lengths: Vec::new(), from_native: false }
        }
        ModelId::AiMasterNet => {
            // Mastering: deterministic soft-clip + makeup gain pass.
            let audio_out: Vec<f32> = req
                .audio_in
                .iter()
                .map(|&x| (x * 1.2).clamp(-0.98, 0.98))
                .collect();
            InferenceOutput { audio_out, stem_lengths: Vec::new(), from_native: false }
        }
        ModelId::AudioSr => {
            // Super-resolution: deterministic 2x linear upsample.
            let mut audio_out = Vec::with_capacity(req.audio_in.len() * 2);
            for w in req.audio_in.windows(2) {
                audio_out.push(w[0]);
                audio_out.push((w[0] + w[1]) * 0.5);
            }
            if let Some(&last) = req.audio_in.last() {
                audio_out.push(last);
                audio_out.push(last);
            }
            InferenceOutput { audio_out, stem_lengths: Vec::new(), from_native: false }
        }
        ModelId::VocalAutoTune => {
            // Auto-tune: deterministic pass-through (pitch handled in MIDI land).
            InferenceOutput {
                audio_out: req.audio_in.clone(),
                stem_lengths: Vec::new(),
                from_native: false,
            }
        }
    }
}

// ─── OnnxModelKind bridge ─────────────────────────────────────────────────────

impl ModelId {
    /// Map the download-registry's [`OnnxModelKind`] onto a compute [`ModelId`].
    /// `OnnxModelKind` has no separate vocal entries, so those map to `None`.
    pub fn from_onnx_kind(kind: &OnnxModelKind) -> Option<ModelId> {
        Some(match kind {
            OnnxModelKind::MusicGen => ModelId::MusicGen,
            OnnxModelKind::Demucs => ModelId::Demucs,
            OnnxModelKind::MelodyRnn => ModelId::MelodyRnn,
            OnnxModelKind::MusicTransformer => ModelId::MusicTransformer,
            OnnxModelKind::AudioSr => ModelId::AudioSr,
            OnnxModelKind::AiMasterNet => ModelId::AiMasterNet,
        })
    }
}

// ─── App apply helpers ──────────────────────────────────────────────────────

impl App {
    /// Handle the inference-layer actions (path registration + backend select).
    pub(super) fn apply_inference(&mut self, action: &Action) {
        match action {
            Action::RegisterModelPath { model, path } => {
                self.model_registry.register(*model, path.clone());
            }
            Action::ClearModelPath { model } => {
                self.model_registry.clear(*model);
            }
            Action::SetInferenceBackend { backend } => {
                self.inference_backend = *backend;
            }
            _ => {}
        }
    }

    /// Bridge a completed model download into the real inference registry, so a
    /// freshly-downloaded `.onnx` becomes immediately loadable. Called from the
    /// `onnx_runtime` `CompleteModelDownload` handler.
    pub(crate) fn sync_registry_on_download(&mut self, kind: &OnnxModelKind, local_path: &str) {
        if let Some(model) = ModelId::from_onnx_kind(kind) {
            self.model_registry.register(model, local_path.to_string());
        }
    }

    /// Bridge a removed model out of the inference registry.
    pub(crate) fn sync_registry_on_remove(&mut self, kind: &OnnxModelKind) {
        if let Some(model) = ModelId::from_onnx_kind(kind) {
            self.model_registry.clear(model);
        }
    }

    /// Run inference for `req` through the currently-selected backend, with the
    /// app's [`ModelRegistry`]. Always returns a usable result (stub fallback).
    pub fn run_model_inference(&mut self, req: &InferenceRequest) -> InferenceOutput {
        run_inference(&mut self.model_registry, self.inference_backend, req)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::Action;

    #[test]
    fn registry_starts_empty() {
        let reg = ModelRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.load_state(ModelId::MusicGen), LoadState::Unregistered);
    }

    #[test]
    fn register_sets_path_and_state() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::MusicGen, "/models/musicgen.onnx");
        assert_eq!(reg.path(ModelId::MusicGen), Some("/models/musicgen.onnx"));
        assert_eq!(reg.load_state(ModelId::MusicGen), LoadState::Registered);
        assert!(reg.is_loadable(ModelId::MusicGen));
    }

    #[test]
    fn register_empty_path_is_unregistered() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::Demucs, "");
        assert_eq!(reg.load_state(ModelId::Demucs), LoadState::Unregistered);
        assert!(!reg.is_loadable(ModelId::Demucs));
    }

    #[test]
    fn clear_resets_slot() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::AiMasterNet, "/m/aim.onnx");
        assert!(reg.is_loadable(ModelId::AiMasterNet));
        reg.clear(ModelId::AiMasterNet);
        assert!(!reg.is_loadable(ModelId::AiMasterNet));
        assert_eq!(reg.load_state(ModelId::AiMasterNet), LoadState::Unregistered);
        assert_eq!(reg.path(ModelId::AiMasterNet), None);
    }

    #[test]
    fn mark_failed_and_loaded() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::Demucs, "/m/demucs.onnx");
        reg.mark_failed(ModelId::Demucs, "oom");
        assert_eq!(reg.load_state(ModelId::Demucs), LoadState::Failed("oom".into()));
        reg.mark_loaded(ModelId::Demucs);
        assert_eq!(reg.load_state(ModelId::Demucs), LoadState::Loaded);
    }

    #[test]
    fn default_backend_is_stub() {
        assert_eq!(InferenceBackend::default(), InferenceBackend::Stub);
        assert!(!InferenceBackend::default().supports_native());
    }

    #[test]
    fn stub_backend_never_supports_native() {
        assert!(!InferenceBackend::Stub.supports_native());
    }

    #[test]
    fn ort_native_support_tracks_feature_flag() {
        // With the default build (`onnx` off) the Ort backend cannot run native.
        assert_eq!(InferenceBackend::Ort.supports_native(), cfg!(feature = "onnx"));
    }

    #[test]
    fn fallback_to_stub_when_no_model_registered() {
        let mut reg = ModelRegistry::new();
        let req = InferenceRequest::generation(ModelId::MusicGen, "lofi beat", 4, 1.0);
        // Even asking for Ort, with no registered model we get the stub.
        let out = run_inference(&mut reg, InferenceBackend::Ort, &req);
        assert!(!out.from_native);
        assert!(!out.audio_out.is_empty());
    }

    #[test]
    fn fallback_to_stub_when_backend_is_stub() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::MusicGen, "/models/musicgen.onnx");
        let req = InferenceRequest::generation(ModelId::MusicGen, "jazz", 8, 0.9);
        let out = run_inference(&mut reg, InferenceBackend::Stub, &req);
        assert!(!out.from_native);
    }

    #[cfg(not(feature = "onnx"))]
    #[test]
    fn ort_without_feature_falls_back_to_stub_even_with_path() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::MusicGen, "/models/musicgen.onnx");
        let req = InferenceRequest::generation(ModelId::MusicGen, "techno", 4, 1.1);
        let out = run_inference(&mut reg, InferenceBackend::Ort, &req);
        // Feature off → run_native reports unavailable → stub fallback.
        assert!(!out.from_native);
    }

    #[test]
    fn stub_generation_is_deterministic_per_prompt() {
        let req = InferenceRequest::generation(ModelId::MusicGen, "ambient pad", 2, 1.0);
        let a = stub_inference(&req);
        let b = stub_inference(&req);
        assert_eq!(a, b);
        assert!(!a.audio_out.is_empty());
    }

    #[test]
    fn stub_generation_differs_by_prompt() {
        let a = stub_inference(&InferenceRequest::generation(ModelId::MusicGen, "rock", 2, 1.0));
        let b = stub_inference(&InferenceRequest::generation(ModelId::MusicGen, "blues", 2, 1.0));
        assert_ne!(a.audio_out, b.audio_out);
    }

    #[test]
    fn stub_demucs_splits_four_stems() {
        let audio = vec![0.5f32; 400];
        let req = InferenceRequest::audio(ModelId::Demucs, audio, 48_000);
        let out = stub_inference(&req);
        assert_eq!(out.stem_count(), 4);
        assert_eq!(out.stem_lengths, vec![100, 100, 100, 100]);
        assert_eq!(out.audio_out.len(), 400);
    }

    #[test]
    fn stub_vocal_isolate_splits_two_stems() {
        let audio = vec![0.3f32; 200];
        let req = InferenceRequest::audio(ModelId::VocalIsolate, audio, 48_000);
        let out = stub_inference(&req);
        assert_eq!(out.stem_count(), 2);
    }

    #[test]
    fn stub_mastering_soft_clips() {
        let audio = vec![1.0f32, -1.0, 0.5];
        let req = InferenceRequest::audio(ModelId::AiMasterNet, audio, 48_000);
        let out = stub_inference(&req);
        assert!(out.audio_out.iter().all(|&x| x.abs() <= 0.98));
    }

    #[test]
    fn stub_audio_sr_doubles_length() {
        let audio = vec![0.0f32, 0.5, 1.0];
        let req = InferenceRequest::audio(ModelId::AudioSr, audio, 48_000);
        let out = stub_inference(&req);
        assert_eq!(out.audio_out.len(), 6);
    }

    #[test]
    fn stub_autotune_passes_through() {
        let audio = vec![0.1f32, 0.2, 0.3];
        let req = InferenceRequest::audio(ModelId::VocalAutoTune, audio.clone(), 44_100);
        let out = stub_inference(&req);
        assert_eq!(out.audio_out, audio);
    }

    #[test]
    fn model_id_filenames_unique() {
        let ids = [
            ModelId::MusicGen,
            ModelId::Demucs,
            ModelId::MelodyRnn,
            ModelId::MusicTransformer,
            ModelId::AudioSr,
            ModelId::AiMasterNet,
            ModelId::VocalAutoTune,
            ModelId::VocalIsolate,
        ];
        let mut names: Vec<&str> = ids.iter().map(|m| m.filename()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "filenames must be unique");
    }

    #[test]
    fn run_inference_never_panics_for_all_models() {
        let mut reg = ModelRegistry::new();
        for &m in &[
            ModelId::MusicGen,
            ModelId::Demucs,
            ModelId::MelodyRnn,
            ModelId::MusicTransformer,
            ModelId::AudioSr,
            ModelId::AiMasterNet,
            ModelId::VocalAutoTune,
            ModelId::VocalIsolate,
        ] {
            let req = InferenceRequest::audio(m, vec![0.2f32; 64], 48_000);
            let out = run_inference(&mut reg, InferenceBackend::Stub, &req);
            assert!(!out.from_native);
        }
    }

    #[test]
    fn inference_error_display() {
        let e = InferenceError::ModelUnavailable(ModelId::Demucs);
        assert!(e.to_string().contains("Demucs"));
        let e2 = InferenceError::Backend("boom".into());
        assert!(e2.to_string().contains("boom"));
    }

    #[test]
    fn registry_len_tracks_entries() {
        let mut reg = ModelRegistry::new();
        reg.register(ModelId::MusicGen, "/a.onnx");
        reg.register(ModelId::Demucs, "/b.onnx");
        assert_eq!(reg.len(), 2);
    }

    // ── App-level wiring ───────────────────────────────────────────────────

    #[test]
    fn app_defaults_to_stub_backend() {
        let app = App::new();
        assert_eq!(app.inference_backend, InferenceBackend::Stub);
        assert!(app.model_registry.is_empty());
    }

    #[test]
    fn register_model_path_action() {
        let mut app = App::new();
        app.apply(Action::RegisterModelPath {
            model: ModelId::MusicGen,
            path: "/models/musicgen.onnx".into(),
        });
        assert_eq!(app.model_registry.path(ModelId::MusicGen), Some("/models/musicgen.onnx"));
        assert!(app.model_registry.is_loadable(ModelId::MusicGen));
    }

    #[test]
    fn clear_model_path_action() {
        let mut app = App::new();
        app.apply(Action::RegisterModelPath { model: ModelId::Demucs, path: "/m/d.onnx".into() });
        app.apply(Action::ClearModelPath { model: ModelId::Demucs });
        assert!(!app.model_registry.is_loadable(ModelId::Demucs));
    }

    #[test]
    fn set_inference_backend_action() {
        let mut app = App::new();
        app.apply(Action::SetInferenceBackend { backend: InferenceBackend::Ort });
        assert_eq!(app.inference_backend, InferenceBackend::Ort);
        app.apply(Action::SetInferenceBackend { backend: InferenceBackend::Stub });
        assert_eq!(app.inference_backend, InferenceBackend::Stub);
    }

    #[test]
    fn download_complete_bridges_into_registry() {
        let mut app = App::new();
        app.apply(Action::CompleteModelDownload {
            kind: OnnxModelKind::MusicGen,
            local_path: "/models/musicgen.onnx".into(),
        });
        // The download path is now loadable through the real inference registry.
        assert!(app.model_registry.is_loadable(ModelId::MusicGen));
        assert_eq!(app.model_registry.load_state(ModelId::MusicGen), LoadState::Registered);
    }

    #[test]
    fn remove_model_clears_registry() {
        let mut app = App::new();
        app.apply(Action::CompleteModelDownload {
            kind: OnnxModelKind::Demucs,
            local_path: "/models/demucs.onnx".into(),
        });
        assert!(app.model_registry.is_loadable(ModelId::Demucs));
        app.apply(Action::RemoveOnnxModel { kind: OnnxModelKind::Demucs });
        assert!(!app.model_registry.is_loadable(ModelId::Demucs));
    }

    #[test]
    fn app_run_model_inference_uses_stub_by_default() {
        let mut app = App::new();
        let req = InferenceRequest::generation(ModelId::MusicGen, "lofi", 4, 1.0);
        let out = app.run_model_inference(&req);
        assert!(!out.from_native);
        assert!(!out.audio_out.is_empty());
    }

    #[test]
    fn app_run_inference_falls_back_when_ort_selected_without_model() {
        let mut app = App::new();
        app.apply(Action::SetInferenceBackend { backend: InferenceBackend::Ort });
        let req = InferenceRequest::audio(ModelId::Demucs, vec![0.4f32; 100], 48_000);
        let out = app.run_model_inference(&req);
        // No registered model → stub fallback even though Ort is selected.
        assert!(!out.from_native);
    }

    #[test]
    fn from_onnx_kind_maps_all_variants() {
        for k in [
            OnnxModelKind::MusicGen,
            OnnxModelKind::Demucs,
            OnnxModelKind::MelodyRnn,
            OnnxModelKind::MusicTransformer,
            OnnxModelKind::AudioSr,
            OnnxModelKind::AiMasterNet,
        ] {
            assert!(ModelId::from_onnx_kind(&k).is_some());
        }
    }
}
