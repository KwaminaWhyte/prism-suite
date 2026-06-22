use super::{App, Action};

/// The stage camera used for viewport control and animatable pan/zoom/rotation.
#[derive(Clone, Debug)]
pub struct DriftCamera {
    pub x: f32,
    pub y: f32,
    /// Zoom level: 1.0 = 100%, clamped to 0.1..=10.0.
    pub zoom: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// When true the camera position/zoom/rotation can be keyframed.
    pub is_animatable: bool,
}

impl DriftCamera {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
            is_animatable: false,
        }
    }
}

/// A single keyframe on the camera track.
#[derive(Clone, Debug)]
pub struct CameraKeyframe {
    pub id: usize,
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub rotation: f32,
    /// Id of an easing curve from the easing library (0 = linear fallback).
    pub easing: usize,
}

impl App {
    /// Interpolate camera (x, y, zoom, rotation) at the given frame.
    /// Uses linear interpolation between adjacent keyframes.
    /// Returns the raw camera values when no keyframes exist.
    pub fn camera_at_frame(&self, frame: u32) -> (f32, f32, f32, f32) {
        if self.camera_keyframes.is_empty() {
            return (self.camera.x, self.camera.y, self.camera.zoom, self.camera.rotation);
        }

        let mut sorted = self.camera_keyframes.clone();
        sorted.sort_by_key(|k| k.frame);

        // Before the first keyframe.
        if frame <= sorted[0].frame {
            let k = &sorted[0];
            return (k.x, k.y, k.zoom, k.rotation);
        }

        // After the last keyframe.
        let last = &sorted[sorted.len() - 1];
        if frame >= last.frame {
            return (last.x, last.y, last.zoom, last.rotation);
        }

        // Find surrounding pair.
        for w in sorted.windows(2) {
            let a = &w[0];
            let b = &w[1];
            if frame >= a.frame && frame <= b.frame {
                let span = (b.frame - a.frame) as f32;
                let t = if span > 0.0 {
                    (frame - a.frame) as f32 / span
                } else {
                    0.0
                };
                return (
                    a.x + (b.x - a.x) * t,
                    a.y + (b.y - a.y) * t,
                    a.zoom + (b.zoom - a.zoom) * t,
                    a.rotation + (b.rotation - a.rotation) * t,
                );
            }
        }

        (self.camera.x, self.camera.y, self.camera.zoom, self.camera.rotation)
    }

    pub fn apply_camera(&mut self, action: Action) {
        match action {
            Action::SetCameraPosition { x, y } => {
                self.camera.x = x;
                self.camera.y = y;
            }
            Action::SetCameraZoom(z) => {
                self.camera.zoom = z.clamp(0.1, 10.0);
            }
            Action::SetCameraRotation(r) => {
                self.camera.rotation = r;
            }
            Action::ResetCamera => {
                self.camera = DriftCamera::new();
            }
            Action::ToggleCameraEnabled => {
                self.camera_enabled = !self.camera_enabled;
            }
            Action::AddCameraKeyframe { frame, x, y, zoom, rotation } => {
                let id = self.next_camera_kf_id;
                self.next_camera_kf_id += 1;
                // Remove existing keyframe at the same frame if present.
                self.camera_keyframes.retain(|k| k.frame != frame);
                self.camera_keyframes.push(CameraKeyframe {
                    id,
                    frame,
                    x,
                    y,
                    zoom: zoom.clamp(0.1, 10.0),
                    rotation,
                    easing: 0,
                });
            }
            Action::RemoveCameraKeyframe { kf_id } => {
                self.camera_keyframes.retain(|k| k.id != kf_id);
            }
            Action::SetCameraAnimatable(v) => {
                self.camera.is_animatable = v;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_camera_defaults() {
        let a = app();
        assert_eq!(a.camera.x, 0.0);
        assert_eq!(a.camera.y, 0.0);
        assert_eq!(a.camera.zoom, 1.0);
        assert_eq!(a.camera.rotation, 0.0);
        assert!(!a.camera.is_animatable);
        assert!(!a.camera_enabled);
    }

    #[test]
    fn test_set_camera_position() {
        let mut a = app();
        a.apply(Action::SetCameraPosition { x: 100.0, y: 200.0 });
        assert_eq!(a.camera.x, 100.0);
        assert_eq!(a.camera.y, 200.0);
    }

    #[test]
    fn test_set_camera_zoom_clamp_high() {
        let mut a = app();
        a.apply(Action::SetCameraZoom(99.0));
        assert_eq!(a.camera.zoom, 10.0);
    }

    #[test]
    fn test_set_camera_zoom_clamp_low() {
        let mut a = app();
        a.apply(Action::SetCameraZoom(0.0));
        assert_eq!(a.camera.zoom, 0.1);
    }

    #[test]
    fn test_set_camera_rotation() {
        let mut a = app();
        a.apply(Action::SetCameraRotation(45.0));
        assert_eq!(a.camera.rotation, 45.0);
    }

    #[test]
    fn test_reset_camera() {
        let mut a = app();
        a.apply(Action::SetCameraPosition { x: 500.0, y: 300.0 });
        a.apply(Action::SetCameraZoom(3.0));
        a.apply(Action::ResetCamera);
        assert_eq!(a.camera.x, 0.0);
        assert_eq!(a.camera.zoom, 1.0);
    }

    #[test]
    fn test_toggle_camera_enabled() {
        let mut a = app();
        a.apply(Action::ToggleCameraEnabled);
        assert!(a.camera_enabled);
        a.apply(Action::ToggleCameraEnabled);
        assert!(!a.camera_enabled);
    }

    #[test]
    fn test_add_camera_keyframe() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 10, x: 50.0, y: 60.0, zoom: 2.0, rotation: 15.0 });
        assert_eq!(a.camera_keyframes.len(), 1);
        assert_eq!(a.camera_keyframes[0].frame, 10);
        assert_eq!(a.camera_keyframes[0].zoom, 2.0);
    }

