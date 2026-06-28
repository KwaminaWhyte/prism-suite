//! Phase 6 — Liquify: a real forward-warp **mesh deformation**.
//!
//! A 2-D displacement field ([`WarpField`]) is stored independently of the image
//! (one `[dx, dy]` per pixel). Brush tools push displacement into the field within
//! a radius with a smooth falloff:
//!
//! * [`push`]      — forward warp: drag vector displaces pixels along the drag.
//! * [`bloat_pucker`] — radial: `amount > 0` bloats (outward), `< 0` puckers (in).
//! * [`twirl`]     — rotate pixels around the brush center, falloff-scaled.
//! * [`reconstruct`] — decay the field back toward identity within the radius.
//!
//! The field is applied to a pixel buffer by **inverse sampling** ([`apply_warp`]):
//! every destination pixel `d` reads the source at `d − field[d]` (bilinear, edge-
//! clamped), so the warp has no holes and never reads out of bounds. Because the
//! field is independent of the image, a Liquify session is fully non-destructive
//! until committed — the displayed layer is just `apply_warp(base, field)`, and
//! [`App::apply_liquify`] commits it as a single undo step (or resets to `base`).
//!
//! Everything in the upper half of this file is a *pure function* (no `App`, no
//! GPU) so it is exhaustively unit-testable and deterministic. The `impl App`
//! helpers follow the established read → transform → upload idiom from
//! `healing.rs` (`read_layer_f32` → pure fn → `snapshot_layer` → `upload_layer_f32`).
//!
//! Like `healing.rs`, this deliberately lives in Pigment, not the shared crates:
//! it is the raster-editor warp surface, and PLAN §0a keeps app glue out of
//! `prism-core`. The bilinear sampler is reused from `healing.rs` so the two
//! retouch surfaces share one (tested) edge-clamped resampler.

use super::healing::bilinear_sample;
use super::{Action, App};
use prism_core::LayerId;

// ---------------------------------------------------------------------------
// Warp field — the per-pixel displacement mesh
// ---------------------------------------------------------------------------

/// A 2-D per-pixel forward-warp displacement field, row-major, length `w*h`.
///
/// `disp[y*w + x] = [dx, dy]` is the displacement (in pixels) accumulated at that
/// pixel by the Liquify tools. Identity (all-zero) ⇒ [`apply_warp`] is a no-op
/// copy. The field is image-independent, so it can be edited, previewed, reset, or
/// committed without touching the source pixels until the user commits.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpField {
    pub w: u32,
    pub h: u32,
    /// Row-major per-pixel displacement `[dx, dy]` in pixels; `disp[y*w + x]`.
    pub disp: Vec<[f32; 2]>,
}

impl WarpField {
    /// A zero (identity) field of size `w×h`.
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            disp: vec![[0.0, 0.0]; (w as usize) * (h as usize)],
        }
    }

    /// True when every displacement is exactly zero (the warp is a no-op).
    pub fn is_identity(&self) -> bool {
        self.disp.iter().all(|d| d[0] == 0.0 && d[1] == 0.0)
    }

    /// The largest displacement magnitude in the field (0 for identity). Used by
    /// tests and the reconstruct decay to gauge how far the mesh deviates.
    pub fn max_magnitude(&self) -> f32 {
        self.disp
            .iter()
            .fold(0.0f32, |m, d| m.max((d[0] * d[0] + d[1] * d[1]).sqrt()))
    }
}

// ---------------------------------------------------------------------------
// Falloff + bounding box
// ---------------------------------------------------------------------------

/// Smooth radial brush falloff: `1` at the center, smoothstepped down to exactly
/// `0` at (and beyond) `radius`. Returns 0 for a non-positive radius. This is the
/// weight every tool multiplies its displacement by, so edits taper to nothing at
/// the brush rim (no hard discontinuity in the mesh).
pub fn falloff(d: f32, radius: f32) -> f32 {
    if radius <= 0.0 || d >= radius {
        return 0.0;
    }
    let t = 1.0 - (d / radius).clamp(0.0, 1.0); // 1 at center → 0 at rim
    t * t * (3.0 - 2.0 * t) // smoothstep
}

