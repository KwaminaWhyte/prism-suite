use super::{App, Action};

/// Source of the facial capture data.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptureSource {
    Webcam { device_name: String },
    VideoFile { path: String },
}

/// A tracked facial feature detected per frame.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackedFeature {
    HeadPosition,
    HeadRotation,
    LeftEyeBlink,
    RightEyeBlink,
    MouthOpen,
    BrowRaise,
    SmileLeft,
    SmileRight,
    JawOpen,
    NoseFrown,
    Custom { name: String },
}

/// An active facial capture session.
#[derive(Clone, Debug)]
pub struct FacialCaptureSession {
    pub id: usize,
    pub name: String,
    pub source: CaptureSource,
    pub target_layer_id: Option<usize>,
    pub is_active: bool,
    pub frame_count: u32,
    pub fps: f32,
    pub tracked_features: Vec<TrackedFeature>,
    pub recording: bool,
}

/// Maps a tracked feature to a rig parameter.
#[derive(Clone, Debug)]
pub struct CaptureMapping {
    pub feature: TrackedFeature,
    /// Parameter name on the target rig (e.g. "mouth_open").
    pub target_param: String,
    pub multiplier: f32,
    pub offset: f32,
    pub clamped: bool,
    pub min_val: f32,
    pub max_val: f32,
}

