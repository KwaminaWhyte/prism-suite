//! Undo/redo history stack and document lifecycle (new/save/open) for Drift.
//!
//! Uses a snapshot-label approach: we store the action label at each history
//! entry so the UI can display "Undo: Add Layer" etc. Actual state rollback
//! would require full snapshots (deferred to a future phase); for now the
//! cursor position is the source of truth for undo availability.

use super::{App, Action};

// ── History types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub label: String,
    pub snapshot_marker: usize,
}

pub struct AppHistory {
    pub entries: Vec<HistoryEntry>,
    /// Points just past the last committed entry (i.e. `entries[cursor-1]` is
    /// the most-recently-done action; `entries[cursor]` is the next redo).
    pub cursor: usize,
    pub max_entries: usize,
}

impl Default for AppHistory {
    fn default() -> Self {
        Self {
            entries: vec![],
            cursor: 0,
            max_entries: 50,
        }
    }
}

impl AppHistory {
    /// Record a new action. Drops any redo branch that existed after the
    /// current cursor position, then appends and enforces the size cap.
    pub fn push(&mut self, label: String) {
        // Drop redo branch
        self.entries.truncate(self.cursor);
        self.entries.push(HistoryEntry {
            label,
            snapshot_marker: self.cursor,
        });
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    pub fn undo_label(&self) -> Option<&str> {
        if self.cursor > 0 {
            Some(&self.entries[self.cursor - 1].label)
        } else {
            None
        }
    }

    pub fn redo_label(&self) -> Option<&str> {
        if self.cursor < self.entries.len() {
            Some(&self.entries[self.cursor].label)
        } else {
            None
        }
    }

    pub fn do_undo(&mut self) {
        if self.can_undo() {
            self.cursor -= 1;
        }
    }

    pub fn do_redo(&mut self) {
        if self.can_redo() {
            self.cursor += 1;
        }
    }
}

// ── App impl ──────────────────────────────────────────────────────────────────

impl App {
    /// Dispatch handler for history and document-lifecycle actions.
    pub(super) fn apply_history(&mut self, action: Action) {
        match action {
            Action::Undo => self.history.do_undo(),
            Action::Redo => self.history.do_redo(),
            Action::ClearHistory => {
                self.history.entries.clear();
                self.history.cursor = 0;
            }
            Action::NewDocument => {
                // Reset core document state
                self.layers.clear();
                self.layer_counter = 0;
                self.active_layer = None;
                self.keyframes.clear();
                self.keyframe_counter = 0;
                self.transforms.clear();
                self.current_frame = 0;
                self.playing = false;
                self.history.entries.clear();
                self.history.cursor = 0;
                self.document_path = None;
                self.document_dirty = false;
            }
            Action::SaveDocument { path } => {
                // Stub: record path; actual I/O deferred to prism-io
                self.document_path = Some(path);
                self.document_dirty = false;
            }
            Action::OpenDocument { path } => {
                // Stub: record path; actual I/O deferred to prism-io
                self.document_path = Some(path);
            }
            Action::MarkDirty => {
                self.document_dirty = true;
            }
            _ => {}
        }
    }

    /// Push a label onto the history stack and mark the document dirty.
    /// Call this from `apply()` before every mutating action.
    pub fn record_history(&mut self, label: &str) {
        self.history.push(label.to_string());
        self.document_dirty = true;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action, LayerKind};

    fn app() -> App {
        App::new()
    }

    // ── AppHistory unit tests ─────────────────────────────────────────────────

    #[test]
    fn test_push_can_undo() {
        let mut a = app();
        assert!(!a.history.can_undo());
        a.history.push("Add Layer".to_string());
        assert!(a.history.can_undo());
    }

    #[test]
    fn test_push_can_redo_false_initially() {
        let mut a = app();
        a.history.push("Add Layer".to_string());
        assert!(!a.history.can_redo());
    }

    #[test]
    fn test_undo_label() {
        let mut a = app();
        a.history.push("Delete Layer".to_string());
        assert_eq!(a.history.undo_label(), Some("Delete Layer"));
    }

    #[test]
    fn test_redo_label_after_undo() {
        let mut a = app();
        a.history.push("Move Bone".to_string());
        a.history.do_undo();
        assert_eq!(a.history.redo_label(), Some("Move Bone"));
    }

