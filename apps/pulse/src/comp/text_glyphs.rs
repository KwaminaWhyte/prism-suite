//! The built-in **stroke vector font** glyph table extracted from `comp/text.rs`
//! (workspace size rule): the `glyph` lookup and the per-character segment tables
//! it returns, authored on the unit em grid. Pure data — the layout / rasterizer
//! in `text.rs` calls [`glyph`] exactly as when it lived inline, so the stroke
//! font renders byte-identically.

use super::text::Seg;

/// The stroke skeleton of one character in the built-in font, as a slice of
/// segments on the unit em grid: x in `[0, GLYPH_W]`, y in `[0, 1]` with `y = 0`
/// the cap line (top) and `y = 1` the baseline (bottom). Letters are uppercased;
/// unknown printable characters fall back to a small box, and space/control
/// characters draw nothing.
///
/// The font is a simple humanist sans skeleton — enough strokes to read clearly
/// at motion-graphics sizes, authored once here so text needs no font
/// dependency.
pub(super) fn glyph(ch: char) -> &'static [Seg] {
    // Convenience corners on the em grid.
    let up = ch.to_ascii_uppercase();
    match up {
        ' ' => &[],
        'A' => A,
        'B' => B,
        'C' => C,
        'D' => D,
        'E' => E,
        'F' => F,
        'G' => G,
        'H' => H,
        'I' => I,
        'J' => J,
        'K' => K,
        'L' => L,
        'M' => M,
        'N' => N,
        'O' => O,
        'P' => P,
        'Q' => Q,
        'R' => R,
        'S' => S,
        'T' => T,
        'U' => U,
        'V' => V,
        'W' => W,
        'X' => X,
        'Y' => Y,
        'Z' => Z,
        '0' => O, // share the O ring
        '1' => ONE,
        '2' => TWO,
        '3' => THREE,
        '4' => FOUR,
        '5' => S, // 5 reads like S
        '6' => SIX,
        '7' => SEVEN,
        '8' => EIGHT,
        '9' => NINE,
        '.' => DOT,
        ',' => DOT,
        '!' => EXCLAIM,
        '?' => QUESTION,
        '-' => DASH,
        '_' => UNDERSCORE,
        ':' => COLON,
        '/' => SLASH,
        '+' => PLUS,
        '=' => EQUALS,
        '\t' => &[],
        c if c.is_control() => &[],
        _ => FALLBACK,
    }
}

// Glyph geometry. Each is a slice of segments in the `[0, 0.5] × [0, 1]` em box.
// y grows downward (0 = top / cap, 1 = bottom / baseline).
const L0: f32 = 0.04; // left margin
const R0: f32 = 0.46; // right margin
const MX: f32 = 0.25; // horizontal center
const TOP: f32 = 0.05;
const MID: f32 = 0.5;
const BOT: f32 = 0.95;

