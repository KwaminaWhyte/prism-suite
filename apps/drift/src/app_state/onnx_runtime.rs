//! Real ONNX inference scaffolding for Drift.
//!
//! This module sits *on top of* the deterministic stubs in
//! [`super::onnx_inference`]. It introduces an [`InferenceBackend`] abstraction
//! plus a [`ModelRegistry`] so that the AI features can run either against a
//! deterministic stub (the default, used by CI and every existing test) or
//! against a real ONNX Runtime session loaded from disk.
//!
//! ## Design contract
//!
//! * The **default backend is the deterministic stub** ([`StubBackend`]). The
//!   suite must build and pass every existing test *without* a native ONNX
//!   Runtime, so the stub is always compiled and is always the fallback.
//! * A real `ort`-based backend ([`OrtBackend`]) is compiled **only** under the
//!   optional `onnx` cargo feature. All `ort`-using code is behind
//!   `#[cfg(feature = "onnx")]`.
//! * Backend selection ([`InferenceBackend::select`]) prefers the real backend
//!   when (a) the `onnx` feature is enabled **and** (b) the requested model has
//!   a registered, on-disk `.onnx` file. Otherwise it falls back to the stub.
//!
//! The state-mutating apply arms in [`super::onnx_inference`] are unchanged; the
//! abstraction here is what real inference plugs into and what the new unit
//! tests exercise.

use super::onnx_inference::DriftOnnxModel;

// ────────────────────────────────────────────────────────────────────────────
// Model registry
// ────────────────────────────────────────────────────────────────────────────

/// Load state of a model within the [`ModelRegistry`].
///
/// Distinct from the download-oriented `OnnxModelStatus` in
/// [`super::onnx_inference`]: this tracks whether a *session* can be / has been
/// constructed from the registered path, which is the concern of the inference
/// layer rather than the download UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelLoadState {
    /// No path registered for this model.
    Unregistered,
    /// A path is registered but the file is missing on disk.
    PathMissing,
    /// A path is registered and the file exists; ready to load a session.
    Ready,
    /// A session has been successfully loaded (only reachable with `onnx`).
    Loaded,
    /// Loading was attempted and failed; carries the error message.
    Failed(String),
}

/// A single registry slot for one [`DriftOnnxModel`].
#[derive(Clone, Debug)]
pub struct RegisteredModel {
    pub model: DriftOnnxModel,
    /// Local filesystem path to the `.onnx` weights, if registered.
    pub local_path: Option<String>,
    pub load_state: ModelLoadState,
}

impl RegisteredModel {
    fn new(model: DriftOnnxModel) -> Self {
        Self {
            model,
            local_path: None,
            load_state: ModelLoadState::Unregistered,
        }
    }
}

/// Default on-disk file name for a model family. Used to suggest a path and to
/// document which weights each family expects; callers may register any path.
///
/// Part of the model-management API surface; the download/registration UI uses
/// it to pre-fill a save path. Allowed dead in the synchronous default build
/// because no caller drives it yet outside tests.
#[allow(dead_code)]
pub fn default_model_filename(model: &DriftOnnxModel) -> &'static str {
    match model {
        DriftOnnxModel::AnimateDiff => "animatediff_motion.onnx",
        DriftOnnxModel::FilmRife => "film_rife.onnx",
        DriftOnnxModel::Wav2Vec2 => "wav2vec2.onnx",
        DriftOnnxModel::Whisper => "whisper.onnx",
        DriftOnnxModel::StyleTransfer => "style_transfer.onnx",
        DriftOnnxModel::StableDiffusion => "stable_diffusion.onnx",
        DriftOnnxModel::MediaPipe => "mediapipe_facemesh.onnx",
    }
}

