//! Scenes domain — Session view scenes, clip launch, arrangement mode + apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// What a scene slot does when its follow time elapses.
#[derive(Clone, Debug, PartialEq)]
pub enum FollowAction {
    None,
    Stop,
    PlayNext,
    PlayPrev,
    PlayFirst,
    PlayRandom,
}

/// One slot in a scene — binds a track to an optional clip and a follow action.
#[derive(Clone, Debug)]
pub struct SceneSlot {
    pub track_id: usize,
    pub clip_id: Option<usize>,
    pub follow_action: FollowAction,
    pub follow_after_bars: f32,
}

/// A scene groups one clip slot per track for session-view launching.
#[derive(Clone, Debug)]
pub struct Scene {
    pub id: usize,
    pub name: String,
    pub bpm_override: Option<f32>,
    pub color: String,
    pub slots: Vec<SceneSlot>,
}

/// Whether the project is in session (clip-launch) or arrangement (timeline) mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrangementMode {
    Session,
    Arrangement,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_scenes(&mut self, action: Action) {
        match action {
            Action::AddScene { name } => {
                let id = self.next_scene_id;
                self.next_scene_id += 1;
                self.scenes.push(Scene {
                    id,
                    name,
                    bpm_override: None,
                    color: "#3B82F6".to_string(),
                    slots: Vec::new(),
                });
            }
            Action::DeleteScene { scene_id } => {
                self.scenes.retain(|s| s.id != scene_id);
                if self.active_scene_id == Some(scene_id) {
                    self.active_scene_id = None;
                }
            }
            Action::RenameScene { scene_id, name } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    s.name = name;
                }
            }
            Action::SetSceneColor { scene_id, color } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    s.color = color;
                }
            }
            Action::SetSceneBpmOverride { scene_id, bpm } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    s.bpm_override = bpm;
                }
            }
            Action::AssignClipToScene { scene_id, track_id, clip_id } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    if let Some(slot) = s.slots.iter_mut().find(|slot| slot.track_id == track_id) {
                        slot.clip_id = clip_id;
                    } else {
                        s.slots.push(SceneSlot {
                            track_id,
                            clip_id,
                            follow_action: FollowAction::None,
                            follow_after_bars: 1.0,
                        });
                    }
                }
            }
            Action::SetFollowAction { scene_id, track_id, action: follow } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    if let Some(slot) = s.slots.iter_mut().find(|slot| slot.track_id == track_id) {
                        slot.follow_action = follow;
                    }
                }
            }
            Action::LaunchScene { scene_id } => {
                self.active_scene_id = Some(scene_id);
            }
            Action::SetArrangementMode(mode) => {
                self.arrangement_mode = mode;
            }
            Action::DuplicateScene { scene_id } => {
                if let Some(src) = self.scenes.iter().find(|s| s.id == scene_id).cloned() {
                    let new_id = self.next_scene_id;
                    self.next_scene_id += 1;
                    let mut dup = src.clone();
                    dup.id = new_id;
                    dup.name = format!("{} (copy)", src.name);
                    self.scenes.push(dup);
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::{ArrangementMode, FollowAction};

    fn fresh() -> App {
        App::new()
    }

    fn make_scene(app: &mut App, name: &str) -> usize {
        let before = app.scenes.len();
        app.apply(Action::AddScene { name: name.to_string() });
        app.scenes[before].id
    }

    #[test]
    fn add_scene() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "Verse");
        assert_eq!(app.scenes.len(), 1);
        assert_eq!(app.scenes[0].id, sid);
        assert_eq!(app.scenes[0].name, "Verse");
        assert!(app.scenes[0].bpm_override.is_none());
    }

    #[test]
    fn add_multiple_scenes_increment_id() {
        let mut app = fresh();
        let s1 = make_scene(&mut app, "A");
        let s2 = make_scene(&mut app, "B");
        assert_ne!(s1, s2);
        assert_eq!(app.scenes.len(), 2);
    }

    #[test]
    fn delete_scene() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "Verse");
        app.apply(Action::DeleteScene { scene_id: sid });
        assert!(app.scenes.is_empty());
    }

    #[test]
    fn delete_active_scene_clears_active() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "Verse");
        app.apply(Action::LaunchScene { scene_id: sid });
        assert_eq!(app.active_scene_id, Some(sid));
        app.apply(Action::DeleteScene { scene_id: sid });
        assert_eq!(app.active_scene_id, None);
    }

    #[test]
    fn delete_non_active_scene_keeps_active() {
        let mut app = fresh();
        let s1 = make_scene(&mut app, "A");
        let s2 = make_scene(&mut app, "B");
        app.apply(Action::LaunchScene { scene_id: s1 });
        app.apply(Action::DeleteScene { scene_id: s2 });
        assert_eq!(app.active_scene_id, Some(s1));
    }

    #[test]
    fn rename_scene() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "Verse");
        app.apply(Action::RenameScene { scene_id: sid, name: "Chorus".to_string() });
        assert_eq!(app.scenes[0].name, "Chorus");
    }

    #[test]
    fn set_scene_color() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "A");
        app.apply(Action::SetSceneColor { scene_id: sid, color: "#FF0000".to_string() });
        assert_eq!(app.scenes[0].color, "#FF0000");
    }

    #[test]
    fn set_scene_bpm_override() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "A");
        app.apply(Action::SetSceneBpmOverride { scene_id: sid, bpm: Some(140.0) });
        assert_eq!(app.scenes[0].bpm_override, Some(140.0));
        app.apply(Action::SetSceneBpmOverride { scene_id: sid, bpm: None });
        assert!(app.scenes[0].bpm_override.is_none());
    }

    #[test]
    fn assign_clip_to_scene_new_slot() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "A");
        app.apply(Action::AssignClipToScene { scene_id: sid, track_id: 1, clip_id: Some(42) });
        assert_eq!(app.scenes[0].slots.len(), 1);
        assert_eq!(app.scenes[0].slots[0].track_id, 1);
        assert_eq!(app.scenes[0].slots[0].clip_id, Some(42));
    }

    #[test]
    fn assign_clip_to_scene_updates_existing_slot() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "A");
        app.apply(Action::AssignClipToScene { scene_id: sid, track_id: 1, clip_id: Some(42) });
        app.apply(Action::AssignClipToScene { scene_id: sid, track_id: 1, clip_id: Some(99) });
        assert_eq!(app.scenes[0].slots.len(), 1);
        assert_eq!(app.scenes[0].slots[0].clip_id, Some(99));
    }

    #[test]
    fn set_follow_action() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "A");
        app.apply(Action::AssignClipToScene { scene_id: sid, track_id: 0, clip_id: Some(1) });
        app.apply(Action::SetFollowAction { scene_id: sid, track_id: 0, action: FollowAction::PlayNext });
        assert_eq!(app.scenes[0].slots[0].follow_action, FollowAction::PlayNext);
    }

    #[test]
    fn launch_scene() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "Verse");
        app.apply(Action::LaunchScene { scene_id: sid });
        assert_eq!(app.active_scene_id, Some(sid));
    }

    #[test]
    fn set_arrangement_mode() {
        let mut app = fresh();
        assert_eq!(app.arrangement_mode, ArrangementMode::Arrangement);
        app.apply(Action::SetArrangementMode(ArrangementMode::Session));
        assert_eq!(app.arrangement_mode, ArrangementMode::Session);
        app.apply(Action::SetArrangementMode(ArrangementMode::Arrangement));
        assert_eq!(app.arrangement_mode, ArrangementMode::Arrangement);
    }

    #[test]
    fn duplicate_scene() {
        let mut app = fresh();
        let sid = make_scene(&mut app, "Verse");
        app.apply(Action::AssignClipToScene { scene_id: sid, track_id: 0, clip_id: Some(5) });
        app.apply(Action::DuplicateScene { scene_id: sid });
        assert_eq!(app.scenes.len(), 2);
        let dup = &app.scenes[1];
        assert_eq!(dup.name, "Verse (copy)");
        assert_ne!(dup.id, sid);
        assert_eq!(dup.slots.len(), 1);
        assert_eq!(dup.slots[0].clip_id, Some(5));
    }

    #[test]
    fn duplicate_nonexistent_scene_is_noop() {
        let mut app = fresh();
        app.apply(Action::DuplicateScene { scene_id: 999 });
        assert!(app.scenes.is_empty());
    }
}
