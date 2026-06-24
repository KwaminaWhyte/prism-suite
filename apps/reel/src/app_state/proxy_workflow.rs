//! **Proxy workflow.**
//!
//! A proxy-generation *job* model layered on top of the simpler `proxy.rs`
//! attachment record. A [`ProxyJob`] describes transcoding one source clip to a
//! lower-resolution substitute (½ or ¼ frame size) and tracks its lifecycle
//! ([`ProxyJobState`]: Queued → Running → Done / Failed). A global
//! **proxy/full toggle** ([`PlaybackSource`]) decides whether the editor —
//! *and the export path* — read from proxy media or the full-res original.
//!
//! No transcoding happens here: jobs are advanced deterministically by explicit
//! `start` / `complete` / `fail` calls so the whole model is unit-testable. The
//! key invariant the export path must respect is
//! [`App::export_source_for_clip`], which returns the proxy path only when the
//! toggle is on **and** a finished proxy is attached.

use std::path::PathBuf;

use super::{App, Action};

/// Proxy target frame-size relative to the source.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ProxyResolution {
    /// One-half width and height (¼ the pixels).
    #[default]
    Half,
    /// One-quarter width and height (1/16 the pixels).
    Quarter,
}

impl ProxyResolution {
    /// The linear scale factor applied to width and height.
    pub fn scale(self) -> f32 {
        match self {
            ProxyResolution::Half => 0.5,
            ProxyResolution::Quarter => 0.25,
        }
    }

    /// Scale a `(w, h)` source size to the proxy size (each dim at least 1).
    pub fn target_size(self, w: u32, h: u32) -> (u32, u32) {
        let s = self.scale();
        (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1))
    }
}

/// Lifecycle of a proxy-generation job.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ProxyJobState {
    #[default]
    Queued,
    Running,
    Done,
    Failed,
}

/// One proxy-generation job: transcode `source_clip_idx` to `proxy_path` at
/// `resolution`. The `progress` is `0.0..=1.0` while Running.
#[derive(Clone, Debug)]
pub struct ProxyJob {
    pub source_clip_idx: usize,
    pub resolution: ProxyResolution,
    pub proxy_path: PathBuf,
    pub state: ProxyJobState,
    pub progress: f32,
}

impl ProxyJob {
    pub fn new(source_clip_idx: usize, resolution: ProxyResolution, proxy_path: impl Into<PathBuf>) -> Self {
        Self {
            source_clip_idx,
            resolution,
            proxy_path: proxy_path.into(),
            state: ProxyJobState::Queued,
            progress: 0.0,
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, ProxyJobState::Done | ProxyJobState::Failed)
    }
}

/// Whether the editor + export read proxy or full-res media.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum PlaybackSource {
    #[default]
    Full,
    Proxy,
}

/// The proxy workflow state owned by `App`.
#[derive(Clone, Debug, Default)]
pub struct ProxyWorkflow {
    pub jobs: Vec<ProxyJob>,
    /// Finished proxies attached to clips: `clip_idx → proxy_path`.
    pub attached: Vec<(usize, PathBuf)>,
    pub playback_source: PlaybackSource,
    pub default_resolution: ProxyResolution,
}

impl ProxyWorkflow {
    /// Queue a proxy job for `clip_idx` (no duplicate Queued/Running job for the
    /// same clip). Returns the job index.
    pub fn queue(&mut self, clip_idx: usize, resolution: ProxyResolution, path: impl Into<PathBuf>) -> usize {
        if let Some(i) = self
            .jobs
            .iter()
            .position(|j| j.source_clip_idx == clip_idx && !j.is_finished())
        {
            return i;
        }
        self.jobs.push(ProxyJob::new(clip_idx, resolution, path));
        self.jobs.len() - 1
    }

    /// Mark a queued job Running (no-op when out of range or already finished).
    pub fn start(&mut self, job_idx: usize) {
        if let Some(j) = self.jobs.get_mut(job_idx) {
            if j.state == ProxyJobState::Queued {
                j.state = ProxyJobState::Running;
                j.progress = 0.0;
            }
        }
    }