    #[test]
    fn test_add_camera_keyframe_replaces_same_frame() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 5, x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0 });
        a.apply(Action::AddCameraKeyframe { frame: 5, x: 10.0, y: 10.0, zoom: 1.5, rotation: 5.0 });
        assert_eq!(a.camera_keyframes.len(), 1);
        assert_eq!(a.camera_keyframes[0].x, 10.0);
    }

    #[test]
    fn test_remove_camera_keyframe() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 10, x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0 });
        let kf_id = a.camera_keyframes[0].id;
        a.apply(Action::RemoveCameraKeyframe { kf_id });
        assert!(a.camera_keyframes.is_empty());
    }

    #[test]
    fn test_set_camera_animatable() {
        let mut a = app();
        a.apply(Action::SetCameraAnimatable(true));
        assert!(a.camera.is_animatable);
    }

    #[test]
    fn test_camera_at_frame_no_keyframes() {
        let mut a = app();
        a.apply(Action::SetCameraPosition { x: 10.0, y: 20.0 });
        let (x, y, zoom, rot) = a.camera_at_frame(0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 20.0);
        assert_eq!(zoom, 1.0);
        assert_eq!(rot, 0.0);
    }

    #[test]
    fn test_camera_at_frame_interpolation() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 0, x: 0.0, y: 0.0, zoom: 1.0, rotation: 0.0 });
        a.apply(Action::AddCameraKeyframe { frame: 10, x: 100.0, y: 0.0, zoom: 2.0, rotation: 0.0 });
        let (x, _y, zoom, _rot) = a.camera_at_frame(5);
        assert!((x - 50.0).abs() < 0.001);
        assert!((zoom - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_camera_at_frame_before_first_keyframe() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 10, x: 50.0, y: 0.0, zoom: 1.0, rotation: 0.0 });
        let (x, _, _, _) = a.camera_at_frame(0);
        assert_eq!(x, 50.0);
    }

    #[test]
    fn test_camera_at_frame_after_last_keyframe() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 10, x: 50.0, y: 0.0, zoom: 1.0, rotation: 0.0 });
        let (x, _, _, _) = a.camera_at_frame(100);
        assert_eq!(x, 50.0);
    }

    #[test]
    fn test_camera_keyframe_zoom_clamped() {
        let mut a = app();
        a.apply(Action::AddCameraKeyframe { frame: 0, x: 0.0, y: 0.0, zoom: 50.0, rotation: 0.0 });
        assert_eq!(a.camera_keyframes[0].zoom, 10.0);
    }
}
