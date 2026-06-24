//! **Autosave + crash recovery.**
//!
//! A versioned, count-driven snapshot model: every `every_n_actions` applied
//! actions (or on an explicit request) the project is captured into a
//! [`ProjectSnapshot`], pushed onto a fixed-size ring of `keep` snapshots
//! ([`AutosaveRing`]). Crash recovery restores from the *latest* snapshot. The
//! triggering is **count-driven** (not wall-clock) so it is fully deterministic
//! and unit-testable; the wall-clock interval is modelled as a setting only.
//!
//! Snapshots clone the live [`Project`](super::timeline::Project) so they are
//! decoupled from later edits. This module does no filesystem I/O — persisting a
//! snapshot to a `.reel` file on disk is an executor concern layered on top.

use super::{App, Action};
use super::timeline::Project;

/// One captured project version: a monotonically-increasing `version`, the
/// `action_count` at capture time, and a deep clone of the project.
#[derive(Clone, Debug)]
pub struct ProjectSnapshot {
    pub version: u64,
    pub action_count: u64,
    pub project: Project,
}

/// A fixed-capacity ring of the most recent `keep` snapshots (oldest dropped).
#[derive(Clone, Debug)]
pub struct AutosaveRing {
    snapshots: Vec<ProjectSnapshot>,
    keep: usize,
    next_version: u64,
}

impl AutosaveRing {
    /// A ring keeping the last `keep` snapshots (at least 1).
    pub fn new(keep: usize) -> Self {
        Self { snapshots: Vec::new(), keep: keep.max(1), next_version: 1 }
    }

    /// Push a new snapshot of `project` taken at `action_count`, evicting the
    /// oldest when the ring is full. Returns the new snapshot's version.
    pub fn push(&mut self, project: &Project, action_count: u64) -> u64 {
        let version = self.next_version;
        self.next_version += 1;
        self.snapshots.push(ProjectSnapshot {
            version,
            action_count,
            project: project.clone(),
        });
        while self.snapshots.len() > self.keep {
            self.snapshots.remove(0);
        }
        version
    }

    /// The number of retained snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// The capacity (max retained snapshots).
    pub fn capacity(&self) -> usize {
        self.keep
    }

    /// The most recent snapshot, if any (the one crash recovery restores).
    pub fn latest(&self) -> Option<&ProjectSnapshot> {
        self.snapshots.last()
    }

    /// All retained snapshots, oldest first.
    pub fn snapshots(&self) -> &[ProjectSnapshot] {
        &self.snapshots
    }
}

/// Count-driven autosave configuration. `enabled` gates capture; `every_n_actions`
/// is the cadence; `interval_secs` is the modelled wall-clock interval (settings
/// only — not used by the deterministic count trigger). `keep` is the ring size.
#[derive(Clone, Debug)]
pub struct AutosaveConfig {
    pub enabled: bool,
    pub every_n_actions: u64,
    pub interval_secs: u32,
    pub keep: usize,
}

impl Default for AutosaveConfig {
    fn default() -> Self {
        Self { enabled: true, every_n_actions: 20, interval_secs: 300, keep: 10 }
    }
}

impl App {
    /// Note that one user action was applied; if autosave is enabled and the
    /// action count has reached the next cadence boundary, capture a snapshot.
    /// Returns the new snapshot's version when one was taken. Deterministic.
    pub fn autosave_on_action(&mut self) -> Option<u64> {
        self.autosave_action_count += 1;
        if !self.autosave_config.enabled {
            return None;
        }
        let cadence = self.autosave_config.every_n_actions.max(1);
        if self.autosave_action_count % cadence == 0 {
            Some(self.capture_snapshot())
        } else {
            None
        }
    }

    /// Force-capture a project snapshot now (e.g. on an explicit save / before a
    /// risky operation). Returns the new version.
    pub fn capture_snapshot(&mut self) -> u64 {
        let count = self.autosave_action_count;
        self.autosave_ring.push(&self.project, count)
    }

    /// Restore the project from the latest snapshot (crash recovery). Returns the
    /// restored version, or `None` when no snapshot exists.
    pub fn restore_latest_snapshot(&mut self) -> Option<u64> {
        let (version, project) = {
            let snap = self.autosave_ring.latest()?;
            (snap.version, snap.project.clone())
        };
        self.project = project;
        self.host.mark_dirty();
        Some(version)
    }

    /// Restore a specific snapshot `version`, if it is still in the ring.
    pub fn restore_snapshot_version(&mut self, version: u64) -> bool {
        let project = self.autosave_ring.snapshots().iter()
            .find(|s| s.version == version)
            .map(|s| s.project.clone());
        if let Some(project) = project {
            self.project = project;
            self.host.mark_dirty();
            true
        } else {
            false
        }
    }

