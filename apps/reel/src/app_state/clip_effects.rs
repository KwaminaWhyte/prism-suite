//! Phase 3 — built-in, non-destructive per-clip video effects.
//!
//! Each [`super::timeline::Clip`] owns an ordered `Vec<BuiltinEffect>` evaluated
//! top-to-bottom. This module owns:
//!
//! - the [`EffectParams`] enum (one fully-parametric variant per built-in
//!   effect) and the [`BuiltinEffect`] wrapper (params + `enabled` toggle),
//! - the *real, clamped math* for each effect — a stacked 2D affine transform
//!   matrix, combined crop rect, multiplied opacity, separable blend-mode
//!   compositing (reusing [`prism_core::BlendMode`]), summed blur/sharpen, and
//!   a drop-shadow sample,
//! - the [`AppClipEffectsExt::apply_clip_effects`] handler routed from
//!   `App::apply` for the add / remove / reorder / toggle / clear / edit
//!   actions.
//!
//! Nothing here mutates `App` outside of `apply_clip_effects`, keeping the single
//! mutation choke point intact.

use prism_core::BlendMode;

use super::timeline::Clip;
use super::{Action, App};

// --- Clamp bounds ------------------------------------------------------------

/// Smallest allowed transform scale (avoids a degenerate zero/negative matrix).
pub const MIN_SCALE: f32 = 0.01;
/// Largest allowed transform scale.
pub const MAX_SCALE: f32 = 100.0;
/// Pixel limit applied to position / anchor / shadow-offset components.
pub const POS_LIMIT: f32 = 100_000.0;
/// Largest allowed blur / drop-shadow blur radius (pixels).
pub const MAX_BLUR: f32 = 1_000.0;
/// Largest allowed unsharp-mask sharpen amount.
pub const MAX_SHARPEN: f32 = 10.0;
/// Opposing crop fractions (left+right, top+bottom) never sum past this, so at
/// least a sliver of the clip always survives.
pub const MAX_CROP_PAIR: f32 = 0.999;

// --- Small numeric helpers ---------------------------------------------------

/// Clamp to `[lo, hi]`, mapping any non-finite input to `lo`.
fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { lo }
}

/// Clamp to `[0, 1]`, mapping non-finite input to `0`.
fn clamp01(v: f32) -> f32 {
    clampf(v, 0.0, 1.0)
}

/// Clamp an RGBA color channel-wise to `[0, 1]`.
fn clamp_color(c: [f32; 4]) -> [f32; 4] {
    [clamp01(c[0]), clamp01(c[1]), clamp01(c[2]), clamp01(c[3])]
}

/// Clamp two opposing crop fractions into `[0, 1]` and proportionally scale them
/// down so their sum never reaches `1.0` (keeps a visible sliver).
fn cap_pair(a: f32, b: f32) -> (f32, f32) {
    let a = clamp01(a);
    let b = clamp01(b);
    let sum = a + b;
    if sum > MAX_CROP_PAIR && sum > 0.0 {
        let k = MAX_CROP_PAIR / sum;
        (a * k, b * k)
    } else {
        (a, b)
    }
}

// --- Effect parameters -------------------------------------------------------

/// Parameters for one built-in clip effect. Every variant is fully parametric
/// with real, clamped math (see the `impl Clip` block below).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectParams {
    /// 2D affine transform around an anchor pivot. `pos_*` are pixel offsets,
    /// `scale_*` multipliers (`1.0` = identity), `rotation_deg` clockwise
    /// degrees, `anchor_*` the pivot in pixels relative to the clip origin.
    Transform {
        pos_x: f32,
        pos_y: f32,
        scale_x: f32,
        scale_y: f32,
        rotation_deg: f32,
        anchor_x: f32,
        anchor_y: f32,
    },
    /// Crop fractions per side in `0..1` (left+right and top+bottom each capped
    /// below `1.0`).
    Crop { left: f32, right: f32, top: f32, bottom: f32 },
    /// Constant opacity multiplier in `0..1`.
    Opacity { opacity: f32 },
    /// Compositing blend mode (reuses [`prism_core::BlendMode`]).
    Blend { mode: BlendMode },
    /// Drop shadow: pixel offset, blur radius, opacity `0..1`, straight RGBA tint.
    DropShadow {
        offset_x: f32,
        offset_y: f32,
        blur: f32,
        opacity: f32,
        color: [f32; 4],
    },
    /// Gaussian blur with a pixel radius `>= 0` (`0` = no-op).
    GaussianBlur { radius: f32 },
    /// Unsharp-mask sharpen amount `>= 0` (`0` = no-op).
    Sharpen { amount: f32 },
}