#[rustfmt::skip]
const A: &[Seg] = &[
    ((L0, BOT), (MX, TOP)), ((MX, TOP), (R0, BOT)), ((0.13, MID + 0.12), (0.37, MID + 0.12)),
];
#[rustfmt::skip]
const B: &[Seg] = &[
    ((L0, TOP), (L0, BOT)), ((L0, TOP), (0.34, TOP)), ((0.34, TOP), (R0, 0.22)),
    ((R0, 0.22), (0.34, MID)), ((L0, MID), (0.34, MID)), ((0.34, MID), (R0, 0.72)),
    ((R0, 0.72), (0.34, BOT)), ((L0, BOT), (0.34, BOT)),
];
#[rustfmt::skip]
const C: &[Seg] = &[
    ((R0, 0.2), (MX, TOP)), ((MX, TOP), (L0, 0.3)), ((L0, 0.3), (L0, 0.7)),
    ((L0, 0.7), (MX, BOT)), ((MX, BOT), (R0, 0.8)),
];
#[rustfmt::skip]
const D: &[Seg] = &[
    ((L0, TOP), (L0, BOT)), ((L0, TOP), (0.28, TOP)), ((0.28, TOP), (R0, 0.3)),
    ((R0, 0.3), (R0, 0.7)), ((R0, 0.7), (0.28, BOT)), ((L0, BOT), (0.28, BOT)),
];
#[rustfmt::skip]
const E: &[Seg] = &[
    ((R0, TOP), (L0, TOP)), ((L0, TOP), (L0, BOT)), ((L0, MID), (0.36, MID)),
    ((L0, BOT), (R0, BOT)),
];
#[rustfmt::skip]
const F: &[Seg] = &[
    ((R0, TOP), (L0, TOP)), ((L0, TOP), (L0, BOT)), ((L0, MID), (0.36, MID)),
];
#[rustfmt::skip]
const G: &[Seg] = &[
    ((R0, 0.2), (MX, TOP)), ((MX, TOP), (L0, 0.3)), ((L0, 0.3), (L0, 0.7)),
    ((L0, 0.7), (MX, BOT)), ((MX, BOT), (R0, 0.7)), ((R0, 0.7), (R0, MID)),
    ((R0, MID), (0.3, MID)),
];
#[rustfmt::skip]
const H: &[Seg] = &[
    ((L0, TOP), (L0, BOT)), ((R0, TOP), (R0, BOT)), ((L0, MID), (R0, MID)),
];
#[rustfmt::skip]
const I: &[Seg] = &[
    ((MX, TOP), (MX, BOT)), ((0.13, TOP), (0.37, TOP)), ((0.13, BOT), (0.37, BOT)),
];
#[rustfmt::skip]
const J: &[Seg] = &[
    ((R0, TOP), (R0, 0.78)), ((R0, 0.78), (MX, BOT)), ((MX, BOT), (L0, 0.78)),
];
#[rustfmt::skip]
const K: &[Seg] = &[
    ((L0, TOP), (L0, BOT)), ((R0, TOP), (L0, MID)), ((L0, MID), (R0, BOT)),
];
#[rustfmt::skip]
const L: &[Seg] = &[
    ((L0, TOP), (L0, BOT)), ((L0, BOT), (R0, BOT)),
];
#[rustfmt::skip]
const M: &[Seg] = &[
    ((L0, BOT), (L0, TOP)), ((L0, TOP), (MX, MID)), ((MX, MID), (R0, TOP)),
    ((R0, TOP), (R0, BOT)),
];
#[rustfmt::skip]
const N: &[Seg] = &[
    ((L0, BOT), (L0, TOP)), ((L0, TOP), (R0, BOT)), ((R0, BOT), (R0, TOP)),
];
#[rustfmt::skip]
const O: &[Seg] = &[
    ((MX, TOP), (R0, 0.3)), ((R0, 0.3), (R0, 0.7)), ((R0, 0.7), (MX, BOT)),
    ((MX, BOT), (L0, 0.7)), ((L0, 0.7), (L0, 0.3)), ((L0, 0.3), (MX, TOP)),
];
#[rustfmt::skip]
const P: &[Seg] = &[
    ((L0, BOT), (L0, TOP)), ((L0, TOP), (0.34, TOP)), ((0.34, TOP), (R0, 0.2)),
    ((R0, 0.2), (0.34, MID)), ((0.34, MID), (L0, MID)),
];
#[rustfmt::skip]
const Q: &[Seg] = &[
    ((MX, TOP), (R0, 0.3)), ((R0, 0.3), (R0, 0.7)), ((R0, 0.7), (MX, BOT)),
    ((MX, BOT), (L0, 0.7)), ((L0, 0.7), (L0, 0.3)), ((L0, 0.3), (MX, TOP)),
    ((0.3, 0.7), (R0, BOT)),
];
#[rustfmt::skip]
const R: &[Seg] = &[
    ((L0, BOT), (L0, TOP)), ((L0, TOP), (0.34, TOP)), ((0.34, TOP), (R0, 0.2)),
    ((R0, 0.2), (0.34, MID)), ((0.34, MID), (L0, MID)), ((0.28, MID), (R0, BOT)),
];
#[rustfmt::skip]
const S: &[Seg] = &[
    ((R0, 0.2), (MX, TOP)), ((MX, TOP), (L0, 0.25)), ((L0, 0.25), (R0, MID)),
    ((R0, MID), (R0, 0.7)), ((R0, 0.7), (MX, BOT)), ((MX, BOT), (L0, 0.8)),
];
#[rustfmt::skip]
const T: &[Seg] = &[
    ((L0, TOP), (R0, TOP)), ((MX, TOP), (MX, BOT)),
];
#[rustfmt::skip]
const U: &[Seg] = &[
    ((L0, TOP), (L0, 0.72)), ((L0, 0.72), (0.16, BOT)), ((0.16, BOT), (0.34, BOT)),
    ((0.34, BOT), (R0, 0.72)), ((R0, 0.72), (R0, TOP)),
];
#[rustfmt::skip]
const V: &[Seg] = &[
    ((L0, TOP), (MX, BOT)), ((MX, BOT), (R0, TOP)),
];
#[rustfmt::skip]
const W: &[Seg] = &[
    ((L0, TOP), (0.16, BOT)), ((0.16, BOT), (MX, MID)), ((MX, MID), (0.34, BOT)),
    ((0.34, BOT), (R0, TOP)),
];
#[rustfmt::skip]
const X: &[Seg] = &[
    ((L0, TOP), (R0, BOT)), ((R0, TOP), (L0, BOT)),
];
#[rustfmt::skip]
const Y: &[Seg] = &[
    ((L0, TOP), (MX, MID)), ((R0, TOP), (MX, MID)), ((MX, MID), (MX, BOT)),
];
#[rustfmt::skip]
const Z: &[Seg] = &[
    ((L0, TOP), (R0, TOP)), ((R0, TOP), (L0, BOT)), ((L0, BOT), (R0, BOT)),
];
#[rustfmt::skip]
const ONE: &[Seg] = &[
    ((0.16, 0.2), (MX, TOP)), ((MX, TOP), (MX, BOT)), ((0.13, BOT), (0.37, BOT)),
];
#[rustfmt::skip]
const TWO: &[Seg] = &[
    ((L0, 0.22), (MX, TOP)), ((MX, TOP), (R0, 0.25)), ((R0, 0.25), (L0, BOT)),
    ((L0, BOT), (R0, BOT)),
];
#[rustfmt::skip]
const THREE: &[Seg] = &[
    ((L0, TOP), (R0, TOP)), ((R0, TOP), (0.28, MID)), ((0.28, MID), (R0, 0.7)),
    ((R0, 0.7), (MX, BOT)), ((MX, BOT), (L0, 0.8)),
];
#[rustfmt::skip]
const FOUR: &[Seg] = &[
    ((0.34, BOT), (0.34, TOP)), ((0.34, TOP), (L0, 0.6)), ((L0, 0.6), (R0, 0.6)),
];
#[rustfmt::skip]
const SIX: &[Seg] = &[
    ((R0, 0.2), (MX, TOP)), ((MX, TOP), (L0, MID)), ((L0, MID), (L0, 0.8)),
    ((L0, 0.8), (MX, BOT)), ((MX, BOT), (R0, 0.75)), ((R0, 0.75), (0.3, MID)),
    ((0.3, MID), (L0, MID)),
];
#[rustfmt::skip]
const SEVEN: &[Seg] = &[
    ((L0, TOP), (R0, TOP)), ((R0, TOP), (0.2, BOT)),
];
#[rustfmt::skip]
const EIGHT: &[Seg] = &[
    ((MX, TOP), (R0, 0.2)), ((R0, 0.2), (MX, MID)), ((MX, MID), (L0, 0.2)),
    ((L0, 0.2), (MX, TOP)), ((MX, MID), (R0, 0.72)), ((R0, 0.72), (MX, BOT)),
    ((MX, BOT), (L0, 0.72)), ((L0, 0.72), (MX, MID)),
];
#[rustfmt::skip]
const NINE: &[Seg] = &[
    ((0.3, MID), (L0, 0.25)), ((L0, 0.25), (MX, TOP)), ((MX, TOP), (R0, MID)),
    ((R0, MID), (R0, 0.5)), ((MX, TOP), (R0, MID)), ((R0, MID), (MX, BOT)),
    ((MX, BOT), (L0, 0.8)),
];
#[rustfmt::skip]
const DOT: &[Seg] = &[
    ((MX, 0.9), (MX, BOT)),
];
#[rustfmt::skip]
const EXCLAIM: &[Seg] = &[
    ((MX, TOP), (MX, 0.65)), ((MX, 0.88), (MX, BOT)),
];
#[rustfmt::skip]
const QUESTION: &[Seg] = &[
    ((L0, 0.22), (MX, TOP)), ((MX, TOP), (R0, 0.25)), ((R0, 0.25), (MX, MID)),
    ((MX, MID), (MX, 0.65)), ((MX, 0.88), (MX, BOT)),
];
#[rustfmt::skip]
const DASH: &[Seg] = &[
    ((0.12, MID), (0.38, MID)),
];
#[rustfmt::skip]
const UNDERSCORE: &[Seg] = &[
    ((L0, BOT), (R0, BOT)),
];
#[rustfmt::skip]
const COLON: &[Seg] = &[
    ((MX, 0.35), (MX, 0.45)), ((MX, 0.78), (MX, 0.88)),
];
#[rustfmt::skip]
const SLASH: &[Seg] = &[
    ((L0, BOT), (R0, TOP)),
];
#[rustfmt::skip]
const PLUS: &[Seg] = &[
    ((MX, 0.3), (MX, 0.7)), ((0.13, MID), (0.37, MID)),
];
#[rustfmt::skip]
const EQUALS: &[Seg] = &[
    ((0.12, 0.4), (0.38, 0.4)), ((0.12, 0.6), (0.38, 0.6)),
];
#[rustfmt::skip]
const FALLBACK: &[Seg] = &[
    ((L0, TOP), (R0, TOP)), ((R0, TOP), (R0, BOT)), ((R0, BOT), (L0, BOT)),
    ((L0, BOT), (L0, TOP)),
];
