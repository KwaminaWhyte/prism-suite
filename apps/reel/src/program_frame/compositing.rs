//! Frame-buffer compositing + transition primitives split out of
//! `program_frame/mod.rs` (file-size rule): aspect-fit, flat fills, the
//! transition geometry helpers (reveal-clip / pixel-mask / translate), the
//! bottom-first track fold (source-over), and the flatten-over-black pass. All
//! operate on straight-sRGB RGBA8 buffers. Re-exported from the parent module.

/// An opaque black `w*h*4` RGBA layer (alpha 255). The nested-sequence backdrop
/// and the depth-guard fallback.
pub(crate) fn black_layer(w: u32, h: u32) -> Vec<u8> {
    let n = (w as usize) * (h as usize);
    let mut buf = Vec::with_capacity(n * 4);
    for _ in 0..n {
        buf.extend_from_slice(&[0, 0, 0, 255]);
    }
    buf
}

/// Aspect-fit a source RGBA8 image into a `dst_w`x`dst_h` frame (nearest
/// sample), centered, with transparent (alpha 0) letterbox bars so the comp
/// black shows through after flatten.
pub(crate) fn aspect_fit(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    if src_w == 0 || src_h == 0 {
        return out;
    }
    let scale = (dst_w as f32 / src_w as f32).min(dst_h as f32 / src_h as f32);
    let fit_w = (src_w as f32 * scale).round().max(1.0) as u32;
    let fit_h = (src_h as f32 * scale).round().max(1.0) as u32;
    let off_x = (dst_w - fit_w.min(dst_w)) / 2;
    let off_y = (dst_h - fit_h.min(dst_h)) / 2;

    for y in 0..fit_h.min(dst_h) {
        let sy = ((y as f32 + 0.5) / scale).floor() as u32;
        let sy = sy.min(src_h - 1);
        for x in 0..fit_w.min(dst_w) {
            let sx = ((x as f32 + 0.5) / scale).floor() as u32;
            let sx = sx.min(src_w - 1);
            let si = ((sy * src_w + sx) * 4) as usize;
            let di = (((y + off_y) * dst_w + (x + off_x)) * 4) as usize;
            if si + 4 <= src.len() && di + 4 <= out.len() {
                out[di] = src[si];
                out[di + 1] = src[si + 1];
                out[di + 2] = src[si + 2];
                out[di + 3] = src[si + 3];
            }
        }
    }
    out
}

/// A flat straight-sRGB color fill covering the whole `w*h` frame (alpha 255).
/// Used as the dip-to-color overlay in the transition pass.
pub(crate) fn color_fill(color: [f32; 4], w: u32, h: u32) -> Vec<u8> {
    let px = [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255u8,
    ];
    let n = (w as usize) * (h as usize);
    let mut buf = Vec::with_capacity(n * 4);
    for _ in 0..n {
        buf.extend_from_slice(&px);
    }
    buf
}

/// Knock out (set alpha 0) every pixel of `buf` *outside* the `reveal`
/// fraction-rect `(x0, y0, x1, y1)` (each `0..=1` of the frame). The pixels
/// inside the rect keep their coverage so a wipe shows the incoming clip only in
/// the swept-in region. Mirrors the egui app's wipe clip.
pub(crate) fn clip_to_reveal(buf: &mut [u8], w: u32, h: u32, reveal: (f32, f32, f32, f32)) {
    let (x0, y0, x1, y1) = reveal;
    let px0 = (x0.clamp(0.0, 1.0) * w as f32).round() as u32;
    let px1 = (x1.clamp(0.0, 1.0) * w as f32).round() as u32;
    let py0 = (y0.clamp(0.0, 1.0) * h as f32).round() as u32;
    let py1 = (y1.clamp(0.0, 1.0) * h as f32).round() as u32;
    for y in 0..h {
        for x in 0..w {
            let inside = x >= px0 && x < px1 && y >= py0 && y < py1;
            if !inside {
                let i = ((y * w + x) * 4 + 3) as usize;
                if i < buf.len() {
                    buf[i] = 0;
                }
            }
        }
    }
}

/// Multiply each pixel's alpha by the corresponding byte in `mask` (0=transparent,
/// 255=opaque). Used for iris-circle and clock-wipe transitions.
pub(crate) fn apply_pixel_mask(buf: &mut [u8], mask: &[u8]) {
    for (i, &m) in mask.iter().enumerate() {
        let ai = i * 4 + 3;
        if ai < buf.len() {
            buf[ai] = ((buf[ai] as u16 * m as u16) / 255) as u8;
        }
    }
}

/// Apply a pixel-shift translate to a `w*h*4` RGBA buffer by `(dx_frac, dy_frac)`
/// fractions of the frame size (positive = shift right/down). Pixels that shift
/// out of bounds become transparent (alpha 0). Returns a new buffer.
pub(crate) fn translate_frame(buf: &[u8], w: u32, h: u32, dx_frac: f32, dy_frac: f32) -> Vec<u8> {
    let dx = (dx_frac * w as f32).round() as i32;
    let dy = (dy_frac * h as f32).round() as i32;
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx < 0 || sx >= w as i32 || sy < 0 || sy >= h as i32 {
                continue;
            }
            let si = ((sy as u32 * w + sx as u32) * 4) as usize;
            let di = ((y as u32 * w + x as u32) * 4) as usize;
            if si + 4 <= buf.len() && di + 4 <= out.len() {
                out[di] = buf[si];
                out[di + 1] = buf[si + 1];
                out[di + 2] = buf[si + 2];
                out[di + 3] = buf[si + 3];
            }
        }
    }
    out
}

/// Fold the track layers (bottom first) into one premultiplied-ish straight
/// RGBA buffer with the source-over operator, scaling each layer's alpha by its
/// clip opacity. The accumulator starts fully transparent.
pub(crate) fn fold_tracks(w: u32, h: u32, layers: &[(Vec<u8>, f32)]) -> Vec<u8> {
    let n = (w as usize) * (h as usize);
    let mut acc = vec![0u8; n * 4];
    for (fg, opacity) in layers {
        over_in_place(&mut acc, fg, *opacity);
    }
    acc
}

/// Composite `fg` (scaled by `opacity`) over `acc` in place, straight-alpha
/// source-over, per pixel: `out = fg*a + acc*(1-a)` where `a = fg_alpha*opacity`.
fn over_in_place(acc: &mut [u8], fg: &[u8], opacity: f32) {
    for (a, f) in acc.chunks_exact_mut(4).zip(fg.chunks_exact(4)) {
        let fa = (f[3] as f32 / 255.0) * opacity;
        if fa <= 0.0 {
            continue;
        }
        let inv = 1.0 - fa;
        for c in 0..3 {
            a[c] = (f[c] as f32 * fa + a[c] as f32 * inv).round().clamp(0.0, 255.0) as u8;
        }
        // Straight-over alpha accumulation.
        let out_a = fa + (a[3] as f32 / 255.0) * inv;
        a[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

/// Flatten a straight-alpha RGBA buffer over an opaque black backdrop: scale RGB
/// by alpha and force alpha to 255 (the program output is opaque).
pub(crate) fn flatten_over_black(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        let a = px[3] as f32 / 255.0;
        for c in 0..3 {
            px[c] = (px[c] as f32 * a).round().clamp(0.0, 255.0) as u8;
        }
        px[3] = 255;
    }
}
