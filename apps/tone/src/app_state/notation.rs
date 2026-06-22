//! Notation domain — staves, clef, key signature, transpose, PDF export.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum NotationClef {
    Treble,
    Bass,
    Alto,
    Tenor,
    Percussion,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StemDir {
    Up,
    Down,
    Auto,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NoteHead {
    Normal,
    X,
    Triangle,
    Diamond,
}

#[derive(Clone, Debug)]
pub struct NotationStaff {
    pub id: usize,
    pub track_id: usize,
    pub clef: NotationClef,
    pub key_sig: i8,
    pub transpose_semitones: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PdfExportStatus {
    Idle,
    Exporting,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct NotationExport {
    pub id: usize,
    pub path: String,
    pub status: PdfExportStatus,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_notation(&mut self, action: Action) {
        match action {
            Action::OpenNotationView { track_id } => {
                let id = self.next_notation_staff_id;
                self.next_notation_staff_id += 1;
                self.notation_staves.push(NotationStaff {
                    id,
                    track_id,
                    clef: NotationClef::Treble,
                    key_sig: 0,
                    transpose_semitones: 0,
                });
            }
            Action::CloseNotationView { staff_id } => {
                self.notation_staves.retain(|s| s.id != staff_id);
            }
            Action::SetStaffClef { staff_id, clef } => {
                if let Some(staff) = self.notation_staves.iter_mut().find(|s| s.id == staff_id) {
                    staff.clef = clef;
                }
            }
            Action::SetStaffKeySig { staff_id, key_sig } => {
                if let Some(staff) = self.notation_staves.iter_mut().find(|s| s.id == staff_id) {
                    staff.key_sig = key_sig.clamp(-7i8, 7i8);
                }
            }
            Action::SetStaffTranspose { staff_id, semitones } => {
                if let Some(staff) = self.notation_staves.iter_mut().find(|s| s.id == staff_id) {
                    staff.transpose_semitones = semitones;
                }
            }
            Action::ExportNotationPdf { path } => {
                let id = self.next_notation_export_id;
                self.next_notation_export_id += 1;
                self.notation_exports.push(NotationExport {
                    id,
                    path,
                    status: PdfExportStatus::Exporting,
                });
            }
            Action::CompleteNotationExport { export_id } => {
                if let Some(exp) = self.notation_exports.iter_mut().find(|e| e.id == export_id) {
                    exp.status = PdfExportStatus::Done;
                }
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
    fn open_notation_view_creates_staff() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        assert_eq!(app.notation_staves.len(), 1);
        assert_eq!(app.notation_staves[0].track_id, 1);
        assert_eq!(app.notation_staves[0].clef, NotationClef::Treble);
    }

    #[test]
    fn open_notation_view_unique_ids() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        app.apply(Action::OpenNotationView { track_id: 2 });
        assert_ne!(app.notation_staves[0].id, app.notation_staves[1].id);
    }

    #[test]
    fn close_notation_view_removes_staff() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        let sid = app.notation_staves[0].id;
        app.apply(Action::CloseNotationView { staff_id: sid });
        assert!(app.notation_staves.is_empty());
    }

    #[test]
    fn set_staff_clef_bass() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        let sid = app.notation_staves[0].id;
        app.apply(Action::SetStaffClef { staff_id: sid, clef: NotationClef::Bass });
        assert_eq!(app.notation_staves[0].clef, NotationClef::Bass);
    }

    #[test]
    fn set_staff_key_sig_clamped() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        let sid = app.notation_staves[0].id;
        app.apply(Action::SetStaffKeySig { staff_id: sid, key_sig: 10 });
        assert_eq!(app.notation_staves[0].key_sig, 7);
    }

    #[test]
    fn set_staff_key_sig_negative_clamped() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        let sid = app.notation_staves[0].id;
        app.apply(Action::SetStaffKeySig { staff_id: sid, key_sig: -10 });
        assert_eq!(app.notation_staves[0].key_sig, -7);
    }

    #[test]
    fn set_staff_transpose() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 1 });
        let sid = app.notation_staves[0].id;
        app.apply(Action::SetStaffTranspose { staff_id: sid, semitones: -5 });
        assert_eq!(app.notation_staves[0].transpose_semitones, -5);
    }

    #[test]
    fn export_notation_pdf_creates_export_exporting() {
        let mut app = fresh();
        app.apply(Action::ExportNotationPdf { path: "/tmp/score.pdf".to_string() });
        assert_eq!(app.notation_exports.len(), 1);
        assert_eq!(app.notation_exports[0].status, PdfExportStatus::Exporting);
    }

    #[test]
    fn complete_notation_export_sets_done() {
        let mut app = fresh();
        app.apply(Action::ExportNotationPdf { path: "/tmp/score.pdf".to_string() });
        let eid = app.notation_exports[0].id;
        app.apply(Action::CompleteNotationExport { export_id: eid });
        assert_eq!(app.notation_exports[0].status, PdfExportStatus::Done);
    }

    #[test]
    fn notation_staff_defaults() {
        let mut app = fresh();
        app.apply(Action::OpenNotationView { track_id: 5 });
        assert_eq!(app.notation_staves[0].key_sig, 0);
        assert_eq!(app.notation_staves[0].transpose_semitones, 0);
    }
}
