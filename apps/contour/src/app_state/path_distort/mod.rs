//! The **Distort & Transform** family (Illustrator's `Effect ▸ Distort &
//! Transform`) plus **Offset Path**, implemented as real, deterministic,
//! pure-geometry transforms on a path's editable `(points, handles, closed)`
//! triple — the exact model that backs [`Shape::Path`] / [`SubPath`].
//!
//! Every transform here is a free function with no UI / RNG-global state:
//!
//! - [`offset_path`] — parallel-offset a closed outline outward / inward with a
//!   miter / round / bevel corner join (open paths get a simple normal offset).
//! - [`roughen`] — subdivide each segment to `detail` then jitter every point by
//!   up to `size`, driven by a **seeded SplitMix64** PRNG so a given `seed` is
//!   fully reproducible (no `rand`, no globals).
//! - [`zigzag`] — turn each segment into `ridges` alternating peaks of amplitude
//!   `size`, as hard corners or (smoothed) waves.
//! - [`pucker_bloat`] — push anchors toward (pucker) / away from (bloat) the
//!   path centroid by a fraction, pulling the bezier handles the opposite way.
//! - [`twist`] — swirl points about the centroid by an angle that grows with the
//!   point's distance from the centre.
//! - [`transform_each_affine`] — the per-copy scale / move / rotate matrix that
//!   backs **Transform Each** (with optional replication at the apply layer).
//!
//! The apply layer ([`App::apply_path_distort`]) is the **terminal** stage of the
//! dispatch chain (reached from `apply_textfield`'s catch-all); it maps these
//! over the selected shape's contours, checkpoints once, and marks the host dirty.

use crate::document::flatten;
use crate::transform::Affine;

/// How a path corner is resolved when [`offset_path`] moves the two adjacent
/// edges apart. Mirrors Illustrator's Offset Path *Joins* dropdown. (`Round` /
/// `Bevel` are wired through `apply` and exercised by tests but not yet emitted
/// by a panel — kept as seams, like the `Action` enum's own variants.)
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OffsetJoin {
    /// Extend the two offset edges until they meet at a sharp point (clamped by a
    /// miter limit, beyond which it falls back to a bevel).
    #[default]
    Miter,
    /// Connect the two offset edges with a circular arc of radius `|distance|`.
    Round,
    /// Connect the two offset edges with a single straight chamfer.
    Bevel,
}

// ─────────────────────────── seeded PRNG (SplitMix64) ───────────────────────

/// Advance a SplitMix64 state and return the next 64-bit value. A tiny,
/// allocation-free, fully deterministic generator: the same seed always yields
/// the same stream, so [`roughen`] is reproducible without any global RNG.
#[inline]
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Next pseudo-random `f32` in `[-1.0, 1.0)` from the SplitMix64 `state`.
#[inline]
fn next_signed(state: &mut u64) -> f32 {
    // Take 24 high bits → a uniform mantissa in [0,1), then map to [-1,1).
    let u = (splitmix64(state) >> 40) as f32 / (1u64 << 24) as f32;
    u * 2.0 - 1.0
}

// ─────────────────────────── small geometry helpers ────────────────────────

/// The signed (shoelace) area of a ring; its sign encodes winding direction.
fn signed_area(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a * 0.5
}

/// The centroid (mean) of a point set, or the origin for an empty set.
fn centroid(pts: &[(f32, f32)]) -> (f32, f32) {
    let n = pts.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    for &(x, y) in pts {
        sx += x;
        sy += y;
    }
    (sx / n as f32, sy / n as f32)
}

/// Intersection of the infinite lines through `p1→p2` and `p3→p4`, or `None`
/// when they are (near) parallel.
fn line_intersect(
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    p4: (f32, f32),
) -> Option<(f32, f32)> {
    let den = (p1.0 - p2.0) * (p3.1 - p4.1) - (p1.1 - p2.1) * (p3.0 - p4.0);
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((p1.0 - p3.0) * (p3.1 - p4.1) - (p1.1 - p3.1) * (p3.0 - p4.0)) / den;
    Some((p1.0 + t * (p2.0 - p1.0), p1.1 + t * (p2.1 - p1.1)))
}

