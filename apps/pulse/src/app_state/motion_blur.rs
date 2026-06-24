//! **Per-layer motion blur** — shutter-angle + sample-count config and the
//! transform-averaging math that integrates a layer's motion across the shutter.
//!
//! After Effects motion-blurs a layer by opening a virtual shutter for a
//! fraction of the frame (set by *shutter angle*) and integrating `samples`
//! sub-frame snapshots of the layer's transform across that window. The engine
//! already models the comp-level shutter ([`crate::comp::MotionBlur`]) and
//! per-layer enable flag; this file adds an **app-side per-layer override**
//! (its own angle / phase / samples) and the deterministic averaging used to
//! preview the blurred position. The averaging is a free function so the
//! "N samples across the shutter → expected averaged position" math is
//! unit-testable without an `App`.

use std::collections::HashMap;

use super::{App, Action};
use crate::comp::{MotionBlur, Prop, PulseLayer};

/// Per-layer motion-blur settings (an override of the comp's shutter for one
/// layer). A layer with no entry inherits the comp shutter; an entry lets a user
/// give one layer a wider/narrower shutter or more samples than the comp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerMotionBlur {
    /// Whether this per-layer override is active (vs. inheriting the comp).
    pub enabled: bool,
    /// Shutter angle in degrees (`angle/360` of a frame open). 180° is the
    /// cinematic default; clamped to `(0, 720]`.
    pub shutter_angle: f32,
    /// Shutter phase in degrees, positioning the open window relative to the
    /// frame (`0` opens at the frame, `-angle/2` centers it).
    pub shutter_phase: f32,
    /// Sub-frame samples integrated across the shutter (clamped `[1, 64]`).
    pub samples: u32,
}

impl Default for LayerMotionBlur {
    fn default() -> Self {
        Self {
            enabled: true,
            shutter_angle: 180.0,
            shutter_phase: 0.0,
            samples: 16,
        }
    }
}

impl LayerMotionBlur {
    /// The engine [`MotionBlur`] this override represents (so the same
    /// `sample_times` shutter math applies). `enabled` here means "override is
    /// on"; the returned `MotionBlur.enabled` is forced `true` because the
    /// caller only builds this when blurring the layer.
    pub fn as_shutter(self) -> MotionBlur {
        MotionBlur {
            enabled: true,
            angle: self.shutter_angle.clamp(1e-3, 720.0),
            phase: self.shutter_phase.clamp(-360.0, 360.0),
            samples: self.samples.clamp(1, 64),
        }
    }
}

impl App {
    /// Apply a per-layer motion-blur [`Action`]. Dispatched from [`App::apply`].
    pub(super) fn apply_motion_blur(&mut self, action: Action) {
        match action {
            Action::SetLayerShutterAngle { layer_id, angle } => {
                let e = self.layer_motion_blur.entry(layer_id).or_default();
                e.shutter_angle = angle.clamp(1e-3, 720.0);
                self.host.mark_dirty();
            }
            Action::SetLayerShutterPhase { layer_id, phase } => {
                let e = self.layer_motion_blur.entry(layer_id).or_default();
                e.shutter_phase = phase.clamp(-360.0, 360.0);
                self.host.mark_dirty();
            }
            Action::SetLayerMotionBlurSamples { layer_id, samples } => {
                let e = self.layer_motion_blur.entry(layer_id).or_default();
                e.samples = samples.clamp(1, 64);
                self.host.mark_dirty();
            }
            Action::SetLayerMotionBlurOverride { layer_id, enabled } => {
                self.layer_motion_blur.entry(layer_id).or_default().enabled = enabled;
                self.host.mark_dirty();
            }
            Action::ClearLayerMotionBlurOverride { layer_id } => {
                self.layer_motion_blur.remove(&layer_id);
                self.host.mark_dirty();
            }
            _ => unreachable!("apply_motion_blur called with wrong action"),
        }
    }

