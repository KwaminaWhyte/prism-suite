use super::{App, Action};

/// A scene groups a set of layers with its own duration.
#[derive(Clone, Debug)]
pub struct Scene {
    pub id: usize,
    pub name: String,
    pub duration_frames: usize,
    pub layer_ids: Vec<usize>,
}

impl App {
    pub fn apply_scenes(&mut self, action: Action) {
        match action {
            Action::AddScene { name, duration_frames } => {
                let id = self.next_scene_id;
                self.next_scene_id += 1;
                self.scenes.push(Scene {
                    id,
                    name,
                    duration_frames,
                    layer_ids: Vec::new(),
                });
            }
            Action::RemoveScene { scene_id } => {
                // Never remove the last scene.
                if self.scenes.len() <= 1 {
                    return;
                }
                self.scenes.retain(|s| s.id != scene_id);
                // If active scene was removed, switch to first remaining.
                if self.active_scene_id == scene_id {
                    self.active_scene_id = self.scenes[0].id;
                }
            }
            Action::RenameScene { scene_id, name } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    s.name = name;
                }
            }
            Action::SetActiveScene { scene_id } => {
                if self.scenes.iter().any(|s| s.id == scene_id) {
                    self.active_scene_id = scene_id;
                }
            }
            Action::SetSceneDuration { scene_id, frames } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    s.duration_frames = frames.max(1);
                }
            }
            Action::DuplicateScene { scene_id } => {
                if let Some(src) = self.scenes.iter().find(|s| s.id == scene_id).cloned() {
                    let new_id = self.next_scene_id;
                    self.next_scene_id += 1;
                    self.scenes.push(Scene {
                        id: new_id,
                        name: format!("{} Copy", src.name),
                        duration_frames: src.duration_frames,
                        layer_ids: src.layer_ids.clone(),
                    });
                }
            }
            Action::MoveScene { scene_id, new_index } => {
                if let Some(pos) = self.scenes.iter().position(|s| s.id == scene_id) {
                    let scene = self.scenes.remove(pos);
                    let insert_at = new_index.min(self.scenes.len());
                    self.scenes.insert(insert_at, scene);
                }
            }
            Action::AddLayerToScene { scene_id, layer_id } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    if !s.layer_ids.contains(&layer_id) {
                        s.layer_ids.push(layer_id);
                    }
                }
            }
            Action::RemoveLayerFromScene { scene_id, layer_id } => {
                if let Some(s) = self.scenes.iter_mut().find(|s| s.id == scene_id) {
                    s.layer_ids.retain(|&lid| lid != layer_id);
                }
            }
            _ => {}
        }
    }

    /// Returns the currently active scene, if any.
    pub fn active_scene(&self) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == self.active_scene_id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_initial_scene_exists() {
        let a = app();
        assert_eq!(a.scenes.len(), 1);
        assert_eq!(a.scenes[0].name, "Scene 1");
        assert_eq!(a.active_scene_id, a.scenes[0].id);
    }

    #[test]
    fn test_add_scene() {
        let mut a = app();
        a.apply(Action::AddScene { name: "Intro".to_string(), duration_frames: 120 });
        assert_eq!(a.scenes.len(), 2);
        assert_eq!(a.scenes[1].name, "Intro");
        assert_eq!(a.scenes[1].duration_frames, 120);
    }

    #[test]
    fn test_add_scene_id_increments() {
        let mut a = app();
        a.apply(Action::AddScene { name: "A".to_string(), duration_frames: 60 });
        a.apply(Action::AddScene { name: "B".to_string(), duration_frames: 60 });
        assert_ne!(a.scenes[1].id, a.scenes[2].id);
    }

    #[test]
    fn test_remove_scene() {
        let mut a = app();
        a.apply(Action::AddScene { name: "Second".to_string(), duration_frames: 60 });
        let sid = a.scenes[1].id;
        a.apply(Action::RemoveScene { scene_id: sid });
        assert_eq!(a.scenes.len(), 1);
    }

    #[test]
    fn test_remove_last_scene_noop() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::RemoveScene { scene_id: sid });
        assert_eq!(a.scenes.len(), 1, "Cannot remove the last scene");
    }

    #[test]
    fn test_remove_active_scene_switches_to_first() {
        let mut a = app();
        a.apply(Action::AddScene { name: "Second".to_string(), duration_frames: 60 });
        let first_id = a.scenes[0].id;
        let second_id = a.scenes[1].id;
        a.apply(Action::SetActiveScene { scene_id: second_id });
        a.apply(Action::RemoveScene { scene_id: second_id });
        assert_eq!(a.active_scene_id, first_id);
    }

    #[test]
    fn test_rename_scene() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::RenameScene { scene_id: sid, name: "Main".to_string() });
        assert_eq!(a.scenes[0].name, "Main");
    }

    #[test]
    fn test_set_active_scene() {
        let mut a = app();
        a.apply(Action::AddScene { name: "Two".to_string(), duration_frames: 60 });
        let sid = a.scenes[1].id;
        a.apply(Action::SetActiveScene { scene_id: sid });
        assert_eq!(a.active_scene_id, sid);
    }

    #[test]
    fn test_set_active_scene_nonexistent_noop() {
        let mut a = app();
        let original = a.active_scene_id;
        a.apply(Action::SetActiveScene { scene_id: 9999 });
        assert_eq!(a.active_scene_id, original);
    }

    #[test]
    fn test_set_scene_duration() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::SetSceneDuration { scene_id: sid, frames: 480 });
        assert_eq!(a.scenes[0].duration_frames, 480);
    }

    #[test]
    fn test_set_scene_duration_min_one() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::SetSceneDuration { scene_id: sid, frames: 0 });
        assert_eq!(a.scenes[0].duration_frames, 1);
    }

    #[test]
    fn test_duplicate_scene() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::DuplicateScene { scene_id: sid });
        assert_eq!(a.scenes.len(), 2);
        assert!(a.scenes[1].name.contains("Copy"));
        assert_eq!(a.scenes[1].duration_frames, a.scenes[0].duration_frames);
    }

    #[test]
    fn test_move_scene() {
        let mut a = app();
        a.apply(Action::AddScene { name: "B".to_string(), duration_frames: 60 });
        a.apply(Action::AddScene { name: "C".to_string(), duration_frames: 60 });
        let first_id = a.scenes[0].id;
        // Move first scene to position 2
        a.apply(Action::MoveScene { scene_id: first_id, new_index: 2 });
        assert_eq!(a.scenes[2].id, first_id);
    }

    #[test]
    fn test_add_layer_to_scene() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::AddLayerToScene { scene_id: sid, layer_id: 42 });
        assert!(a.scenes[0].layer_ids.contains(&42));
    }

    #[test]
    fn test_add_layer_to_scene_no_duplicates() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::AddLayerToScene { scene_id: sid, layer_id: 42 });
        a.apply(Action::AddLayerToScene { scene_id: sid, layer_id: 42 });
        assert_eq!(a.scenes[0].layer_ids.len(), 1);
    }

    #[test]
    fn test_remove_layer_from_scene() {
        let mut a = app();
        let sid = a.scenes[0].id;
        a.apply(Action::AddLayerToScene { scene_id: sid, layer_id: 10 });
        a.apply(Action::RemoveLayerFromScene { scene_id: sid, layer_id: 10 });
        assert!(a.scenes[0].layer_ids.is_empty());
    }

    #[test]
    fn test_active_scene_helper() {
        let a = app();
        let scene = a.active_scene();
        assert!(scene.is_some());
        assert_eq!(scene.unwrap().id, a.active_scene_id);
    }
}