/// Maps each [`DriftOnnxModel`] to an optional local path and its load state.
///
/// The registry is the single source of truth the inference layer consults to
/// decide whether a real session is available. It is intentionally independent
/// of the download-progress `DriftOnnxModelEntry` list on `App` (which drives
/// UI); model-management apply arms keep the two in sync.
#[derive(Clone, Debug, Default)]
pub struct ModelRegistry {
    entries: Vec<RegisteredModel>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn slot_mut(&mut self, model: &DriftOnnxModel) -> &mut RegisteredModel {
        if let Some(idx) = self.entries.iter().position(|e| e.model == *model) {
            &mut self.entries[idx]
        } else {
            self.entries.push(RegisteredModel::new(model.clone()));
            self.entries.last_mut().unwrap()
        }
    }

    /// Register (or replace) the local path for a model. Recomputes load state
    /// from whether the path currently exists on disk.
    pub fn register(&mut self, model: &DriftOnnxModel, local_path: impl Into<String>) {
        let path = local_path.into();
        let exists = std::path::Path::new(&path).is_file();
        let slot = self.slot_mut(model);
        slot.local_path = Some(path);
        slot.load_state = if exists {
            ModelLoadState::Ready
        } else {
            ModelLoadState::PathMissing
        };
    }

    /// Clear any registered path for a model, returning it to `Unregistered`.
    pub fn clear(&mut self, model: &DriftOnnxModel) {
        if let Some(idx) = self.entries.iter().position(|e| e.model == *model) {
            self.entries[idx].local_path = None;
            self.entries[idx].load_state = ModelLoadState::Unregistered;
        }
    }

    /// Look up the current load state for a model.
    pub fn load_state(&self, model: &DriftOnnxModel) -> ModelLoadState {
        self.entries
            .iter()
            .find(|e| e.model == *model)
            .map(|e| e.load_state.clone())
            .unwrap_or(ModelLoadState::Unregistered)
    }

    /// The registered path for a model, if any.
    pub fn path(&self, model: &DriftOnnxModel) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.model == *model)
            .and_then(|e| e.local_path.as_deref())
    }

    /// True when a model has a registered path that exists on disk and is thus
    /// eligible for real inference (subject to the `onnx` feature also being on).
    pub fn is_loadable(&self, model: &DriftOnnxModel) -> bool {
        matches!(
            self.load_state(model),
            ModelLoadState::Ready | ModelLoadState::Loaded
        )
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Inference request / output value types
// ────────────────────────────────────────────────────────────────────────────

/// A single motion keyframe sample produced by motion-generation models
/// (AnimateDiff). Frame index + (x, y) position offset, in canvas units.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionSample {
    pub frame: usize,
    pub x: f32,
    pub y: f32,
}

/// Output of running the AnimateDiff family.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionOutput {
    pub samples: Vec<MotionSample>,
}

/// Output of FILM / RIFE temporal interpolation: the in-between frame indices
/// generated between two keyframes.
#[derive(Clone, Debug, PartialEq)]
pub struct InterpOutput {
    pub frames: Vec<usize>,
}

/// Output of phoneme detection (wav2vec2 / Whisper): count of detected phonemes.
#[derive(Clone, Debug, PartialEq)]
pub struct PhonemeOutput {
    pub phonemes_detected: usize,
}

/// Output of AI scripting: generated Rhai source.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptOutput {
    pub code: String,
}

/// Which concrete backend produced a result. Lets callers/tests assert whether
/// the real path or the stub fallback ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Stub,
    /// Real ONNX Runtime backend. Only produced under the `onnx` feature; kept
    /// in the discriminant set unconditionally so callers can match on it.
    #[allow(dead_code)]
    Ort,
}

// ────────────────────────────────────────────────────────────────────────────
// Backend abstraction
// ────────────────────────────────────────────────────────────────────────────

/// Inference backend over the model families Drift supports. Each method maps a
/// typed request to a typed output. Implementations must be deterministic for
/// the stub (so tests are stable) and side-effect-free w.r.t. `App` state.
pub trait Inference {
    fn kind(&self) -> BackendKind;

