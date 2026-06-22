use super::{App, Action};

/// Which ONNX model to use for AI motion generation.
#[derive(Clone, Debug, PartialEq)]
pub enum AiMotionModel {
    MotionDiffuse,
    CharacterNet,
    PoseEstimator,
    CustomOnnx { path: String },
}

/// Progress / outcome of an AI request.
#[derive(Clone, Debug, PartialEq)]
pub enum AiRequestStatus {
    Pending,
    Running { progress: f32 },
    Done,
    Failed(String),
}

/// A text-prompt → keyframe generation request.
#[derive(Clone, Debug)]
pub struct AiMotionRequest {
    pub id: usize,
    pub prompt: String,
    pub model: AiMotionModel,
    pub target_layer_ids: Vec<usize>,
    pub duration_frames: u32,
    pub seed: Option<u64>,
    /// Sampling temperature 0.1..=2.0.
    pub temperature: f32,
    pub status: AiRequestStatus,
    pub result_keyframe_ids: Vec<usize>,
}

/// A frame-interpolation request between two existing keyframes.
#[derive(Clone, Debug)]
pub struct AiInterpolationRequest {
    pub id: usize,
    pub layer_id: usize,
    pub from_frame: u32,
    pub to_frame: u32,
    pub num_inbetweens: u32,
    pub model: AiMotionModel,
    pub status: AiRequestStatus,
}

/// A style-transfer request that applies the motion style of a reference clip.
#[derive(Clone, Debug)]
pub struct AiStyleTransfer {
    pub id: usize,
    pub source_clip_path: String,
    pub target_layer_id: usize,
    /// 0.0..=1.0
    pub style_strength: f32,
    pub preserve_timing: bool,
    pub status: AiRequestStatus,
}

/// Compute backend used for ONNX inference.
#[derive(Clone, Debug, PartialEq)]
pub enum AiBackend {
    Cpu,
    Cuda,
    Metal,
    CoreMl,
}

impl App {
    pub fn apply_ai_motion(&mut self, action: Action) {
        match action {
            Action::RequestAiMotion { prompt, model, layer_ids, frames } => {
                let id = self.next_ai_motion_id;
                self.next_ai_motion_id += 1;
                self.ai_motion_requests.push(AiMotionRequest {
                    id,
                    prompt,
                    model,
                    target_layer_ids: layer_ids,
                    duration_frames: frames,
                    seed: None,
                    temperature: 1.0,
                    status: AiRequestStatus::Pending,
                    result_keyframe_ids: Vec::new(),
                });
            }
            Action::UpdateAiMotionStatus { request_id, status } => {
                if let Some(r) = self.ai_motion_requests.iter_mut().find(|r| r.id == request_id) {
                    r.status = status;
                }
            }
            Action::CancelAiMotion { request_id } => {
                if let Some(r) = self.ai_motion_requests.iter_mut().find(|r| r.id == request_id) {
                    r.status = AiRequestStatus::Failed("Cancelled".to_string());
                }
            }
            Action::RequestAiInterpolation2 { layer_id, from_frame, to_frame, inbetweens } => {
                let id = self.next_interp_id;
                self.next_interp_id += 1;
                self.ai_interpolation_requests.push(AiInterpolationRequest {
                    id,
                    layer_id,
                    from_frame,
                    to_frame,
                    num_inbetweens: inbetweens,
                    model: AiMotionModel::MotionDiffuse,
                    status: AiRequestStatus::Pending,
                });
            }
            Action::UpdateAiInterpStatus { request_id, status } => {
                if let Some(r) = self.ai_interpolation_requests.iter_mut().find(|r| r.id == request_id) {
                    r.status = status;
                }
            }
            Action::RequestStyleTransfer { source, layer_id, strength } => {
                let id = self.next_style_id;
                self.next_style_id += 1;
                self.ai_style_transfers.push(AiStyleTransfer {
                    id,
                    source_clip_path: source,
                    target_layer_id: layer_id,
                    style_strength: strength.clamp(0.0, 1.0),
                    preserve_timing: true,
                    status: AiRequestStatus::Pending,
                });
            }
            Action::UpdateStyleTransferStatus { request_id, status } => {
                if let Some(r) = self.ai_style_transfers.iter_mut().find(|r| r.id == request_id) {
                    r.status = status;
                }
            }
            Action::SetAiModelPath(path) => {
                self.ai_model_path = path;
            }
            Action::SetAiComputeBackend(backend) => {
                self.ai_compute_backend = backend;
            }
            _ => {}
        }
    }

    /// Pending or running motion requests.
    pub fn active_ai_motion_requests(&self) -> Vec<&AiMotionRequest> {
        self.ai_motion_requests.iter().filter(|r| {
            matches!(r.status, AiRequestStatus::Pending | AiRequestStatus::Running { .. })
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{AiMotionModel, AiRequestStatus, AiBackend};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_request_ai_motion() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "walk forward".to_string(),
            model: AiMotionModel::MotionDiffuse,
            layer_ids: vec![0],
            frames: 60,
        });
        assert_eq!(a.ai_motion_requests.len(), 1);
        assert_eq!(a.ai_motion_requests[0].prompt, "walk forward");
        assert_eq!(a.ai_motion_requests[0].status, AiRequestStatus::Pending);
    }

