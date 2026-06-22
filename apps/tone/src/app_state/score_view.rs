//! Score / Notation view domain — score display state, chord symbols.

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clef { Treble, Bass, Alto, Tenor, Grand }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StemDirection { Auto, Up, Down }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantizeDisplay { AsPlayed, Q8, Q16, Q32 }

pub struct ScoreView {
    pub enabled: bool,
    pub track_ids: Vec<usize>,
    pub zoom_h: f32,
    pub scroll_beat: f32,
    pub show_velocity_lane: bool,
    pub show_chord_symbols: bool,
    pub clef: Clef,
    pub key_signature: i8,
    pub time_signature_visible: bool,
    pub stem_direction: StemDirection,
    pub quantize_display: QuantizeDisplay,
    pub print_layout: bool,
}

impl ScoreView {
    pub fn new() -> Self {
        Self {
            enabled: false,
            track_ids: Vec::new(),
            zoom_h: 1.0,
            scroll_beat: 0.0,
            show_velocity_lane: false,
            show_chord_symbols: false,
            clef: Clef::Treble,
            key_signature: 0,
            time_signature_visible: true,
            stem_direction: StemDirection::Auto,
            quantize_display: QuantizeDisplay::Q16,
            print_layout: false,
        }
    }
}

impl Default for ScoreView {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordQuality {
    Major, Minor, Diminished, Augmented, Dominant,
    HalfDim, Suspended2, Suspended4,
}

pub struct ChordSymbol {
    pub id: usize,
    pub beat: f32,
    pub root: String,
    pub quality: ChordQuality,
    pub extension: Option<String>,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_score_view(&mut self, action: Action) {
        match action {
            Action::ToggleScoreView => {
                self.score_view.enabled = !self.score_view.enabled;
            }
            Action::AddTrackToScore { track_id } => {
                if !self.score_view.track_ids.contains(&track_id) {
                    self.score_view.track_ids.push(track_id);
                }
            }
            Action::RemoveTrackFromScore { track_id } => {
                self.score_view.track_ids.retain(|&id| id != track_id);
            }
            Action::SetScoreZoom(zoom) => {
                self.score_view.zoom_h = zoom.clamp(0.1, 10.0);
            }
            Action::SetScoreScroll(beat) => {
                self.score_view.scroll_beat = beat.max(0.0);
            }
            Action::SetScoreClef(clef) => {
                self.score_view.clef = clef;
            }
            Action::SetScoreKeySignature(key) => {
                self.score_view.key_signature = key.clamp(-7, 7);
            }
            Action::SetScoreStemDirection(dir) => {
                self.score_view.stem_direction = dir;
            }
            Action::SetScoreQuantizeDisplay(qd) => {
                self.score_view.quantize_display = qd;
            }
            Action::ToggleChordSymbols => {
                self.score_view.show_chord_symbols = !self.score_view.show_chord_symbols;
            }
            Action::AddChordSymbol { beat, root, quality, extension } => {
                let id = self.next_chord_id;
                self.next_chord_id += 1;
                self.chord_symbols.push(ChordSymbol { id, beat, root, quality, extension });
                self.chord_symbols.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(std::cmp::Ordering::Equal));
            }
            Action::RemoveChordSymbol { at_beat } => {
                if let Some(pos) = self.chord_symbols.iter().position(|c| (c.beat - at_beat).abs() < 0.01) {
                    self.chord_symbols.remove(pos);
                }
            }
            Action::SetPrintLayout(enabled) => {
                self.score_view.print_layout = enabled;
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::{Clef, ChordQuality, QuantizeDisplay, StemDirection};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn score_view_defaults() {
        let app = fresh();
        assert!(!app.score_view.enabled);
        assert!(app.score_view.track_ids.is_empty());
        assert!((app.score_view.zoom_h - 1.0).abs() < 0.001);
        assert_eq!(app.score_view.clef, Clef::Treble);
        assert_eq!(app.score_view.key_signature, 0);
        assert_eq!(app.score_view.stem_direction, StemDirection::Auto);
        assert_eq!(app.score_view.quantize_display, QuantizeDisplay::Q16);
        assert!(!app.score_view.print_layout);
        assert!(app.score_view.time_signature_visible);
    }

    #[test]
    fn toggle_score_view() {
        let mut app = fresh();
        assert!(!app.score_view.enabled);
        app.apply(Action::ToggleScoreView);
        assert!(app.score_view.enabled);
        app.apply(Action::ToggleScoreView);
        assert!(!app.score_view.enabled);
    }

    #[test]
    fn add_remove_track_from_score() {
        let mut app = fresh();
        app.apply(Action::AddTrackToScore { track_id: 1 });
        app.apply(Action::AddTrackToScore { track_id: 2 });
        assert_eq!(app.score_view.track_ids.len(), 2);
        // Duplicate should not add again
        app.apply(Action::AddTrackToScore { track_id: 1 });
        assert_eq!(app.score_view.track_ids.len(), 2);
        app.apply(Action::RemoveTrackFromScore { track_id: 1 });
        assert_eq!(app.score_view.track_ids, vec![2]);
    }

    #[test]
    fn set_score_zoom_clamped() {
        let mut app = fresh();
        app.apply(Action::SetScoreZoom(0.0));
        assert!((app.score_view.zoom_h - 0.1).abs() < 0.001);
        app.apply(Action::SetScoreZoom(100.0));
        assert!((app.score_view.zoom_h - 10.0).abs() < 0.001);
        app.apply(Action::SetScoreZoom(2.5));
        assert!((app.score_view.zoom_h - 2.5).abs() < 0.001);
    }

    #[test]
    fn set_score_scroll() {
        let mut app = fresh();
        app.apply(Action::SetScoreScroll(8.0));
        assert!((app.score_view.scroll_beat - 8.0).abs() < 0.001);
        // Negative clamped to 0
        app.apply(Action::SetScoreScroll(-1.0));
        assert_eq!(app.score_view.scroll_beat, 0.0);
    }

    #[test]
    fn set_score_clef() {
        let mut app = fresh();
        for clef in [Clef::Bass, Clef::Alto, Clef::Tenor, Clef::Grand, Clef::Treble] {
            app.apply(Action::SetScoreClef(clef));
            assert_eq!(app.score_view.clef, clef);
        }
    }

    #[test]
    fn set_score_key_signature_clamped() {
        let mut app = fresh();
        app.apply(Action::SetScoreKeySignature(10));
        assert_eq!(app.score_view.key_signature, 7);
        app.apply(Action::SetScoreKeySignature(-10));
        assert_eq!(app.score_view.key_signature, -7);
        app.apply(Action::SetScoreKeySignature(3));
        assert_eq!(app.score_view.key_signature, 3);
    }

    #[test]
    fn set_score_stem_direction() {
        let mut app = fresh();
        app.apply(Action::SetScoreStemDirection(StemDirection::Up));
        assert_eq!(app.score_view.stem_direction, StemDirection::Up);
        app.apply(Action::SetScoreStemDirection(StemDirection::Down));
        assert_eq!(app.score_view.stem_direction, StemDirection::Down);
    }

    #[test]
    fn set_score_quantize_display() {
        let mut app = fresh();
        app.apply(Action::SetScoreQuantizeDisplay(QuantizeDisplay::Q32));
        assert_eq!(app.score_view.quantize_display, QuantizeDisplay::Q32);
    }

    #[test]
    fn toggle_chord_symbols() {
        let mut app = fresh();
        assert!(!app.score_view.show_chord_symbols);
        app.apply(Action::ToggleChordSymbols);
        assert!(app.score_view.show_chord_symbols);
    }

    #[test]
    fn add_chord_symbol() {
        let mut app = fresh();
        app.apply(Action::AddChordSymbol {
            beat: 4.0,
            root: "G".to_string(),
            quality: ChordQuality::Major,
            extension: Some("7".to_string()),
        });
        assert_eq!(app.chord_symbols.len(), 1);
        assert_eq!(app.chord_symbols[0].root, "G");
        assert_eq!(app.chord_symbols[0].quality, ChordQuality::Major);
    }

    #[test]
    fn chord_symbols_sorted_by_beat() {
        let mut app = fresh();
        app.apply(Action::AddChordSymbol { beat: 8.0, root: "Am".to_string(), quality: ChordQuality::Minor, extension: None });
        app.apply(Action::AddChordSymbol { beat: 0.0, root: "C".to_string(), quality: ChordQuality::Major, extension: None });
        app.apply(Action::AddChordSymbol { beat: 4.0, root: "F".to_string(), quality: ChordQuality::Major, extension: None });
        assert!((app.chord_symbols[0].beat - 0.0).abs() < 0.001);
        assert!((app.chord_symbols[1].beat - 4.0).abs() < 0.001);
        assert!((app.chord_symbols[2].beat - 8.0).abs() < 0.001);
    }

    #[test]
    fn remove_chord_symbol() {
        let mut app = fresh();
        app.apply(Action::AddChordSymbol { beat: 4.0, root: "C".to_string(), quality: ChordQuality::Major, extension: None });
        app.apply(Action::RemoveChordSymbol { at_beat: 4.0 });
        assert!(app.chord_symbols.is_empty());
    }

    #[test]
    fn set_print_layout() {
        let mut app = fresh();
        app.apply(Action::SetPrintLayout(true));
        assert!(app.score_view.print_layout);
    }
}
