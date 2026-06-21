use super::{App, Action, EasingKind, Keyframe};

impl App {
    pub fn apply_ai(&mut self, action: Action) {
        match action {
            Action::SetAiMotionPrompt(prompt) => {
                self.ai_motion_prompt = prompt;
            }
            Action::GenerateAiMotion { layer_id } => {
                let desc = format!(
                    "AI motion for layer {layer_id}: \"{}\"",
                    self.ai_motion_prompt
                );
                self.ai_motion_results.push((layer_id, desc));
                // Stub: generate 6 keyframes on position_x at frames 0/5/10/15/20/25
                let positions = [0.0_f32, 50.0, 100.0, 50.0, 0.0, -50.0];
                for (i, &v) in positions.iter().enumerate() {
                    let frame = i * 5;
                    let id = self.keyframe_counter;
                    self.keyframe_counter += 1;
                    self.keyframes.push(Keyframe {
                        id,
                        layer_id,
                        property: "position_x".to_string(),
                        frame,
                        value: v,
                        easing: EasingKind::EaseInOut,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }
            Action::StartAiLipSync { layer_id, audio_path } => {
                self.ai_lipsync_jobs.push((layer_id, audio_path));
            }
            Action::CompleteAiLipSync { layer_id } => {
                self.ai_lipsync_jobs.retain(|(id, _)| *id != layer_id);
                // Stub: 12 mouth_open keyframes
                for i in 0..12_usize {
                    let id = self.keyframe_counter;
                    self.keyframe_counter += 1;
                    let value = if i % 2 == 0 { 0.0 } else { 1.0 };
                    self.keyframes.push(Keyframe {
                        id,
                        layer_id,
                        property: "mouth_open".to_string(),
                        frame: i * 2,
                        value,
                        easing: EasingKind::EaseInOut,
                        bezier_handle_in: (0.0, 0.0),
                        bezier_handle_out: (1.0, 1.0),
                    });
                }
            }
            Action::RequestAiInterpolation { layer_id, from_frame, to_frame } => {
                self.ai_interpolation_queue.push((layer_id, from_frame, to_frame));
            }
            Action::CompleteAiInterpolation { layer_id } => {
                if let Some(pos) =
                    self.ai_interpolation_queue.iter().position(|(id, _, _)| *id == layer_id)
                {
                    self.ai_interpolation_queue.remove(pos);
                }
            }
            Action::AutoRigWithAi(layer_id) => {
                self.auto_rig_layer(layer_id);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::LayerKind;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_set_ai_motion_prompt() {
        let mut a = app();
        a.apply(Action::SetAiMotionPrompt("walk cycle".to_string()));
        assert_eq!(a.ai_motion_prompt, "walk cycle");
    }

    #[test]
    fn test_generate_ai_motion_adds_result() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::SetAiMotionPrompt("bounce".to_string()));
        a.apply(Action::GenerateAiMotion { layer_id: lid });
        assert_eq!(a.ai_motion_results.len(), 1);
        assert_eq!(a.ai_motion_results[0].0, lid);
    }

    #[test]
    fn test_generate_ai_motion_creates_keyframes() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::GenerateAiMotion { layer_id: lid });
        // 6 keyframes on position_x
        let kfs: Vec<_> = a.keyframes.iter().filter(|k| k.property == "position_x").collect();
        assert_eq!(kfs.len(), 6);
        assert_eq!(kfs[0].frame, 0);
        assert_eq!(kfs[1].frame, 5);
    }

    #[test]
    fn test_start_ai_lipsync() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::StartAiLipSync { layer_id: lid, audio_path: "/tmp/voice.wav".to_string() });
        assert_eq!(a.ai_lipsync_jobs.len(), 1);
        assert_eq!(a.ai_lipsync_jobs[0].1, "/tmp/voice.wav");
    }

    #[test]
    fn test_complete_ai_lipsync_removes_job_and_adds_keyframes() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Bitmap });
        let lid = a.layers[0].id;
        a.apply(Action::StartAiLipSync { layer_id: lid, audio_path: "/tmp/voice.wav".to_string() });
        a.apply(Action::CompleteAiLipSync { layer_id: lid });
        assert!(a.ai_lipsync_jobs.is_empty());
        let mouth_kfs: Vec<_> =
            a.keyframes.iter().filter(|k| k.property == "mouth_open").collect();
        assert_eq!(mouth_kfs.len(), 12);
    }

    #[test]
    fn test_request_ai_interpolation() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::RequestAiInterpolation { layer_id: lid, from_frame: 0, to_frame: 24 });
        assert_eq!(a.ai_interpolation_queue.len(), 1);
        assert_eq!(a.ai_interpolation_queue[0], (lid, 0, 24));
    }

    #[test]
    fn test_complete_ai_interpolation_removes_from_queue() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "A".to_string(), kind: LayerKind::Vector });
        let lid = a.layers[0].id;
        a.apply(Action::RequestAiInterpolation { layer_id: lid, from_frame: 0, to_frame: 24 });
        a.apply(Action::CompleteAiInterpolation { layer_id: lid });
        assert!(a.ai_interpolation_queue.is_empty());
    }

    #[test]
    fn test_ai_motion_prompt_default_empty() {
        let a = app();
        assert!(a.ai_motion_prompt.is_empty());
    }
}
