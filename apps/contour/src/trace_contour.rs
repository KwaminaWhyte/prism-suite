//! Dependency-free raster → vector **contour extraction** for the extended
//! Image Trace panel (`ImageTraceMode` / `ImageTraceConfig`).
//!
//! Where [`crate::trace`] drives the external `vtracer` engine, this module is a
//! pure, deterministic, fully testable pipeline written from scratch:
//!
//! 1. **Quantise** the source pixels into a small label map. Black & White modes
//!    threshold luminance; the colour modes posterise to N colours (median-cut by
//!    value buckets) so each distinct colour is its own region.
//! 2. For every label, **march the region boundary** with Moore neighbour
//!    boundary tracing into one closed polyline per connected component.
//! 3. **Simplify** each polyline with Douglas–Peucker so a straight raster edge
//!    collapses to two endpoints instead of one anchor per pixel.
//!
//! The result is a list of [`TraceContour`]s — closed polylines tagged with the
//! region's RGBA fill — that the apply layer turns into document `Shape::Path`s.
//! No `vtracer`, no `visioncortex`: this code runs in unit tests on hand-built
//! bitmaps and produces an exact, predictable contour count.

use crate::app_state::{ImageTraceConfig, ImageTraceMode};

/// One traced region boundary: a closed polyline of absolute pixel-space points
/// plus the region's representative RGBA fill (straight sRGB, 0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct TraceContour {
    /// Closed boundary polyline (no duplicated closing point).
    pub points: Vec<(f32, f32)>,
    /// Region fill colour.
    pub fill: [f32; 4],
}

