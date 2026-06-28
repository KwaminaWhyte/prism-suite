//! Pure video **scopes** — deterministic measurement of a rendered program
//! frame, mirroring Premiere's Lumetri Scopes panel.
//!
//! Every function here is a pure transform from a straight-sRGB pixel buffer
//! (`&[[f32; 4]]`, row-major, `width` pixels per row) to a small count grid:
//!
//! * [`luma_waveform`] — per-column luma histogram (the classic waveform).
//! * [`rgb_parade`]    — three side-by-side per-column channel waveforms.
//! * [`vectorscope`]   — a 2-D U/V chroma bin grid (neutral sits dead centre).
//! * [`rgb_histogram`] — per-channel + luma value histograms over the frame.
//!
//! Counts are exact: a waveform/parade column sums to the number of rows; a
//! histogram / vectorscope sums to the number of pixels. No floats accumulate so
//! the results are bit-reproducible. Luma uses Rec.601 weights.

/// Rec.601 luma of a straight-sRGB triple.
pub fn luma_of(rgb: [f32; 3]) -> f32 {
    0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]
}

/// Map a `0..=1` value to a bin index in `0..bins` (floor, top clamped).
#[inline]
fn bin_of(v: f32, bins: usize) -> usize {
    if bins == 0 {
        return 0;
    }
    let b = (v.clamp(0.0, 1.0) * bins as f32) as usize;
    b.min(bins - 1)
}

/// A luma waveform: `columns[x][bin]` is the count of pixels in image column `x`
/// whose luma falls in `bin`. Each column sums to the image height.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeWaveform {
    pub columns: Vec<Vec<u32>>,
    pub bins: usize,
}

/// Compute a luma waveform over a straight-sRGB buffer laid out `width` per row.
pub fn luma_waveform(pixels: &[[f32; 4]], width: usize, bins: usize) -> ScopeWaveform {
    let bins = bins.max(1);
    let width = width.max(1);
    let mut columns = vec![vec![0u32; bins]; width];
    for (i, px) in pixels.iter().enumerate() {
        let x = i % width;
        let y = luma_of([px[0], px[1], px[2]]);
        columns[x][bin_of(y, bins)] += 1;
    }
    ScopeWaveform { columns, bins }
}

/// An RGB parade: one per-column histogram per channel (each laid out exactly
/// like [`ScopeWaveform::columns`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeParade {
    pub r: Vec<Vec<u32>>,
    pub g: Vec<Vec<u32>>,
    pub b: Vec<Vec<u32>>,
    pub bins: usize,
}

/// Compute an RGB parade (three channel waveforms) over a buffer.
pub fn rgb_parade(pixels: &[[f32; 4]], width: usize, bins: usize) -> ScopeParade {
    let bins = bins.max(1);
    let width = width.max(1);
    let mut r = vec![vec![0u32; bins]; width];
    let mut g = vec![vec![0u32; bins]; width];
    let mut b = vec![vec![0u32; bins]; width];
    for (i, px) in pixels.iter().enumerate() {
        let x = i % width;
        r[x][bin_of(px[0], bins)] += 1;
        g[x][bin_of(px[1], bins)] += 1;
        b[x][bin_of(px[2], bins)] += 1;
    }
    ScopeParade { r, g, b, bins }
}

/// A vectorscope: a `size × size` row-major grid of chroma (U/V) bin counts.
/// The grid is indexed `bins[vy * size + ux]`; a neutral (grey) pixel lands in
/// the centre bin (`size/2, size/2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeVectorscope {
    pub bins: Vec<u32>,
    pub size: usize,
}

impl ScopeVectorscope {
    /// Count at grid cell `(ux, vy)`.
    pub fn at(&self, ux: usize, vy: usize) -> u32 {
        self.bins[vy * self.size + ux]
    }
    /// The centre cell index along one axis (where neutral lands).
    pub fn center(&self) -> usize {
        self.size / 2
    }
}