    #[test]
    fn test_do_undo_moves_cursor_back() {
        let mut a = app();
        a.history.push("A".to_string());
        a.history.push("B".to_string());
        assert_eq!(a.history.cursor, 2);
        a.history.do_undo();
        assert_eq!(a.history.cursor, 1);
    }

    #[test]
    fn test_do_redo_moves_cursor_forward() {
        let mut a = app();
        a.history.push("A".to_string());
        a.history.do_undo();
        assert_eq!(a.history.cursor, 0);
        a.history.do_redo();
        assert_eq!(a.history.cursor, 1);
    }

    #[test]
    fn test_push_after_undo_clears_redo_branch() {
        let mut a = app();
        a.history.push("A".to_string());
        a.history.push("B".to_string());
        a.history.do_undo(); // cursor = 1, entries = [A, B]
        a.history.push("C".to_string()); // should drop B
        assert_eq!(a.history.entries.len(), 2);
        assert_eq!(a.history.entries[1].label, "C");
        assert!(!a.history.can_redo());
    }

    #[test]
    fn test_max_50_entries_eviction() {
        let mut a = app();
        for i in 0..55usize {
            a.history.push(format!("action {}", i));
        }
        assert_eq!(a.history.entries.len(), 50);
        assert!(a.history.cursor <= 50);
        // The oldest entries should have been evicted
        assert!(a.history.entries[0].label.starts_with("action "));
    }

    #[test]
    fn test_undo_at_zero_is_noop() {
        let mut a = app();
        a.history.do_undo(); // should not panic
        assert_eq!(a.history.cursor, 0);
    }

    #[test]
    fn test_redo_at_end_is_noop() {
        let mut a = app();
        a.history.push("X".to_string());
        a.history.do_redo(); // cursor already at end
        assert_eq!(a.history.cursor, 1);
    }

    // ── Action-based tests ────────────────────────────────────────────────────

    #[test]
    fn test_action_undo_redo() {
        let mut a = app();
        a.history.push("step1".to_string());
        a.history.push("step2".to_string());
        a.apply(Action::Undo);
        assert_eq!(a.history.cursor, 1);
        a.apply(Action::Redo);
        assert_eq!(a.history.cursor, 2);
    }

    #[test]
    fn test_action_clear_history() {
        let mut a = app();
        a.history.push("A".to_string());
        a.history.push("B".to_string());
        a.apply(Action::ClearHistory);
        assert!(a.history.entries.is_empty());
        assert_eq!(a.history.cursor, 0);
    }

    #[test]
    fn test_action_new_document_resets_state() {
        let mut a = app();
        a.apply(Action::AddLayer { name: "Bg".to_string(), kind: LayerKind::Bitmap });
        a.history.push("Add Layer".to_string());
        a.document_path = Some("/tmp/test.drift".to_string());
        a.apply(Action::NewDocument);
        assert!(a.layers.is_empty());
        assert_eq!(a.layer_counter, 0);
        assert!(a.active_layer.is_none());
        assert!(a.keyframes.is_empty());
        assert_eq!(a.current_frame, 0);
        assert!(!a.playing);
        assert!(a.history.entries.is_empty());
        assert!(a.document_path.is_none());
        assert!(!a.document_dirty);
    }

    #[test]
    fn test_action_save_document_sets_path_and_clears_dirty() {
        let mut a = app();
        a.document_dirty = true;
        a.apply(Action::SaveDocument { path: "/home/user/anim.drift".to_string() });
        assert_eq!(a.document_path.as_deref(), Some("/home/user/anim.drift"));
        assert!(!a.document_dirty);
    }

    #[test]
    fn test_action_open_document_sets_path() {
        let mut a = app();
        a.apply(Action::OpenDocument { path: "/projects/hero.drift".to_string() });
        assert_eq!(a.document_path.as_deref(), Some("/projects/hero.drift"));
    }

    #[test]
    fn test_action_mark_dirty() {
        let mut a = app();
        assert!(!a.document_dirty);
        a.apply(Action::MarkDirty);
        assert!(a.document_dirty);
    }

    #[test]
    fn test_record_history_marks_dirty() {
        let mut a = app();
        a.record_history("Add Keyframe");
        assert!(a.document_dirty);
        assert_eq!(a.history.cursor, 1);
        assert_eq!(a.history.undo_label(), Some("Add Keyframe"));
    }
}