#[inline]
fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Drop consecutive duplicate vertices (and a closing duplicate) so edge
/// normals are well-defined.
fn dedup_ring(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|&q| dist(p, q) > 1e-6) {
            out.push(p);
        }
    }
    while out.len() >= 2 && dist(out[0], *out.last().unwrap()) <= 1e-6 {
        out.pop();
    }
    out
}

/// Catmull-Rom-style auto out-tangent handle offsets through `points`: each
/// handle is `(next − prev) / 6`, the standard cardinal-spline → cubic-bezier
/// tangent that makes a smooth curve pass through every point.
fn auto_smooth_handles(points: &[(f32, f32)], closed: bool) -> Vec<(f32, f32)> {
    let n = points.len();
    let mut h = vec![(0.0, 0.0); n];
    if n < 3 {
        return h;
    }
    for i in 0..n {
        let prev = if i > 0 {
            points[i - 1]
        } else if closed {
            points[n - 1]
        } else {
            points[i]
        };
        let next = if i + 1 < n {
            points[i + 1]
        } else if closed {
            points[0]
        } else {
            points[i]
        };
        h[i] = ((next.0 - prev.0) / 6.0, (next.1 - prev.1) / 6.0);
    }
    h
}

/// Subdivide each segment of the (anchor) polyline into `detail` equal straight
/// pieces, returning the densified vertex list. `detail` is clamped to ≥ 1.
fn subdivide(points: &[(f32, f32)], closed: bool, detail: usize) -> Vec<(f32, f32)> {
    let detail = detail.max(1);
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let segs = if closed { n } else { n - 1 };
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(segs * detail + 1);
    for i in 0..segs {
        let a = points[i];
        let b = points[(i + 1) % n];
        for k in 0..detail {
            let t = k as f32 / detail as f32;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    if !closed {
        out.push(points[n - 1]);
    }
    out
}

// ───────────────────────────────── Offset Path ─────────────────────────────

/// Offset a path's outline by `distance` document units: positive expands
/// (outward), negative contracts (inward). Curves are flattened first, then each
/// edge is shifted along its outward normal and the corners reconnected with the
/// requested [`OffsetJoin`]. Returns a *corner* result (handles all zero), the
/// same form Illustrator's Offset Path produces. A zero distance is the identity.
pub fn offset_path(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    closed: bool,
    distance: f32,
    join: OffsetJoin,
) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    if distance == 0.0 || points.len() < 2 {
        return (points.to_vec(), handles.to_vec());
    }
    let poly = flatten(points, handles, closed);
    let out = if closed {
        offset_closed_polygon(&poly, distance, join)
    } else {
        offset_open_polyline(&poly, distance)
    };
    let n = out.len();
    (out, vec![(0.0, 0.0); n])
}

/// Parallel-offset of a closed polygon with corner-join resolution.
fn offset_closed_polygon(poly: &[(f32, f32)], distance: f32, join: OffsetJoin) -> Vec<(f32, f32)> {
    let pts = dedup_ring(poly);
    let n = pts.len();
    if n < 3 {
        return poly.to_vec();
    }
    // Choose the normal sign so that a positive distance always grows the ring,
    // regardless of whether the source winds clockwise or counter-clockwise.
    let s = if signed_area(&pts) >= 0.0 { 1.0 } else { -1.0 };
    // Each edge offset, stored as (offset start, offset end).
    let mut edges: Vec<((f32, f32), (f32, f32))> = Vec::with_capacity(n);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let nx = s * dy / len;
        let ny = -s * dx / len;
        let off = (nx * distance, ny * distance);
        edges.push(((a.0 + off.0, a.1 + off.1), (b.0 + off.0, b.1 + off.1)));
    }
    let m = edges.len();
    if m < 3 {
        return poly.to_vec();
    }
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(m + m / 2);
    for i in 0..m {
        let prev = edges[(i + m - 1) % m];
        let cur = edges[i];
        let orig = pts[i];
        let pa = prev.1; // end of the previous offset edge
        let pb = cur.0; // start of the current offset edge
        match join {
            OffsetJoin::Miter => match line_intersect(prev.0, prev.1, cur.0, cur.1) {
                Some(ip) if dist(ip, orig) <= distance.abs() * 4.0 + 1e-3 => out.push(ip),
                // Miter too long (very sharp corner) → bevel it.
                _ => {
                    out.push(pa);
                    if dist(pa, pb) > 1e-6 {
                        out.push(pb);
                    }
                }
            },
            OffsetJoin::Bevel => {
                out.push(pa);
                if dist(pa, pb) > 1e-6 {
                    out.push(pb);
                }
            }
            OffsetJoin::Round => {
                out.push(pa);
                out.extend(arc_between(orig, pa, pb, distance.abs()));
                if dist(pa, pb) > 1e-6 {
                    out.push(pb);
                }
            }
        }
    }
    out
}

