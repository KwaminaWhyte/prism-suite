//! Transport domain — transport state + apply methods + tests.

use super::{Action, App};

impl App {
    pub(super) fn apply_transport(&mut self, action: Action) {
        match action {
            Action::Play => {
                self.playing = true;
                self.recording = false;
            }
            Action::Stop => {
                self.playing = false;
                self.recording = false;
                self.playhead_beat = if self.loop_enabled { self.loop_start } else { 0.0 };
            }
            Action::Pause => {
                self.playing = false;
            }
            Action::Record => {
                self.playing = true;
                self.recording = true;
            }
            Action::SetPlayheadBeat(beat) => {
                self.playhead_beat = beat.max(0.0);
            }
            Action::ToggleLoop => {
                self.loop_enabled = !self.loop_enabled;
            }
            Action::SetLoopRange { start, end } => {
                if end > start {
                    self.loop_start = start.max(0.0);
                    self.loop_end = end;
                }
            }
            Action::ToggleMetronome => {
                self.metronome_enabled = !self.metronome_enabled;
            }
            Action::Rewind => {
                self.playhead_beat = 0.0;
                self.playing = false;
            }
            Action::FastForward => {
                self.playhead_beat += 4.0;
            }
            // ── Loop region bar controls ───────────────────────────────────────
            Action::SetLoopRegion { start_bar, end_bar } => {
                if end_bar > start_bar && start_bar >= 0.0 {
                    self.loop_start_bar = start_bar;
                    self.loop_end_bar = end_bar;
                }
            }
            Action::SetLoopBarEnabled(enabled) => {
                self.loop_bar_enabled = enabled;
            }
            Action::MoveLoopRegion { delta_bars } => {
                let new_start = (self.loop_start_bar + delta_bars).max(0.0);
                let region_len = self.loop_end_bar - self.loop_start_bar;
                self.loop_start_bar = new_start;
                self.loop_end_bar = new_start + region_len;
            }
            Action::ResizeLoopStart { bar } => {
                let clamped = bar.max(0.0);
                if clamped < self.loop_end_bar - 0.0625 {
                    self.loop_start_bar = clamped;
                }
            }
            Action::ResizeLoopEnd { bar } => {
                if bar > self.loop_start_bar + 0.0625 {
                    self.loop_end_bar = bar;
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn play_sets_playing() {
        let mut app = fresh();
        app.apply(Action::Play);
        assert!(app.playing);
        assert!(!app.recording);
    }

    #[test]
    fn stop_resets_state() {
        let mut app = fresh();
        app.apply(Action::Play);
        app.apply(Action::SetPlayheadBeat(10.0));
        app.apply(Action::Stop);
        assert!(!app.playing);
        assert!(!app.recording);
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn stop_with_loop_returns_to_loop_start() {
        let mut app = fresh();
        app.apply(Action::SetLoopRange { start: 4.0, end: 8.0 });
        app.apply(Action::ToggleLoop);
        app.apply(Action::Play);
        app.apply(Action::Stop);
        assert_eq!(app.playhead_beat, 4.0);
    }

    #[test]
    fn pause_stops_without_resetting() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(5.0));
        app.apply(Action::Play);
        app.apply(Action::Pause);
        assert!(!app.playing);
        assert_eq!(app.playhead_beat, 5.0);
    }

    #[test]
    fn record_sets_playing_and_recording() {
        let mut app = fresh();
        app.apply(Action::Record);
        assert!(app.playing);
        assert!(app.recording);
    }

    #[test]
    fn set_playhead_beat() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(16.0));
        assert_eq!(app.playhead_beat, 16.0);
    }

    #[test]
    fn set_playhead_beat_clamp_negative() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(-5.0));
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn toggle_loop() {
        let mut app = fresh();
        assert!(!app.loop_enabled);
        app.apply(Action::ToggleLoop);
        assert!(app.loop_enabled);
    }

    #[test]
    fn set_loop_range() {
        let mut app = fresh();
        app.apply(Action::SetLoopRange { start: 4.0, end: 12.0 });
        assert_eq!(app.loop_start, 4.0);
        assert_eq!(app.loop_end, 12.0);
    }

    #[test]
    fn set_loop_range_invalid_ignored() {
        let mut app = fresh();
        let prev_start = app.loop_start;
        app.apply(Action::SetLoopRange { start: 10.0, end: 5.0 });
        assert_eq!(app.loop_start, prev_start);
    }

    #[test]
    fn toggle_metronome() {
        let mut app = fresh();
        assert!(!app.metronome_enabled);
        app.apply(Action::ToggleMetronome);
        assert!(app.metronome_enabled);
    }

