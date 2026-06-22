//! Vocal tools domain — auto-tune, harmony generation, vocal isolation.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum VocalJobKind {
    AutoTune,
    Harmony,
    IsolateVocals,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VocalJobStatus {
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct AutoTuneConfig {
    pub key: u8,
    pub scale_mask: u16,
    pub strength: f32,
    pub formant_shift: f32,
    pub speed: f32,
}

impl Default for AutoTuneConfig {
    fn default() -> Self {
        Self {
            key: 0,
            scale_mask: 0b101011010101,
            strength: 1.0,
            formant_shift: 0.0,
            speed: 0.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HarmonyConfig {
    pub voices: u8,
    pub intervals: Vec<i8>,
    pub formant_preserve: bool,
}

#[derive(Clone, Debug)]
pub struct VocalJob {
    pub id: usize,
    pub kind: VocalJobKind,
    pub clip_id: usize,
    pub status: VocalJobStatus,
    pub output_clip_ids: Vec<usize>,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_vocal_tools(&mut self, action: &Action) {
        match action {
            Action::ApplyAutoTune { clip_id, config } => {
                let id = self.next_vocal_job_id;
                self.next_vocal_job_id += 1;
                self.vocal_jobs.push(VocalJob {
                    id,
                    kind: VocalJobKind::AutoTune,
                    clip_id: *clip_id,
                    status: VocalJobStatus::Queued,
                    output_clip_ids: vec![],
                });
                self.auto_tune_configs.push((*clip_id, config.clone()));
            }
            Action::GenVocalHarmony { clip_id, .. } => {
                let id = self.next_vocal_job_id;
                self.next_vocal_job_id += 1;
                self.vocal_jobs.push(VocalJob {
                    id,
                    kind: VocalJobKind::Harmony,
                    clip_id: *clip_id,
                    status: VocalJobStatus::Queued,
                    output_clip_ids: vec![],
                });
            }
            Action::IsolateVocals { clip_id } => {
                let id = self.next_vocal_job_id;
                self.next_vocal_job_id += 1;
                self.vocal_jobs.push(VocalJob {
                    id,
                    kind: VocalJobKind::IsolateVocals,
                    clip_id: *clip_id,
                    status: VocalJobStatus::Queued,
                    output_clip_ids: vec![],
                });
            }
            Action::StartVocalJob { job_id } => {
                if let Some(job) = self.vocal_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = VocalJobStatus::Running;
                }
            }
            Action::CompleteVocalJob { job_id, output_clip_ids } => {
                if let Some(job) = self.vocal_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = VocalJobStatus::Done;
                    job.output_clip_ids = output_clip_ids.clone();
                }
            }
            Action::CancelVocalJob { job_id } => {
                self.vocal_jobs.retain(|j| j.id != *job_id);
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
    fn apply_autotune_queues_job() {
        let mut app = fresh();
        let cfg = AutoTuneConfig::default();
        app.apply(Action::ApplyAutoTune { clip_id: 10, config: cfg });
        assert_eq!(app.vocal_jobs.len(), 1);
        assert_eq!(app.vocal_jobs[0].kind, VocalJobKind::AutoTune);
        assert_eq!(app.vocal_jobs[0].clip_id, 10);
        assert_eq!(app.vocal_jobs[0].status, VocalJobStatus::Queued);
    }

    #[test]
    fn apply_autotune_stores_config() {
        let mut app = fresh();
        let cfg = AutoTuneConfig { key: 5, scale_mask: 0xFFF, strength: 0.8, formant_shift: 0.2, speed: 0.3 };
        app.apply(Action::ApplyAutoTune { clip_id: 3, config: cfg });
        assert_eq!(app.auto_tune_configs.len(), 1);
        assert_eq!(app.auto_tune_configs[0].0, 3);
        assert!((app.auto_tune_configs[0].1.strength - 0.8).abs() < 0.001);
    }

    #[test]
    fn gen_vocal_harmony_queues_job() {
        let mut app = fresh();
        let cfg = HarmonyConfig { voices: 3, intervals: vec![4, 7], formant_preserve: true };
        app.apply(Action::GenVocalHarmony { clip_id: 20, config: cfg });
        assert_eq!(app.vocal_jobs.len(), 1);
        assert_eq!(app.vocal_jobs[0].kind, VocalJobKind::Harmony);
        assert_eq!(app.vocal_jobs[0].clip_id, 20);
        assert_eq!(app.vocal_jobs[0].status, VocalJobStatus::Queued);
    }

    #[test]
    fn isolate_vocals_queues_job() {
        let mut app = fresh();
        app.apply(Action::IsolateVocals { clip_id: 42 });
        assert_eq!(app.vocal_jobs.len(), 1);
        assert_eq!(app.vocal_jobs[0].kind, VocalJobKind::IsolateVocals);
        assert_eq!(app.vocal_jobs[0].clip_id, 42);
        assert_eq!(app.vocal_jobs[0].status, VocalJobStatus::Queued);
    }

    #[test]
    fn start_vocal_job_sets_running() {
        let mut app = fresh();
        app.apply(Action::IsolateVocals { clip_id: 1 });
        let id = app.vocal_jobs[0].id;
        app.apply(Action::StartVocalJob { job_id: id });
        assert_eq!(app.vocal_jobs[0].status, VocalJobStatus::Running);
    }

    #[test]
    fn complete_vocal_job_sets_done_and_output_ids() {
        let mut app = fresh();
        app.apply(Action::IsolateVocals { clip_id: 1 });
        let id = app.vocal_jobs[0].id;
        app.apply(Action::CompleteVocalJob { job_id: id, output_clip_ids: vec![100, 101] });
        assert_eq!(app.vocal_jobs[0].status, VocalJobStatus::Done);
        assert_eq!(app.vocal_jobs[0].output_clip_ids, vec![100, 101]);
    }

    #[test]
    fn cancel_vocal_job_removes_it() {
        let mut app = fresh();
        app.apply(Action::IsolateVocals { clip_id: 1 });
        app.apply(Action::IsolateVocals { clip_id: 2 });
        let id = app.vocal_jobs[0].id;
        app.apply(Action::CancelVocalJob { job_id: id });
        assert_eq!(app.vocal_jobs.len(), 1);
        assert_eq!(app.vocal_jobs[0].clip_id, 2);
    }

    #[test]
    fn multiple_job_kinds_get_unique_ids() {
        let mut app = fresh();
        app.apply(Action::ApplyAutoTune { clip_id: 1, config: AutoTuneConfig::default() });
        app.apply(Action::GenVocalHarmony { clip_id: 2, config: HarmonyConfig { voices: 2, intervals: vec![3], formant_preserve: false } });
        app.apply(Action::IsolateVocals { clip_id: 3 });
        let ids: Vec<usize> = app.vocal_jobs.iter().map(|j| j.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn autotune_config_defaults() {
        let cfg = AutoTuneConfig::default();
        assert_eq!(cfg.key, 0);
        assert_eq!(cfg.scale_mask, 0b101011010101);
        assert!((cfg.strength - 1.0).abs() < 0.001);
        assert!((cfg.formant_shift).abs() < 0.001);
        assert!((cfg.speed - 0.5).abs() < 0.001);
    }

    #[test]
    fn start_nonexistent_vocal_job_noop() {
        let mut app = fresh();
        app.apply(Action::StartVocalJob { job_id: 999 });
        assert!(app.vocal_jobs.is_empty());
    }

    #[test]
    fn autotune_job_no_output_initially() {
        let mut app = fresh();
        app.apply(Action::ApplyAutoTune { clip_id: 5, config: AutoTuneConfig::default() });
        assert!(app.vocal_jobs[0].output_clip_ids.is_empty());
    }

    #[test]
    fn vocal_job_counter_increments_across_kinds() {
        let mut app = fresh();
        app.apply(Action::IsolateVocals { clip_id: 1 });
        assert_eq!(app.next_vocal_job_id, 2);
        app.apply(Action::IsolateVocals { clip_id: 2 });
        assert_eq!(app.next_vocal_job_id, 3);
    }
}
