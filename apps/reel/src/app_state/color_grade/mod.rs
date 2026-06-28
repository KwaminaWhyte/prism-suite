//! Phase 4 — **Lumetri-grade colour math + scopes** (pure, deterministic).
//!
//! This module owns a full per-clip colour grade ([`LumetriGrade`]) and the
//! single pure pipeline [`grade_pixel`] that applies it to one straight-sRGB
//! `Rgba`, plus the [`scopes`] sub-module of pure frame-measurement functions.
//!
//! It deliberately does **not** disturb the simpler `timeline::ColorGrade`
//! (exposure/contrast/saturation) already stored on every `Clip`; this is the
//! richer, secondary, Lumetri-style corrector kept off the hot `timeline.rs`
//! file and stored per-clip in [`App::clip_grades`] (a sparse map keyed by clip
//! index), so it costs nothing for un-graded clips and never grows the `Clip`
//! struct.
//!
//! # Pipeline order (documented, fixed)
//! `exposure → temperature/tint → tone (whites/blacks/highlights/shadows) →
//! lift/gamma/gain → contrast → curves → saturation → secondary`.
//!
//! All stages but **exposure** operate in straight-sRGB display-referred 0..1
//! space (matching the rest of Reel's CPU grading, and making "contrast pivots
//! on mid-grey 0.5" and "saturation 0 ⇒ grey" exact). **Exposure** is the one
//! physically-light operation: it decodes to linear via `prism_color`, scales by
//! `2^stops`, and re-encodes — so `+1` stop doubles the linear value.

use std::collections::HashMap;

use prism_core::color::{linear_to_srgb, srgb_to_linear};

use super::{rgb_to_hsl, hsl_to_rgb, Action, App};

pub mod scopes;
pub use scopes::{
    luma_of, luma_waveform, rgb_histogram, rgb_parade, vectorscope, ScopeHistogram, ScopeParade,
    ScopeVectorscope, ScopeWaveform,
};

// =============================================================================
// Lift / Gamma / Gain wheels
// =============================================================================

/// One colour wheel: an RGB trackball offset plus a master (luminance ring).
/// All three components are neutral at `0.0` regardless of which range the wheel
/// drives — [`LiftGammaGain`] interprets them per range.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Wheel {
    /// Per-channel trackball value (R, G, B). `0` = neutral.
    pub rgb: [f32; 3],
    /// Master ring value applied to all channels. `0` = neutral.
    pub master: f32,
}

impl Wheel {
    pub fn is_identity(&self) -> bool {
        self.rgb == [0.0, 0.0, 0.0] && self.master == 0.0
    }

    /// The effective per-channel amount = trackball + master.
    fn per_channel(&self) -> [f32; 3] {
        [
            self.rgb[0] + self.master,
            self.rgb[1] + self.master,
            self.rgb[2] + self.master,
        ]
    }

    fn clamp(&mut self) {
        for c in self.rgb.iter_mut() {
            *c = c.clamp(-1.0, 1.0);
        }
        self.master = self.master.clamp(-1.0, 1.0);
    }
}

/// Which of the three wheels an action targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelKind {
    Lift,
    Gamma,
    Gain,
}

/// The 3-way lift / gamma / gain corrector (shadows / midtones / highlights).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LiftGammaGain {
    /// Shadows — a white-pivoting additive offset (raises darks, holds 1.0).
    pub lift: Wheel,
    /// Midtones — a power curve (holds 0 and 1, bends the middle).
    pub gamma: Wheel,
    /// Highlights — a black-pivoting multiply (scales brights, holds 0.0).
    pub gain: Wheel,
}

impl LiftGammaGain {
    pub fn is_identity(&self) -> bool {
        self.lift.is_identity() && self.gamma.is_identity() && self.gain.is_identity()
    }

    /// Apply lift→gamma→gain per channel, in place, on straight-sRGB 0..1.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() {
            return;
        }
        let lift = self.lift.per_channel();
        let gamma = self.gamma.per_channel();
        let gain = self.gain.per_channel();
        for c in 0..3 {
            let mut v = rgb[c];
            // Lift: shadow-weighted offset, pivots on white (v=1 stays 1).
            v += lift[c] * (1.0 - v);
            // Gamma: midtone power, pivots on 0 and 1.
            let g = (1.0 + gamma[c]).max(0.01);
            v = v.max(0.0).powf(1.0 / g);
            // Gain: highlight-weighted multiply, pivots on black (v=0 stays 0).
            v *= (1.0 + gain[c]).max(0.0);
            rgb[c] = v.clamp(0.0, 1.0);
        }
    }
}

