//! Text / glyph rasterization + the S-Log2→Rec.709 transfer, split out of
//! `program_frame/mod.rs` (file-size rule): the embedded 5×7 bitmap font, the
//! caption-cue burn-in, the title-clip rasterizer, and the per-track log→Rec.709
//! gamma. Re-exported from the parent module so the compositor keeps calling
//! them unqualified.

use crate::app_state::{Caption, CaptionPosition};

/// Minimal 5×7 pixel bitmap font for ASCII 32–126 (95 printable chars).
/// Each character is 5 bytes, one per column (left→right).
/// Each byte encodes rows top (bit 0) to bottom (bit 6).
#[rustfmt::skip]
pub(crate) const FONT_5X7: [[u8; 5]; 95] = [
    [0x00,0x00,0x00,0x00,0x00], // 32  (space)
    [0x00,0x00,0x5F,0x00,0x00], // 33  !
    [0x00,0x07,0x00,0x07,0x00], // 34  "
    [0x14,0x7F,0x14,0x7F,0x14], // 35  #
    [0x24,0x2A,0x7F,0x2A,0x12], // 36  $
    [0x23,0x13,0x08,0x64,0x62], // 37  %
    [0x36,0x49,0x56,0x20,0x50], // 38  &
    [0x00,0x06,0x07,0x00,0x00], // 39  '
    [0x00,0x1C,0x22,0x41,0x00], // 40  (
    [0x00,0x41,0x22,0x1C,0x00], // 41  )
    [0x2A,0x1C,0x7F,0x1C,0x2A], // 42  *
    [0x08,0x08,0x3E,0x08,0x08], // 43  +
    [0x00,0x50,0x30,0x00,0x00], // 44  ,
    [0x08,0x08,0x08,0x08,0x08], // 45  -
    [0x00,0x60,0x60,0x00,0x00], // 46  .
    [0x20,0x10,0x08,0x04,0x02], // 47  /
    [0x3E,0x51,0x49,0x45,0x3E], // 48  0
    [0x00,0x42,0x7F,0x40,0x00], // 49  1
    [0x42,0x61,0x51,0x49,0x46], // 50  2
    [0x21,0x41,0x45,0x4B,0x31], // 51  3
    [0x18,0x14,0x12,0x7F,0x10], // 52  4
    [0x27,0x45,0x45,0x45,0x39], // 53  5
    [0x3C,0x4A,0x49,0x49,0x30], // 54  6
    [0x01,0x71,0x09,0x05,0x03], // 55  7
    [0x36,0x49,0x49,0x49,0x36], // 56  8
    [0x06,0x49,0x49,0x29,0x1E], // 57  9
    [0x00,0x36,0x36,0x00,0x00], // 58  :
    [0x00,0x56,0x36,0x00,0x00], // 59  ;
    [0x08,0x14,0x22,0x41,0x00], // 60  <
    [0x14,0x14,0x14,0x14,0x14], // 61  =
    [0x00,0x41,0x22,0x14,0x08], // 62  >
    [0x02,0x01,0x51,0x09,0x06], // 63  ?
    [0x32,0x49,0x79,0x41,0x3E], // 64  @
    [0x7E,0x11,0x11,0x11,0x7E], // 65  A
    [0x7F,0x49,0x49,0x49,0x36], // 66  B
    [0x3E,0x41,0x41,0x41,0x22], // 67  C
    [0x7F,0x41,0x41,0x22,0x1C], // 68  D
    [0x7F,0x49,0x49,0x49,0x41], // 69  E
    [0x7F,0x09,0x09,0x09,0x01], // 70  F
    [0x3E,0x41,0x49,0x49,0x7A], // 71  G
    [0x7F,0x08,0x08,0x08,0x7F], // 72  H
    [0x00,0x41,0x7F,0x41,0x00], // 73  I
    [0x20,0x40,0x41,0x3F,0x01], // 74  J
    [0x7F,0x08,0x14,0x22,0x41], // 75  K
    [0x7F,0x40,0x40,0x40,0x40], // 76  L
    [0x7F,0x02,0x04,0x02,0x7F], // 77  M
    [0x7F,0x04,0x08,0x10,0x7F], // 78  N
    [0x3E,0x41,0x41,0x41,0x3E], // 79  O
    [0x7F,0x09,0x09,0x09,0x06], // 80  P
    [0x3E,0x41,0x51,0x21,0x5E], // 81  Q
    [0x7F,0x09,0x19,0x29,0x46], // 82  R
    [0x46,0x49,0x49,0x49,0x31], // 83  S
    [0x01,0x01,0x7F,0x01,0x01], // 84  T
    [0x3F,0x40,0x40,0x40,0x3F], // 85  U
    [0x1F,0x20,0x40,0x20,0x1F], // 86  V
    [0x3F,0x40,0x38,0x40,0x3F], // 87  W
    [0x63,0x14,0x08,0x14,0x63], // 88  X
    [0x07,0x08,0x70,0x08,0x07], // 89  Y
    [0x61,0x51,0x49,0x45,0x43], // 90  Z
    [0x00,0x7F,0x41,0x41,0x00], // 91  [
    [0x02,0x04,0x08,0x10,0x20], // 92  backslash
    [0x00,0x41,0x41,0x7F,0x00], // 93  ]
    [0x04,0x02,0x01,0x02,0x04], // 94  ^
    [0x40,0x40,0x40,0x40,0x40], // 95  _
    [0x00,0x01,0x02,0x04,0x00], // 96  `
    [0x20,0x54,0x54,0x54,0x78], // 97  a
    [0x7F,0x48,0x44,0x44,0x38], // 98  b
    [0x38,0x44,0x44,0x44,0x20], // 99  c
    [0x38,0x44,0x44,0x48,0x7F], // 100 d
    [0x38,0x54,0x54,0x54,0x18], // 101 e
    [0x08,0x7E,0x09,0x01,0x02], // 102 f
    [0x0C,0x52,0x52,0x52,0x3E], // 103 g
    [0x7F,0x08,0x04,0x04,0x78], // 104 h
    [0x00,0x44,0x7D,0x40,0x00], // 105 i
    [0x20,0x40,0x44,0x3D,0x00], // 106 j
    [0x7F,0x10,0x28,0x44,0x00], // 107 k
    [0x00,0x41,0x7F,0x40,0x00], // 108 l
    [0x7C,0x04,0x18,0x04,0x78], // 109 m
    [0x7C,0x08,0x04,0x04,0x78], // 110 n
    [0x38,0x44,0x44,0x44,0x38], // 111 o
    [0x7C,0x14,0x14,0x14,0x08], // 112 p
    [0x08,0x14,0x14,0x18,0x7C], // 113 q
    [0x7C,0x08,0x04,0x04,0x08], // 114 r
    [0x48,0x54,0x54,0x54,0x20], // 115 s
    [0x04,0x3F,0x44,0x40,0x20], // 116 t
    [0x3C,0x40,0x40,0x20,0x7C], // 117 u
    [0x1C,0x20,0x40,0x20,0x1C], // 118 v
    [0x3C,0x40,0x30,0x40,0x3C], // 119 w
    [0x44,0x28,0x10,0x28,0x44], // 120 x
    [0x0C,0x50,0x50,0x50,0x3C], // 121 y
    [0x44,0x64,0x54,0x4C,0x44], // 122 z
    [0x00,0x08,0x36,0x41,0x00], // 123 {
    [0x00,0x00,0x7F,0x00,0x00], // 124 |
    [0x00,0x41,0x36,0x08,0x00], // 125 }
    [0x10,0x08,0x08,0x10,0x08], // 126 ~
];

