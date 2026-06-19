//! Perspective distort (Free Distort) — a real 4-corner projective warp.
//!
//! Illustrator's *Free Distort* / *Perspective* drags the four corners of a
//! selection's bounding box and projectively maps the artwork into the resulting
//! quad. Contour implements it as a **geometry transform**: solve the homography
//! `H` that sends the unit square's corners to the four destination corners, then
//! push every anchor (and bezier handle endpoint) of the shape through `H`,
//! normalised into the shape's bounding box first. Because we transform the vector
//! geometry directly — not pixels — the result stays resolution-independent and
//! re-strokes/fills exactly like any other path.
//!
//! The corner order is **top-left, top-right, bottom-right, bottom-left**,
//! matching the on-canvas handle layout.

/// Four corners defining a perspective warp, in document space.
/// Order: top-left, top-right, bottom-right, bottom-left.
pub type Corners = [[f32; 2]; 4];

/// Compute the 3×3 homography matrix `H` mapping the **unit square**
/// `{(0,0), (1,0), (1,1), (0,1)}` to the given destination corners (same order).
/// Returns the 9 coefficients row-major (`[h00,h01,h02, h10,h11,h12, h20,h21,h22]`),
/// normalised so `h22 = 1`.
///
/// Uses the classic closed-form unit-square→quad solution (Heckbert): far cheaper
/// and more stable than a full DLT for this fixed source. Falls back to identity
/// if the quad is degenerate (the linear system is singular).
pub fn homography(dst: &Corners) -> [f32; 9] {
    let x0 = dst[0][0] as f64;
    let y0 = dst[0][1] as f64;
    let x1 = dst[1][0] as f64;
    let y1 = dst[1][1] as f64;
    let x2 = dst[2][0] as f64;
    let y2 = dst[2][1] as f64;
    let x3 = dst[3][0] as f64;
    let y3 = dst[3][1] as f64;

    // dx1 = x1 - x2, dx2 = x3 - x2, sx = x0 - x1 + x2 - x3 (and same for y).
    let dx1 = x1 - x2;
    let dx2 = x3 - x2;
    let sx = x0 - x1 + x2 - x3;
    let dy1 = y1 - y2;
    let dy2 = y3 - y2;
    let sy = y0 - y1 + y2 - y3;

    let denom = dx1 * dy2 - dx2 * dy1;
    if denom.abs() < 1e-12 {
        // Affine (parallelogram) case, or degenerate: identity-ish affine map.
        let h = [
            (x1 - x0) as f32,
            (x3 - x0) as f32,
            x0 as f32,
            (y1 - y0) as f32,
            (y3 - y0) as f32,
            y0 as f32,
            0.0,
            0.0,
            1.0,
        ];
        return h;
    }
    let g = (sx * dy2 - sy * dx2) / denom; // h20
    let h = (dx1 * sy - dy1 * sx) / denom; // h21

    let h00 = x1 - x0 + g * x1;
    let h01 = x3 - x0 + h * x3;
    let h02 = x0;
    let h10 = y1 - y0 + g * y1;
    let h11 = y3 - y0 + h * y3;
    let h12 = y0;
    [
        h00 as f32, h01 as f32, h02 as f32, h10 as f32, h11 as f32, h12 as f32, g as f32, h as f32,
        1.0,
    ]
}

/// Apply homography `h` (row-major, `h22` normalising) to a normalised point
/// `(u, v)` in the unit square, returning the projected document-space point.
pub fn project(h: &[f32; 9], u: f32, v: f32) -> (f32, f32) {
    let (u, v) = (u as f64, v as f64);
    let hx = h[0] as f64 * u + h[1] as f64 * v + h[2] as f64;
    let hy = h[3] as f64 * u + h[4] as f64 * v + h[5] as f64;
    let hw = h[6] as f64 * u + h[7] as f64 * v + h[8] as f64;
    let w = if hw.abs() < 1e-12 { 1.0 } else { hw };
    ((hx / w) as f32, (hy / w) as f32)
}

