use super::{App, Action};

#[derive(Clone, Debug, PartialEq)]
pub enum SlotAction {
    Launch,
    Stop,
    Record,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SlotFollowAction {
    None,
    Stop,
    PlayNext,
    PlayPrev,
    PlayFirst,
    PlayLast,
    PlayAny,
    PlayOther { slot_index: usize },
}

#[derive(Clone, Debug)]
pub struct ClipSlot {
    pub id: usize,
    pub track_id: usize,
    pub slot_index: usize,
    pub clip_id: Option<usize>,
    pub is_playing: bool,
    pub is_recording: bool,
    pub queued_action: Option<SlotAction>,
    pub follow_action: SlotFollowAction,
    pub follow_time_bars: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaunchQuantize {
    None,
    EighthNote,
    QuarterNote,
    HalfNote,
    OneBar,
    TwoBars,
    FourBars,
}

#[derive(Clone, Debug)]
pub struct LaunchGrid {
    pub tracks: Vec<usize>,
    pub num_slots: usize,
    pub master_launch_quantize: LaunchQuantize,
    pub link_enabled: bool,
}

impl LaunchGrid {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            num_slots: 8,
            master_launch_quantize: LaunchQuantize::OneBar,
            link_enabled: false,
        }
    }
}

impl Default for LaunchGrid {
    fn default() -> Self { Self::new() }
}

impl App {
    pub(crate) fn apply_clip_launch(&mut self, action: Action) {
        match action {
            Action::AddClipSlot { track_id, slot_index } => {
                let id = self.next_slot_id;
                self.next_slot_id += 1;
                self.clip_slots.push(ClipSlot {
                    id,
                    track_id,
                    slot_index,
                    clip_id: None,
                    is_playing: false,
                    is_recording: false,
                    queued_action: None,
                    follow_action: SlotFollowAction::None,
                    follow_time_bars: 1.0,
                });
            }
            Action::AssignClipToSlot { slot_id, clip_id } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.clip_id = clip_id;
                }
            }
            Action::LaunchSlot { slot_id } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.is_playing = true;
                    s.queued_action = None;
                }
            }
            Action::StopSlot { slot_id } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.is_playing = false;
                    s.is_recording = false;
                    s.queued_action = None;
                }
            }
            Action::QueueSlotAction { slot_id, action: slot_action } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.queued_action = Some(slot_action);
                }
            }
            Action::ClearSlotQueue { slot_id } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.queued_action = None;
                }
            }
            Action::SetSlotFollowAction { slot_id, action: fa } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.follow_action = fa;
                }
            }
            Action::SetSlotFollowTime { slot_id, bars } => {
                if let Some(s) = self.clip_slots.iter_mut().find(|s| s.id == slot_id) {
                    s.follow_time_bars = bars.max(0.0);
                }
            }
            Action::SetLaunchQuantize(q) => {
                self.launch_grid.master_launch_quantize = q;
            }
            Action::SetLinkEnabled(enabled) => {
                self.launch_grid.link_enabled = enabled;
            }
            Action::StopAllSlots => {
                for s in &mut self.clip_slots {
                    s.is_playing = false;
                    s.is_recording = false;
                }
            }
            Action::LaunchSceneSlots { scene_id } => {
                // Find all slots whose slot_index matches scene_id and launch them
                for s in &mut self.clip_slots {
                    if s.slot_index == scene_id {
                        s.is_playing = true;
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App { App::new() }

    #[test]
    fn add_clip_slot() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        assert_eq!(app.clip_slots.len(), 1);
        assert_eq!(app.clip_slots[0].track_id, 0);
        assert!(!app.clip_slots[0].is_playing);
    }

    #[test]
    fn add_multiple_slots_unique_ids() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 1 });
        assert_ne!(app.clip_slots[0].id, app.clip_slots[1].id);
    }

    #[test]
    fn assign_clip_to_slot() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::AssignClipToSlot { slot_id: sid, clip_id: Some(42) });
        assert_eq!(app.clip_slots[0].clip_id, Some(42));
    }

    #[test]
    fn launch_slot_sets_playing() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::LaunchSlot { slot_id: sid });
        assert!(app.clip_slots[0].is_playing);
    }

    #[test]
    fn stop_slot_clears_playing() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::LaunchSlot { slot_id: sid });
        app.apply(Action::StopSlot { slot_id: sid });
        assert!(!app.clip_slots[0].is_playing);
    }

    #[test]
    fn queue_slot_action() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::QueueSlotAction { slot_id: sid, action: SlotAction::Launch });
        assert_eq!(app.clip_slots[0].queued_action, Some(SlotAction::Launch));
    }

    #[test]
    fn clear_slot_queue() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::QueueSlotAction { slot_id: sid, action: SlotAction::Stop });
        app.apply(Action::ClearSlotQueue { slot_id: sid });
        assert!(app.clip_slots[0].queued_action.is_none());
    }

    #[test]
    fn set_slot_follow_action() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::SetSlotFollowAction { slot_id: sid, action: SlotFollowAction::PlayNext });
        assert_eq!(app.clip_slots[0].follow_action, SlotFollowAction::PlayNext);
    }

    #[test]
    fn set_slot_follow_time() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        let sid = app.clip_slots[0].id;
        app.apply(Action::SetSlotFollowTime { slot_id: sid, bars: 4.0 });
        assert!((app.clip_slots[0].follow_time_bars - 4.0).abs() < 0.01);
    }

    #[test]
    fn set_launch_quantize() {
        let mut app = fresh();
        app.apply(Action::SetLaunchQuantize(LaunchQuantize::HalfNote));
        assert_eq!(app.launch_grid.master_launch_quantize, LaunchQuantize::HalfNote);
    }

    #[test]
    fn set_link_enabled() {
        let mut app = fresh();
        app.apply(Action::SetLinkEnabled(true));
        assert!(app.launch_grid.link_enabled);
    }

    #[test]
    fn stop_all_slots() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 0 });
        app.apply(Action::AddClipSlot { track_id: 1, slot_index: 0 });
        let s0 = app.clip_slots[0].id;
        let s1 = app.clip_slots[1].id;
        app.apply(Action::LaunchSlot { slot_id: s0 });
        app.apply(Action::LaunchSlot { slot_id: s1 });
        app.apply(Action::StopAllSlots);
        assert!(!app.clip_slots[0].is_playing);
        assert!(!app.clip_slots[1].is_playing);
    }

    #[test]
    fn launch_scene_slots() {
        let mut app = fresh();
        app.apply(Action::AddClipSlot { track_id: 0, slot_index: 2 });
        app.apply(Action::AddClipSlot { track_id: 1, slot_index: 2 });
        app.apply(Action::AddClipSlot { track_id: 2, slot_index: 3 });
        app.apply(Action::LaunchSceneSlots { scene_id: 2 });
        assert!(app.clip_slots[0].is_playing);
        assert!(app.clip_slots[1].is_playing);
        assert!(!app.clip_slots[2].is_playing);
    }
}
