use super::*;
use crate::comp::Prop;

/// Script language used by the expression engine.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExprLang {
    #[default]
    JavaScript,
    Python,
}

// ── Full After Effects Expression API ───────────────────────────────────────
//
// The keyframe-driven sampling/expression engine in `crate::comp::expr` already
// hosts `valueAtTime` / `loopOut` / `loopIn` / `wiggle` / `linear` / `ease*` /
// `clamp` / `random` over a rhai engine. This module adds the *rest* of the
// standard AE expression surface as pure, deterministic Rust functions that the
// app-state layer evaluates directly against a property's keyframes:
//
//   loopOut(mode)        — cycle / pingpong / offset / continue (all four modes)
//   loopIn(mode)         — same, before the first key
//   loopInOut(mode)      — loop on both sides
//   loopOutDuration(n)   — loop only the last `n` keyframes' worth
//   valueAtTime(t)       — sample the keyed curve at any time
//   velocityAtTime(t)    — finite-difference velocity (units/sec) at `t`
//   wiggle(freq, amp)    — DETERMINISTIC jitter seeded from (layer index, time)
//   linear / ease / easeIn / easeOut / clamp — remap & clamp helpers
//   posterizeTime(fps)   — quantize a time to a coarser frame rate
//   time / thisComp / thisLayer index helpers
//
// `wiggle` seeds from `(layer index, time)` with a SplitMix64 hash — never a
// global RNG — so every call is reproducible frame-to-frame and across runs.

/// How a `loop*` helper extends the animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    /// Repeat the segment over and over.
    Cycle,
    /// Repeat alternating forward / reversed.
    PingPong,
    /// Repeat but accumulate the end-to-end delta each cycle (a staircase).
    Offset,
    /// Hold the end velocity and extrapolate linearly past the last key.
    Continue,
}

impl LoopMode {
    /// Parse an AE-style mode string (`"cycle"`, `"pingpong"`, `"offset"`,
    /// `"continue"`), defaulting to `Cycle`.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "pingpong" => LoopMode::PingPong,
            "offset" => LoopMode::Offset,
            "continue" => LoopMode::Continue,
            _ => LoopMode::Cycle,
        }
    }
}

/// A read-only snapshot of a property's keyframes for the AE temporal helpers.
/// `(time, value)` pairs ascending by time, plus the held default for an empty
/// track. Lightweight + `Clone` so it can be primed per evaluation.
#[derive(Clone, Debug, Default)]
pub struct ExprTrack {
    pub keys: Vec<(f32, f32)>,
    pub default: f32,
}

impl ExprTrack {
    /// Build a snapshot from a comp layer's [`Prop`] track at app level.
    pub fn from_layer(layer: &crate::comp::PulseLayer, prop: Prop) -> Self {
        let track = layer.track(prop);
        ExprTrack {
            keys: track.keys.iter().map(|k| (k.t, k.value)).collect(),
            default: 0.0,
        }
    }

    /// First / last keyframe times, or `None` with fewer than two keys.
    fn span(&self) -> Option<(f32, f32)> {
        if self.keys.len() < 2 {
            return None;
        }
        Some((self.keys[0].0, self.keys[self.keys.len() - 1].0))
    }

    /// Linearly sample the curve at `t`, constant-held outside `[first, last]`.
    pub fn value_at_time(&self, t: f32) -> f32 {
        match self.keys.as_slice() {
            [] => self.default,
            [only] => only.1,
            keys => {
                let first = keys[0];
                let last = keys[keys.len() - 1];
                if t <= first.0 {
                    return first.1;
                }
                if t >= last.0 {
                    return last.1;
                }
                let i = keys.partition_point(|k| k.0 <= t);
                let a = keys[i - 1];
                let b = keys[i];
                let span = b.0 - a.0;
                if span <= f32::EPSILON {
                    return b.1;
                }
                a.1 + (b.1 - a.1) * ((t - a.0) / span)
            }
        }
    }