    /// The effective shutter for `layer_id` at the playhead: the per-layer
    /// override when present + enabled, else the comp's shutter. `None` when
    /// neither the comp nor the layer enables motion blur (so the caller renders
    /// crisp).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn effective_layer_shutter(&self, layer_id: usize) -> Option<MotionBlur> {
        let ci = self.active_comp_index();
        let comp = &self.project.comps[ci];
        let layer = comp.layers.get(layer_id)?;
        // Per-layer override wins when present + enabled.
        if let Some(over) = self.layer_motion_blur.get(&layer_id) {
            if over.enabled {
                return Some(over.as_shutter());
            }
        }
        // Otherwise the comp shutter, but only if both master + layer are on.
        if comp.motion_blur.enabled && layer.motion_blur {
            Some(comp.motion_blur)
        } else {
            None
        }
    }

    /// The motion-blur-averaged **position** `(x, y)` (comp px) of `layer_id` at
    /// the playhead, integrating its X/Y tracks across the effective shutter.
    /// Falls back to the un-blurred sampled position when motion blur is off for
    /// the layer.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn motion_blurred_position(&self, layer_id: usize) -> Option<(f32, f32)> {
        let ci = self.active_comp_index();
        let comp = &self.project.comps[ci];
        let layer = comp.layers.get(layer_id)?;
        let fps = comp.fps.max(1.0);
        let t = self.time;
        match self.effective_layer_shutter(layer_id) {
            Some(shutter) => Some(averaged_position(layer, shutter, t, fps)),
            None => Some((layer.value(Prop::X, t), layer.value(Prop::Y, t))),
        }
    }
}

