//! Phase 6 — Retouching core: healing brush, clone stamp, red-eye, content-aware
//! patch. Real pixel algorithms operating on the host's **linear-premultiplied
//! RGBA f32** layer buffer (the format `CanvasHost::read_layer_f32` returns).
//!
//! Everything here is a *pure function* (no `App`, no GPU) so it is exhaustively
//! unit-testable and deterministic, plus thin `apply_*` helpers on `impl App`
//! that follow the established read → transform → upload idiom from
//! `filters_advanced.rs` (`read_layer_f32` → pure fn → `upload_layer_f32`), with
//! a pre-write `snapshot_layer` so each op is one region-COW undo step.
//!
//! These deliberately live in Pigment (not the shared crates): they are the
//! raster-editor retouch surface, and the PLAN §0a rule keeps app-specific glue
//! out of `prism-core`.

use super::{Action, App};

/// Default Gauss–Seidel sweeps for the healing-brush Poisson solve. Enough for
/// the membrane to converge on a brush-sized region without a noticeable stall.
const HEAL_ITERATIONS: usize = 200;

// ---------------------------------------------------------------------------
// Healing brush — gradient-domain (Poisson) seamless clone
// ---------------------------------------------------------------------------

/// Solve a Poisson image-editing problem (Pérez et al. 2003) by Gauss–Seidel.
///
/// `dest`/`src` are linear-premultiplied RGBA f32 (`w*h*4`); `mask` is `w*h`
/// (`true` = solve here). `src` is already aligned to destination coordinates:
/// `src[p]` is the source value intended to land at `p`.
///
/// For every masked pixel the result is solved so its Laplacian matches the
/// source's gradient field, with the **destination imposed as a Dirichlet
/// boundary** — a membrane that smoothly carries the boundary tone offset across
/// the region, so a transplanted patch matches the surrounding tone instead of
/// pasting a hard edge. RGB is healed; alpha is taken from `dest` unchanged.
/// Iteration order is fixed → bit-for-bit deterministic for identical input.
pub fn poisson_blend(
    dest: &[f32],
    src: &[f32],
    mask: &[bool],
    w: u32,
    h: u32,
    iterations: usize,
) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    let mut out = dest.to_vec();
    if wi == 0 || hi == 0 {
        return out;
    }
    assert_eq!(dest.len(), wi * hi * 4, "dest must be w*h*4");
    assert_eq!(src.len(), wi * hi * 4, "src must be w*h*4");
    assert_eq!(mask.len(), wi * hi, "mask must be w*h");

    let Some((x0, y0, x1, y1)) = mask_bbox(mask, wi, hi) else {
        return out;
    };

    // Seed masked pixels with the (aligned) source value — converges faster than
    // starting from the destination.
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = y * wi + x;
            if mask[i] {
                out[i * 4] = src[i * 4];
                out[i * 4 + 1] = src[i * 4 + 1];
                out[i * 4 + 2] = src[i * 4 + 2];
            }
        }
    }

    // f(p) = ( Σ_q b(q) + Σ_q (src(p) − src(q)) ) / |N(p)|,
    // b(q) = f(q) if q masked else dest(q); N(p) = in-bounds 4-neighborhood.
    for _ in 0..iterations {
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = y * wi + x;
                if !mask[i] {
                    continue;
                }
                let neighbors = [
                    if x > 0 { Some(i - 1) } else { None },
                    if x + 1 < wi { Some(i + 1) } else { None },
                    if y > 0 { Some(i - wi) } else { None },
                    if y + 1 < hi { Some(i + wi) } else { None },
                ];
                for c in 0..3 {
                    let sp = src[i * 4 + c];
                    let mut sum = 0.0f32;
                    let mut n = 0.0f32;
                    for nb in neighbors.into_iter().flatten() {
                        let bq = if mask[nb] {
                            out[nb * 4 + c]
                        } else {
                            dest[nb * 4 + c]
                        };
                        sum += bq + (sp - src[nb * 4 + c]);
                        n += 1.0;
                    }
                    if n > 0.0 {
                        out[i * 4 + c] = sum / n;
                    }
                }
            }
        }
    }
    out
}