impl TraceContour {
    /// Axis-aligned bounding box `[x, y, w, h]`, or `None` when empty.
    #[allow(dead_code)]
    pub fn bbox(&self) -> Option<[f32; 4]> {
        if self.points.is_empty() {
            return None;
        }
        let (mut minx, mut miny) = (f32::INFINITY, f32::INFINITY);
        let (mut maxx, mut maxy) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &(x, y) in &self.points {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
        Some([minx, miny, maxx - minx, maxy - miny])
    }
}

/// How many posterise buckets a mode targets (per channel granularity is folded
/// into a single colour-count cap by [`quantize`]).
fn mode_colors(mode: ImageTraceMode, cfg: &ImageTraceConfig) -> usize {
    match mode {
        ImageTraceMode::BlackWhite
        | ImageTraceMode::BlackAndWhite
        | ImageTraceMode::Silhouette
        | ImageTraceMode::Sketch => 2,
        ImageTraceMode::Grayscale | ImageTraceMode::Technical => 4,
        ImageTraceMode::Color3 => 3,
        ImageTraceMode::Color6 | ImageTraceMode::Outlined => 6,
        ImageTraceMode::Color16 => 16,
        ImageTraceMode::Photo => 16,
        ImageTraceMode::Logo => 4,
        // The plain `Color` mode honours the panel's colour count.
        ImageTraceMode::Color => (cfg.colors as usize).clamp(2, 30),
    }
}

/// Rec.601 luminance (0..255) of a straight-sRGB byte triple.
fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

/// A quantised label map: `labels[y*w+x]` is a palette index, with one RGBA
/// colour per index, and a `background` label (the most common one) that the
/// tracer skips so we vectorise the foreground, not the field.
struct LabelMap {
    width: usize,
    height: usize,
    labels: Vec<u16>,
    palette: Vec<[f32; 4]>,
    background: u16,
}

/// Quantise an RGBA buffer into a [`LabelMap`] honouring `cfg.mode`,
/// `cfg.threshold`, and `cfg.ignore_white`.
fn quantize(rgba: &[u8], width: usize, height: usize, cfg: &ImageTraceConfig) -> LabelMap {
    let n_colors = mode_colors(cfg.mode, cfg);
    let bw = matches!(
        cfg.mode,
        ImageTraceMode::BlackWhite
            | ImageTraceMode::BlackAndWhite
            | ImageTraceMode::Silhouette
            | ImageTraceMode::Sketch
    ) || n_colors == 2;

    let mut labels = vec![0u16; width * height];
    let palette: Vec<[f32; 4]>;

    if bw {
        // Two-level luminance threshold: label 0 = light (background), 1 = dark.
        palette = vec![[1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]];
        for (i, px) in rgba.chunks_exact(4).take(width * height).enumerate() {
            let l = luma(px[0], px[1], px[2]);
            let dark = px[3] > 0 && l < cfg.threshold as f32;
            labels[i] = if dark { 1 } else { 0 };
        }
    } else {
        // Posterise luminance into `n_colors` value buckets, averaging the true
        // colour of every pixel that falls in a bucket so the palette entry is a
        // faithful representative rather than a grey ramp.
        let levels = n_colors.max(2);
        let mut sums = vec![[0.0f64; 4]; levels]; // r,g,b,count
        for px in rgba.chunks_exact(4).take(width * height) {
            let l = luma(px[0], px[1], px[2]);
            let b = ((l / 255.0) * (levels as f32 - 1.0)).round() as usize;
            let b = b.min(levels - 1);
            sums[b][0] += px[0] as f64;
            sums[b][1] += px[1] as f64;
            sums[b][2] += px[2] as f64;
            sums[b][3] += 1.0;
        }
        palette = (0..levels)
            .map(|b| {
                let c = sums[b][3].max(1.0);
                if sums[b][3] == 0.0 {
                    // Empty bucket: fall back to the grey ramp value.
                    let g = b as f32 / (levels as f32 - 1.0);
                    [g, g, g, 1.0]
                } else {
                    [
                        (sums[b][0] / c / 255.0) as f32,
                        (sums[b][1] / c / 255.0) as f32,
                        (sums[b][2] / c / 255.0) as f32,
                        1.0,
                    ]
                }
            })
            .collect();
        for (i, px) in rgba.chunks_exact(4).take(width * height).enumerate() {
            let l = luma(px[0], px[1], px[2]);
            let b = ((l / 255.0) * (levels as f32 - 1.0)).round() as usize;
            labels[i] = b.min(levels - 1) as u16;
        }
    }

    // Pick the background label = the most-frequent palette index. In B/W modes
    // the lightest level is background; `ignore_white` forces the white level
    // to background even if it is a minority.
    let mut counts = vec![0usize; palette.len()];
    for &l in &labels {
        counts[l as usize] += 1;
    }
    let mut background = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(i, _)| i as u16)
        .unwrap_or(0);
    if cfg.ignore_white {
        // The label whose palette colour is nearest white.
        if let Some((wi, _)) = palette.iter().enumerate().max_by(|(_, a), (_, b)| {
            (a[0] + a[1] + a[2])
                .partial_cmp(&(b[0] + b[1] + b[2]))
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            background = wi as u16;
        }
    }

    LabelMap {
        width,
        height,
        labels,
        palette,
        background,
    }
}

/// 8-direction Moore-neighbour offsets, clockwise starting east.
const MOORE: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

impl LabelMap {
    #[inline]
    fn at(&self, x: i32, y: i32) -> Option<u16> {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            None
        } else {
            Some(self.labels[y as usize * self.width + x as usize])
        }
    }

    /// Whether `(x,y)` belongs to region `label` (and is inside the grid).
    #[inline]
    fn is(&self, x: i32, y: i32, label: u16) -> bool {
        self.at(x, y) == Some(label)
    }
}