/// The mean **position** `(x, y)` of a layer over the shutter window: sample the
/// X/Y tracks at each of the shutter's [`MotionBlur::sample_times`] and average.
///
/// This is the core motion-blur integration reduced to its testable kernel:
/// `N` evenly-spaced midpoint samples across `[open, close]` give the smeared
/// centroid the blur is centered on. With a still layer it returns the static
/// position; with linear motion it returns the window's midpoint position
/// (the mean of a linear ramp), which is exactly what an N-sample average
/// converges to.
pub fn averaged_position(layer: &PulseLayer, shutter: MotionBlur, t: f32, fps: f32) -> (f32, f32) {
    let times = shutter.sample_times(t, fps);
    if times.is_empty() {
        return (layer.value(Prop::X, t), layer.value(Prop::Y, t));
    }
    let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
    for &st in &times {
        sx += layer.value(Prop::X, st);
        sy += layer.value(Prop::Y, st);
    }
    let n = times.len() as f32;
    (sx / n, sy / n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layer whose X track ramps linearly from `x0` at t=0 to `x1` at t=`dur`.
    fn ramping_layer(x0: f32, x1: f32, dur: f32) -> PulseLayer {
        let mut l = PulseLayer::of_kind(crate::comp::LayerKind::Solid, "ramp", [1.0, 1.0, 1.0, 1.0]);
        l.x.set_key(0.0, x0);
        l.x.set_key(dur, x1);
        l
    }

    #[test]
    fn still_layer_averages_to_static_position() {
        // A layer with a single X key holds that value; the average over any
        // shutter is that same value.
        let mut l = PulseLayer::of_kind(crate::comp::LayerKind::Solid, "still", [1.0; 4]);
        l.x.set_key(0.0, 120.0);
        let shutter = MotionBlur { enabled: true, angle: 180.0, phase: 0.0, samples: 8 };
        let (x, _) = averaged_position(&l, shutter, 1.0, 30.0);
        assert!((x - 120.0).abs() < 1e-3, "still layer averages to its position, got {x}");
    }

    #[test]
    fn linear_motion_averages_to_window_midpoint() {
        // Over a linear ramp the N-sample midpoint average equals the position
        // at the shutter-window center. Shutter centered on t via phase = -angle/2.
        let dur = 4.0;
        let l = ramping_layer(0.0, 400.0, dur); // slope 100 px/s
        let fps = 25.0; // frame = 0.04 s
        let t = 1.0;
        let angle = 180.0;
        let shutter = MotionBlur { enabled: true, angle, phase: -angle / 2.0, samples: 16 };
        let (x, _) = averaged_position(&l, shutter, t, fps);
        // Window centered on t=1.0 ⇒ centroid position == value at t = 100 px.
        let expected = 100.0; // 100 px/s * 1.0 s
        assert!((x - expected).abs() < 1.0, "centered linear avg ≈ midpoint, got {x}");
    }

    #[test]
    fn more_samples_converge_to_analytic_mean() {
        // The analytic mean of a linear ramp over [open, close] is the value at
        // the window midpoint. A large sample count should land very close.
        let l = ramping_layer(0.0, 1000.0, 10.0); // slope 100 px/s
        let fps = 30.0;
        let t = 2.0;
        let angle = 360.0;
        let shutter = MotionBlur { enabled: true, angle, phase: -angle / 2.0, samples: 64 };
        let (open, close) = shutter.shutter_window(t, fps);
        let mid = (open + close) * 0.5;
        let analytic = 100.0 * mid;
        let (x, _) = averaged_position(&l, shutter, t, fps);
        assert!((x - analytic).abs() < 0.5, "64-sample avg ≈ analytic mean ({analytic}), got {x}");
    }

    #[test]
    fn single_sample_is_window_center() {
        // A 1-sample shutter lands at the window center (a crisp snapshot there).
        let l = ramping_layer(0.0, 600.0, 6.0); // slope 100 px/s
        let fps = 30.0;
        let t = 1.0;
        let shutter = MotionBlur { enabled: true, angle: 180.0, phase: 0.0, samples: 1 };
        let (open, close) = shutter.shutter_window(t, fps);
        let center = (open + close) * 0.5;
        let (x, _) = averaged_position(&l, shutter, t, fps);
        assert!((x - 100.0 * center).abs() < 1e-3);
    }

    #[test]
    fn set_layer_shutter_actions_clamp() {
        let mut app = App::new();
        app.apply(Action::SetLayerShutterAngle { layer_id: 0, angle: 9999.0 });
        app.apply(Action::SetLayerMotionBlurSamples { layer_id: 0, samples: 200 });
        app.apply(Action::SetLayerShutterPhase { layer_id: 0, phase: -999.0 });
        let mb = app.layer_motion_blur.get(&0).copied().unwrap();
        assert!((mb.shutter_angle - 720.0).abs() < 1e-3, "angle clamps to 720");
        assert_eq!(mb.samples, 64, "samples clamp to 64");
        assert!((mb.shutter_phase + 360.0).abs() < 1e-3, "phase clamps to -360");
    }

    #[test]
    fn override_enable_and_clear() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        // Force a clean baseline: no comp/layer motion blur, so we can isolate
        // the per-layer override's effect (the demo project enables both).
        app.project.comps[ci].motion_blur.enabled = false;
        app.project.comps[ci].layers[0].motion_blur = false;

        app.apply(Action::SetLayerShutterAngle { layer_id: 0, angle: 90.0 });
        assert!(app.layer_motion_blur.contains_key(&0));
        // The override is on by default; its shutter wins regardless of comp MB.
        let sh = app.effective_layer_shutter(0).expect("override shutter");
        assert!((sh.angle - 90.0).abs() < 1e-3);
        // Clearing removes the per-layer entry; with comp MB off the layer is crisp.
        app.apply(Action::ClearLayerMotionBlurOverride { layer_id: 0 });
        assert!(!app.layer_motion_blur.contains_key(&0));
        assert!(app.effective_layer_shutter(0).is_none());
    }

    #[test]
    fn cleared_override_falls_back_to_comp_shutter() {
        // With the demo project's comp+layer motion blur on, clearing a per-layer
        // override falls back to the comp's shutter (not crisp).
        let mut app = App::new();
        app.apply(Action::SetLayerShutterAngle { layer_id: 0, angle: 45.0 });
        app.apply(Action::ClearLayerMotionBlurOverride { layer_id: 0 });
        let ci = app.active_comp_index();
        let sh = app.effective_layer_shutter(0).expect("comp shutter fallback");
        assert!((sh.angle - app.project.comps[ci].motion_blur.angle).abs() < 1e-3);
    }

    #[test]
    fn comp_shutter_used_when_no_override() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        // Enable comp master MB + the layer flag; no per-layer override.
        app.project.comps[ci].motion_blur.enabled = true;
        app.project.comps[ci].layers[0].motion_blur = true;
        let sh = app.effective_layer_shutter(0).expect("comp shutter");
        assert!((sh.angle - app.project.comps[ci].motion_blur.angle).abs() < 1e-3);
    }

    #[test]
    fn motion_blurred_position_through_app() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        // Replace layer 0's (eased, demo-seeded) X track with a clean *linear*
        // ramp 0→400 over 0→4 s, so the averaged position has a known value.
        app.project.comps[ci].layers[0].x.keys.clear();
        app.project.comps[ci].layers[0].x.set_key(0.0, 0.0);
        app.project.comps[ci].layers[0].x.set_key(4.0, 400.0);
        app.apply(Action::SetLayerShutterAngle { layer_id: 0, angle: 180.0 });
        app.apply(Action::SetLayerShutterPhase { layer_id: 0, phase: -90.0 });
        app.time = 1.0;
        let (x, _) = app.motion_blurred_position(0).expect("position");
        // Centered shutter ⇒ ≈ position at t=1 = 100 px.
        assert!((x - 100.0).abs() < 1.5, "blurred position ≈ 100, got {x}");
    }
}