    /// Finite-difference **velocity** (units/sec) at `t`, AE's `velocityAtTime`.
    /// Uses a small symmetric step; flat outside the keyed range.
    pub fn velocity_at_time(&self, t: f32) -> f32 {
        if self.keys.len() < 2 {
            return 0.0;
        }
        let h = 1.0 / 240.0; // sub-frame step
        let v0 = self.value_at_time(t - h);
        let v1 = self.value_at_time(t + h);
        (v1 - v0) / (2.0 * h)
    }

    /// `loopOut(mode)`: extend the animation past the **last** keyframe.
    pub fn loop_out(&self, t: f32, mode: LoopMode) -> f32 {
        let Some((first, last)) = self.span() else {
            return self.value_at_time(t);
        };
        let period = last - first;
        if period <= f32::EPSILON || t <= last {
            return self.value_at_time(t);
        }
        let elapsed = t - last; // time past the end
        match mode {
            LoopMode::Cycle => {
                let folded = (t - first).rem_euclid(period) + first;
                self.value_at_time(folded)
            }
            LoopMode::PingPong => {
                let two = period * 2.0;
                let p = (t - first).rem_euclid(two);
                let mapped = if p <= period { p } else { two - p };
                self.value_at_time(mapped + first)
            }
            LoopMode::Offset => {
                let cycles = (elapsed / period).floor() + 1.0;
                let delta = self.value_at_time(last) - self.value_at_time(first);
                let folded = (t - first).rem_euclid(period) + first;
                self.value_at_time(folded) + delta * cycles
            }
            LoopMode::Continue => {
                let vel = self.velocity_at_time(last - 1e-3);
                self.value_at_time(last) + vel * elapsed
            }
        }
    }

    /// `loopIn(mode)`: extend the animation before the **first** keyframe.
    pub fn loop_in(&self, t: f32, mode: LoopMode) -> f32 {
        let Some((first, last)) = self.span() else {
            return self.value_at_time(t);
        };
        let period = last - first;
        if period <= f32::EPSILON || t >= first {
            return self.value_at_time(t);
        }
        let before = first - t; // time before the start
        match mode {
            LoopMode::Cycle => {
                let folded = last - before.rem_euclid(period);
                self.value_at_time(folded)
            }
            LoopMode::PingPong => {
                let two = period * 2.0;
                let p = before.rem_euclid(two);
                let mapped = if p <= period { p } else { two - p };
                self.value_at_time(first + mapped)
            }
            LoopMode::Offset => {
                let cycles = (before / period).floor() + 1.0;
                let delta = self.value_at_time(first) - self.value_at_time(last);
                let folded = last - before.rem_euclid(period);
                self.value_at_time(folded) + delta * cycles
            }
            LoopMode::Continue => {
                let vel = self.velocity_at_time(first + 1e-3);
                self.value_at_time(first) - vel * before
            }
        }
    }

    /// `loopInOut(mode)`: loop on whichever side of the keyed range `t` falls.
    pub fn loop_in_out(&self, t: f32, mode: LoopMode) -> f32 {
        let Some((first, last)) = self.span() else {
            return self.value_at_time(t);
        };
        if t < first {
            self.loop_in(t, mode)
        } else if t > last {
            self.loop_out(t, mode)
        } else {
            self.value_at_time(t)
        }
    }

    /// `loopOutDuration(mode, n)`: like `loopOut` but only the **last `n`
    /// keyframes** participate in the cycle (n ≥ 1). `n == 0` loops the whole
    /// range. Implemented by clipping the snapshot to the trailing window.
    pub fn loop_out_duration(&self, t: f32, mode: LoopMode, n: usize) -> f32 {
        if n == 0 || self.keys.len() <= n {
            return self.loop_out(t, mode);
        }
        let start = self.keys.len() - (n + 1);
        let sub = ExprTrack {
            keys: self.keys[start..].to_vec(),
            default: self.default,
        };
        sub.loop_out(t, mode)
    }
}

/// `posterizeTime(t, fps)`: quantize a time to the nearest lower frame boundary
/// of a coarser `fps` (AE's stop-motion / strobe helper). `fps <= 0` is a no-op.
pub fn posterize_time(t: f32, fps: f32) -> f32 {
    if fps <= 0.0 {
        return t;
    }
    (t * fps).floor() / fps
}

