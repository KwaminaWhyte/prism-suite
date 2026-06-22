use super::{App, Action};

// ── Status types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum AiExtJobStatus {
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DftExportStatus {
    Idle,
    Packing,
    Done,
    Error,
}

// ── Job types ─────────────────────────────────────────────────────────────────

/// Motion path smoothing job — applies AI curve smoothing to a layer property.
#[derive(Clone, Debug)]
pub struct MotionSmoothJob {
    pub id: usize,
    pub layer_id: usize,
    pub property: String,
    pub strength: f32,
    pub status: AiExtJobStatus,
}

/// Single easing suggestion for a layer property.
#[derive(Clone, Debug)]
pub struct EaseSuggestion {
    pub property: String,
    pub suggested_easing: String,
    pub confidence: f32,
}

/// AI ease suggestion job — returns a list of property easing suggestions.
#[derive(Clone, Debug)]
pub struct EaseSuggestJob {
    pub id: usize,
    pub layer_id: usize,
    pub suggestions: Vec<EaseSuggestion>,
    pub status: AiExtJobStatus,
}

/// In-betweening job — fills frames between two pose extremes.
#[derive(Clone, Debug)]
pub struct InbetweenJob {
    pub id: usize,
    pub layer_id: usize,
    pub from_frame: usize,
    pub to_frame: usize,
    pub frames_to_fill: usize,
    pub status: AiExtJobStatus,
}

/// Character expression transfer — maps a reference image's expression onto a rig.
#[derive(Clone, Debug)]
pub struct ExpressionTransferJob {
    pub id: usize,
    pub rig_layer_id: usize,
    pub reference_image_path: String,
    pub status: AiExtJobStatus,
}

/// Motion data extraction from video (MediaPipe stub).
#[derive(Clone, Debug)]
pub struct MotionFromVideoJob {
    pub id: usize,
    pub video_path: String,
    pub target_rig_layer_id: usize,
    pub fps: f32,
    pub status: AiExtJobStatus,
    pub keyframes_created: usize,
}

/// Character pack export into a `.dft` bundle.
#[derive(Clone, Debug)]
pub struct CharacterPackExport {
    pub id: usize,
    pub rig_layer_id: usize,
    pub output_path: String,
    pub include_audio: bool,
    pub status: DftExportStatus,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_ai_motion_ext(&mut self, action: &Action) {
        match action {
            Action::QueueMotionSmooth { layer_id, property, strength } => {
                let id = self.next_ms_job_id;
                self.next_ms_job_id += 1;
                self.motion_smooth_jobs.push(MotionSmoothJob {
                    id,
                    layer_id: *layer_id,
                    property: property.clone(),
                    strength: *strength,
                    status: AiExtJobStatus::Queued,
                });
            }
            Action::CompleteMotionSmooth { job_id } => {
                if let Some(j) = self.motion_smooth_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = AiExtJobStatus::Done;
                }
            }
            Action::CancelMotionSmooth { job_id } => {
                self.motion_smooth_jobs.retain(|j| j.id != *job_id);
            }

            Action::QueueEaseSuggest { layer_id } => {
                let id = self.next_ease_job_id;
                self.next_ease_job_id += 1;
                self.ease_suggest_jobs.push(EaseSuggestJob {
                    id,
                    layer_id: *layer_id,
                    suggestions: vec![],
                    status: AiExtJobStatus::Queued,
                });
            }
            Action::CompleteEaseSuggest { job_id, suggestions } => {
                if let Some(j) = self.ease_suggest_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = AiExtJobStatus::Done;
                    j.suggestions = suggestions.clone();
                }
            }

