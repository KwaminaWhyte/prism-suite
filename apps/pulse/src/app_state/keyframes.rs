use super::*;

pub type PreviewRect = Rc<Cell<Option<Bounds<Pixels>>>>;

/// Which graph element a drag is reshaping (see [`App::graph_grab`]). Mirrors the
/// egui graph editor's private `Grab`, but kept here so the GPUI panel can arm it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GraphGrab {
    /// A keyframe body: the drag retimes (x) and revalues (y) it.
    Key { prop: Prop, key_index: usize },
    /// A Bézier ease handle on the segment leaving key `key_index` of `prop`.
    Handle {
        prop: Prop,
        key_index: usize,
        which: GizmoHandle2,
    },
}

/// Which ease handle of a graph segment a drag targets. (A thin alias over the
/// engine's [`crate::comp::Handle`], re-named to avoid clashing with the
/// gizmo's `Handle`.)
pub use crate::comp::Handle as GizmoHandle2;

/// An in-progress preview transform-gizmo drag: the held handle, the layer +
/// grab-time transform/parent for the local-space delta math, and the pointer's
/// comp-space position at grab time. Recomputed each frame against the live
/// pointer (mirrors the egui app's `GizmoDrag`).
#[derive(Clone, Copy, Debug)]
pub struct GizmoDrag {
    pub layer: usize,
    pub handle: GizmoHandle,
    /// Playhead time when the grab started (where edits are keyed).
    pub time: f32,
    /// The layer's sampled transform at grab time.
    pub start_tf: Transform,
    /// The layer's parent matrix at grab time (parent-local conversion).
    pub parent: Affine2,
    /// Pointer position (comp space) when the grab started.
    pub start_comp: (f32, f32),
}

/// Which end of the work area a timeline drag is moving (see [`App::wa_drag`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkAreaHandle {
    /// The in-point (work-area start).
    In,
    /// The out-point (work-area end).
    Out,
}

/// Identifies the keyframe being dragged on a timeline lane (see [`App::kf_drag`]).
#[derive(Clone, Copy, Debug)]
pub struct KeyframeDrag {
    pub layer: usize,
    pub prop: Prop,
    pub key_index: usize,
}


impl App {
    pub(super) fn apply_keyframes(&mut self, action: Action) {
        match action {
            Action::MoveKeyframe { prop, key_index, time } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let dur = self.active_duration();
                let new_t = time.clamp(0.0, dur);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if let Some(value) = track.keys.get(key_index).map(|k| k.value) {
                        let landed = track.move_key(key_index, new_t, value);
                        self.host.mark_dirty();
                        if let Some(d) = self.kf_drag.as_mut() {
                            if d.layer == i && d.prop == prop && d.key_index == key_index {
                                d.key_index = landed;
                            }
                        }
                    }
                }
            }
            Action::ToggleGraph => {
                self.graph_open = !self.graph_open;
                self.graph_grab = None;
            }
            Action::ToggleGraphProp(prop) => {
                if let Some(i) = self.graph_shown.iter().position(|&p| p == prop) {
                    self.graph_shown.remove(i);
                } else {
                    self.graph_shown.push(prop);
                }
            }
            Action::ClearGraphProps => {
                self.graph_shown.clear();
            }
            Action::SetInterp { prop, key_index, interp } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if let Some(k) = track.key_mut(key_index) {
                        k.interp = interp;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::MoveKeyframeXY { prop, key_index, time, value } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let dur = self.active_duration();
                let new_t = time.clamp(0.0, dur);
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let track = layer.track_mut(prop);
                    if key_index < track.keys.len() {
                        let landed = track.move_key(key_index, new_t, value);
                        self.host.mark_dirty();
                        if let Some(GraphGrab::Key { prop: gp, key_index: gk }) = self.graph_grab.as_mut() {
                            if *gp == prop && *gk == key_index {
                                *gk = landed;
                            }
                        }
                    }
                }
            }
            Action::GizmoKeys { time, keys } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                let t = time;
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    for (prop, value) in keys {
                        layer.track_mut(prop).set_key(t, value);
                    }
                    self.host.mark_dirty();
                }
            }
            _ => unreachable!("apply_keyframes called with wrong action"),
        }
    }
}
