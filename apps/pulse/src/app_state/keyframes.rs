use super::*;

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