    #[test]
    fn test_request_ai_motion_unique_ids() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "A".to_string(), model: AiMotionModel::CharacterNet, layer_ids: vec![], frames: 30,
        });
        a.apply(Action::RequestAiMotion {
            prompt: "B".to_string(), model: AiMotionModel::CharacterNet, layer_ids: vec![], frames: 30,
        });
        assert_ne!(a.ai_motion_requests[0].id, a.ai_motion_requests[1].id);
    }

    #[test]
    fn test_update_ai_motion_status_running() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "run".to_string(), model: AiMotionModel::MotionDiffuse, layer_ids: vec![1], frames: 30,
        });
        let id = a.ai_motion_requests[0].id;
        a.apply(Action::UpdateAiMotionStatus { request_id: id, status: AiRequestStatus::Running { progress: 0.5 } });
        assert_eq!(a.ai_motion_requests[0].status, AiRequestStatus::Running { progress: 0.5 });
    }

    #[test]
    fn test_update_ai_motion_status_done() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "jump".to_string(), model: AiMotionModel::MotionDiffuse, layer_ids: vec![], frames: 20,
        });
        let id = a.ai_motion_requests[0].id;
        a.apply(Action::UpdateAiMotionStatus { request_id: id, status: AiRequestStatus::Done });
        assert_eq!(a.ai_motion_requests[0].status, AiRequestStatus::Done);
    }

    #[test]
    fn test_cancel_ai_motion() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "spin".to_string(), model: AiMotionModel::MotionDiffuse, layer_ids: vec![], frames: 30,
        });
        let id = a.ai_motion_requests[0].id;
        a.apply(Action::CancelAiMotion { request_id: id });
        assert!(matches!(a.ai_motion_requests[0].status, AiRequestStatus::Failed(_)));
    }

    #[test]
    fn test_request_ai_interpolation() {
        let mut a = app();
        a.apply(Action::RequestAiInterpolation2 {
            layer_id: 0, from_frame: 0, to_frame: 30, inbetweens: 5,
        });
        assert_eq!(a.ai_interpolation_requests.len(), 1);
        assert_eq!(a.ai_interpolation_requests[0].num_inbetweens, 5);
        assert_eq!(a.ai_interpolation_requests[0].status, AiRequestStatus::Pending);
    }

    #[test]
    fn test_update_ai_interp_status() {
        let mut a = app();
        a.apply(Action::RequestAiInterpolation2 {
            layer_id: 1, from_frame: 10, to_frame: 20, inbetweens: 3,
        });
        let id = a.ai_interpolation_requests[0].id;
        a.apply(Action::UpdateAiInterpStatus { request_id: id, status: AiRequestStatus::Done });
        assert_eq!(a.ai_interpolation_requests[0].status, AiRequestStatus::Done);
    }

    #[test]
    fn test_request_style_transfer_clamps_strength() {
        let mut a = app();
        a.apply(Action::RequestStyleTransfer {
            source: "/clips/ref.mp4".to_string(), layer_id: 0, strength: 1.5,
        });
        assert_eq!(a.ai_style_transfers[0].style_strength, 1.0, "clamped to 1.0");
    }

    #[test]
    fn test_update_style_transfer_status() {
        let mut a = app();
        a.apply(Action::RequestStyleTransfer {
            source: "/ref.mp4".to_string(), layer_id: 2, strength: 0.7,
        });
        let id = a.ai_style_transfers[0].id;
        a.apply(Action::UpdateStyleTransferStatus { request_id: id, status: AiRequestStatus::Running { progress: 0.3 } });
        assert_eq!(a.ai_style_transfers[0].status, AiRequestStatus::Running { progress: 0.3 });
    }

    #[test]
    fn test_set_ai_model_path() {
        let mut a = app();
        a.apply(Action::SetAiModelPath(Some("/models/motion_diffuse.onnx".to_string())));
        assert_eq!(a.ai_model_path, Some("/models/motion_diffuse.onnx".to_string()));
        a.apply(Action::SetAiModelPath(None));
        assert!(a.ai_model_path.is_none());
    }

    #[test]
    fn test_set_ai_compute_backend() {
        let mut a = app();
        a.apply(Action::SetAiComputeBackend(AiBackend::Metal));
        assert_eq!(a.ai_compute_backend, AiBackend::Metal);
    }

    #[test]
    fn test_active_ai_motion_requests_filter() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "A".to_string(), model: AiMotionModel::MotionDiffuse, layer_ids: vec![], frames: 30,
        });
        a.apply(Action::RequestAiMotion {
            prompt: "B".to_string(), model: AiMotionModel::MotionDiffuse, layer_ids: vec![], frames: 30,
        });
        let id_b = a.ai_motion_requests[1].id;
        a.apply(Action::UpdateAiMotionStatus { request_id: id_b, status: AiRequestStatus::Done });
        let active = a.active_ai_motion_requests();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].prompt, "A");
    }

    #[test]
    fn test_custom_onnx_model() {
        let mut a = app();
        a.apply(Action::RequestAiMotion {
            prompt: "custom".to_string(),
            model: AiMotionModel::CustomOnnx { path: "/models/custom.onnx".to_string() },
            layer_ids: vec![],
            frames: 24,
        });
        assert_eq!(
            a.ai_motion_requests[0].model,
            AiMotionModel::CustomOnnx { path: "/models/custom.onnx".to_string() },
        );
    }
}
