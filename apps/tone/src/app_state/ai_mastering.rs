//! AI mastering domain — loudness targeting, multiband compression, A/B preview.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum LufsTarget {
    SpotifyStream,
    YouTubeStream,
    CdLoud,
    Custom(i32),
}

impl LufsTarget {
    pub fn target_lufs(&self) -> i32 {
        match self {
            Self::SpotifyStream => -14,
            Self::YouTubeStream => -14,
            Self::CdLoud => -9,
            Self::Custom(v) => *v,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AiMasterStatus {
    Idle,
    Analysing,
    Processing,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct MultibandComp {
    pub low_threshold: f32,
    pub mid_threshold: f32,
    pub high_threshold: f32,
    pub ratio: f32,
}

impl Default for MultibandComp {
    fn default() -> Self {
        Self {
            low_threshold: -20.0,
            mid_threshold: -18.0,
            high_threshold: -16.0,
            ratio: 3.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiMasterJob {
    pub id: usize,
    pub target: LufsTarget,
    pub mb_comp: MultibandComp,
    pub status: AiMasterStatus,
    pub input_lufs: f32,
    pub output_lufs: f32,
    pub ab_preview: bool,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_ai_mastering(&mut self, action: &Action) {
        match action {
            Action::QueueAiMaster { target } => {
                let id = self.next_aimaster_id;
                self.next_aimaster_id += 1;
                self.aimaster_jobs.push(AiMasterJob {
                    id,
                    target: target.clone(),
                    mb_comp: MultibandComp::default(),
                    status: AiMasterStatus::Idle,
                    input_lufs: -20.0,
                    output_lufs: -20.0,
                    ab_preview: false,
                });
            }
            Action::StartAiMaster { job_id } => {
                if let Some(job) = self.aimaster_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = AiMasterStatus::Analysing;
                }
            }
            Action::UpdateAiMasterProgress { job_id, input_lufs } => {
                if let Some(job) = self.aimaster_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = AiMasterStatus::Processing;
                    job.input_lufs = *input_lufs;
                }
            }
            Action::CompleteAiMaster { job_id, output_lufs } => {
                if let Some(job) = self.aimaster_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = AiMasterStatus::Done;
                    job.output_lufs = *output_lufs;
                }
            }
            Action::ToggleAiMasterAB { job_id } => {
                if let Some(job) = self.aimaster_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.ab_preview = !job.ab_preview;
                }
            }
            Action::SetAiMasterTarget { job_id, target } => {
                if let Some(job) = self.aimaster_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.target = target.clone();
                }
            }
            Action::CancelAiMaster { job_id } => {
                self.aimaster_jobs.retain(|j| j.id != *job_id);
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
    fn lufs_target_spotify() {
        assert_eq!(LufsTarget::SpotifyStream.target_lufs(), -14);
    }

    #[test]
    fn lufs_target_youtube() {
        assert_eq!(LufsTarget::YouTubeStream.target_lufs(), -14);
    }

    #[test]
    fn lufs_target_cd() {
        assert_eq!(LufsTarget::CdLoud.target_lufs(), -9);
    }

    #[test]
    fn lufs_target_custom() {
        assert_eq!(LufsTarget::Custom(-18).target_lufs(), -18);
    }

    #[test]
    fn queue_ai_master_job() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::SpotifyStream });
        assert_eq!(app.aimaster_jobs.len(), 1);
        assert_eq!(app.aimaster_jobs[0].status, AiMasterStatus::Idle);
        assert_eq!(app.aimaster_jobs[0].target, LufsTarget::SpotifyStream);
        assert_eq!(app.next_aimaster_id, 2);
    }

    #[test]
    fn start_ai_master_sets_analysing() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::CdLoud });
        let id = app.aimaster_jobs[0].id;
        app.apply(Action::StartAiMaster { job_id: id });
        assert_eq!(app.aimaster_jobs[0].status, AiMasterStatus::Analysing);
    }

    #[test]
    fn update_progress_sets_processing_and_lufs() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::YouTubeStream });
        let id = app.aimaster_jobs[0].id;
        app.apply(Action::UpdateAiMasterProgress { job_id: id, input_lufs: -22.5 });
        assert_eq!(app.aimaster_jobs[0].status, AiMasterStatus::Processing);
        assert!((app.aimaster_jobs[0].input_lufs - -22.5).abs() < 0.001);
    }

    #[test]
    fn complete_ai_master_sets_done_and_output_lufs() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::SpotifyStream });
        let id = app.aimaster_jobs[0].id;
        app.apply(Action::CompleteAiMaster { job_id: id, output_lufs: -14.0 });
        assert_eq!(app.aimaster_jobs[0].status, AiMasterStatus::Done);
        assert!((app.aimaster_jobs[0].output_lufs - -14.0).abs() < 0.001);
    }

    #[test]
    fn toggle_ab_preview() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::CdLoud });
        let id = app.aimaster_jobs[0].id;
        assert!(!app.aimaster_jobs[0].ab_preview);
        app.apply(Action::ToggleAiMasterAB { job_id: id });
        assert!(app.aimaster_jobs[0].ab_preview);
        app.apply(Action::ToggleAiMasterAB { job_id: id });
        assert!(!app.aimaster_jobs[0].ab_preview);
    }

    #[test]
    fn set_ai_master_target() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::SpotifyStream });
        let id = app.aimaster_jobs[0].id;
        app.apply(Action::SetAiMasterTarget { job_id: id, target: LufsTarget::Custom(-16) });
        assert_eq!(app.aimaster_jobs[0].target, LufsTarget::Custom(-16));
    }

    #[test]
    fn cancel_ai_master_removes_job() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::SpotifyStream });
        app.apply(Action::QueueAiMaster { target: LufsTarget::CdLoud });
        assert_eq!(app.aimaster_jobs.len(), 2);
        let id = app.aimaster_jobs[0].id;
        app.apply(Action::CancelAiMaster { job_id: id });
        assert_eq!(app.aimaster_jobs.len(), 1);
        assert_eq!(app.aimaster_jobs[0].target, LufsTarget::CdLoud);
    }

    #[test]
    fn multiband_comp_defaults() {
        let mb = MultibandComp::default();
        assert!((mb.low_threshold - -20.0).abs() < 0.001);
        assert!((mb.mid_threshold - -18.0).abs() < 0.001);
        assert!((mb.high_threshold - -16.0).abs() < 0.001);
        assert!((mb.ratio - 3.0).abs() < 0.001);
    }

    #[test]
    fn queued_job_has_default_mb_comp() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::SpotifyStream });
        let job = &app.aimaster_jobs[0];
        assert!((job.mb_comp.ratio - 3.0).abs() < 0.001);
    }

    #[test]
    fn multiple_jobs_get_unique_ids() {
        let mut app = fresh();
        app.apply(Action::QueueAiMaster { target: LufsTarget::SpotifyStream });
        app.apply(Action::QueueAiMaster { target: LufsTarget::CdLoud });
        app.apply(Action::QueueAiMaster { target: LufsTarget::Custom(-12) });
        let ids: Vec<usize> = app.aimaster_jobs.iter().map(|j| j.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn start_nonexistent_job_noop() {
        let mut app = fresh();
        app.apply(Action::StartAiMaster { job_id: 999 });
        assert!(app.aimaster_jobs.is_empty());
    }
}
