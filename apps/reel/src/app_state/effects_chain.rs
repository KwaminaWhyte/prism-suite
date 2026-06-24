use super::{App, Action};

pub trait AppEffectsExt {
    fn apply_effects(&mut self, action: Action);
}

impl AppEffectsExt for App {
    fn apply_effects(&mut self, action: Action) {
        match action {
            Action::AddClipEffect { clip_idx, effect } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.effects.push(effect);
                    self.host.mark_dirty();
                }
            }
            Action::RemoveClipEffect { clip_idx, effect_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if effect_idx < c.effects.len() {
                        c.effects.remove(effect_idx);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetClipEffect { clip_idx, effect_idx, effect } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if let Some(e) = c.effects.get_mut(effect_idx) {
                        *e = effect;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleClipEffect { clip_idx, effect_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if let Some(e) = c.effects.get_mut(effect_idx) {
                        e.enabled = !e.enabled;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ReorderClipEffects { clip_idx, from, to } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    let len = c.effects.len();
                    if from < len && to < len && from != to {
                        let effect = c.effects.remove(from);
                        c.effects.insert(to, effect);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ClearClipEffects { clip_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.effects.clear();
                    self.host.mark_dirty();
                }
            }
            Action::SetClipMotion { clip_idx, x, y } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    // A NaN axis means "leave this axis unchanged" — lets a single
                    // typeable X-or-Y field edit one component without clobbering
                    // the other.
                    if x.is_finite() { c.motion_x = x; }
                    if y.is_finite() { c.motion_y = y; }
                    self.host.mark_dirty();
                }
            }
            Action::SetClipMotionScale { clip_idx, sx, sy } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    // NaN axis = "leave unchanged" (single-axis typeable field).
                    if sx.is_finite() { c.motion_scale_x = sx.max(0.01); }
                    if sy.is_finite() { c.motion_scale_y = sy.max(0.01); }
                    self.host.mark_dirty();
                }
            }
            Action::SetClipMotionRotation { clip_idx, angle } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.motion_rotation = angle;
                    self.host.mark_dirty();
                }
            }
            Action::ResetClipMotion { clip_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.motion_x = 0.0;
                    c.motion_y = 0.0;
                    c.motion_scale_x = 1.0;
                    c.motion_scale_y = 1.0;
                    c.motion_rotation = 0.0;
                    self.host.mark_dirty();
                }
            }
            Action::SetSceneEditSensitivity(s) => {
                self.scene_edit_sensitivity = s.clamp(0.0, 1.0);
            }
            Action::DetectSceneEdits { clip_idx } => {
                if let Some(c) = self.project.clips.get(clip_idx) {
                    let duration = c.duration;
                    let n = (self.scene_edit_sensitivity * 5.0).ceil() as usize;
                    let cut_times = if n == 0 || duration <= 0.0 {
                        Vec::new()
                    } else {
                        (1..=n).map(|i| c.start + duration * (i as f32) / (n as f32 + 1.0)).collect()
                    };
                    self.last_scene_edit_result = Some(super::timeline::SceneEditResult { clip_idx, cut_times });
                }
            }
            Action::ApplySceneEditSplits { clip_idx } => {
                if let Some(ref result) = self.last_scene_edit_result.clone() {
                    if result.clip_idx == clip_idx { self.last_scene_edit_result = None; }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};
    use super::super::timeline::{ClipEffect, ClipEffectKind};

    #[test]
    fn test_add_clip_effect() {
        let mut app = App::new();
        let effect = ClipEffect::default();
        app.apply(Action::AddClipEffect { clip_idx: 0, effect });
        assert_eq!(app.project.clips[0].effects.len(), 1);
    }

    #[test]
    fn test_remove_clip_effect() {
        let mut app = App::new();
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect::default() });
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect { kind: ClipEffectKind::Sharpen, ..ClipEffect::default() } });
        app.apply(Action::RemoveClipEffect { clip_idx: 0, effect_idx: 0 });
        assert_eq!(app.project.clips[0].effects.len(), 1);
        assert_eq!(app.project.clips[0].effects[0].kind, ClipEffectKind::Sharpen);
    }

    #[test]
    fn test_toggle_clip_effect() {
        let mut app = App::new();
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect::default() });
        assert!(app.project.clips[0].effects[0].enabled);
        app.apply(Action::ToggleClipEffect { clip_idx: 0, effect_idx: 0 });
        assert!(!app.project.clips[0].effects[0].enabled);
        app.apply(Action::ToggleClipEffect { clip_idx: 0, effect_idx: 0 });
        assert!(app.project.clips[0].effects[0].enabled);
    }

    #[test]
    fn test_clear_clip_effects() {
        let mut app = App::new();
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect::default() });
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect::default() });
        app.apply(Action::ClearClipEffects { clip_idx: 0 });
        assert!(app.project.clips[0].effects.is_empty());
    }

    #[test]
    fn test_reorder_clip_effects() {
        let mut app = App::new();
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect { kind: ClipEffectKind::GaussianBlur, ..ClipEffect::default() } });
        app.apply(Action::AddClipEffect { clip_idx: 0, effect: ClipEffect { kind: ClipEffectKind::Glow, ..ClipEffect::default() } });
        // swap: from=0, to=1 → Glow first, GaussianBlur second
        app.apply(Action::ReorderClipEffects { clip_idx: 0, from: 0, to: 1 });
        assert_eq!(app.project.clips[0].effects[0].kind, ClipEffectKind::Glow);
        assert_eq!(app.project.clips[0].effects[1].kind, ClipEffectKind::GaussianBlur);
    }

    #[test]
    fn test_clip_motion_set() {
        let mut app = App::new();
        app.apply(Action::SetClipMotion { clip_idx: 0, x: 100.0, y: -50.0 });
        let c = &app.project.clips[0];
        assert!((c.motion_x - 100.0).abs() < 1e-5);
        assert!((c.motion_y - (-50.0)).abs() < 1e-5);
    }

    #[test]
    fn test_clip_motion_scale_clamp() {
        let mut app = App::new();
        app.apply(Action::SetClipMotionScale { clip_idx: 0, sx: 0.0, sy: -1.0 });
        let c = &app.project.clips[0];
        assert!(c.motion_scale_x >= 0.01);
        assert!(c.motion_scale_y >= 0.01);
    }

    #[test]
    fn test_clip_motion_nan_axis_is_kept() {
        let mut app = App::new();
        app.apply(Action::SetClipMotion { clip_idx: 0, x: 100.0, y: 200.0 });
        // Editing only X (Y = NaN) leaves Y untouched.
        app.apply(Action::SetClipMotion { clip_idx: 0, x: 42.0, y: f32::NAN });
        let c = &app.project.clips[0];
        assert!((c.motion_x - 42.0).abs() < 1e-5);
        assert!((c.motion_y - 200.0).abs() < 1e-5, "Y preserved across single-axis edit");
        // Same for scale: editing only Y leaves X.
        app.apply(Action::SetClipMotionScale { clip_idx: 0, sx: 2.0, sy: 3.0 });
        app.apply(Action::SetClipMotionScale { clip_idx: 0, sx: f32::NAN, sy: 5.0 });
        let c = &app.project.clips[0];
        assert!((c.motion_scale_x - 2.0).abs() < 1e-5, "X preserved");
        assert!((c.motion_scale_y - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_clip_motion_reset() {
        let mut app = App::new();
        app.apply(Action::SetClipMotion { clip_idx: 0, x: 200.0, y: 300.0 });
        app.apply(Action::SetClipMotionScale { clip_idx: 0, sx: 2.0, sy: 3.0 });
        app.apply(Action::SetClipMotionRotation { clip_idx: 0, angle: 45.0 });
        app.apply(Action::ResetClipMotion { clip_idx: 0 });
        let c = &app.project.clips[0];
        assert!((c.motion_x).abs() < 1e-5);
        assert!((c.motion_y).abs() < 1e-5);
        assert!((c.motion_scale_x - 1.0).abs() < 1e-5);
        assert!((c.motion_scale_y - 1.0).abs() < 1e-5);
        assert!((c.motion_rotation).abs() < 1e-5);
    }

    #[test]
    fn test_scene_edit_sensitivity() {
        let mut app = App::new();
        app.apply(Action::SetSceneEditSensitivity(0.7));
        assert!((app.scene_edit_sensitivity - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_scene_edit_sensitivity_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSceneEditSensitivity(1.5));
        assert!((app.scene_edit_sensitivity - 1.0).abs() < 1e-5);
        app.apply(Action::SetSceneEditSensitivity(-0.3));
        assert!((app.scene_edit_sensitivity).abs() < 1e-5);
    }

    #[test]
    fn test_detect_scene_edits_produces_result() {
        let mut app = App::new();
        app.apply(Action::SetSceneEditSensitivity(0.6));
        app.apply(Action::DetectSceneEdits { clip_idx: 0 });
        assert!(app.last_scene_edit_result.is_some());
        let result = app.last_scene_edit_result.as_ref().unwrap();
        assert_eq!(result.clip_idx, 0);
        assert!(!result.cut_times.is_empty());
    }

    #[test]
    fn test_set_project_name() {
        let mut app = App::new();
        app.apply(Action::SetProjectName("My Film".to_string()));
        assert_eq!(app.project_name, "My Film");
    }

    #[test]
    fn test_set_project_path_adds_to_recent() {
        let mut app = App::new();
        let path = std::path::PathBuf::from("/tmp/my_project.reel");
        app.apply(Action::SetProjectPath(path.clone()));
        assert_eq!(app.project_path, Some(path.clone()));
        assert!(app.recent_project_paths.contains(&path));
    }

    #[test]
    fn test_recent_projects_capped_at_10() {
        let mut app = App::new();
        for i in 0..15 {
            app.apply(Action::AddRecentProject(std::path::PathBuf::from(format!("/tmp/project_{}.reel", i))));
        }
        assert!(app.recent_project_paths.len() <= 10);
    }

    #[test]
    fn test_auto_save_interval_min() {
        let mut app = App::new();
        app.apply(Action::SetAutoSaveInterval(5));
        assert_eq!(app.auto_save_interval_sec, 30);
    }
}
