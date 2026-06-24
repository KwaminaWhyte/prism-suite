//! **Background render cache.**
//!
//! A smart render-cache model in the spirit of Premiere's "Render & Replace" /
//! red-yellow-green render bar. The timeline is partitioned into half-open time
//! [`CacheRange`]s; each rendered segment is stored as a [`CacheSegment`] keyed
//! by `(content_hash, range)` where `content_hash` summarises the **clip stack**
//! that covers the range. When the stack over a range changes (an edit), the
//! segment's hash no longer matches and the range reports as *dirty* — this is
//! the invalidation. A fixed-capacity ring evicts the **oldest** segment when
//! full. Per-range [`RenderBarStatus`] drives the render-bar colour.
//!
//! Pure + deterministic (no actual rendering / GPU): segments are inserted by
//! explicit calls so the whole thing is unit-testable.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{App, Action};
use super::timeline::{Clip, ClipBlendMode, ClipSource};

/// A stable integer for each blend mode so the hash doesn't require `Hash` on
/// [`ClipBlendMode`].
fn blend_mode_index(m: ClipBlendMode) -> u8 {
    match m {
        ClipBlendMode::Normal => 0,
        ClipBlendMode::Multiply => 1,
        ClipBlendMode::Screen => 2,
        ClipBlendMode::Overlay => 3,
        ClipBlendMode::Add => 4,
        ClipBlendMode::Subtract => 5,
    }
}

/// A half-open timeline range `[start, end)` in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CacheRange {
    pub start: f32,
    pub end: f32,
}

impl CacheRange {
    pub fn new(start: f32, end: f32) -> Self {
        let (start, end) = if end < start { (end, start) } else { (start, end) };
        Self { start, end }
    }

    pub fn len(&self) -> f32 {
        (self.end - self.start).max(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= f32::EPSILON
    }

    /// Whether two ranges refer to the same span (within a tight tolerance).
    pub fn same_as(&self, other: &CacheRange) -> bool {
        (self.start - other.start).abs() < 1e-4 && (self.end - other.end).abs() < 1e-4
    }

    /// Whether `t` falls inside `[start, end)`.
    pub fn contains(&self, t: f32) -> bool {
        t >= self.start && t < self.end
    }
}

/// One cached render: the range it covers, the content hash it was rendered for,
/// and the (modelled) preview file it produced.
#[derive(Clone, Debug)]
pub struct CacheSegment {
    pub range: CacheRange,
    pub content_hash: u64,
    pub preview_file: String,
}

/// The render-bar colour for a range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderBarStatus {
    /// No cached segment overlaps — needs rendering (red).
    Unrendered,
    /// A segment exists but its hash is stale — needs re-render (yellow).
    Stale,
    /// A fresh cached segment exists (green).
    Rendered,
}