    #[test]
    fn rewind() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(32.0));
        app.apply(Action::Play);
        app.apply(Action::Rewind);
        assert_eq!(app.playhead_beat, 0.0);
        assert!(!app.playing);
    }

    #[test]
    fn fast_forward() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(8.0));
        app.apply(Action::FastForward);
        assert_eq!(app.playhead_beat, 12.0);
    }

    #[test]
    fn play_then_stop_clears_recording() {
        let mut app = fresh();
        app.apply(Action::Record);
        assert!(app.recording);
        app.apply(Action::Stop);
        assert!(!app.recording);
    }

    #[test]
    fn loop_range_start_clamped_to_zero() {
        let mut app = fresh();
        app.apply(Action::SetLoopRange { start: -2.0, end: 4.0 });
        assert_eq!(app.loop_start, 0.0);
        assert_eq!(app.loop_end, 4.0);
    }

    #[test]
    fn fast_forward_multiple_times() {
        let mut app = fresh();
        app.apply(Action::FastForward);
        app.apply(Action::FastForward);
        assert_eq!(app.playhead_beat, 8.0);
    }

    #[test]
    fn toggle_loop_off_after_on() {
        let mut app = fresh();
        app.apply(Action::ToggleLoop);
        assert!(app.loop_enabled);
        app.apply(Action::ToggleLoop);
        assert!(!app.loop_enabled);
    }

    #[test]
    fn stop_without_loop_returns_to_zero() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(20.0));
        app.apply(Action::Play);
        assert!(!app.loop_enabled);
        app.apply(Action::Stop);
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn rewind_while_recording() {
        let mut app = fresh();
        app.apply(Action::Record);
        app.apply(Action::Rewind);
        assert!(!app.playing);
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn playhead_cannot_go_negative() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(-100.0));
        assert_eq!(app.playhead_beat, 0.0);
    }

    #[test]
    fn metronome_toggle_twice_is_noop() {
        let mut app = fresh();
        let initial = app.metronome_enabled;
        app.apply(Action::ToggleMetronome);
        app.apply(Action::ToggleMetronome);
        assert_eq!(app.metronome_enabled, initial);
    }

    #[test]
    fn set_loop_range_equal_start_end_ignored() {
        let mut app = fresh();
        let prev = app.loop_start;
        app.apply(Action::SetLoopRange { start: 5.0, end: 5.0 });
        assert_eq!(app.loop_start, prev);
    }

    #[test]
    fn play_clears_recording_flag() {
        let mut app = fresh();
        app.apply(Action::Record);
        assert!(app.recording);
        app.apply(Action::Play);
        assert!(app.playing);
        assert!(!app.recording);
    }

    #[test]
    fn fast_forward_from_nonzero() {
        let mut app = fresh();
        app.apply(Action::SetPlayheadBeat(10.0));
        app.apply(Action::FastForward);
        assert_eq!(app.playhead_beat, 14.0);
    }

    #[test]
    fn transport_default_state() {
        let app = fresh();
        assert!(!app.playing);
        assert!(!app.recording);
        assert!(!app.loop_enabled);
        assert!(!app.metronome_enabled);
        assert_eq!(app.playhead_beat, 0.0);
        assert_eq!(app.loop_start, 0.0);
        assert_eq!(app.loop_end, 16.0);
    }

    // ── Loop region bar tests ─────────────────────────────────────────────────

    #[test]
    fn loop_bar_defaults() {
        let app = fresh();
        assert!((app.loop_start_bar - 1.0).abs() < 0.001);
        assert!((app.loop_end_bar - 5.0).abs() < 0.001);
        assert!(!app.loop_bar_enabled);
    }

    #[test]
    fn set_loop_region() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 2.0, end_bar: 6.0 });
        assert!((app.loop_start_bar - 2.0).abs() < 0.001);
        assert!((app.loop_end_bar - 6.0).abs() < 0.001);
    }

    #[test]
    fn set_loop_region_invalid_ignored() {
        let mut app = fresh();
        let prev_start = app.loop_start_bar;
        let prev_end = app.loop_end_bar;
        // end <= start should be ignored
        app.apply(Action::SetLoopRegion { start_bar: 5.0, end_bar: 3.0 });
        assert!((app.loop_start_bar - prev_start).abs() < 0.001);
        assert!((app.loop_end_bar - prev_end).abs() < 0.001);
    }

    #[test]
    fn set_loop_bar_enabled() {
        let mut app = fresh();
        assert!(!app.loop_bar_enabled);
        app.apply(Action::SetLoopBarEnabled(true));
        assert!(app.loop_bar_enabled);
        app.apply(Action::SetLoopBarEnabled(false));
        assert!(!app.loop_bar_enabled);
    }

    #[test]
    fn move_loop_region() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 2.0, end_bar: 6.0 });
        app.apply(Action::MoveLoopRegion { delta_bars: 2.0 });
        assert!((app.loop_start_bar - 4.0).abs() < 0.001);
        assert!((app.loop_end_bar - 8.0).abs() < 0.001);
    }

    #[test]
    fn move_loop_region_clamps_at_zero() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 1.0, end_bar: 4.0 });
        app.apply(Action::MoveLoopRegion { delta_bars: -5.0 });
        // start clamped to 0, region length preserved
        assert_eq!(app.loop_start_bar, 0.0);
        assert!((app.loop_end_bar - 3.0).abs() < 0.001);
    }

    #[test]
    fn resize_loop_start() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 2.0, end_bar: 8.0 });
        app.apply(Action::ResizeLoopStart { bar: 3.0 });
        assert!((app.loop_start_bar - 3.0).abs() < 0.001);
    }

    #[test]
    fn resize_loop_start_bounds_enforced() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 2.0, end_bar: 8.0 });
        // Trying to move start past (end - minimum gap) should be rejected
        app.apply(Action::ResizeLoopStart { bar: 7.99 });
        // Should remain at 2.0 since 7.99 is not < 8.0 - 0.0625
        assert!((app.loop_start_bar - 2.0).abs() < 0.001);
    }

    #[test]
    fn resize_loop_end() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 2.0, end_bar: 8.0 });
        app.apply(Action::ResizeLoopEnd { bar: 10.0 });
        assert!((app.loop_end_bar - 10.0).abs() < 0.001);
    }

    #[test]
    fn resize_loop_end_bounds_enforced() {
        let mut app = fresh();
        app.apply(Action::SetLoopRegion { start_bar: 4.0, end_bar: 8.0 });
        // Trying to move end to before (start + minimum gap) should be rejected
        app.apply(Action::ResizeLoopEnd { bar: 3.0 });
        // Should remain at 8.0 since 3.0 is not > 4.0 + 0.0625
        assert!((app.loop_end_bar - 8.0).abs() < 0.001);
    }
}
