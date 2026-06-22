//! Loop recording / comping domain for Tone.
//!
//! Manages multi-take loop recording: TakeStacks group Takes recorded in loop
//! passes. Comp regions allow cherry-picking sections from different takes.
//! BakeComp finalises the comp into a single clip.

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum TakeStatus {
    Recording,
    Complete,
    Discarded,
}

#[derive(Clone, Debug)]
pub struct Take {
    pub id: usize,
    pub clip_id: usize,
    pub track_id: usize,
    pub beat_start: f32,
    pub beat_end: f32,
    pub status: TakeStatus,
}

#[derive(Clone, Debug)]
pub struct CompRegion {
    pub beat_start: f32,
    pub beat_end: f32,
}

#[derive(Clone, Debug)]
pub struct TakeStack {
    pub id: usize,
    pub track_id: usize,
    pub takes: Vec<Take>,
    pub comp_regions: Vec<CompRegion>,
    pub active_take_id: Option<usize>,
}

// ─── App impl ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_loop_recording(&mut self, action: Action) {
        match action {
            Action::StartLoopRecord { track_id, beat_start } => {
                let stack_id = self.next_take_stack_id;
                self.next_take_stack_id += 1;
                let take_id = self.next_take_id;
                self.next_take_id += 1;

                let take = Take {
                    id: take_id,
                    clip_id: 0,
                    track_id,
                    beat_start,
                    beat_end: beat_start,
                    status: TakeStatus::Recording,
                };
                let mut stack = TakeStack {
                    id: stack_id,
                    track_id,
                    takes: vec![],
                    comp_regions: vec![],
                    active_take_id: None,
                };
                stack.takes.push(take.clone());
                self.take_stacks.push(stack);
                self.loop_takes.push(take);
            }

            Action::EndLoopRecord { take_id, beat_end, clip_id } => {
                if let Some(t) = self.loop_takes.iter_mut().find(|t| t.id == take_id) {
                    t.beat_end = beat_end;
                    t.clip_id = clip_id;
                    t.status = TakeStatus::Complete;
                }
                // Keep stack's internal take in sync
                for stack in &mut self.take_stacks {
                    if let Some(t) = stack.takes.iter_mut().find(|t| t.id == take_id) {
                        t.beat_end = beat_end;
                        t.clip_id = clip_id;
                        t.status = TakeStatus::Complete;
                    }
                }
            }

            Action::DiscardTake { take_id } => {
                if let Some(t) = self.loop_takes.iter_mut().find(|t| t.id == take_id) {
                    t.status = TakeStatus::Discarded;
                }
                for stack in &mut self.take_stacks {
                    if let Some(t) = stack.takes.iter_mut().find(|t| t.id == take_id) {
                        t.status = TakeStatus::Discarded;
                    }
                }
            }

            Action::SetLoopActiveTake { stack_id, take_id } => {
                if let Some(s) = self.take_stacks.iter_mut().find(|s| s.id == stack_id) {
                    s.active_take_id = Some(take_id);
                }
            }

            Action::SetCompRegion { stack_id, beat_start, beat_end } => {
                if let Some(s) = self.take_stacks.iter_mut().find(|s| s.id == stack_id) {
                    s.comp_regions.push(CompRegion {
                        beat_start,
                        beat_end,
                    });
                }
            }

            Action::BakeComp { stack_id } => {
                // Stub: sets active_take_id as the "baked" take (no-op semantics)
                if let Some(s) = self.take_stacks.iter_mut().find(|s| s.id == stack_id) {
                    // Nothing to mutate — baking would produce a new clip via render
                    let _ = s.active_take_id;
                }
            }

            Action::DeleteTakeStack { stack_id } => {
                self.take_stacks.retain(|s| s.id != stack_id);
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
    fn start_loop_record_creates_stack_and_take() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        assert_eq!(app.take_stacks.len(), 1);
        assert_eq!(app.loop_takes.len(), 1);
        assert_eq!(app.loop_takes[0].status, TakeStatus::Recording);
    }

    #[test]
    fn start_loop_record_assigns_ids() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 4.0 });
        assert_eq!(app.take_stacks[0].id, 1);
        assert_eq!(app.loop_takes[0].id, 1);
        assert_eq!(app.next_take_stack_id, 2);
        assert_eq!(app.next_take_id, 2);
    }

    #[test]
    fn end_loop_record_completes_take() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let take_id = app.loop_takes[0].id;
        app.apply(Action::EndLoopRecord { take_id, beat_end: 16.0, clip_id: 5 });
        assert_eq!(app.loop_takes[0].status, TakeStatus::Complete);
        assert_eq!(app.loop_takes[0].beat_end, 16.0);
        assert_eq!(app.loop_takes[0].clip_id, 5);
    }

    #[test]
    fn end_loop_record_syncs_stack_take() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 2, beat_start: 0.0 });
        let take_id = app.loop_takes[0].id;
        app.apply(Action::EndLoopRecord { take_id, beat_end: 8.0, clip_id: 3 });
        assert_eq!(app.take_stacks[0].takes[0].status, TakeStatus::Complete);
    }

    #[test]
    fn discard_take_sets_discarded() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let take_id = app.loop_takes[0].id;
        app.apply(Action::EndLoopRecord { take_id, beat_end: 4.0, clip_id: 1 });
        app.apply(Action::DiscardTake { take_id });
        assert_eq!(app.loop_takes[0].status, TakeStatus::Discarded);
    }

    #[test]
    fn set_loop_active_take() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let stack_id = app.take_stacks[0].id;
        let take_id = app.loop_takes[0].id;
        app.apply(Action::SetLoopActiveTake { stack_id, take_id });
        assert_eq!(app.take_stacks[0].active_take_id, Some(take_id));
    }

    #[test]
    fn set_comp_region_pushes_region() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let stack_id = app.take_stacks[0].id;
        app.apply(Action::SetCompRegion { stack_id, beat_start: 2.0, beat_end: 6.0 });
        assert_eq!(app.take_stacks[0].comp_regions.len(), 1);
        assert_eq!(app.take_stacks[0].comp_regions[0].beat_start, 2.0);
    }

    #[test]
    fn set_comp_region_multiple() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let stack_id = app.take_stacks[0].id;
        app.apply(Action::SetCompRegion { stack_id, beat_start: 0.0, beat_end: 4.0 });
        app.apply(Action::SetCompRegion { stack_id, beat_start: 4.0, beat_end: 8.0 });
        assert_eq!(app.take_stacks[0].comp_regions.len(), 2);
    }

    #[test]
    fn bake_comp_is_noop_stub() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let stack_id = app.take_stacks[0].id;
        // Should not panic
        app.apply(Action::BakeComp { stack_id });
        assert_eq!(app.take_stacks.len(), 1);
    }

    #[test]
    fn delete_take_stack_removes_it() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        let stack_id = app.take_stacks[0].id;
        app.apply(Action::DeleteTakeStack { stack_id });
        assert!(app.take_stacks.is_empty());
    }

    #[test]
    fn multiple_loop_record_sessions() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        app.apply(Action::StartLoopRecord { track_id: 2, beat_start: 0.0 });
        assert_eq!(app.take_stacks.len(), 2);
        assert_eq!(app.loop_takes.len(), 2);
    }

    #[test]
    fn delete_take_stack_unknown_id_is_noop() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        app.apply(Action::DeleteTakeStack { stack_id: 9999 });
        assert_eq!(app.take_stacks.len(), 1);
    }

    #[test]
    fn take_stack_starts_with_no_active_take() {
        let mut app = fresh();
        app.apply(Action::StartLoopRecord { track_id: 1, beat_start: 0.0 });
        assert!(app.take_stacks[0].active_take_id.is_none());
    }
}