/// Hash the clip stack covering a range: the ordered set of clips that overlap
/// `[start, end)`, folding in the fields that affect the rendered pixels. Two
/// stacks render identically iff this hash matches.
pub fn hash_clip_stack(clips: &[Clip], range: CacheRange) -> u64 {
    let mut covering: Vec<&Clip> = clips
        .iter()
        .filter(|c| c.start < range.end && c.end() > range.start)
        .collect();
    // Order by track then start so a stable stack hashes the same regardless of
    // the underlying Vec order.
    covering.sort_by(|a, b| {
        a.track
            .cmp(&b.track)
            .then(a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut h = DefaultHasher::new();
    covering.len().hash(&mut h);
    for c in covering {
        c.track.hash(&mut h);
        c.start.to_bits().hash(&mut h);
        c.duration.to_bits().hash(&mut h);
        c.source_in.to_bits().hash(&mut h);
        c.opacity.to_bits().hash(&mut h);
        c.speed.to_bits().hash(&mut h);
        c.reversed.hash(&mut h);
        blend_mode_index(c.blend_mode).hash(&mut h);
        c.effects.len().hash(&mut h);
        // Source identity (path / color), which changes the pixels.
        hash_source(&c.source, &mut h);
    }
    h.finish()
}

fn hash_source(source: &ClipSource, h: &mut DefaultHasher) {
    match source {
        ClipSource::Color(c) => {
            0u8.hash(h);
            for v in c {
                v.to_bits().hash(h);
            }
        }
        ClipSource::Image(p) => {
            1u8.hash(h);
            p.hash(h);
        }
        ClipSource::Video(v) => {
            2u8.hash(h);
            v.path.hash(h);
        }
        ClipSource::Audio(a) => {
            3u8.hash(h);
            a.path.hash(h);
        }
        ClipSource::Title { text, font_size, color, .. } => {
            4u8.hash(h);
            text.hash(h);
            font_size.to_bits().hash(h);
            color.hash(h);
        }
        ClipSource::NestedClip { clips, duration_secs, .. } => {
            5u8.hash(h);
            clips.len().hash(h);
            duration_secs.to_bits().hash(h);
        }
    }
}

/// A fixed-capacity render cache with oldest-first (ring) eviction.
#[derive(Clone, Debug)]
pub struct RenderCache {
    segments: Vec<CacheSegment>,
    capacity: usize,
    next_file: u64,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new(64)
    }
}

impl RenderCache {
    pub fn new(capacity: usize) -> Self {
        Self { segments: Vec::new(), capacity: capacity.max(1), next_file: 0 }
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn segments(&self) -> &[CacheSegment] {
        &self.segments
    }

    /// Insert a rendered segment for `range` rendered against `content_hash`. If a
    /// segment already covers that exact range, it is replaced (and moved to the
    /// most-recent slot). Evicts the oldest when over capacity. Returns the
    /// modelled preview file name.
    pub fn insert(&mut self, range: CacheRange, content_hash: u64) -> String {
        let file = format!("rendercache_{:08}.mxf", self.next_file);
        self.next_file += 1;
        // Replace an existing segment for the same range (re-render).
        self.segments.retain(|s| !s.range.same_as(&range));
        self.segments.push(CacheSegment { range, content_hash, preview_file: file.clone() });
        while self.segments.len() > self.capacity {
            self.segments.remove(0);
        }
        file
    }

    /// The segment whose range exactly matches `range`, if any.
    pub fn segment_for(&self, range: CacheRange) -> Option<&CacheSegment> {
        self.segments.iter().find(|s| s.range.same_as(&range))
    }

    /// The render-bar status of `range` given the current `content_hash`:
    /// - no matching segment → [`RenderBarStatus::Unrendered`]
    /// - matching range, stale hash → [`RenderBarStatus::Stale`]
    /// - matching range, fresh hash → [`RenderBarStatus::Rendered`].
    pub fn status(&self, range: CacheRange, content_hash: u64) -> RenderBarStatus {
        match self.segment_for(range) {
            None => RenderBarStatus::Unrendered,
            Some(s) if s.content_hash == content_hash => RenderBarStatus::Rendered,
            Some(_) => RenderBarStatus::Stale,
        }
    }

    /// Drop every segment whose range overlaps `[start, end)` (edit invalidation).
    /// Returns the count removed.
    pub fn invalidate_overlapping(&mut self, start: f32, end: f32) -> usize {
        let before = self.segments.len();
        self.segments.retain(|s| !(s.range.start < end && s.range.end > start));
        before - self.segments.len()
    }

    /// Remove every segment whose stored hash no longer matches the hash computed
    /// for its range against `clips` (a global re-validation pass). Returns the
    /// count removed.
    pub fn prune_stale(&mut self, clips: &[Clip]) -> usize {
        let before = self.segments.len();
        self.segments.retain(|s| hash_clip_stack(clips, s.range) == s.content_hash);
        before - self.segments.len()
    }

    pub fn clear(&mut self) {
        self.segments.clear();
    }
}

impl App {
    /// Render (model) `range` of the timeline into the cache, keyed by the current
    /// clip-stack hash for that range. Returns the preview file name.
    pub fn render_range(&mut self, range: CacheRange) -> String {
        let hash = hash_clip_stack(&self.project.clips, range);
        self.render_cache.insert(range, hash)
    }

    /// The render-bar status of `range` against the live clip stack.
    pub fn render_status(&self, range: CacheRange) -> RenderBarStatus {
        let hash = hash_clip_stack(&self.project.clips, range);
        self.render_cache.status(range, hash)
    }

    /// Apply the render-cache actions. Routed from `mod.rs`.
    pub(crate) fn apply_render_cache(&mut self, action: Action) {
        match action {
            Action::RenderCacheRange { start, end } => {
                self.render_range(CacheRange::new(start, end));
            }
            Action::InvalidateRenderCache { start, end } => {
                self.render_cache.invalidate_overlapping(start, end);
            }
            Action::PruneRenderCache => {
                let clips = self.project.clips.clone();
                self.render_cache.prune_stale(&clips);
            }
            Action::ClearRenderCache => {
                self.render_cache.clear();
            }
            Action::SetRenderCacheCapacity(cap) => {
                self.render_cache = RenderCache::new(cap);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};
    use super::super::timeline::{Clip, ClipSource};

    fn color_clip(track: usize, start: f32, dur: f32, c: [f32; 4]) -> Clip {
        Clip {
            source: ClipSource::Color(c),
            track,
            start,
            duration: dur,
            ..Clip::default()
        }
    }

    #[test]
    fn range_normalises_and_contains() {
        let r = CacheRange::new(5.0, 2.0);
        assert_eq!(r.start, 2.0);
        assert_eq!(r.end, 5.0);
        assert!(r.contains(3.0));
        assert!(!r.contains(5.0)); // half-open
        assert!((r.len() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn hash_changes_when_stack_changes() {
        let r = CacheRange::new(0.0, 5.0);
        let stack_a = vec![color_clip(0, 0.0, 5.0, [1.0, 0.0, 0.0, 1.0])];
        let h_a = hash_clip_stack(&stack_a, r);
        // Same stack → same hash (deterministic).
        assert_eq!(hash_clip_stack(&stack_a, r), h_a);
        // Change the clip's color → different hash.
        let stack_b = vec![color_clip(0, 0.0, 5.0, [0.0, 1.0, 0.0, 1.0])];
        assert_ne!(hash_clip_stack(&stack_b, r), h_a);
        // Add a clip on another track → different hash.
        let mut stack_c = stack_a.clone();
        stack_c.push(color_clip(1, 0.0, 5.0, [0.0, 0.0, 1.0, 1.0]));
        assert_ne!(hash_clip_stack(&stack_c, r), h_a);
    }

    #[test]
    fn hash_ignores_clips_outside_range() {
        let r = CacheRange::new(0.0, 2.0);
        let base = vec![color_clip(0, 0.0, 2.0, [1.0, 0.0, 0.0, 1.0])];
        let h = hash_clip_stack(&base, r);
        // A clip well after the range doesn't affect the hash.
        let mut with_far = base.clone();
        with_far.push(color_clip(0, 100.0, 5.0, [0.0, 1.0, 0.0, 1.0]));
        assert_eq!(hash_clip_stack(&with_far, r), h);
    }

    #[test]
    fn status_reports_unrendered_stale_rendered() {
        let mut cache = RenderCache::new(8);
        let r = CacheRange::new(0.0, 4.0);
        // Nothing cached yet.
        assert_eq!(cache.status(r, 111), RenderBarStatus::Unrendered);
        // Render against hash 111.
        cache.insert(r, 111);
        assert_eq!(cache.status(r, 111), RenderBarStatus::Rendered);
        // The content changed (new hash) → stale.
        assert_eq!(cache.status(r, 222), RenderBarStatus::Stale);
    }

    #[test]
    fn ring_evicts_oldest() {
        let mut cache = RenderCache::new(2);
        cache.insert(CacheRange::new(0.0, 1.0), 1);
        cache.insert(CacheRange::new(1.0, 2.0), 2);
        cache.insert(CacheRange::new(2.0, 3.0), 3); // evicts [0,1)
        assert_eq!(cache.len(), 2);
        assert!(cache.segment_for(CacheRange::new(0.0, 1.0)).is_none());
        assert!(cache.segment_for(CacheRange::new(2.0, 3.0)).is_some());
    }

    #[test]
    fn reinsert_same_range_replaces() {
        let mut cache = RenderCache::new(8);
        let r = CacheRange::new(0.0, 4.0);
        let f1 = cache.insert(r, 1);
        let f2 = cache.insert(r, 2);
        assert_ne!(f1, f2);
        assert_eq!(cache.len(), 1, "same range replaced not duplicated");
        assert_eq!(cache.segment_for(r).unwrap().content_hash, 2);
    }

    #[test]
    fn invalidate_overlapping_removes_segments() {
        let mut cache = RenderCache::new(8);
        cache.insert(CacheRange::new(0.0, 2.0), 1);
        cache.insert(CacheRange::new(2.0, 4.0), 2);
        cache.insert(CacheRange::new(4.0, 6.0), 3);
        // An edit at [1.5, 2.5) overlaps the first two segments.
        let removed = cache.invalidate_overlapping(1.5, 2.5);
        assert_eq!(removed, 2);
        assert_eq!(cache.len(), 1);
        assert!(cache.segment_for(CacheRange::new(4.0, 6.0)).is_some());
    }

    #[test]
    fn render_range_and_status_via_app() {
        let mut app = App::new();
        app.project.clips = vec![color_clip(0, 0.0, 10.0, [0.5, 0.5, 0.5, 1.0])];
        let r = CacheRange::new(0.0, 5.0);
        // Unrendered initially.
        assert_eq!(app.render_status(r), RenderBarStatus::Unrendered);
        app.apply(Action::RenderCacheRange { start: 0.0, end: 5.0 });
        assert_eq!(app.render_status(r), RenderBarStatus::Rendered);
        assert_eq!(app.render_cache.len(), 1);
    }

    #[test]
    fn cache_invalidates_on_edit() {
        let mut app = App::new();
        app.project.clips = vec![color_clip(0, 0.0, 10.0, [0.5, 0.5, 0.5, 1.0])];
        let r = CacheRange::new(0.0, 5.0);
        app.apply(Action::RenderCacheRange { start: 0.0, end: 5.0 });
        assert_eq!(app.render_status(r), RenderBarStatus::Rendered);
        // Edit the clip stack (change color) → status goes stale (hash mismatch).
        app.project.clips[0].source = ClipSource::Color([0.9, 0.1, 0.1, 1.0]);
        assert_eq!(app.render_status(r), RenderBarStatus::Stale);
        // PruneRenderCache drops the now-stale segment.
        app.apply(Action::PruneRenderCache);
        assert_eq!(app.render_cache.len(), 0);
        assert_eq!(app.render_status(r), RenderBarStatus::Unrendered);
    }

    #[test]
    fn explicit_invalidate_action() {
        let mut app = App::new();
        app.project.clips = vec![color_clip(0, 0.0, 10.0, [0.5, 0.5, 0.5, 1.0])];
        app.apply(Action::RenderCacheRange { start: 0.0, end: 5.0 });
        app.apply(Action::InvalidateRenderCache { start: 1.0, end: 2.0 });
        assert_eq!(app.render_cache.len(), 0);
    }

    #[test]
    fn set_capacity_resets_cache() {
        let mut app = App::new();
        app.apply(Action::RenderCacheRange { start: 0.0, end: 1.0 });
        app.apply(Action::SetRenderCacheCapacity(4));
        assert_eq!(app.render_cache.capacity(), 4);
        assert_eq!(app.render_cache.len(), 0);
    }
}