/// Inclusive pixel bounding box `(x0, y0, x1, y1)` of the brush at `center` with
/// `radius`, clamped to `[0, w-1] × [0, h-1]`. Pixels at the clamped boundary that
/// fall outside the radius get a zero falloff weight, so they are skipped — the
/// clamp only avoids out-of-range indexing, it never widens the affected region.
fn bbox(w: usize, h: usize, center: [f32; 2], r: f32) -> (usize, usize, usize, usize) {
    let mx = (w - 1) as f32;
    let my = (h - 1) as f32;
    let x0 = (center[0] - r).floor().clamp(0.0, mx) as usize;
    let y0 = (center[1] - r).floor().clamp(0.0, my) as usize;
    let x1 = (center[0] + r).ceil().clamp(0.0, mx) as usize;
    let y1 = (center[1] + r).ceil().clamp(0.0, my) as usize;
    (x0, y0, x1, y1)
}

// ---------------------------------------------------------------------------
// Pure field-mutation tools
// ---------------------------------------------------------------------------

/// Forward warp ("push"): add `drag * strength`, falloff-weighted, to the
/// displacement of every pixel within `radius` of `center`. With inverse sampling
/// this drags pixels along the `drag` direction. Accumulative — repeated calls
/// build up the warp. Mutates `field` in place; pixels outside the radius are
/// left untouched.
pub fn push(field: &mut WarpField, center: [f32; 2], radius: f32, drag: [f32; 2], strength: f32) {
    let (w, h) = (field.w as usize, field.h as usize);
    if w == 0 || h == 0 || radius <= 0.0 {
        return;
    }
    let (x0, y0, x1, y1) = bbox(w, h, center, radius);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - center[0];
            let dy = y as f32 + 0.5 - center[1];
            let wgt = falloff((dx * dx + dy * dy).sqrt(), radius) * strength;
            if wgt == 0.0 {
                continue;
            }
            let p = y * w + x;
            field.disp[p][0] += drag[0] * wgt;
            field.disp[p][1] += drag[1] * wgt;
        }
    }
}

/// Bloat (`amount > 0`, push pixels radially outward from `center`) / pucker
/// (`amount < 0`, pull inward), falloff-weighted, within `radius`. The added
/// displacement is `(p − center) * amount * falloff`, so under inverse sampling
/// bloat magnifies the region and pucker shrinks it. Mutates `field` in place.
pub fn bloat_pucker(field: &mut WarpField, center: [f32; 2], radius: f32, amount: f32) {
    let (w, h) = (field.w as usize, field.h as usize);
    if w == 0 || h == 0 || radius <= 0.0 || amount == 0.0 {
        return;
    }
    let (x0, y0, x1, y1) = bbox(w, h, center, radius);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let rx = x as f32 + 0.5 - center[0];
            let ry = y as f32 + 0.5 - center[1];
            let wgt = falloff((rx * rx + ry * ry).sqrt(), radius);
            if wgt == 0.0 {
                continue;
            }
            let k = amount * wgt;
            let p = y * w + x;
            field.disp[p][0] += rx * k;
            field.disp[p][1] += ry * k;
        }
    }
}

/// Twirl: rotate content around `center` by `angle` radians (scaled by falloff),
/// counter-clockwise when `ccw` (clockwise otherwise). The displacement added is
/// `(I − R(−φ))·(p − center)` with `φ = ±angle·falloff`, the exact field that makes
/// inverse sampling spin the region: a feature at `q` lands at `center + R(φ)(q −
/// center)`. Mutates `field` in place.
pub fn twirl(field: &mut WarpField, center: [f32; 2], radius: f32, angle: f32, ccw: bool) {
    let (w, h) = (field.w as usize, field.h as usize);
    if w == 0 || h == 0 || radius <= 0.0 || angle == 0.0 {
        return;
    }
    let signed = if ccw { angle } else { -angle };
    let (x0, y0, x1, y1) = bbox(w, h, center, radius);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let rx = x as f32 + 0.5 - center[0];
            let ry = y as f32 + 0.5 - center[1];
            let wgt = falloff((rx * rx + ry * ry).sqrt(), radius);
            if wgt == 0.0 {
                continue;
            }
            let phi = signed * wgt;
            let (sp, cp) = phi.sin_cos();
            // R(-phi) * rel:  (cp*rx + sp*ry, -sp*rx + cp*ry)
            let bx = cp * rx + sp * ry;
            let by = -sp * rx + cp * ry;
            let p = y * w + x;
            field.disp[p][0] += rx - bx;
            field.disp[p][1] += ry - by;
        }
    }
}