impl App {
    pub fn apply_facial_capture(&mut self, action: Action) {
        match action {
            Action::AddCaptureSession { name, source } => {
                let id = self.next_capture_id;
                self.next_capture_id += 1;
                self.capture_sessions.push(FacialCaptureSession {
                    id,
                    name,
                    source,
                    target_layer_id: None,
                    is_active: false,
                    frame_count: 0,
                    fps: 30.0,
                    tracked_features: Vec::new(),
                    recording: false,
                });
            }
            Action::RemoveCaptureSession { session_id } => {
                self.capture_sessions.retain(|s| s.id != session_id);
            }
            Action::SetCaptureTarget { session_id, layer_id } => {
                if let Some(s) = self.capture_sessions.iter_mut().find(|s| s.id == session_id) {
                    s.target_layer_id = layer_id;
                }
            }
            Action::ToggleCaptureRecording { session_id } => {
                if let Some(s) = self.capture_sessions.iter_mut().find(|s| s.id == session_id) {
                    s.recording = !s.recording;
                }
            }
            Action::SetCaptureActive { session_id, active } => {
                if let Some(s) = self.capture_sessions.iter_mut().find(|s| s.id == session_id) {
                    s.is_active = active;
                }
            }
            Action::AddCaptureMapping { feature, target_param } => {
                self.capture_mappings.push(CaptureMapping {
                    feature,
                    target_param,
                    multiplier: 1.0,
                    offset: 0.0,
                    clamped: true,
                    min_val: 0.0,
                    max_val: 1.0,
                });
            }
            Action::RemoveCaptureMapping { mapping_index } => {
                if mapping_index < self.capture_mappings.len() {
                    self.capture_mappings.remove(mapping_index);
                }
            }
            Action::SetCaptureMappingMultiplier { index, multiplier } => {
                if let Some(m) = self.capture_mappings.get_mut(index) {
                    m.multiplier = multiplier;
                }
            }
            Action::SetCaptureMappingOffset { index, offset } => {
                if let Some(m) = self.capture_mappings.get_mut(index) {
                    m.offset = offset;
                }
            }
            Action::ToggleLivePreview => {
                self.live_preview_enabled = !self.live_preview_enabled;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{CaptureSource, TrackedFeature};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_capture_session_webcam() {
        let mut a = app();
        a.apply(Action::AddCaptureSession {
            name: "Face Cam".to_string(),
            source: CaptureSource::Webcam { device_name: "FaceTime HD Camera".to_string() },
        });
        assert_eq!(a.capture_sessions.len(), 1);
        assert_eq!(a.capture_sessions[0].name, "Face Cam");
        assert!(!a.capture_sessions[0].is_active);
    }

    #[test]
    fn test_add_multiple_sessions_unique_ids() {
        let mut a = app();
        a.apply(Action::AddCaptureSession {
            name: "A".to_string(),
            source: CaptureSource::Webcam { device_name: "cam0".to_string() },
        });
        a.apply(Action::AddCaptureSession {
            name: "B".to_string(),
            source: CaptureSource::VideoFile { path: "/video.mp4".to_string() },
        });
        assert_ne!(a.capture_sessions[0].id, a.capture_sessions[1].id);
    }

    #[test]
    fn test_remove_capture_session() {
        let mut a = app();
        a.apply(Action::AddCaptureSession {
            name: "S".to_string(),
            source: CaptureSource::Webcam { device_name: "cam".to_string() },
        });
        let id = a.capture_sessions[0].id;
        a.apply(Action::RemoveCaptureSession { session_id: id });
        assert!(a.capture_sessions.is_empty());
    }

    #[test]
    fn test_set_capture_target() {
        let mut a = app();
        a.apply(Action::AddCaptureSession {
            name: "S".to_string(),
            source: CaptureSource::Webcam { device_name: "cam".to_string() },
        });
        let id = a.capture_sessions[0].id;
        a.apply(Action::SetCaptureTarget { session_id: id, layer_id: Some(42) });
        assert_eq!(a.capture_sessions[0].target_layer_id, Some(42));
    }

    #[test]
    fn test_toggle_capture_recording() {
        let mut a = app();
        a.apply(Action::AddCaptureSession {
            name: "S".to_string(),
            source: CaptureSource::Webcam { device_name: "cam".to_string() },
        });
        let id = a.capture_sessions[0].id;
        a.apply(Action::ToggleCaptureRecording { session_id: id });
        assert!(a.capture_sessions[0].recording);
        a.apply(Action::ToggleCaptureRecording { session_id: id });
        assert!(!a.capture_sessions[0].recording);
    }

    #[test]
    fn test_set_capture_active() {
        let mut a = app();
        a.apply(Action::AddCaptureSession {
            name: "S".to_string(),
            source: CaptureSource::Webcam { device_name: "cam".to_string() },
        });
        let id = a.capture_sessions[0].id;
        a.apply(Action::SetCaptureActive { session_id: id, active: true });
        assert!(a.capture_sessions[0].is_active);
        a.apply(Action::SetCaptureActive { session_id: id, active: false });
        assert!(!a.capture_sessions[0].is_active);
    }

    #[test]
    fn test_add_capture_mapping() {
        let mut a = app();
        a.apply(Action::AddCaptureMapping {
            feature: TrackedFeature::MouthOpen,
            target_param: "mouth_open".to_string(),
        });
        assert_eq!(a.capture_mappings.len(), 1);
        assert_eq!(a.capture_mappings[0].multiplier, 1.0);
        assert!(a.capture_mappings[0].clamped);
    }

    #[test]
    fn test_remove_capture_mapping() {
        let mut a = app();
        a.apply(Action::AddCaptureMapping {
            feature: TrackedFeature::LeftEyeBlink,
            target_param: "left_blink".to_string(),
        });
        assert_eq!(a.capture_mappings.len(), 1);
        a.apply(Action::RemoveCaptureMapping { mapping_index: 0 });
        assert!(a.capture_mappings.is_empty());
    }

    #[test]
    fn test_set_capture_mapping_multiplier() {
        let mut a = app();
        a.apply(Action::AddCaptureMapping {
            feature: TrackedFeature::BrowRaise,
            target_param: "brow_raise".to_string(),
        });
        a.apply(Action::SetCaptureMappingMultiplier { index: 0, multiplier: 2.5 });
        assert_eq!(a.capture_mappings[0].multiplier, 2.5);
    }

    #[test]
    fn test_set_capture_mapping_offset() {
        let mut a = app();
        a.apply(Action::AddCaptureMapping {
            feature: TrackedFeature::HeadRotation,
            target_param: "head_rot".to_string(),
        });
        a.apply(Action::SetCaptureMappingOffset { index: 0, offset: -0.1 });
        assert_eq!(a.capture_mappings[0].offset, -0.1);
    }

    #[test]
    fn test_toggle_live_preview() {
        let mut a = app();
        assert!(!a.live_preview_enabled);
        a.apply(Action::ToggleLivePreview);
        assert!(a.live_preview_enabled);
        a.apply(Action::ToggleLivePreview);
        assert!(!a.live_preview_enabled);
    }
}
