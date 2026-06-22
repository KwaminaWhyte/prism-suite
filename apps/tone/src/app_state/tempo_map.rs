//! Tempo map domain — TempoEvent, TimeSigEvent types + apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// A tempo change event in the project timeline.
#[derive(Clone, Debug)]
pub struct TempoEvent {
    pub id: usize,
    /// Bar position (1-indexed, 1.0 = start of project).
    pub bar: f32,
    /// Beats per minute at this position.
    pub bpm: f32,
}

/// A time signature change event in the project timeline.
#[derive(Clone, Debug)]
pub struct TimeSigEvent {
    pub id: usize,
    /// Bar position (1-indexed).
    pub bar: f32,
    pub numerator: u8,
    pub denominator: u8,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_tempo_map(&mut self, action: Action) {
        match action {
            Action::AddTempoEvent { bar, bpm } => {
                let id = self.next_tempo_event_id;
                self.next_tempo_event_id += 1;
                self.tempo_map.push(TempoEvent {
                    id,
                    bar,
                    bpm: bpm.clamp(20.0, 999.0),
                });
                self.tempo_map.sort_by(|a, b| a.bar.partial_cmp(&b.bar).unwrap());
            }
            Action::RemoveTempoEvent { event_id } => {
                // Cannot remove the bar-1 anchor event.
                if let Some(ev) = self.tempo_map.iter().find(|e| e.id == event_id) {
                    if (ev.bar - 1.0).abs() < 1e-6 {
                        return;
                    }
                }
                self.tempo_map.retain(|e| e.id != event_id);
            }
            Action::MoveTempoEvent { event_id, bar } => {
                // Cannot move the bar-1 anchor.
                if let Some(ev) = self.tempo_map.iter().find(|e| e.id == event_id) {
                    if (ev.bar - 1.0).abs() < 1e-6 {
                        return;
                    }
                }
                if let Some(ev) = self.tempo_map.iter_mut().find(|e| e.id == event_id) {
                    ev.bar = bar;
                }
                self.tempo_map.sort_by(|a, b| a.bar.partial_cmp(&b.bar).unwrap());
            }
            Action::AddTimeSigEvent { bar, numerator, denominator } => {
                let id = self.next_time_sig_event_id;
                self.next_time_sig_event_id += 1;
                self.time_sig_map.push(TimeSigEvent { id, bar, numerator, denominator });
                self.time_sig_map.sort_by(|a, b| a.bar.partial_cmp(&b.bar).unwrap());
            }
            Action::RemoveTimeSigEvent { event_id } => {
                // Cannot remove the bar-1 anchor.
                if let Some(ev) = self.time_sig_map.iter().find(|e| e.id == event_id) {
                    if (ev.bar - 1.0).abs() < 1e-6 {
                        return;
                    }
                }
                self.time_sig_map.retain(|e| e.id != event_id);
            }
            Action::SetGlobalBpm(bpm) => {
                let clamped = bpm.clamp(20.0, 999.0);
                self.project.bpm = clamped;
                // Update the bar-1 anchor event.
                if let Some(ev) = self.tempo_map.iter_mut().find(|e| (e.bar - 1.0).abs() < 1e-6) {
                    ev.bpm = clamped;
                }
            }
            _ => {}
        }
    }

    /// Return the BPM in effect at the given bar position.
    pub fn bpm_at_bar(&self, bar: f32) -> f32 {
        let mut result = self.project.bpm;
        for ev in &self.tempo_map {
            if ev.bar <= bar {
                result = ev.bpm;
            } else {
                break;
            }
        }
        result
    }

    /// Return the time signature (numerator, denominator) in effect at the given bar.
    pub fn time_sig_at_bar(&self, bar: f32) -> (u8, u8) {
        let mut num = self.project.time_signature_num;
        let mut den = self.project.time_signature_den;
        for ev in &self.time_sig_map {
            if ev.bar <= bar {
                num = ev.numerator;
                den = ev.denominator;
            } else {
                break;
            }
        }
        (num, den)
    }