/// Map a normalized `[-1, 1]` chroma coordinate to a bin index in `0..size`.
#[inline]
fn axis_bin(coord_norm: f32, size: usize) -> usize {
    let t = (coord_norm * 0.5 + 0.5).clamp(0.0, 0.999_999);
    ((t * size as f32) as usize).min(size - 1)
}

/// Compute a vectorscope (U/V chroma scatter) over a buffer. `size` is the grid
/// resolution per axis. Chroma uses ITU-R BT.601 U/V, normalized so the larger
/// (V) axis fills the grid; a grey pixel maps to U = V = 0 → the centre cell.
pub fn vectorscope(pixels: &[[f32; 4]], size: usize) -> ScopeVectorscope {
    let size = size.max(1);
    let mut bins = vec![0u32; size * size];
    // BT.601 U/V coefficients. |U| ≤ 0.436, |V| ≤ 0.615.
    const NORM: f32 = 0.615;
    for px in pixels {
        let (r, g, b) = (px[0], px[1], px[2]);
        let u = -0.14713 * r - 0.28886 * g + 0.436 * b;
        let v = 0.615 * r - 0.51499 * g - 0.10001 * b;
        let ux = axis_bin(u / NORM, size);
        let vy = axis_bin(v / NORM, size);
        bins[vy * size + ux] += 1;
    }
    ScopeVectorscope { bins, size }
}

/// Per-channel + luma value histograms over the whole frame. Each vector has
/// `bins` entries and sums to the pixel count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeHistogram {
    pub r: Vec<u32>,
    pub g: Vec<u32>,
    pub b: Vec<u32>,
    pub luma: Vec<u32>,
    pub bins: usize,
}