// =============================================================================
// Curves (RGB master + per-channel) — control-point monotone-cubic spline → LUT
// =============================================================================

/// Which curve a control-point action targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GradeCurveChannel {
    #[default]
    Master,
    Red,
    Green,
    Blue,
}

fn identity_knots() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

/// Per-channel tone curves. Each is a sorted list of `[x, y]` control points in
/// `0..1`; the identity is `[[0,0],[1,1]]`.
#[derive(Clone, Debug, PartialEq)]
pub struct GradeCurves {
    pub master: Vec<[f32; 2]>,
    pub red: Vec<[f32; 2]>,
    pub green: Vec<[f32; 2]>,
    pub blue: Vec<[f32; 2]>,
}

impl Default for GradeCurves {
    fn default() -> Self {
        Self {
            master: identity_knots(),
            red: identity_knots(),
            green: identity_knots(),
            blue: identity_knots(),
        }
    }
}

impl GradeCurves {
    pub fn is_identity(&self) -> bool {
        let id = identity_knots();
        self.master == id && self.red == id && self.green == id && self.blue == id
    }

    fn channel_mut(&mut self, ch: GradeCurveChannel) -> &mut Vec<[f32; 2]> {
        match ch {
            GradeCurveChannel::Master => &mut self.master,
            GradeCurveChannel::Red => &mut self.red,
            GradeCurveChannel::Green => &mut self.green,
            GradeCurveChannel::Blue => &mut self.blue,
        }
    }

    /// Apply master, then per-channel curves, in place on straight-sRGB 0..1.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() {
            return;
        }
        let per = [&self.red, &self.green, &self.blue];
        for c in 0..3 {
            let v = eval_curve(&self.master, rgb[c]);
            rgb[c] = eval_curve(per[c], v).clamp(0.0, 1.0);
        }
    }
}

/// Evaluate a control-point curve at `x` with **monotone cubic Hermite**
/// (Fritsch–Carlson tangents). Collinear knots — including the identity
/// `[[0,0],[1,1]]` — reduce to an exact straight line, so an identity curve is a
/// true pass-through. Queries outside the knot range clamp to the end values.
pub fn eval_curve(points: &[[f32; 2]], x: f32) -> f32 {
    let n = points.len();
    if n == 0 {
        return x;
    }
    if n == 1 {
        return points[0][1];
    }
    if x <= points[0][0] {
        return points[0][1];
    }
    if x >= points[n - 1][0] {
        return points[n - 1][1];
    }
    // Secant slopes between consecutive knots.
    let mut secant = vec![0.0f32; n - 1];
    for k in 0..n - 1 {
        let dx = (points[k + 1][0] - points[k][0]).max(1e-9);
        secant[k] = (points[k + 1][1] - points[k][1]) / dx;
    }
    // Endpoint + interior tangents, then Fritsch–Carlson monotonicity clamp.
    let mut m = vec![0.0f32; n];
    m[0] = secant[0];
    m[n - 1] = secant[n - 2];
    for k in 1..n - 1 {
        m[k] = if secant[k - 1] * secant[k] <= 0.0 {
            0.0
        } else {
            (secant[k - 1] + secant[k]) / 2.0
        };
    }
    for k in 0..n - 1 {
        if secant[k].abs() < 1e-9 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let alpha = m[k] / secant[k];
        let beta = m[k + 1] / secant[k];
        let s = alpha * alpha + beta * beta;
        if s > 9.0 {
            let tau = 3.0 / s.sqrt();
            m[k] = tau * alpha * secant[k];
            m[k + 1] = tau * beta * secant[k];
        }
    }
    // Locate the segment and evaluate the cubic Hermite basis.
    for k in 0..n - 1 {
        if x <= points[k + 1][0] {
            let h = (points[k + 1][0] - points[k][0]).max(1e-9);
            let t = (x - points[k][0]) / h;
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            return h00 * points[k][1]
                + h10 * h * m[k]
                + h01 * points[k + 1][1]
                + h11 * h * m[k + 1];
        }
    }
    points[n - 1][1]
}

/// Bake a control-point curve into a `size`-entry LUT sampled uniformly over
/// `0..=1` (the "spline → LUT" step). `out[i]` = `eval_curve(points, i/(size-1))`.
pub fn build_channel_lut(points: &[[f32; 2]], size: usize) -> Vec<f32> {
    let size = size.max(2);
    (0..size)
        .map(|i| eval_curve(points, i as f32 / (size - 1) as f32).clamp(0.0, 1.0))
        .collect()
}

