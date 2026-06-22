//! Demucs stem-splitting domain for Tone.
//!
//! Manages the queue of Demucs stem-split jobs: source-separation of audio
//! clips into drums, bass, other, and vocals tracks via a (stubbed) ONNX
//! Demucs model.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum DemucsStatus {
    Queued,
    Splitting,
    Done,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StemKind {
    Drums,
    Bass,
    Other,
    Vocals,
}

#[derive(Clone, Debug)]
pub struct StemOutput {
    pub kind: StemKind,
    pub track_id: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct DemucsJob {
    pub id: usize,
    pub source_clip_id: usize,
    pub status: DemucsStatus,
    pub progress_pct: f32,
    pub stems: Vec<StemOutput>,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_demucs(&mut self, action: Action) {
        match &action {
            Action::QueueStemSplit { clip_id } => {
                let id = self.next_demucs_id;
                self.next_demucs_id += 1;
                self.demucs_jobs.push(DemucsJob {
                    id,
                    source_clip_id: *clip_id,
                    status: DemucsStatus::Queued,
                    progress_pct: 0.0,
                    stems: vec![
                        StemOutput { kind: StemKind::Drums, track_id: None },
                        StemOutput { kind: StemKind::Bass, track_id: None },
                        StemOutput { kind: StemKind::Other, track_id: None },
                        StemOutput { kind: StemKind::Vocals, track_id: None },
                    ],
                });
            }
            Action::StartStemSplit { job_id } => {
                if let Some(job) = self.demucs_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = DemucsStatus::Splitting;
                }
            }
            Action::UpdateStemSplitProgress { job_id, pct } => {
                if let Some(job) = self.demucs_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.progress_pct = pct.clamp(0.0, 1.0);
                }
            }
            Action::CompleteStemSplit { job_id, stem_track_ids } => {
                if let Some(job) = self.demucs_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = DemucsStatus::Done;
                    for (i, stem) in job.stems.iter_mut().enumerate() {
                        stem.track_id = stem_track_ids.get(i).copied();
                    }
                }
            }
            Action::CancelStemSplit { job_id } => {
                self.demucs_jobs.retain(|j| j.id != *job_id);
            }
            _ => {}
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn queue_stem_split_creates_job() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 10 });
        assert_eq!(app.demucs_jobs.len(), 1);
        assert_eq!(app.demucs_jobs[0].source_clip_id, 10);
    }

    #[test]
    fn queue_stem_split_starts_queued() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 5 });
        assert_eq!(app.demucs_jobs[0].status, DemucsStatus::Queued);
        assert!((app.demucs_jobs[0].progress_pct - 0.0).abs() < 0.001);
    }

    #[test]
    fn queue_stem_split_has_four_stems() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 1 });
        let stems = &app.demucs_jobs[0].stems;
        assert_eq!(stems.len(), 4);
        assert_eq!(stems[0].kind, StemKind::Drums);
        assert_eq!(stems[1].kind, StemKind::Bass);
        assert_eq!(stems[2].kind, StemKind::Other);
        assert_eq!(stems[3].kind, StemKind::Vocals);
    }

    #[test]
    fn queue_stem_split_stems_have_no_track_ids() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 2 });
        for stem in &app.demucs_jobs[0].stems {
            assert!(stem.track_id.is_none());
        }
    }

    #[test]
    fn start_stem_split_sets_splitting() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 3 });
        let id = app.demucs_jobs[0].id;
        app.apply(Action::StartStemSplit { job_id: id });
        assert_eq!(app.demucs_jobs[0].status, DemucsStatus::Splitting);
    }

    #[test]
    fn update_stem_split_progress_clamped() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 4 });
        let id = app.demucs_jobs[0].id;
        app.apply(Action::UpdateStemSplitProgress { job_id: id, pct: 1.5 });
        assert!((app.demucs_jobs[0].progress_pct - 1.0).abs() < 0.001);
        app.apply(Action::UpdateStemSplitProgress { job_id: id, pct: -0.5 });
        assert!((app.demucs_jobs[0].progress_pct - 0.0).abs() < 0.001);
    }

    #[test]
    fn complete_stem_split_sets_done_and_assigns_tracks() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 7 });
        let id = app.demucs_jobs[0].id;
        app.apply(Action::StartStemSplit { job_id: id });
        app.apply(Action::CompleteStemSplit {
            job_id: id,
            stem_track_ids: vec![100, 101, 102, 103],
        });
        let job = &app.demucs_jobs[0];
        assert_eq!(job.status, DemucsStatus::Done);
        assert_eq!(job.stems[0].track_id, Some(100));
        assert_eq!(job.stems[1].track_id, Some(101));
        assert_eq!(job.stems[2].track_id, Some(102));
        assert_eq!(job.stems[3].track_id, Some(103));
    }

    #[test]
    fn cancel_stem_split_removes_job() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 8 });
        let id = app.demucs_jobs[0].id;
        app.apply(Action::CancelStemSplit { job_id: id });
        assert!(app.demucs_jobs.is_empty());
    }

    #[test]
    fn cancel_stem_split_unknown_id_noop() {
        let mut app = fresh();
        app.apply(Action::QueueStemSplit { clip_id: 9 });
        app.apply(Action::CancelStemSplit { job_id: 999 });
        assert_eq!(app.demucs_jobs.len(), 1);
    }

    #[test]
    fn next_demucs_id_starts_at_1() {
        let app = fresh();
        assert_eq!(app.next_demucs_id, 1);
    }

    #[test]
    fn demucs_jobs_empty_initially() {
        let app = fresh();
        assert!(app.demucs_jobs.is_empty());
    }
}
