//! MusicGen AI generation domain for Tone.
//!
//! Manages the queue of MusicGen jobs: prompt-based full-track generation
//! via a (stubbed) ONNX MusicGen model.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum MusicGenStatus {
    Idle,
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct MusicGenJob {
    pub id: usize,
    pub prompt: String,
    pub style_tag: String,
    pub bars: u8,
    pub temperature: f32,
    pub status: MusicGenStatus,
    pub progress_bars: u8,
    pub output_track_id: Option<usize>,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_musicgen(&mut self, action: Action) {
        match &action {
            Action::QueueMusicGen { prompt, style_tag, bars, temperature } => {
                let id = self.next_musicgen_id;
                self.next_musicgen_id += 1;
                self.musicgen_jobs.push(MusicGenJob {
                    id,
                    prompt: prompt.clone(),
                    style_tag: style_tag.clone(),
                    bars: *bars,
                    temperature: *temperature,
                    status: MusicGenStatus::Idle,
                    progress_bars: 0,
                    output_track_id: None,
                });
            }
            Action::StartMusicGen { job_id } => {
                if let Some(job) = self.musicgen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MusicGenStatus::Running;
                }
            }
            Action::UpdateMusicGenProgress { job_id, bars_done } => {
                if let Some(job) = self.musicgen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.progress_bars = *bars_done;
                }
            }
            Action::CompleteMusicGen { job_id, track_id } => {
                if let Some(job) = self.musicgen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MusicGenStatus::Done;
                    job.output_track_id = Some(*track_id);
                }
            }
            Action::CancelMusicGen { job_id } => {
                self.musicgen_jobs.retain(|j| j.id != *job_id);
            }
            Action::RetryMusicGen { job_id } => {
                if let Some(job) = self.musicgen_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = MusicGenStatus::Idle;
                    job.progress_bars = 0;
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
    fn queue_musicgen_increments_id() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "chill lo-fi beat".into(),
            style_tag: "lofi".into(),
            bars: 8,
            temperature: 1.0,
        });
        app.apply(Action::QueueMusicGen {
            prompt: "energetic techno drop".into(),
            style_tag: "techno".into(),
            bars: 4,
            temperature: 0.9,
        });
        assert_eq!(app.musicgen_jobs.len(), 2);
        assert_eq!(app.musicgen_jobs[0].id, 1);
        assert_eq!(app.musicgen_jobs[1].id, 2);
    }

    #[test]
    fn queue_musicgen_idle_initially() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "ambient pad".into(),
            style_tag: "ambient".into(),
            bars: 16,
            temperature: 0.8,
        });
        assert_eq!(app.musicgen_jobs[0].status, MusicGenStatus::Idle);
        assert_eq!(app.musicgen_jobs[0].progress_bars, 0);
        assert!(app.musicgen_jobs[0].output_track_id.is_none());
    }

    #[test]
    fn start_musicgen_sets_running() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "drum groove".into(),
            style_tag: "funk".into(),
            bars: 8,
            temperature: 1.1,
        });
        let id = app.musicgen_jobs[0].id;
        app.apply(Action::StartMusicGen { job_id: id });
        assert_eq!(app.musicgen_jobs[0].status, MusicGenStatus::Running);
    }

    #[test]
    fn start_musicgen_unknown_id_noop() {
        let mut app = fresh();
        app.apply(Action::StartMusicGen { job_id: 999 });
        assert!(app.musicgen_jobs.is_empty());
    }

    #[test]
    fn update_musicgen_progress() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "jazz piano".into(),
            style_tag: "jazz".into(),
            bars: 4,
            temperature: 0.7,
        });
        let id = app.musicgen_jobs[0].id;
        app.apply(Action::StartMusicGen { job_id: id });
        app.apply(Action::UpdateMusicGenProgress { job_id: id, bars_done: 2 });
        assert_eq!(app.musicgen_jobs[0].progress_bars, 2);
    }

    #[test]
    fn complete_musicgen_sets_done_and_track_id() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "orchestral strings".into(),
            style_tag: "cinematic".into(),
            bars: 32,
            temperature: 0.6,
        });
        let id = app.musicgen_jobs[0].id;
        app.apply(Action::StartMusicGen { job_id: id });
        app.apply(Action::CompleteMusicGen { job_id: id, track_id: 42 });
        assert_eq!(app.musicgen_jobs[0].status, MusicGenStatus::Done);
        assert_eq!(app.musicgen_jobs[0].output_track_id, Some(42));
    }

    #[test]
    fn cancel_musicgen_removes_job() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "house beat".into(),
            style_tag: "house".into(),
            bars: 8,
            temperature: 1.0,
        });
        let id = app.musicgen_jobs[0].id;
        app.apply(Action::CancelMusicGen { job_id: id });
        assert!(app.musicgen_jobs.is_empty());
    }

    #[test]
    fn cancel_musicgen_unknown_id_noop() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "reggae skank".into(),
            style_tag: "reggae".into(),
            bars: 8,
            temperature: 1.0,
        });
        app.apply(Action::CancelMusicGen { job_id: 999 });
        assert_eq!(app.musicgen_jobs.len(), 1);
    }

    #[test]
    fn retry_musicgen_resets_to_idle() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "metal riff".into(),
            style_tag: "metal".into(),
            bars: 8,
            temperature: 1.2,
        });
        let id = app.musicgen_jobs[0].id;
        app.apply(Action::StartMusicGen { job_id: id });
        app.apply(Action::UpdateMusicGenProgress { job_id: id, bars_done: 3 });
        app.apply(Action::RetryMusicGen { job_id: id });
        assert_eq!(app.musicgen_jobs[0].status, MusicGenStatus::Idle);
        assert_eq!(app.musicgen_jobs[0].progress_bars, 0);
    }

    #[test]
    fn next_musicgen_id_starts_at_1() {
        let app = fresh();
        assert_eq!(app.next_musicgen_id, 1);
    }

    #[test]
    fn musicgen_jobs_empty_initially() {
        let app = fresh();
        assert!(app.musicgen_jobs.is_empty());
    }

    #[test]
    fn queue_preserves_prompt_and_style() {
        let mut app = fresh();
        app.apply(Action::QueueMusicGen {
            prompt: "dreamy reverb pads".into(),
            style_tag: "shoegaze".into(),
            bars: 16,
            temperature: 0.85,
        });
        let job = &app.musicgen_jobs[0];
        assert_eq!(job.prompt, "dreamy reverb pads");
        assert_eq!(job.style_tag, "shoegaze");
        assert_eq!(job.bars, 16);
        assert!((job.temperature - 0.85).abs() < 0.001);
    }

    #[test]
    fn multiple_jobs_independent() {
        let mut app = fresh();
        for i in 0..3u8 {
            app.apply(Action::QueueMusicGen {
                prompt: format!("track {i}"),
                style_tag: "pop".into(),
                bars: 8,
                temperature: 1.0,
            });
        }
        assert_eq!(app.musicgen_jobs.len(), 3);
        let id1 = app.musicgen_jobs[0].id;
        app.apply(Action::StartMusicGen { job_id: id1 });
        assert_eq!(app.musicgen_jobs[0].status, MusicGenStatus::Running);
        assert_eq!(app.musicgen_jobs[1].status, MusicGenStatus::Idle);
        assert_eq!(app.musicgen_jobs[2].status, MusicGenStatus::Idle);
    }
}