/// Trace the outer boundary of the connected component of `label` that contains
/// the seed pixel `(sx, sy)`, using Moore boundary tracing (Radial Sweep). Marks
/// every visited boundary pixel in `visited` so a component is traced once.
/// Returns the closed boundary polyline in pixel coordinates.
fn moore_trace(
    map: &LabelMap,
    label: u16,
    sx: i32,
    sy: i32,
    visited: &mut [bool],
) -> Vec<(f32, f32)> {
    let w = map.width as i32;
    let mut contour: Vec<(i32, i32)> = Vec::new();
    let start = (sx, sy);
    contour.push(start);
    visited[(sy * w + sx) as usize] = true;

    // Direction we *entered* the current boundary pixel from. Start as if we
    // came from the west (the seed is the leftmost pixel of its top row), so the
    // backtrack point is to the left.
    let mut b_dir = 4usize; // index into MOORE pointing west (entry direction)
    let mut cur = start;

    // Cap iterations defensively (perimeter ≤ 8 * area worst case).
    let max_iter = (map.width * map.height * 8 + 16) as usize;
    for _ in 0..max_iter {
        // Begin the clockwise sweep from the pixel *after* the backtrack
        // neighbour (the cell we came from), as per Moore tracing.
        let start_dir = (b_dir + 1) % 8;
        let mut found = false;
        for k in 0..8 {
            let dir = (start_dir + k) % 8;
            let (dx, dy) = MOORE[dir];
            let nx = cur.0 + dx;
            let ny = cur.1 + dy;
            if map.is(nx, ny, label) {
                // Move to the foreground neighbour; record where we came from.
                // The backtrack direction is the reverse of the step we took.
                b_dir = (dir + 4) % 8;
                cur = (nx, ny);
                if !(nx == start.0 && ny == start.1) {
                    visited[(ny * w + nx) as usize] = true;
                }
                contour.push(cur);
                found = true;
                break;
            }
        }
        if !found {
            // Isolated pixel: a single-cell region.
            break;
        }
        // Jacob's stopping criterion: back at the start with the same entry dir.
        if cur == start && contour.len() > 2 {
            contour.pop(); // drop the duplicated start
            break;
        }
    }

    contour
        .into_iter()
        .map(|(x, y)| (x as f32, y as f32))
        .collect()
}

/// Perpendicular distance from point `p` to the line through `a`–`b`.
fn perp_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((dx * (a.1 - p.1) - (a.0 - p.0) * dy).abs()) / len
}

/// Douglas–Peucker simplification of an **open** polyline.
fn douglas_peucker(pts: &[(f32, f32)], epsilon: f32) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    // Iterative stack-based DP to avoid deep recursion on long contours.
    let mut stack = vec![(0usize, n - 1)];
    while let Some((lo, hi)) = stack.pop() {
        if hi <= lo + 1 {
            continue;
        }
        let mut max_d = 0.0f32;
        let mut idx = lo;
        for i in (lo + 1)..hi {
            let d = perp_distance(pts[i], pts[lo], pts[hi]);
            if d > max_d {
                max_d = d;
                idx = i;
            }
        }
        if max_d > epsilon {
            keep[idx] = true;
            stack.push((lo, idx));
            stack.push((idx, hi));
        }
    }
    pts.iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, &p)| p)
        .collect()
}

/// Simplify a **closed** ring with Douglas–Peucker. Closes the ring (duplicates
/// the first point to the end), simplifies as an open chain, then drops the
/// duplicate, so the closing segment participates in the tolerance test.
fn simplify_ring(pts: &[(f32, f32)], epsilon: f32) -> Vec<(f32, f32)> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let mut chain = pts.to_vec();
    chain.push(pts[0]);
    let mut simp = douglas_peucker(&chain, epsilon);
    if simp.len() > 1 && simp.first() == simp.last() {
        simp.pop();
    }
    simp
}

/// Trace `rgba` (`width`×`height`, row-major, 4 B/px) into closed, simplified
/// region contours honouring `cfg`. Pure and deterministic. Components smaller
/// than `cfg.noise` pixels of perimeter are dropped (speckle filter), and the
/// corner/path knobs drive the Douglas–Peucker tolerance.
pub fn trace_contours(
    rgba: &[u8],
    width: usize,
    height: usize,
    cfg: &ImageTraceConfig,
) -> Vec<TraceContour> {
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return Vec::new();
    }
    let map = quantize(rgba, width, height, cfg);

    // Douglas–Peucker epsilon: the panel's `paths` knob (1..=100) is a fidelity
    // slider — higher fidelity → smaller epsilon (more anchors). Map 100→0.25px,
    // 1→~4px so even a coarse trace keeps the gross silhouette.
    let fidelity = (cfg.paths as f32).clamp(1.0, 100.0) / 100.0;
    let epsilon = 0.25 + (1.0 - fidelity) * 3.75;
    // Minimum boundary length (perimeter) to keep, from the noise knob.
    let min_perimeter = (cfg.noise as f32).max(0.0);

    let mut visited = vec![false; width * height];
    let mut out: Vec<TraceContour> = Vec::new();

    // Scan in raster order; a foreground pixel whose left neighbour is *not* the
    // same region is a boundary seed (the leftmost pixel of a new row of the
    // component), so each connected component is seeded once via `visited`.
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let label = map.labels[y as usize * width + x as usize];
            if label == map.background {
                continue;
            }
            if visited[(y * width as i32 + x) as usize] {
                continue;
            }
            // Only seed at a left edge of the region (no same-label pixel to the
            // left) so we start tracing on an actual boundary cell.
            if map.is(x - 1, y, label) {
                continue;
            }
            let ring = moore_trace(&map, label, x, y, &mut visited);
            if ring.len() < 3 {
                continue;
            }
            // Perimeter in pixels (closed).
            let mut perim = 0.0f32;
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                perim += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            }
            if perim < min_perimeter {
                continue;
            }
            let simplified = simplify_ring(&ring, epsilon);
            if simplified.len() < 3 {
                continue;
            }
            out.push(TraceContour {
                points: simplified,
                fill: map.palette[label as usize],
            });
        }
    }
    out
}