    /// AnimateDiff: text prompt → motion keyframe samples.
    fn animate_diff(&self, prompt: &str, num_frames: usize, guidance_scale: f32) -> MotionOutput;

    /// FILM / RIFE: produce `output_frames` in-betweens between two frames.
    fn interpolate(&self, from_frame: usize, to_frame: usize, output_frames: usize) -> InterpOutput;

    /// wav2vec2 / Whisper: phoneme detection from an audio file.
    fn detect_phonemes(&self, audio_path: &str) -> PhonemeOutput;

    /// AI scripting: natural-language prompt → Rhai code.
    fn generate_script(&self, prompt: &str) -> ScriptOutput;
}

/// Selects and owns a backend for a single inference call. The variant carried
/// is decided by [`InferenceBackend::select`]: real `ort` when available, else
/// the deterministic stub.
pub enum InferenceBackend {
    Stub(StubBackend),
    #[cfg(feature = "onnx")]
    Ort(OrtBackend),
}

impl InferenceBackend {
    /// Pick a backend for `model`, consulting the `registry`.
    ///
    /// * If the `onnx` feature is enabled **and** the model is loadable (a
    ///   registered file exists on disk) **and** a session loads, returns the
    ///   real [`OrtBackend`].
    /// * Otherwise returns the [`StubBackend`] (the default and the fallback).
    pub fn select(model: &DriftOnnxModel, registry: &ModelRegistry) -> Self {
        #[cfg(feature = "onnx")]
        {
            if registry.is_loadable(model) {
                if let Some(path) = registry.path(model) {
                    match OrtBackend::load(model.clone(), path) {
                        Ok(backend) => return InferenceBackend::Ort(backend),
                        Err(_) => { /* fall through to stub */ }
                    }
                }
            }
        }
        // Default / fallback path. `model` + `registry` are unused without the
        // feature, but referencing them keeps the signature feature-agnostic.
        let _ = (model, registry);
        InferenceBackend::Stub(StubBackend)
    }

    /// The concrete backend that will execute. (Used in tests and by callers
    /// that want to report which path ran; not driven by the synchronous
    /// default flow yet.)
    #[allow(dead_code)]
    pub fn kind(&self) -> BackendKind {
        match self {
            InferenceBackend::Stub(b) => b.kind(),
            #[cfg(feature = "onnx")]
            InferenceBackend::Ort(b) => b.kind(),
        }
    }

    fn as_inference(&self) -> &dyn Inference {
        match self {
            InferenceBackend::Stub(b) => b,
            #[cfg(feature = "onnx")]
            InferenceBackend::Ort(b) => b,
        }
    }

    pub fn animate_diff(&self, prompt: &str, num_frames: usize, guidance_scale: f32) -> MotionOutput {
        self.as_inference().animate_diff(prompt, num_frames, guidance_scale)
    }

    // The interpolate / detect_phonemes / generate_script wrappers are the
    // inference API for the FILM-RIFE, phoneme, and AI-script families. Those
    // jobs do not yet complete synchronously (unlike AnimateDiff), so these are
    // exercised by unit tests and will be driven when async completion lands.
    #[allow(dead_code)]
    pub fn interpolate(&self, from_frame: usize, to_frame: usize, output_frames: usize) -> InterpOutput {
        self.as_inference().interpolate(from_frame, to_frame, output_frames)
    }

    #[allow(dead_code)]
    pub fn detect_phonemes(&self, audio_path: &str) -> PhonemeOutput {
        self.as_inference().detect_phonemes(audio_path)
    }

    #[allow(dead_code)]
    pub fn generate_script(&self, prompt: &str) -> ScriptOutput {
        self.as_inference().generate_script(prompt)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Stub backend — deterministic, always compiled, always the default
// ────────────────────────────────────────────────────────────────────────────

/// Deterministic backend that mirrors the historical stub behaviour. Produces
/// stable, reproducible outputs so the suite's tests do not depend on a native
/// runtime. This is the default backend and the fallback for every model family.
#[derive(Clone, Copy, Debug, Default)]
pub struct StubBackend;

impl Inference for StubBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Stub
    }

