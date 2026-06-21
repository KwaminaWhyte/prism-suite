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
                    c.motion_x = x;
                    c.motion_y = y;
                    self.host.mark_dirty();
                }
            }
            Action::SetClipMotionScale { clip_idx, sx, sy } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    c.motion_scale_x = sx.max(0.01);
                    c.motion_scale_y = sy.max(0.01);
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