    /// Complete a Running/Queued job → Done, attaching its proxy to the clip.
    pub fn complete(&mut self, job_idx: usize) {
        let Some(j) = self.jobs.get_mut(job_idx) else { return };
        if j.is_finished() {
            return;
        }
        j.state = ProxyJobState::Done;
        j.progress = 1.0;
        let clip_idx = j.source_clip_idx;
        let path = j.proxy_path.clone();
        self.attach(clip_idx, path);
    }

    /// Fail a job (no proxy attached).
    pub fn fail(&mut self, job_idx: usize) {
        if let Some(j) = self.jobs.get_mut(job_idx) {
            if !j.is_finished() {
                j.state = ProxyJobState::Failed;
            }
        }
    }

    /// Attach (or replace) a finished proxy path for `clip_idx`.
    pub fn attach(&mut self, clip_idx: usize, path: PathBuf) {
        if let Some(e) = self.attached.iter_mut().find(|(ci, _)| *ci == clip_idx) {
            e.1 = path;
        } else {
            self.attached.push((clip_idx, path));
        }
    }

    /// Detach the proxy from `clip_idx`. Returns `true` if one was attached.
    pub fn detach(&mut self, clip_idx: usize) -> bool {
        let before = self.attached.len();
        self.attached.retain(|(ci, _)| *ci != clip_idx);
        self.attached.len() != before
    }

    /// The attached proxy path for `clip_idx`, if any.
    pub fn proxy_path(&self, clip_idx: usize) -> Option<&PathBuf> {
        self.attached.iter().find(|(ci, _)| *ci == clip_idx).map(|(_, p)| p)
    }

    /// `true` when the toggle is set to read proxy media.
    pub fn using_proxy(&self) -> bool {
        self.playback_source == PlaybackSource::Proxy
    }

    /// The number of finished (Done) jobs.
    pub fn done_count(&self) -> usize {
        self.jobs.iter().filter(|j| j.state == ProxyJobState::Done).count()
    }
}

impl App {
    /// The media path the **export** path should read for clip `clip_idx`: the
    /// attached proxy when the toggle is on Proxy and a finished proxy exists,
    /// otherwise the full-res `full_source`. This is the single function the
    /// export pipeline calls so the proxy↔full toggle is honoured.
    pub fn export_source_for_clip(&self, clip_idx: usize, full_source: PathBuf) -> PathBuf {
        if self.proxy_workflow.using_proxy() {
            if let Some(p) = self.proxy_workflow.proxy_path(clip_idx) {
                return p.clone();
            }
        }
        full_source
    }