impl EffectParams {
    /// A stable identifier for the effect kind (independent of parameter values).
    pub fn label(&self) -> &'static str {
        match self {
            EffectParams::Transform { .. } => "Transform",
            EffectParams::Crop { .. } => "Crop",
            EffectParams::Opacity { .. } => "Opacity",
            EffectParams::Blend { .. } => "Blend",
            EffectParams::DropShadow { .. } => "Drop Shadow",
            EffectParams::GaussianBlur { .. } => "Gaussian Blur",
            EffectParams::Sharpen { .. } => "Sharpen",
        }
    }

    /// Return a copy with every numeric field clamped into its valid range.
    pub fn sanitized(self) -> EffectParams {
        match self {
            EffectParams::Transform {
                pos_x,
                pos_y,
                scale_x,
                scale_y,
                rotation_deg,
                anchor_x,
                anchor_y,
            } => EffectParams::Transform {
                pos_x: clampf(pos_x, -POS_LIMIT, POS_LIMIT),
                pos_y: clampf(pos_y, -POS_LIMIT, POS_LIMIT),
                scale_x: clampf(scale_x, MIN_SCALE, MAX_SCALE),
                scale_y: clampf(scale_y, MIN_SCALE, MAX_SCALE),
                rotation_deg: if rotation_deg.is_finite() { rotation_deg } else { 0.0 },
                anchor_x: clampf(anchor_x, -POS_LIMIT, POS_LIMIT),
                anchor_y: clampf(anchor_y, -POS_LIMIT, POS_LIMIT),
            },
            EffectParams::Crop { left, right, top, bottom } => {
                let (left, right) = cap_pair(left, right);
                let (top, bottom) = cap_pair(top, bottom);
                EffectParams::Crop { left, right, top, bottom }
            }
            EffectParams::Opacity { opacity } => {
                EffectParams::Opacity { opacity: clamp01(opacity) }
            }
            EffectParams::Blend { mode } => EffectParams::Blend { mode },
            EffectParams::DropShadow { offset_x, offset_y, blur, opacity, color } => {
                EffectParams::DropShadow {
                    offset_x: clampf(offset_x, -POS_LIMIT, POS_LIMIT),
                    offset_y: clampf(offset_y, -POS_LIMIT, POS_LIMIT),
                    blur: clampf(blur, 0.0, MAX_BLUR),
                    opacity: clamp01(opacity),
                    color: clamp_color(color),
                }
            }
            EffectParams::GaussianBlur { radius } => {
                EffectParams::GaussianBlur { radius: clampf(radius, 0.0, MAX_BLUR) }
            }
            EffectParams::Sharpen { amount } => {
                EffectParams::Sharpen { amount: clampf(amount, 0.0, MAX_SHARPEN) }
            }
        }
    }
}

// --- Effect wrapper ----------------------------------------------------------

/// A single entry in a clip's built-in effect stack: parameters plus an
/// `enabled` toggle for non-destructive bypass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuiltinEffect {
    pub params: EffectParams,
    pub enabled: bool,
}

impl BuiltinEffect {
    /// Wrap (and sanitize) explicit parameters; starts enabled.
    pub fn new(params: EffectParams) -> Self {
        Self { params: params.sanitized(), enabled: true }
    }

    /// Identity transform (no offset, unit scale, no rotation, origin anchor).
    pub fn transform() -> Self {
        Self::new(EffectParams::Transform {
            pos_x: 0.0,
            pos_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            anchor_x: 0.0,
            anchor_y: 0.0,
        })
    }

    /// Zero crop on every side.
    pub fn crop() -> Self {
        Self::new(EffectParams::Crop { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 })
    }

    /// Full (1.0) opacity.
    pub fn opacity() -> Self {
        Self::new(EffectParams::Opacity { opacity: 1.0 })
    }

    /// A blend effect with the given mode.
    pub fn blend(mode: BlendMode) -> Self {
        Self::new(EffectParams::Blend { mode })
    }

