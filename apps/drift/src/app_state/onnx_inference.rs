use super::{App, Action};

/// Status of an individual ONNX inference job.
#[derive(Clone, Debug, PartialEq)]
pub enum OnnxJobStatus {
    Queued,
    Loading,
    Running,
    Done,
    Error,
}

/// Which ONNX model is being used for a given Drift AI feature.
#[derive(Clone, Debug, PartialEq)]
pub enum DriftOnnxModel {
    AnimateDiff,
    FilmRife,
    Wav2Vec2,
    Whisper,
    StyleTransfer,
    StableDiffusion,
    MediaPipe,
}

/// Download / readiness state for a model asset.
#[derive(Clone, Debug, PartialEq)]
pub enum OnnxModelStatus {
    NotDownloaded,
    Downloading,
    Ready,
    Error,
}

/// Registry entry for a Drift ONNX model.
#[derive(Clone, Debug)]
pub struct DriftOnnxModelEntry {
    pub model: DriftOnnxModel,
    pub local_path: Option<String>,
    pub status: OnnxModelStatus,
    pub download_progress: f32,
}

/// AnimateDiff: text prompt → motion keyframes.
#[derive(Clone, Debug)]
pub struct AnimateDiffJob {
    pub id: usize,
    pub layer_id: usize,
    pub prompt: String,
    pub num_frames: usize,
    pub guidance_scale: f32,
    pub status: OnnxJobStatus,
    pub error: Option<String>,
}

/// FILM / RIFE: temporal frame interpolation between two keyframes.
#[derive(Clone, Debug)]
pub struct FilmRifeJob {
    pub id: usize,
    pub layer_id: usize,
    pub from_frame: usize,
    pub to_frame: usize,
    pub output_frames: usize,
    pub model: DriftOnnxModel,
    pub status: OnnxJobStatus,
}

/// wav2vec2 / Whisper: phoneme detection from audio → lip-sync keyframes.
#[derive(Clone, Debug)]
pub struct PhonemeDetectJob {
    pub id: usize,
    pub audio_path: String,
    pub target_layer_id: usize,
    pub model: DriftOnnxModel,
    pub phonemes_detected: usize,
    pub status: OnnxJobStatus,
}

/// Style transfer: apply visual style to a bitmap layer.
#[derive(Clone, Debug)]
pub struct StyleTransferJob {
    pub id: usize,
    pub layer_id: usize,
    pub style_prompt: String,
    pub strength: f32,
    pub status: OnnxJobStatus,
}

/// AI background generation (Stable Diffusion stub).
#[derive(Clone, Debug)]
pub struct AiBgGenJob {
    pub id: usize,
    pub prompt: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub output_layer_id: Option<usize>,
    pub status: OnnxJobStatus,
}

/// AI scripting: natural-language prompt → Rhai code.
#[derive(Clone, Debug)]
pub struct AiScriptJob {
    pub id: usize,
    pub prompt: String,
    pub generated_code: String,
    pub status: OnnxJobStatus,
}

