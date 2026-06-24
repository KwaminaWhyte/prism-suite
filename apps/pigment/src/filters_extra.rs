//! Additional CPU raster filters: Surface Blur and Path Blur.
//!
//! Each filter is a pure function over a linear-premultiplied RGBA `f32` buffer
//! (`read_layer_f32` / `upload_layer_f32` contract), so it slots straight into
//! the `Action::ApplyFilter`-style dispatch exactly like `filters.rs` does — no
//! engine/shader or shared-crate change.
//!
//! * **Surface Blur** is an edge-preserving (bilateral-style) blur: it averages
//!   neighbours weighted by how close their color is to the centre pixel, so it
//!   smooths flat regions while keeping strong edges crisp.
//! * **Path Blur** approximates Photoshop's path-driven motion blur as a single
//!   directional smear: each segment contributes an angle + length, and the
//!   per-pixel blur direction is the average path direction.

/// Bilinear sample of a premultiplied RGBA `f32` buffer with clamped edges.
#[inline]
fn bilinear(px: &[f32], w: u32, h: u32, x: f32, y: f32) -> [f32; 4] {
    let (wi, hi) = (w as i32, h as i32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; 4];
    for dy in 0..2i32 {
        for dx in 0..2i32 {
            let sx = (x0 + dx).clamp(0, wi - 1);
            let sy = (y0 + dy).clamp(0, hi - 1);
            let wb = (if dx == 0 { 1.0 - tx } else { tx }) * (if dy == 0 { 1.0 - ty } else { ty });
            let i = ((sy as u32 * w + sx as u32) * 4) as usize;
            for c in 0..4 {
                out[c] += px[i + c] * wb;
            }
        }
    }
    out
}

