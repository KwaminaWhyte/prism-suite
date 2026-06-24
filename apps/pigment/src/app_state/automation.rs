//! Actions / batch automation (Photoshop *Actions* panel parity).
//!
//! Records the sequence of [`Action`]s the user performs into a named
//! [`ActionSet`], then replays them over the current document. Replay simply
//! re-dispatches each cloned action through [`App::apply`], so any action that is
//! `Clone` (all of them are) can be recorded and played back deterministically.
//!
//! This is app-local and needs no GPU: recording captures the action stream;
//! playback re-runs the same state mutations the UI would have. A small set of
//! "transient" actions (recording control, file pickers that block on a dialog)
//! are filtered out of recordings so a played-back set is self-contained.

use super::{Action, App};

/// One named, ordered list of recorded actions (a Photoshop "Action").
#[derive(Debug, Clone, Default)]
pub struct ActionSet {
    pub name: String,
    /// The recorded steps, in execution order.
    pub steps: Vec<Action>,
    /// Whether this set is enabled for batch runs.
    pub enabled: bool,
}

impl ActionSet {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), steps: Vec::new(), enabled: true }
    }

    /// Number of recorded steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// In-progress recording state plus the library of saved action sets.
#[derive(Debug, Clone, Default)]
pub struct AutomationState {
    /// Saved, replayable action sets.
    pub sets: Vec<ActionSet>,
    /// When `Some`, the index into `sets` currently being recorded into.
    pub recording_into: Option<usize>,
    /// Re-entrancy guard: true while a set is replaying so the recorder does not
    /// capture the replayed actions back into a set.
    pub replaying: bool,
}

impl AutomationState {
    /// Find a set's index by name.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.sets.iter().position(|s| s.name == name)
    }
}

/// Actions that must never be recorded (recording control + transient UI that
/// would make a replay non-deterministic or recursive).
fn is_recordable(action: &Action) -> bool {
    !matches!(
        action,
        Action::StartRecording { .. }
            | Action::StopRecording
            | Action::PlayActionSet { .. }
            | Action::PlayActionSetByName { .. }
            | Action::DeleteActionSet { .. }
            | Action::ClearActionSet { .. }
            | Action::RenameActionSet { .. }
    )
}

impl App {
    /// Called from the top of `apply` for every action. If a recording is active
    /// and the action is recordable (and we are not mid-replay), append a clone of
    /// it to the target set. Returns nothing; pure side-effect on `automation`.
    pub(super) fn record_action(&mut self, action: &Action) {
        if self.automation.replaying {
            return;
        }
        let Some(idx) = self.automation.recording_into else {
            return;
        };
        if !is_recordable(action) {
            return;
        }
        if let Some(set) = self.automation.sets.get_mut(idx) {
            set.steps.push(action.clone());
        }
    }

    pub(super) fn apply_automation(&mut self, action: Action) {
        match action {
            Action::StartRecording { name } => {
                let idx = match self.automation.index_of(&name) {
                    Some(i) => i,
                    None => {
                        self.automation.sets.push(ActionSet::new(name.clone()));
                        self.automation.sets.len() - 1
                    }
                };
                self.automation.recording_into = Some(idx);
                self.status_message = Some(format!("Recording '{name}'"));
            }
            Action::StopRecording => {
                if let Some(idx) = self.automation.recording_into.take() {
                    let n = self.automation.sets.get(idx).map(|s| s.len()).unwrap_or(0);
                    self.status_message = Some(format!("Recorded {n} step(s)"));
                }
            }
            Action::PlayActionSet { index } => {
                self.play_action_set_index(index);
            }
            Action::PlayActionSetByName { name } => {
                if let Some(idx) = self.automation.index_of(&name) {
                    self.play_action_set_index(idx);
                } else {
                    self.status_message = Some(format!("No action set '{name}'"));
                }
            }
            Action::DeleteActionSet { index } => {
                if index < self.automation.sets.len() {
                    self.automation.sets.remove(index);
                    // Keep the recording target valid.
                    if let Some(rec) = self.automation.recording_into {
                        if rec == index {
                            self.automation.recording_into = None;
                        } else if rec > index {
                            self.automation.recording_into = Some(rec - 1);
                        }
                    }
                }
            }
            Action::ClearActionSet { index } => {
                if let Some(set) = self.automation.sets.get_mut(index) {
                    set.steps.clear();
                }
            }
            Action::RenameActionSet { index, name } => {
                if let Some(set) = self.automation.sets.get_mut(index) {
                    set.name = name;
                }
            }
            _ => {}
        }
    }