/// Convert native [`TraceContour`]s into document [`Shape`]s: one closed, filled
/// [`Shape::Path`] per contour, all tagged into `group_id` so the trace result
/// selects / moves as one object. Mirrors [`crate::trace::regions_to_shapes`] for
/// the dependency-free path.
#[allow(dead_code)]
pub fn contours_to_shapes(contours: &[TraceContour], group_id: u64) -> Vec<crate::document::Shape> {
    contours
        .iter()
        .filter(|c| c.points.len() >= 3)
        .map(|c| {
            let handles = vec![(0.0, 0.0); c.points.len()];
            let mut s = crate::document::Shape::path(
                c.points.clone(),
                handles,
                true,
                c.fill,
                [0.0, 0.0, 0.0, 0.0],
                0.0,
            );
            s.set_group(Some(group_id));
            s
        })
        .collect()
}

/// Trace `rgba` into ready-to-insert grouped [`Shape`]s in one call using the
/// native (vtracer-free) contour pipeline.
#[allow(dead_code)]
pub fn trace_to_shapes(
    rgba: &[u8],
    width: usize,
    height: usize,
    cfg: &ImageTraceConfig,
    group_id: u64,
) -> Vec<crate::document::Shape> {
    let contours = trace_contours(rgba, width, height, cfg);
    contours_to_shapes(&contours, group_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Shape;

    /// A `field`×`field` white image with a black `side`×`side` square at
    /// `(margin, margin)`.
    fn black_square(field: usize, margin: usize, side: usize) -> Vec<u8> {
        let mut buf = vec![255u8; field * field * 4];
        for y in margin..margin + side {
            for x in margin..margin + side {
                let i = (y * field + x) * 4;
                buf[i] = 0;
                buf[i + 1] = 0;
                buf[i + 2] = 0;
                buf[i + 3] = 255;
            }
        }
        buf
    }

    fn bw_cfg() -> ImageTraceConfig {
        ImageTraceConfig {
            mode: ImageTraceMode::BlackWhite,
            threshold: 128,
            paths: 100,
            noise: 0,
            ..ImageTraceConfig::new()
        }
    }

    #[test]
    fn single_square_yields_one_contour() {
        let buf = black_square(40, 8, 24);
        let contours = trace_contours(&buf, 40, 40, &bw_cfg());
        assert_eq!(contours.len(), 1, "one black square ⇒ one contour");
        let c = &contours[0];
        assert!(c.points.len() >= 4, "a rectangle keeps ≥4 corners");
        let bb = c.bbox().expect("bbox");
        // Square spans [8,32) ⇒ origin near 8, side near ~23–24 (boundary cells).
        assert!(bb[0] >= 7.0 && bb[0] <= 9.0, "x0 {:?}", bb);
        assert!(bb[1] >= 7.0 && bb[1] <= 9.0, "y0 {:?}", bb);
        assert!((bb[2] - 23.0).abs() <= 2.0, "w {:?}", bb);
        assert!((bb[3] - 23.0).abs() <= 2.0, "h {:?}", bb);
    }

    #[test]
    fn square_simplifies_to_corner_count() {
        // A clean axis-aligned square should collapse to ~4 anchors after DP.
        let buf = black_square(40, 8, 24);
        let contours = trace_contours(&buf, 40, 40, &bw_cfg());
        let c = &contours[0];
        assert!(
            c.points.len() <= 8,
            "DP should collapse a square to a handful of corners, got {}",
            c.points.len()
        );
    }

    #[test]
    fn two_disjoint_squares_yield_two_contours() {
        // Two black squares on one field ⇒ two connected components.
        let field = 60usize;
        let mut buf = vec![255u8; field * field * 4];
        for (ox, oy) in [(6usize, 6usize), (36, 36)] {
            for y in oy..oy + 14 {
                for x in ox..ox + 14 {
                    let i = (y * field + x) * 4;
                    buf[i] = 0;
                    buf[i + 1] = 0;
                    buf[i + 2] = 0;
                    buf[i + 3] = 255;
                }
            }
        }
        let contours = trace_contours(&buf, field, field, &bw_cfg());
        assert_eq!(contours.len(), 2, "two squares ⇒ two contours");
    }

    #[test]
    fn noise_filter_drops_tiny_speckles() {
        // One big square + a 1px speckle; a high noise threshold drops the speckle.
        let field = 40usize;
        let mut buf = black_square(field, 8, 20);
        let i = (2 * field + 2) * 4; // lone pixel near the corner
        buf[i] = 0;
        buf[i + 1] = 0;
        buf[i + 2] = 0;
        buf[i + 3] = 255;
        let mut cfg = bw_cfg();
        cfg.noise = 20; // perimeter threshold larger than a 1px speckle
        let contours = trace_contours(&buf, field, field, &cfg);
        assert_eq!(contours.len(), 1, "speckle filtered, square kept");
    }

    #[test]
    fn deterministic() {
        let buf = black_square(40, 8, 24);
        let a = trace_contours(&buf, 40, 40, &bw_cfg());
        let b = trace_contours(&buf, 40, 40, &bw_cfg());
        assert_eq!(a, b, "same input ⇒ same contours");
    }

    #[test]
    fn color_mode_traces_each_region() {
        // Two solid colour halves; colour mode should trace at least the
        // non-background half.
        let field = 32usize;
        let mut buf = vec![255u8; field * field * 4];
        for y in 0..field {
            for x in 0..field {
                let i = (y * field + x) * 4;
                if x < field / 2 {
                    buf[i] = 20;
                    buf[i + 1] = 20;
                    buf[i + 2] = 20;
                } else {
                    buf[i] = 200;
                    buf[i + 1] = 200;
                    buf[i + 2] = 200;
                }
                buf[i + 3] = 255;
            }
        }
        let cfg = ImageTraceConfig {
            mode: ImageTraceMode::Color3,
            paths: 100,
            noise: 0,
            ..ImageTraceConfig::new()
        };
        let contours = trace_contours(&buf, field, field, &cfg);
        assert!(!contours.is_empty(), "colour trace finds a region");
    }

    #[test]
    fn empty_image_yields_nothing() {
        assert!(trace_contours(&[], 0, 0, &bw_cfg()).is_empty());
    }

    #[test]
    fn douglas_peucker_collapses_collinear() {
        let line: Vec<(f32, f32)> =
            (0..=10).map(|i| (i as f32, 0.0)).collect();
        let simp = douglas_peucker(&line, 0.1);
        assert_eq!(simp, vec![(0.0, 0.0), (10.0, 0.0)], "collinear ⇒ endpoints");
    }

    #[test]
    fn trace_to_shapes_produces_grouped_closed_paths() {
        let buf = black_square(40, 8, 24);
        let shapes = trace_to_shapes(&buf, 40, 40, &bw_cfg(), 7);
        assert_eq!(shapes.len(), 1, "one square ⇒ one shape");
        match &shapes[0] {
            Shape::Path { closed, group, .. } => {
                assert!(*closed, "traced path is closed");
                assert_eq!(*group, Some(7), "tagged into the group");
            }
            _ => panic!("expected a Path"),
        }
    }
}