/// Healing brush: transplant the texture sampled around `src_center` into a
/// circular region at `dst_center`, gradient-domain blended so the boundary tone
/// matches the destination. Pure: returns a new buffer, input untouched.
///
/// The source is shifted by the (rounded) `dst_center − src_center` offset across
/// the whole image (edge-clamped) so the Poisson guidance has valid gradients at
/// the region boundary; the circular mask is solved by [`poisson_blend`].
pub fn heal_region(
    px: &[f32],
    w: u32,
    h: u32,
    src_center: [f32; 2],
    dst_center: [f32; 2],
    radius: f32,
    iterations: usize,
) -> Vec<f32> {
    let r = radius.max(0.0);
    let (wi, hi) = (w as usize, h as usize);
    if r < 0.5 || wi == 0 || hi == 0 {
        return px.to_vec();
    }
    let off_x = (dst_center[0] - src_center[0]).round() as i64;
    let off_y = (dst_center[1] - src_center[1]).round() as i64;
    if off_x == 0 && off_y == 0 {
        // Source == destination → nothing to transplant.
        return px.to_vec();
    }

    let r2 = r * r;
    let (dcx, dcy) = (dst_center[0], dst_center[1]);
    let mut mask = vec![false; wi * hi];
    for y in 0..hi {
        for x in 0..wi {
            let dx = x as f32 + 0.5 - dcx;
            let dy = y as f32 + 0.5 - dcy;
            if dx * dx + dy * dy <= r2 {
                mask[y * wi + x] = true;
            }
        }
    }

    // Aligned source over the whole image: dest pixel p samples src at p − off.
    let mut src = px.to_vec();
    for y in 0..hi {
        for x in 0..wi {
            let sx = (x as i64 - off_x).clamp(0, wi as i64 - 1) as usize;
            let sy = (y as i64 - off_y).clamp(0, hi as i64 - 1) as usize;
            let (p, sp) = (y * wi + x, sy * wi + sx);
            for c in 0..4 {
                src[p * 4 + c] = px[sp * 4 + c];
            }
        }
    }
    poisson_blend(px, &src, &mask, w, h, iterations)
}

// ---------------------------------------------------------------------------
// Clone stamp — sample-offset copy with a soft round brush
// ---------------------------------------------------------------------------