// =============================================================================
// HSL secondary qualifier
// =============================================================================

/// An HSL **secondary** qualifier + correction: key a hue/sat/luma range (with a
/// feathered shoulder) and grade only the qualifying pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecondaryQualifier {
    pub enabled: bool,
    /// Hue band centre (turns, `0..1`).
    pub hue_center: f32,
    /// Hue band half-width (turns). `>= 0.5` keys all hues.
    pub hue_width: f32,
    /// Saturation acceptance window `0..1`.
    pub sat_low: f32,
    pub sat_high: f32,
    /// Luma acceptance window `0..1`.
    pub luma_low: f32,
    pub luma_high: f32,
    /// Soft-shoulder width added outside each window edge (`0..1`).
    pub feather: f32,
    // --- correction applied to qualifying pixels (blended by membership q) ---
    /// Hue rotation in turns.
    pub hue_shift: f32,
    /// Saturation multiplier (`1` = none).
    pub sat_scale: f32,
    /// Luma multiplier (`1` = none).
    pub lum_scale: f32,
}

impl Default for SecondaryQualifier {
    fn default() -> Self {
        Self {
            enabled: false,
            hue_center: 0.0,
            hue_width: 0.5,
            sat_low: 0.0,
            sat_high: 1.0,
            luma_low: 0.0,
            luma_high: 1.0,
            feather: 0.05,
            hue_shift: 0.0,
            sat_scale: 1.0,
            lum_scale: 1.0,
        }
    }
}

/// Soft trapezoidal membership of `v` in `[lo, hi]` with `feather` shoulders.
fn band_membership(v: f32, lo: f32, hi: f32, feather: f32) -> f32 {
    let f = feather.max(1e-6);
    if v < lo {
        (1.0 - (lo - v) / f).clamp(0.0, 1.0)
    } else if v > hi {
        (1.0 - (v - hi) / f).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

impl SecondaryQualifier {
    pub fn is_identity(&self) -> bool {
        !self.enabled
            || (self.hue_shift == 0.0 && self.sat_scale == 1.0 && self.lum_scale == 1.0)
    }

    /// Membership `q ∈ 0..1` of a straight-sRGB pixel in the key (1 = fully in).
    pub fn qualify(&self, rgb: &[f32; 3]) -> f32 {
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        // Circular hue distance to the band centre.
        let dist = {
            let d = (h - self.hue_center).rem_euclid(1.0);
            d.min(1.0 - d)
        };
        let hue_q = if self.hue_width >= 0.5 {
            1.0
        } else {
            band_membership(dist, 0.0, self.hue_width, self.feather)
        };
        let sat_q = band_membership(s, self.sat_low, self.sat_high, self.feather);
        let luma_q = band_membership(l, self.luma_low, self.luma_high, self.feather);
        hue_q * sat_q * luma_q
    }

    /// Apply the secondary correction, blended by membership, in place.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        if self.is_identity() {
            return;
        }
        let q = self.qualify(rgb);
        if q <= 0.0 {
            return;
        }
        let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        let new_h = (h + self.hue_shift).rem_euclid(1.0);
        let new_s = (s * self.sat_scale).clamp(0.0, 1.0);
        let new_l = (l * self.lum_scale).clamp(0.0, 1.0);
        let (r2, g2, b2) = hsl_to_rgb(new_h, new_s, new_l);
        rgb[0] = rgb[0] * (1.0 - q) + r2 * q;
        rgb[1] = rgb[1] * (1.0 - q) + g2 * q;
        rgb[2] = rgb[2] * (1.0 - q) + b2 * q;
    }

    fn clamp(&mut self) {
        self.hue_center = self.hue_center.rem_euclid(1.0);
        self.hue_width = self.hue_width.clamp(0.0, 0.5);
        self.sat_low = self.sat_low.clamp(0.0, 1.0);
        self.sat_high = self.sat_high.clamp(0.0, 1.0);
        self.luma_low = self.luma_low.clamp(0.0, 1.0);
        self.luma_high = self.luma_high.clamp(0.0, 1.0);
        self.feather = self.feather.clamp(1e-4, 1.0);
        self.sat_scale = self.sat_scale.max(0.0);
        self.lum_scale = self.lum_scale.max(0.0);
    }
}

// =============================================================================
// The full per-clip grade
// =============================================================================

