//! CPU PatchMatch content-aware fill.

const PATCH: usize = 8;

struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self { Self(seed) }
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (u32::MAX as f32)
    }
    fn gen_range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        let range = (hi - lo) as u32;
        if range == 0 { return lo; }
        lo + (self.next_f32() * range as f32) as i32
    }
}

fn sample(px: &[f32], w: u32, h: u32, x: f32, y: f32) -> [f32; 4] {
    let (wi, hi) = (w as i32, h as i32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; 4];
    for dy in 0..2i32 {
        for dx in 0..2i32 {
            let sx = x0 + dx;
            let sy = y0 + dy;
            if sx < 0 || sy < 0 || sx >= wi || sy >= hi { continue; }
            let w_bi = (if dx == 0 { 1.0 - tx } else { tx }) * (if dy == 0 { 1.0 - ty } else { ty });
            let i = ((sy as u32 * w + sx as u32) * 4) as usize;
            for c in 0..4 { out[c] += px[i + c] * w_bi; }
        }
    }
    out
}

fn patch_dist(
    src: &[f32], w: u32, h: u32, mask: &[f32],
    ax: i32, ay: i32, ox: i32, oy: i32,
) -> f32 {
    let (wi, hi) = (w as i32, h as i32);
    let half = (PATCH / 2) as i32;
    let mut dist = 0.0f32;
    let mut n = 0u32;
    for dy in -half..half {
        for dx in -half..half {
            let (tx, ty) = (ax + dx, ay + dy);
            if tx < 0 || ty < 0 || tx >= wi || ty >= hi { continue; }
            if mask[(ty as u32 * w + tx as u32) as usize] > 0.5 { continue; }
            let (sx, sy) = (tx + ox, ty + oy);
            if sx < 0 || sy < 0 || sx >= wi || sy >= hi { continue; }
            let ti = ((ty as u32 * w + tx as u32) * 4) as usize;
            let si = ((sy as u32 * w + sx as u32) * 4) as usize;
            for c in 0..3 {
                let d = src[ti + c] - src[si + c];
                dist += d * d;
            }
            n += 1;
        }
    }
    if n == 0 { f32::MAX } else { dist / n as f32 }
}

/// Content-aware fill using PatchMatch. Fills all pixels where `mask[i] > 0.5`.
pub fn content_aware_fill(pixels: &[f32], w: u32, h: u32, mask: &[f32]) -> Vec<f32> {
    let mut bx0 = w as i32;
    let mut by0 = h as i32;
    let mut bx1 = 0i32;
    let mut by1 = 0i32;
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] > 0.5 {
                bx0 = bx0.min(x as i32);
                by0 = by0.min(y as i32);
                bx1 = bx1.max(x as i32 + 1);
                by1 = by1.max(y as i32 + 1);
            }
        }
    }
    if bx1 <= bx0 || by1 <= by0 {
        return pixels.to_vec();
    }
    let margin = 64i32;
    let rx0 = (bx0 - margin).max(0) as u32;
    let ry0 = (by0 - margin).max(0) as u32;
    let rx1 = (bx1 + margin).min(w as i32) as u32;
    let ry1 = (by1 + margin).min(h as i32) as u32;
    let rw = rx1 - rx0;
    let rh = ry1 - ry0;

    let scale = if rw > 512 || rh > 512 {
        let sx = rw as f32 / 512.0;
        let sy = rh as f32 / 512.0;
        sx.max(sy)
    } else {
        1.0f32
    };
    let sw = ((rw as f32 / scale).round() as u32).max(1);
    let sh = ((rh as f32 / scale).round() as u32).max(1);

    let mut sub_px = vec![0.0f32; (sw * sh * 4) as usize];
    for dy in 0..sh {
        for dx in 0..sw {
            let spx = rx0 as f32 + dx as f32 * scale;
            let spy = ry0 as f32 + dy as f32 * scale;
            let s = sample(pixels, w, h, spx, spy);
            let i = ((dy * sw + dx) * 4) as usize;
            sub_px[i..i + 4].copy_from_slice(&s);
        }
    }
    let sub_mask: Vec<f32> = (0..sh).flat_map(|dy| (0..sw).map(move |dx| {
        let spx = ((rx0 as f32 + dx as f32 * scale) as u32).min(w - 1);
        let spy = ((ry0 as f32 + dy as f32 * scale) as u32).min(h - 1);
        mask[(spy * w + spx) as usize]
    })).collect();

    let filled = patchmatch(&sub_px, sw, sh, &sub_mask);

    let mut out = pixels.to_vec();
    for dy in 0..sh {
        for dx in 0..sw {
            let si = ((dy * sw + dx) * 4) as usize;
            let ox = rx0 as f32 + dx as f32 * scale;
            let oy = ry0 as f32 + dy as f32 * scale;
            let ox0 = ox.floor() as u32;
            let oy0 = oy.floor() as u32;
            let ceil_scale = scale.ceil() as u32;
            for fy in 0..ceil_scale {
                for fx in 0..ceil_scale {
                    let px_x = (ox0 + fx).min(w - 1);
                    let px_y = (oy0 + fy).min(h - 1);
                    if mask[(px_y * w + px_x) as usize] > 0.5 {
                        let di = ((px_y * w + px_x) * 4) as usize;
                        out[di..di + 4].copy_from_slice(&filled[si..si + 4]);
                    }
                }
            }
        }
    }
    out
}

