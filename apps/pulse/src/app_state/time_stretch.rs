//! **Layer time-stretch + time-remap** — layer-level retiming: a scalar stretch
//! factor and a keyframable curve remapping in-comp time to source time.
//!
//! The engine carries the per-layer storage ([`PulseLayer::time_stretch`] and
//! [`crate::comp::TimeRemap`]); the basic enable / single-key actions live in
//! `composition_layers.rs`. This file adds the **richer retiming editing** —
//! reverse, freeze-frame, remove / clear remap keys, set stretch from a target
//! duration — plus the pure `comp_time → source_time` mapping that composes the
//! stretch and the remap curve, factored out so the retiming math is
//! unit-testable without an `App`.

use super::{App, Action};
use crate::comp::TimeRemap;

impl App {
    /// Apply a time-stretch / time-remap [`Action`]. Dispatched from
    /// [`App::apply`] for the retiming variants this file owns.
    pub(super) fn apply_time_stretch(&mut self, action: Action) {
        let ci = self.active_comp_index();
        match action {
            Action::SetLayerStretchFromDuration { layer_id, source_duration, target_duration } => {
                // Stretch factor = target / source (how much longer the layer
                // plays than its source). Guards against a zero source.
                let src = source_duration.max(1e-3);
                let factor = (target_duration / src).max(0.01);
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_stretch = factor;
                    self.host.mark_dirty();
                }
            }
            Action::ReverseLayerTime { layer_id } => {
                // A negative stretch isn't representable, so reverse is encoded as
                // a reversing time-remap curve over the layer's comp duration.
                let dur = self.project.comps[ci].duration;
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap.enabled = true;
                    l.time_remap.track.keys.clear();
                    // r(0) = dur, r(dur) = 0 ⇒ plays the source backwards.
                    l.time_remap.track.set_key(0.0, dur);
                    l.time_remap.track.set_key(dur, 0.0);
                    self.host.mark_dirty();
                }
            }
            Action::FreezeFrameAt { layer_id, comp_time, source_time } => {
                // A constant remap holds one source frame for the whole layer.
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap.enabled = true;
                    let _ = comp_time; // a freeze ignores comp_time (it's constant)
                    l.time_remap.track.keys.clear();
                    l.time_remap.track.set_key(0.0, source_time.max(0.0));
                    self.host.mark_dirty();
                }
            }
            Action::RemoveTimeRemapKey { layer_id, key_index } => {
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if key_index < l.time_remap.track.keys.len() {
                        l.time_remap.track.keys.remove(key_index);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ClearTimeRemap { layer_id } => {
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.time_remap = TimeRemap::default();
                    self.host.mark_dirty();
                }
            }
            _ => unreachable!("apply_time_stretch called with wrong action"),
        }
    }

    /// The **source time** a layer samples at, given a comp time `comp_time`:
    /// applies the layer's time-stretch and (when active) its time-remap curve.
    /// Identity (`comp_time`) for a 1× un-remapped layer. Returns `None` if the
    /// layer is gone.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn layer_source_time(&self, layer_id: usize, comp_time: f32) -> Option<f32> {
        let ci = self.active_comp_index();
        let layer = self.project.comps[ci].layers.get(layer_id)?;
        Some(comp_to_source_time(
            comp_time,
            layer.time_stretch,
            &layer.time_remap,
        ))
    }
}

