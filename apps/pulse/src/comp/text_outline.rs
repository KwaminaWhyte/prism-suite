//! The **real outline-font** layout machinery extracted from `comp/text.rs`
//! (workspace size rule): the per-line + per-glyph TrueType layout
//! ([`layout_outline_line`] / [`OutlineLayout`] / [`OutlineLine`]), the
//! [`ttf_parser::OutlineBuilder`] that flattens glyph curves into closed polygons
//! ([`OutlineFlattener`]), and the straight source-over helper [`over_straight`]
//! the fill/stroke coverage composites through. `TextLayer`'s outline methods in
//! `text.rs` call these exactly as when they lived inline.

/// A laid-out outline block: every glyph contour in layer-local space.
pub(super) struct OutlineLayout {
    pub(super) contours: Vec<Vec<(f32, f32)>>,
}

/// One measured outline line: its advance width and glyph contours, with the pen
/// at x = 0 and the baseline at line-local y = 0 (y grows downward).
pub(super) struct OutlineLine {
    pub(super) width: f32,
    pub(super) contours: Vec<Vec<(f32, f32)>>,
}

/// Build one baseline run of outline glyphs: walk the characters, extract each
/// glyph's flattened contours at the running pen x, and advance the pen by the
/// glyph's horizontal advance plus the layer's `tracking`. The trailing tracking
/// past the last glyph is dropped so the line width is tight (matching the stroke
/// font's measure). Contours come back y-down (baseline at y = 0).
pub(super) fn layout_outline_line(
    face: &ttf_parser::Face,
    line: &str,
    scale: f32,
    tracking: f32,
) -> OutlineLine {
    let mut pen_x = 0.0_f32;
    let mut contours: Vec<Vec<(f32, f32)>> = Vec::new();
    for ch in line.chars() {
        let advance = match face.glyph_index(ch) {
            Some(gid) => {
                if !ch.is_whitespace() {
                    let mut builder = OutlineFlattener::new(pen_x, scale);
                    if face.outline_glyph(gid, &mut builder).is_some() {
                        contours.extend(builder.finish());
                    }
                }
                face.glyph_hor_advance(gid)
                    .map(|a| a as f32 * scale)
                    .unwrap_or_else(|| space_advance(face, scale))
            }
            // No glyph for this char: advance by a space so layout stays sane.
            None => space_advance(face, scale),
        };
        pen_x += advance + tracking;
    }
    // Tight width: total pen travel less the trailing tracking after the last
    // glyph (so a single-char line measures its glyph advance, not advance+track).
    let width = (pen_x - tracking).max(0.0);
    OutlineLine { width, contours }
}

/// A reasonable advance for a missing / whitespace glyph: the face's space
/// advance if it has one, else half the em.
fn space_advance(face: &ttf_parser::Face, scale: f32) -> f32 {
    face.glyph_index(' ')
        .and_then(|g| face.glyph_hor_advance(g))
        .map(|a| a as f32 * scale)
        .unwrap_or(face.units_per_em() as f32 * 0.5 * scale)
}

/// Number of straight chords each glyph curve (quadratic / cubic) is flattened
/// into. Fixed subdivision keeps the polygon fill cheap and deterministic, and is
/// plenty smooth at motion-graphics sizes (mirrors the shape system's `ARC_STEPS`
/// approach).
const CURVE_STEPS: u32 = 8;

/// A [`ttf_parser::OutlineBuilder`] that flattens a glyph's outline into closed
/// layer-local polygons (one per glyph contour). TrueType outlines are y-up;
/// layer space is y-down with the baseline at y = 0, so every y is negated. The
/// pen-x offset and em→px `scale` are folded in as points arrive. Quadratic and
/// cubic segments are tessellated into [`CURVE_STEPS`] line chords each, so the
/// shape system's polygon fill consumes the result unchanged.
struct OutlineFlattener {
    pen_x: f32,
    scale: f32,
    /// Completed contours.
    done: Vec<Vec<(f32, f32)>>,
    /// Current contour's points (layer-local).
    cur: Vec<(f32, f32)>,
    /// Last emitted point (the current pen position), for curve starts.
    last: (f32, f32),
}

impl OutlineFlattener {
    fn new(pen_x: f32, scale: f32) -> Self {
        Self {
            pen_x,
            scale,
            done: Vec::new(),
            cur: Vec::new(),
            last: (0.0, 0.0),
        }
    }

    /// Map a font-space point to layer space (apply pen offset + scale, flip y).
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.pen_x + x * self.scale, -y * self.scale)
    }

    /// Finish the in-progress contour (if any) and return every contour built.
    fn finish(mut self) -> Vec<Vec<(f32, f32)>> {
        self.flush();
        self.done
    }

    /// Close out the current contour into `done` (glyph contours are closed
    /// regions; the polygon fill closes the implicit last edge). Drops degenerate
    /// (< 3-point) contours.
    fn flush(&mut self) {
        if self.cur.len() >= 3 {
            self.done.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

impl ttf_parser::OutlineBuilder for OutlineFlattener {
    fn move_to(&mut self, x: f32, y: f32) {
        // Starting a new contour: flush any previous one.
        self.flush();
        let p = self.map(x, y);
        self.cur.push(p);
        self.last = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.cur.push(p);
        self.last = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let a = self.last;
        let ctrl = self.map(x1, y1);
        let end = self.map(x, y);
        // Tessellate the quadratic Bézier into line chords.
        for s in 1..=CURVE_STEPS {
            let t = s as f32 / CURVE_STEPS as f32;
            let mt = 1.0 - t;
            let bx = mt * mt * a.0 + 2.0 * mt * t * ctrl.0 + t * t * end.0;
            let by = mt * mt * a.1 + 2.0 * mt * t * ctrl.1 + t * t * end.1;
            self.cur.push((bx, by));
        }
        self.last = end;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let a = self.last;
        let c1 = self.map(x1, y1);
        let c2 = self.map(x2, y2);
        let end = self.map(x, y);
        // Tessellate the cubic Bézier into line chords.
        for s in 1..=CURVE_STEPS {
            let t = s as f32 / CURVE_STEPS as f32;
            let mt = 1.0 - t;
            let bx = mt * mt * mt * a.0
                + 3.0 * mt * mt * t * c1.0
                + 3.0 * mt * t * t * c2.0
                + t * t * t * end.0;
            let by = mt * mt * mt * a.1
                + 3.0 * mt * mt * t * c1.1
                + 3.0 * mt * t * t * c2.1
                + t * t * t * end.1;
            self.cur.push((bx, by));
        }
        self.last = end;
    }

    fn close(&mut self) {
        self.flush();
    }
}

/// Straight (non-premultiplied) source-over of `src` over `dst`, both
/// `[r, g, b, a]` with `a` as coverage. (Mirrors the shape system's blend.)
pub(super) fn over_straight(src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    let sa = src[3].clamp(0.0, 1.0);
    let da = dst[3].clamp(0.0, 1.0);
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return [0.0; 4];
    }
    let blend = |s: f32, d: f32| (s * sa + d * da * (1.0 - sa)) / out_a;
    [
        blend(src[0], dst[0]),
        blend(src[1], dst[1]),
        blend(src[2], dst[2]),
        out_a,
    ]
}
