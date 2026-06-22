use super::{App, Action};

#[derive(Clone, Debug, PartialEq)]
pub enum ScaleMode {
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    WholeTone,
    Diminished,
    Blues,
    Pentatonic,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChordQualityTone {
    Major,
    Minor,
    Dominant7,
    Major7,
    Minor7,
    HalfDim,
    Dim,
    Aug,
    Sus2,
    Sus4,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChordVoicing {
    Close,
    Open,
    Drop2,
    Drop3,
    Spread,
}

#[derive(Clone, Debug)]
pub struct ChordDegree {
    pub degree: u8,
    pub quality: ChordQualityTone,
    pub inversion: u8,
    pub added_notes: Vec<i8>,
    pub voicing: ChordVoicing,
}

#[derive(Clone, Debug)]
pub struct ChordProgression {
    pub id: usize,
    pub name: String,
    pub root: String,
    pub scale: ScaleMode,
    pub chords: Vec<ChordDegree>,
    pub bpm: Option<f32>,
    pub bars_per_chord: f32,
}

#[derive(Clone, Debug)]
pub struct ChordSuggestion {
    pub progression_id: usize,
    pub degree: u8,
    pub confidence: f32,
}

impl App {
    pub(crate) fn apply_chord_tools(&mut self, action: Action) {
        match action {
            Action::AddChordProgression { name, root, scale } => {
                let id = self.next_progression_id;
                self.next_progression_id += 1;
                self.chord_progressions.push(ChordProgression {
                    id,
                    name,
                    root,
                    scale,
                    chords: Vec::new(),
                    bpm: None,
                    bars_per_chord: 1.0,
                });
            }
            Action::DeleteChordProgression { progression_id } => {
                self.chord_progressions.retain(|p| p.id != progression_id);
                if self.active_progression_id == Some(progression_id) {
                    self.active_progression_id = None;
                }
            }
            Action::RenameChordProgression { progression_id, name } => {
                if let Some(p) = self.chord_progressions.iter_mut().find(|p| p.id == progression_id) {
                    p.name = name;
                }
            }
            Action::AddChordToProgression { progression_id, degree } => {
                if let Some(p) = self.chord_progressions.iter_mut().find(|p| p.id == progression_id) {
                    p.chords.push(degree);
                }
            }
            Action::RemoveChordFromProgression { progression_id, index } => {
                if let Some(p) = self.chord_progressions.iter_mut().find(|p| p.id == progression_id) {
                    if index < p.chords.len() {
                        p.chords.remove(index);
                    }
                }
            }
            Action::SetProgressionRoot { progression_id, root } => {
                if let Some(p) = self.chord_progressions.iter_mut().find(|p| p.id == progression_id) {
                    p.root = root;
                }
            }
            Action::SetProgressionScale { progression_id, scale } => {
                if let Some(p) = self.chord_progressions.iter_mut().find(|p| p.id == progression_id) {
                    p.scale = scale;
                }
            }
            Action::SetActiveProgression(id) => {
                self.active_progression_id = id;
            }
            Action::SetScaleLock { root, scale } => {
                match (root, scale) {
                    (Some(r), Some(s)) => self.scale_lock = Some((r, s)),
                    _ => self.scale_lock = None,
                }
            }
            Action::ClearChordSuggestions => {
                self.chord_suggestions.clear();
            }
            Action::AddChordSuggestion { progression_id, degree, confidence } => {
                self.chord_suggestions.push(ChordSuggestion {
                    progression_id,
                    degree,
                    confidence: confidence.clamp(0.0, 1.0),
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App { App::new() }

    #[test]
    fn add_chord_progression() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "I-IV-V".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        assert_eq!(app.chord_progressions.len(), 1);
        assert_eq!(app.chord_progressions[0].root, "C");
    }

    #[test]
    fn delete_chord_progression() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "test".to_string(),
            root: "G".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::DeleteChordProgression { progression_id: pid });
        assert!(app.chord_progressions.is_empty());
    }

    #[test]
    fn delete_active_progression_clears_active() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "test".to_string(),
            root: "G".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::SetActiveProgression(Some(pid)));
        app.apply(Action::DeleteChordProgression { progression_id: pid });
        assert!(app.active_progression_id.is_none());
    }

    #[test]
    fn rename_chord_progression() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "old".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::RenameChordProgression { progression_id: pid, name: "new".to_string() });
        assert_eq!(app.chord_progressions[0].name, "new");
    }

    #[test]
    fn add_chord_to_progression() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "p".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::AddChordToProgression {
            progression_id: pid,
            degree: ChordDegree {
                degree: 1,
                quality: ChordQualityTone::Major,
                inversion: 0,
                added_notes: vec![],
                voicing: ChordVoicing::Close,
            },
        });
        assert_eq!(app.chord_progressions[0].chords.len(), 1);
    }

    #[test]
    fn remove_chord_from_progression() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "p".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::AddChordToProgression {
            progression_id: pid,
            degree: ChordDegree { degree: 1, quality: ChordQualityTone::Major, inversion: 0, added_notes: vec![], voicing: ChordVoicing::Close },
        });
        app.apply(Action::RemoveChordFromProgression { progression_id: pid, index: 0 });
        assert!(app.chord_progressions[0].chords.is_empty());
    }

    #[test]
    fn set_progression_root() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "p".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::SetProgressionRoot { progression_id: pid, root: "F#".to_string() });
        assert_eq!(app.chord_progressions[0].root, "F#");
    }

    #[test]
    fn set_progression_scale() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "p".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::SetProgressionScale { progression_id: pid, scale: ScaleMode::Dorian });
        assert_eq!(app.chord_progressions[0].scale, ScaleMode::Dorian);
    }

    #[test]
    fn set_active_progression() {
        let mut app = fresh();
        app.apply(Action::AddChordProgression {
            name: "p".to_string(),
            root: "C".to_string(),
            scale: ScaleMode::Major,
        });
        let pid = app.chord_progressions[0].id;
        app.apply(Action::SetActiveProgression(Some(pid)));
        assert_eq!(app.active_progression_id, Some(pid));
    }

    #[test]
    fn set_scale_lock() {
        let mut app = fresh();
        app.apply(Action::SetScaleLock {
            root: Some("D".to_string()),
            scale: Some(ScaleMode::NaturalMinor),
        });
        assert!(app.scale_lock.is_some());
    }

    #[test]
    fn set_scale_lock_none_clears() {
        let mut app = fresh();
        app.apply(Action::SetScaleLock { root: Some("D".to_string()), scale: Some(ScaleMode::Major) });
        app.apply(Action::SetScaleLock { root: None, scale: None });
        assert!(app.scale_lock.is_none());
    }

    #[test]
    fn add_chord_suggestion_clamped_confidence() {
        let mut app = fresh();
        app.apply(Action::AddChordSuggestion { progression_id: 0, degree: 1, confidence: 1.5 });
        assert!((app.chord_suggestions[0].confidence - 1.0).abs() < 0.01);
    }

    #[test]
    fn clear_chord_suggestions() {
        let mut app = fresh();
        app.apply(Action::AddChordSuggestion { progression_id: 0, degree: 1, confidence: 0.9 });
        app.apply(Action::ClearChordSuggestions);
        assert!(app.chord_suggestions.is_empty());
    }
}