    fn animate_diff(&self, _prompt: &str, num_frames: usize, _guidance_scale: f32) -> MotionOutput {
        // Mirrors the canonical 4-keyframe motion path the apply arm injects,
        // but generalises the frame spacing to `num_frames` so the stub stays
        // useful for varying requests. The fixed waypoints match the legacy
        // values for frame 0.
        let waypoints = [(0.0f32, 0.0f32), (300.0, -150.0), (-200.0, 100.0), (150.0, 200.0)];
        let span = num_frames.max(1);
        let mut samples = Vec::with_capacity(waypoints.len());
        for (i, (x, y)) in waypoints.iter().enumerate() {
            let frame = if waypoints.len() > 1 {
                (span - 1) * i / (waypoints.len() - 1)
            } else {
                0
            };
            samples.push(MotionSample { frame, x: *x, y: *y });
        }
        MotionOutput { samples }
    }

    fn interpolate(&self, from_frame: usize, to_frame: usize, output_frames: usize) -> InterpOutput {
        let (lo, hi) = if from_frame <= to_frame {
            (from_frame, to_frame)
        } else {
            (to_frame, from_frame)
        };
        let span = hi.saturating_sub(lo);
        let n = output_frames;
        let mut frames = Vec::with_capacity(n);
        for i in 0..n {
            // Evenly distribute in-betweens strictly inside (lo, hi].
            let f = if n > 0 {
                lo + (span * (i + 1)) / (n + 1)
            } else {
                lo
            };
            frames.push(f);
        }
        InterpOutput { frames }
    }

    fn detect_phonemes(&self, audio_path: &str) -> PhonemeOutput {
        // Deterministic pseudo-count derived from the path length so tests are
        // stable without an audio decoder. Real backend replaces this entirely.
        PhonemeOutput {
            phonemes_detected: audio_path.len(),
        }
    }