/// Unpremultiply one texel: returns straight RGBA, guarding against zero alpha.
#[inline]
fn unpremul(c: [f32; 4]) -> [f32; 4] {
    if c[3] > 1e-6 {
        [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// Repremultiply straight RGBA back into premultiplied storage.
#[inline]
fn premul(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

/// Surface Blur — edge-preserving bilateral blur.
///
/// For each pixel, average the `radius`-neighbourhood weighting each neighbour by
/// a falloff on its color distance from the centre: neighbours whose straight
/// color differs by more than `threshold` (0..1) contribute little, so strong
/// edges are preserved while flat areas blur. Operates on straight color (so
/// alpha edges stay clean) and re-premultiplies.
pub fn surface_blur(px: &[f32], w: u32, h: u32, radius: f32, threshold: f32) -> Vec<f32> {
    let r = radius.round().max(0.0) as i32;
    if r == 0 {
        return px.to_vec();
    }
    let (wi, hi) = (w as i32, h as i32);
    let thr = threshold.max(1e-3);
    let mut out = vec![0.0f32; px.len()];
    for y in 0..hi {
        for x in 0..wi {
            let ci = ((y as u32 * w + x as u32) * 4) as usize;
            let center = unpremul([px[ci], px[ci + 1], px[ci + 2], px[ci + 3]]);
            let mut acc = [0.0f32; 4];
            let mut wsum = 0.0f32;
            for dy in -r..=r {
                let sy = (y + dy).clamp(0, hi - 1);
                for dx in -r..=r {
                    let sx = (x + dx).clamp(0, wi - 1);
                    let si = ((sy as u32 * w + sx as u32) * 4) as usize;
                    let s = unpremul([px[si], px[si + 1], px[si + 2], px[si + 3]]);
                    let d = (s[0] - center[0])
                        .abs()
                        .max((s[1] - center[1]).abs())
                        .max((s[2] - center[2]).abs());
                    // Tent weight: 1 at zero color distance, 0 at >= threshold.
                    let weight = (1.0 - d / thr).max(0.0);
                    if weight > 0.0 {
                        for c in 0..4 {
                            acc[c] += s[c] * weight;
                        }
                        wsum += weight;
                    }
                }
            }
            let result = if wsum > 0.0 {
                [acc[0] / wsum, acc[1] / wsum, acc[2] / wsum, acc[3] / wsum]
            } else {
                center
            };
            let pm = premul(result);
            for c in 0..4 {
                out[ci + c] = pm[c];
            }
        }
    }
    out
}

/// A single segment of a blur path: a start point and an end point in doc px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSegment {
    pub start: [f32; 2],
    pub end: [f32; 2],
}

impl PathSegment {
    /// Direction angle of this segment in radians.
    pub fn angle(&self) -> f32 {
        (self.end[1] - self.start[1]).atan2(self.end[0] - self.start[0])
    }

    /// Euclidean length of this segment in px.
    pub fn length(&self) -> f32 {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        (dx * dx + dy * dy).sqrt()
    }
}

/// Average direction (unit vector) and average segment length across all
/// segments. The direction is the unit of the summed displacement vector.
pub fn path_average(segments: &[PathSegment]) -> Option<([f32; 2], f32)> {
    if segments.is_empty() {
        return None;
    }
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut len = 0.0f32;
    for s in segments {
        sx += s.end[0] - s.start[0];
        sy += s.end[1] - s.start[1];
        len += s.length();
    }
    let mag = (sx * sx + sy * sy).sqrt();
    let dir = if mag > 1e-6 {
        [sx / mag, sy / mag]
    } else {
        let a = segments[0].angle();
        [a.cos(), a.sin()]
    };
    Some((dir, len / segments.len() as f32))
}

/// Path Blur — directional motion blur along a user path.
///
/// The path is reduced to an average direction + average segment length (see
/// [`path_average`]); every pixel is then smeared along that direction over
/// `length` px (scaled by `strength`). A single-shared-direction approximation
/// of Photoshop's Path Blur. Operates on premultiplied color (averaging premult
/// is correct for blur).
pub fn path_blur(px: &[f32], w: u32, h: u32, segments: &[PathSegment], strength: f32) -> Vec<f32> {
    let Some((dir, avg_len)) = path_average(segments) else {
        return px.to_vec();
    };
    let dist = (avg_len * strength.max(0.0)).max(0.0);
    if dist < 0.5 {
        return px.to_vec();
    }
    let (dx, dy) = (dir[0], dir[1]);
    let samples = (dist.round() as i32).clamp(1, 256);
    let mut out = vec![0.0f32; px.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for s in 0..=samples {
                let t = (s as f32 / samples as f32 - 0.5) * dist;
                let sx = x as f32 + 0.5 + dx * t;
                let sy = y as f32 + 0.5 + dy * t;
                let c = bilinear(px, w, h, sx - 0.5, sy - 0.5);
                for k in 0..4 {
                    acc[k] += c[k];
                }
            }
            let inv = 1.0 / (samples as f32 + 1.0);
            let i = ((y * w + x) * 4) as usize;
            for k in 0..4 {
                out[i + k] = acc[k] * inv;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, c: [f32; 4]) -> Vec<f32> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&c);
        }
        v
    }

    #[test]
    fn surface_blur_zero_radius_identity() {
        let p = flat(4, 4, [0.5, 0.25, 0.1, 1.0]);
        let out = surface_blur(&p, 4, 4, 0.0, 0.1);
        assert_eq!(out, p);
    }

    #[test]
    fn surface_blur_flat_preserved() {
        let p = flat(8, 8, [0.4, 0.6, 0.2, 1.0]);
        let out = surface_blur(&p, 8, 8, 3.0, 0.1);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.4).abs() < 1e-3, "r drifted: {}", ch[0]);
            assert!((ch[3] - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn surface_blur_preserves_strong_edge() {
        // A hard black/white vertical edge with a tiny threshold stays sharp.
        let mut p = flat(8, 8, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..8 {
            for x in 4..8 {
                let i = ((y * 8 + x) * 4) as usize;
                p[i] = 1.0;
                p[i + 1] = 1.0;
                p[i + 2] = 1.0;
            }
        }
        let out = surface_blur(&p, 8, 8, 2.0, 0.05);
        let i = ((4 * 8 + 7) * 4) as usize;
        assert!(out[i] > 0.95, "edge bled: {}", out[i]);
        let j = ((4 * 8) * 4) as usize;
        assert!(out[j] < 0.05, "edge bled: {}", out[j]);
    }

    #[test]
    fn surface_blur_smooths_noise_in_flat_region() {
        // A single salt pixel inside a flat grey field, generous threshold.
        let mut p = flat(8, 8, [0.5, 0.5, 0.5, 1.0]);
        let i = ((4 * 8 + 4) * 4) as usize;
        p[i] = 1.0;
        p[i + 1] = 1.0;
        p[i + 2] = 1.0;
        let out = surface_blur(&p, 8, 8, 2.0, 1.0);
        assert!(out[i] < 0.9, "salt not smoothed: {}", out[i]);
    }

    #[test]
    fn path_segment_angle_and_length() {
        let s = PathSegment { start: [0.0, 0.0], end: [3.0, 4.0] };
        assert!((s.length() - 5.0).abs() < 1e-4);
        assert!((s.angle() - (4.0f32).atan2(3.0)).abs() < 1e-4);
    }

    #[test]
    fn path_average_single_segment() {
        let segs = [PathSegment { start: [0.0, 0.0], end: [10.0, 0.0] }];
        let (dir, len) = path_average(&segs).unwrap();
        assert!((dir[0] - 1.0).abs() < 1e-4);
        assert!(dir[1].abs() < 1e-4);
        assert!((len - 10.0).abs() < 1e-4);
    }

    #[test]
    fn path_average_empty_is_none() {
        assert!(path_average(&[]).is_none());
    }

    #[test]
    fn path_blur_empty_path_identity() {
        let p = flat(4, 4, [0.3, 0.3, 0.3, 1.0]);
        let out = path_blur(&p, 4, 4, &[], 1.0);
        assert_eq!(out, p);
    }

    #[test]
    fn path_blur_flat_preserved() {
        let p = flat(8, 8, [0.4, 0.6, 0.2, 1.0]);
        let segs = [PathSegment { start: [0.0, 0.0], end: [6.0, 0.0] }];
        let out = path_blur(&p, 8, 8, &segs, 1.0);
        for ch in out.chunks(4) {
            assert!((ch[0] - 0.4).abs() < 1e-3, "r drifted: {}", ch[0]);
            assert!((ch[3] - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn path_blur_smears_edge_horizontally() {
        // A vertical edge blurred along a horizontal path produces a gradient.
        let mut p = flat(16, 16, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..16 {
            for x in 8..16 {
                let i = ((y * 16 + x) * 4) as usize;
                p[i] = 1.0;
                p[i + 1] = 1.0;
                p[i + 2] = 1.0;
            }
        }
        let segs = [PathSegment { start: [0.0, 8.0], end: [8.0, 8.0] }];
        let out = path_blur(&p, 16, 16, &segs, 1.0);
        let i = ((8 * 16 + 7) * 4) as usize;
        assert!(out[i] > 0.05, "edge not smeared: {}", out[i]);
    }
}