    /// A subtle default drop shadow (offset down-right, soft, half opacity, black).
    pub fn drop_shadow() -> Self {
        Self::new(EffectParams::DropShadow {
            offset_x: 8.0,
            offset_y: 8.0,
            blur: 10.0,
            opacity: 0.5,
            color: [0.0, 0.0, 0.0, 1.0],
        })
    }

    /// Gaussian blur with a default radius.
    pub fn gaussian_blur() -> Self {
        Self::new(EffectParams::GaussianBlur { radius: 5.0 })
    }

    /// Unsharp-mask sharpen with a default amount.
    pub fn sharpen() -> Self {
        Self::new(EffectParams::Sharpen { amount: 1.0 })
    }

    /// Human-readable kind label.
    pub fn label(&self) -> &'static str {
        self.params.label()
    }
}

// --- 2D affine matrix helpers (2x3 row form: [a, b, c, d, e, f]) -------------
//
// A point (x, y) maps to (a*x + c*y + e, b*x + d*y + f) — the same packing used
// by CSS `matrix()`, Cairo, and Skia.

/// The identity affine matrix.
pub fn mat_identity() -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

/// Compose two affine matrices: the result applies `n` first, then `m`
/// (i.e. it represents `m ∘ n`, so a point maps as `m(n(p))`).
pub fn mat_mul(m: [f32; 6], n: [f32; 6]) -> [f32; 6] {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// Pure translation.
pub fn mat_translate(tx: f32, ty: f32) -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

/// Pure (possibly non-uniform) scale.
pub fn mat_scale(sx: f32, sy: f32) -> [f32; 6] {
    [sx, 0.0, 0.0, sy, 0.0, 0.0]
}

/// Pure clockwise rotation by `deg` degrees.
pub fn mat_rotate_deg(deg: f32) -> [f32; 6] {
    let (s, c) = deg.to_radians().sin_cos();
    [c, s, -s, c, 0.0, 0.0]
}

/// Apply an affine matrix to a point.
pub fn mat_apply(m: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// The affine matrix for a single [`EffectParams::Transform`], pivoting about
/// its anchor: `T(anchor + pos) · R(rot) · S(scale) · T(-anchor)`. Returns the
/// identity for any non-transform variant.
pub fn transform_matrix(params: &EffectParams) -> [f32; 6] {
    if let EffectParams::Transform {
        pos_x,
        pos_y,
        scale_x,
        scale_y,
        rotation_deg,
        anchor_x,
        anchor_y,
    } = *params
    {
        let m = mat_translate(anchor_x + pos_x, anchor_y + pos_y);
        let m = mat_mul(m, mat_rotate_deg(rotation_deg));
        let m = mat_mul(m, mat_scale(scale_x, scale_y));
        mat_mul(m, mat_translate(-anchor_x, -anchor_y))
    } else {
        mat_identity()
    }
}

// --- Separable blend-mode compositing ---------------------------------------

/// W3C separable blend function for one channel. `b` is the backdrop, `s` the
/// source (both straight, `0..1`). HSL modes fall back to the source color.
pub fn blend_channel(mode: BlendMode, b: f32, s: f32) -> f32 {
    let v = match mode {
        BlendMode::Normal => s,
        BlendMode::Multiply => b * s,
        BlendMode::Screen => 1.0 - (1.0 - b) * (1.0 - s),
        BlendMode::Overlay => {
            if b <= 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) }
        }
        BlendMode::Darken => b.min(s),
        BlendMode::Lighten => b.max(s),
        BlendMode::ColorDodge => {
            if b <= 0.0 { 0.0 } else if s >= 1.0 { 1.0 } else { (b / (1.0 - s)).min(1.0) }
        }
        BlendMode::ColorBurn => {
            if b >= 1.0 { 1.0 } else if s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / s).min(1.0) }
        }
        BlendMode::HardLight => {
            if s <= 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) }
        }
        BlendMode::SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        BlendMode::LinearDodge => (b + s).min(1.0), // "Add"
        BlendMode::LinearBurn => (b + s - 1.0).max(0.0),
        // Non-separable HSL modes — CPU fallback to the source color.
        _ => s,
    };
    v.clamp(0.0, 1.0)
}