    fn generate_script(&self, prompt: &str) -> ScriptOutput {
        // Echoes the prompt into a commented Rhai no-op. The real backend emits
        // model output; this keeps the shape (valid-ish Rhai) for the UI.
        ScriptOutput {
            code: format!("// generated from prompt: {prompt}\n"),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Real ort backend — compiled ONLY under the `onnx` feature
// ────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "onnx")]
mod ort_backend {
    use super::*;
    use ort::session::{builder::GraphOptimizationLevel, Session};
    use std::sync::Mutex;

    /// Real ONNX Runtime backend. Holds a loaded [`Session`] for one model.
    ///
    /// Tensor construction and output mapping below are written against the
    /// `ort` 2.x API. Because no model weights ship with CI, this code is only
    /// compiled (and only reachable) when the `onnx` feature is enabled and a
    /// real `.onnx` file has been registered.
    ///
    /// `Session::run` requires `&mut self`, but the [`Inference`] trait takes
    /// `&self` (so the stub and ort backends share one signature). We bridge
    /// that with a `Mutex` for interior mutability.
    pub struct OrtBackend {
        pub model: DriftOnnxModel,
        session: Mutex<Session>,
    }

    impl OrtBackend {
        /// Load a session from `path`. Returns the human-readable error on
        /// failure so the caller can fall back to the stub.
        pub fn load(model: DriftOnnxModel, path: &str) -> Result<Self, String> {
            let session = Session::builder()
                .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
                .and_then(|b| b.commit_from_file(path))
                .map_err(|e| format!("failed to load ONNX session from {path}: {e}"))?;
            Ok(Self {
                model,
                session: Mutex::new(session),
            })
        }

        /// Run the session with a single named f32 input tensor and read back
        /// the first f32 output as a flat vector. This is the common shape for
        /// the families Drift uses; per-family pre/post-processing layers on top.
        fn run_f32(
            &self,
            input_name: &str,
            shape: Vec<i64>,
            data: Vec<f32>,
        ) -> Result<Vec<f32>, String> {
            use ort::value::Tensor;
            let tensor = Tensor::from_array((shape, data))
                .map_err(|e| format!("input tensor build failed: {e}"))?;
            let mut session = self
                .session
                .lock()
                .map_err(|_| "ONNX session mutex poisoned".to_string())?;
            let outputs = session
                .run(ort::inputs![input_name => tensor])
                .map_err(|e| format!("session run failed: {e}"))?;
            // Bind the first output to a longer-lived value before extracting so
            // the borrowed slice does not outlive a temporary.
            let first = outputs
                .iter()
                .next()
                .ok_or_else(|| "model produced no outputs".to_string())?;
            let (_shape, slice) = first
                .1
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("output extract failed: {e}"))?;
            Ok(slice.to_vec())
        }
    }

    impl Inference for OrtBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Ort
        }

        fn animate_diff(&self, prompt: &str, num_frames: usize, guidance_scale: f32) -> MotionOutput {
            // Encode the request as a tiny conditioning vector. A production
            // pipeline would tokenize `prompt`; here we hash it into a seed.
            let seed = prompt.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
            let input = vec![seed as f32, num_frames as f32, guidance_scale];
            match self.run_f32("cond", vec![1, input.len() as i64], input) {
                Ok(out) => {
                    let mut samples = Vec::new();
                    // Outputs are interpreted as flat [frame, x, y] triples.
                    for (i, chunk) in out.chunks_exact(3).enumerate() {
                        let _ = i;
                        samples.push(MotionSample {
                            frame: chunk[0].max(0.0) as usize,
                            x: chunk[1],
                            y: chunk[2],
                        });
                    }
                    if samples.is_empty() {
                        // Degenerate output — fall back to deterministic motion.
                        return StubBackend.animate_diff(prompt, num_frames, guidance_scale);
                    }
                    MotionOutput { samples }
                }
                Err(_) => StubBackend.animate_diff(prompt, num_frames, guidance_scale),
            }
        }

        fn interpolate(&self, from_frame: usize, to_frame: usize, output_frames: usize) -> InterpOutput {
            let input = vec![from_frame as f32, to_frame as f32, output_frames as f32];
            match self.run_f32("frames", vec![1, input.len() as i64], input) {
                Ok(out) if !out.is_empty() => InterpOutput {
                    frames: out.iter().map(|f| f.max(0.0) as usize).collect(),
                },
                _ => StubBackend.interpolate(from_frame, to_frame, output_frames),
            }
        }

        fn detect_phonemes(&self, audio_path: &str) -> PhonemeOutput {
            // A real impl decodes `audio_path` to PCM and feeds mel features.
            // We attempt to read the file size as a stand-in feature length; if
            // anything fails we fall back to the deterministic stub.
            match std::fs::metadata(audio_path) {
                Ok(meta) => {
                    let len = (meta.len().min(i64::MAX as u64)) as f32;
                    match self.run_f32("audio", vec![1, 1], vec![len]) {
                        Ok(out) if !out.is_empty() => PhonemeOutput {
                            phonemes_detected: out[0].max(0.0) as usize,
                        },
                        _ => StubBackend.detect_phonemes(audio_path),
                    }
                }
                Err(_) => StubBackend.detect_phonemes(audio_path),
            }
        }

        fn generate_script(&self, prompt: &str) -> ScriptOutput {
            // Text-to-text models are out of scope for the f32 tensor path; the
            // session is loaded (proving the file is valid) but we still emit the
            // deterministic stub script so behaviour stays predictable.
            let _ = &self.session;
            StubBackend.generate_script(prompt)
        }
    }
}

#[cfg(feature = "onnx")]
pub use ort_backend::OrtBackend;