/// Interior arc vertices (exclusive of the endpoints) sweeping the shortest way
/// from `from` to `to` around `center` at `radius`, for a round join.
fn arc_between(
    center: (f32, f32),
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
) -> Vec<(f32, f32)> {
    use std::f32::consts::{PI, TAU};
    let a0 = (from.1 - center.1).atan2(from.0 - center.0);
    let a1 = (to.1 - center.1).atan2(to.0 - center.0);
    let mut da = a1 - a0;
    while da > PI {
        da -= TAU;
    }
    while da < -PI {
        da += TAU;
    }
    let steps = ((da.abs() / (PI / 8.0)).ceil() as usize).max(1);
    let mut pts = Vec::with_capacity(steps.saturating_sub(1));
    for k in 1..steps {
        let a = a0 + da * (k as f32 / steps as f32);
        pts.push((center.0 + a.cos() * radius, center.1 + a.sin() * radius));
    }
    pts
}

/// Simple per-vertex offset for an open polyline: each vertex moves along the
/// average of its adjacent edge normals (endpoints use their single edge).
fn offset_open_polyline(poly: &[(f32, f32)], distance: f32) -> Vec<(f32, f32)> {
    let n = poly.len();
    if n < 2 {
        return poly.to_vec();
    }
    let edge_normal = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            (0.0, 0.0)
        } else {
            (-dy / len, dx / len)
        }
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let n_in = if i > 0 {
            edge_normal(poly[i - 1], poly[i])
        } else {
            (0.0, 0.0)
        };
        let n_out = if i + 1 < n {
            edge_normal(poly[i], poly[i + 1])
        } else {
            (0.0, 0.0)
        };
        let mut nx = n_in.0 + n_out.0;
        let mut ny = n_in.1 + n_out.1;
        let l = (nx * nx + ny * ny).sqrt();
        if l > 1e-9 {
            nx /= l;
            ny /= l;
        }
        out.push((poly[i].0 + nx * distance, poly[i].1 + ny * distance));
    }
    out
}

// ───────────────────────────────── Roughen ─────────────────────────────────

/// Roughen a path: subdivide every segment into `detail` pieces, then jitter
/// each resulting vertex by up to `size` document units in x and y, using a
/// SplitMix64 PRNG seeded with `seed`. Fully deterministic — identical `seed`
/// gives identical output. `smooth` rounds the result with auto tangents; open
/// paths keep their two endpoints pinned (matching Illustrator). A zero `size`
/// only subdivides (no displacement).
pub fn roughen(
    points: &[(f32, f32)],
    _handles: &[(f32, f32)],
    closed: bool,
    size: f32,
    detail: usize,
    seed: u64,
    smooth: bool,
) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    if points.len() < 2 {
        return (points.to_vec(), vec![(0.0, 0.0); points.len()]);
    }
    let mut dense = subdivide(points, closed, detail);
    let n = dense.len();
    let mut state = seed;
    for (i, p) in dense.iter_mut().enumerate() {
        // Pin the endpoints of an open path so the curve doesn't drift away.
        if !closed && (i == 0 || i == n - 1) {
            // Still advance the stream so later points stay seed-stable.
            let _ = next_signed(&mut state);
            let _ = next_signed(&mut state);
            continue;
        }
        let dx = next_signed(&mut state) * size;
        let dy = next_signed(&mut state) * size;
        p.0 += dx;
        p.1 += dy;
    }
    let handles = if smooth {
        auto_smooth_handles(&dense, closed)
    } else {
        vec![(0.0, 0.0); dense.len()]
    };
    (dense, handles)
}

// ───────────────────────────────── Zig-Zag ─────────────────────────────────