/// Map an in-comp time `comp_time` to the **source time** the layer should be
/// sampled at, composing two retiming stages:
///
/// 1. **Time stretch** — a `stretch` factor of `2.0` makes the layer play half
///    speed, so a source frame at comp time `t` is `t / stretch`. (After
///    Effects' stretch percentage; `100% == 1.0`.)
/// 2. **Time remap** — when the remap is active, its keyframable curve *replaces*
///    the stretched time with an explicit `source_time` value (so a remap fully
///    overrides the stretch, matching AE where enabling remap drives source time
///    directly).
///
/// Pure and deterministic — the unit tests pin the stretch math and the
/// remap-override behaviour.
pub fn comp_to_source_time(comp_time: f32, stretch: f32, remap: &TimeRemap) -> f32 {
    if remap.is_active() {
        // Remap drives source time directly (overrides the stretch).
        remap.source_time(comp_time)
    } else {
        let s = stretch.max(0.01);
        comp_time / s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_stretch_no_remap() {
        let remap = TimeRemap::default();
        for &t in &[0.0, 1.0, 2.5, 5.0] {
            assert!((comp_to_source_time(t, 1.0, &remap) - t).abs() < 1e-4);
        }
    }

    #[test]
    fn double_stretch_halves_source_time() {
        // 2× stretch (half speed): source advances at half the comp rate.
        let remap = TimeRemap::default();
        assert!((comp_to_source_time(4.0, 2.0, &remap) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn half_stretch_doubles_source_time() {
        // 0.5× stretch (double speed): source advances twice as fast.
        let remap = TimeRemap::default();
        assert!((comp_to_source_time(2.0, 0.5, &remap) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn stretch_clamps_to_min() {
        // A zero/negative stretch is clamped so the mapping never divides by ~0.
        let remap = TimeRemap::default();
        let r = comp_to_source_time(1.0, 0.0, &remap);
        assert!(r.is_finite() && r > 0.0);
    }

    #[test]
    fn active_remap_overrides_stretch() {
        // With an active reversing remap (r(t) = dur - t) the stretch is ignored.
        let dur = 4.0;
        let mut remap = TimeRemap::default();
        remap.enabled = true;
        remap.track.set_key(0.0, dur);
        remap.track.set_key(dur, 0.0);
        // Even with a 2× stretch, the remap drives source time.
        for &t in &[0.0, 1.0, 2.0, 4.0] {
            assert!((comp_to_source_time(t, 2.0, &remap) - (dur - t)).abs() < 1e-3, "t={t}");
        }
    }

    #[test]
    fn set_stretch_from_duration() {
        let mut app = App::new();
        // Stretch a 2 s source to play over 6 s ⇒ factor 3.0.
        app.apply(Action::SetLayerStretchFromDuration {
            layer_id: 0,
            source_duration: 2.0,
            target_duration: 6.0,
        });
        let ci = app.active_comp_index();
        assert!((app.project.comps[ci].layers[0].time_stretch - 3.0).abs() < 1e-4);
    }

    #[test]
    fn set_stretch_from_duration_guards_zero_source() {
        let mut app = App::new();
        app.apply(Action::SetLayerStretchFromDuration {
            layer_id: 0,
            source_duration: 0.0,
            target_duration: 5.0,
        });
        let ci = app.active_comp_index();
        // No NaN/inf; the clamp keeps it finite and >= 0.01.
        let f = app.project.comps[ci].layers[0].time_stretch;
        assert!(f.is_finite() && f >= 0.01);
    }

    #[test]
    fn reverse_layer_lays_reversing_remap() {
        let mut app = App::new();
        app.apply(Action::ReverseLayerTime { layer_id: 0 });
        let ci = app.active_comp_index();
        let l = &app.project.comps[ci].layers[0];
        assert!(l.time_remap.enabled);
        assert!(l.time_remap.is_active());
        let dur = app.project.comps[ci].duration;
        // r(0) = dur and r(dur) = 0 (plays backwards).
        assert!((l.time_remap.source_time(0.0) - dur).abs() < 1e-3);
        assert!((l.time_remap.source_time(dur)).abs() < 1e-3);
    }

    #[test]
    fn freeze_frame_holds_one_source_time() {
        let mut app = App::new();
        app.apply(Action::FreezeFrameAt { layer_id: 0, comp_time: 2.0, source_time: 1.5 });
        let ci = app.active_comp_index();
        let l = &app.project.comps[ci].layers[0];
        assert!(l.time_remap.is_active());
        for &t in &[0.0, 1.0, 3.0, 9.0] {
            assert!((l.time_remap.source_time(t) - 1.5).abs() < 1e-3, "frozen at 1.5, t={t}");
        }
    }

    #[test]
    fn remove_remap_key_and_clear() {
        let mut app = App::new();
        app.apply(Action::ReverseLayerTime { layer_id: 0 }); // lays 2 keys
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].time_remap.track.keys.len(), 2);
        app.apply(Action::RemoveTimeRemapKey { layer_id: 0, key_index: 0 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].time_remap.track.keys.len(), 1);
        app.apply(Action::ClearTimeRemap { layer_id: 0 });
        let ci = app.active_comp_index();
        let l = &app.project.comps[ci].layers[0];
        assert!(!l.time_remap.enabled);
        assert!(l.time_remap.track.keys.is_empty());
    }

    #[test]
    fn layer_source_time_through_app_uses_stretch() {
        let mut app = App::new();
        app.apply(Action::SetLayerTimeStretch { layer_id: 0, factor: 2.0 });
        // At comp time 4, a 2× stretched layer samples source time 2.
        let st = app.layer_source_time(0, 4.0).expect("source time");
        assert!((st - 2.0).abs() < 1e-4);
    }

    #[test]
    fn layer_source_time_through_app_uses_remap() {
        let mut app = App::new();
        app.apply(Action::FreezeFrameAt { layer_id: 0, comp_time: 0.0, source_time: 0.8 });
        // A frozen layer reports the held source time regardless of comp time.
        assert!((app.layer_source_time(0, 3.0).unwrap() - 0.8).abs() < 1e-3);
    }
}
