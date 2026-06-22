//! Magenta AI melody/continuation/chord-voicing domain for Tone.
//!
//! Manages generation queues for three Magenta model families:
//! - MelodyRnn / MusicTransformer / PerformanceRnn — generate melodies from scratch
//! - MelodyContinue — extend an existing MIDI clip
//! - ChordVoicing — harmonise a chord progression

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum MagentaModel {
    MelodyRnn,
    MusicTransformer,
    DrumRnn,
    PerformanceRnn,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MagentaStatus {
    Idle,
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct MelodyGenJob {
    pub id: usize,
    pub track_id: usize,
    pub bars: u8,
    pub temperature: f32,
    pub model: MagentaModel,
    pub status: MagentaStatus,
    pub output_clip_id: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct MelodyContinueJob {
    pub id: usize,
    pub source_clip_id: usize,
    pub bars: u8,
    pub temperature: f32,
    pub status: MagentaStatus,
    pub output_clip_id: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ChordVoicingJob {
    pub id: usize,
    pub progression_id: usize,
    pub voices: u8,
    pub status: MagentaStatus,
    pub output_clip_id: Option<usize>,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_magenta(&mut self, action: Action) {
        match &action {
            Action::QueueMelodyGen { track_id, bars, temperature, model } => {
                let id = self.next_melody_gen_id;
                self.next_melody_gen_id += 1;
                self.melody_gen_jobs.push(MelodyGenJob {
                    id,
                    track_id: *track_id,
                    bars: *bars,
                    temperature: *temperature,
                    model: model.clone(),
                    status: MagentaStatus::Idle,
                    output_clip_id: None,
                });
            }
            Action::StartMelodyGen { job_id } => {
                if let Some(job) = self.melody_gen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MagentaStatus::Running;
                }
            }
            Action::CompleteMelodyGen { job_id, clip_id } => {
                if let Some(job) = self.melody_gen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MagentaStatus::Done;
                    job.output_clip_id = Some(*clip_id);
                }
            }
            Action::CancelMelodyGen { job_id } => {
                self.melody_gen_jobs.retain(|j| j.id != *job_id);
            }
            Action::QueueMelodyContinue { clip_id, bars, temperature } => {
                let id = self.next_melody_cont_id;
                self.next_melody_cont_id += 1;
                self.melody_cont_jobs.push(MelodyContinueJob {
                    id,
                    source_clip_id: *clip_id,
                    bars: *bars,
                    temperature: *temperature,
                    status: MagentaStatus::Idle,
                    output_clip_id: None,
                });
            }
            Action::CompleteMelodyContinue { job_id, clip_id } => {
                if let Some(job) = self.melody_cont_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MagentaStatus::Done;
                    job.output_clip_id = Some(*clip_id);
                }
            }
            Action::QueueChordVoicing { progression_id, voices } => {
                let id = self.next_chord_voicing_id;
                self.next_chord_voicing_id += 1;
                self.chord_voicing_jobs.push(ChordVoicingJob {
                    id,
                    progression_id: *progression_id,
                    voices: *voices,
                    status: MagentaStatus::Idle,
                    output_clip_id: None,
                });
            }
            Action::CompleteChordVoicing { job_id, clip_id } => {
                if let Some(job) = self.chord_voicing_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MagentaStatus::Done;
                    job.output_clip_id = Some(*clip_id);
                }
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
    fn queue_melody_gen_creates_job() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyGen {
            track_id: 1,
            bars: 8,
            temperature: 1.0,
            model: MagentaModel::MelodyRnn,
        });
        assert_eq!(app.melody_gen_jobs.len(), 1);
        assert_eq!(app.melody_gen_jobs[0].track_id, 1);
        assert_eq!(app.melody_gen_jobs[0].model, MagentaModel::MelodyRnn);
    }

    #[test]
    fn queue_melody_gen_idle_initially() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyGen {
            track_id: 2,
            bars: 4,
            temperature: 0.9,
            model: MagentaModel::MusicTransformer,
        });
        assert_eq!(app.melody_gen_jobs[0].status, MagentaStatus::Idle);
        assert!(app.melody_gen_jobs[0].output_clip_id.is_none());
    }

    #[test]
    fn start_melody_gen_sets_running() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyGen {
            track_id: 3,
            bars: 8,
            temperature: 1.0,
            model: MagentaModel::PerformanceRnn,
        });
        let id = app.melody_gen_jobs[0].id;
        app.apply(Action::StartMelodyGen { job_id: id });
        assert_eq!(app.melody_gen_jobs[0].status, MagentaStatus::Running);
    }

    #[test]
    fn complete_melody_gen_sets_done() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyGen {
            track_id: 4,
            bars: 16,
            temperature: 0.8,
            model: MagentaModel::DrumRnn,
        });
        let id = app.melody_gen_jobs[0].id;
        app.apply(Action::StartMelodyGen { job_id: id });
        app.apply(Action::CompleteMelodyGen { job_id: id, clip_id: 55 });
        assert_eq!(app.melody_gen_jobs[0].status, MagentaStatus::Done);
        assert_eq!(app.melody_gen_jobs[0].output_clip_id, Some(55));
    }

    #[test]
    fn cancel_melody_gen_removes_job() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyGen {
            track_id: 5,
            bars: 8,
            temperature: 1.0,
            model: MagentaModel::MelodyRnn,
        });
        let id = app.melody_gen_jobs[0].id;
        app.apply(Action::CancelMelodyGen { job_id: id });
        assert!(app.melody_gen_jobs.is_empty());
    }

    #[test]
    fn queue_melody_continue_creates_job() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyContinue {
            clip_id: 20,
            bars: 4,
            temperature: 0.95,
        });
        assert_eq!(app.melody_cont_jobs.len(), 1);
        assert_eq!(app.melody_cont_jobs[0].source_clip_id, 20);
        assert_eq!(app.melody_cont_jobs[0].status, MagentaStatus::Idle);
    }

    #[test]
    fn complete_melody_continue_sets_done() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyContinue {
            clip_id: 30,
            bars: 8,
            temperature: 1.0,
        });
        let id = app.melody_cont_jobs[0].id;
        app.apply(Action::CompleteMelodyContinue { job_id: id, clip_id: 77 });
        assert_eq!(app.melody_cont_jobs[0].status, MagentaStatus::Done);
        assert_eq!(app.melody_cont_jobs[0].output_clip_id, Some(77));
    }

    #[test]
    fn queue_chord_voicing_creates_job() {
        let mut app = fresh();
        app.apply(Action::QueueChordVoicing { progression_id: 3, voices: 4 });
        assert_eq!(app.chord_voicing_jobs.len(), 1);
        assert_eq!(app.chord_voicing_jobs[0].progression_id, 3);
        assert_eq!(app.chord_voicing_jobs[0].voices, 4);
        assert_eq!(app.chord_voicing_jobs[0].status, MagentaStatus::Idle);
    }

    #[test]
    fn complete_chord_voicing_sets_done() {
        let mut app = fresh();
        app.apply(Action::QueueChordVoicing { progression_id: 5, voices: 3 });
        let id = app.chord_voicing_jobs[0].id;
        app.apply(Action::CompleteChordVoicing { job_id: id, clip_id: 99 });
        assert_eq!(app.chord_voicing_jobs[0].status, MagentaStatus::Done);
        assert_eq!(app.chord_voicing_jobs[0].output_clip_id, Some(99));
    }

    #[test]
    fn melody_gen_ids_increment() {
        let mut app = fresh();
        for i in 0..3 {
            app.apply(Action::QueueMelodyGen {
                track_id: i,
                bars: 8,
                temperature: 1.0,
                model: MagentaModel::MelodyRnn,
            });
        }
        assert_eq!(app.melody_gen_jobs[0].id, 1);
        assert_eq!(app.melody_gen_jobs[1].id, 2);
        assert_eq!(app.melody_gen_jobs[2].id, 3);
    }

    #[test]
    fn all_job_vecs_empty_initially() {
        let app = fresh();
        assert!(app.melody_gen_jobs.is_empty());
        assert!(app.melody_cont_jobs.is_empty());
        assert!(app.chord_voicing_jobs.is_empty());
    }

    #[test]
    fn melody_cont_and_chord_voicing_independent() {
        let mut app = fresh();
        app.apply(Action::QueueMelodyContinue { clip_id: 10, bars: 4, temperature: 1.0 });
        app.apply(Action::QueueChordVoicing { progression_id: 1, voices: 4 });
        assert_eq!(app.melody_cont_jobs.len(), 1);
        assert_eq!(app.chord_voicing_jobs.len(), 1);
        // Completing chord voicing doesn't affect melody continue
        let cv_id = app.chord_voicing_jobs[0].id;
        app.apply(Action::CompleteChordVoicing { job_id: cv_id, clip_id: 50 });
        assert_eq!(app.melody_cont_jobs[0].status, MagentaStatus::Idle);
    }
}
