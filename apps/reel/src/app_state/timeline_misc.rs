//! Timeline **time-remap + project-metadata** action arms split out of
//! `timeline.rs` (file-size rule): the speed-curve / freeze-frame / time-remap
//! key edits, plus project name/path/notes/autosave settings and the legacy
//! export-preset + sequence-size/frame-rate/sample-rate arms.
//!
//! Action arms live in [`App::apply_timeline_misc`] (a sub-router of
//! `apply_timeline`): it returns `Some(action)` when the action is not one of
//! ours (so the caller routes it on), or `None` once handled.
//!
//! `impl App` blocks are additive across the crate, so this sub-router composes
//! cleanly with the other `apply_timeline_*` handlers.

use super::{App, Action};

impl App {
    /// Sub-router for **time-remap** edits and **project-metadata** actions.
    /// Returns `Some(action)` when the action is not one of ours (routes on),
    /// `None` once handled.
    pub(crate) fn apply_timeline_misc(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SetTimeRemapEnabled { clip_idx, enabled } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = enabled;
                    if enabled && c.time_remap_keys.is_empty() {
                        c.time_remap_keys = vec![(c.start, c.source_in), (c.end(), c.source_in + c.duration)];
                    }
                    self.host.mark_dirty();
                }
            }
            Action::AddTimeRemapKey { clip_idx, timeline_t, source_t } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_keys.push((timeline_t, source_t));
                    c.time_remap_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::MoveTimeRemapKey { clip_idx, key_idx, source_t } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if let Some(k) = c.time_remap_keys.get_mut(key_idx) { k.1 = source_t.max(0.0); self.host.mark_dirty(); }
                }
            }
            Action::RemoveTimeRemapKey { clip_idx, key_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if key_idx < c.time_remap_keys.len() { c.time_remap_keys.remove(key_idx); self.host.mark_dirty(); }
                }
            }
            Action::SetFreezeFrame { clip_idx, at_t } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    let src_t = c.remapped_source_t(at_t);
                    let end = c.end();
                    c.time_remap_keys.retain(|k| k.0 < at_t || k.0 >= end);
                    c.time_remap_keys.push((at_t, src_t));
                    c.time_remap_keys.push((end - 1e-4, src_t));
                    c.time_remap_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::SetTimeRemapSpeedKeys { clip_idx, mut keys } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    c.time_remap_speed_keys = keys;
                    c.time_remap_enabled = true;
                    self.host.mark_dirty();
                }
            }
            Action::AddTimeRemapSpeedKey { clip_idx, timeline_t, factor } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    c.time_remap_speed_keys.push((timeline_t, factor.max(0.0)));
                    c.time_remap_speed_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::AddSpeedFreezeFrame { clip_idx, at_t, hold_secs } => {
                // Insert a freeze (factor 0) of `hold_secs` at `at_t`: unity speed
                // before and after, a zero-speed plateau in between. The clip's
                // duration grows by `hold_secs` so the held frames have room.
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.time_remap_enabled = true;
                    let hold = hold_secs.max(0.0);
                    let end = c.end();
                    if c.time_remap_speed_keys.is_empty() {
                        c.time_remap_speed_keys = vec![(c.start, 1.0), (end, 1.0)];
                    }
                    c.duration += hold;
                    // Shift any keys at/after the freeze point later by `hold`.
                    for k in c.time_remap_speed_keys.iter_mut() {
                        if k.0 > at_t { k.0 += hold; }
                    }
                    c.time_remap_speed_keys.push((at_t, 0.0));
                    c.time_remap_speed_keys.push((at_t + hold, 0.0));
                    c.time_remap_speed_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    self.host.mark_dirty();
                }
            }
            Action::SetProjectName(n) => { self.project_name = n; }
            Action::SetProjectPath(p) => {
                self.recent_project_paths.insert(0, p.clone());
                self.recent_project_paths.dedup();
                self.recent_project_paths.truncate(10);
                self.project_path = Some(p);
            }
            Action::AddRecentProject(p) => {
                self.recent_project_paths.insert(0, p);
                self.recent_project_paths.dedup();
                self.recent_project_paths.truncate(10);
            }
            Action::SetProjectNotes(n) => { self.project_notes = n; }
            Action::SetAutoSaveEnabled(b) => { self.auto_save_enabled = b; }
            Action::SetAutoSaveInterval(s) => { self.auto_save_interval_sec = s.max(30); }
            Action::TriggerAutoSave => {}
            Action::SaveExportPreset { name, format, width, height, fps } => {
                self.export_presets.push((name, format, width, height, fps));
            }
            Action::DeleteExportPreset(idx) => {
                if idx < self.export_presets.len() {
                    self.export_presets.remove(idx);
                    if self.active_preset == Some(idx) { self.active_preset = None; }
                }
            }
            Action::ApplyExportPreset(idx) => {
                if idx < self.export_presets.len() { self.active_preset = Some(idx); }
            }
            Action::SetSequenceSize { w, h } => {
                self.project.width = w.max(1);
                self.project.height = h.max(1);
                self.host.mark_dirty();
            }
            Action::SetFrameRate(fps) => {
                self.project.fps = fps.clamp(1.0, 120.0);
                self.host.mark_dirty();
            }
            Action::SetSampleRate(rate) => { self.sequence_sample_rate = rate; }
            // --- Real-typing rename (TextField-driven) ----------------------
            Action::RenameClip { index, name } => {
                // Ignore a blank name so a fully-cleared field never wipes the
                // clip's label; trim surrounding whitespace from typed input.
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    if let Some(c) = self.project.clips.get_mut(index) {
                        c.name = trimmed.to_string();
                    }
                }
            }
            Action::RenameTrack { index, name } => {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    if let Some(t) = self.project.tracks.get_mut(index) {
                        t.name = trimmed.to_string();
                    }
                }
            }
            other => return Some(other),
        }
        None
    }
}