/// Composite straight-RGBA `over` (top) onto `base` (bottom) using `mode`,
/// returning the resulting straight RGBA. Uses the standard PDF blend formula:
/// the blended color is weighted by the backdrop alpha, then alpha-composited.
pub fn blend_sample(mode: BlendMode, base: [f32; 4], over: [f32; 4]) -> [f32; 4] {
    let ba = base[3].clamp(0.0, 1.0);
    let oa = over[3].clamp(0.0, 1.0);
    let out_a = oa + ba * (1.0 - oa);
    let mut out = [0.0f32; 4];
    for i in 0..3 {
        let blended = blend_channel(mode, base[i], over[i]);
        // Source color = backdrop-weighted mix of the raw source and blended result.
        let src = (1.0 - ba) * over[i] + ba * blended;
        let c = oa * src + ba * (1.0 - oa) * base[i];
        out[i] = if out_a > 0.0 { (c / out_a).clamp(0.0, 1.0) } else { 0.0 };
    }
    out[3] = out_a.clamp(0.0, 1.0);
    out
}

// --- Per-clip evaluation -----------------------------------------------------

impl Clip {
    /// Stacked affine transform from every enabled [`EffectParams::Transform`],
    /// applied in stack order (the first effect is the innermost). Identity when
    /// the clip has no enabled transform effects.
    pub fn effective_transform_matrix(&self) -> [f32; 6] {
        let mut m = mat_identity();
        for e in &self.builtin_effects {
            if e.enabled && matches!(e.params, EffectParams::Transform { .. }) {
                m = mat_mul(transform_matrix(&e.params), m);
            }
        }
        m
    }