impl App {
    pub(super) fn apply_onnx_inference(&mut self, action: &Action) {
        match action {
            Action::RegisterDriftModel { model, local_path } => {
                let entry = DriftOnnxModelEntry {
                    model: model.clone(),
                    local_path: Some(local_path.clone()),
                    status: OnnxModelStatus::Ready,
                    download_progress: 1.0,
                };
                self.drift_onnx_models.retain(|m| m.model != *model);
                self.drift_onnx_models.push(entry);
            }
            Action::StartDriftModelDownload { model } => {
                if let Some(e) = self.drift_onnx_models.iter_mut().find(|m| m.model == *model) {
                    e.status = OnnxModelStatus::Downloading;
                    e.download_progress = 0.0;
                } else {
                    self.drift_onnx_models.push(DriftOnnxModelEntry {
                        model: model.clone(),
                        local_path: None,
                        status: OnnxModelStatus::Downloading,
                        download_progress: 0.0,
                    });
                }
            }
            Action::UpdateDriftModelDownload { model, progress } => {
                if let Some(e) = self.drift_onnx_models.iter_mut().find(|m| m.model == *model) {
                    e.download_progress = progress.clamp(0.0, 1.0);
                } else {
                    self.drift_onnx_models.push(DriftOnnxModelEntry {
                        model: model.clone(),
                        local_path: None,
                        status: OnnxModelStatus::Downloading,
                        download_progress: progress.clamp(0.0, 1.0),
                    });
                }
            }
            Action::CompleteDriftModelDownload { model, local_path } => {
                if let Some(e) = self.drift_onnx_models.iter_mut().find(|m| m.model == *model) {
                    e.status = OnnxModelStatus::Ready;
                    e.download_progress = 1.0;
                    e.local_path = Some(local_path.clone());
                } else {
                    self.drift_onnx_models.push(DriftOnnxModelEntry {
                        model: model.clone(),
                        local_path: Some(local_path.clone()),
                        status: OnnxModelStatus::Ready,
                        download_progress: 1.0,
                    });
                }
            }
            Action::ErrorDriftModelDownload { model, message: _ } => {
                if let Some(e) = self.drift_onnx_models.iter_mut().find(|m| m.model == *model) {
                    e.status = OnnxModelStatus::Error;
                } else {
                    self.drift_onnx_models.push(DriftOnnxModelEntry {
                        model: model.clone(),
                        local_path: None,
                        status: OnnxModelStatus::Error,
                        download_progress: 0.0,
                    });
                }
            }
            Action::QueueAnimateDiff { layer_id, prompt, num_frames, guidance_scale } => {
                let id = self.next_animatediff_id;
                self.next_animatediff_id += 1;
                self.animatediff_jobs.push(AnimateDiffJob {
                    id,
                    layer_id: *layer_id,
                    prompt: prompt.clone(),
                    num_frames: *num_frames,
                    guidance_scale: *guidance_scale,
                    status: OnnxJobStatus::Done,
                    error: None,
                });
                // Stub: immediately add motion keyframes to the target layer so
                // the UI resets out of "Generating..." as soon as the action fires.
                let lid = *layer_id;
                let kf_counter = &mut self.keyframe_counter;
                let keyframes = &mut self.keyframes;
                for (frame, x, y) in [(0usize, 0.0f32, 0.0f32), (60, 300.0, -150.0), (120, -200.0, 100.0), (180, 150.0, 200.0)] {
                    keyframes.push(super::keyframes::Keyframe {
                        id: { let k = *kf_counter; *kf_counter += 1; k },
                        layer_id: lid,
                        property: "position_x".to_string(),
                        frame,
                        value: x,
                        easing: super::keyframes::EasingKind::EaseInOut,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                    keyframes.push(super::keyframes::Keyframe {
                        id: { let k = *kf_counter; *kf_counter += 1; k },
                        layer_id: lid,
                        property: "position_y".to_string(),
                        frame,
                        value: y,
                        easing: super::keyframes::EasingKind::EaseInOut,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }
            Action::CompleteAnimateDiff { job_id } => {
                if let Some(j) = self.animatediff_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = OnnxJobStatus::Done;
                }
            }
            Action::FailAnimateDiff { job_id, error } => {
                if let Some(j) = self.animatediff_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = OnnxJobStatus::Error;
                    j.error = Some(error.clone());
                }
            }
            Action::QueueFilmRife { layer_id, from_frame, to_frame, output_frames, model } => {
                let id = self.next_filmrife_id;
                self.next_filmrife_id += 1;
                self.filmrife_jobs.push(FilmRifeJob {
                    id,
                    layer_id: *layer_id,
                    from_frame: *from_frame,
                    to_frame: *to_frame,
                    output_frames: *output_frames,
                    model: model.clone(),
                    status: OnnxJobStatus::Queued,
                });
            }
            Action::CompleteFilmRife { job_id } => {
                if let Some(j) = self.filmrife_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = OnnxJobStatus::Done;
                }
            }
            Action::QueuePhonemeDetect { audio_path, target_layer_id, model } => {
                let id = self.next_phoneme_job_id;
                self.next_phoneme_job_id += 1;
                self.phoneme_jobs.push(PhonemeDetectJob {
                    id,
                    audio_path: audio_path.clone(),
                    target_layer_id: *target_layer_id,
                    model: model.clone(),
                    phonemes_detected: 0,
                    status: OnnxJobStatus::Queued,
                });
            }
            Action::CompletePhonemeDetect { job_id, phonemes_detected } => {
                if let Some(j) = self.phoneme_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = OnnxJobStatus::Done;
                    j.phonemes_detected = *phonemes_detected;
                }
            }
            Action::QueueStyleTransfer { layer_id, style_prompt, strength } => {
                let id = self.next_style_job_id;
                self.next_style_job_id += 1;
                self.style_jobs.push(StyleTransferJob {
                    id,
                    layer_id: *layer_id,
                    style_prompt: style_prompt.clone(),
                    strength: *strength,
                    status: OnnxJobStatus::Queued,
                });
            }
            Action::CompleteStyleTransfer { job_id } => {
                if let Some(j) = self.style_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = OnnxJobStatus::Done;
                }
            }
            Action::QueueAiBgGen { prompt, width, height, steps } => {
                let id = self.next_bg_gen_id;
                self.next_bg_gen_id += 1;
                self.bg_gen_jobs.push(AiBgGenJob {
                    id,
                    prompt: prompt.clone(),
                    width: *width,
                    height: *height,
                    steps: *steps,
                    output_layer_id: None,
                    status: OnnxJobStatus::Queued,
                });
            }
            Action::CompleteAiBgGen { job_id, output_layer_id } => {
                if let Some(j) = self.bg_gen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = OnnxJobStatus::Done;
                    j.output_layer_id = Some(*output_layer_id);
                }
            }
            Action::QueueAiScript { prompt } => {
                let id = self.next_ai_script_id;
                self.next_ai_script_id += 1;
                self.ai_script_jobs.push(AiScriptJob {
                    id,
                    prompt: prompt.clone(),
                    generated_code: String::new(),
                    status: OnnxJobStatus::Queued,
                });
            }
            Action::CompleteAiScript { job_id, code } => {
                if let Some(j) = self.ai_script_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.generated_code = code.clone();
                    j.status = OnnxJobStatus::Done;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{DriftOnnxModel, OnnxJobStatus, OnnxModelStatus};

    fn app() -> App {
        App::new()
    }

    // ── RegisterDriftModel ────────────────────────────────────────────────────

    #[test]
    fn test_register_drift_model() {
        let mut a = app();
        a.apply(Action::RegisterDriftModel {
            model: DriftOnnxModel::AnimateDiff,
            local_path: "/models/animatediff.onnx".to_string(),
        });
        assert_eq!(a.drift_onnx_models.len(), 1);
        assert_eq!(a.drift_onnx_models[0].status, OnnxModelStatus::Ready);
        assert_eq!(a.drift_onnx_models[0].download_progress, 1.0);
    }

    #[test]
    fn test_register_drift_model_replaces_existing() {
        let mut a = app();
        a.apply(Action::RegisterDriftModel {
            model: DriftOnnxModel::Whisper,
            local_path: "/old.onnx".to_string(),
        });
        a.apply(Action::RegisterDriftModel {
            model: DriftOnnxModel::Whisper,
            local_path: "/new.onnx".to_string(),
        });
        assert_eq!(a.drift_onnx_models.len(), 1);
        assert_eq!(
            a.drift_onnx_models[0].local_path.as_deref(),
            Some("/new.onnx")
        );
    }

    // ── AnimateDiff ───────────────────────────────────────────────────────────

    #[test]
    fn test_queue_animatediff() {
        let mut a = app();
        a.apply(Action::QueueAnimateDiff {
            layer_id: 1,
            prompt: "walk cycle".to_string(),
            num_frames: 24,
            guidance_scale: 7.5,
        });
        assert_eq!(a.animatediff_jobs.len(), 1);
        assert_eq!(a.animatediff_jobs[0].prompt, "walk cycle");
        // The stub completes synchronously: queuing an AnimateDiff job adds the
        // motion keyframes immediately and marks the job Done so the UI leaves
        // its "Generating…" state as soon as the action fires.
        assert_eq!(a.animatediff_jobs[0].status, OnnxJobStatus::Done);
        assert!(a.animatediff_jobs[0].error.is_none());
    }

    #[test]
    fn test_complete_animatediff() {
        let mut a = app();
        a.apply(Action::QueueAnimateDiff {
            layer_id: 0,
            prompt: "run".to_string(),
            num_frames: 30,
            guidance_scale: 5.0,
        });
        let id = a.animatediff_jobs[0].id;
        a.apply(Action::CompleteAnimateDiff { job_id: id });
        assert_eq!(a.animatediff_jobs[0].status, OnnxJobStatus::Done);
    }

    #[test]
    fn test_fail_animatediff() {
        let mut a = app();
        a.apply(Action::QueueAnimateDiff {
            layer_id: 0,
            prompt: "jump".to_string(),
            num_frames: 12,
            guidance_scale: 7.0,
        });
        let id = a.animatediff_jobs[0].id;
        a.apply(Action::FailAnimateDiff {
            job_id: id,
            error: "CUDA OOM".to_string(),
        });
        assert_eq!(a.animatediff_jobs[0].status, OnnxJobStatus::Error);
        assert_eq!(a.animatediff_jobs[0].error.as_deref(), Some("CUDA OOM"));
    }

    #[test]
    fn test_animatediff_unique_ids() {
        let mut a = app();
        a.apply(Action::QueueAnimateDiff {
            layer_id: 0,
            prompt: "A".to_string(),
            num_frames: 10,
            guidance_scale: 5.0,
        });
        a.apply(Action::QueueAnimateDiff {
            layer_id: 1,
            prompt: "B".to_string(),
            num_frames: 10,
            guidance_scale: 5.0,
        });
        assert_ne!(a.animatediff_jobs[0].id, a.animatediff_jobs[1].id);
    }

    // ── FILM / RIFE ───────────────────────────────────────────────────────────

    #[test]
    fn test_queue_film_rife() {
        let mut a = app();
        a.apply(Action::QueueFilmRife {
            layer_id: 2,
            from_frame: 0,
            to_frame: 30,
            output_frames: 5,
            model: DriftOnnxModel::FilmRife,
        });
        assert_eq!(a.filmrife_jobs.len(), 1);
        assert_eq!(a.filmrife_jobs[0].output_frames, 5);
        assert_eq!(a.filmrife_jobs[0].status, OnnxJobStatus::Queued);
    }

    #[test]
    fn test_complete_film_rife() {
        let mut a = app();
        a.apply(Action::QueueFilmRife {
            layer_id: 0,
            from_frame: 0,
            to_frame: 24,
            output_frames: 3,
            model: DriftOnnxModel::FilmRife,
        });
        let id = a.filmrife_jobs[0].id;
        a.apply(Action::CompleteFilmRife { job_id: id });
        assert_eq!(a.filmrife_jobs[0].status, OnnxJobStatus::Done);
    }

    // ── Phoneme Detect ────────────────────────────────────────────────────────

    #[test]
    fn test_queue_phoneme_detect() {
        let mut a = app();
        a.apply(Action::QueuePhonemeDetect {
            audio_path: "/audio/speech.wav".to_string(),
            target_layer_id: 3,
            model: DriftOnnxModel::Wav2Vec2,
        });
        assert_eq!(a.phoneme_jobs.len(), 1);
        assert_eq!(a.phoneme_jobs[0].phonemes_detected, 0);
        assert_eq!(a.phoneme_jobs[0].status, OnnxJobStatus::Queued);
    }

    #[test]
    fn test_complete_phoneme_detect() {
        let mut a = app();
        a.apply(Action::QueuePhonemeDetect {
            audio_path: "/audio/hello.wav".to_string(),
            target_layer_id: 0,
            model: DriftOnnxModel::Whisper,
        });
        let id = a.phoneme_jobs[0].id;
        a.apply(Action::CompletePhonemeDetect {
            job_id: id,
            phonemes_detected: 42,
        });
        assert_eq!(a.phoneme_jobs[0].status, OnnxJobStatus::Done);
        assert_eq!(a.phoneme_jobs[0].phonemes_detected, 42);
    }

    // ── Style Transfer ────────────────────────────────────────────────────────

    #[test]
    fn test_queue_style_transfer() {
        let mut a = app();
        a.apply(Action::QueueStyleTransfer {
            layer_id: 1,
            style_prompt: "watercolor painting".to_string(),
            strength: 0.8,
        });
        assert_eq!(a.style_jobs.len(), 1);
        assert_eq!(a.style_jobs[0].status, OnnxJobStatus::Queued);
    }

    #[test]
    fn test_complete_style_transfer() {
        let mut a = app();
        a.apply(Action::QueueStyleTransfer {
            layer_id: 0,
            style_prompt: "oil painting".to_string(),
            strength: 0.5,
        });
        let id = a.style_jobs[0].id;
        a.apply(Action::CompleteStyleTransfer { job_id: id });
        assert_eq!(a.style_jobs[0].status, OnnxJobStatus::Done);
    }

    // ── AI Background Gen ─────────────────────────────────────────────────────

    #[test]
    fn test_queue_ai_bg_gen() {
        let mut a = app();
        a.apply(Action::QueueAiBgGen {
            prompt: "forest at dusk".to_string(),
            width: 1920,
            height: 1080,
            steps: 30,
        });
        assert_eq!(a.bg_gen_jobs.len(), 1);
        assert_eq!(a.bg_gen_jobs[0].width, 1920);
        assert!(a.bg_gen_jobs[0].output_layer_id.is_none());
        assert_eq!(a.bg_gen_jobs[0].status, OnnxJobStatus::Queued);
    }

    #[test]
    fn test_complete_ai_bg_gen() {
        let mut a = app();
        a.apply(Action::QueueAiBgGen {
            prompt: "night sky".to_string(),
            width: 1280,
            height: 720,
            steps: 20,
        });
        let id = a.bg_gen_jobs[0].id;
        a.apply(Action::CompleteAiBgGen {
            job_id: id,
            output_layer_id: 7,
        });
        assert_eq!(a.bg_gen_jobs[0].status, OnnxJobStatus::Done);
        assert_eq!(a.bg_gen_jobs[0].output_layer_id, Some(7));
    }

    // ── AI Script ─────────────────────────────────────────────────────────────

    #[test]
    fn test_queue_ai_script() {
        let mut a = app();
        a.apply(Action::QueueAiScript {
            prompt: "make layer bounce on beat".to_string(),
        });
        assert_eq!(a.ai_script_jobs.len(), 1);
        assert_eq!(a.ai_script_jobs[0].generated_code, "");
        assert_eq!(a.ai_script_jobs[0].status, OnnxJobStatus::Queued);
    }

    #[test]
    fn test_complete_ai_script() {
        let mut a = app();
        a.apply(Action::QueueAiScript {
            prompt: "rotate layer 360 degrees".to_string(),
        });
        let id = a.ai_script_jobs[0].id;
        a.apply(Action::CompleteAiScript {
            job_id: id,
            code: "layer.rotation += 360.0;".to_string(),
        });
        assert_eq!(a.ai_script_jobs[0].status, OnnxJobStatus::Done);
        assert_eq!(
            a.ai_script_jobs[0].generated_code,
            "layer.rotation += 360.0;"
        );
    }

    #[test]
    fn test_ai_script_unique_ids() {
        let mut a = app();
        a.apply(Action::QueueAiScript { prompt: "A".to_string() });
        a.apply(Action::QueueAiScript { prompt: "B".to_string() });
        assert_ne!(a.ai_script_jobs[0].id, a.ai_script_jobs[1].id);
    }
}
