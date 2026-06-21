use super::{App, Action};

impl App {
    pub fn apply_playback(&mut self, action: Action) {
        match action {
            Action::Play => {
                self.playing = true;
            }
            Action::Pause => {
                self.playing = false;
            }
            Action::Stop => {
                self.playing = false;
                self.current_frame = self.in_point;
            }
            Action::SetCurrentFrame(frame) => {
                self.current_frame = frame.min(self.document.duration_frames);
            }
            Action::ToggleLoop => {
                self.loop_playback = !self.loop_playback;
            }
            Action::SetInPoint(frame) => {
                self.in_point = frame.min(self.out_point);
            }
            Action::SetOutPoint(frame) => {
                self.out_point = frame.max(self.in_point);
            }
            Action::StepForward => {
                if self.current_frame < self.document.duration_frames {
                    self.current_frame += 1;
                }
            }
            Action::StepBackward => {
                if self.current_frame > 0 {
                    self.current_frame -= 1;
                }
            }
            Action::GoToFirstFrame => {
                self.current_frame = 0;
            }
            Action::GoToLastFrame => {
                self.current_frame = self.document.duration_frames;
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
    fn test_play_pause() {
        let mut a = app();
        a.apply(Action::Play);
        assert!(a.playing);
        a.apply(Action::Pause);
        assert!(!a.playing);
    }

    #[test]
    fn test_stop_resets_frame() {
        let mut a = app();
        a.apply(Action::SetInPoint(10));
        a.apply(Action::SetCurrentFrame(50));
        a.apply(Action::Play);
        a.apply(Action::Stop);
        assert!(!a.playing);
        assert_eq!(a.current_frame, 10);
    }

    #[test]
    fn test_set_current_frame() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(100));
        assert_eq!(a.current_frame, 100);
    }

    #[test]
    fn test_set_current_frame_clamp() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(99999));
        assert_eq!(a.current_frame, a.document.duration_frames);
    }

    #[test]
    fn test_toggle_loop() {
        let mut a = app();
        assert!(!a.loop_playback);
        a.apply(Action::ToggleLoop);
        assert!(a.loop_playback);
        a.apply(Action::ToggleLoop);
        assert!(!a.loop_playback);
    }

    #[test]
    fn test_set_in_out_point() {
        let mut a = app();
        a.apply(Action::SetInPoint(10));
        a.apply(Action::SetOutPoint(100));
        assert_eq!(a.in_point, 10);
        assert_eq!(a.out_point, 100);
    }

    #[test]
    fn test_step_forward() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(5));
        a.apply(Action::StepForward);
        assert_eq!(a.current_frame, 6);
    }

    #[test]
    fn test_step_backward() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(5));
        a.apply(Action::StepBackward);
        assert_eq!(a.current_frame, 4);
    }

    #[test]
    fn test_step_backward_at_zero() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(0));
        a.apply(Action::StepBackward);
        assert_eq!(a.current_frame, 0);
    }

    #[test]
    fn test_go_to_first_frame() {
        let mut a = app();
        a.apply(Action::SetCurrentFrame(100));
        a.apply(Action::GoToFirstFrame);
        assert_eq!(a.current_frame, 0);
    }

    #[test]
    fn test_go_to_last_frame() {
        let mut a = app();
        a.apply(Action::GoToLastFrame);
        assert_eq!(a.current_frame, a.document.duration_frames);
    }

    #[test]
    fn test_in_point_clamped_to_out_point() {
        let mut a = app();
        a.apply(Action::SetOutPoint(50));
        a.apply(Action::SetInPoint(200));
        assert!(a.in_point <= a.out_point);
    }
}