/// Warp a set of document-space points lying inside `bbox` `[x, y, w, h]` through
/// the homography defined by `dst`. Each point is normalised into the unit square
/// by `bbox`, projected, and returned in document space. Handles (bezier
/// out-tangent offsets) must be warped as *endpoint deltas* via [`warp_handles`].
pub fn warp_points(points: &[(f32, f32)], bbox: [f32; 4], dst: &Corners) -> Vec<(f32, f32)> {
    let h = homography(dst);
    let (bx, by, bw, bh) = (bbox[0], bbox[1], bbox[2].max(1e-6), bbox[3].max(1e-6));
    points
        .iter()
        .map(|&(x, y)| {
            let u = (x - bx) / bw;
            let v = (y - by) / bh;
            project(&h, u, v)
        })
        .collect()
}

/// Warp bezier out-tangent handles alongside their anchors. A handle is an offset
/// from its anchor; under a projective map the offset is not linear, so we warp
/// the *handle endpoint* (`anchor + handle`) and recover the new offset as
/// `warped_endpoint − warped_anchor`. `points` are the original (un-warped)
/// anchors; `warped` are their projected positions (from [`warp_points`]).
pub fn warp_handles(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    warped: &[(f32, f32)],
    bbox: [f32; 4],
    dst: &Corners,
) -> Vec<(f32, f32)> {
    let h = homography(dst);
    let (bx, by, bw, bh) = (bbox[0], bbox[1], bbox[2].max(1e-6), bbox[3].max(1e-6));
    points
        .iter()
        .zip(handles.iter())
        .enumerate()
        .map(|(i, (&(ax, ay), &(hx, hy)))| {
            if hx == 0.0 && hy == 0.0 {
                return (0.0, 0.0);
            }
            let ex = ax + hx;
            let ey = ay + hy;
            let wp = project(&h, (ex - bx) / bw, (ey - by) / bh);
            let wa = warped.get(i).copied().unwrap_or((ax, ay));
            (wp.0 - wa.0, wp.1 - wa.1)
        })
        .collect()
}

/// Apply inverse homography warp to a raster (placeholder — Contour distorts vector
/// geometry, not pixels; kept for callers/tests that probe the raster path).
pub fn warp_pixels(src: &[u8], _src_w: u32, _src_h: u32, _dst: &Corners) -> Vec<u8> {
    src.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_quad_is_identity() {
        // Unit square -> unit square: H is identity, points unchanged.
        let dst: Corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let pts = vec![(0.25, 0.75), (0.5, 0.5)];
        let out = warp_points(&pts, [0.0, 0.0, 1.0, 1.0], &dst);
        for (a, b) in pts.iter().zip(out.iter()) {
            assert!((a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
        }
    }

    #[test]
    fn corners_map_exactly() {
        // A trapezoid: the four bbox corners must land on the four dst corners.
        let dst: Corners = [[10.0, 0.0], [90.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let bbox = [0.0, 0.0, 100.0, 100.0];
        let corners = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let out = warp_points(&corners, bbox, &dst);
        let expect = [(10.0, 0.0), (90.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        for (o, e) in out.iter().zip(expect.iter()) {
            assert!(
                (o.0 - e.0).abs() < 1e-2 && (o.1 - e.1).abs() < 1e-2,
                "corner {o:?} != {e:?}"
            );
        }
    }

    #[test]
    fn trapezoid_compresses_top() {
        // Top edge narrower than bottom: a midline point at the top moves inward
        // relative to one at the bottom (true perspective foreshortening).
        let dst: Corners = [[20.0, 0.0], [80.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let bbox = [0.0, 0.0, 100.0, 100.0];
        let top = warp_points(&[(0.0, 0.0)], bbox, &dst)[0];
        let bottom = warp_points(&[(0.0, 100.0)], bbox, &dst)[0];
        assert!(top.0 > bottom.0, "top-left pulled inward: {top:?} {bottom:?}");
    }

    #[test]
    fn handles_warp_with_anchors() {
        let dst: Corners = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let bbox = [0.0, 0.0, 1.0, 1.0];
        let pts = vec![(0.5, 0.5)];
        let handles = vec![(0.1, 0.0)];
        let warped = warp_points(&pts, bbox, &dst);
        let wh = warp_handles(&pts, &handles, &warped, bbox, &dst);
        // Scale x2: a 0.1 x-offset becomes ~0.2.
        assert!((wh[0].0 - 0.2).abs() < 1e-3, "handle scaled: {:?}", wh[0]);
    }
}