/// Zig-Zag: replace every segment with `ridges` alternating peaks of amplitude
/// `size` (perpendicular to the segment). `smooth` turns the corners into a wave
/// via auto tangents. `ridges == 0` (or `size == 0`) leaves the anchor positions
/// on the original segments. Operates on the flattened polyline.
pub fn zigzag(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    closed: bool,
    size: f32,
    ridges: usize,
    smooth: bool,
) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let poly = flatten(points, handles, closed);
    let n = poly.len();
    if n < 2 || ridges == 0 {
        let h = if smooth {
            auto_smooth_handles(&poly, closed)
        } else {
            vec![(0.0, 0.0); n]
        };
        return (poly, h);
    }
    let segs = if closed { n } else { n - 1 };
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(segs * (ridges + 1) + 1);
    for i in 0..segs {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        out.push(a);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = if len < 1e-9 {
            (0.0, 0.0)
        } else {
            (-dy / len, dx / len) // left normal
        };
        for k in 1..=ridges {
            let t = k as f32 / (ridges as f32 + 1.0);
            let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
            let bx = a.0 + dx * t;
            let by = a.1 + dy * t;
            out.push((bx + nx * size * sign, by + ny * size * sign));
        }
    }
    if !closed {
        out.push(poly[n - 1]);
    }
    let handles = if smooth {
        auto_smooth_handles(&out, closed)
    } else {
        vec![(0.0, 0.0); out.len()]
    };
    (out, handles)
}

// ──────────────────────────────── Pucker & Bloat ───────────────────────────

/// Pucker (`amount < 0`) / Bloat (`amount > 0`) about the path centroid: each
/// anchor moves by `(anchor − centroid) · amount` (toward the centroid for
/// pucker, away for bloat), while every bezier handle is scaled by `(1 − amount)`
/// so the tangents pull the opposite way — the spiky pucker / rounded bloat look.
/// `amount == 0` is the identity. Anchor count is preserved.
pub fn pucker_bloat(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    _closed: bool,
    amount: f32,
) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let c = centroid(points);
    let new_pts: Vec<(f32, f32)> = points
        .iter()
        .map(|&(x, y)| (x + (x - c.0) * amount, y + (y - c.1) * amount))
        .collect();
    let scale = 1.0 - amount;
    let new_handles: Vec<(f32, f32)> = (0..new_pts.len())
        .map(|i| {
            let (hx, hy) = handles.get(i).copied().unwrap_or((0.0, 0.0));
            (hx * scale, hy * scale)
        })
        .collect();
    (new_pts, new_handles)
}

// ────────────────────────────────── Twist ──────────────────────────────────

/// Twist: swirl points about the path centroid. Each anchor (and its handle
/// vector) is rotated by `angle_deg · (r / r_max)`, where `r` is the anchor's
/// distance from the centroid — so the rim spins the full angle and the centre
/// barely moves. `angle_deg == 0` is the identity. Anchor count is preserved.
pub fn twist(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    _closed: bool,
    angle_deg: f32,
) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let n = points.len();
    let c = centroid(points);
    let r_max = points
        .iter()
        .map(|&(x, y)| (x - c.0).hypot(y - c.1))
        .fold(0.0f32, f32::max)
        .max(1e-6);
    let full = angle_deg.to_radians();
    let mut new_pts = Vec::with_capacity(n);
    let mut new_handles = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y) = points[i];
        let (vx, vy) = (x - c.0, y - c.1);
        let r = vx.hypot(vy);
        let ang = full * (r / r_max);
        let (s, co) = ang.sin_cos();
        new_pts.push((c.0 + vx * co - vy * s, c.1 + vx * s + vy * co));
        let (hx, hy) = handles.get(i).copied().unwrap_or((0.0, 0.0));
        new_handles.push((hx * co - hy * s, hx * s + hy * co));
    }
    (new_pts, new_handles)
}

// ──────────────────────────────── Transform Each ───────────────────────────

