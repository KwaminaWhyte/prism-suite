//! Smart mix domain — AI-driven frequency analysis and automated mix suggestions.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct FreqAnalysis {
    pub track_id: usize,
    pub low_rms: f32,
    pub mid_rms: f32,
    pub high_rms: f32,
    pub peak: f32,
}

#[derive(Clone, Debug)]
pub struct MixSuggestion {
    pub track_id: usize,
    pub gain_delta: f32,
    pub low_eq_delta: f32,
    pub mid_eq_delta: f32,
    pub high_eq_delta: f32,
    pub pan: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutoMixStatus {
    Idle,
    Analysing,
    Suggesting,
    Applied,
}

#[derive(Clone, Debug)]
pub struct AutoMixSession {
    pub id: usize,
    pub status: AutoMixStatus,
    pub analyses: Vec<FreqAnalysis>,
    pub suggestions: Vec<MixSuggestion>,
    pub target_lufs: f32,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_smart_mix(&mut self, action: &Action) {
        match action {
            Action::StartAutoMix => {
                let id = self.next_automix_id;
                self.next_automix_id += 1;
                self.automix_sessions.push(AutoMixSession {
                    id,
                    status: AutoMixStatus::Analysing,
                    analyses: vec![],
                    suggestions: vec![],
                    target_lufs: -14.0,
                });
            }
            Action::UpdateAutoMixAnalysis { session_id, analyses } => {
                if let Some(s) = self.automix_sessions.iter_mut().find(|s| s.id == *session_id) {
                    s.analyses = analyses.clone();
                    s.status = AutoMixStatus::Suggesting;
                }
            }
            Action::ApplyMixSuggestions { session_id, suggestions } => {
                if let Some(s) = self.automix_sessions.iter_mut().find(|s| s.id == *session_id) {
                    s.suggestions = suggestions.clone();
                    s.status = AutoMixStatus::Applied;
                }
            }
            Action::SetAutoMixTarget { session_id, lufs } => {
                if let Some(s) = self.automix_sessions.iter_mut().find(|s| s.id == *session_id) {
                    s.target_lufs = *lufs;
                }
            }
            Action::ResetAutoMix { session_id } => {
                if let Some(s) = self.automix_sessions.iter_mut().find(|s| s.id == *session_id) {
                    s.status = AutoMixStatus::Idle;
                    s.analyses = vec![];
                    s.suggestions = vec![];
                }
            }
            Action::DiscardAutoMix { session_id } => {
                self.automix_sessions.retain(|s| s.id != *session_id);
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn start_auto_mix_creates_session() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        assert_eq!(app.automix_sessions.len(), 1);
        assert_eq!(app.automix_sessions[0].status, AutoMixStatus::Analysing);
        assert!((app.automix_sessions[0].target_lufs - -14.0).abs() < 0.001);
    }

    #[test]
    fn multiple_sessions_get_unique_ids() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        app.apply(Action::StartAutoMix);
        app.apply(Action::StartAutoMix);
        let ids: Vec<usize> = app.automix_sessions.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(app.next_automix_id, 4);
    }

    #[test]
    fn update_analysis_sets_suggesting() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        let id = app.automix_sessions[0].id;
        let analyses = vec![FreqAnalysis { track_id: 1, low_rms: -30.0, mid_rms: -28.0, high_rms: -32.0, peak: -6.0 }];
        app.apply(Action::UpdateAutoMixAnalysis { session_id: id, analyses });
        assert_eq!(app.automix_sessions[0].status, AutoMixStatus::Suggesting);
        assert_eq!(app.automix_sessions[0].analyses.len(), 1);
        assert_eq!(app.automix_sessions[0].analyses[0].track_id, 1);
    }

    #[test]
    fn apply_suggestions_sets_applied() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        let id = app.automix_sessions[0].id;
        let suggestions = vec![MixSuggestion { track_id: 1, gain_delta: -2.0, low_eq_delta: 1.0, mid_eq_delta: 0.0, high_eq_delta: -1.0, pan: 0.1 }];
        app.apply(Action::ApplyMixSuggestions { session_id: id, suggestions });
        assert_eq!(app.automix_sessions[0].status, AutoMixStatus::Applied);
        assert_eq!(app.automix_sessions[0].suggestions.len(), 1);
        assert!((app.automix_sessions[0].suggestions[0].gain_delta - -2.0).abs() < 0.001);
    }

    #[test]
    fn set_auto_mix_target() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        let id = app.automix_sessions[0].id;
        app.apply(Action::SetAutoMixTarget { session_id: id, lufs: -16.0 });
        assert!((app.automix_sessions[0].target_lufs - -16.0).abs() < 0.001);
    }

    #[test]
    fn reset_auto_mix_clears_analyses_and_suggestions() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        let id = app.automix_sessions[0].id;
        let analyses = vec![FreqAnalysis { track_id: 2, low_rms: -20.0, mid_rms: -22.0, high_rms: -25.0, peak: -3.0 }];
        app.apply(Action::UpdateAutoMixAnalysis { session_id: id, analyses });
        app.apply(Action::ResetAutoMix { session_id: id });
        assert_eq!(app.automix_sessions[0].status, AutoMixStatus::Idle);
        assert!(app.automix_sessions[0].analyses.is_empty());
        assert!(app.automix_sessions[0].suggestions.is_empty());
    }

    #[test]
    fn discard_auto_mix_removes_session() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        app.apply(Action::StartAutoMix);
        let id = app.automix_sessions[0].id;
        app.apply(Action::DiscardAutoMix { session_id: id });
        assert_eq!(app.automix_sessions.len(), 1);
        assert_eq!(app.automix_sessions[0].id, 2);
    }

    #[test]
    fn session_starts_with_no_analyses() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        assert!(app.automix_sessions[0].analyses.is_empty());
        assert!(app.automix_sessions[0].suggestions.is_empty());
    }

    #[test]
    fn update_nonexistent_session_noop() {
        let mut app = fresh();
        app.apply(Action::UpdateAutoMixAnalysis { session_id: 999, analyses: vec![] });
        assert!(app.automix_sessions.is_empty());
    }

    #[test]
    fn discard_nonexistent_session_noop() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        app.apply(Action::DiscardAutoMix { session_id: 999 });
        assert_eq!(app.automix_sessions.len(), 1);
    }

    #[test]
    fn reset_preserves_target_lufs() {
        let mut app = fresh();
        app.apply(Action::StartAutoMix);
        let id = app.automix_sessions[0].id;
        app.apply(Action::SetAutoMixTarget { session_id: id, lufs: -9.0 });
        app.apply(Action::ResetAutoMix { session_id: id });
        assert!((app.automix_sessions[0].target_lufs - -9.0).abs() < 0.001);
    }
}
