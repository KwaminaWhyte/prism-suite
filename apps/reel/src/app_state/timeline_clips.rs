//! Timeline **clip-edit** operations split out of `timeline.rs` (file-size rule):
//! the structural edits that move / trim / ripple / roll / slip / slide / split
//! clips, plus copy-paste-duplicate, group ripple, and nest. The action arms for
//! these live in [`App::apply_timeline_clips`] (a sub-router of `apply_timeline`);
//! the low-level edit helpers (`trim_in`, `slip_clip`, …) are plain `impl App`
//! methods reused by the arms and by panels.
//!
//! `impl App` blocks are additive across the crate, so methods defined here are
//! callable from `timeline.rs` and vice-versa.

use super::{App, Action, MIN_DUR};
use super::timeline::{ClipBlendMode, ClipSource};

impl App {
    /// Sub-router for clip **structural edits** (move / trim / ripple / roll /
    /// slip / slide / split / copy-paste / nest). Returns `Some(action)` when the
    /// action is NOT one of ours (so the caller routes it on), or `None` once
    /// handled. Keeps `apply_timeline` a thin dispatcher.
    pub(crate) fn apply_timeline_clips(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::MoveClip { index, start } => {
                let snapped = self.snap_to_frame(start.max(0.0));
                if let Some(clip) = self.project.clips.get(index) {
                    let delta = snapped - clip.start;
                    let group = clip.link_group;
                    self.project.clips[index].start = snapped;
                    if let Some(gid) = group {
                        for i in 0..self.project.clips.len() {
                            if i != index && self.project.clips[i].link_group == Some(gid) {
                                let new_start = (self.project.clips[i].start + delta).max(0.0);
                                self.project.clips[i].start = self.snap_to_frame(new_start);
                            }
                        }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::TrimClipIn { index, t } => {
                let t = self.snap_to_frame(t);
                let group = self.project.clips.get(index).and_then(|c| c.link_group);
                if self.trim_in(index, t).is_some() {
                    if let Some(gid) = group {
                        let partners: Vec<usize> = self.project.clips.iter().enumerate()
                            .filter(|(i, c)| *i != index && c.link_group == Some(gid))
                            .map(|(i, _)| i).collect();
                        for pi in partners { self.trim_in(pi, t); }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::TrimClipOut { index, t } => {
                let t = self.snap_to_frame(t);
                let group = self.project.clips.get(index).and_then(|c| c.link_group);
                if self.trim_out(index, t).is_some() {
                    if let Some(gid) = group {
                        let partners: Vec<usize> = self.project.clips.iter().enumerate()
                            .filter(|(i, c)| *i != index && c.link_group == Some(gid))
                            .map(|(i, _)| i).collect();
                        for pi in partners { self.trim_out(pi, t); }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::RippleTrimClipIn { index, t } => {
                let t = self.snap_to_frame(t);
                if self.ripple_trim_in(index, t).is_some() { self.host.mark_dirty(); }
            }
            Action::RippleTrimClipOut { index, t } => {
                let t = self.snap_to_frame(t);
                if self.ripple_trim_out(index, t).is_some() { self.host.mark_dirty(); }
            }
            Action::RollTrimEdit { index, delta } => {
                if self.roll_trim(index, delta) { self.host.mark_dirty(); }
            }
            Action::SlipClip { index, delta } => {
                if self.slip_clip(index, delta) { self.host.mark_dirty(); }
            }
            Action::SlideClip { index, delta } => {
                if self.slide_clip(index, delta) { self.host.mark_dirty(); }
            }
            Action::SplitClip { index, t } => {
                let t = self.snap_to_frame(t);
                if let Some(new_idx) = self.split_clip(index, t) {
                    self.selected = Some(new_idx);
                    self.host.mark_dirty();
                }
            }
            Action::SplitAtPlayhead => {
                let t = self.snap_to_frame(self.time);
                if self.split_at(t) > 0 { self.host.mark_dirty(); }
            }
            Action::CopySelectedClips => {
                if let Some(idx) = self.selected {
                    if let Some(clip) = self.project.clips.get(idx).cloned() {
                        self.clipboard_clips = vec![clip];
                    }
                }
            }
            Action::CutSelectedClips => {
                if let Some(idx) = self.selected {
                    if idx < self.project.clips.len() {
                        let clip = self.project.clips.remove(idx);
                        self.clipboard_clips = vec![clip];
                        self.selected = None;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::PasteClips { at_t } => {
                if self.clipboard_clips.is_empty() { return None; }
                let start_t = self.snap_to_frame(at_t);
                let base_t = self.clipboard_clips[0].start;
                let first_new = self.project.clips.len();
                for src in self.clipboard_clips.clone() {
                    let offset = src.start - base_t;
                    let mut c = src;
                    c.start = self.snap_to_frame(start_t + offset);
                    c.link_group = None;
                    self.project.clips.push(c);
                }
                self.selected = Some(first_new);
                self.host.mark_dirty();
            }
            Action::DuplicateSelectedClips => {
                if let Some(idx) = self.selected {
                    if let Some(src) = self.project.clips.get(idx).cloned() {
                        let mut dup = src;
                        dup.start = self.snap_to_frame(dup.start + dup.duration);
                        dup.link_group = None;
                        self.project.clips.push(dup);
                        self.selected = Some(self.project.clips.len() - 1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::GroupRippleTrimIn { clip_indices, delta } => {
                if delta == 0.0 || clip_indices.is_empty() { return None; }
                for &ci in &clip_indices {
                    let t = self.project.clips.get(ci).map(|c| c.start + delta);
                    if let Some(t) = t { self.ripple_trim_in(ci, t); }
                }
                self.host.mark_dirty();
            }
            Action::GroupRippleTrimOut { clip_indices, delta } => {
                if delta == 0.0 || clip_indices.is_empty() { return None; }
                for &ci in &clip_indices {
                    let t = self.project.clips.get(ci).map(|c| c.end() + delta);
                    if let Some(t) = t { self.ripple_trim_out(ci, t); }
                }
                self.host.mark_dirty();
            }
            Action::NestSelectedClips { name } => {
                let Some(sel_idx) = self.selected else { return None };
                if sel_idx >= self.project.clips.len() { return None };
                let inner_clip = self.project.clips[sel_idx].clone();
                let dur = inner_clip.duration;
                let inner_tracks = self.project.tracks.clone();
                self.project.clips[sel_idx].source = ClipSource::NestedClip {
                    tracks: inner_tracks, clips: vec![inner_clip], duration_secs: dur.max(MIN_DUR),
                };
                self.project.clips[sel_idx].name = name;
                self.host.mark_dirty();
            }
            // Per-clip transform edits (anchor / crop / blend / reset).
            Action::SetClipAnchor { clip_idx, x, y } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.anchor_x = x.clamp(-1.0, 1.0);
                    c.anchor_y = y.clamp(-1.0, 1.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetClipCrop { clip_idx, left, right, top, bottom } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.crop_left = left.clamp(0.0, 1.0);
                    c.crop_right = right.clamp(0.0, 1.0);
                    c.crop_top = top.clamp(0.0, 1.0);
                    c.crop_bottom = bottom.clamp(0.0, 1.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetClipBlendMode { clip_idx, mode } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.blend_mode = mode;
                    self.host.mark_dirty();
                }
            }
            Action::ResetClipTransform { clip_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.anchor_x = 0.0; c.anchor_y = 0.0;
                    c.crop_left = 0.0; c.crop_right = 0.0;
                    c.crop_top = 0.0; c.crop_bottom = 0.0;
                    c.blend_mode = ClipBlendMode::Normal;
                    self.host.mark_dirty();
                }
            }
            // Not ours — pass through to the next sub-router.
            other => return Some(other),
        }
        None
    }

    /// Trim a clip's left edge to `t` without rippling.
    pub fn trim_in(&mut self, idx: usize, t: f32) -> Option<f32> {
        let clip = self.project.clips.get(idx)?;
        let end = clip.end();
        let max_start = end - MIN_DUR;
        let min_start = (clip.start - clip.source_in).max(0.0);
        let new_start = t.clamp(min_start, max_start);
        let shift = new_start - clip.start;
        let clip = &mut self.project.clips[idx];
        clip.start = new_start;
        clip.source_in += shift;
        clip.duration -= shift;
        clip.clamp_to_source();
        Some(shift)
    }

    /// Trim a clip's right edge to `t` without rippling.
    pub fn trim_out(&mut self, idx: usize, t: f32) -> Option<f32> {
        let clip = self.project.clips.get(idx)?;
        let start = clip.start;
        let old_dur = clip.duration;
        let mut new_end = t.max(start + MIN_DUR);
        if let Some(len) = clip.source.source_len() {
            new_end = new_end.min(start + (len - clip.source_in));
        }
        let clip = &mut self.project.clips[idx];
        clip.duration = new_end - start;
        clip.clamp_to_source();
        Some(clip.duration - old_dur)
    }

    /// Ripple-trim the left edge of clip `idx` to `t`.
    pub fn ripple_trim_in(&mut self, idx: usize, t: f32) -> Option<f32> {
        let track = self.project.clips.get(idx)?.track;
        let old_start = self.project.clips[idx].start;
        let shift = self.trim_in(idx, t)?;
        if shift == 0.0 { return Some(0.0); }
        for i in 0..self.project.clips.len() {
            if i != idx && self.project.clips[i].track == track && self.project.clips[i].start < old_start {
                self.project.clips[i].start -= shift;
            }
        }
        Some(shift)
    }

    /// Ripple-trim the right edge of clip `idx` to `t`.
    pub fn ripple_trim_out(&mut self, idx: usize, t: f32) -> Option<f32> {
        let track = self.project.clips.get(idx)?.track;
        let old_end = self.project.clips[idx].end();
        let delta = self.trim_out(idx, t)?;
        if delta == 0.0 { return Some(0.0); }
        for i in 0..self.project.clips.len() {
            if i != idx && self.project.clips[i].track == track && self.project.clips[i].start >= old_end - 1e-4 {
                self.project.clips[i].start += delta;
            }
        }
        Some(delta)
    }

    /// Roll-trim: shift the edit point between clip `idx` and its right neighbour.
    pub fn roll_trim(&mut self, idx: usize, delta: f32) -> bool {
        let track = self.project.clips.get(idx).map(|c| c.track);
        let Some(track) = track else { return false };
        let right_idx = {
            let end = self.project.clips[idx].end();
            self.project.clips.iter().enumerate()
                .filter(|(i, c)| *i != idx && c.track == track && c.start >= end - 1e-3)
                .min_by(|(_, a), (_, b)| a.start.partial_cmp(&b.start).unwrap())
                .map(|(i, _)| i)
        };
        let Some(right) = right_idx else { return false };
        let old_out = self.project.clips[idx].end();
        let new_out = (old_out + delta).max(self.project.clips[idx].start + MIN_DUR);
        let actual_delta = new_out - old_out;
        if actual_delta == 0.0 { return false; }
        let idx_new_dur = self.project.clips[idx].duration + actual_delta;
        if idx_new_dur < MIN_DUR { return false; }
        self.project.clips[idx].duration = idx_new_dur;
        let nb = &mut self.project.clips[right];
        let nb_new_in = nb.source_in + actual_delta;
        if nb_new_in < 0.0 { return false; }
        nb.start += actual_delta;
        nb.source_in = nb_new_in;
        nb.duration = (nb.duration - actual_delta).max(MIN_DUR);
        true
    }

    /// Slip edit: shift the clip's source in/out window by `delta` **without**
    /// moving the clip on the timeline (start, duration, and neighbours all stay
    /// put). Clamped so `source_in` stays ≥ 0 and the window stays inside a
    /// bounded source. Returns `true` if the window actually moved.
    pub fn slip_clip(&mut self, idx: usize, delta: f32) -> bool {
        let Some(clip) = self.project.clips.get(idx) else { return false };
        let dur = clip.duration;
        let mut new_in = (clip.source_in + delta).max(0.0);
        if let Some(len) = clip.source.source_len() {
            // The window [new_in, new_in+dur] must fit inside [0, len].
            let max_in = (len - dur).max(0.0);
            new_in = new_in.min(max_in);
        }
        let clip = &mut self.project.clips[idx];
        if (new_in - clip.source_in).abs() < 1e-6 { return false; }
        clip.source_in = new_in;
        true
    }

    /// Slide edit: move the clip along the timeline by `delta`, extending the
    /// left neighbour's tail and trimming the right neighbour's head (on the same
    /// track) by the same amount, so the overall sequence length is unchanged.
    /// Clamped so neither neighbour drops below [`MIN_DUR`]. Returns `true` if the
    /// clip moved.
    pub fn slide_clip(&mut self, idx: usize, delta: f32) -> bool {
        let Some(clip) = self.project.clips.get(idx) else { return false };
        let track = clip.track;
        let start = clip.start;
        let end = clip.end();
        // Left neighbour: ends at our start. Right neighbour: starts at our end.
        let left = self.project.clips.iter().enumerate()
            .filter(|(i, c)| *i != idx && c.track == track && c.end() <= start + 1e-3)
            .max_by(|(_, a), (_, b)| a.end().partial_cmp(&b.end()).unwrap())
            .map(|(i, _)| i);
        let right = self.project.clips.iter().enumerate()
            .filter(|(i, c)| *i != idx && c.track == track && c.start >= end - 1e-3)
            .min_by(|(_, a), (_, b)| a.start.partial_cmp(&b.start).unwrap())
            .map(|(i, _)| i);

        // Clamp delta so neighbours keep at least MIN_DUR.
        let mut d = delta;
        if let Some(l) = left {
            // Left grows by d (its duration += d). It must stay ≥ MIN_DUR.
            let l_dur = self.project.clips[l].duration;
            d = d.max(MIN_DUR - l_dur);
        }
        if let Some(r) = right {
            // Right shrinks by d (its duration -= d) and its head advances by d
            // (source_in += d ≥ 0). It must stay ≥ MIN_DUR and source ≥ 0.
            let r = &self.project.clips[r];
            d = d.min(r.duration - MIN_DUR);
            d = d.max(-r.source_in);
        }
        if d.abs() < 1e-6 { return false; }

        self.project.clips[idx].start += d;
        if let Some(l) = left {
            self.project.clips[l].duration += d;
            self.project.clips[l].clamp_to_source();
        }
        if let Some(r) = right {
            let nb = &mut self.project.clips[r];
            nb.start += d;
            nb.source_in = (nb.source_in + d).max(0.0);
            nb.duration -= d;
            nb.clamp_to_source();
        }
        true
    }

    /// Split clip `idx` at time `t`. Returns the new right clip's index, or `None`.
    pub fn split_clip(&mut self, idx: usize, t: f32) -> Option<usize> {
        let clip = self.project.clips.get(idx)?;
        let left_dur = t - clip.start;
        let right_dur = clip.end() - t;
        if left_dur < MIN_DUR || right_dur < MIN_DUR { return None; }
        let mut right = clip.clone();
        right.start = t;
        right.source_in = clip.source_in + left_dur;
        right.duration = right_dur;
        self.project.clips[idx].duration = left_dur;
        self.project.clips.push(right);
        Some(self.project.clips.len() - 1)
    }

    /// Split every clip straddling `t` on enabled tracks. Returns count cut.
    pub fn split_at(&mut self, t: f32) -> usize {
        let straddling: Vec<usize> = self.project.clips.iter().enumerate()
            .filter(|(_, c)| {
                c.covers(t)
                    && (t - c.start) >= MIN_DUR
                    && (c.end() - t) >= MIN_DUR
                    && self.project.tracks.get(c.track).map(|tr| tr.enabled).unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        let mut n = 0;
        for i in straddling {
            if self.split_clip(i, t).is_some() { n += 1; }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};
    use super::super::timeline::{Clip, ClipSource, ColorGrade, VideoSource};

    #[test]
    fn trim_out_shrinks_tail_and_clamps_to_min_dur() {
        let mut app = App::new();
        app.apply(Action::TrimClipOut { index: 0, t: 4.0 });
        assert!((app.project.clips[0].start - 0.0).abs() < 1e-5);
        assert!((app.project.clips[0].duration - 4.0).abs() < 1e-5);
        assert!((app.project.clips[0].source_in - 0.0).abs() < 1e-6);
        app.apply(Action::TrimClipOut { index: 0, t: -2.0 });
        assert!((app.project.clips[0].duration - MIN_DUR).abs() < 1e-5);
    }

    #[test]
    fn trim_in_advances_source_and_keeps_right_edge() {
        let mut app = App::new();
        let end_before = app.project.clips[0].end();
        app.apply(Action::TrimClipIn { index: 0, t: 2.0 });
        let c = &app.project.clips[0];
        assert!((c.start - 2.0).abs() < 1e-5);
        assert!((c.source_in - 2.0).abs() < 1e-5);
        assert!((c.end() - end_before).abs() < 1e-4);
        app.apply(Action::TrimClipIn { index: 0, t: 100.0 });
        assert!(app.project.clips[0].duration >= MIN_DUR - 1e-6);
    }

    #[test]
    fn trim_out_clamps_to_bounded_source_len() {
        let mut app = App::new();
        app.project.clips.push(Clip {
            name: "movie".into(),
            source: ClipSource::Video(VideoSource { path: "/m.mp4".into(), fps: Some(30.0), duration: Some(5.0) }),
            track: 0, start: 0.0, duration: 4.0, ..Clip::default()
        });
        let idx = app.project.clips.len() - 1;
        app.apply(Action::TrimClipOut { index: idx, t: 9.0 });
        assert!((app.project.clips[idx].duration - 5.0).abs() < 1e-4);
    }

    #[test]
    fn split_clip_cuts_into_two_source_continuous_parts() {
        let mut app = App::new();
        let before = app.project.clips.len();
        let new_idx = app.split_clip(0, 2.0).expect("split should succeed");
        assert_eq!(app.project.clips.len(), before + 1);
        assert!((app.project.clips[0].duration - 2.0).abs() < 1e-5);
        let right = &app.project.clips[new_idx];
        assert!((right.start - 2.0).abs() < 1e-5);
        assert!((right.duration - 4.0).abs() < 1e-5);
        assert!((right.source_in - 2.0).abs() < 1e-5);
    }

    #[test]
    fn split_at_playhead_razors_every_straddling_visible_clip() {
        let mut app = App::new();
        let before = app.project.clips.len();
        app.apply(Action::Seek(4.0));
        app.apply(Action::SplitAtPlayhead);
        assert_eq!(app.project.clips.len(), before + 2);
    }

    #[test]
    fn move_clip_snaps_start_and_clamps() {
        let mut app = App::new();
        app.apply(Action::MoveClip { index: 0, start: 2.51 });
        let expected = app.snap_to_frame(2.51);
        assert!((app.project.clips[0].start - expected).abs() < 1e-5);
        app.apply(Action::MoveClip { index: 0, start: -3.0 });
        assert!((app.project.clips[0].start).abs() < 1e-6);
        // An out-of-range index is a no-op (no panic).
        app.apply(Action::MoveClip { index: 999, start: 1.0 });
    }

    #[test]
    fn out_of_range_trim_and_split_are_noops() {
        let mut app = App::new();
        let n = app.project.clips.len();
        assert!(app.trim_in(999, 1.0).is_none());
        assert!(app.trim_out(999, 1.0).is_none());
        assert!(app.split_clip(999, 1.0).is_none());
        assert_eq!(app.project.clips.len(), n);
    }

    #[test]
    fn copy_paste_clips_lands_at_target_time() {
        let mut app = App::new();
        app.selected = Some(0);
        app.apply(Action::CopySelectedClips);
        assert_eq!(app.clipboard_clips.len(), 1);
        let before = app.project.clips.len();
        app.apply(Action::PasteClips { at_t: 15.0 });
        assert_eq!(app.project.clips.len(), before + 1);
        let pasted = app.project.clips.last().unwrap();
        let expected_start = app.snap_to_frame(15.0);
        assert!((pasted.start - expected_start).abs() < 1e-4);
        assert!(pasted.link_group.is_none());
    }

    #[test]
    fn cut_clip_removes_original() {
        let mut app = App::new();
        let n = app.project.clips.len();
        app.selected = Some(0);
        app.apply(Action::CutSelectedClips);
        assert_eq!(app.project.clips.len(), n - 1);
        assert_eq!(app.clipboard_clips.len(), 1);
        assert!(app.selected.is_none());
    }

    #[test]
    fn duplicate_clip_places_copy_right_after() {
        let mut app = App::new();
        app.selected = Some(0);
        let start = app.project.clips[0].start;
        let dur = app.project.clips[0].duration;
        let n = app.project.clips.len();
        app.apply(Action::DuplicateSelectedClips);
        assert_eq!(app.project.clips.len(), n + 1);
        let dup = app.project.clips.last().unwrap();
        let expected = app.snap_to_frame(start + dur);
        assert!((dup.start - expected).abs() < 1e-4);
        assert!(dup.link_group.is_none());
    }

    #[test]
    fn set_clip_crop_clamps_to_unit_range() {
        let mut app = App::new();
        app.apply(Action::SetClipCrop { clip_idx: 0, left: 0.3, right: 2.0, top: -0.1, bottom: 0.5 });
        let c = &app.project.clips[0];
        assert!((c.crop_left - 0.3).abs() < 1e-5);
        assert!((c.crop_right - 1.0).abs() < 1e-5);
        assert!((c.crop_top).abs() < 1e-5);
        assert!((c.crop_bottom - 0.5).abs() < 1e-5);
    }

    #[test]
    fn reset_clip_transform_restores_defaults() {
        let mut app = App::new();
        app.apply(Action::SetClipAnchor { clip_idx: 0, x: 0.5, y: 0.25 });
        app.apply(Action::SetClipBlendMode { clip_idx: 0, mode: ClipBlendMode::Multiply });
        app.apply(Action::ResetClipTransform { clip_idx: 0 });
        let c = &app.project.clips[0];
        assert!((c.anchor_x).abs() < 1e-6 && (c.anchor_y).abs() < 1e-6);
        assert_eq!(c.blend_mode, ClipBlendMode::Normal);
        assert!((c.crop_left + c.crop_right + c.crop_top + c.crop_bottom).abs() < 1e-6);
    }

    #[test]
    fn group_ripple_trim_in_trims_multiple_clips() {
        let mut app = App::new();
        let start0 = app.project.clips[0].start;
        let start1 = app.project.clips[1].start;
        app.apply(Action::GroupRippleTrimIn { clip_indices: vec![0, 1], delta: 1.0 });
        assert!(app.project.clips[0].start > start0 - 1e-4);
        assert!(app.project.clips[1].start > start1 - 1e-4);
    }

    #[test]
    fn group_ripple_trim_out_extends_multiple_clips() {
        let mut app = App::new();
        let end0 = app.project.clips[0].end();
        let end1 = app.project.clips[1].end();
        app.apply(Action::GroupRippleTrimOut { clip_indices: vec![0, 1], delta: 1.0 });
        assert!(app.project.clips[0].end() > end0 - 1e-4);
        assert!(app.project.clips[1].end() > end1 - 1e-4);
    }

    // --- New: slip / slide edits --------------------------------------------

    /// Slip shifts the source window but leaves the clip's timeline placement
    /// untouched. On an unbounded color source it always slides; clamped ≥ 0.
    #[test]
    fn slip_moves_source_window_not_timeline() {
        let mut app = App::new();
        // Use a bounded movie clip so we can test source clamping.
        app.project.clips.push(Clip {
            name: "movie".into(),
            source: ClipSource::Video(VideoSource { path: "/m.mp4".into(), fps: Some(30.0), duration: Some(10.0) }),
            track: 0, start: 5.0, duration: 4.0, source_in: 2.0, ..Clip::default()
        });
        let idx = app.project.clips.len() - 1;
        let start_before = app.project.clips[idx].start;
        let dur_before = app.project.clips[idx].duration;
        // Slip +1s: source_in 2 → 3, placement unchanged.
        assert!(app.slip_clip(idx, 1.0));
        assert!((app.project.clips[idx].source_in - 3.0).abs() < 1e-5);
        assert!((app.project.clips[idx].start - start_before).abs() < 1e-6);
        assert!((app.project.clips[idx].duration - dur_before).abs() < 1e-6);
        // Slip far negative clamps source_in to 0.
        assert!(app.slip_clip(idx, -100.0));
        assert!((app.project.clips[idx].source_in - 0.0).abs() < 1e-5);
        // A bounded slip can't push the window past the source end (in+dur ≤ 10).
        assert!(app.slip_clip(idx, 100.0));
        assert!((app.project.clips[idx].source_in - 6.0).abs() < 1e-4); // 10 - 4
    }

    /// Slide moves the clip and adjusts both neighbours by the same delta so the
    /// total length is unchanged.
    #[test]
    fn slide_shifts_clip_and_adjusts_neighbours() {
        let mut app = App::new();
        // Three abutting movie clips on a fresh track: [0,4) [4,4) [8,4).
        let track = 0usize;
        let mk = |start: f32| Clip {
            name: "c".into(),
            source: ClipSource::Video(VideoSource { path: "/m.mp4".into(), fps: Some(30.0), duration: Some(20.0) }),
            track, start, duration: 4.0, source_in: 4.0, ..Clip::default()
        };
        app.project.clips.clear();
        app.project.clips.push(mk(0.0));
        app.project.clips.push(mk(4.0));
        app.project.clips.push(mk(8.0));
        let mid = 1usize;
        let left_end_before = app.project.clips[0].end();
        let right_start_before = app.project.clips[2].start;
        // Slide the middle clip +1s.
        assert!(app.slide_clip(mid, 1.0));
        assert!((app.project.clips[mid].start - 5.0).abs() < 1e-5);
        // Left clip's tail grew by 1.
        assert!((app.project.clips[0].end() - (left_end_before + 1.0)).abs() < 1e-4);
        // Right clip's head advanced by 1 (start unchanged at 8 since slide moves
        // the right neighbour's start with the clip's end).
        assert!((app.project.clips[2].start - (right_start_before + 1.0)).abs() < 1e-4);
    }

    #[test]
    fn slide_clip_out_of_range_is_noop() {
        let mut app = App::new();
        assert!(!app.slide_clip(999, 1.0));
        assert!(!app.slip_clip(999, 1.0));
    }

    // --- Per-clip color grade (applied to clips) -----------------------------

    #[test]
    fn color_grade_identity_is_a_noop() {
        let g = ColorGrade::default();
        assert!(g.is_identity());
        let mut px = [0.3, 0.6, 0.9];
        g.apply(&mut px);
        assert!((px[0] - 0.3).abs() < 1e-4);
        assert!((px[1] - 0.6).abs() < 1e-4);
        assert!((px[2] - 0.9).abs() < 1e-4);
    }

    #[test]
    fn color_grade_exposure_brightens() {
        let g = ColorGrade { exposure: 1.0, ..Default::default() };
        assert!(!g.is_identity());
        let mut px = [0.25, 0.25, 0.25];
        g.apply(&mut px);
        assert!(px[0] > 0.45 && px[0] < 0.55, "exposure +1 ≈ ×2: {}", px[0]);
    }

    #[test]
    fn color_grade_saturation_zero_is_greyscale() {
        let g = ColorGrade { saturation: 0.0, ..Default::default() };
        let mut px = [0.8, 0.2, 0.2];
        g.apply(&mut px);
        assert!((px[0] - px[1]).abs() < 1e-4 && (px[1] - px[2]).abs() < 1e-4);
    }
}