            Action::QueueInbetween { layer_id, from_frame, to_frame, frames_to_fill } => {
                let id = self.next_inbetween_id;
                self.next_inbetween_id += 1;
                self.inbetween_jobs.push(InbetweenJob {
                    id,
                    layer_id: *layer_id,
                    from_frame: *from_frame,
                    to_frame: *to_frame,
                    frames_to_fill: *frames_to_fill,
                    status: AiExtJobStatus::Queued,
                });
            }
            Action::CompleteInbetween { job_id } => {
                if let Some(j) = self.inbetween_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = AiExtJobStatus::Done;
                }
            }
            Action::CancelInbetween { job_id } => {
                self.inbetween_jobs.retain(|j| j.id != *job_id);
            }

            Action::QueueExpressionTransfer { rig_layer_id, reference_image_path } => {
                let id = self.next_expr_transfer_id;
                self.next_expr_transfer_id += 1;
                self.expr_transfer_jobs.push(ExpressionTransferJob {
                    id,
                    rig_layer_id: *rig_layer_id,
                    reference_image_path: reference_image_path.clone(),
                    status: AiExtJobStatus::Queued,
                });
            }
            Action::CompleteExpressionTransfer { job_id } => {
                if let Some(j) = self.expr_transfer_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = AiExtJobStatus::Done;
                }
            }

            Action::QueueMotionFromVideo { video_path, target_rig_layer_id, fps } => {
                let id = self.next_mfv_id;
                self.next_mfv_id += 1;
                self.mfv_jobs.push(MotionFromVideoJob {
                    id,
                    video_path: video_path.clone(),
                    target_rig_layer_id: *target_rig_layer_id,
                    fps: *fps,
                    status: AiExtJobStatus::Queued,
                    keyframes_created: 0,
                });
            }
            Action::UpdateMotionFromVideoProgress { job_id, keyframes_created } => {
                if let Some(j) = self.mfv_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.keyframes_created = *keyframes_created;
                }
            }
            Action::CompleteMotionFromVideo { job_id, keyframes_created } => {
                if let Some(j) = self.mfv_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = AiExtJobStatus::Done;
                    j.keyframes_created = *keyframes_created;
                }
            }

            Action::StartCharPackExport { rig_layer_id, output_path, include_audio } => {
                let id = self.next_char_pack_id;
                self.next_char_pack_id += 1;
                self.char_pack_exports.push(CharacterPackExport {
                    id,
                    rig_layer_id: *rig_layer_id,
                    output_path: output_path.clone(),
                    include_audio: *include_audio,
                    status: DftExportStatus::Packing,
                });
            }
            Action::CompleteCharPackExport { export_id } => {
                if let Some(e) = self.char_pack_exports.iter_mut().find(|e| e.id == *export_id) {
                    e.status = DftExportStatus::Done;
                }
            }
            Action::CancelCharPackExport { export_id } => {
                self.char_pack_exports.retain(|e| e.id != *export_id);
            }

            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{AiExtJobStatus, DftExportStatus, EaseSuggestion};

    fn app() -> App {
        App::new()
    }

    // ── MotionSmooth ──────────────────────────────────────────────────────────

    #[test]
    fn test_queue_motion_smooth() {
        let mut a = app();
        a.apply(Action::QueueMotionSmooth {
            layer_id: 1,
            property: "position_x".to_string(),
            strength: 0.8,
        });
        assert_eq!(a.motion_smooth_jobs.len(), 1);
        assert_eq!(a.motion_smooth_jobs[0].layer_id, 1);
        assert_eq!(a.motion_smooth_jobs[0].status, AiExtJobStatus::Queued);
    }

    #[test]
    fn test_complete_motion_smooth() {
        let mut a = app();
        a.apply(Action::QueueMotionSmooth {
            layer_id: 2,
            property: "rotation".to_string(),
            strength: 0.5,
        });
        let id = a.motion_smooth_jobs[0].id;
        a.apply(Action::CompleteMotionSmooth { job_id: id });
        assert_eq!(a.motion_smooth_jobs[0].status, AiExtJobStatus::Done);
    }

    #[test]
    fn test_cancel_motion_smooth() {
        let mut a = app();
        a.apply(Action::QueueMotionSmooth {
            layer_id: 3,
            property: "opacity".to_string(),
            strength: 0.3,
        });
        let id = a.motion_smooth_jobs[0].id;
        a.apply(Action::CancelMotionSmooth { job_id: id });
        assert!(a.motion_smooth_jobs.is_empty());
    }

    #[test]
    fn test_motion_smooth_unique_ids() {
        let mut a = app();
        a.apply(Action::QueueMotionSmooth {
            layer_id: 1,
            property: "x".to_string(),
            strength: 0.5,
        });
        a.apply(Action::QueueMotionSmooth {
            layer_id: 2,
            property: "y".to_string(),
            strength: 0.5,
        });
        assert_ne!(a.motion_smooth_jobs[0].id, a.motion_smooth_jobs[1].id);
    }

    // ── EaseSuggest ───────────────────────────────────────────────────────────

    #[test]
    fn test_queue_ease_suggest() {
        let mut a = app();
        a.apply(Action::QueueEaseSuggest { layer_id: 5 });
        assert_eq!(a.ease_suggest_jobs.len(), 1);
        assert_eq!(a.ease_suggest_jobs[0].layer_id, 5);
        assert_eq!(a.ease_suggest_jobs[0].status, AiExtJobStatus::Queued);
        assert!(a.ease_suggest_jobs[0].suggestions.is_empty());
    }

    #[test]
    fn test_complete_ease_suggest_with_suggestions() {
        let mut a = app();
        a.apply(Action::QueueEaseSuggest { layer_id: 6 });
        let id = a.ease_suggest_jobs[0].id;
        let suggestions = vec![
            EaseSuggestion {
                property: "position_x".to_string(),
                suggested_easing: "ease_in_out".to_string(),
                confidence: 0.92,
            },
        ];
        a.apply(Action::CompleteEaseSuggest { job_id: id, suggestions });
        assert_eq!(a.ease_suggest_jobs[0].status, AiExtJobStatus::Done);
        assert_eq!(a.ease_suggest_jobs[0].suggestions.len(), 1);
        assert_eq!(a.ease_suggest_jobs[0].suggestions[0].property, "position_x");
    }

    // ── Inbetween ─────────────────────────────────────────────────────────────

    #[test]
    fn test_queue_inbetween() {
        let mut a = app();
        a.apply(Action::QueueInbetween {
            layer_id: 10,
            from_frame: 0,
            to_frame: 30,
            frames_to_fill: 5,
        });
        assert_eq!(a.inbetween_jobs.len(), 1);
        assert_eq!(a.inbetween_jobs[0].frames_to_fill, 5);
        assert_eq!(a.inbetween_jobs[0].status, AiExtJobStatus::Queued);
    }

    #[test]
    fn test_complete_inbetween() {
        let mut a = app();
        a.apply(Action::QueueInbetween {
            layer_id: 11,
            from_frame: 0,
            to_frame: 10,
            frames_to_fill: 3,
        });
        let id = a.inbetween_jobs[0].id;
        a.apply(Action::CompleteInbetween { job_id: id });
        assert_eq!(a.inbetween_jobs[0].status, AiExtJobStatus::Done);
    }

    #[test]
    fn test_cancel_inbetween() {
        let mut a = app();
        a.apply(Action::QueueInbetween {
            layer_id: 12,
            from_frame: 0,
            to_frame: 20,
            frames_to_fill: 4,
        });
        let id = a.inbetween_jobs[0].id;
        a.apply(Action::CancelInbetween { job_id: id });
        assert!(a.inbetween_jobs.is_empty());
    }

    // ── ExpressionTransfer ────────────────────────────────────────────────────

    #[test]
    fn test_queue_expression_transfer() {
        let mut a = app();
        a.apply(Action::QueueExpressionTransfer {
            rig_layer_id: 20,
            reference_image_path: "/refs/smile.png".to_string(),
        });
        assert_eq!(a.expr_transfer_jobs.len(), 1);
        assert_eq!(a.expr_transfer_jobs[0].rig_layer_id, 20);
        assert_eq!(a.expr_transfer_jobs[0].status, AiExtJobStatus::Queued);
    }

    #[test]
    fn test_complete_expression_transfer() {
        let mut a = app();
        a.apply(Action::QueueExpressionTransfer {
            rig_layer_id: 21,
            reference_image_path: "/refs/frown.png".to_string(),
        });
        let id = a.expr_transfer_jobs[0].id;
        a.apply(Action::CompleteExpressionTransfer { job_id: id });
        assert_eq!(a.expr_transfer_jobs[0].status, AiExtJobStatus::Done);
    }

    // ── MotionFromVideo ───────────────────────────────────────────────────────

    #[test]
    fn test_queue_motion_from_video() {
        let mut a = app();
        a.apply(Action::QueueMotionFromVideo {
            video_path: "/videos/reference.mp4".to_string(),
            target_rig_layer_id: 30,
            fps: 24.0,
        });
        assert_eq!(a.mfv_jobs.len(), 1);
        assert_eq!(a.mfv_jobs[0].keyframes_created, 0);
        assert_eq!(a.mfv_jobs[0].status, AiExtJobStatus::Queued);
    }

    #[test]
    fn test_update_motion_from_video_progress() {
        let mut a = app();
        a.apply(Action::QueueMotionFromVideo {
            video_path: "/videos/walk.mp4".to_string(),
            target_rig_layer_id: 31,
            fps: 30.0,
        });
        let id = a.mfv_jobs[0].id;
        a.apply(Action::UpdateMotionFromVideoProgress { job_id: id, keyframes_created: 120 });
        assert_eq!(a.mfv_jobs[0].keyframes_created, 120);
    }

    #[test]
    fn test_complete_motion_from_video() {
        let mut a = app();
        a.apply(Action::QueueMotionFromVideo {
            video_path: "/videos/run.mp4".to_string(),
            target_rig_layer_id: 32,
            fps: 60.0,
        });
        let id = a.mfv_jobs[0].id;
        a.apply(Action::CompleteMotionFromVideo { job_id: id, keyframes_created: 240 });
        assert_eq!(a.mfv_jobs[0].status, AiExtJobStatus::Done);
        assert_eq!(a.mfv_jobs[0].keyframes_created, 240);
    }

    // ── CharPackExport ────────────────────────────────────────────────────────

    #[test]
    fn test_start_char_pack_export_starts_packing() {
        let mut a = app();
        a.apply(Action::StartCharPackExport {
            rig_layer_id: 40,
            output_path: "/exports/hero.dft".to_string(),
            include_audio: true,
        });
        assert_eq!(a.char_pack_exports.len(), 1);
        assert_eq!(a.char_pack_exports[0].status, DftExportStatus::Packing);
        assert!(a.char_pack_exports[0].include_audio);
    }

    #[test]
    fn test_complete_char_pack_export() {
        let mut a = app();
        a.apply(Action::StartCharPackExport {
            rig_layer_id: 41,
            output_path: "/exports/villain.dft".to_string(),
            include_audio: false,
        });
        let id = a.char_pack_exports[0].id;
        a.apply(Action::CompleteCharPackExport { export_id: id });
        assert_eq!(a.char_pack_exports[0].status, DftExportStatus::Done);
    }

    #[test]
    fn test_cancel_char_pack_export() {
        let mut a = app();
        a.apply(Action::StartCharPackExport {
            rig_layer_id: 42,
            output_path: "/exports/npc.dft".to_string(),
            include_audio: false,
        });
        let id = a.char_pack_exports[0].id;
        a.apply(Action::CancelCharPackExport { export_id: id });
        assert!(a.char_pack_exports.is_empty());
    }
}