/// A full Lumetri-style per-clip colour grade. `Default` is a true no-op.
#[derive(Clone, Debug, PartialEq)]
pub struct LumetriGrade {
    /// Exposure in **stops** (applied in linear light).
    pub exposure: f32,
    /// Contrast about mid-grey 0.5 (`0` = none).
    pub contrast: f32,
    /// Saturation multiplier (`1` = none, `0` = greyscale).
    pub saturation: f32,
    /// White-balance temperature (`>0` warmer / redder).
    pub temperature: f32,
    /// White-balance tint (`>0` magenta, `<0` green).
    pub tint: f32,
    /// 3-way lift/gamma/gain wheels.
    pub lgg: LiftGammaGain,
    /// Tone control — pushes the very top end.
    pub whites: f32,
    /// Tone control — pushes the very bottom end.
    pub blacks: f32,
    /// Tone control — pushes the upper range.
    pub highlights: f32,
    /// Tone control — pushes the lower range.
    pub shadows: f32,
    /// RGB master + per-channel curves.
    pub curves: GradeCurves,
    /// HSL secondary qualifier + correction.
    pub secondary: SecondaryQualifier,
}

impl Default for LumetriGrade {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            saturation: 1.0,
            temperature: 0.0,
            tint: 0.0,
            lgg: LiftGammaGain::default(),
            whites: 0.0,
            blacks: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            curves: GradeCurves::default(),
            secondary: SecondaryQualifier::default(),
        }
    }
}

impl LumetriGrade {
    /// True when every stage is at its neutral value (a full no-op).
    pub fn is_identity(&self) -> bool {
        self.exposure == 0.0
            && self.contrast == 0.0
            && self.saturation == 1.0
            && self.temperature == 0.0
            && self.tint == 0.0
            && self.lgg.is_identity()
            && self.whites == 0.0
            && self.blacks == 0.0
            && self.highlights == 0.0
            && self.shadows == 0.0
            && self.curves.is_identity()
            && self.secondary.is_identity()
    }