/// SplitMix64 — a fast, well-mixed integer hash used to seed `wiggle`.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic `wiggle(freq, amp)`: smooth pseudo-random jitter seeded from
/// `(layer_index, time)`. NEVER a global RNG — the same `(index, time)` always
/// yields the same offset, so renders are byte-reproducible across runs. Sums a
/// few seed-phased sines normalized to ~`±amp`; `freq` speeds up the jitter.
pub fn wiggle(layer_index: usize, time: f32, freq: f32, amp: f32) -> f32 {
    // Quantize time to ms so the seed is a function of the frame yet advances.
    let t_ms = (time as f64 * 1000.0).round() as i64 as u64;
    let seed = splitmix64((layer_index as u64).wrapping_mul(0x100_0000_01b3) ^ splitmix64(t_ms));
    let mut acc = 0.0_f64;
    let mut norm = 0.0_f64;
    for k in 0..3u64 {
        let h = splitmix64(seed.wrapping_add(k.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let phase = (h as f64 / u64::MAX as f64) * std::f64::consts::TAU;
        let weight = 1.0 / (1.0 + k as f64);
        acc += weight * (phase * (1.0 + freq as f64)).sin();
        norm += weight;
    }
    if norm == 0.0 {
        return 0.0;
    }
    ((acc / norm) * amp as f64) as f32
}

/// `linear(t, tmin, tmax, v1, v2)` — remap `t` from `[tmin,tmax]` to `[v1,v2]`,
/// clamped to the endpoints outside the range.
pub fn linear(t: f32, tmin: f32, tmax: f32, v1: f32, v2: f32) -> f32 {
    if (tmax - tmin).abs() < f32::EPSILON {
        return v1;
    }
    let f = ((t - tmin) / (tmax - tmin)).clamp(0.0, 1.0);
    v1 + (v2 - v1) * f
}

/// The Hermite ease shape used by [`ease`], [`ease_in`], [`ease_out`].
#[derive(Clone, Copy)]
enum EaseShape {
    In,
    Out,
    InOut,
}

fn ease_remap(shape: EaseShape, t: f32, t0: f32, t1: f32, v0: f32, v1: f32) -> f32 {
    if (t1 - t0).abs() < f32::EPSILON {
        return v0;
    }
    let x = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    let f = match shape {
        EaseShape::InOut => x * x * (3.0 - 2.0 * x),
        EaseShape::In => x * x,
        EaseShape::Out => 1.0 - (1.0 - x) * (1.0 - x),
    };
    v0 + (v1 - v0) * f
}

/// `ease(t, t0, t1, v0, v1)` — smooth (ease-in-out) remap.
pub fn ease(t: f32, t0: f32, t1: f32, v0: f32, v1: f32) -> f32 {
    ease_remap(EaseShape::InOut, t, t0, t1, v0, v1)
}
/// `easeIn(t, t0, t1, v0, v1)` — slow start.
pub fn ease_in(t: f32, t0: f32, t1: f32, v0: f32, v1: f32) -> f32 {
    ease_remap(EaseShape::In, t, t0, t1, v0, v1)
}
/// `easeOut(t, t0, t1, v0, v1)` — slow finish.
pub fn ease_out(t: f32, t0: f32, t1: f32, v0: f32, v1: f32) -> f32 {
    ease_remap(EaseShape::Out, t, t0, t1, v0, v1)
}

/// `clamp(v, lo, hi)` — clamp `v` into `[lo, hi]` (tolerating a swapped range).
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    v.clamp(lo, hi)
}


/// Expression control layer kind (Wave 14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExprControlKind {
    Slider,
    Angle,
    Checkbox,
    Color,
    Point,
}

impl ExprControlKind {
    /// Short badge label for the UI (used by the expr_controls panel).
    pub fn label(self) -> &'static str {
        match self {
            ExprControlKind::Slider => "SL",
            ExprControlKind::Angle => "∠",
            ExprControlKind::Checkbox => "☑",
            ExprControlKind::Color => "◉",
            ExprControlKind::Point => "↖",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ExprControlKind::Slider => "Slider Control",
            ExprControlKind::Angle => "Angle Control",
            ExprControlKind::Checkbox => "Checkbox Control",
            ExprControlKind::Color => "Color Control",
            ExprControlKind::Point => "Point Control",
        }
    }

    /// All control kinds, for Add-Control buttons.
    pub const ALL: [ExprControlKind; 5] = [
        ExprControlKind::Slider,
        ExprControlKind::Angle,
        ExprControlKind::Checkbox,
        ExprControlKind::Color,
        ExprControlKind::Point,
    ];
}