fn patchmatch(pixels: &[f32], w: u32, h: u32, mask: &[f32]) -> Vec<f32> {
    let n = (w * h) as usize;
    let mut offsets: Vec<(i32, i32)> = vec![(0, 0); n];
    let mut best_dist: Vec<f32> = vec![f32::MAX; n];
    let mut rng = Lcg::new(0xdeadbeef_cafebabe);
    let (wi, hi) = (w as i32, h as i32);

    for y in 0..hi {
        for x in 0..wi {
            let idx = (y as u32 * w + x as u32) as usize;
            if mask[idx] > 0.5 {
                let ox = rng.gen_range_i32(-wi, wi);
                let oy = rng.gen_range_i32(-hi, hi);
                offsets[idx] = (ox, oy);
                best_dist[idx] = patch_dist(pixels, w, h, mask, x, y, ox, oy);
            }
        }
    }

    for _round in 0..5 {
        for y in 0..hi {
            for x in 0..wi {
                let idx = (y as u32 * w + x as u32) as usize;
                if mask[idx] <= 0.5 { continue; }
                let mut best = offsets[idx];
                let mut bd = best_dist[idx];
                if x > 0 {
                    let ni = (y as u32 * w + (x - 1) as u32) as usize;
                    if mask[ni] > 0.5 {
                        let (nox, noy) = offsets[ni];
                        let d = patch_dist(pixels, w, h, mask, x, y, nox, noy);
                        if d < bd { best = (nox, noy); bd = d; }
                    }
                }
                if y > 0 {
                    let ni = ((y - 1) as u32 * w + x as u32) as usize;
                    if mask[ni] > 0.5 {
                        let (nox, noy) = offsets[ni];
                        let d = patch_dist(pixels, w, h, mask, x, y, nox, noy);
                        if d < bd { best = (nox, noy); bd = d; }
                    }
                }
                let mut radius = wi.max(hi) as f32;
                while radius >= 1.0 {
                    let r = radius as i32;
                    let try_ox = best.0 + rng.gen_range_i32(-r, r + 1);
                    let try_oy = best.1 + rng.gen_range_i32(-r, r + 1);
                    let d = patch_dist(pixels, w, h, mask, x, y, try_ox, try_oy);
                    if d < bd { best = (try_ox, try_oy); bd = d; }
                    radius *= 0.5;
                }
                offsets[idx] = best;
                best_dist[idx] = bd;
            }
        }
    }

    let mut result = pixels.to_vec();
    for y in 0..hi {
        for x in 0..wi {
            let idx = (y as u32 * w + x as u32) as usize;
            if mask[idx] <= 0.5 { continue; }
            let mut candidates: Vec<((i32, i32), f32)> = Vec::new();
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= wi || ny >= hi { continue; }
                    let ni = (ny as u32 * w + nx as u32) as usize;
                    if mask[ni] > 0.5 {
                        let off = offsets[ni];
                        let d = patch_dist(pixels, w, h, mask, x, y, off.0, off.1);
                        candidates.push((off, d));
                    }
                }
            }
            candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(3);
            if candidates.is_empty() { continue; }
            let mut rgba = [0.0f32; 4];
            let mut total_w = 0.0f32;
            for (off, d) in &candidates {
                let w_inv = 1.0 / (d + 1e-6);
                let sx = x + off.0;
                let sy = y + off.1;
                let s = sample(pixels, w, h, sx as f32, sy as f32);
                for c in 0..4 { rgba[c] += s[c] * w_inv; }
                total_w += w_inv;
            }
            if total_w > 0.0 {
                let i = idx * 4;
                for c in 0..4 { result[i + c] = rgba[c] / total_w; }
            }
        }
    }
    result
}