    /// Apply the full pipeline to a straight-sRGB triple, in place.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        // 1. Exposure — the one linear-light stage.
        if self.exposure != 0.0 {
            let scale = exposure_scale(self.exposure);
            for v in rgb.iter_mut() {
                let lin = (srgb_to_linear(v.clamp(0.0, 1.0)) * scale).clamp(0.0, 1.0);
                *v = linear_to_srgb(lin);
            }
        }
        // 2. Temperature / tint (sRGB channel balance).
        apply_temp_tint(rgb, self.temperature, self.tint);
        // 3. Tone controls (range-weighted pushes).
        apply_tone(rgb, self.shadows, self.highlights, self.blacks, self.whites);
        // 4. Lift / gamma / gain.
        self.lgg.apply(rgb);
        // 5. Contrast about mid-grey.
        if self.contrast != 0.0 {
            let c = 1.0 + self.contrast;
            for v in rgb.iter_mut() {
                *v = ((*v - 0.5) * c + 0.5).clamp(0.0, 1.0);
            }
        }
        // 6. Curves.
        self.curves.apply(rgb);
        // 7. Saturation (lerp around Rec.601 luma).
        if self.saturation != 1.0 {
            let l = luma_of(*rgb);
            for v in rgb.iter_mut() {
                *v = (l + self.saturation * (*v - l)).clamp(0.0, 1.0);
            }
        }
        // 8. Secondary qualifier.
        self.secondary.apply(rgb);
        for v in rgb.iter_mut() {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

/// Linear-light exposure scale for `stops` (`2^stops`). `+1` stop = ×2.
pub fn exposure_scale(stops: f32) -> f32 {
    2.0f32.powf(stops)
}

/// Apply white-balance temperature/tint as an sRGB channel balance, in place.
/// `temp > 0` warms (boosts R, cuts B); `tint > 0` pushes magenta (cuts G).
fn apply_temp_tint(rgb: &mut [f32; 3], temp: f32, tint: f32) {
    if temp == 0.0 && tint == 0.0 {
        return;
    }
    const K: f32 = 0.3;
    rgb[0] = (rgb[0] * (1.0 + K * temp)).clamp(0.0, 1.0);
    rgb[2] = (rgb[2] * (1.0 - K * temp)).clamp(0.0, 1.0);
    rgb[1] = (rgb[1] * (1.0 - K * tint)).clamp(0.0, 1.0);
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Apply the four tone controls (shadows/highlights broad, blacks/whites at the
/// extremes), in place. Each is a range-weighted additive push.
fn apply_tone(rgb: &mut [f32; 3], shadows: f32, highlights: f32, blacks: f32, whites: f32) {
    if shadows == 0.0 && highlights == 0.0 && blacks == 0.0 && whites == 0.0 {
        return;
    }
    for v in rgb.iter_mut() {
        let x = *v;
        let w_shadow = 1.0 - smoothstep(0.0, 0.5, x); // 1 at black → 0 at mid
        let w_high = smoothstep(0.5, 1.0, x); // 0 at mid → 1 at white
        let w_black = 1.0 - smoothstep(0.0, 0.25, x); // bottom quarter
        let w_white = smoothstep(0.75, 1.0, x); // top quarter
        let mut out = x;
        out += 0.5 * shadows * w_shadow;
        out += 0.5 * highlights * w_high;
        out += 0.5 * blacks * w_black;
        out += 0.5 * whites * w_white;
        *v = out.clamp(0.0, 1.0);
    }
}

/// Apply a [`LumetriGrade`] to one straight-sRGB `Rgba` (alpha preserved). The
/// single pure entry point used by both the compositor and the scopes sampler.
pub fn grade_pixel(rgba: [f32; 4], grade: &LumetriGrade) -> [f32; 4] {
    let mut rgb = [rgba[0], rgba[1], rgba[2]];
    grade.apply(&mut rgb);
    [rgb[0], rgb[1], rgb[2], rgba[3]]
}

// =============================================================================
// App integration — actions mutate the per-clip grade map
// =============================================================================

pub trait AppColorGradeExt {
    fn apply_color_grade(&mut self, action: Action);
    /// Mutable access to a clip's grade, creating a default if absent.
    fn clip_grade_mut(&mut self, clip: usize) -> &mut LumetriGrade;
}

impl AppColorGradeExt for App {
    fn clip_grade_mut(&mut self, clip: usize) -> &mut LumetriGrade {
        self.clip_grades.entry(clip).or_default()
    }

    fn apply_color_grade(&mut self, action: Action) {
        match action {
            Action::SetGradeExposure { clip, value } => {
                self.clip_grade_mut(clip).exposure = value.clamp(-6.0, 6.0);
                self.host.mark_dirty();
            }
            Action::SetGradeContrast { clip, value } => {
                self.clip_grade_mut(clip).contrast = value.clamp(-1.0, 2.0);
                self.host.mark_dirty();
            }
            Action::SetGradeSaturation { clip, value } => {
                self.clip_grade_mut(clip).saturation = value.clamp(0.0, 4.0);
                self.host.mark_dirty();
            }
            Action::SetGradeTemperature { clip, value } => {
                self.clip_grade_mut(clip).temperature = value.clamp(-1.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetGradeTint { clip, value } => {
                self.clip_grade_mut(clip).tint = value.clamp(-1.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetGradeWhites { clip, value } => {
                self.clip_grade_mut(clip).whites = value.clamp(-1.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetGradeBlacks { clip, value } => {
                self.clip_grade_mut(clip).blacks = value.clamp(-1.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetGradeHighlights { clip, value } => {
                self.clip_grade_mut(clip).highlights = value.clamp(-1.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetGradeShadows { clip, value } => {
                self.clip_grade_mut(clip).shadows = value.clamp(-1.0, 1.0);
                self.host.mark_dirty();
            }
            Action::SetGradeWheel { clip, which, rgb, master } => {
                let grade = self.clip_grade_mut(clip);
                let wheel = match which {
                    WheelKind::Lift => &mut grade.lgg.lift,
                    WheelKind::Gamma => &mut grade.lgg.gamma,
                    WheelKind::Gain => &mut grade.lgg.gain,
                };
                wheel.rgb = rgb;
                wheel.master = master;
                wheel.clamp();
                self.host.mark_dirty();
            }
            Action::AddGradeCurvePoint { clip, channel, point } => {
                let pt = [point[0].clamp(0.0, 1.0), point[1].clamp(0.0, 1.0)];
                let curve = self.clip_grade_mut(clip).curves.channel_mut(channel);
                curve.push(pt);
                curve.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
                self.host.mark_dirty();
            }
            Action::SetGradeSecondary { clip, mut qualifier } => {
                qualifier.clamp();
                self.clip_grade_mut(clip).secondary = qualifier;
                self.host.mark_dirty();
            }
            Action::ResetClipGrade { clip } => {
                self.clip_grades.remove(&clip);
                self.host.mark_dirty();
            }
            _ => {}
        }
    }
}

/// Per-clip grade storage type aliased for the `App` field.
pub type ClipGrades = HashMap<usize, LumetriGrade>;

#[cfg(test)]
mod tests;