/// The per-copy affine for **Transform Each**: scale by `(scale_x, scale_y)` and
/// rotate `angle_deg`, both about the pivot `(cx, cy)`, then translate by
/// `(move_x, move_y)` (an absolute move, applied last so it is unaffected by the
/// scale). Replication composes this matrix cumulatively (copy *k* = the matrix
/// applied *k* times).
pub fn transform_each_affine(
    move_x: f32,
    move_y: f32,
    scale_x: f32,
    scale_y: f32,
    angle_deg: f32,
    cx: f32,
    cy: f32,
) -> Affine {
    let mut aff = Affine::IDENTITY;
    if scale_x != 1.0 || scale_y != 1.0 {
        aff = aff.then(Affine::scale_about(scale_x, scale_y, cx, cy));
    }
    if angle_deg != 0.0 {
        aff = aff.then(Affine::rotate_about(angle_deg.to_radians(), cx, cy));
    }
    if move_x != 0.0 || move_y != 0.0 {
        aff = aff.then(Affine::translate(move_x, move_y));
    }
    aff
}


mod apply;

#[cfg(test)]
mod tests {
    use super::*;


    /// A unit-ish closed square as a corner path: (0,0)→(100,0)→(100,100)→(0,100).
    fn square() -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
        let pts = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let h = vec![(0.0, 0.0); 4];
        (pts, h)
    }

    fn bounds(pts: &[(f32, f32)]) -> (f32, f32, f32, f32) {
        let mut minx = f32::MAX;
        let mut miny = f32::MAX;
        let mut maxx = f32::MIN;
        let mut maxy = f32::MIN;
        for &(x, y) in pts {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
        (minx, miny, maxx - minx, maxy - miny)
    }

    fn dist_to(c: (f32, f32), p: (f32, f32)) -> f32 {
        (p.0 - c.0).hypot(p.1 - c.1)
    }

    // ── Offset Path ──────────────────────────────────────────────────────

    #[test]
    fn offset_zero_is_identity() {
        let (p, h) = square();
        let (np, _) = offset_path(&p, &h, true, 0.0, OffsetJoin::Miter);
        assert_eq!(np, p, "zero offset leaves the path unchanged");
    }

    #[test]
    fn offset_miter_expands_bounds_by_2d() {
        let (p, h) = square();
        let (np, nh) = offset_path(&p, &h, true, 10.0, OffsetJoin::Miter);
        let (x, y, w, hgt) = bounds(&np);
        // Each side moves out by d=10, so the box grows 2d in each axis.
        assert!((w - 120.0).abs() < 0.5, "width ~120, got {w}");
        assert!((hgt - 120.0).abs() < 0.5, "height ~120, got {hgt}");
        assert!((x - -10.0).abs() < 0.5 && (y - -10.0).abs() < 0.5, "origin moved to (-10,-10)");
        assert!(nh.iter().all(|&(hx, hy)| hx == 0.0 && hy == 0.0), "offset yields corners");
    }

    #[test]
    fn offset_inward_contracts_bounds() {
        let (p, h) = square();
        let (np, _) = offset_path(&p, &h, true, -10.0, OffsetJoin::Miter);
        let (_, _, w, hgt) = bounds(&np);
        assert!((w - 80.0).abs() < 0.5, "inward offset shrinks width to ~80, got {w}");
        assert!((hgt - 80.0).abs() < 0.5, "inward offset shrinks height to ~80, got {hgt}");
    }

    #[test]
    fn offset_bevel_adds_corner_points() {
        let (p, h) = square();
        let (miter, _) = offset_path(&p, &h, true, 10.0, OffsetJoin::Miter);
        let (bevel, _) = offset_path(&p, &h, true, 10.0, OffsetJoin::Bevel);
        // A bevel splits each of the 4 corners into 2 points.
        assert_eq!(miter.len(), 4);
        assert_eq!(bevel.len(), 8, "bevel doubles each corner");
    }

    #[test]
    fn offset_round_adds_arc_points() {
        let (p, h) = square();
        let (round, _) = offset_path(&p, &h, true, 10.0, OffsetJoin::Round);
        // Round corners interpolate arcs → strictly more vertices than a bevel.
        assert!(round.len() > 8, "round join inserts arc vertices, got {}", round.len());
        // Every rounded vertex sits ~d from its source corner (radius = |d|).
    }

    #[test]
    fn offset_handles_reversed_winding() {
        // Clockwise square (reverse order) should still expand for d>0.
        let pts = vec![(0.0, 0.0), (0.0, 100.0), (100.0, 100.0), (100.0, 0.0)];
        let h = vec![(0.0, 0.0); 4];
        let (np, _) = offset_path(&pts, &h, true, 10.0, OffsetJoin::Miter);
        let (_, _, w, hgt) = bounds(&np);
        assert!((w - 120.0).abs() < 0.5 && (hgt - 120.0).abs() < 0.5, "winding-independent expand");
    }

    // ── Roughen (seeded) ────────────────────────────────────────────────

    #[test]
    fn roughen_is_deterministic_for_a_seed() {
        let (p, h) = square();
        let a = roughen(&p, &h, true, 8.0, 4, 12345, false);
        let b = roughen(&p, &h, true, 8.0, 4, 12345, false);
        assert_eq!(a.0, b.0, "same seed → identical points");
    }

    #[test]
    fn roughen_differs_by_seed() {
        let (p, h) = square();
        let a = roughen(&p, &h, true, 8.0, 4, 1, false);
        let b = roughen(&p, &h, true, 8.0, 4, 2, false);
        assert_ne!(a.0, b.0, "different seed → different jitter");
    }

    #[test]
    fn roughen_increases_point_count() {
        let (p, h) = square();
        let (np, _) = roughen(&p, &h, true, 8.0, 4, 7, false);
        // 4 segments × detail 4 = 16 points.
        assert_eq!(np.len(), 16);
        assert!(np.len() > p.len(), "subdivision adds points");
    }

    #[test]
    fn roughen_zero_size_only_subdivides() {
        let (p, h) = square();
        let (np, _) = roughen(&p, &h, true, 0.0, 2, 99, false);
        // With size 0 the densified vertices lie exactly on the original edges.
        assert_eq!(np.len(), 8);
        // Midpoint of the top edge is (50,0).
        assert!((np[1].0 - 50.0).abs() < 1e-3 && np[1].1.abs() < 1e-3);
    }

    #[test]
    fn roughen_smooth_emits_handles() {
        let (p, h) = square();
        let (_, nh) = roughen(&p, &h, true, 8.0, 3, 5, true);
        assert!(nh.iter().any(|&(hx, hy)| hx != 0.0 || hy != 0.0), "smooth → some curved handles");
    }

    #[test]
    fn roughen_open_pins_endpoints() {
        let p = vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)];
        let h = vec![(0.0, 0.0); 3];
        let (np, _) = roughen(&p, &h, false, 10.0, 1, 3, false);
        // Open path keeps its first and last anchors fixed.
        assert_eq!(np.first().copied(), Some((0.0, 0.0)));
        assert_eq!(np.last().copied(), Some((100.0, 0.0)));
    }

    // ── Zig-Zag ─────────────────────────────────────────────────────────

    #[test]
    fn zigzag_zero_ridges_is_identity_positions() {
        let (p, h) = square();
        let (np, _) = zigzag(&p, &h, true, 10.0, 0, false);
        assert_eq!(np, p, "0 ridges keeps the original anchors");
    }

    #[test]
    fn zigzag_increases_point_count() {
        let (p, h) = square();
        let (np, _) = zigzag(&p, &h, true, 10.0, 3, false);
        // 4 segments × (1 anchor + 3 ridges) = 16.
        assert_eq!(np.len(), 16);
    }

    #[test]
    fn zigzag_displaces_perpendicular() {
        // A single horizontal segment: ridges land off the y=0 line by ±size.
        let p = vec![(0.0, 0.0), (90.0, 0.0)];
        let h = vec![(0.0, 0.0); 2];
        let (np, _) = zigzag(&p, &h, false, 10.0, 3, false);
        // np = [a, r1, r2, r3, b]; ridge ys alternate +,-,+ * 10.
        assert!((np[1].1 - 10.0).abs() < 1e-3, "first ridge up");
        assert!((np[2].1 + 10.0).abs() < 1e-3, "second ridge down");
        assert!((np[3].1 - 10.0).abs() < 1e-3, "third ridge up");
    }

    #[test]
    fn zigzag_smooth_emits_handles() {
        let (p, h) = square();
        let (_, nh) = zigzag(&p, &h, true, 10.0, 2, true);
        assert!(nh.iter().any(|&(hx, hy)| hx != 0.0 || hy != 0.0), "wave → curved handles");
    }

    // ── Pucker & Bloat ──────────────────────────────────────────────────

    #[test]
    fn pucker_moves_toward_centroid() {
        let (p, h) = square();
        let c = centroid(&p);
        let before = dist_to(c, p[0]);
        let (np, _) = pucker_bloat(&p, &h, true, -0.5);
        let after = dist_to(c, np[0]);
        assert!(after < before, "pucker pulls anchors inward: {after} < {before}");
    }

    #[test]
    fn bloat_moves_away_from_centroid() {
        let (p, h) = square();
        let c = centroid(&p);
        let before = dist_to(c, p[0]);
        let (np, _) = pucker_bloat(&p, &h, true, 0.5);
        let after = dist_to(c, np[0]);
        assert!(after > before, "bloat pushes anchors outward: {after} > {before}");
    }

    #[test]
    fn pucker_bloat_zero_is_identity() {
        let (p, h) = square();
        let (np, nh) = pucker_bloat(&p, &h, true, 0.0);
        assert_eq!(np, p);
        assert_eq!(nh, h);
    }

    #[test]
    fn pucker_bloat_pulls_handles_opposite() {
        let p = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let h = vec![(10.0, 0.0); 4];
        // Bloat 0.25 → handles scale by (1 - 0.25) = 0.75.
        let (_, nh) = pucker_bloat(&p, &h, true, 0.25);
        assert!((nh[0].0 - 7.5).abs() < 1e-4, "handle shrinks as anchors bloat");
    }

    // ── Twist ───────────────────────────────────────────────────────────

    #[test]
    fn twist_rotates_farther_points_more() {
        // Two points at different radii from the centroid; the outer rotates more.
        let p = vec![(0.0, 0.0), (10.0, 0.0), (100.0, 0.0), (110.0, 0.0)];
        let h = vec![(0.0, 0.0); 4];
        let c = centroid(&p);
        // Signed rotation between the original and twisted vectors (wrap-safe via
        // the cross/dot atan2), absolute value = how far each point swung.
        let ang = |a: (f32, f32), b: (f32, f32)| {
            let va = (a.0 - c.0, a.1 - c.1);
            let vb = (b.0 - c.0, b.1 - c.1);
            let cross = va.0 * vb.1 - va.1 * vb.0;
            let dot = va.0 * vb.0 + va.1 * vb.1;
            cross.atan2(dot).abs()
        };
        let (np, _) = twist(&p, &h, false, 90.0);
        let inner = ang(p[1], np[1]); // closer to centre
        let outer = ang(p[3], np[3]); // farthest
        assert!(outer > inner, "outer point rotates more: {outer} > {inner}");
    }

    #[test]
    fn twist_zero_is_identity() {
        let (p, h) = square();
        let (np, _) = twist(&p, &h, true, 0.0);
        for (a, b) in p.iter().zip(np.iter()) {
            assert!((a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
        }
    }

    #[test]
    fn twist_preserves_point_count_and_centroid() {
        let (p, h) = square();
        let c0 = centroid(&p);
        let (np, _) = twist(&p, &h, true, 45.0);
        assert_eq!(np.len(), p.len());
        let c1 = centroid(&np);
        assert!((c0.0 - c1.0).abs() < 1e-2 && (c0.1 - c1.1).abs() < 1e-2, "twist keeps centroid");
    }

    // ── Transform Each affine ───────────────────────────────────────────

    #[test]
    fn transform_each_affine_rotates_about_pivot() {
        // 90° about (0,0): (10,0) → (0,10) in screen space (+rotation = clockwise).
        let aff = transform_each_affine(0.0, 0.0, 1.0, 1.0, 90.0, 0.0, 0.0);
        let (x, y) = aff.apply_point(10.0, 0.0);
        assert!((x).abs() < 1e-3 && (y - 10.0).abs() < 1e-3, "got ({x},{y})");
    }

    #[test]
    fn transform_each_affine_translates_and_scales() {
        let aff = transform_each_affine(5.0, -3.0, 2.0, 2.0, 0.0, 0.0, 0.0);
        let (x, y) = aff.apply_point(10.0, 10.0);
        // scale 2 about origin → (20,20), then translate (5,-3).
        assert!((x - 25.0).abs() < 1e-3 && (y - 17.0).abs() < 1e-3, "got ({x},{y})");
    }
}