/// Compute RGB + luma histograms over a straight-sRGB buffer.
pub fn rgb_histogram(pixels: &[[f32; 4]], bins: usize) -> ScopeHistogram {
    let bins = bins.max(1);
    let mut r = vec![0u32; bins];
    let mut g = vec![0u32; bins];
    let mut b = vec![0u32; bins];
    let mut luma = vec![0u32; bins];
    for px in pixels {
        r[bin_of(px[0], bins)] += 1;
        g[bin_of(px[1], bins)] += 1;
        b[bin_of(px[2], bins)] += 1;
        luma[bin_of(luma_of([px[0], px[1], px[2]]), bins)] += 1;
    }
    ScopeHistogram { r, g, b, luma, bins }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `width × height` buffer from a per-pixel closure.
    fn buf(width: usize, height: usize, f: impl Fn(usize, usize) -> [f32; 4]) -> Vec<[f32; 4]> {
        let mut v = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                v.push(f(x, y));
            }
        }
        v
    }

    #[test]
    fn luma_of_neutral_is_value() {
        // A grey pixel's luma equals its value (weights sum to 1).
        assert!((luma_of([0.5, 0.5, 0.5]) - 0.5).abs() < 1e-6);
        assert!((luma_of([1.0, 1.0, 1.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn waveform_columns_sum_to_height() {
        let (w, h) = (8, 5);
        let pixels = buf(w, h, |x, _| {
            let v = x as f32 / (w as f32 - 1.0);
            [v, v, v, 1.0]
        });
        let wf = luma_waveform(&pixels, w, 16);
        assert_eq!(wf.columns.len(), w);
        for col in &wf.columns {
            assert_eq!(col.iter().sum::<u32>(), h as u32);
        }
    }

    #[test]
    fn waveform_total_equals_pixel_count() {
        let (w, h) = (6, 6);
        let pixels = buf(w, h, |_, y| {
            let v = y as f32 / (h as f32 - 1.0);
            [v, v, v, 1.0]
        });
        let wf = luma_waveform(&pixels, w, 32);
        let total: u32 = wf.columns.iter().flatten().sum();
        assert_eq!(total, (w * h) as u32);
    }

    #[test]
    fn parade_channel_totals_equal_pixel_count() {
        let (w, h) = (4, 4);
        let pixels = buf(w, h, |x, y| [x as f32 / 3.0, y as f32 / 3.0, 0.25, 1.0]);
        let p = rgb_parade(&pixels, w, 16);
        let n = (w * h) as u32;
        assert_eq!(p.r.iter().flatten().sum::<u32>(), n);
        assert_eq!(p.g.iter().flatten().sum::<u32>(), n);
        assert_eq!(p.b.iter().flatten().sum::<u32>(), n);
        // The constant blue channel collapses into a single bin per column.
        for col in &p.b {
            assert_eq!(col.iter().filter(|&&c| c > 0).count(), 1);
        }
    }

    #[test]
    fn parade_red_ramp_spreads_across_bins() {
        // A horizontal red ramp should hit many distinct red bins.
        let (w, h) = (16, 2);
        let pixels = buf(w, h, |x, _| [x as f32 / (w as f32 - 1.0), 0.0, 0.0, 1.0]);
        let p = rgb_parade(&pixels, w, 16);
        // Column 0 = black → bin 0; last column = full red → top bin.
        assert!(p.r[0][0] > 0);
        assert!(*p.r[w - 1].last().unwrap() > 0);
    }

    #[test]
    fn vectorscope_neutral_centers_at_origin() {
        let pixels = buf(4, 4, |_, _| [0.4, 0.4, 0.4, 1.0]);
        let vs = vectorscope(&pixels, 16);
        let c = vs.center();
        // All grey → every pixel in the centre cell.
        assert_eq!(vs.at(c, c), 16);
        let total: u32 = vs.bins.iter().sum();
        assert_eq!(vs.at(c, c), total);
    }

    #[test]
    fn vectorscope_total_equals_pixel_count() {
        let (w, h) = (5, 3);
        let pixels = buf(w, h, |x, y| {
            [x as f32 / 4.0, 1.0 - y as f32 / 2.0, 0.5, 1.0]
        });
        let vs = vectorscope(&pixels, 32);
        assert_eq!(vs.bins.iter().sum::<u32>(), (w * h) as u32);
    }

    #[test]
    fn vectorscope_saturated_red_is_off_center() {
        let pixels = buf(2, 2, |_, _| [1.0, 0.0, 0.0, 1.0]);
        let vs = vectorscope(&pixels, 16);
        let c = vs.center();
        // Pure red has positive V (red-difference) and negative U → not centre.
        assert_eq!(vs.at(c, c), 0);
        assert_eq!(vs.bins.iter().sum::<u32>(), 4);
    }

    #[test]
    fn histogram_bins_sum_to_pixel_count() {
        let (w, h) = (7, 5);
        let pixels = buf(w, h, |x, y| {
            [x as f32 / 6.0, y as f32 / 4.0, 0.3, 1.0]
        });
        let hg = rgb_histogram(&pixels, 64);
        let n = (w * h) as u32;
        assert_eq!(hg.r.iter().sum::<u32>(), n);
        assert_eq!(hg.g.iter().sum::<u32>(), n);
        assert_eq!(hg.b.iter().sum::<u32>(), n);
        assert_eq!(hg.luma.iter().sum::<u32>(), n);
    }

    #[test]
    fn histogram_white_pixels_in_top_bin() {
        let pixels = buf(3, 3, |_, _| [1.0, 1.0, 1.0, 1.0]);
        let hg = rgb_histogram(&pixels, 8);
        assert_eq!(hg.r[7], 9);
        assert_eq!(hg.luma[7], 9);
        // Nothing anywhere else.
        assert_eq!(hg.r[..7].iter().sum::<u32>(), 0);
    }

    #[test]
    fn histogram_black_pixels_in_bottom_bin() {
        let pixels = buf(3, 3, |_, _| [0.0, 0.0, 0.0, 1.0]);
        let hg = rgb_histogram(&pixels, 8);
        assert_eq!(hg.r[0], 9);
        assert_eq!(hg.luma[0], 9);
    }
}