/// Runtime value for an expression control layer.
#[derive(Clone, Debug)]
pub enum ExprControlValue {
    Slider(f32),
    Angle(f32),
    Checkbox(bool),
    Color([f32; 4]),
    Point(f32, f32),
}

/// Expression control layer state (one per ExpressionControl layer).
#[derive(Clone, Debug)]
pub struct ExprControl {
    pub kind: ExprControlKind,
    pub value: ExprControlValue,
}

impl ExprControl {
    pub fn default_for(kind: ExprControlKind) -> Self {
        let value = match kind {
            ExprControlKind::Slider => ExprControlValue::Slider(50.0),
            ExprControlKind::Angle => ExprControlValue::Angle(0.0),
            ExprControlKind::Checkbox => ExprControlValue::Checkbox(false),
            ExprControlKind::Color => ExprControlValue::Color([1.0, 1.0, 1.0, 1.0]),
            ExprControlKind::Point => ExprControlValue::Point(0.0, 0.0),
        };
        ExprControl { kind, value }
    }
}

impl App {
    pub(super) fn apply_expressions(&mut self, action: Action) {
        match action {
            // --- Wave 14: Expression controls ---
            Action::AddExpressionControl(kind) => {
                let ci = self.active_comp_index();
                let name = kind.name().to_string();
                let mut layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::ExpressionControl,
                    name,
                    [0.3, 0.5, 0.9, 1.0],
                );
                layer.visible = false;
                let idx = self.project.comps[ci].layers.len();
                self.project.comps[ci].layers.push(layer);
                self.expr_controls.insert(idx, ExprControl::default_for(kind));
            }
            Action::SetExprControlValue { layer_idx, value } => {
                if let Some(ctrl) = self.expr_controls.get_mut(&layer_idx) {
                    ctrl.value = value;
                }
            }

            // --- Batch 4: Expression Engine depth ---
            Action::SetExpressionEnabled { layer_id, prop, enabled } => {
                self.expr_enabled.insert((layer_id, prop), enabled);
            }
            Action::AddExpressionError { layer_id, prop, error } => {
                self.expr_errors.insert((layer_id, prop), error);
            }
            Action::ClearExpressionErrors { layer_id } => {
                self.expr_errors.retain(|k, _| k.0 != layer_id);
            }
            Action::SetExpressionLanguage(l) => {
                self.expr_language = l;
            }
            Action::EvaluateExpression { layer_id, prop, at_time } => {
                // Evaluate the real expression string set on this (layer, prop)
                // through the rhai engine (with the property's keyframes primed so
                // temporal helpers resolve). Falls back to the keyframed sample,
                // then to `at_time * layer_id` only when no expression exists.
                let result = self.evaluate_expression(layer_id, &prop, at_time);
                self.last_expr_result = Some(result.unwrap_or(at_time * layer_id as f32));
            }

            _ => unreachable!("apply_expressions called with wrong action"),
        }
    }

    /// Evaluate the expression string stored for `(layer_id, prop_name)` at
    /// `at_time`, returning its scalar result. Uses the shared rhai engine (so the
    /// whole AE function library — `wiggle`, `loopOut`, `valueAtTime`, … — is
    /// available) primed with the property's keyframes. Returns `None` when there
    /// is no expression set for that key.
    pub fn evaluate_expression(&self, layer_id: usize, prop_name: &str, at_time: f32) -> Option<f32> {
        let expr = self
            .expressions
            .get(&(layer_id, prop_name.to_string()))?
            .clone();
        if expr.trim().is_empty() {
            return None;
        }
        let ci = self.active_comp_index();
        let comp = self.project.comps.get(ci)?;
        let layer = comp.layers.get(layer_id)?;
        // Resolve the prop name → keyframe track snapshot when it maps to a real
        // transform property, so `valueAtTime`/`loopOut` etc. read the curve.
        let track = prop_from_name(prop_name).map(|p| {
            let t = layer.track(p);
            crate::comp::expr::TrackView {
                keys: t.keys.iter().map(|k| (k.t, k.value)).collect(),
                default: 0.0,
            }
        });
        let keyed = track
            .as_ref()
            .map(|tv| tv.keys.iter().find(|(kt, _)| (*kt - at_time).abs() < 1e-4).map(|k| k.1)
                .unwrap_or_else(|| sample_keys(&tv.keys, at_time)))
            .unwrap_or(0.0);
        let ctx = crate::comp::ExprCtx {
            time: at_time,
            value: keyed,
            fps: comp.fps,
            duration: comp.duration,
            index: layer_id,
            width: comp.width as f32,
            height: comp.height as f32,
        };
        crate::comp::expr::eval_with_track(&expr, &ctx, track)
    }
}

