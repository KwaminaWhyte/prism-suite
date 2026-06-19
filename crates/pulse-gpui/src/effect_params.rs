//! Per-effect **scalar parameter** accessor for the effects panel.
//!
//! The GPUI host has no native slider, so the effects panel edits each effect's
//! parameters as **stepper rows** (`−` / value / `+`), exactly like the
//! properties panel edits transform tracks. To do that uniformly across the six
//! effect stacks, this module exposes every effect variant's editable **scalar**
//! parameters as an ordered list of [`ScalarParam`] descriptors (label + current
//! value + sensible `[lo, hi]` range + nudge `step`), plus a [`set`] that writes
//! one back in place by its index.
//!
//! The ranges and labels MIRROR the egui app's per-effect slider rows
//! (`pulse_app::app::properties::{effect_params, spatial_effect_params,
//! distort_effect_params, stylize_effect_params, key_effect_params}`) so the two
//! hosts edit the same parameters with the same bounds. This is a pure *reader/
//! writer* over the engine's public effect enums (`pulse_app::comp::*`) — it does
//! NOT fork the comp model; it only surfaces the existing fields for editing.
//!
//! Non-scalar params (colours `[f32;3]`, bool toggles, mode enums, and the
//! `[f32;2]` point pairs) are intentionally out of scope for this stepper-row
//! cut — they need swatch / checkbox / combo widgets the host doesn't have yet.
//! Vector / integer params that read naturally as scalars (a `[f32;2]` centre,
//! a `u32` count) ARE exposed as scalar rows so the bulk of each effect is
//! editable. Editing is unit-tested in `tests` below against the engine enums.

use pulse_app::comp::{
    DistortEffect, Effect, GenerateEffect, KeyEffect, SpatialEffect, StylizeEffect,
};

/// One editable scalar parameter of an effect: a display `label`, the current
/// `value`, the inclusive `[lo, hi]` range it's clamped to, and the `step` a
/// `+` / `−` tap nudges it by. Mirrors one egui slider row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalarParam {
    pub label: &'static str,
    pub value: f32,
    pub lo: f32,
    pub hi: f32,
    pub step: f32,
}

impl ScalarParam {
    fn new(label: &'static str, value: f32, lo: f32, hi: f32, step: f32) -> Self {
        Self {
            label,
            value,
            lo,
            hi,
            step,
        }
    }
}

/// Which per-layer effect stack an effect lives on. Used by the panel + the
/// undo-aware actions to address an effect as `(stack, effect_index)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectStack {
    Color,
    Spatial,
    Distort,
    Stylize,
    Keying,
    /// The single `generate` slot (`Option`); `effect_index` is ignored.
    Generate,
}

impl EffectStack {
    /// A short stack tag for the panel (matches the egui browser's tags).
    pub fn tag(self) -> &'static str {
        match self {
            EffectStack::Color => "color",
            EffectStack::Spatial => "buffer",
            EffectStack::Distort => "distort",
            EffectStack::Stylize => "stylize",
            EffectStack::Keying => "keying",
            EffectStack::Generate => "generate",
        }
    }
}

// ---------------------------------------------------------------------------
// Color (per-pixel) effects
// ---------------------------------------------------------------------------