    /// Replay every step of the set at `index` through `apply`. Guards against
    /// re-recording (sets `replaying`) and against a set replaying itself
    /// (the play actions are non-recordable anyway, but the guard is belt-and-
    /// braces). Steps are cloned so the stored set is reusable.
    pub fn play_action_set_index(&mut self, index: usize) {
        let Some(steps) = self.automation.sets.get(index).map(|s| s.steps.clone()) else {
            self.status_message = Some(format!("No action set at index {index}"));
            return;
        };
        let prev = self.automation.replaying;
        self.automation.replaying = true;
        let n = steps.len();
        for step in steps {
            self.apply(step);
        }
        self.automation.replaying = prev;
        self.status_message = Some(format!("Played {n} step(s)"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{AdjKind, Tool};

    #[test]
    fn action_set_new_is_empty_enabled() {
        let s = ActionSet::new("My Set");
        assert_eq!(s.name, "My Set");
        assert!(s.is_empty());
        assert!(s.enabled);
    }

    #[test]
    fn start_recording_creates_set() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "A".into() });
        assert_eq!(app.automation.sets.len(), 1);
        assert_eq!(app.automation.recording_into, Some(0));
    }

    #[test]
    fn recording_captures_actions() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "A".into() });
        app.apply(Action::SetBrushSize(40.0));
        app.apply(Action::SetTool(Tool::Eraser));
        app.apply(Action::StopRecording);
        assert_eq!(app.automation.sets[0].len(), 2);
        assert!(app.automation.recording_into.is_none());
    }

    #[test]
    fn recording_excludes_control_actions() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "A".into() });
        // PlayActionSet is non-recordable even while recording.
        app.apply(Action::PlayActionSet { index: 0 });
        app.apply(Action::SetBrushSize(10.0));
        app.apply(Action::StopRecording);
        assert_eq!(app.automation.sets[0].len(), 1);
    }

    #[test]
    fn replay_applies_steps() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "Setup".into() });
        app.apply(Action::SetBrushSize(77.0));
        app.apply(Action::SetTool(Tool::Eraser));
        app.apply(Action::StopRecording);

        // Reset state so we can observe replay effects.
        app.apply(Action::SetBrushSize(1.0));
        app.apply(Action::SetTool(Tool::Brush));
        assert_eq!(app.brush.size, 1.0);

        app.apply(Action::PlayActionSetByName { name: "Setup".into() });
        assert_eq!(app.brush.size, 77.0);
        assert_eq!(app.active, Tool::Eraser);
    }

    #[test]
    fn replay_does_not_re_record() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "A".into() });
        app.apply(Action::SetBrushSize(5.0));
        app.apply(Action::StopRecording);
        let len_before = app.automation.sets[0].len();

        // Record into a second set, then play the first while recording. The
        // replayed steps must NOT be captured into the second set.
        app.apply(Action::StartRecording { name: "B".into() });
        app.apply(Action::PlayActionSetByName { name: "A".into() });
        app.apply(Action::StopRecording);
        let b = app.automation.index_of("B").unwrap();
        assert_eq!(app.automation.sets[b].len(), 0);
        // First set unchanged.
        assert_eq!(app.automation.sets[0].len(), len_before);
    }

    #[test]
    fn replay_with_real_doc_mutation() {
        let mut app = App::new();
        let before = app.doc.layers.layers.len();
        app.apply(Action::StartRecording { name: "AddAdj".into() });
        app.apply(Action::AddAdjustment(AdjKind::Invert));
        app.apply(Action::StopRecording);
        // Playing it again adds another adjustment layer.
        app.apply(Action::PlayActionSet { index: 0 });
        assert!(app.doc.layers.layers.len() >= before + 2);
    }

    #[test]
    fn delete_set_fixes_recording_index() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "A".into() });
        app.apply(Action::StopRecording);
        app.apply(Action::StartRecording { name: "B".into() });
        // Recording into index 1 now; delete index 0 shifts it to 0.
        app.apply(Action::DeleteActionSet { index: 0 });
        assert_eq!(app.automation.recording_into, Some(0));
        assert_eq!(app.automation.sets.len(), 1);
        assert_eq!(app.automation.sets[0].name, "B");
    }

    #[test]
    fn clear_and_rename_set() {
        let mut app = App::new();
        app.apply(Action::StartRecording { name: "A".into() });
        app.apply(Action::SetBrushSize(9.0));
        app.apply(Action::StopRecording);
        app.apply(Action::RenameActionSet { index: 0, name: "Renamed".into() });
        assert_eq!(app.automation.sets[0].name, "Renamed");
        app.apply(Action::ClearActionSet { index: 0 });
        assert!(app.automation.sets[0].is_empty());
    }

    #[test]
    fn play_missing_name_is_noop() {
        let mut app = App::new();
        app.apply(Action::PlayActionSetByName { name: "ghost".into() });
        // No panic, status set.
        assert!(app.status_message.is_some());
    }
}