    /// Combined crop rect `(left, right, top, bottom)` summing every enabled
    /// [`EffectParams::Crop`], with opposing pairs capped below `1.0`.
    pub fn effective_crop(&self) -> (f32, f32, f32, f32) {
        let (mut l, mut r, mut t, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for e in &self.builtin_effects {
            if !e.enabled {
                continue;
            }
            if let EffectParams::Crop { left, right, top, bottom } = e.params {
                l += left;
                r += right;
                t += top;
                b += bottom;
            }
        }
        let (l, r) = cap_pair(l, r);
        let (t, b) = cap_pair(t, b);
        (l, r, t, b)
    }

    /// Fraction of the clip area still visible after [`effective_crop`].
    ///
    /// [`effective_crop`]: Clip::effective_crop
    pub fn effective_crop_area(&self) -> f32 {
        let (l, r, t, b) = self.effective_crop();
        ((1.0 - l - r) * (1.0 - t - b)).clamp(0.0, 1.0)
    }

    /// Effective opacity: the clip's base opacity times every enabled
    /// [`EffectParams::Opacity`] multiplier, clamped to `0..1`.
    pub fn effective_builtin_opacity(&self) -> f32 {
        let mut o = self.opacity.clamp(0.0, 1.0);
        for e in &self.builtin_effects {
            if !e.enabled {
                continue;
            }
            if let EffectParams::Opacity { opacity } = e.params {
                o *= opacity.clamp(0.0, 1.0);
            }
        }
        o.clamp(0.0, 1.0)
    }

    /// The top-most enabled [`EffectParams::Blend`] mode (later entries win), or
    /// [`BlendMode::Normal`] when none is present.
    pub fn effective_blend_mode(&self) -> BlendMode {
        let mut mode = BlendMode::Normal;
        for e in &self.builtin_effects {
            if e.enabled {
                if let EffectParams::Blend { mode: m } = e.params {
                    mode = m;
                }
            }
        }
        mode
    }

    /// Summed radius of every enabled [`EffectParams::GaussianBlur`], clamped.
    pub fn effective_blur_radius(&self) -> f32 {
        let mut r = 0.0f32;
        for e in &self.builtin_effects {
            if e.enabled {
                if let EffectParams::GaussianBlur { radius } = e.params {
                    r += radius;
                }
            }
        }
        r.clamp(0.0, MAX_BLUR)
    }

    /// Summed amount of every enabled [`EffectParams::Sharpen`], clamped.
    pub fn effective_sharpen_amount(&self) -> f32 {
        let mut a = 0.0f32;
        for e in &self.builtin_effects {
            if e.enabled {
                if let EffectParams::Sharpen { amount } = e.params {
                    a += amount;
                }
            }
        }
        a.clamp(0.0, MAX_SHARPEN)
    }

    /// The first enabled [`EffectParams::DropShadow`] as
    /// `(offset, blur, straight-RGBA sample)`, where the sample's alpha already
    /// folds in the shadow opacity. `None` when no drop shadow is active.
    pub fn effective_drop_shadow(&self) -> Option<([f32; 2], f32, [f32; 4])> {
        for e in &self.builtin_effects {
            if !e.enabled {
                continue;
            }
            if let EffectParams::DropShadow { offset_x, offset_y, blur, opacity, color } = e.params {
                let sample = [color[0], color[1], color[2], (color[3] * opacity).clamp(0.0, 1.0)];
                return Some(([offset_x, offset_y], blur, sample));
            }
        }
        None
    }

    /// Count of enabled built-in effects.
    pub fn enabled_builtin_effect_count(&self) -> usize {
        self.builtin_effects.iter().filter(|e| e.enabled).count()
    }
}

// --- App action handler ------------------------------------------------------

pub trait AppClipEffectsExt {
    fn apply_clip_effects(&mut self, action: Action);
}

impl AppClipEffectsExt for App {
    fn apply_clip_effects(&mut self, action: Action) {
        match action {
            Action::AddBuiltinEffect { clip_idx, mut effect } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    effect.params = effect.params.sanitized();
                    c.builtin_effects.push(effect);
                    self.host.mark_dirty();
                }
            }
            Action::RemoveBuiltinEffect { clip_idx, effect_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if effect_idx < c.builtin_effects.len() {
                        c.builtin_effects.remove(effect_idx);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ReorderBuiltinEffects { clip_idx, from, to } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    let len = c.builtin_effects.len();
                    if from < len && to < len && from != to {
                        let e = c.builtin_effects.remove(from);
                        c.builtin_effects.insert(to, e);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleBuiltinEffect { clip_idx, effect_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if let Some(e) = c.builtin_effects.get_mut(effect_idx) {
                        e.enabled = !e.enabled;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ClearBuiltinEffects { clip_idx } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if !c.builtin_effects.is_empty() {
                        c.builtin_effects.clear();
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetBuiltinEffectParams { clip_idx, effect_idx, params } => {
                if let Some(c) = self.project.clips.get_mut(clip_idx) {
                    if let Some(e) = c.builtin_effects.get_mut(effect_idx) {
                        e.params = params.sanitized();
                        self.host.mark_dirty();
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{Action, App};

    fn add(app: &mut App, effect: BuiltinEffect) {
        app.apply(Action::AddBuiltinEffect { clip_idx: 0, effect });
    }

    // --- Actions -------------------------------------------------------------

    #[test]
    fn add_builtin_effect_pushes_onto_stack() {
        let mut app = App::new();
        assert!(app.project.clips[0].builtin_effects.is_empty());
        add(&mut app, BuiltinEffect::gaussian_blur());
        assert_eq!(app.project.clips[0].builtin_effects.len(), 1);
        assert_eq!(app.project.clips[0].builtin_effects[0].label(), "Gaussian Blur");
    }

    #[test]
    fn add_builtin_effect_sanitizes_params() {
        let mut app = App::new();
        // Wildly out-of-range scale + crop should be clamped on insertion.
        add(
            &mut app,
            BuiltinEffect {
                params: EffectParams::Transform {
                    pos_x: 0.0,
                    pos_y: 0.0,
                    scale_x: 999.0,
                    scale_y: -4.0,
                    rotation_deg: 30.0,
                    anchor_x: 0.0,
                    anchor_y: 0.0,
                },
                enabled: true,
            },
        );
        if let EffectParams::Transform { scale_x, scale_y, .. } = app.project.clips[0].builtin_effects[0].params {
            assert!((scale_x - MAX_SCALE).abs() < 1e-4);
            assert!((scale_y - MIN_SCALE).abs() < 1e-4);
        } else {
            panic!("expected transform");
        }
    }

    #[test]
    fn remove_builtin_effect() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::transform());
        add(&mut app, BuiltinEffect::sharpen());
        app.apply(Action::RemoveBuiltinEffect { clip_idx: 0, effect_idx: 0 });
        assert_eq!(app.project.clips[0].builtin_effects.len(), 1);
        assert_eq!(app.project.clips[0].builtin_effects[0].label(), "Sharpen");
        // Out-of-range index is a no-op.
        app.apply(Action::RemoveBuiltinEffect { clip_idx: 0, effect_idx: 99 });
        assert_eq!(app.project.clips[0].builtin_effects.len(), 1);
    }

    #[test]
    fn toggle_builtin_effect_flips_enabled() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::opacity());
        assert!(app.project.clips[0].builtin_effects[0].enabled);
        app.apply(Action::ToggleBuiltinEffect { clip_idx: 0, effect_idx: 0 });
        assert!(!app.project.clips[0].builtin_effects[0].enabled);
        app.apply(Action::ToggleBuiltinEffect { clip_idx: 0, effect_idx: 0 });
        assert!(app.project.clips[0].builtin_effects[0].enabled);
    }

    #[test]
    fn clear_builtin_effects() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::transform());
        add(&mut app, BuiltinEffect::crop());
        app.apply(Action::ClearBuiltinEffects { clip_idx: 0 });
        assert!(app.project.clips[0].builtin_effects.is_empty());
    }

    #[test]
    fn reorder_builtin_effects() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::gaussian_blur());
        add(&mut app, BuiltinEffect::sharpen());
        add(&mut app, BuiltinEffect::crop());
        // Move the first (blur) to the end.
        app.apply(Action::ReorderBuiltinEffects { clip_idx: 0, from: 0, to: 2 });
        let labels: Vec<_> = app.project.clips[0].builtin_effects.iter().map(|e| e.label()).collect();
        assert_eq!(labels, vec!["Sharpen", "Crop", "Gaussian Blur"]);
    }

    #[test]
    fn reorder_out_of_range_is_noop() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::transform());
        app.apply(Action::ReorderBuiltinEffects { clip_idx: 0, from: 0, to: 5 });
        assert_eq!(app.project.clips[0].builtin_effects.len(), 1);
    }

