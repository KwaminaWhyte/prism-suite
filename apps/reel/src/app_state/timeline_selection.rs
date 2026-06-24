//! Timeline **selection / snapping / view** operations split out of `timeline.rs`
//! (file-size rule): pixel↔time mapping, frame & edge snapping, the in-flight
//! clip drag, zoom/reset view, single-select, and a **marquee multi-select**
//! model. Action arms live in [`App::apply_timeline_selection`] (a sub-router of
//! `apply_timeline`); the helpers are plain `impl App` methods reused throughout.
//!
//! `impl App` blocks are additive across the crate, so `snap_to_frame` (etc.)
//! defined here is callable from `timeline.rs` and the other domain files.

use super::{App, Action, snap_candidates};

impl App {
    /// Sub-router for **selection / snapping / view / drag** actions. Returns
    /// `Some(action)` when the action is not one of ours (routes on), `None` once
    /// handled.
    pub(crate) fn apply_timeline_selection(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::SelectClip(i) => {
                if i < self.project.clips.len() {
                    self.selected = Some(i);
                    self.marquee_selection = vec![i];
                }
            }
            Action::ZoomBy(factor) => { self.zoom = (self.zoom * factor).clamp(0.1, 8.0); }
            Action::ResetView => { self.zoom = 1.0; }
            Action::ToggleSnap => { self.snap_enabled = !self.snap_enabled; }
            Action::BeginClipDrag { clip_id, grab_offset } => {
                self.clip_drag.set(Some(super::ClipDrag {
                    index: clip_id, kind: super::ClipDragKind::Move, grab_offset,
                }));
            }
            Action::MoveClipDrag { track_idx: _, raw_t } => {
                let drag = match self.clip_drag.get() { Some(d) => d, None => return None };
                let snapped = self.snapped_time(raw_t, drag.index, 8.0);
                self.snap_point = if self.snap_enabled && (snapped - raw_t).abs() > 1e-6 { Some(snapped) } else { None };
                let snapped = self.snap_to_frame(snapped);
                if let Some(clip) = self.project.clips.get_mut(drag.index) {
                    clip.start = snapped;
                    self.host.mark_dirty();
                }
                self.update_scope_data();
            }
            Action::EndClipDrag => {
                self.clip_drag.set(None);
                self.snap_point = None;
            }
            Action::MarqueeSelect { t0, t1, tracks } => {
                self.marquee_select(t0, t1, &tracks);
            }
            Action::ClearMarqueeSelection => {
                self.marquee_selection.clear();
            }
            other => return Some(other),
        }
        None
    }

    /// Map an absolute window x (pixels) to a timeline time (seconds).
    pub fn timeline_x_to_time(&self, x_px: f32) -> Option<f32> {
        let bounds = self.timeline_bounds.get()?;
        let left = f32::from(bounds.origin.x);
        let width = f32::from(bounds.size.width);
        if width <= 0.0 { return None; }
        let frac = ((x_px - left) / width).clamp(0.0, 1.0);
        Some(frac * self.project.duration)
    }

    /// Convert a horizontal pixel delta on the timeline into a time delta.
    pub fn timeline_dx_to_dt(&self, dx_px: f32) -> Option<f32> {
        let bounds = self.timeline_bounds.get()?;
        let width = f32::from(bounds.size.width);
        if width <= 0.0 { return None; }
        Some(dx_px / width * self.project.duration)
    }

    /// Snap a timeline time to the nearest whole frame at the project fps.
    pub fn snap_to_frame(&self, t: f32) -> f32 {
        let fps = self.project.fps;
        if !fps.is_finite() || fps <= 0.0 { return t.max(0.0); }
        ((t * fps).round() / fps).max(0.0)
    }

    /// Snap `raw_t` to the nearest candidate if snap is enabled and close enough.
    pub fn snapped_time(&self, raw_t: f32, exclude_clip: usize, snap_threshold_px: f32) -> f32 {
        if !self.snap_enabled { return raw_t.max(0.0); }
        let bounds = match self.timeline_bounds.get() {
            Some(b) => b,
            None => return raw_t.max(0.0),
        };
        let width_px = f32::from(bounds.size.width);
        if width_px <= 0.0 { return raw_t.max(0.0); }
        let dur = self.project.duration.max(1e-3);
        let threshold_t = snap_threshold_px / width_px * dur;
        let candidates = snap_candidates(&self.project.clips, self.time, self.work_area_in, self.work_area_out);
        let exclude_start = self.project.clips.get(exclude_clip).map(|c| c.start);
        let exclude_end = self.project.clips.get(exclude_clip).map(|c| c.end());
        let best = candidates.into_iter()
            .filter(|&t| Some(t) != exclude_start && Some(t) != exclude_end)
            .min_by(|a, b| (a - raw_t).abs().partial_cmp(&(b - raw_t).abs()).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(t) = best {
            if (t - raw_t).abs() <= threshold_t { return t.max(0.0); }
        }
        raw_t.max(0.0)
    }

    /// Index of the first enabled track, or 0.
    pub(crate) fn first_visible_track(&self) -> usize {
        self.project.tracks.iter().position(|t| t.enabled).unwrap_or(0)
    }

    /// Marquee multi-select: collect every clip whose time span overlaps the
    /// rubber-band time range `[t0, t1]` and whose track is in `tracks` (an empty
    /// `tracks` matches **all** tracks). The result replaces `marquee_selection`
    /// (sorted, deduped); `selected` follows the first clip in the band.
    pub fn marquee_select(&mut self, t0: f32, t1: f32, tracks: &[usize]) {
        let (lo, hi) = if t0 <= t1 { (t0, t1) } else { (t1, t0) };
        let mut hits: Vec<usize> = self.project.clips.iter().enumerate()
            .filter(|(_, c)| {
                let track_ok = tracks.is_empty() || tracks.contains(&c.track);
                // Overlap test: clip [start, end) intersects [lo, hi].
                track_ok && c.start < hi && c.end() > lo
            })
            .map(|(i, _)| i)
            .collect();
        hits.sort_unstable();
        hits.dedup();
        self.selected = hits.first().copied();
        self.marquee_selection = hits;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::timeline::{Clip, ClipSource};
    use gpui::{point, px, size, Bounds};

    #[test]
    fn scrub_x_maps_to_time_within_recorded_bounds() {
        let app = App::new();
        assert!(app.timeline_x_to_time(100.0).is_none());
        app.timeline_bounds.set(Some(Bounds {
            origin: point(px(100.0), px(0.0)),
            size: size(px(800.0), px(40.0)),
        }));
        let dur = app.project.duration;
        assert!((app.timeline_x_to_time(100.0).unwrap() - 0.0).abs() < 1e-4);
        assert!((app.timeline_x_to_time(900.0).unwrap() - dur).abs() < 1e-3);
        assert!((app.timeline_x_to_time(500.0).unwrap() - dur * 0.5).abs() < 1e-3);
    }

    #[test]
    fn timeline_dx_maps_to_dt_within_recorded_bounds() {
        let app = App::new();
        assert!(app.timeline_dx_to_dt(100.0).is_none());
        app.timeline_bounds.set(Some(Bounds {
            origin: point(px(100.0), px(0.0)),
            size: size(px(800.0), px(40.0)),
        }));
        assert!((app.timeline_dx_to_dt(400.0).unwrap() - 15.0).abs() < 1e-3);
        assert!((app.timeline_dx_to_dt(-80.0).unwrap() + 3.0).abs() < 1e-3);
    }

    #[test]
    fn snap_to_frame_rounds_to_project_fps() {
        let app = App::new();
        assert!((app.snap_to_frame(1.0) - 1.0).abs() < 1e-5);
        assert!((app.snap_to_frame(1.01) - 1.0).abs() < 1e-5);
        assert!((app.snap_to_frame(1.02) - 31.0 / 30.0).abs() < 1e-5);
        assert!((app.snap_to_frame(-0.5)).abs() < 1e-6);
    }

    #[test]
    fn toggle_snap_flips() {
        let mut app = App::new();
        let before = app.snap_enabled;
        app.apply(Action::ToggleSnap);
        assert_eq!(app.snap_enabled, !before);
    }

    #[test]
    fn zoom_clamps_and_reset() {
        let mut app = App::new();
        app.apply(Action::ZoomBy(100.0));
        assert!((app.zoom - 8.0).abs() < 1e-5);
        app.apply(Action::ResetView);
        assert!((app.zoom - 1.0).abs() < 1e-6);
    }

    /// Marquee selects every clip overlapping the time band on the given tracks.
    #[test]
    fn marquee_selects_overlapping_clips() {
        let mut app = App::new();
        app.project.clips.clear();
        let mk = |start: f32, track: usize| Clip {
            name: "c".into(),
            source: ClipSource::Color([0.5, 0.5, 0.5, 1.0]),
            track, start, duration: 2.0, ..Clip::default()
        };
        app.project.clips.push(mk(0.0, 0)); // [0,2) V1
        app.project.clips.push(mk(3.0, 0)); // [3,5) V1
        app.project.clips.push(mk(1.0, 1)); // [1,3) V2
        // Band [0.5, 1.5] over all tracks → clips 0 and 2 overlap, not 1.
        app.marquee_select(0.5, 1.5, &[]);
        assert_eq!(app.marquee_selection, vec![0, 2]);
        assert_eq!(app.selected, Some(0));
        // Restrict to track 0 → only clip 0.
        app.marquee_select(0.5, 1.5, &[0]);
        assert_eq!(app.marquee_selection, vec![0]);
        // Reversed band is normalized.
        app.marquee_select(5.0, 3.5, &[]);
        assert_eq!(app.marquee_selection, vec![1]);
    }

    #[test]
    fn marquee_select_action_and_clear() {
        let mut app = App::new();
        app.apply(Action::MarqueeSelect { t0: 0.0, t1: 100.0, tracks: vec![] });
        assert!(!app.marquee_selection.is_empty());
        app.apply(Action::ClearMarqueeSelection);
        assert!(app.marquee_selection.is_empty());
    }
}
