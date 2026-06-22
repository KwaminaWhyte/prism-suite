use super::{App, Action};

/// A single keyframe within a nested timeline.
#[derive(Clone, Debug)]
pub struct NestedKeyframe {
    pub frame: usize,
    pub property: String,
    pub value: f32,
}

/// A layer inside a nested (symbol) timeline.
#[derive(Clone, Debug)]
pub struct NestedLayer {
    pub id: usize,
    pub name: String,
    pub visible: bool,
}

/// An independent timeline owned by a symbol (MovieClip).
/// Nested timelines play independently of the main stage timeline.
#[derive(Clone, Debug)]
pub struct NestedTimeline {
    pub id: usize,
    pub symbol_id: usize,
    pub fps: f32,
    pub duration_frames: usize,
    pub layers: Vec<NestedLayer>,
    pub keyframes: Vec<NestedKeyframe>,
    pub loop_playback: bool,
    pub current_frame: usize,
}

impl App {
    pub(super) fn apply_nested_timeline(&mut self, action: Action) {
        match action {
            Action::CreateNestedTimeline { symbol_id, fps, duration_frames } => {
                let id = self.next_nested_timeline_id;
                self.next_nested_timeline_id += 1;
                self.nested_timelines.push(NestedTimeline {
                    id,
                    symbol_id,
                    fps,
                    duration_frames,
                    layers: vec![],
                    keyframes: vec![],
                    loop_playback: true,
                    current_frame: 0,
                });
            }
            Action::AddNestedLayer { timeline_id, name } => {
                if let Some(nt) = self.nested_timelines.iter_mut().find(|t| t.id == timeline_id) {
                    let id = nt.layers.len();
                    nt.layers.push(NestedLayer { id, name, visible: true });
                }
            }
            Action::AddNestedKeyframe { timeline_id, frame, property, value } => {
                if let Some(nt) = self.nested_timelines.iter_mut().find(|t| t.id == timeline_id) {
                    nt.keyframes.push(NestedKeyframe { frame, property, value });
                }
            }
            Action::SetNestedFrame { timeline_id, frame } => {
                if let Some(nt) = self.nested_timelines.iter_mut().find(|t| t.id == timeline_id) {
                    nt.current_frame = frame.min(nt.duration_frames);
                }
            }
            Action::SetNestedLoop { timeline_id, loop_on } => {
                if let Some(nt) = self.nested_timelines.iter_mut().find(|t| t.id == timeline_id) {
                    nt.loop_playback = loop_on;
                }
            }
            Action::DeleteNestedTimeline { id } => {
                self.nested_timelines.retain(|t| t.id != id);
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
    fn test_create_nested_timeline() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        assert_eq!(a.nested_timelines.len(), 1);
        assert_eq!(a.nested_timelines[0].symbol_id, 1);
        assert_eq!(a.nested_timelines[0].fps, 24.0);
        assert_eq!(a.nested_timelines[0].duration_frames, 48);
    }

    #[test]
    fn test_create_nested_timeline_id_increments() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        a.apply(Action::CreateNestedTimeline { symbol_id: 2, fps: 30.0, duration_frames: 90 });
        assert_ne!(a.nested_timelines[0].id, a.nested_timelines[1].id);
    }

    #[test]
    fn test_add_nested_layer() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        let tid = a.nested_timelines[0].id;
        a.apply(Action::AddNestedLayer { timeline_id: tid, name: "Body".to_string() });
        assert_eq!(a.nested_timelines[0].layers.len(), 1);
        assert_eq!(a.nested_timelines[0].layers[0].name, "Body");
        assert!(a.nested_timelines[0].layers[0].visible);
    }

    #[test]
    fn test_add_nested_layer_nonexistent_timeline_noop() {
        let mut a = app();
        a.apply(Action::AddNestedLayer { timeline_id: 999, name: "Ghost".to_string() });
        assert!(a.nested_timelines.is_empty());
    }

    #[test]
    fn test_add_nested_keyframe() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        let tid = a.nested_timelines[0].id;
        a.apply(Action::AddNestedKeyframe {
            timeline_id: tid,
            frame: 12,
            property: "x".to_string(),
            value: 100.0,
        });
        assert_eq!(a.nested_timelines[0].keyframes.len(), 1);
        assert_eq!(a.nested_timelines[0].keyframes[0].frame, 12);
        assert_eq!(a.nested_timelines[0].keyframes[0].property, "x");
        assert_eq!(a.nested_timelines[0].keyframes[0].value, 100.0);
    }

    #[test]
    fn test_set_nested_frame_clamped() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        let tid = a.nested_timelines[0].id;
        // Within range
        a.apply(Action::SetNestedFrame { timeline_id: tid, frame: 20 });
        assert_eq!(a.nested_timelines[0].current_frame, 20);
        // Over duration — should clamp
        a.apply(Action::SetNestedFrame { timeline_id: tid, frame: 100 });
        assert_eq!(a.nested_timelines[0].current_frame, 48);
    }

    #[test]
    fn test_set_nested_loop() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        let tid = a.nested_timelines[0].id;
        assert!(a.nested_timelines[0].loop_playback);
        a.apply(Action::SetNestedLoop { timeline_id: tid, loop_on: false });
        assert!(!a.nested_timelines[0].loop_playback);
    }

    #[test]
    fn test_delete_nested_timeline() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        let tid = a.nested_timelines[0].id;
        a.apply(Action::DeleteNestedTimeline { id: tid });
        assert!(a.nested_timelines.is_empty());
    }

    #[test]
    fn test_delete_nested_timeline_only_removes_target() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 1, fps: 24.0, duration_frames: 48 });
        a.apply(Action::CreateNestedTimeline { symbol_id: 2, fps: 30.0, duration_frames: 90 });
        let tid0 = a.nested_timelines[0].id;
        a.apply(Action::DeleteNestedTimeline { id: tid0 });
        assert_eq!(a.nested_timelines.len(), 1);
        assert_eq!(a.nested_timelines[0].symbol_id, 2);
    }

    #[test]
    fn test_nested_timeline_default_state() {
        let mut a = app();
        a.apply(Action::CreateNestedTimeline { symbol_id: 5, fps: 12.0, duration_frames: 24 });
        let nt = &a.nested_timelines[0];
        assert_eq!(nt.current_frame, 0);
        assert!(nt.loop_playback);
        assert!(nt.layers.is_empty());
        assert!(nt.keyframes.is_empty());
    }
}