/// Map a property name string to a [`Prop`], for expression evaluation.
fn prop_from_name(name: &str) -> Option<Prop> {
    match name.to_ascii_lowercase().as_str() {
        "x" | "positionx" | "position_x" => Some(Prop::X),
        "y" | "positiony" | "position_y" => Some(Prop::Y),
        "scale" => Some(Prop::Scale),
        "rotation" | "rotate" => Some(Prop::Rotation),
        "opacity" => Some(Prop::Opacity),
        "anchorx" | "anchor_x" => Some(Prop::AnchorX),
        "anchory" | "anchor_y" => Some(Prop::AnchorY),
        _ => None,
    }
}

/// Linearly sample `(time, value)` keyframe pairs at `t`, held outside the range.
fn sample_keys(keys: &[(f32, f32)], t: f32) -> f32 {
    match keys {
        [] => 0.0,
        [only] => only.1,
        keys => {
            let first = keys[0];
            let last = keys[keys.len() - 1];
            if t <= first.0 {
                return first.1;
            }
            if t >= last.0 {
                return last.1;
            }
            let i = keys.partition_point(|k| k.0 <= t);
            let a = keys[i - 1];
            let b = keys[i];
            let span = b.0 - a.0;
            if span <= f32::EPSILON {
                return b.1;
            }
            a.1 + (b.1 - a.1) * ((t - a.0) / span)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr_language_set() {
        let mut app = App::new();
        assert_eq!(app.expr_language, ExprLang::JavaScript);
        app.apply(Action::SetExpressionLanguage(ExprLang::Python));
        assert_eq!(app.expr_language, ExprLang::Python);
    }

    #[test]
    fn test_expr_enable_disable() {
        let mut app = App::new();
        app.apply(Action::SetExpressionEnabled { layer_id: 0, prop: "X".to_string(), enabled: true });
        assert_eq!(app.expr_enabled.get(&(0, "X".to_string())), Some(&true));
        app.apply(Action::SetExpressionEnabled { layer_id: 0, prop: "X".to_string(), enabled: false });
        assert_eq!(app.expr_enabled.get(&(0, "X".to_string())), Some(&false));
    }

    #[test]
    fn test_expr_clear_errors_for_layer() {
        let mut app = App::new();
        app.apply(Action::AddExpressionError { layer_id: 0, prop: "X".to_string(), error: "err1".to_string() });
        app.apply(Action::AddExpressionError { layer_id: 0, prop: "Y".to_string(), error: "err2".to_string() });
        app.apply(Action::AddExpressionError { layer_id: 1, prop: "X".to_string(), error: "err3".to_string() });
        assert_eq!(app.expr_errors.len(), 3);
        app.apply(Action::ClearExpressionErrors { layer_id: 0 });
        assert_eq!(app.expr_errors.len(), 1);
        assert!(app.expr_errors.contains_key(&(1, "X".to_string())));
    }

    #[test]
    fn test_expr_evaluate_result_fallback() {
        let mut app = App::new();
        // No expression set → falls back to at_time * layer_id.
        app.apply(Action::EvaluateExpression { layer_id: 2, prop: "Scale".to_string(), at_time: 3.0 });
        assert_eq!(app.last_expr_result, Some(6.0));
    }

    #[test]
    fn test_expr_evaluate_real_expression() {
        let mut app = App::new();
        // Set a real expression on layer 0's X and evaluate it through rhai.
        app.expressions.insert((0, "X".to_string()), "time * 100".to_string());
        app.apply(Action::EvaluateExpression { layer_id: 0, prop: "X".to_string(), at_time: 2.0 });
        assert_eq!(app.last_expr_result, Some(200.0));
    }

    #[test]
    fn test_expr_evaluate_wiggle_is_deterministic() {
        let mut app = App::new();
        app.expressions.insert((0, "Y".to_string()), "wiggle(2, 50)".to_string());
        app.apply(Action::EvaluateExpression { layer_id: 0, prop: "Y".to_string(), at_time: 1.0 });
        let a = app.last_expr_result.unwrap();
        app.apply(Action::EvaluateExpression { layer_id: 0, prop: "Y".to_string(), at_time: 1.0 });
        let b = app.last_expr_result.unwrap();
        assert_eq!(a, b, "wiggle through the engine must be reproducible");
        assert!(a.abs() <= 50.0 + 1e-3);
    }

    // --- Full AE Expression API (pure-fn) tests --------------------------------

    fn track(keys: &[(f32, f32)]) -> ExprTrack {
        ExprTrack { keys: keys.to_vec(), default: 0.0 }
    }

    #[test]
    fn test_loop_mode_parse() {
        assert_eq!(LoopMode::parse("cycle"), LoopMode::Cycle);
        assert_eq!(LoopMode::parse("PingPong"), LoopMode::PingPong);
        assert_eq!(LoopMode::parse("offset"), LoopMode::Offset);
        assert_eq!(LoopMode::parse("continue"), LoopMode::Continue);
        assert_eq!(LoopMode::parse("nonsense"), LoopMode::Cycle);
    }

    #[test]
    fn test_value_at_time_ramp() {
        let tr = track(&[(0.0, 0.0), (2.0, 100.0)]);
        assert!((tr.value_at_time(1.0) - 50.0).abs() < 1e-3);
        // Held outside the range.
        assert!((tr.value_at_time(-1.0) - 0.0).abs() < 1e-3);
        assert!((tr.value_at_time(5.0) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn test_velocity_at_time() {
        // 0→100 over [0,2] → velocity 50 units/sec.
        let tr = track(&[(0.0, 0.0), (2.0, 100.0)]);
        assert!((tr.velocity_at_time(1.0) - 50.0).abs() < 1e-1);
    }

    #[test]
    fn test_loop_out_cycle() {
        // 0→10 over [0,1]; at t=2.5 cycle folds to 0.5 → 5.
        let tr = track(&[(0.0, 0.0), (1.0, 10.0)]);
        assert!((tr.loop_out(2.5, LoopMode::Cycle) - 5.0).abs() < 1e-3);
        // Inside the range, identity.
        assert!((tr.loop_out(0.5, LoopMode::Cycle) - 5.0).abs() < 1e-3);
    }

    #[test]
    fn test_loop_out_pingpong() {
        let tr = track(&[(0.0, 0.0), (1.0, 10.0)]);
        // t=1.5 is 0.5 into the reverse leg → 5; t=2.0 back to start → 0.
        assert!((tr.loop_out(1.5, LoopMode::PingPong) - 5.0).abs() < 1e-3);
        assert!(tr.loop_out(2.0, LoopMode::PingPong).abs() < 1e-3);
    }

    #[test]
    fn test_loop_out_offset_staircase() {
        // 0→10 over [0,1]; offset adds the +10 delta each cycle.
        let tr = track(&[(0.0, 0.0), (1.0, 10.0)]);
        // t=1.5: one full cycle past → folded value 5 + delta(10)*1 = 15.
        assert!((tr.loop_out(1.5, LoopMode::Offset) - 15.0).abs() < 1e-3);
        // t=2.5: two cycles → 5 + 20 = 25.
        assert!((tr.loop_out(2.5, LoopMode::Offset) - 25.0).abs() < 1e-3);
    }

    #[test]
    fn test_loop_out_continue_extrapolates() {
        // Constant velocity 10/sec; continue extends linearly past the end.
        let tr = track(&[(0.0, 0.0), (1.0, 10.0)]);
        let v = tr.loop_out(2.0, LoopMode::Continue);
        assert!((v - 20.0).abs() < 1.0, "continue ~20 at t=2, got {v}");
    }

    #[test]
    fn test_loop_in_cycle() {
        let tr = track(&[(1.0, 0.0), (2.0, 10.0)]);
        // Before the start: t=0.5 cycles to value at 1.5 → 5.
        assert!((tr.loop_in(0.5, LoopMode::Cycle) - 5.0).abs() < 1e-3);
    }

    #[test]
    fn test_loop_in_out_picks_side() {
        let tr = track(&[(1.0, 0.0), (2.0, 10.0)]);
        assert!((tr.loop_in_out(0.5, LoopMode::Cycle) - 5.0).abs() < 1e-3); // before
        assert!((tr.loop_in_out(2.5, LoopMode::Cycle) - 5.0).abs() < 1e-3); // after
        assert!((tr.loop_in_out(1.5, LoopMode::Cycle) - 5.0).abs() < 1e-3); // inside
    }

    #[test]
    fn test_loop_out_duration_window() {
        // Three keys; loopOutDuration(1) loops only the last segment [1,2]:10→20.
        let tr = track(&[(0.0, 0.0), (1.0, 10.0), (2.0, 20.0)]);
        // t=2.5 with the last-1-keyframe window: cycle of [1,2] → folds to 1.5 → 15.
        let v = tr.loop_out_duration(2.5, LoopMode::Cycle, 1);
        assert!((v - 15.0).abs() < 1e-3, "loopOutDuration window, got {v}");
    }

    #[test]
    fn test_wiggle_deterministic_and_bounded() {
        let a = wiggle(0, 1.0, 2.0, 50.0);
        let b = wiggle(0, 1.0, 2.0, 50.0);
        assert_eq!(a, b, "wiggle reproducible for same (index, time)");
        // Different layer index diverges.
        let c = wiggle(5, 1.0, 2.0, 50.0);
        assert!(a != c, "different layer indices should differ");
        // Amplitude bounded.
        for &t in &[0.0_f32, 0.3, 0.7, 1.1, 2.4] {
            assert!(wiggle(0, t, 2.0, 50.0).abs() <= 50.0 + 1e-3);
        }
    }

    #[test]
    fn test_linear_ease_clamp_helpers() {
        assert!((linear(0.5, 0.0, 1.0, 0.0, 100.0) - 50.0).abs() < 1e-4);
        assert!((linear(-1.0, 0.0, 1.0, 10.0, 20.0) - 10.0).abs() < 1e-4);
        assert!((ease(0.5, 0.0, 1.0, 0.0, 100.0) - 50.0).abs() < 1e-3);
        assert!(ease_in(0.25, 0.0, 1.0, 0.0, 100.0) < 25.0);
        assert!(ease_out(0.25, 0.0, 1.0, 0.0, 100.0) > 25.0);
        assert!((clamp(5.0, 0.0, 1.0) - 1.0).abs() < 1e-4);
        assert!((clamp(-5.0, 0.0, 1.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_posterize_time() {
        // At 2 fps, t=0.7 → 0.5 (floor to the previous 0.5s boundary).
        assert!((posterize_time(0.7, 2.0) - 0.5).abs() < 1e-4);
        assert!((posterize_time(0.99, 2.0) - 0.5).abs() < 1e-4);
        assert!((posterize_time(1.0, 2.0) - 1.0).abs() < 1e-4);
        // fps<=0 is a no-op.
        assert!((posterize_time(0.7, 0.0) - 0.7).abs() < 1e-4);
    }

    #[test]
    fn test_expr_track_from_layer() {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.project.comps[ci].layers[0].track_mut(Prop::X).set_key(0.0, 0.0);
        app.project.comps[ci].layers[0].track_mut(Prop::X).set_key(1.0, 100.0);
        let tr = ExprTrack::from_layer(&app.project.comps[ci].layers[0], Prop::X);
        assert_eq!(tr.keys.len(), 2);
        assert!((tr.value_at_time(0.5) - 50.0).abs() < 1e-3);
    }
}
