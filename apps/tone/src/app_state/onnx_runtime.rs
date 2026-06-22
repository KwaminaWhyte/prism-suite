//! ONNX runtime model management for Tone.
//!
//! The shared `prism-ai` crate is not yet promoted. This module holds inline
//! ONNX model registry state so Tone can track downloads, load status, and
//! queued inference jobs without depending on the future shared crate.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Which AI model this entry describes.
#[derive(Clone, Debug, PartialEq)]
pub enum OnnxModelKind {
    MusicGen,
    Demucs,
    MelodyRnn,
    MusicTransformer,
    AudioSr,
    AiMasterNet,
}

/// Download / availability status for an ONNX model.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelDownloadStatus {
    NotDownloaded,
    Downloading,
    Downloaded,
    Error,
}

/// A single entry in the model registry.
#[derive(Clone, Debug)]
pub struct OnnxModelEntry {
    pub kind: OnnxModelKind,
    pub name: String,
    pub url_hint: String,
    pub local_path: Option<String>,
    pub size_mb: u32,
    pub status: ModelDownloadStatus,
    pub download_progress: f32,
}

/// A queued or completed inference job.
#[derive(Clone, Debug)]
pub struct OnnxInferenceJob {
    pub id: usize,
    pub model_kind: OnnxModelKind,
    pub input_desc: String,
    pub status_done: bool,
    pub error: Option<String>,
}