/// The editable scalar params of a colour [`Effect`], in panel order. Colours
/// and the `monochrome` toggle are omitted (no swatch/checkbox widget yet).
pub fn color_params(e: &Effect) -> Vec<ScalarParam> {
    let p = ScalarParam::new;
    match *e {
        Effect::Tint { amount, .. } => vec![p("Amount", amount, 0.0, 1.0, 0.05)],
        Effect::BrightnessContrast {
            brightness,
            contrast,
        } => vec![
            p("Brightness", brightness, -1.0, 1.0, 0.05),
            p("Contrast", contrast, 0.0, 3.0, 0.05),
        ],
        Effect::Exposure {
            stops,
            offset,
            gamma,
        } => vec![
            p("Stops", stops, -5.0, 5.0, 0.1),
            p("Offset", offset, -0.5, 0.5, 0.02),
            p("Gamma", gamma, 0.1, 3.0, 0.05),
        ],
        Effect::Levels {
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => vec![
            p("In black", in_black, 0.0, 1.0, 0.02),
            p("In white", in_white, 0.0, 1.0, 0.02),
            p("Gamma", gamma, 0.1, 3.0, 0.05),
            p("Out black", out_black, 0.0, 1.0, 0.02),
            p("Out white", out_white, 0.0, 1.0, 0.02),
        ],
        Effect::HueSaturation {
            hue,
            saturation,
            lightness,
        } => vec![
            p("Hue", hue, -180.0, 180.0, 1.0),
            p("Saturation", saturation, -1.0, 1.0, 0.05),
            p("Lightness", lightness, -1.0, 1.0, 0.05),
        ],
        Effect::Curves { points } => points
            .iter()
            .zip(["0.00", "0.25", "0.50", "0.75", "1.00"])
            .map(|(&v, label)| p(label, v, 0.0, 1.0, 0.02))
            .collect(),
        // ColorBalance / ChannelMixer / GradientMap / Tritone are colour-triple /
        // matrix authored — no scalar rows in this cut (their `amount`s below).
        Effect::GradientMap { amount, .. } | Effect::Tritone { amount, .. } => {
            vec![p("Amount", amount, 0.0, 1.0, 0.05)]
        }
        Effect::ColorBalance { .. } | Effect::ChannelMixer { .. } => Vec::new(),
        Effect::MotionBlur { samples, shutter_angle } => vec![
            p("Samples", samples as f32, 1.0, 64.0, 1.0),
            p("Shutter Angle", shutter_angle, 0.0, 360.0, 5.0),
        ],
    }
}

/// Write `value` (clamped) into the colour [`Effect`]'s scalar param at `i` (the
/// index into [`color_params`]). No-op if `i` is out of range.
pub fn set_color(e: &mut Effect, i: usize, value: f32) {
    // Re-derive the param list to clamp to the right range, then assign.
    let Some(sp) = color_params(e).get(i).copied() else {
        return;
    };
    let v = value.clamp(sp.lo, sp.hi);
    match e {
        Effect::Tint { amount, .. } => set_one(&mut [amount], i, v),
        Effect::BrightnessContrast {
            brightness,
            contrast,
        } => set_one(&mut [brightness, contrast], i, v),
        Effect::Exposure {
            stops,
            offset,
            gamma,
        } => set_one(&mut [stops, offset, gamma], i, v),
        Effect::Levels {
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => set_one(&mut [in_black, in_white, gamma, out_black, out_white], i, v),
        Effect::HueSaturation {
            hue,
            saturation,
            lightness,
        } => set_one(&mut [hue, saturation, lightness], i, v),
        Effect::Curves { points } => {
            if let Some(slot) = points.get_mut(i) {
                *slot = v;
            }
        }
        Effect::GradientMap { amount, .. } | Effect::Tritone { amount, .. } => {
            set_one(&mut [amount], i, v)
        }
        Effect::ColorBalance { .. } | Effect::ChannelMixer { .. } => {}
        Effect::MotionBlur { samples, shutter_angle } => match i {
            0 => *samples = (v.round().max(1.0) as u32).min(64),
            1 => *shutter_angle = v,
            _ => {}
        },
    }
}

// ---------------------------------------------------------------------------
// Spatial (whole-buffer) effects
// ---------------------------------------------------------------------------

/// The editable scalar params of a [`SpatialEffect`], in panel order.
pub fn spatial_params(e: &SpatialEffect) -> Vec<ScalarParam> {
    let p = ScalarParam::new;
    match *e {
        SpatialEffect::GaussianBlur {
            sigma_x, sigma_y, ..
        } => vec![
            p("Blur X", sigma_x, 0.0, 100.0, 1.0),
            p("Blur Y", sigma_y, 0.0, 100.0, 1.0),
        ],
        SpatialEffect::BoxBlur {
            radius, iterations, ..
        } => vec![
            p("Radius", radius, 0.0, 100.0, 1.0),
            p("Iterations", iterations as f32, 1.0, 8.0, 1.0),
        ],
        SpatialEffect::DirectionalBlur { angle, length } => vec![
            p("Direction", angle, -180.0, 180.0, 1.0),
            p("Length", length, 0.0, 200.0, 1.0),
        ],
        SpatialEffect::RadialBlur {
            center, amount, ..
        } => vec![
            p("Center X", center[0], -1.0, 2.0, 0.01),
            p("Center Y", center[1], -1.0, 2.0, 0.01),
            p("Amount", amount, 0.0, 90.0, 0.5),
        ],
        SpatialEffect::DropShadow {
            opacity,
            angle,
            distance,
            softness,
            ..
        } => vec![
            p("Opacity", opacity, 0.0, 1.0, 0.05),
            p("Direction", angle, -180.0, 180.0, 1.0),
            p("Distance", distance, 0.0, 200.0, 1.0),
            p("Softness", softness, 0.0, 100.0, 1.0),
        ],
        SpatialEffect::Glow {
            threshold,
            radius,
            intensity,
        } => vec![
            p("Threshold", threshold, 0.0, 1.0, 0.05),
            p("Radius", radius, 0.0, 100.0, 1.0),
            p("Intensity", intensity, 0.0, 4.0, 0.1),
        ],
    }
}

/// Write `value` (clamped) into the [`SpatialEffect`]'s scalar param at `i`.
pub fn set_spatial(e: &mut SpatialEffect, i: usize, value: f32) {
    let Some(sp) = spatial_params(e).get(i).copied() else {
        return;
    };
    let v = value.clamp(sp.lo, sp.hi);
    match e {
        SpatialEffect::GaussianBlur {
            sigma_x, sigma_y, ..
        } => set_one(&mut [sigma_x, sigma_y], i, v),
        SpatialEffect::BoxBlur {
            radius, iterations, ..
        } => match i {
            0 => *radius = v,
            1 => *iterations = v.round().max(1.0) as u32,
            _ => {}
        },
        SpatialEffect::DirectionalBlur { angle, length } => set_one(&mut [angle, length], i, v),
        SpatialEffect::RadialBlur {
            center, amount, ..
        } => match i {
            0 => center[0] = v,
            1 => center[1] = v,
            2 => *amount = v,
            _ => {}
        },
        SpatialEffect::DropShadow {
            opacity,
            angle,
            distance,
            softness,
            ..
        } => set_one(&mut [opacity, angle, distance, softness], i, v),
        SpatialEffect::Glow {
            threshold,
            radius,
            intensity,
        } => set_one(&mut [threshold, radius, intensity], i, v),
    }
}

// ---------------------------------------------------------------------------
// Distort (whole-buffer coordinate-remap) effects
// ---------------------------------------------------------------------------

/// The editable scalar params of a [`DistortEffect`], in panel order. The four
/// `[f32;2]` corner/anchor/position points are surfaced as X/Y scalar rows.
pub fn distort_params(e: &DistortEffect) -> Vec<ScalarParam> {
    let p = ScalarParam::new;
    let pt = |label_x, label_y, v: [f32; 2]| {
        vec![
            p(label_x, v[0], -1.0, 2.0, 0.01),
            p(label_y, v[1], -1.0, 2.0, 0.01),
        ]
    };
    match *e {
        DistortEffect::CornerPin {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => {
            let mut v = pt("TL X", "TL Y", top_left);
            v.extend(pt("TR X", "TR Y", top_right));
            v.extend(pt("BR X", "BR Y", bottom_right));
            v.extend(pt("BL X", "BL Y", bottom_left));
            v
        }
        DistortEffect::Transform {
            anchor,
            position,
            scale,
            rotation,
            skew,
            opacity,
        } => {
            let mut v = pt("Anchor X", "Anchor Y", anchor);
            v.extend(pt("Pos X", "Pos Y", position));
            v.push(p("Scale", scale, 0.0, 4.0, 0.05));
            v.push(p("Rotation", rotation, -180.0, 180.0, 1.0));
            v.push(p("Skew", skew, -80.0, 80.0, 1.0));
            v.push(p("Opacity", opacity, 0.0, 1.0, 0.05));
            v
        }
        DistortEffect::Mirror { center, angle } => {
            let mut v = pt("Center X", "Center Y", center);
            v.push(p("Angle", angle, -180.0, 180.0, 1.0));
            v
        }
        DistortEffect::Polar { center, interp, .. } => {
            let mut v = pt("Center X", "Center Y", center);
            v.push(p("Interpolation", interp, 0.0, 1.0, 0.02));
            v
        }
        DistortEffect::DisplacementMap { scale_x, scale_y, .. } => {
            vec![
                p("Scale X", scale_x, -1.0, 1.0, 0.01),
                p("Scale Y", scale_y, -1.0, 1.0, 0.01),
            ]
        }
    }
}

/// Write `value` (clamped) into the [`DistortEffect`]'s scalar param at `i`.
pub fn set_distort(e: &mut DistortEffect, i: usize, value: f32) {
    let Some(sp) = distort_params(e).get(i).copied() else {
        return;
    };
    let v = value.clamp(sp.lo, sp.hi);
    match e {
        DistortEffect::CornerPin {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => {
            // Eight rows: TL{xy} TR{xy} BR{xy} BL{xy}.
            let corner = [top_left, top_right, bottom_right, bottom_left];
            if let Some(c) = corner.into_iter().nth(i / 2) {
                c[i % 2] = v;
            }
        }
        DistortEffect::Transform {
            anchor,
            position,
            scale,
            rotation,
            skew,
            opacity,
        } => match i {
            0 => anchor[0] = v,
            1 => anchor[1] = v,
            2 => position[0] = v,
            3 => position[1] = v,
            4 => *scale = v,
            5 => *rotation = v,
            6 => *skew = v,
            7 => *opacity = v,
            _ => {}
        },
        DistortEffect::Mirror { center, angle } => match i {
            0 => center[0] = v,
            1 => center[1] = v,
            2 => *angle = v,
            _ => {}
        },
        DistortEffect::Polar { center, interp, .. } => match i {
            0 => center[0] = v,
            1 => center[1] = v,
            2 => *interp = v,
            _ => {}
        },
        DistortEffect::DisplacementMap { scale_x, scale_y, .. } => match i {
            0 => *scale_x = v,
            1 => *scale_y = v,
            _ => {}
        },
    }
}

// ---------------------------------------------------------------------------
// Stylize (whole-buffer look-shaping) effects
// ---------------------------------------------------------------------------

/// The editable scalar params of a [`StylizeEffect`], in panel order.
pub fn stylize_params(e: &StylizeEffect) -> Vec<ScalarParam> {
    let p = ScalarParam::new;
    match *e {
        StylizeEffect::FindEdges { amount, .. } => vec![p("Amount", amount, 0.0, 8.0, 0.1)],
        StylizeEffect::Mosaic {
            horizontal,
            vertical,
        } => vec![
            p("Horizontal", horizontal as f32, 1.0, 200.0, 1.0),
            p("Vertical", vertical as f32, 1.0, 200.0, 1.0),
        ],
        StylizeEffect::Stroke { width, .. } => vec![
            p("Width", width, 0.0, 50.0, 0.5),
        ],
        StylizeEffect::Posterize { levels } => vec![p("Levels", levels as f32, 2.0, 32.0, 1.0)],
        StylizeEffect::Invert { amount } => vec![p("Amount", amount, 0.0, 1.0, 0.05)],
        StylizeEffect::Threshold { level } => vec![p("Level", level, 0.0, 1.0, 0.02)],
    }
}

/// Write `value` (clamped) into the [`StylizeEffect`]'s scalar param at `i`.
pub fn set_stylize(e: &mut StylizeEffect, i: usize, value: f32) {
    let Some(sp) = stylize_params(e).get(i).copied() else {
        return;
    };
    let v = value.clamp(sp.lo, sp.hi);
    match e {
        StylizeEffect::FindEdges { amount, .. } => {
            if i == 0 {
                *amount = v;
            }
        }
        StylizeEffect::Mosaic {
            horizontal,
            vertical,
        } => match i {
            0 => *horizontal = v.round().max(1.0) as u32,
            1 => *vertical = v.round().max(1.0) as u32,
            _ => {}
        },
        StylizeEffect::Stroke { width, .. } => {
            if i == 0 {
                *width = v;
            }
        }
        StylizeEffect::Posterize { levels } => {
            if i == 0 {
                *levels = v.round().max(2.0) as u32;
            }
        }
        StylizeEffect::Invert { amount } => {
            if i == 0 {
                *amount = v;
            }
        }
        StylizeEffect::Threshold { level } => {
            if i == 0 {
                *level = v;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Keying (whole-buffer matte-pull) effects
// ---------------------------------------------------------------------------

/// The editable scalar params of a [`KeyEffect`], in panel order. Key colours
/// and the `key_high` toggle are omitted (no swatch/checkbox widget yet).
pub fn key_params(e: &KeyEffect) -> Vec<ScalarParam> {
    let p = ScalarParam::new;
    match *e {
        KeyEffect::ColorKey {
            tolerance,
            softness,
            ..
        } => vec![
            p("Tolerance", tolerance, 0.0, 1.0, 0.02),
            p("Softness", softness, 0.0, 1.0, 0.02),
        ],
        KeyEffect::LumaKey {
            threshold,
            softness,
            ..
        } => vec![
            p("Threshold", threshold, 0.0, 1.0, 0.02),
            p("Softness", softness, 0.0, 1.0, 0.02),
        ],
        KeyEffect::ChromaKey {
            gain,
            balance,
            softness,
            ..
        } => vec![
            p("Gain", gain, 0.1, 4.0, 0.05),
            p("Balance", balance, 0.0, 1.0, 0.02),
            p("Softness", softness, 0.0, 1.0, 0.02),
        ],
        KeyEffect::SpillSuppression { amount, .. } => vec![p("Amount", amount, 0.0, 1.0, 0.02)],
        KeyEffect::MatteChoke {
            choke,
            clip_black,
            clip_white,
        } => vec![
            p("Choke", choke, -10.0, 10.0, 0.5),
            p("Clip black", clip_black, 0.0, 1.0, 0.02),
            p("Clip white", clip_white, 0.0, 1.0, 0.02),
        ],
    }
}

/// Write `value` (clamped) into the [`KeyEffect`]'s scalar param at `i`.
pub fn set_key(e: &mut KeyEffect, i: usize, value: f32) {
    let Some(sp) = key_params(e).get(i).copied() else {
        return;
    };
    let v = value.clamp(sp.lo, sp.hi);
    match e {
        KeyEffect::ColorKey {
            tolerance,
            softness,
            ..
        } => set_one(&mut [tolerance, softness], i, v),
        KeyEffect::LumaKey {
            threshold,
            softness,
            ..
        } => set_one(&mut [threshold, softness], i, v),
        KeyEffect::ChromaKey {
            gain,
            balance,
            softness,
            ..
        } => set_one(&mut [gain, balance, softness], i, v),
        KeyEffect::SpillSuppression { amount, .. } => set_one(&mut [amount], i, v),
        KeyEffect::MatteChoke {
            choke,
            clip_black,
            clip_white,
        } => set_one(&mut [choke, clip_black, clip_white], i, v),
    }
}

// ---------------------------------------------------------------------------
// Generate (whole-buffer fill) effects — scalar subset
// ---------------------------------------------------------------------------

/// The editable scalar params of a [`GenerateEffect`], in panel order. A
/// pragmatic scalar subset (the colour/enum/seed params are omitted) so the
/// common look knobs are tunable from the stepper rows.
pub fn generate_params(e: &GenerateEffect) -> Vec<ScalarParam> {
    let p = ScalarParam::new;
    match *e {
        GenerateEffect::FractalNoise {
            contrast,
            brightness,
            scale,
            evolution,
            opacity,
            ..
        } => vec![
            p("Contrast", contrast, 0.0, 4.0, 0.05),
            p("Brightness", brightness, -1.0, 1.0, 0.05),
            p("Scale", scale, 1.0, 600.0, 5.0),
            p("Evolution", evolution, -50.0, 50.0, 0.25),
            p("Opacity", opacity, 0.0, 1.0, 0.05),
        ],
        GenerateEffect::Ramp { opacity, .. } => vec![p("Opacity", opacity, 0.0, 1.0, 0.05)],
        GenerateEffect::Checkerboard {
            size_w,
            size_h,
            opacity,
            ..
        } => vec![
            p("Size W", size_w, 1.0, 600.0, 2.0),
            p("Size H", size_h, 1.0, 600.0, 2.0),
            p("Opacity", opacity, 0.0, 1.0, 0.05),
        ],
        GenerateEffect::FourColorGradient {
            blend,
            jitter,
            opacity,
            ..
        } => vec![
            p("Blend", blend, 0.0, 1.0, 0.02),
            p("Jitter", jitter, 0.0, 1.0, 0.02),
            p("Opacity", opacity, 0.0, 1.0, 0.05),
        ],
        GenerateEffect::Grid {
            size_w,
            size_h,
            border,
            opacity,
            ..
        } => vec![
            p("Size W", size_w, 1.0, 600.0, 2.0),
            p("Size H", size_h, 1.0, 600.0, 2.0),
            p("Border", border, 0.0, 50.0, 0.5),
            p("Opacity", opacity, 0.0, 1.0, 0.05),
        ],
        GenerateEffect::CellPattern {
            size,
            disorder,
            contrast,
            brightness,
            evolution,
            opacity,
            ..
        } => vec![
            p("Size", size, 1.0, 600.0, 5.0),
            p("Disorder", disorder, 0.0, 1.0, 0.02),
            p("Contrast", contrast, 0.0, 4.0, 0.05),
            p("Brightness", brightness, -1.0, 1.0, 0.05),
            p("Evolution", evolution, -50.0, 50.0, 0.25),
            p("Opacity", opacity, 0.0, 1.0, 0.05),
        ],
    }
}

/// Write `value` (clamped) into the [`GenerateEffect`]'s scalar param at `i`.
pub fn set_generate(e: &mut GenerateEffect, i: usize, value: f32) {
    let Some(sp) = generate_params(e).get(i).copied() else {
        return;
    };
    let v = value.clamp(sp.lo, sp.hi);
    match e {
        GenerateEffect::FractalNoise {
            contrast,
            brightness,
            scale,
            evolution,
            opacity,
            ..
        } => set_one(&mut [contrast, brightness, scale, evolution, opacity], i, v),
        GenerateEffect::Ramp { opacity, .. } => set_one(&mut [opacity], i, v),
        GenerateEffect::Checkerboard {
            size_w,
            size_h,
            opacity,
            ..
        } => set_one(&mut [size_w, size_h, opacity], i, v),
        GenerateEffect::FourColorGradient {
            blend,
            jitter,
            opacity,
            ..
        } => set_one(&mut [blend, jitter, opacity], i, v),
        GenerateEffect::Grid {
            size_w,
            size_h,
            border,
            opacity,
            ..
        } => set_one(&mut [size_w, size_h, border, opacity], i, v),
        GenerateEffect::CellPattern {
            size,
            disorder,
            contrast,
            brightness,
            evolution,
            opacity,
            ..
        } => set_one(
            &mut [size, disorder, contrast, brightness, evolution, opacity],
            i,
            v,
        ),
    }
}

/// Assign `v` into the `i`th `&mut f32` slot of a fixed list (the common case
/// where every scalar row maps 1:1 to a mutable field). No-op if `i` is out of
/// range.
fn set_one(slots: &mut [&mut f32], i: usize, v: f32) {
    if let Some(slot) = slots.get_mut(i) {
        **slot = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_param_roundtrips_and_clamps() {
        let mut e = Effect::defaults()[1]; // BrightnessContrast
        let params = color_params(&e);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].label, "Brightness");
        // Set within range.
        set_color(&mut e, 0, 0.5);
        assert_eq!(color_params(&e)[0].value, 0.5);
        // Clamp above hi (brightness hi = 1.0).
        set_color(&mut e, 0, 99.0);
        assert_eq!(color_params(&e)[0].value, 1.0);
        // Clamp below lo (brightness lo = -1.0).
        set_color(&mut e, 0, -99.0);
        assert_eq!(color_params(&e)[0].value, -1.0);
        // Out-of-range index is a no-op (doesn't panic).
        set_color(&mut e, 9, 0.0);
    }

    #[test]
    fn curves_exposes_five_points() {
        let e = Effect::defaults()[5]; // Curves
        let params = color_params(&e);
        assert_eq!(params.len(), 5);
        assert_eq!(params[2].value, 0.5); // identity mid
    }

    #[test]
    fn spatial_box_blur_iterations_round_to_int() {
        let mut e = SpatialEffect::defaults()[1]; // BoxBlur
        set_spatial(&mut e, 1, 3.4);
        // Reads back as 3 (rounded, min 1).
        assert_eq!(spatial_params(&e)[1].value, 3.0);
        set_spatial(&mut e, 1, 0.0);
        assert_eq!(spatial_params(&e)[1].value, 1.0); // floored to 1
    }

    #[test]
    fn distort_corner_pin_has_eight_rows() {
        let mut e = DistortEffect::defaults()[0]; // CornerPin
        assert_eq!(distort_params(&e).len(), 8);
        set_distort(&mut e, 0, 0.25);
        assert_eq!(distort_params(&e)[0].value, 0.25);
        set_distort(&mut e, 7, 0.75);
        assert_eq!(distort_params(&e)[7].value, 0.75);
    }

    #[test]
    fn stylize_mosaic_counts_are_integers() {
        let mut e = StylizeEffect::defaults()[1]; // Mosaic
        set_stylize(&mut e, 0, 12.6);
        assert_eq!(stylize_params(&e)[0].value, 13.0);
    }

    #[test]
    fn key_matte_choke_negative_choke() {
        let mut e = KeyEffect::defaults()[4]; // MatteChoke
        set_key(&mut e, 0, -3.0);
        assert_eq!(key_params(&e)[0].value, -3.0);
        // Clamp below -10.
        set_key(&mut e, 0, -50.0);
        assert_eq!(key_params(&e)[0].value, -10.0);
    }

    #[test]
    fn generate_fractal_noise_scalar_subset() {
        let mut e = GenerateEffect::defaults()[0]; // FractalNoise
        let params = generate_params(&e);
        assert_eq!(params.len(), 5);
        set_generate(&mut e, 0, 2.0); // contrast
        assert_eq!(generate_params(&e)[0].value, 2.0);
    }
}
