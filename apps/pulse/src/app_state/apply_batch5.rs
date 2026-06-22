use super::*;
use crate::app_state::effects_chain::{
    Batch5Effect, GlowColors, GlowChannel, TilingMode, OverflowMode, StrokePath, StrokeComposite,
};

impl App {
    pub(super) fn apply_batch5(&mut self, action: Action) {
        match action {
            // --- Effects ---
            Action::AddMotionBlurEffect { layer_id, angle, distance } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::MotionBlur { angle, distance }
                );
                self.host.mark_dirty();
            }
            Action::AddGlowEffect { layer_id, threshold, radius, intensity } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::Glow {
                        threshold,
                        radius,
                        intensity,
                        glow_colors: GlowColors::Original,
                        glow_channel: GlowChannel::Luminance,
                    }
                );
                self.host.mark_dirty();
            }
            Action::AddCcRepeTileEffect { layer_id, expand_right, expand_left, expand_up, expand_down } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::CcRepeTile { expand_right, expand_left, expand_up, expand_down, tiling: TilingMode::Tile }
                );
                self.host.mark_dirty();
            }
            Action::AddPosterizeTime { layer_id, frame_rate } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::PosterizeTime { frame_rate: frame_rate.max(1.0) }
                );
                self.host.mark_dirty();
            }
            Action::AddCellPattern { layer_id, pattern, size } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::CellPattern {
                        pattern,
                        size: size.max(0.1),
                        feather: 0.0,
                        offset_x: 0.0,
                        offset_y: 0.0,
                        overflow: OverflowMode::Clip,
                    }
                );
                self.host.mark_dirty();
            }
            Action::AddCheckerboard { layer_id, size, color_a, color_b } => {
                let w = size.max(1.0);
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::Checkerboard {
                        anchor: (0.0, 0.0),
                        size: w,
                        feather: 0.0,
                        color_a,
                        color_b,
                        width: w,
                    }
                );
                self.host.mark_dirty();
            }
            Action::AddGradientEffect { layer_id, kind, start_color, end_color } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::Gradient {
                        start: (0.0, 0.0),
                        end: (1.0, 1.0),
                        kind,
                        start_color,
                        end_color,
                    }
                );
                self.host.mark_dirty();
            }
            Action::AddGridEffect { layer_id, size, border, color } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::Grid {
                        anchor: (0.0, 0.0),
                        size,
                        border,
                        feather: 0.0,
                        color,
                        invert: false,
                    }
                );
                self.host.mark_dirty();
            }
            Action::AddStrokeEffect { layer_id, color, brush_size } => {
                self.batch5_effects.entry(layer_id).or_default().push(
                    Batch5Effect::Stroke {
                        path: StrokePath::AllMaskPaths,
                        all_masks: true,
                        color,
                        brush_size,
                        softness: 0.0,
                        opacity: 1.0,
                        composite: StrokeComposite::Over,
                    }
                );
                self.host.mark_dirty();
            }
            Action::SetMotionBlur { layer_id, angle, distance } => {
                if let Some(stack) = self.batch5_effects.get_mut(&layer_id) {
                    for e in stack.iter_mut() {
                        if let Batch5Effect::MotionBlur { angle: a, distance: d } = e {
                            *a = angle;
                            *d = distance;
                        }
                    }
                }
                self.host.mark_dirty();
            }
            Action::RemoveBatch5Effect { layer_id, effect_idx } => {
                if let Some(stack) = self.batch5_effects.get_mut(&layer_id) {
                    if effect_idx < stack.len() {
                        stack.remove(effect_idx);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Motion Paths ---
            Action::CreateMotionPath { layer_id } => {
                let id = self.next_motion_path_id;
                self.next_motion_path_id += 1;
                self.motion_paths.push(MotionPath {
                    id,
                    layer_id,
                    points: Vec::new(),
                    closed: false,
                    auto_orient: false,
                    orient_smoothness: 0.5,
                });
            }
            Action::AddMotionPathPoint { path_id, time_s, x, y } => {
                if let Some(path) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    path.points.push(MotionPathPoint {
                        time_s,
                        x,
                        y,
                        in_handle: (0.0, 0.0),
                        out_handle: (0.0, 0.0),
                        easing: MotionEasing::Linear,
                    });
                    path.points.sort_by(|a, b| {
                        a.time_s.partial_cmp(&b.time_s).unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
            Action::RemoveMotionPathPoint { path_id, index } => {
                if let Some(path) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    if index < path.points.len() {
                        path.points.remove(index);
                    }
                }
            }
            Action::SetMotionPathPoint { path_id, index, x, y } => {
                if let Some(path) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    if let Some(pt) = path.points.get_mut(index) {
                        pt.x = x;
                        pt.y = y;
                    }
                }
            }
            Action::SetMotionPathEasing { path_id, index, easing } => {
                if let Some(path) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    if let Some(pt) = path.points.get_mut(index) {
                        pt.easing = easing;
                    }
                }
            }
            Action::SetAutoOrient { path_id, enabled } => {
                if let Some(path) = self.motion_paths.iter_mut().find(|p| p.id == path_id) {
                    path.auto_orient = enabled;
                }
            }
            Action::DeleteMotionPath { path_id } => {
                self.motion_paths.retain(|p| p.id != path_id);
            }

            // --- Shape Groups ---
            Action::AddShapeGroup { layer_id: _, name } => {
                let id = self.next_shape_group_id;
                self.next_shape_group_id += 1;
                self.shape_groups.push(ShapeLayerGroup {
                    id,
                    name,
                    transform: ShapeGroupTransform::default(),
                    items: Vec::new(),
                });
            }
            Action::AddShapeItemToGroup { group_id, kind } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    g.items.push(kind);
                }
            }
            Action::RemoveShapeItemFromGroup { group_id, item_index } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    if item_index < g.items.len() {
                        g.items.remove(item_index);
                    }
                }
            }
            Action::SetShapeGroupTransform { group_id, transform } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    g.transform = transform;
                }
            }
            Action::SetShapeStar { layer_id: _, group_id, points, inner_radius, outer_radius } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    g.items.push(ShapeItemKind::Star {
                        points,
                        inner_radius,
                        outer_radius,
                        inner_roundness: 0.0,
                        outer_roundness: 0.0,
                        rotation: 0.0,
                        position: (0.0, 0.0),
                    });
                }
            }
            Action::AddRepeaterToGroup { layer_id: _, group_id, copies } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    g.items.push(ShapeItemKind::Repeater {
                        copies,
                        offset: 0.0,
                        anchor: (0.0, 0.0),
                        position: (0.0, 0.0),
                        scale: (1.0, 1.0),
                        rotation: 0.0,
                        start_opacity: 1.0,
                        end_opacity: 1.0,
                    });
                }
            }
            Action::AddTrimPath { layer_id: _, group_id, start, end } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    g.items.push(ShapeItemKind::Trim {
                        start: start.clamp(0.0, 1.0),
                        end: end.clamp(0.0, 1.0),
                        offset: 0.0,
                        multiple: TrimMultiple::Simultaneously,
                    });
                }
            }
            Action::AddMergeShapes { layer_id: _, group_id, mode } => {
                if let Some(g) = self.shape_groups.iter_mut().find(|g| g.id == group_id) {
                    g.items.push(ShapeItemKind::Merge { mode });
                }
            }
            Action::DeleteShapeGroup { group_id } => {
                self.shape_groups.retain(|g| g.id != group_id);
            }

            // --- Audio Mixer ---
            Action::AddAudioBus { name } => {
                let id = self.next_bus_id;
                self.next_bus_id += 1;
                self.audio_buses.push(AudioBus::new(id, name));
            }
            Action::RemoveAudioBus { bus_id } => {
                self.audio_buses.retain(|b| b.id != bus_id);
            }
            Action::SetBusVolume { bus_id, volume } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == bus_id) {
                    b.volume = volume.clamp(0.0, 1.0);
                }
            }
            Action::SetBusPan { bus_id, pan } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == bus_id) {
                    b.pan = pan.clamp(-1.0, 1.0);
                }
            }
            Action::MuteBus { bus_id, muted } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == bus_id) {
                    b.muted = muted;
                }
            }
            Action::SoloBus { bus_id, solo } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == bus_id) {
                    b.solo = solo;
                }
            }
            Action::AddBusSend { from_id, to_id, level } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == from_id) {
                    b.sends.retain(|(t, _)| *t != to_id);
                    b.sends.push((to_id, level.clamp(0.0, 1.0)));
                }
            }
            Action::RemoveBusSend { from_id, to_id } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == from_id) {
                    b.sends.retain(|(t, _)| *t != to_id);
                }
            }
            Action::SetBusEq { bus_id, low, mid, high } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == bus_id) {
                    b.eq_low = low.clamp(-12.0, 12.0);
                    b.eq_mid = mid.clamp(-12.0, 12.0);
                    b.eq_high = high.clamp(-12.0, 12.0);
                }
            }
            Action::SetBusCompressor { bus_id, threshold, ratio } => {
                if let Some(b) = self.audio_buses.iter_mut().find(|b| b.id == bus_id) {
                    b.compressor_threshold = threshold;
                    b.compressor_ratio = ratio.max(1.0);
                }
            }
            Action::SetMasterVolume(v) => {
                self.master_volume = v.clamp(0.0, 2.0);
            }
            Action::SetMasterPan(p) => {
                self.master_pan = p.clamp(-1.0, 1.0);
            }
            _ => unreachable!("apply_batch5 called with wrong action"),
        }
    }

    /// Sample the motion path at `time_s`, interpolating between surrounding points.
    /// Returns `None` if no path with that id exists or the path has no points.
    pub fn sample_motion_path(&self, path_id: usize, time_s: f32) -> Option<(f32, f32)> {
        let path = self.motion_paths.iter().find(|p| p.id == path_id)?;
        if path.points.is_empty() {
            return None;
        }
        if path.points.len() == 1 {
            return Some((path.points[0].x, path.points[0].y));
        }
        let first = &path.points[0];
        let last = &path.points[path.points.len() - 1];
        if time_s <= first.time_s {
            return Some((first.x, first.y));
        }
        if time_s >= last.time_s {
            return Some((last.x, last.y));
        }
        for i in 0..path.points.len() - 1 {
            let a = &path.points[i];
            let b = &path.points[i + 1];
            if time_s >= a.time_s && time_s <= b.time_s {
                let span = (b.time_s - a.time_s).max(f32::EPSILON);
                let raw_t = (time_s - a.time_s) / span;
                let t = match &b.easing {
                    MotionEasing::Linear => raw_t,
                    MotionEasing::EaseIn => raw_t * raw_t,
                    MotionEasing::EaseOut => raw_t * (2.0 - raw_t),
                    MotionEasing::EaseInOut => {
                        if raw_t < 0.5 {
                            2.0 * raw_t * raw_t
                        } else {
                            -1.0 + (4.0 - 2.0 * raw_t) * raw_t
                        }
                    }
                    MotionEasing::Hold => 0.0,
                };
                return Some((a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
            }
        }
        None
    }
}