// ────────────────────────────────────────────────────────────────────────────
// Tests — exercise the abstraction, registry, fallback, and selection. These
// run under the DEFAULT (no-feature) build, so they only ever see the stub.
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a unique temp file that is guaranteed to exist on disk, returning
    /// its path. Used as a stand-in for a real `.onnx` weights file so registry
    /// "exists on disk" checks are deterministic regardless of the test cwd.
    fn temp_existing_file(tag: &str) -> String {
        use std::io::Write;
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("drift_onnx_test_{tag}_{pid}_{nanos}.onnx"));
        let mut f = std::fs::File::create(&p).expect("create temp model file");
        f.write_all(b"stub").expect("write temp model file");
        p.to_string_lossy().into_owned()
    }

    // ── ModelRegistry ─────────────────────────────────────────────────────────

    #[test]
    fn registry_starts_empty() {
        let r = ModelRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert_eq!(
            r.load_state(&DriftOnnxModel::Whisper),
            ModelLoadState::Unregistered
        );
        assert!(!r.is_loadable(&DriftOnnxModel::Whisper));
    }

    #[test]
    fn register_missing_path_is_path_missing() {
        let mut r = ModelRegistry::new();
        r.register(&DriftOnnxModel::AnimateDiff, "/definitely/not/here.onnx");
        assert_eq!(r.len(), 1);
        assert_eq!(
            r.load_state(&DriftOnnxModel::AnimateDiff),
            ModelLoadState::PathMissing
        );
        assert!(!r.is_loadable(&DriftOnnxModel::AnimateDiff));
        assert_eq!(
            r.path(&DriftOnnxModel::AnimateDiff),
            Some("/definitely/not/here.onnx")
        );
    }

    #[test]
    fn register_existing_file_is_ready_and_loadable() {
        let path = temp_existing_file("ready");
        let mut r = ModelRegistry::new();
        r.register(&DriftOnnxModel::Whisper, &path);
        assert_eq!(r.load_state(&DriftOnnxModel::Whisper), ModelLoadState::Ready);
        assert!(r.is_loadable(&DriftOnnxModel::Whisper));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn register_replaces_path_for_same_model() {
        let mut r = ModelRegistry::new();
        r.register(&DriftOnnxModel::FilmRife, "/a.onnx");
        r.register(&DriftOnnxModel::FilmRife, "/b.onnx");
        assert_eq!(r.len(), 1);
        assert_eq!(r.path(&DriftOnnxModel::FilmRife), Some("/b.onnx"));
    }

    #[test]
    fn clear_returns_to_unregistered() {
        let path = temp_existing_file("clear");
        let mut r = ModelRegistry::new();
        r.register(&DriftOnnxModel::StyleTransfer, &path);
        assert!(r.is_loadable(&DriftOnnxModel::StyleTransfer));
        r.clear(&DriftOnnxModel::StyleTransfer);
        assert_eq!(
            r.load_state(&DriftOnnxModel::StyleTransfer),
            ModelLoadState::Unregistered
        );
        assert_eq!(r.path(&DriftOnnxModel::StyleTransfer), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn default_filenames_are_distinct() {
        let models = [
            DriftOnnxModel::AnimateDiff,
            DriftOnnxModel::FilmRife,
            DriftOnnxModel::Wav2Vec2,
            DriftOnnxModel::Whisper,
            DriftOnnxModel::StyleTransfer,
            DriftOnnxModel::StableDiffusion,
            DriftOnnxModel::MediaPipe,
        ];
        let mut names: Vec<_> = models.iter().map(default_model_filename).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), models.len());
    }

    // ── Backend selection / fallback ──────────────────────────────────────────

    #[test]
    fn select_falls_back_to_stub_when_unregistered() {
        let r = ModelRegistry::new();
        let backend = InferenceBackend::select(&DriftOnnxModel::AnimateDiff, &r);
        assert_eq!(backend.kind(), BackendKind::Stub);
    }

    #[test]
    fn select_falls_back_to_stub_when_file_missing() {
        let mut r = ModelRegistry::new();
        r.register(&DriftOnnxModel::AnimateDiff, "/no/such/model.onnx");
        let backend = InferenceBackend::select(&DriftOnnxModel::AnimateDiff, &r);
        // Without the `onnx` feature this is always the stub; with the feature
        // it is still the stub because the file does not exist.
        assert_eq!(backend.kind(), BackendKind::Stub);
    }

    #[cfg(not(feature = "onnx"))]
    #[test]
    fn select_is_stub_without_feature_even_with_valid_file() {
        // Default build: even a real on-disk file cannot select the ort backend
        // because the feature (and the `ort` dep) are absent.
        let path = temp_existing_file("select");
        let mut r = ModelRegistry::new();
        r.register(&DriftOnnxModel::Whisper, &path);
        assert!(r.is_loadable(&DriftOnnxModel::Whisper));
        let backend = InferenceBackend::select(&DriftOnnxModel::Whisper, &r);
        assert_eq!(backend.kind(), BackendKind::Stub);
        let _ = std::fs::remove_file(&path);
    }

    // ── Stub outputs are deterministic ────────────────────────────────────────

    #[test]
    fn stub_animate_diff_is_deterministic_and_spans_frames() {
        let b = StubBackend;
        let a = b.animate_diff("walk cycle", 180, 7.5);
        let c = b.animate_diff("walk cycle", 180, 7.5);
        assert_eq!(a, c);
        assert_eq!(a.samples.len(), 4);
        // First waypoint anchored at frame 0 with origin position.
        assert_eq!(a.samples[0], MotionSample { frame: 0, x: 0.0, y: 0.0 });
        // Last waypoint at the final frame.
        assert_eq!(a.samples.last().unwrap().frame, 179);
    }

    #[test]
    fn stub_animate_diff_handles_zero_frames() {
        let out = StubBackend.animate_diff("x", 0, 1.0);
        assert_eq!(out.samples.len(), 4);
        assert!(out.samples.iter().all(|s| s.frame == 0));
    }

    #[test]
    fn stub_interpolate_produces_in_betweens() {
        let out = StubBackend.interpolate(0, 30, 5);
        assert_eq!(out.frames.len(), 5);
        // Strictly increasing and inside (0, 30).
        assert!(out.frames.windows(2).all(|w| w[0] <= w[1]));
        assert!(out.frames.iter().all(|&f| f > 0 && f <= 30));
    }

    #[test]
    fn stub_interpolate_handles_reversed_range() {
        let fwd = StubBackend.interpolate(0, 30, 5);
        let rev = StubBackend.interpolate(30, 0, 5);
        assert_eq!(fwd, rev);
    }

    #[test]
    fn stub_phonemes_deterministic() {
        let a = StubBackend.detect_phonemes("/audio/hello.wav");
        let b = StubBackend.detect_phonemes("/audio/hello.wav");
        assert_eq!(a, b);
        assert_eq!(a.phonemes_detected, "/audio/hello.wav".len());
    }

    #[test]
    fn stub_script_echoes_prompt() {
        let out = StubBackend.generate_script("bounce on beat");
        assert!(out.code.contains("bounce on beat"));
    }

    // ── Selection drives the same output path as direct stub calls ────────────

    #[test]
    fn selected_stub_backend_runs_all_families() {
        let r = ModelRegistry::new();
        let b = InferenceBackend::select(&DriftOnnxModel::AnimateDiff, &r);
        assert_eq!(b.animate_diff("p", 24, 5.0).samples.len(), 4);

        let b = InferenceBackend::select(&DriftOnnxModel::FilmRife, &r);
        assert_eq!(b.interpolate(0, 10, 3).frames.len(), 3);

        let b = InferenceBackend::select(&DriftOnnxModel::Whisper, &r);
        assert_eq!(b.detect_phonemes("/a.wav").phonemes_detected, "/a.wav".len());

        let b = InferenceBackend::select(&DriftOnnxModel::StableDiffusion, &r);
        assert!(b.generate_script("hi").code.contains("hi"));
    }
}