    /// Re-create the autosave ring from the current config's `keep` size (e.g.
    /// after the user changes the retention setting). Existing snapshots are
    /// dropped.
    pub fn reset_autosave_ring(&mut self) {
        self.autosave_ring = AutosaveRing::new(self.autosave_config.keep);
    }

    /// Apply the autosave actions. Routed from the Batch-5 dispatcher.
    pub(crate) fn apply_autosave(&mut self, action: Action) {
        match action {
            Action::SetAutosaveCadence(n) => {
                self.autosave_config.every_n_actions = n.max(1);
            }
            Action::SetAutosaveKeep(k) => {
                self.autosave_config.keep = k.max(1);
                self.reset_autosave_ring();
            }
            Action::SetAutosaveEnabledV2(b) => {
                self.autosave_config.enabled = b;
            }
            Action::CaptureSnapshot => {
                self.capture_snapshot();
            }
            Action::RestoreLatestSnapshot => {
                self.restore_latest_snapshot();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Action;
    use super::super::timeline::Project;

    #[test]
    fn ring_keeps_only_k_snapshots() {
        let mut ring = AutosaveRing::new(3);
        let proj = Project::new();
        for i in 0..5 {
            ring.push(&proj, i);
        }
        assert_eq!(ring.len(), 3, "ring caps at K");
        assert_eq!(ring.capacity(), 3);
        // The retained snapshots are the last 3 (versions 3,4,5).
        let versions: Vec<u64> = ring.snapshots().iter().map(|s| s.version).collect();
        assert_eq!(versions, vec![3, 4, 5]);
        assert_eq!(ring.latest().unwrap().version, 5);
    }

    #[test]
    fn ring_keep_is_at_least_one() {
        let ring = AutosaveRing::new(0);
        assert_eq!(ring.capacity(), 1);
        assert!(ring.is_empty());
    }

    #[test]
    fn autosave_fires_every_n_actions() {
        let mut app = App::new();
        app.autosave_config.enabled = true;
        app.autosave_config.every_n_actions = 5;
        app.reset_autosave_ring();
        let mut captured = 0;
        for _ in 0..14 {
            if app.autosave_on_action().is_some() {
                captured += 1;
            }
        }
        // 14 actions at cadence 5 → snapshots at 5 and 10 → 2 captures.
        assert_eq!(captured, 2);
        assert_eq!(app.autosave_ring.len(), 2);
    }

    #[test]
    fn disabled_autosave_does_not_capture() {
        let mut app = App::new();
        app.autosave_config.enabled = false;
        app.autosave_config.every_n_actions = 2;
        app.reset_autosave_ring();
        for _ in 0..10 {
            assert!(app.autosave_on_action().is_none());
        }
        assert!(app.autosave_ring.is_empty());
    }

    #[test]
    fn restore_latest_recovers_project() {
        let mut app = App::new();
        app.reset_autosave_ring();
        // Snapshot the original project, then mutate it.
        app.project.name = "v1".into();
        app.capture_snapshot();
        let orig_clip_count = app.project.clips.len();
        // Mutate after the snapshot.
        app.project.name = "v2-dirty".into();
        app.project.clips.clear();
        assert!(app.project.clips.is_empty());
        // Restore → back to the captured "v1" with its clips.
        let v = app.restore_latest_snapshot().expect("restored");
        assert_eq!(app.project.name, "v1");
        assert_eq!(app.project.clips.len(), orig_clip_count);
        assert_eq!(v, app.autosave_ring.latest().unwrap().version);
    }

    #[test]
    fn restore_specific_version() {
        let mut app = App::new();
        app.reset_autosave_ring();
        app.project.name = "a".into();
        let v_a = app.capture_snapshot();
        app.project.name = "b".into();
        let _v_b = app.capture_snapshot();
        // Restore the earlier version.
        assert!(app.restore_snapshot_version(v_a));
        assert_eq!(app.project.name, "a");
        // A missing version returns false.
        assert!(!app.restore_snapshot_version(9999));
    }

    #[test]
    fn restore_with_no_snapshots_is_none() {
        let mut app = App::new();
        app.reset_autosave_ring();
        assert!(app.restore_latest_snapshot().is_none());
    }

    #[test]
    fn autosave_actions_configure_and_capture() {
        let mut app = App::new();
        app.apply(Action::SetAutosaveCadence(3));
        assert_eq!(app.autosave_config.every_n_actions, 3);
        app.apply(Action::SetAutosaveKeep(2));
        assert_eq!(app.autosave_ring.capacity(), 2);
        app.apply(Action::CaptureSnapshot);
        app.apply(Action::CaptureSnapshot);
        app.apply(Action::CaptureSnapshot);
        // keep=2 → only the last 2 snapshots retained.
        assert_eq!(app.autosave_ring.len(), 2);
        app.apply(Action::RestoreLatestSnapshot);
    }
}