    /// Convert a beat position to seconds using constant BPM from bar 1.
    pub fn beats_to_seconds(&self, beat: f32) -> f64 {
        let bpm = self.bpm_at_bar(1.0) as f64;
        beat as f64 * 60.0 / bpm
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
    fn initial_tempo_map_has_anchor() {
        let app = fresh();
        assert_eq!(app.tempo_map.len(), 1);
        assert!((app.tempo_map[0].bar - 1.0).abs() < 0.001);
        assert!((app.tempo_map[0].bpm - 120.0).abs() < 0.001);
    }

    #[test]
    fn initial_time_sig_map_has_anchor() {
        let app = fresh();
        assert_eq!(app.time_sig_map.len(), 1);
        assert!((app.time_sig_map[0].bar - 1.0).abs() < 0.001);
        assert_eq!(app.time_sig_map[0].numerator, 4);
        assert_eq!(app.time_sig_map[0].denominator, 4);
    }

    #[test]
    fn add_tempo_event() {
        let mut app = fresh();
        app.apply(Action::AddTempoEvent { bar: 5.0, bpm: 140.0 });
        assert_eq!(app.tempo_map.len(), 2);
        let ev = app.tempo_map.iter().find(|e| (e.bar - 5.0).abs() < 0.001).unwrap();
        assert!((ev.bpm - 140.0).abs() < 0.001);
    }

    #[test]
    fn add_tempo_event_bpm_clamped() {
        let mut app = fresh();
        app.apply(Action::AddTempoEvent { bar: 2.0, bpm: 5000.0 });
        let ev = app.tempo_map.iter().find(|e| (e.bar - 2.0).abs() < 0.001).unwrap();
        assert!(ev.bpm <= 999.0);
        app.apply(Action::AddTempoEvent { bar: 3.0, bpm: 5.0 });
        let ev = app.tempo_map.iter().find(|e| (e.bar - 3.0).abs() < 0.001).unwrap();
        assert!(ev.bpm >= 20.0);
    }

    #[test]
    fn tempo_events_sorted_by_bar() {
        let mut app = fresh();
        app.apply(Action::AddTempoEvent { bar: 9.0, bpm: 150.0 });
        app.apply(Action::AddTempoEvent { bar: 3.0, bpm: 130.0 });
        // Sorted: bar 1, 3, 9
        assert!(app.tempo_map[0].bar <= app.tempo_map[1].bar);
        assert!(app.tempo_map[1].bar <= app.tempo_map[2].bar);
    }

    #[test]
    fn remove_tempo_event() {
        let mut app = fresh();
        app.apply(Action::AddTempoEvent { bar: 5.0, bpm: 140.0 });
        let id = app.tempo_map.iter().find(|e| (e.bar - 5.0).abs() < 0.001).unwrap().id;
        app.apply(Action::RemoveTempoEvent { event_id: id });
        assert_eq!(app.tempo_map.len(), 1); // only anchor remains
    }

    #[test]
    fn remove_tempo_event_bar1_anchor_is_noop() {
        let mut app = fresh();
        let anchor_id = app.tempo_map[0].id;
        app.apply(Action::RemoveTempoEvent { event_id: anchor_id });
        assert_eq!(app.tempo_map.len(), 1);
    }

    #[test]
    fn move_tempo_event() {
        let mut app = fresh();
        app.apply(Action::AddTempoEvent { bar: 5.0, bpm: 140.0 });
        let id = app.tempo_map.iter().find(|e| (e.bar - 5.0).abs() < 0.001).unwrap().id;
        app.apply(Action::MoveTempoEvent { event_id: id, bar: 8.0 });
        let ev = app.tempo_map.iter().find(|e| e.id == id).unwrap();
        assert!((ev.bar - 8.0).abs() < 0.001);
    }

    #[test]
    fn move_tempo_event_bar1_anchor_is_noop() {
        let mut app = fresh();
        let anchor_id = app.tempo_map[0].id;
        app.apply(Action::MoveTempoEvent { event_id: anchor_id, bar: 5.0 });
        let anchor = app.tempo_map.iter().find(|e| e.id == anchor_id).unwrap();
        assert!((anchor.bar - 1.0).abs() < 0.001);
    }

    #[test]
    fn set_global_bpm() {
        let mut app = fresh();
        app.apply(Action::SetGlobalBpm(140.0));
        assert!((app.project.bpm - 140.0).abs() < 0.001);
        let anchor = &app.tempo_map[0];
        assert!((anchor.bpm - 140.0).abs() < 0.001);
    }

    #[test]
    fn bpm_at_bar_returns_project_bpm_when_no_events() {
        let app = fresh();
        // Only the anchor exists at bar 1
        let bpm = app.bpm_at_bar(10.0);
        assert!((bpm - 120.0).abs() < 0.001);
    }

    #[test]
    fn bpm_at_bar_returns_correct_event() {
        let mut app = fresh();
        app.apply(Action::AddTempoEvent { bar: 5.0, bpm: 180.0 });
        // Before bar 5, should be 120
        assert!((app.bpm_at_bar(3.0) - 120.0).abs() < 0.001);
        // At or after bar 5, should be 180
        assert!((app.bpm_at_bar(5.0) - 180.0).abs() < 0.001);
        assert!((app.bpm_at_bar(10.0) - 180.0).abs() < 0.001);
    }

    #[test]
    fn time_sig_at_bar() {
        let mut app = fresh();
        app.apply(Action::AddTimeSigEvent { bar: 5.0, numerator: 3, denominator: 4 });
        assert_eq!(app.time_sig_at_bar(1.0), (4, 4));
        assert_eq!(app.time_sig_at_bar(5.0), (3, 4));
        assert_eq!(app.time_sig_at_bar(9.0), (3, 4));
    }

    #[test]
    fn remove_time_sig_event() {
        let mut app = fresh();
        app.apply(Action::AddTimeSigEvent { bar: 5.0, numerator: 3, denominator: 4 });
        let id = app.time_sig_map.iter().find(|e| (e.bar - 5.0).abs() < 0.001).unwrap().id;
        app.apply(Action::RemoveTimeSigEvent { event_id: id });
        assert_eq!(app.time_sig_map.len(), 1);
    }

    #[test]
    fn remove_time_sig_bar1_anchor_is_noop() {
        let mut app = fresh();
        let anchor_id = app.time_sig_map[0].id;
        app.apply(Action::RemoveTimeSigEvent { event_id: anchor_id });
        assert_eq!(app.time_sig_map.len(), 1);
    }

    #[test]
    fn beats_to_seconds_120bpm() {
        let app = fresh();
        // At 120 BPM: 1 beat = 0.5 seconds
        let secs = app.beats_to_seconds(4.0);
        assert!((secs - 2.0).abs() < 0.001);
    }

    #[test]
    fn beats_to_seconds_after_global_bpm_change() {
        let mut app = fresh();
        app.apply(Action::SetGlobalBpm(60.0));
        // At 60 BPM: 1 beat = 1 second
        let secs = app.beats_to_seconds(4.0);
        assert!((secs - 4.0).abs() < 0.001);
    }
}