// ─── App impl ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_onnx_runtime(&mut self, action: &Action) {
        match action {
            Action::RegisterOnnxModel { kind, name, url_hint, size_mb } => {
                if !self.onnx_models.iter().any(|m| m.kind == *kind) {
                    self.onnx_models.push(OnnxModelEntry {
                        kind: kind.clone(),
                        name: name.clone(),
                        url_hint: url_hint.clone(),
                        local_path: None,
                        size_mb: *size_mb,
                        status: ModelDownloadStatus::NotDownloaded,
                        download_progress: 0.0,
                    });
                }
            }
            Action::StartModelDownload { kind } => {
                if let Some(m) = self.onnx_models.iter_mut().find(|m| m.kind == *kind) {
                    m.status = ModelDownloadStatus::Downloading;
                }
            }
            Action::UpdateModelDownload { kind, progress } => {
                if let Some(m) = self.onnx_models.iter_mut().find(|m| m.kind == *kind) {
                    m.download_progress = progress.clamp(0.0, 1.0);
                }
            }
            Action::CompleteModelDownload { kind, local_path } => {
                if let Some(m) = self.onnx_models.iter_mut().find(|m| m.kind == *kind) {
                    m.status = ModelDownloadStatus::Downloaded;
                    m.local_path = Some(local_path.clone());
                    m.download_progress = 1.0;
                }
            }
            Action::RemoveOnnxModel { kind } => {
                self.onnx_models.retain(|m| m.kind != *kind);
            }
            Action::QueueOnnxInference { model_kind, input_desc } => {
                let id = self.next_onnx_job_id;
                self.next_onnx_job_id += 1;
                self.onnx_inference_jobs.push(OnnxInferenceJob {
                    id,
                    model_kind: model_kind.clone(),
                    input_desc: input_desc.clone(),
                    status_done: false,
                    error: None,
                });
            }
            Action::CompleteOnnxInference { job_id } => {
                if let Some(j) = self.onnx_inference_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status_done = true;
                }
            }
            Action::FailOnnxInference { job_id, error } => {
                if let Some(j) = self.onnx_inference_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.error = Some(error.clone());
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn register_model_adds_entry() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::MusicGen,
            name: "MusicGen Small".into(),
            url_hint: "https://example.com/musicgen.onnx".into(),
            size_mb: 250,
        });
        assert_eq!(app.onnx_models.len(), 1);
        assert_eq!(app.onnx_models[0].kind, OnnxModelKind::MusicGen);
        assert_eq!(app.onnx_models[0].status, ModelDownloadStatus::NotDownloaded);
    }

    #[test]
    fn register_model_deduplicates_by_kind() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::Demucs,
            name: "Demucs v4".into(),
            url_hint: "https://example.com/demucs.onnx".into(),
            size_mb: 300,
        });
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::Demucs,
            name: "Demucs v4 dup".into(),
            url_hint: "https://example.com/demucs2.onnx".into(),
            size_mb: 300,
        });
        assert_eq!(app.onnx_models.len(), 1);
    }

    #[test]
    fn start_model_download_sets_status() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::MelodyRnn,
            name: "MelodyRnn".into(),
            url_hint: "https://example.com/melodyrnn.onnx".into(),
            size_mb: 50,
        });
        app.apply(Action::StartModelDownload { kind: OnnxModelKind::MelodyRnn });
        assert_eq!(app.onnx_models[0].status, ModelDownloadStatus::Downloading);
    }

    #[test]
    fn update_model_download_progress() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::AudioSr,
            name: "AudioSR".into(),
            url_hint: "https://example.com/audiosr.onnx".into(),
            size_mb: 100,
        });
        app.apply(Action::StartModelDownload { kind: OnnxModelKind::AudioSr });
        app.apply(Action::UpdateModelDownload { kind: OnnxModelKind::AudioSr, progress: 0.5 });
        assert!((app.onnx_models[0].download_progress - 0.5).abs() < 0.001);
    }

    #[test]
    fn update_model_download_progress_clamped() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::AiMasterNet,
            name: "AiMasterNet".into(),
            url_hint: "https://example.com/aim.onnx".into(),
            size_mb: 80,
        });
        app.apply(Action::UpdateModelDownload { kind: OnnxModelKind::AiMasterNet, progress: 2.5 });
        assert!(app.onnx_models[0].download_progress <= 1.0);
        app.apply(Action::UpdateModelDownload { kind: OnnxModelKind::AiMasterNet, progress: -0.5 });
        assert!(app.onnx_models[0].download_progress >= 0.0);
    }

    #[test]
    fn complete_model_download() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::MusicTransformer,
            name: "MusicTransformer".into(),
            url_hint: "https://example.com/mt.onnx".into(),
            size_mb: 400,
        });
        app.apply(Action::CompleteModelDownload {
            kind: OnnxModelKind::MusicTransformer,
            local_path: "/models/music_transformer.onnx".into(),
        });
        let entry = &app.onnx_models[0];
        assert_eq!(entry.status, ModelDownloadStatus::Downloaded);
        assert_eq!(entry.local_path.as_deref(), Some("/models/music_transformer.onnx"));
        assert!((entry.download_progress - 1.0).abs() < 0.001);
    }

    #[test]
    fn remove_onnx_model() {
        let mut app = fresh();
        app.apply(Action::RegisterOnnxModel {
            kind: OnnxModelKind::Demucs,
            name: "Demucs".into(),
            url_hint: "https://example.com/d.onnx".into(),
            size_mb: 300,
        });
        assert_eq!(app.onnx_models.len(), 1);
        app.apply(Action::RemoveOnnxModel { kind: OnnxModelKind::Demucs });
        assert!(app.onnx_models.is_empty());
    }

    #[test]
    fn queue_inference_job_increments_id() {
        let mut app = fresh();
        app.apply(Action::QueueOnnxInference {
            model_kind: OnnxModelKind::MusicGen,
            input_desc: "generate 8 bars jazz".into(),
        });
        app.apply(Action::QueueOnnxInference {
            model_kind: OnnxModelKind::Demucs,
            input_desc: "separate stems".into(),
        });
        assert_eq!(app.onnx_inference_jobs.len(), 2);
        assert_ne!(app.onnx_inference_jobs[0].id, app.onnx_inference_jobs[1].id);
    }

    #[test]
    fn complete_inference_job() {
        let mut app = fresh();
        app.apply(Action::QueueOnnxInference {
            model_kind: OnnxModelKind::AudioSr,
            input_desc: "upsample clip 0".into(),
        });
        let job_id = app.onnx_inference_jobs[0].id;
        app.apply(Action::CompleteOnnxInference { job_id });
        assert!(app.onnx_inference_jobs[0].status_done);
        assert!(app.onnx_inference_jobs[0].error.is_none());
    }

    #[test]
    fn fail_inference_job() {
        let mut app = fresh();
        app.apply(Action::QueueOnnxInference {
            model_kind: OnnxModelKind::MelodyRnn,
            input_desc: "generate melody".into(),
        });
        let job_id = app.onnx_inference_jobs[0].id;
        app.apply(Action::FailOnnxInference { job_id, error: "out of memory".into() });
        let job = &app.onnx_inference_jobs[0];
        assert!(!job.status_done);
        assert_eq!(job.error.as_deref(), Some("out of memory"));
    }

    #[test]
    fn multiple_model_kinds_registered() {
        let mut app = fresh();
        for (kind, name, size) in [
            (OnnxModelKind::MusicGen, "MusicGen", 250u32),
            (OnnxModelKind::Demucs, "Demucs", 300),
            (OnnxModelKind::MelodyRnn, "MelodyRnn", 50),
            (OnnxModelKind::AudioSr, "AudioSR", 100),
        ] {
            app.apply(Action::RegisterOnnxModel {
                kind,
                name: name.into(),
                url_hint: "https://example.com/model.onnx".into(),
                size_mb: size,
            });
        }
        assert_eq!(app.onnx_models.len(), 4);
    }

    #[test]
    fn start_download_on_missing_kind_is_noop() {
        let mut app = fresh();
        // No panic — just ignored
        app.apply(Action::StartModelDownload { kind: OnnxModelKind::AiMasterNet });
        assert!(app.onnx_models.is_empty());
    }

    #[test]
    fn inference_job_starts_undone() {
        let mut app = fresh();
        app.apply(Action::QueueOnnxInference {
            model_kind: OnnxModelKind::MusicTransformer,
            input_desc: "harmonize track 1".into(),
        });
        let job = &app.onnx_inference_jobs[0];
        assert!(!job.status_done);
        assert!(job.error.is_none());
    }

    #[test]
    fn next_onnx_job_id_starts_at_one() {
        let app = fresh();
        assert_eq!(app.next_onnx_job_id, 1);
    }
}