    /// Apply the proxy-workflow actions. Routed from `mod.rs`.
    pub(crate) fn apply_proxy_workflow(&mut self, action: Action) {
        match action {
            Action::SetProxyWorkflowResolution(r) => {
                self.proxy_workflow.default_resolution = r;
            }
            Action::QueueProxyJob { clip_idx, resolution, path } => {
                self.proxy_workflow.queue(clip_idx, resolution, path);
            }
            Action::StartProxyJob(idx) => {
                self.proxy_workflow.start(idx);
            }
            Action::CompleteProxyJob(idx) => {
                self.proxy_workflow.complete(idx);
                self.host.mark_dirty();
            }
            Action::FailProxyJob(idx) => {
                self.proxy_workflow.fail(idx);
            }
            Action::AttachProxyWorkflow { clip_idx, path } => {
                self.proxy_workflow.attach(clip_idx, path);
            }
            Action::DetachProxyWorkflow { clip_idx } => {
                self.proxy_workflow.detach(clip_idx);
            }
            Action::SetPlaybackSource(src) => {
                self.proxy_workflow.playback_source = src;
                self.host.mark_dirty();
            }
            Action::ToggleProxyFull => {
                self.proxy_workflow.playback_source = match self.proxy_workflow.playback_source {
                    PlaybackSource::Full => PlaybackSource::Proxy,
                    PlaybackSource::Proxy => PlaybackSource::Full,
                };
                self.host.mark_dirty();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn resolution_target_size() {
        assert_eq!(ProxyResolution::Half.target_size(1920, 1080), (960, 540));
        assert_eq!(ProxyResolution::Quarter.target_size(1920, 1080), (480, 270));
        // Never collapses below 1.
        assert_eq!(ProxyResolution::Quarter.target_size(1, 1), (1, 1));
    }

    #[test]
    fn queue_dedups_unfinished_jobs() {
        let mut wf = ProxyWorkflow::default();
        let a = wf.queue(0, ProxyResolution::Half, "p0.mp4");
        let b = wf.queue(0, ProxyResolution::Half, "p0b.mp4");
        assert_eq!(a, b, "duplicate unfinished job is reused");
        assert_eq!(wf.jobs.len(), 1);
        // A different clip gets its own job.
        wf.queue(1, ProxyResolution::Quarter, "p1.mp4");
        assert_eq!(wf.jobs.len(), 2);
    }

    #[test]
    fn job_lifecycle_start_complete_attaches() {
        let mut wf = ProxyWorkflow::default();
        let j = wf.queue(2, ProxyResolution::Half, "/proxies/c2.mov");
        assert_eq!(wf.jobs[j].state, ProxyJobState::Queued);
        wf.start(j);
        assert_eq!(wf.jobs[j].state, ProxyJobState::Running);
        wf.complete(j);
        assert_eq!(wf.jobs[j].state, ProxyJobState::Done);
        assert_eq!(wf.jobs[j].progress, 1.0);
        assert_eq!(wf.done_count(), 1);
        // Completing attaches the proxy to the clip.
        assert_eq!(wf.proxy_path(2), Some(&PathBuf::from("/proxies/c2.mov")));
    }

    #[test]
    fn fail_does_not_attach() {
        let mut wf = ProxyWorkflow::default();
        let j = wf.queue(3, ProxyResolution::Half, "/p/c3.mov");
        wf.start(j);
        wf.fail(j);
        assert_eq!(wf.jobs[j].state, ProxyJobState::Failed);
        assert!(wf.proxy_path(3).is_none());
        // A finished job is not re-completed.
        wf.complete(j);
        assert_eq!(wf.jobs[j].state, ProxyJobState::Failed);
    }

    #[test]
    fn detach_removes_attachment() {
        let mut wf = ProxyWorkflow::default();
        wf.attach(5, PathBuf::from("/p/5.mov"));
        assert!(wf.proxy_path(5).is_some());
        assert!(wf.detach(5));
        assert!(wf.proxy_path(5).is_none());
        // Detaching nothing → false.
        assert!(!wf.detach(5));
    }

    #[test]
    fn export_source_respects_toggle_and_attachment() {
        let mut app = App::new();
        let full = PathBuf::from("/media/full.mov");
        // No proxy queued + toggle off → full source.
        assert_eq!(app.export_source_for_clip(0, full.clone()), full);
        // Queue + complete a proxy for clip 0.
        app.apply(Action::QueueProxyJob {
            clip_idx: 0,
            resolution: ProxyResolution::Half,
            path: PathBuf::from("/media/full_proxy.mov"),
        });
        app.apply(Action::CompleteProxyJob(0));
        // Toggle still Full → export reads the full source.
        assert_eq!(app.export_source_for_clip(0, full.clone()), full);
        // Flip the toggle to Proxy → export now reads the proxy.
        app.apply(Action::ToggleProxyFull);
        assert!(app.proxy_workflow.using_proxy());
        assert_eq!(
            app.export_source_for_clip(0, full.clone()),
            PathBuf::from("/media/full_proxy.mov")
        );
        // A clip without an attached proxy still falls back to full.
        assert_eq!(app.export_source_for_clip(9, full.clone()), full);
    }

    #[test]
    fn set_playback_source_action() {
        let mut app = App::new();
        assert_eq!(app.proxy_workflow.playback_source, PlaybackSource::Full);
        app.apply(Action::SetPlaybackSource(PlaybackSource::Proxy));
        assert_eq!(app.proxy_workflow.playback_source, PlaybackSource::Proxy);
    }

    #[test]
    fn detach_action_via_app() {
        let mut app = App::new();
        app.apply(Action::AttachProxyWorkflow { clip_idx: 1, path: PathBuf::from("/p.mov") });
        assert!(app.proxy_workflow.proxy_path(1).is_some());
        app.apply(Action::DetachProxyWorkflow { clip_idx: 1 });
        assert!(app.proxy_workflow.proxy_path(1).is_none());
    }
}