/// Reconstruct: decay the field back toward identity within `radius`, scaling each
/// displacement by `(1 − amount·falloff)` (clamped to `≥ 0`). `amount` in `0..=1`;
/// `1` at the center fully clears it. Repeated calls drive the mesh toward zero.
/// Mutates `field` in place.
pub fn reconstruct(field: &mut WarpField, center: [f32; 2], radius: f32, amount: f32) {
    let (w, h) = (field.w as usize, field.h as usize);
    if w == 0 || h == 0 || radius <= 0.0 || amount <= 0.0 {
        return;
    }
    let (x0, y0, x1, y1) = bbox(w, h, center, radius);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let rx = x as f32 + 0.5 - center[0];
            let ry = y as f32 + 0.5 - center[1];
            let wgt = falloff((rx * rx + ry * ry).sqrt(), radius);
            if wgt == 0.0 {
                continue;
            }
            let scale = (1.0 - amount * wgt).clamp(0.0, 1.0);
            let p = y * w + x;
            field.disp[p][0] *= scale;
            field.disp[p][1] *= scale;
        }
    }
}

// ---------------------------------------------------------------------------
// Apply — inverse-sampled warp
// ---------------------------------------------------------------------------

/// Apply `field` to `src` (linear-premultiplied RGBA f32, length `w*h*4`) by
/// **inverse sampling**: every destination pixel `d` reads `src` at `d − field[d]`
/// (bilinear, edge-clamped), so the result has no holes and never reads out of
/// bounds. An identity field returns an exact copy (the mesh round-trip). Pure:
/// returns a fresh buffer, `src` untouched. On a size mismatch it returns a copy.
pub fn apply_warp(src: &[f32], field: &WarpField) -> Vec<f32> {
    let (w, h) = (field.w as usize, field.h as usize);
    if w == 0 || h == 0 || src.len() != w * h * 4 {
        return src.to_vec();
    }
    let mut out = vec![0.0f32; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let p = y * w + x;
            let d = field.disp[p];
            let s = bilinear_sample(src, w, h, x as f32 - d[0], y as f32 - d[1]);
            let o = p * 4;
            out[o] = s[0];
            out[o + 1] = s[1];
            out[o + 2] = s[2];
            out[o + 3] = s[3];
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Non-destructive Liquify session
// ---------------------------------------------------------------------------

/// An in-progress, non-destructive Liquify session: the original (frozen) layer
/// pixels plus the editable warp field. The displayed layer is always
/// `apply_warp(base, field)`. Committing bakes that as one undo step; resetting
/// restores `base` exactly (the mesh round-trip). Held in `App::liquify_session`.
pub struct LiquifySession {
    pub layer: LayerId,
    /// Frozen original pixels (linear-premultiplied RGBA f32) at session start.
    pub base: Vec<f32>,
    /// The editable warp mesh applied over `base` for the live preview.
    pub field: WarpField,
}

// ---------------------------------------------------------------------------
// App apply helpers — read layer → pure field mutation → preview / commit
// ---------------------------------------------------------------------------

impl App {
    /// Ensure a Liquify session exists for the current paint target, freezing the
    /// base pixels and snapshotting the layer (the single undo step for the whole
    /// session). Returns `false` when there is no readable target (e.g. headless
    /// tests with no GPU adapter), in which case the caller no-ops.
    fn ensure_liquify_session(&mut self) -> bool {
        if self.liquify_session.is_some() {
            return true;
        }
        let Some(layer) = self.paint_target() else {
            return false;
        };
        let (w, h) = (self.host.doc_w, self.host.doc_h);
        let Some(base) = self.host.read_layer_f32(layer) else {
            return false;
        };
        // Snapshot the untouched layer now → committing the warp is one undo step.
        self.host.snapshot_layer(layer, "Liquify");
        self.liquify_session = Some(LiquifySession {
            layer,
            base,
            field: WarpField::new(w, h),
        });
        true
    }

    /// Re-render the session's field over its frozen base onto the layer (the live,
    /// non-destructive preview). No snapshot — the session start already took one.
    fn refresh_liquify_preview(&mut self) {
        let Some(s) = self.liquify_session.as_ref() else {
            return;
        };
        let layer = s.layer;
        let out = apply_warp(&s.base, &s.field);
        self.host.upload_layer_f32(layer, &out);
    }

    pub(super) fn apply_liquify(&mut self, action: Action) {
        match action {
            Action::LiquifyPush {
                center,
                radius,
                drag,
                strength,
            } => {
                if !self.ensure_liquify_session() {
                    return;
                }
                let mut mag = 0.0;
                if let Some(s) = self.liquify_session.as_mut() {
                    push(&mut s.field, center, radius, drag, strength.clamp(0.0, 1.0));
                    mag = s.field.max_magnitude();
                }
                self.refresh_liquify_preview();
                self.status_message = Some(format!("Liquify: push (warp {mag:.1}px)"));
            }
            Action::LiquifyBloat {
                center,
                radius,
                amount,
            } => {
                if !self.ensure_liquify_session() {
                    return;
                }
                let mut mag = 0.0;
                if let Some(s) = self.liquify_session.as_mut() {
                    bloat_pucker(&mut s.field, center, radius, amount.abs());
                    mag = s.field.max_magnitude();
                }
                self.refresh_liquify_preview();
                self.status_message = Some(format!("Liquify: bloat (warp {mag:.1}px)"));
            }
            Action::LiquifyPucker {
                center,
                radius,
                amount,
            } => {
                if !self.ensure_liquify_session() {
                    return;
                }
                let mut mag = 0.0;
                if let Some(s) = self.liquify_session.as_mut() {
                    bloat_pucker(&mut s.field, center, radius, -amount.abs());
                    mag = s.field.max_magnitude();
                }
                self.refresh_liquify_preview();
                self.status_message = Some(format!("Liquify: pucker (warp {mag:.1}px)"));
            }
            Action::LiquifyTwirl {
                center,
                radius,
                angle,
                ccw,
            } => {
                if !self.ensure_liquify_session() {
                    return;
                }
                let mut mag = 0.0;
                if let Some(s) = self.liquify_session.as_mut() {
                    twirl(&mut s.field, center, radius, angle, ccw);
                    mag = s.field.max_magnitude();
                }
                self.refresh_liquify_preview();
                self.status_message = Some(format!("Liquify: twirl (warp {mag:.1}px)"));
            }
            Action::LiquifyReconstruct {
                center,
                radius,
                amount,
            } => {
                // Only meaningful inside an active session; otherwise a no-op.
                let mut mag = 0.0;
                if let Some(s) = self.liquify_session.as_mut() {
                    reconstruct(&mut s.field, center, radius, amount.clamp(0.0, 1.0));
                    mag = s.field.max_magnitude();
                }
                self.refresh_liquify_preview();
                self.status_message = Some(format!("Liquify: reconstruct (warp {mag:.1}px)"));
            }
            Action::LiquifyCommit => {
                // The preview pixels are already on the layer and the session-start
                // snapshot captured the original → just end the session.
                if let Some(s) = self.liquify_session.take() {
                    self.status_message = Some(
                        if s.field.is_identity() {
                            "Liquify committed (no change)"
                        } else {
                            "Liquify committed"
                        }
                        .into(),
                    );
                }
            }
            Action::LiquifyReset => {
                if let Some(s) = self.liquify_session.take() {
                    let (layer, base) = (s.layer, s.base);
                    self.host.upload_layer_f32(layer, &base);
                    self.status_message = Some("Liquify reset".into());
                }
            }
            _ => {}
        }
    }
}