/// Clone-stamp one dab. For every pixel inside the circular brush at
/// `dst_center`, sample the source buffer at `pixel − offset` (bilinear, so a
/// fractional offset is smooth), and blend it over the destination by
/// `coverage × opacity` where `coverage` is the soft round falloff (`hardness`
/// = the fraction of the radius that stays fully opaque). Pure: returns a new
/// buffer.
///
/// `dst` and `src` may alias the same slice (clone from a frozen snapshot of the
/// layer); the result is written to a fresh buffer so reads stay consistent.
#[allow(clippy::too_many_arguments)]
pub fn clone_stamp(
    dst: &[f32],
    src: &[f32],
    w: u32,
    h: u32,
    offset: [f32; 2],
    dst_center: [f32; 2],
    radius: f32,
    hardness: f32,
    opacity: f32,
) -> Vec<f32> {
    let r = radius.max(0.0);
    let (wi, hi) = (w as usize, h as usize);
    let mut out = dst.to_vec();
    if r < 0.5 || wi == 0 || hi == 0 || opacity <= 0.0 {
        return out;
    }
    let op = opacity.clamp(0.0, 1.0);
    let r2 = r * r;
    let (dcx, dcy) = (dst_center[0], dst_center[1]);
    let x0 = ((dcx - r).floor() as i64).max(0);
    let x1 = ((dcx + r).ceil() as i64).min(wi as i64 - 1);
    let y0 = ((dcy - r).floor() as i64).max(0);
    let y1 = ((dcy + r).ceil() as i64).min(hi as i64 - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let ddx = x as f32 + 0.5 - dcx;
            let ddy = y as f32 + 0.5 - dcy;
            let d2 = ddx * ddx + ddy * ddy;
            if d2 > r2 {
                continue;
            }
            let cov = brush_coverage(d2.sqrt(), r, hardness);
            if cov <= 0.0 {
                continue;
            }
            let a = cov * op;
            let sx = x as f32 - offset[0];
            let sy = y as f32 - offset[1];
            let s = bilinear_sample(src, wi, hi, sx, sy);
            let i = (y as usize * wi + x as usize) * 4;
            for c in 0..4 {
                out[i + c] = out[i + c] * (1.0 - a) + s[c] * a;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Red-eye removal
// ---------------------------------------------------------------------------

/// Red-eye correction over a circular region. A pixel is treated as a flash-lit
/// pupil only when the (unpremultiplied) red channel **clearly dominates**: its
/// share of `r+g+b` exceeds half and it leads `max(g,b)` by a margin. Such
/// pixels are desaturated toward their own luma and darkened, scaled by
/// `strength` (0..1); everything else — neutral pixels, skin (where red leads
/// only mildly) — is left bit-for-bit untouched. Pure: returns a new buffer.
pub fn red_eye_correct(
    px: &[f32],
    w: u32,
    h: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    strength: f32,
) -> Vec<f32> {
    let s = strength.clamp(0.0, 1.0);
    let r = radius.max(0.0);
    let (wi, hi) = (w as usize, h as usize);
    let mut out = px.to_vec();
    if r < 0.5 || s <= 0.0 || wi == 0 || hi == 0 {
        return out;
    }
    let r2 = r * r;
    let x0 = ((cx - r).floor() as i64).max(0);
    let x1 = ((cx + r).ceil() as i64).min(wi as i64 - 1);
    let y0 = ((cy - r).floor() as i64).max(0);
    let y1 = ((cy + r).ceil() as i64).min(hi as i64 - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let ddx = x as f32 + 0.5 - cx;
            let ddy = y as f32 + 0.5 - cy;
            if ddx * ddx + ddy * ddy > r2 {
                continue;
            }
            let i = (y as usize * wi + x as usize) * 4;
            let a = px[i + 3];
            if a <= 1e-6 {
                continue;
            }
            let rr = px[i] / a;
            let gg = px[i + 1] / a;
            let bb = px[i + 2] / a;
            let mx = gg.max(bb);
            let ratio = rr / (rr + gg + bb + 1e-6);
            // Pupil signature: red owns the majority of the energy AND leads the
            // others clearly. Skin (r leads only mildly) fails the ratio gate.
            let is_pupil = ratio > 0.5 && (rr - mx) > 0.2 && rr > 0.3;
            if !is_pupil {
                continue;
            }
            let luma = 0.2126 * rr + 0.7152 * gg + 0.0722 * bb;
            let nr = (rr + (luma - rr) * s) * (1.0 - 0.5 * s);
            let ng = gg + (luma - gg) * s;
            let nb = bb + (luma - bb) * s;
            out[i] = nr * a;
            out[i + 1] = ng * a;
            out[i + 2] = nb * a;
            out[i + 3] = a;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Content-aware patch fill — seeded PatchMatch-lite (greedy exemplar onion-peel)
// ---------------------------------------------------------------------------

/// A tiny seeded PRNG (SplitMix64-style). Local only — the suite forbids a
/// global rng so content-aware fill stays deterministic per `seed`.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) & 0xFFFF_FFFF) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next_u32() % n
        }
    }
}

/// Content-aware fill of a circular region by greedy exemplar synthesis
/// (PatchMatch-lite): masked pixels are filled outer→inner, each from the
/// best-matching `5×5` exemplar patch drawn from the surrounding (unmasked)
/// texture — comparing only the already-known pixels in the target window, so
/// the synthesized region continues the local texture rather than a flat color.
///
/// `seed` drives candidate sampling on large images (small ones search every
/// candidate exhaustively); identical `seed` ⇒ identical output. Pure.
pub fn patch_fill(px: &[f32], w: u32, h: u32, cx: f32, cy: f32, radius: f32, seed: u64) -> Vec<f32> {
    let r = radius.max(0.0);
    let (wi, hi) = (w as usize, h as usize);
    let mut out = px.to_vec();
    if r < 0.5 || wi == 0 || hi == 0 {
        return out;
    }
    let r2 = r * r;
    let mut mask = vec![false; wi * hi];
    let mut masked: Vec<usize> = Vec::new();
    for y in 0..hi {
        for x in 0..wi {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                mask[y * wi + x] = true;
                masked.push(y * wi + x);
            }
        }
    }
    if masked.is_empty() {
        return out;
    }
    let candidates: Vec<usize> = (0..wi * hi).filter(|&i| !mask[i]).collect();
    if candidates.is_empty() {
        return out;
    }

    // Fill outermost masked pixels first (most known context), inward.
    masked.sort_by(|&a, &b| {
        let da = dist2_to_center(a, wi, cx, cy);
        let db = dist2_to_center(b, wi, cx, cy);
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rng = Lcg::new(seed);
    // Exhaustive for small images (deterministic, seed-independent); seeded
    // sample for large ones (still deterministic per seed, no global rng).
    let sample: Vec<usize> = if candidates.len() <= 256 {
        candidates.clone()
    } else {
        (0..256)
            .map(|_| candidates[rng.below(candidates.len() as u32) as usize])
            .collect()
    };

    let mut known: Vec<bool> = mask.iter().map(|&m| !m).collect();
    let pr: i64 = 2; // 5×5 patch
    for &p in &masked {
        let px_x = (p % wi) as i64;
        let px_y = (p / wi) as i64;
        let mut best = f32::INFINITY;
        let mut best_s = sample[0];
        for &cand in &sample {
            let sx = (cand % wi) as i64;
            let sy = (cand / wi) as i64;
            let mut score = 0.0f32;
            let mut n = 0.0f32;
            for dy in -pr..=pr {
                for dx in -pr..=pr {
                    let qx = px_x + dx;
                    let qy = px_y + dy;
                    if qx < 0 || qy < 0 || qx >= wi as i64 || qy >= hi as i64 {
                        continue;
                    }
                    let q = (qy * wi as i64 + qx) as usize;
                    if !known[q] {
                        continue;
                    }
                    let tx = sx + dx;
                    let ty = sy + dy;
                    if tx < 0 || ty < 0 || tx >= wi as i64 || ty >= hi as i64 {
                        continue;
                    }
                    let t = (ty * wi as i64 + tx) as usize;
                    if mask[t] {
                        continue; // exemplar must come from original known texture
                    }
                    for c in 0..4 {
                        let d = out[q * 4 + c] - px[t * 4 + c];
                        score += d * d;
                    }
                    n += 1.0;
                }
            }
            let score = if n > 0.0 { score / n } else { f32::INFINITY };
            if score < best {
                best = score;
                best_s = cand;
            }
        }
        for c in 0..4 {
            out[p * 4 + c] = px[best_s * 4 + c];
        }
        known[p] = true;
    }
    out
}

// ---------------------------------------------------------------------------
// Shared pure helpers
// ---------------------------------------------------------------------------

/// Inclusive bounding box `(x0, y0, x1, y1)` of the `true` pixels, or `None`.
fn mask_bbox(mask: &[bool], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    any.then_some((x0, y0, x1, y1))
}

/// Soft round-brush coverage at distance `d` (px) from the dab center. `hardness`
/// is the fraction of the radius that stays fully opaque; beyond it the coverage
/// smoothsteps to 0 at the rim. Returns 0 outside the radius.
pub(crate) fn brush_coverage(d: f32, radius: f32, hardness: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    let dn = d / radius;
    if dn >= 1.0 {
        return 0.0;
    }
    let hh = hardness.clamp(0.0, 0.999);
    if dn <= hh {
        return 1.0;
    }
    let t = (dn - hh) / (1.0 - hh); // 0..1 across the soft ring
    let s = t * t * (3.0 - 2.0 * t); // smoothstep
    1.0 - s
}

/// Bilinear sample of a premultiplied RGBA f32 buffer at continuous pixel-index
/// coordinates `(fx, fy)`, edge-clamped. (Premultiplied → linear interpolation
/// of the stored channels is correct.)
pub(crate) fn bilinear_sample(buf: &[f32], w: usize, h: usize, fx: f32, fy: f32) -> [f32; 4] {
    let cx = fx.clamp(0.0, w as f32 - 1.0);
    let cy = fy.clamp(0.0, h as f32 - 1.0);
    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = cx - x0 as f32;
    let ty = cy - y0 as f32;
    let i00 = (y0 * w + x0) * 4;
    let i10 = (y0 * w + x1) * 4;
    let i01 = (y1 * w + x0) * 4;
    let i11 = (y1 * w + x1) * 4;
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        let top = buf[i00 + c] * (1.0 - tx) + buf[i10 + c] * tx;
        let bot = buf[i01 + c] * (1.0 - tx) + buf[i11 + c] * tx;
        out[c] = top * (1.0 - ty) + bot * ty;
    }
    out
}

/// Squared distance from pixel `i`'s center to `(cx, cy)`.
fn dist2_to_center(i: usize, w: usize, cx: f32, cy: f32) -> f32 {
    let x = (i % w) as f32 + 0.5;
    let y = (i / w) as f32 + 0.5;
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy
}

// ---------------------------------------------------------------------------
// App apply helpers — read layer → pure fn → snapshot → upload
// ---------------------------------------------------------------------------

impl App {
    pub(super) fn apply_healing(&mut self, action: Action) {
        match action {
            Action::HealBrush {
                src_center,
                dst_center,
                radius,
            } => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let out =
                        heal_region(&px, dw, dh, src_center, dst_center, radius, HEAL_ITERATIONS);
                    self.host.snapshot_layer(layer, "Healing Brush");
                    self.host.upload_layer_f32(layer, &out);
                    self.status_message = Some("Healing brush applied".into());
                }
            }
            Action::CloneStampDab {
                src_center,
                dst_center,
                radius,
                opacity,
            } => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let offset = [dst_center[0] - src_center[0], dst_center[1] - src_center[1]];
                    let out = clone_stamp(
                        &px,
                        &px,
                        dw,
                        dh,
                        offset,
                        dst_center,
                        radius,
                        self.brush.hardness,
                        opacity,
                    );
                    self.host.snapshot_layer(layer, "Clone Stamp");
                    self.host.upload_layer_f32(layer, &out);
                    self.status_message = Some("Clone stamp".into());
                }
            }
            Action::RemoveRedEye {
                center,
                radius,
                strength,
            } => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let out = red_eye_correct(&px, dw, dh, center[0], center[1], radius, strength);
                    self.host.snapshot_layer(layer, "Red-Eye Removal");
                    self.host.upload_layer_f32(layer, &out);
                    self.status_message = Some("Red-eye removed".into());
                }
            }
            Action::ContentAwarePatch {
                center,
                radius,
                seed,
            } => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let out = patch_fill(&px, dw, dh, center[0], center[1], radius, seed);
                    self.host.snapshot_layer(layer, "Content-Aware Patch");
                    self.host.upload_layer_f32(layer, &out);
                    self.status_message = Some("Content-aware patch".into());
                }
            }
            _ => {}
        }
    }
}
