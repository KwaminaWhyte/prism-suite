//! Lens correction: barrel/pincushion radial distortion + vignette.

pub fn apply_lens_correction(
    pixels: &[f32], w: u32, h: u32,
    barrel: f32, pincushion: f32, vignette: f32,
) -> Vec<f32> {
    let (fw, fh) = (w as f32, h as f32);
    let cx = fw * 0.5;
    let cy = fh * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt().max(1e-6);
    let mut out = vec![0.0f32; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let r = (dx * dx + dy * dy).sqrt();
            let r_norm = r / max_r;
            let r_norm2 = r_norm * r_norm;
            let r_prime = r_norm * (1.0 + barrel * r_norm2 + pincushion * r_norm2 * r_norm2);
            let scale = if r_norm > 1e-6 { r_prime / r_norm } else { 1.0 };
            let src_x = cx + dx * scale;
            let src_y = cy + dy * scale;
            let s = bilinear(pixels, w, h, src_x - 0.5, src_y - 0.5);
            let vig = (1.0 - vignette * r_norm2).max(0.0);
            let i = ((y * w + x) * 4) as usize;
            out[i]     = s[0] * vig;
            out[i + 1] = s[1] * vig;
            out[i + 2] = s[2] * vig;
            out[i + 3] = s[3];
        }
    }
    out
}

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
            for c in 0..4 { out[c] += px[i + c] * wb; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, c: [f32; 4]) -> Vec<f32> {
        (0..w * h).flat_map(|_| c).collect()
    }

    #[test]
    fn zero_params_is_identity() {
        let p = flat(8, 8, [0.4, 0.6, 0.2, 1.0]);
        let out = apply_lens_correction(&p, 8, 8, 0.0, 0.0, 0.0);
        for (a, b) in out.iter().zip(p.iter()) {
            assert!((a - b).abs() < 1e-3, "pixel drifted: {a} vs {b}");
        }
    }

    #[test]
    fn flat_field_barrel_preserved() {
        // A uniform field remapped by barrel distortion still returns the same colour.
        let p = flat(12, 12, [0.3, 0.5, 0.7, 1.0]);
        let out = apply_lens_correction(&p, 12, 12, 0.3, 0.0, 0.0);
        for px in out.chunks(4) {
            assert!((px[0] - 0.3).abs() < 1e-2, "r drifted: {}", px[0]);
            assert!((px[3] - 1.0).abs() < 1e-3, "alpha changed");
        }
    }

    #[test]
    fn flat_field_pincushion_preserved() {
        let p = flat(12, 12, [0.8, 0.2, 0.4, 1.0]);
        let out = apply_lens_correction(&p, 12, 12, 0.0, 0.2, 0.0);
        for px in out.chunks(4) {
            assert!((px[0] - 0.8).abs() < 1e-2, "r drifted: {}", px[0]);
        }
    }

    #[test]
    fn vignette_darkens_corners_more_than_center() {
        let p = flat(16, 16, [1.0, 1.0, 1.0, 1.0]);
        let out = apply_lens_correction(&p, 16, 16, 0.0, 0.0, 1.0);
        // Centre pixel (7,7) should be brighter than the top-left corner (0,0).
        let centre_r = out[((7 * 16 + 7) * 4) as usize];
        let corner_r = out[0];
        assert!(centre_r > corner_r, "centre={centre_r} corner={corner_r}");
    }

    #[test]
    fn vignette_does_not_touch_alpha() {
        let p = flat(8, 8, [1.0, 1.0, 1.0, 0.7]);
        let out = apply_lens_correction(&p, 8, 8, 0.0, 0.0, 1.0);
        // Alpha channel must be unchanged — vignette only scales RGB.
        for px in out.chunks(4) {
            assert!((px[3] - 0.7).abs() < 1e-4, "alpha changed: {}", px[3]);
        }
    }

    #[test]
    fn vignette_one_dims_corners_significantly() {
        // vignette = 1.0 → pixel centres at the corner land at r_norm ≈ 0.94
        // (not exactly 1.0 since pixel centres don't reach the image diagonal),
        // giving vig ≈ 1 - 0.88 ≈ 0.12 — well below the centre value of 1.0.
        let p = flat(16, 16, [1.0, 1.0, 1.0, 1.0]);
        let out = apply_lens_correction(&p, 16, 16, 0.0, 0.0, 1.0);
        let corner_r = out[0];
        assert!(corner_r < 0.2, "corner not dimmed: {corner_r}");
    }
}