#[cfg(test)]
mod rename_tests {
    use super::super::{App, Action};
    use crate::app_state::timeline::Track;

    #[test]
    fn test_rename_clip() {
        let mut app = App::new();
        // Project::new() seeds at least one clip; if not, the action is a no-op
        // and we add one to exercise the path.
        if app.project.clips.is_empty() {
            app.project.tracks.push(Track { name: "V1".into(), enabled: true });
            app.project.clips.push(crate::app_state::timeline::Clip::default());
        }
        app.apply(Action::RenameClip { index: 0, name: "Intro Shot".into() });
        assert_eq!(app.project.clips[0].name, "Intro Shot");
    }

    #[test]
    fn test_rename_clip_trims_and_ignores_blank() {
        let mut app = App::new();
        if app.project.clips.is_empty() {
            app.project.tracks.push(Track { name: "V1".into(), enabled: true });
            app.project.clips.push(crate::app_state::timeline::Clip::default());
        }
        app.apply(Action::RenameClip { index: 0, name: "  Padded  ".into() });
        assert_eq!(app.project.clips[0].name, "Padded");
        // A blank / whitespace-only name is ignored, leaving the prior label.
        app.apply(Action::RenameClip { index: 0, name: "   ".into() });
        assert_eq!(app.project.clips[0].name, "Padded");
    }

    #[test]
    fn test_rename_clip_out_of_bounds_is_noop() {
        let mut app = App::new();
        // Should not panic for an index past the end.
        app.apply(Action::RenameClip { index: 9999, name: "X".into() });
    }

    #[test]
    fn test_rename_track() {
        let mut app = App::new();
        if app.project.tracks.is_empty() {
            app.project.tracks.push(Track { name: "V1".into(), enabled: true });
        }
        app.apply(Action::RenameTrack { index: 0, name: "Camera A".into() });
        assert_eq!(app.project.tracks[0].name, "Camera A");
    }

    #[test]
    fn test_rename_track_ignores_blank() {
        let mut app = App::new();
        if app.project.tracks.is_empty() {
            app.project.tracks.push(Track { name: "V1".into(), enabled: true });
        }
        let original = app.project.tracks[0].name.clone();
        app.apply(Action::RenameTrack { index: 0, name: "".into() });
        assert_eq!(app.project.tracks[0].name, original);
    }
}