    #[test]
    fn set_builtin_effect_params_clamps() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::opacity());
        app.apply(Action::SetBuiltinEffectParams {
            clip_idx: 0,
            effect_idx: 0,
            params: EffectParams::Opacity { opacity: 4.0 },
        });
        if let EffectParams::Opacity { opacity } = app.project.clips[0].builtin_effects[0].params {
            assert!((opacity - 1.0).abs() < 1e-5);
        } else {
            panic!("expected opacity");
        }
    }

    #[test]
    fn actions_on_missing_clip_do_not_panic() {
        let mut app = App::new();
        app.apply(Action::AddBuiltinEffect { clip_idx: 999, effect: BuiltinEffect::transform() });
        app.apply(Action::RemoveBuiltinEffect { clip_idx: 999, effect_idx: 0 });
        app.apply(Action::ToggleBuiltinEffect { clip_idx: 999, effect_idx: 0 });
        app.apply(Action::ClearBuiltinEffects { clip_idx: 999 });
        app.apply(Action::ReorderBuiltinEffects { clip_idx: 999, from: 0, to: 0 });
    }

    // --- Sanitize / clamp boundaries -----------------------------------------

    #[test]
    fn crop_pair_capped_below_one() {
        let p = EffectParams::Crop { left: 0.8, right: 0.8, top: 0.0, bottom: 0.0 }.sanitized();
        if let EffectParams::Crop { left, right, .. } = p {
            assert!(left + right < 1.0);
            assert!(left + right <= MAX_CROP_PAIR + 1e-4);
            // Proportional scaling keeps them equal.
            assert!((left - right).abs() < 1e-5);
        } else {
            panic!("expected crop");
        }
    }

    #[test]
    fn crop_clamps_negatives_to_zero() {
        let p = EffectParams::Crop { left: -0.5, right: 0.3, top: -2.0, bottom: 1.4 }.sanitized();
        if let EffectParams::Crop { left, right, top, bottom } = p {
            assert!((left - 0.0).abs() < 1e-6);
            assert!((right - 0.3).abs() < 1e-6);
            assert!((top - 0.0).abs() < 1e-6);
            assert!(bottom <= 1.0);
        } else {
            panic!("expected crop");
        }
    }

    #[test]
    fn sanitize_maps_nonfinite_to_safe() {
        let p = EffectParams::Sharpen { amount: f32::NAN }.sanitized();
        if let EffectParams::Sharpen { amount } = p {
            assert_eq!(amount, 0.0);
        } else {
            panic!("expected sharpen");
        }
        let g = EffectParams::GaussianBlur { radius: f32::INFINITY }.sanitized();
        if let EffectParams::GaussianBlur { radius } = g {
            assert!((radius - 0.0).abs() < 1e-6);
        } else {
            panic!("expected blur");
        }
    }

    #[test]
    fn drop_shadow_clamps_opacity_and_color() {
        let p = EffectParams::DropShadow {
            offset_x: 5.0,
            offset_y: 5.0,
            blur: 9999.0,
            opacity: 3.0,
            color: [2.0, -1.0, 0.5, 5.0],
        }
        .sanitized();
        if let EffectParams::DropShadow { blur, opacity, color, .. } = p {
            assert!((blur - MAX_BLUR).abs() < 1e-3);
            assert!((opacity - 1.0).abs() < 1e-6);
            assert_eq!(color, [1.0, 0.0, 0.5, 1.0]);
        } else {
            panic!("expected drop shadow");
        }
    }

    // --- Transform matrix math -----------------------------------------------

    #[test]
    fn identity_transform_matrix_for_empty_stack() {
        let app = App::new();
        assert_eq!(app.project.clips[0].effective_transform_matrix(), mat_identity());
    }

    #[test]
    fn translation_moves_point() {
        let m = mat_translate(10.0, -5.0);
        let (x, y) = mat_apply(m, 3.0, 4.0);
        assert!((x - 13.0).abs() < 1e-5);
        assert!((y - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn rotate_90_degrees_maps_axes() {
        // Clockwise 90°: (1,0) -> (0,1).
        let m = mat_rotate_deg(90.0);
        let (x, y) = mat_apply(m, 1.0, 0.0);
        assert!(x.abs() < 1e-5);
        assert!((y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn transform_pivots_around_anchor() {
        // Rotating 180° around anchor (100,100): the anchor point stays put.
        let p = EffectParams::Transform {
            pos_x: 0.0,
            pos_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 180.0,
            anchor_x: 100.0,
            anchor_y: 100.0,
        };
        let m = transform_matrix(&p);
        let (ax, ay) = mat_apply(m, 100.0, 100.0);
        assert!((ax - 100.0).abs() < 1e-3);
        assert!((ay - 100.0).abs() < 1e-3);
        // A point at (110,100) flips to (90,100).
        let (px, py) = mat_apply(m, 110.0, 100.0);
        assert!((px - 90.0).abs() < 1e-3);
        assert!((py - 100.0).abs() < 1e-3);
    }

    #[test]
    fn stacked_transforms_compose() {
        let mut app = App::new();
        add(
            &mut app,
            BuiltinEffect::new(EffectParams::Transform {
                pos_x: 10.0,
                pos_y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_deg: 0.0,
                anchor_x: 0.0,
                anchor_y: 0.0,
            }),
        );
        add(
            &mut app,
            BuiltinEffect::new(EffectParams::Transform {
                pos_x: 0.0,
                pos_y: 20.0,
                scale_x: 2.0,
                scale_y: 2.0,
                rotation_deg: 0.0,
                anchor_x: 0.0,
                anchor_y: 0.0,
            }),
        );
        // Inner (first) translate +10x, then outer scale 2 + translate +20y.
        // Point (0,0) -> (10,0) -> (20,20).
        let m = app.project.clips[0].effective_transform_matrix();
        let (x, y) = mat_apply(m, 0.0, 0.0);
        assert!((x - 20.0).abs() < 1e-4, "x was {x}");
        assert!((y - 20.0).abs() < 1e-4, "y was {y}");
    }

    #[test]
    fn disabled_transform_is_skipped() {
        let mut app = App::new();
        add(
            &mut app,
            BuiltinEffect::new(EffectParams::Transform {
                pos_x: 50.0,
                pos_y: 50.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_deg: 0.0,
                anchor_x: 0.0,
                anchor_y: 0.0,
            }),
        );
        app.apply(Action::ToggleBuiltinEffect { clip_idx: 0, effect_idx: 0 });
        assert_eq!(app.project.clips[0].effective_transform_matrix(), mat_identity());
    }

    // --- Crop / opacity / blur / sharpen accumulation ------------------------

    #[test]
    fn effective_crop_sums_and_caps() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::new(EffectParams::Crop { left: 0.2, right: 0.1, top: 0.0, bottom: 0.0 }));
        add(&mut app, BuiltinEffect::new(EffectParams::Crop { left: 0.3, right: 0.0, top: 0.0, bottom: 0.0 }));
        let (l, r, _t, _b) = app.project.clips[0].effective_crop();
        assert!((l - 0.5).abs() < 1e-5);
        assert!((r - 0.1).abs() < 1e-5);
        // Area visible = (1 - 0.5 - 0.1) * 1 = 0.4.
        assert!((app.project.clips[0].effective_crop_area() - 0.4).abs() < 1e-5);
    }

    #[test]
    fn effective_opacity_multiplies() {
        let mut app = App::new();
        app.project.clips[0].opacity = 0.8;
        add(&mut app, BuiltinEffect::new(EffectParams::Opacity { opacity: 0.5 }));
        add(&mut app, BuiltinEffect::new(EffectParams::Opacity { opacity: 0.5 }));
        // 0.8 * 0.5 * 0.5 = 0.2.
        assert!((app.project.clips[0].effective_builtin_opacity() - 0.2).abs() < 1e-5);
    }

    #[test]
    fn effective_blur_and_sharpen_sum() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::new(EffectParams::GaussianBlur { radius: 3.0 }));
        add(&mut app, BuiltinEffect::new(EffectParams::GaussianBlur { radius: 4.0 }));
        add(&mut app, BuiltinEffect::new(EffectParams::Sharpen { amount: 0.5 }));
        assert!((app.project.clips[0].effective_blur_radius() - 7.0).abs() < 1e-5);
        assert!((app.project.clips[0].effective_sharpen_amount() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn effective_blend_mode_takes_last_enabled() {
        let mut app = App::new();
        add(&mut app, BuiltinEffect::blend(BlendMode::Multiply));
        add(&mut app, BuiltinEffect::blend(BlendMode::Screen));
        assert_eq!(app.project.clips[0].effective_blend_mode(), BlendMode::Screen);
        // Disabling the last one falls back to the earlier enabled mode.
        app.apply(Action::ToggleBuiltinEffect { clip_idx: 0, effect_idx: 1 });
        assert_eq!(app.project.clips[0].effective_blend_mode(), BlendMode::Multiply);
    }

    #[test]
    fn effective_drop_shadow_folds_opacity_into_alpha() {
        let mut app = App::new();
        add(
            &mut app,
            BuiltinEffect::new(EffectParams::DropShadow {
                offset_x: 6.0,
                offset_y: 8.0,
                blur: 4.0,
                opacity: 0.5,
                color: [0.0, 0.0, 0.0, 1.0],
            }),
        );
        let (offset, blur, sample) = app.project.clips[0].effective_drop_shadow().unwrap();
        assert_eq!(offset, [6.0, 8.0]);
        assert!((blur - 4.0).abs() < 1e-5);
        assert!((sample[3] - 0.5).abs() < 1e-5);
        assert!(app.project.clips[0].enabled_builtin_effect_count() == 1);
    }

    // --- Blend math ----------------------------------------------------------

    #[test]
    fn blend_channel_multiply_and_screen() {
        assert!((blend_channel(BlendMode::Multiply, 0.5, 0.5) - 0.25).abs() < 1e-5);
        assert!((blend_channel(BlendMode::Screen, 0.5, 0.5) - 0.75).abs() < 1e-5);
        // Normal returns the source.
        assert!((blend_channel(BlendMode::Normal, 0.2, 0.9) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn blend_sample_opaque_normal_returns_source() {
        let base = [0.1, 0.2, 0.3, 1.0];
        let over = [0.7, 0.6, 0.5, 1.0];
        let out = blend_sample(BlendMode::Normal, base, over);
        for i in 0..3 {
            assert!((out[i] - over[i]).abs() < 1e-5, "channel {i}");
        }
        assert!((out[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn blend_sample_transparent_source_keeps_base() {
        let base = [0.4, 0.4, 0.4, 1.0];
        let over = [0.9, 0.9, 0.9, 0.0];
        let out = blend_sample(BlendMode::Multiply, base, over);
        for i in 0..3 {
            assert!((out[i] - base[i]).abs() < 1e-5, "channel {i}");
        }
    }

    #[test]
    fn blend_sample_difference_is_symmetric_magnitude() {
        let out = blend_sample(BlendMode::Difference, [0.8, 0.0, 0.0, 1.0], [0.3, 0.0, 0.0, 1.0]);
        assert!((out[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn matrix_mul_identity_is_noop() {
        let m = mat_mul(mat_identity(), mat_translate(5.0, 7.0));
        assert_eq!(m, mat_translate(5.0, 7.0));
    }
}