/// Burn the caption active at timeline time `t` (the last one whose
/// `[start, end)` covers `t` wins on overlap) into the composited frame, using
/// the embedded 5×7 font, the cue's [`CaptionStyle`] color, and an optional
/// translucent box behind the text per the style. Position is bottom / top /
/// custom (normalized x,y of the text block's top-left). A no-op when no cue is
/// active.
pub(crate) fn draw_active_caption(buf: &mut [u8], w: u32, h: u32, t: f32, captions: &[Caption]) {
    let Some(cap) = captions
        .iter()
        .rev()
        .find(|c| t >= c.start_secs && t < c.end_secs && !c.text.trim().is_empty())
    else {
        return;
    };
    // Glyph scale from the style font size, relative to the frame height so a cue
    // reads the same fraction of the picture at any comp resolution.
    let px_size = (cap.style.font_size / 720.0 * h as f32).max(7.0);
    let scale = ((px_size / 7.0).round() as usize).max(1);
    let char_w = 5 * scale + scale;
    let char_h = 7 * scale;

    // Parse styled inline tags (<b>/<i>/<u>/<font color>) into runs, then flatten
    // to per-character (glyph, color) cells so the rendered caption shows the
    // tag-stripped text with any per-run color overrides. Newlines split lines.
    let cap_color = [
        (cap.style.color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (cap.style.color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (cap.style.color[2].clamp(0.0, 1.0) * 255.0) as u8,
        (cap.style.color[3].clamp(0.0, 1.0) * 255.0) as u8,
    ];
    let runs = crate::app_state::parse_inline_tags(&cap.text);
    let mut styled_lines: Vec<Vec<(char, [u8; 4])>> = vec![Vec::new()];
    for run in &runs {
        let col = run.color.map(|c| [
            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
            255u8,
        ]).unwrap_or(cap_color);
        for ch in run.text.chars() {
            if ch == '\n' {
                styled_lines.push(Vec::new());
            } else {
                styled_lines.last_mut().unwrap().push((ch, col));
            }
        }
    }
    let lines = styled_lines;
    let block_h = lines.len() * char_h + lines.len().saturating_sub(1) * scale;
    let widest = lines
        .iter()
        .map(|l| l.len())
        .max()
        .unwrap_or(0);
    let block_w = if widest > 0 { widest * char_w - scale } else { 0 };

    // Block top-left in pixels per the position.
    let margin = (h as f32 * 0.06) as usize;
    let (bx, by) = match cap.style.position {
        CaptionPosition::Bottom => (
            (w as usize).saturating_sub(block_w) / 2,
            (h as usize).saturating_sub(block_h + margin)),
        CaptionPosition::Top => ((w as usize).saturating_sub(block_w) / 2, margin),
        CaptionPosition::Custom(fx, fy) => (
            (fx.clamp(0.0, 1.0) * w as f32) as usize,
            (fy.clamp(0.0, 1.0) * h as f32) as usize),
    };

    // Translucent box behind the text for legibility (caption convention).
    let pad = scale * 2;
    let box_x0 = bx.saturating_sub(pad);
    let box_y0 = by.saturating_sub(pad);
    let box_x1 = (bx + block_w + pad).min(w as usize);
    let box_y1 = (by + block_h + pad).min(h as usize);
    for y in box_y0..box_y1 {
        for x in box_x0..box_x1 {
            let i = (y * w as usize + x) * 4;
            if i + 4 <= buf.len() {
                // 60% black over the existing pixel.
                for c in 0..3 {
                    buf[i + c] = (buf[i + c] as f32 * 0.4).round() as u8;
                }
                buf[i + 3] = 255;
            }
        }
    }

    // Draw each line centered within the block, each glyph in its run's color.
    for (li, line) in lines.iter().enumerate() {
        let n_chars = line.len();
        let line_w = if n_chars > 0 { n_chars * char_w - scale } else { 0 };
        let lx = bx + (block_w.saturating_sub(line_w)) / 2;
        let ly = by + li * (char_h + scale);
        for (ci, (ch, color)) in line.iter().enumerate() {
            let code = *ch as u32;
            if code < 32 || code > 126 {
                continue;
            }
            let glyph = FONT_5X7[(code - 32) as usize];
            let cx_base = lx + ci * char_w;
            for col in 0..5usize {
                let col_bits = glyph[col];
                for row in 0..7usize {
                    if (col_bits >> row) & 1 == 1 {
                        for sy in 0..scale {
                            let py = ly + row * scale + sy;
                            if py >= h as usize {
                                break;
                            }
                            for sx in 0..scale {
                                let px_x = cx_base + col * scale + sx;
                                if px_x >= w as usize {
                                    break;
                                }
                                let i = (py * w as usize + px_x) * 4;
                                if i + 4 <= buf.len() {
                                    buf[i] = color[0];
                                    buf[i + 1] = color[1];
                                    buf[i + 2] = color[2];
                                    buf[i + 3] = 255;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Rasterize a title clip's text using the embedded 5×7 bitmap font into a
/// `w×h` RGBA8 buffer. Text is centered on the frame. `bg_color = None` means
/// transparent background (text blends over the track beneath it).
pub(crate) fn render_title_frame(
    text: &str,
    font_size: f32,
    color: [u8; 4],
    bg_color: Option<[u8; 4]>,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let scale = ((font_size / 7.0).round() as usize).max(1);
    let char_w = 5 * scale + scale; // 5 pixel columns + 1 pixel gap between chars
    let char_h = 7 * scale;
    let n_chars = text.chars().count();
    let text_w = if n_chars > 0 { n_chars * char_w - scale } else { 0 }; // no trailing gap
    let text_h = char_h;

    // Fill with background.
    let n_px = (w as usize) * (h as usize);
    let mut buf = if let Some(bg) = bg_color {
        let mut b = Vec::with_capacity(n_px * 4);
        for _ in 0..n_px {
            b.extend_from_slice(&[bg[0], bg[1], bg[2], bg[3]]);
        }
        b
    } else {
        vec![0u8; n_px * 4] // transparent
    };

    // Center the text block.
    let off_x = (w as usize).saturating_sub(text_w) / 2;
    let off_y = (h as usize).saturating_sub(text_h) / 2;

    for (ci, ch) in text.chars().enumerate() {
        let code = ch as u32;
        if code < 32 || code > 126 {
            continue;
        }
        let glyph = FONT_5X7[(code - 32) as usize];
        let cx_base = off_x + ci * char_w;
        for col in 0..5usize {
            let col_bits = glyph[col];
            for row in 0..7usize {
                if (col_bits >> row) & 1 == 1 {
                    for sy in 0..scale {
                        let py = off_y + row * scale + sy;
                        if py >= h as usize {
                            break;
                        }
                        for sx in 0..scale {
                            let px_x = cx_base + col * scale + sx;
                            if px_x >= w as usize {
                                break;
                            }
                            let i = (py * w as usize + px_x) * 4;
                            buf[i] = color[0];
                            buf[i + 1] = color[1];
                            buf[i + 2] = color[2];
                            buf[i + 3] = color[3];
                        }
                    }
                }
            }
        }
    }
    buf
}

/// Apply S-Log2 → Rec.709 gamma transform to an RGBA8 buffer in place.
/// S-Log2 → linear: `linear = 10^((code - 0.616596) / 0.432699) - 0.037584`
/// Linear → Rec.709: `y = 1.099 * linear^0.45 - 0.099` for linear >= 0.018.
pub fn apply_slog2_rec709(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        for c in 0..3 {
            let code = px[c] as f32 / 255.0;
            let linear = (10.0_f32.powf((code - 0.616596) / 0.432699) - 0.037584).max(0.0);
            let rec709 = if linear >= 0.018 {
                (1.099 * linear.powf(0.45) - 0.099).clamp(0.0, 1.0)
            } else {
                (4.5 * linear).clamp(0.0, 1.0)
            };
            px[c] = (rec709 * 255.0).round() as u8;
        }
    }
}
