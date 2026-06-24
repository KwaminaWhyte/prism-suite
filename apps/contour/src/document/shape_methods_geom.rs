//! `impl Shape`: fill/stroke/appearance/gradient/envelope styling plus all
//! geometry (bounds, transforms, path conversion, anchor editing, hit-test).
//! Split out of `document/mod.rs` (mechanical extraction).

use super::*;

impl Shape {
    /// The compound path's [`FillRule`], if this is a compound (or text) shape.
    /// Text fills under **even-odd** so glyph counters (the hole in an "o") render
    /// as holes, matching the glyph-outline → compound conversion.
    pub fn fill_rule(&self) -> Option<FillRule> {
        match self {
            Shape::Compound { fill_rule, .. } => Some(*fill_rule),
            Shape::Text { .. } => Some(FillRule::EvenOdd),
            _ => None,
        }
    }

    /// The shape's stroke colour (straight sRGB RGBA). Every variant has a
    /// stroke, so this is always `Some` — the `Option` keeps the accessor shaped
    /// like [`fill_color`](Self::fill_color) for the appearance helpers.
    pub fn stroke_color(&self) -> Option<[f32; 4]> {
        match self {
            Shape::Rect { stroke, .. }
            | Shape::Ellipse { stroke, .. }
            | Shape::Line { stroke, .. }
            | Shape::Path { stroke, .. }
            | Shape::Compound { stroke, .. }
            | Shape::Text { stroke, .. } => Some(*stroke),
        }
    }

    /// Set the shape's stroke colour.
    pub fn set_stroke_color(&mut self, c: [f32; 4]) {
        match self {
            Shape::Rect { stroke, .. }
            | Shape::Ellipse { stroke, .. }
            | Shape::Line { stroke, .. }
            | Shape::Path { stroke, .. }
            | Shape::Compound { stroke, .. }
            | Shape::Text { stroke, .. } => *stroke = c,
        }
    }

    /// The shape's stroke width in document units.
    pub fn stroke_width(&self) -> f32 {
        match self {
            Shape::Rect { stroke_w, .. }
            | Shape::Ellipse { stroke_w, .. }
            | Shape::Line { stroke_w, .. }
            | Shape::Path { stroke_w, .. }
            | Shape::Compound { stroke_w, .. }
            | Shape::Text { stroke_w, .. } => *stroke_w,
        }
    }

    /// Set the shape's stroke width (document units).
    pub fn set_stroke_width(&mut self, w: f32) {
        match self {
            Shape::Rect { stroke_w, .. }
            | Shape::Ellipse { stroke_w, .. }
            | Shape::Line { stroke_w, .. }
            | Shape::Path { stroke_w, .. }
            | Shape::Compound { stroke_w, .. }
            | Shape::Text { stroke_w, .. } => *stroke_w = w,
        }
    }

    /// The shape's stroke attributes (caps/joins/dashes).
    pub fn stroke_style(&self) -> &StrokeStyle {
        match self {
            Shape::Rect { stroke_style, .. }
            | Shape::Ellipse { stroke_style, .. }
            | Shape::Line { stroke_style, .. }
            | Shape::Path { stroke_style, .. }
            | Shape::Compound { stroke_style, .. }
            | Shape::Text { stroke_style, .. } => stroke_style,
        }
    }

    /// Mutable access to the shape's stroke attributes.
    pub fn stroke_style_mut(&mut self) -> &mut StrokeStyle {
        match self {
            Shape::Rect { stroke_style, .. }
            | Shape::Ellipse { stroke_style, .. }
            | Shape::Line { stroke_style, .. }
            | Shape::Path { stroke_style, .. }
            | Shape::Compound { stroke_style, .. }
            | Shape::Text { stroke_style, .. } => stroke_style,
        }
    }

    /// The shape's stacked [`Appearance`], if one has been attached. When `Some`
    /// it overrides the single `fill`/`stroke` fields on every render surface;
    /// when `None` the shape paints from its legacy single fields. Every variant
    /// can carry one (a `Line`'s stack just holds strokes).
    pub fn appearance(&self) -> Option<&Appearance> {
        match self {
            Shape::Rect { appearance, .. }
            | Shape::Ellipse { appearance, .. }
            | Shape::Line { appearance, .. }
            | Shape::Path { appearance, .. }
            | Shape::Compound { appearance, .. }
            | Shape::Text { appearance, .. } => appearance.as_ref(),
        }
    }

    /// Mutable access to the shape's `appearance` slot (set/clear the stack).
    pub fn appearance_mut(&mut self) -> &mut Option<Appearance> {
        match self {
            Shape::Rect { appearance, .. }
            | Shape::Ellipse { appearance, .. }
            | Shape::Line { appearance, .. }
            | Shape::Path { appearance, .. }
            | Shape::Compound { appearance, .. }
            | Shape::Text { appearance, .. } => appearance,
        }
    }

    /// Set (or clear, with `None`) the shape's stacked appearance.
    pub fn set_appearance(&mut self, a: Option<Appearance>) {
        *self.appearance_mut() = a;
    }

    pub fn envelope_mesh(&self) -> Option<&crate::envelope::EnvelopeMesh> {
        match self {
            Shape::Rect { envelope_mesh, .. } => envelope_mesh.as_ref(),
            Shape::Ellipse { envelope_mesh, .. } => envelope_mesh.as_ref(),
            Shape::Line { envelope_mesh, .. } => envelope_mesh.as_ref(),
            Shape::Path { envelope_mesh, .. } => envelope_mesh.as_ref(),
            Shape::Compound { envelope_mesh, .. } => envelope_mesh.as_ref(),
            Shape::Text { envelope_mesh, .. } => envelope_mesh.as_ref(),
        }
    }

    pub fn envelope_mesh_mut(&mut self) -> Option<&mut crate::envelope::EnvelopeMesh> {
        match self {
            Shape::Rect { envelope_mesh, .. } => envelope_mesh.as_mut(),
            Shape::Ellipse { envelope_mesh, .. } => envelope_mesh.as_mut(),
            Shape::Line { envelope_mesh, .. } => envelope_mesh.as_mut(),
            Shape::Path { envelope_mesh, .. } => envelope_mesh.as_mut(),
            Shape::Compound { envelope_mesh, .. } => envelope_mesh.as_mut(),
            Shape::Text { envelope_mesh, .. } => envelope_mesh.as_mut(),
        }
    }

    pub fn set_envelope_mesh(&mut self, m: Option<crate::envelope::EnvelopeMesh>) {
        match self {
            Shape::Rect { envelope_mesh, .. } => *envelope_mesh = m,
            Shape::Ellipse { envelope_mesh, .. } => *envelope_mesh = m,
            Shape::Line { envelope_mesh, .. } => *envelope_mesh = m,
            Shape::Path { envelope_mesh, .. } => *envelope_mesh = m,
            Shape::Compound { envelope_mesh, .. } => *envelope_mesh = m,
            Shape::Text { envelope_mesh, .. } => *envelope_mesh = m,
        }
    }

    /// The appearance the renderers should walk: the attached stack if there is
    /// one, otherwise a freshly-migrated one-fill/one-stroke stack built from the
    /// shape's legacy single fields ([`Appearance::from_legacy`]). This is the
    /// single source of truth for the canvas painter and the SVG / PNG exporters,
    /// so a shape renders identically whether or not it has an explicit stack.
    pub fn effective_appearance(&self) -> Appearance {
        match self.appearance() {
            Some(a) => a.clone(),
            None => Appearance::from_legacy(
                self.fill_color(),
                self.fill_gradient(),
                self.stroke_color().unwrap_or([0.0, 0.0, 0.0, 0.0]),
                self.stroke_width(),
                self.stroke_style(),
            ),
        }
    }

    /// The shape's gradient fill, if it has one (`Line` never does). When
    /// present this overrides the solid `fill` colour on every render surface.
    pub fn fill_gradient(&self) -> Option<&Gradient> {
        match self {
            Shape::Rect { fill_gradient, .. }
            | Shape::Ellipse { fill_gradient, .. }
            | Shape::Path { fill_gradient, .. }
            | Shape::Compound { fill_gradient, .. }
            | Shape::Text { fill_gradient, .. } => fill_gradient.as_ref(),
            Shape::Line { .. } => None,
        }
    }

    /// Set (or clear, with `None`) the shape's gradient fill. No-op on `Line`,
    /// which has no fill region.
    pub fn set_fill_gradient(&mut self, g: Option<Gradient>) {
        match self {
            Shape::Rect { fill_gradient, .. }
            | Shape::Ellipse { fill_gradient, .. }
            | Shape::Path { fill_gradient, .. }
            | Shape::Compound { fill_gradient, .. }
            | Shape::Text { fill_gradient, .. } => *fill_gradient = g,
            Shape::Line { .. } => {}
        }
    }

    /// The shape's solid fill colour, if it has a fill region (`Line` returns
    /// `None`). This is the colour used when there is no gradient, and the
    /// gradient's fallback.
    pub fn fill_color(&self) -> Option<[f32; 4]> {
        match self {
            Shape::Rect { fill, .. }
            | Shape::Ellipse { fill, .. }
            | Shape::Path { fill, .. }
            | Shape::Compound { fill, .. }
            | Shape::Text { fill, .. } => Some(*fill),
            Shape::Line { .. } => None,
        }
    }

    /// Set the shape's solid fill colour. No-op on `Line`.
    pub fn set_fill_color(&mut self, c: [f32; 4]) {
        match self {
            Shape::Rect { fill, .. }
            | Shape::Ellipse { fill, .. }
            | Shape::Path { fill, .. }
            | Shape::Compound { fill, .. }
            | Shape::Text { fill, .. } => *fill = c,
            Shape::Line { .. } => {}
        }
    }

    /// Replace every occurrence of the colour `old` with `new` across this
    /// shape's paint — its solid fill, its stroke, and any gradient-stop colours.
    /// Colours are compared with the same picker-rounding tolerance as the
    /// swatch model. Returns `true` if anything changed.
    ///
    /// This is the per-shape half of a **global swatch** recolour: when a global
    /// swatch's colour is edited, the document walks every shape and remaps the
    /// old colour to the new one, so artwork painted with that swatch follows the
    /// edit (Illustrator's global-colour behaviour).
    pub fn remap_color(&mut self, old: [f32; 4], new: [f32; 4]) -> bool {
        let mut changed = false;
        if let Some(c) = self.fill_color() {
            if swatches::colors_eq(c, old) {
                self.set_fill_color(new);
                changed = true;
            }
        }
        if let Some(c) = self.stroke_color() {
            if swatches::colors_eq(c, old) {
                self.set_stroke_color(new);
                changed = true;
            }
        }
        // Remap colours inside an attached appearance stack (solid paints and
        // gradient stops on every fill / stroke) so a global swatch edit follows
        // stacked artwork too.
        if let Some(ap) = self.appearance_mut() {
            use crate::appearance::Paint;
            let mut remap_paint = |p: &mut Paint| match p {
                Paint::Solid(c) => {
                    if swatches::colors_eq(*c, old) {
                        *c = new;
                        changed = true;
                    }
                }
                Paint::Gradient(g) => {
                    for stop in g.stops.iter_mut() {
                        if swatches::colors_eq(stop.color, old) {
                            stop.color = new;
                            changed = true;
                        }
                    }
                }
            };
            for f in ap.fills.iter_mut() {
                remap_paint(&mut f.paint);
            }
            for s in ap.strokes.iter_mut() {
                remap_paint(&mut s.paint);
            }
        }
        let grad_changed = match self {
            Shape::Rect { fill_gradient, .. }
            | Shape::Ellipse { fill_gradient, .. }
            | Shape::Path { fill_gradient, .. }
            | Shape::Compound { fill_gradient, .. }
            | Shape::Text { fill_gradient, .. } => fill_gradient
                .as_mut()
                .map(|g| {
                    let mut any = false;
                    for stop in g.stops.iter_mut() {
                        if swatches::colors_eq(stop.color, old) {
                            stop.color = new;
                            any = true;
                        }
                    }
                    any
                })
                .unwrap_or(false),
            Shape::Line { .. } => false,
        };
        changed || grad_changed
    }

    /// Axis-aligned bounding box in document space.
    ///
    /// Returns a `prism_core::geometry::Rect` to exercise the shared suite
    /// primitive. Returns `None` for empty paths.
    pub fn bounds(&self) -> Option<CoreRect> {
        let bbox = |pts: &[(f32, f32)]| -> Option<CoreRect> {
            let mut it = pts.iter();
            let &(x0, y0) = it.next()?;
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (x0, y0, x0, y0);
            for &(x, y) in it {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            Some(CoreRect::new(min_x, min_y, max_x - min_x, max_y - min_y))
        };
        match self {
            Shape::Rect { rect, .. } | Shape::Ellipse { rect, .. } => {
                Some(CoreRect::new(rect[0], rect[1], rect[2], rect[3]))
            }
            Shape::Line { p0, p1, .. } => bbox(&[*p0, *p1]),
            Shape::Path {
                points,
                closed,
                handles,
                ..
            } => {
                if points.is_empty() {
                    return None;
                }
                // Build the kurbo BezPath (honoring any bezier handles) and let
                // kurbo compute the tight bounding box.
                let bp = path::bez_path(points, handles, *closed);
                let r = bp.bounding_box();
                Some(CoreRect::new(
                    r.x0 as f32,
                    r.y0 as f32,
                    r.width() as f32,
                    r.height() as f32,
                ))
            }
            Shape::Compound { subpaths, .. } | Shape::Text { glyphs: subpaths, .. } => {
                // Union of every sub-contour's tight (bezier-aware) bounds. Text
                // shares this: its glyph cache is just a list of sub-contours.
                let mut union: Option<kurbo::Rect> = None;
                for sp in subpaths {
                    if sp.points.is_empty() {
                        continue;
                    }
                    let r = path::bez_path(&sp.points, &sp.handles, sp.closed).bounding_box();
                    union = Some(match union {
                        Some(u) => u.union(r),
                        None => r,
                    });
                }
                union.map(|r| {
                    CoreRect::new(
                        r.x0 as f32,
                        r.y0 as f32,
                        r.width() as f32,
                        r.height() as f32,
                    )
                })
            }
        }
    }

    /// Translate every coordinate by `(dx, dy)` in document space. Handles are
    /// offsets, so they are unaffected by translation.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Shape::Rect { rect, .. } | Shape::Ellipse { rect, .. } => {
                rect[0] += dx;
                rect[1] += dy;
            }
            Shape::Line { p0, p1, .. } => {
                p0.0 += dx;
                p0.1 += dy;
                p1.0 += dx;
                p1.1 += dy;
            }
            Shape::Path { points, .. } => {
                for p in points.iter_mut() {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
            Shape::Compound { subpaths, .. } => {
                for sp in subpaths.iter_mut() {
                    for p in sp.points.iter_mut() {
                        p.0 += dx;
                        p.1 += dy;
                    }
                }
            }
            Shape::Text {
                origin, glyphs, ..
            } => {
                // Move the editable anchor *and* the cached glyph outlines so the
                // text stays live (a later relayout keeps producing it in place).
                origin.0 += dx;
                origin.1 += dy;
                for sp in glyphs.iter_mut() {
                    for p in sp.points.iter_mut() {
                        p.0 += dx;
                        p.1 += dy;
                    }
                }
            }
        }
    }

    /// Apply an affine transform (rotate / scale / reflect / shear) to this
    /// shape, in document space.
    ///
    /// Axis-aligned shapes (`Rect`, `Ellipse`) stay their own variant only while
    /// the transform keeps their bounding box axis-aligned (pure
    /// translate/scale/flip). Under any rotation or shear they are rasterised
    /// into a [`Shape::Path`] that traces the transformed outline, exactly the
    /// way Illustrator turns a rotated rectangle into an editable path under the
    /// hood. `Line`/`Path` always transform in place (handles transform by the
    /// matrix's *linear* part, since they are offsets).
    pub fn apply_affine(&mut self, m: &Affine) {
        // A transform is "axis-aligned" if it has no rotation or shear, i.e. the
        // off-diagonal coefficients are (numerically) zero.
        let axis_aligned = m.b.abs() < 1e-6 && m.c.abs() < 1e-6;
        match self {
            Shape::Rect { rect, .. } | Shape::Ellipse { rect, .. } if axis_aligned => {
                let (x0, y0) = m.apply_point(rect[0], rect[1]);
                let (x1, y1) = m.apply_point(rect[0] + rect[2], rect[1] + rect[3]);
                // Re-normalise so width/height stay non-negative after a flip.
                rect[0] = x0.min(x1);
                rect[1] = y0.min(y1);
                rect[2] = (x1 - x0).abs();
                rect[3] = (y1 - y0).abs();
            }
            Shape::Rect { .. } | Shape::Ellipse { .. } => {
                // Rotation/shear: convert to a path tracing the outline, then
                // transform that path.
                *self = self.to_path();
                self.apply_affine(m);
            }
            Shape::Line { p0, p1, .. } => {
                *p0 = m.apply_point(p0.0, p0.1);
                *p1 = m.apply_point(p1.0, p1.1);
            }
            Shape::Path {
                points, handles, ..
            } => {
                for p in points.iter_mut() {
                    *p = m.apply_point(p.0, p.1);
                }
                for h in handles.iter_mut() {
                    *h = m.apply_vector(h.0, h.1);
                }
            }
            Shape::Compound { subpaths, .. } => {
                // Transform every sub-contour in place (anchors by the full
                // matrix, handle offsets by its linear part). A compound path has
                // no axis-aligned fast path — it is already an editable path.
                for sp in subpaths.iter_mut() {
                    for p in sp.points.iter_mut() {
                        *p = m.apply_point(p.0, p.1);
                    }
                    for h in sp.handles.iter_mut() {
                        *h = m.apply_vector(h.0, h.1);
                    }
                }
            }
            Shape::Text { .. } => {
                // A general affine (scale / rotate / shear / reflect) on live text
                // would desync the editable params from the transformed glyph
                // cache, so — like Illustrator baking transformed type — convert to
                // glyph outlines (a Compound) and transform those. The text stops
                // being editable as text, but renders / exports exactly.
                *self = self.text_to_outlines();
                self.apply_affine(m);
            }
        }
    }

    /// A closed corner [`Shape::Path`] tracing `ring`, inheriting this shape's
    /// paint style (fill, gradient, stroke colour / width / dashes) and group
    /// tag, but **never** a clip/mask tag (the result is already clipped, so it
    /// renders as a plain shape). Used by clip-mask resolution to turn a content
    /// outline cropped to the mask into a drawable path. All anchors are corners.
    pub fn with_outline(&self, ring: Vec<(f32, f32)>) -> Shape {
        let n = ring.len();
        Shape::Path {
            points: ring,
            closed: true,
            fill: self.fill_color().unwrap_or([0.0, 0.0, 0.0, 0.0]),
            fill_gradient: self.fill_gradient().cloned(),
            stroke: self.stroke_color().unwrap_or([0.0, 0.0, 0.0, 0.0]),
            stroke_w: self.stroke_width(),
            stroke_style: self.stroke_style().clone(),
            // Carry the stacked appearance through so a clipped multi-fill shape
            // keeps its full paint stack after clipping.
            appearance: self.appearance().cloned(),
            handles: vec![(0.0, 0.0); n],
            // The outline replaces the geometry, so the result is a plain path
            // (a clipped polygon / star is no longer parametric).
            live: None,
            visible: self.visible(),
            group: self.group(),
            clip: None,
            mask: false,
            // Carry opacity-mask tags through so a clipped, opacity-masked shape
            // keeps its mask after clipping.
            omask: self.omask(),
            omask_path: self.is_omask(),
            omask_invert: self.omask_invert(),
            // Carry blend tags through so a clipped blend member stays in its set.
            blend: self.blend(),
            blend_step: self.is_blend_step(),
            // Carry the Layers-panel metadata through so a clipped shape keeps
            // its name / lock / colour.
            name: self.name().map(str::to_string),
            locked: self.locked(),
            layer_color: self.layer_color(),
            envelope_mesh: None,
        }
    }

    /// Convert this shape into an equivalent [`Shape::Path`], preserving paint
    /// style. `Rect` becomes a four-corner closed corner-path; `Ellipse` becomes
    /// a four-anchor closed cubic approximation; `Line` becomes a two-point open
    /// path; an existing `Path` is returned unchanged.
    pub fn to_path(&self) -> Shape {
        match self {
            // A compound path is already an editable path object; there is no
            // single-ring `Path` it reduces to without losing its holes, so it is
            // returned unchanged (transform / Pathfinder handle it as a compound).
            Shape::Path { .. } | Shape::Compound { .. } => self.clone(),
            // Text reduces to its glyph outlines (a compound path), the editable
            // form for Pathfinder / direct-select.
            Shape::Text { .. } => self.text_to_outlines(),
            Shape::Rect {
                rect,
                fill,
                fill_gradient,
                stroke,
                stroke_w,
                stroke_style,
                appearance,
                visible,
                group,
                clip,
                mask,
                omask,
                omask_path,
                omask_invert,
                blend,
                blend_step,
                name,
                locked,
                layer_color,
                envelope_mesh: _,
            } => {
                let pts = vec![
                    (rect[0], rect[1]),
                    (rect[0] + rect[2], rect[1]),
                    (rect[0] + rect[2], rect[1] + rect[3]),
                    (rect[0], rect[1] + rect[3]),
                ];
                let handles = vec![(0.0, 0.0); 4];
                Shape::Path {
                    points: pts,
                    closed: true,
                    fill: *fill,
                    fill_gradient: fill_gradient.clone(),
                    stroke: *stroke,
                    stroke_w: *stroke_w,
                    stroke_style: stroke_style.clone(),
                    appearance: appearance.clone(),
                    handles,
                    live: None,
                    visible: *visible,
                    group: *group,
                    clip: *clip,
                    mask: *mask,
                    omask: *omask,
                    omask_path: *omask_path,
                    omask_invert: *omask_invert,
                    blend: *blend,
                    blend_step: *blend_step,
                    name: name.clone(),
                    locked: *locked,
                    layer_color: *layer_color,
                    envelope_mesh: None,
                }
            }
            Shape::Ellipse {
                rect,
                fill,
                fill_gradient,
                stroke,
                stroke_w,
                stroke_style,
                appearance,
                visible,
                group,
                clip,
                mask,
                omask,
                omask_path,
                omask_invert,
                blend,
                blend_step,
                name,
                locked,
                layer_color,
                envelope_mesh: _,
            } => {
                // Four anchors at the extrema with the classic 0.5523 cubic
                // tangent so the path traces a smooth ellipse.
                let cx = rect[0] + rect[2] * 0.5;
                let cy = rect[1] + rect[3] * 0.5;
                let rx = rect[2] * 0.5;
                let ry = rect[3] * 0.5;
                const K: f32 = 0.552_284_8; // (4/3)·(√2−1)
                                            // Anchors clockwise from the rightmost point. Out-tangent offsets
                                            // are tangent to the ellipse, scaled by K·radius.
                let points = vec![
                    (cx + rx, cy), // right
                    (cx, cy + ry), // bottom
                    (cx - rx, cy), // left
                    (cx, cy - ry), // top
                ];
                let handles = vec![
                    (0.0, K * ry),  // right anchor: tangent down
                    (-K * rx, 0.0), // bottom anchor: tangent left
                    (0.0, -K * ry), // left anchor: tangent up
                    (K * rx, 0.0),  // top anchor: tangent right
                ];
                Shape::Path {
                    points,
                    closed: true,
                    fill: *fill,
                    fill_gradient: fill_gradient.clone(),
                    stroke: *stroke,
                    stroke_w: *stroke_w,
                    stroke_style: stroke_style.clone(),
                    appearance: appearance.clone(),
                    handles,
                    live: None,
                    visible: *visible,
                    group: *group,
                    clip: *clip,
                    mask: *mask,
                    omask: *omask,
                    omask_path: *omask_path,
                    omask_invert: *omask_invert,
                    blend: *blend,
                    blend_step: *blend_step,
                    name: name.clone(),
                    locked: *locked,
                    layer_color: *layer_color,
                    envelope_mesh: None,
                }
            }
            Shape::Line {
                p0,
                p1,
                stroke,
                stroke_w,
                stroke_style,
                appearance,
                visible,
                group,
                clip,
                mask,
                omask,
                omask_path,
                omask_invert,
                blend,
                blend_step,
                name,
                locked,
                layer_color,
                envelope_mesh: _,
            } => Shape::Path {
                points: vec![*p0, *p1],
                closed: false,
                fill: [0.0, 0.0, 0.0, 0.0],
                fill_gradient: None,
                stroke: *stroke,
                stroke_w: *stroke_w,
                stroke_style: stroke_style.clone(),
                appearance: appearance.clone(),
                handles: vec![(0.0, 0.0); 2],
                live: None,
                visible: *visible,
                group: *group,
                clip: *clip,
                mask: *mask,
                omask: *omask,
                omask_path: *omask_path,
                omask_invert: *omask_invert,
                blend: *blend,
                blend_step: *blend_step,
                name: name.clone(),
                locked: *locked,
                layer_color: *layer_color,
                envelope_mesh: None,
            },
        }
    }

    /// Insert an anchor into this path at segment `seg`, parameter `t`,
    /// preserving shape. No-op (returns `None`) on non-`Path` shapes.
    pub fn insert_anchor(&mut self, seg: usize, t: f32) -> Option<usize> {
        self.drop_live();
        if let Shape::Path {
            points,
            closed,
            handles,
            ..
        } = self
        {
            path::insert_anchor(points, handles, *closed, seg, t)
        } else {
            None
        }
    }

    /// Delete anchor `i` from this path (keeps ≥2 points). No-op on non-`Path`.
    pub fn delete_anchor(&mut self, i: usize) -> bool {
        self.drop_live();
        if let Shape::Path {
            points, handles, ..
        } = self
        {
            path::delete_anchor(points, handles, i)
        } else {
            false
        }
    }

    /// Toggle anchor `i` smooth↔corner on this path. Returns the new smooth
    /// state (`true` = now smooth). No-op (returns `false`) on non-`Path`.
    pub fn toggle_anchor_smooth(&mut self, i: usize) -> bool {
        self.drop_live();
        if let Shape::Path {
            points,
            closed,
            handles,
            ..
        } = self
        {
            path::toggle_anchor_smooth(points, handles, *closed, i)
        } else {
            false
        }
    }

    // --- Direct-select sub-contour access -----------------------------------
    //
    // A `Path` is one contour; a `Compound` is several. The Direct-Select tool
    // edits anchors/handles uniformly across both by addressing a `(contour,
    // anchor)` pair, so these accessors expose the `(points, handles, closed)`
    // triple per contour index. `Rect`/`Ellipse`/`Line` are not directly
    // editable (the tool converts them to paths first), so they expose none.

    /// Number of editable sub-contours: 1 for a `Path`, the sub-path count for a
    /// `Compound`, 0 for anything else.
    pub fn contour_count(&self) -> usize {
        match self {
            Shape::Path { .. } => 1,
            Shape::Compound { subpaths, .. } => subpaths.len(),
            _ => 0,
        }
    }

    /// Read-only `(points, handles, closed)` of sub-contour `c`, if it exists.
    pub fn contour(&self, c: usize) -> Option<ContourRef<'_>> {
        match self {
            Shape::Path {
                points,
                handles,
                closed,
                ..
            } if c == 0 => Some((points, handles, *closed)),
            Shape::Compound { subpaths, .. } => subpaths
                .get(c)
                .map(|sp| (sp.points.as_slice(), sp.handles.as_slice(), sp.closed)),
            _ => None,
        }
    }

    /// Mutable `(points, handles, closed)` of sub-contour `c`, if it exists.
    /// `handles` is resized to match `points` first so callers can index it.
    pub fn contour_mut(&mut self, c: usize) -> Option<ContourMut<'_>> {
        match self {
            Shape::Path {
                points,
                handles,
                closed,
                ..
            } if c == 0 => {
                if handles.len() < points.len() {
                    handles.resize(points.len(), (0.0, 0.0));
                }
                Some((points, handles, *closed))
            }
            Shape::Compound { subpaths, .. } => subpaths.get_mut(c).map(|sp| {
                if sp.handles.len() < sp.points.len() {
                    sp.handles.resize(sp.points.len(), (0.0, 0.0));
                }
                let closed = sp.closed;
                (&mut sp.points, &mut sp.handles, closed)
            }),
            _ => None,
        }
    }

    /// Move anchor `a` of sub-contour `c` to `(x, y)`. Returns `true` on success.
    pub fn set_anchor(&mut self, c: usize, a: usize, x: f32, y: f32) -> bool {
        self.drop_live();
        if let Some((points, _, _)) = self.contour_mut(c) {
            if let Some(p) = points.get_mut(a) {
                *p = (x, y);
                return true;
            }
        }
        false
    }

    /// Set the out-tangent handle of anchor `a` of sub-contour `c` so its out-knob
    /// sits at `(x, y)` (the in-knob mirrors). Returns `true` on success.
    pub fn set_handle(&mut self, c: usize, a: usize, x: f32, y: f32) -> bool {
        self.drop_live();
        if let Some((points, handles, _)) = self.contour_mut(c) {
            if let (Some(&(ax, ay)), Some(h)) = (points.get(a), handles.get_mut(a)) {
                *h = (x - ax, y - ay);
                return true;
            }
        }
        false
    }

    /// Insert an anchor into sub-contour `c` at segment `seg`, parameter `t`.
    pub fn insert_anchor_in(&mut self, c: usize, seg: usize, t: f32) -> Option<usize> {
        let closed = self.contour(c)?.2;
        self.drop_live();
        if let Some((points, handles, _)) = self.contour_mut(c) {
            path::insert_anchor(points, handles, closed, seg, t)
        } else {
            None
        }
    }

    /// Delete anchor `a` from sub-contour `c` (keeps ≥2 points). `true` on remove.
    pub fn delete_anchor_in(&mut self, c: usize, a: usize) -> bool {
        self.drop_live();
        if let Some((points, handles, _)) = self.contour_mut(c) {
            path::delete_anchor(points, handles, a)
        } else {
            false
        }
    }

    /// Toggle anchor `a` of sub-contour `c` smooth↔corner. Returns the new smooth
    /// state (`true` = now smooth). A smooth anchor carries mirrored tangent
    /// handles; a corner carries none (its segments are straight unless the
    /// neighbouring anchor still curves its side).
    pub fn toggle_anchor_smooth_in(&mut self, c: usize, a: usize) -> bool {
        let (closed, was_corner) = match self.contour(c) {
            Some((_, handles, closed)) => (closed, path::is_corner(handles, a)),
            None => return false,
        };
        self.drop_live();
        if let Some((points, handles, _)) = self.contour_mut(c) {
            if was_corner {
                // Corner → smooth needs the neighbour points; snapshot them so the
                // immutable read and the mutable handle write don't alias.
                let pts = points.clone();
                path::make_smooth(&pts, handles, closed, a)
            } else {
                let n = points.len();
                !path::make_corner(handles, n, a)
            }
        } else {
            false
        }
    }

    /// Hit-test a document-space point. Tolerance is in document units (used to
    /// give lines/open paths a clickable thickness).
    pub fn hit(&self, x: f32, y: f32, tol: f32) -> bool {
        match self {
            Shape::Rect { rect, .. } => path::point_in_rect(x, y, rect, tol),
            Shape::Ellipse { rect, .. } => {
                let cx = rect[0] + rect[2] * 0.5;
                let cy = rect[1] + rect[3] * 0.5;
                let rx = (rect[2] * 0.5).max(1e-3);
                let ry = (rect[3] * 0.5).max(1e-3);
                let nx = (x - cx) / (rx + tol);
                let ny = (y - cy) / (ry + tol);
                nx * nx + ny * ny <= 1.0
            }
            Shape::Line { p0, p1, .. } => path::dist_to_segment(x, y, *p0, *p1) <= tol.max(2.0),
            Shape::Path {
                points,
                closed,
                handles,
                ..
            } => {
                // Hit-test against the flattened polyline so curves are clickable.
                let flat = path::flatten(points, handles, *closed);
                if *closed && flat.len() >= 3 && path::point_in_polygon(x, y, &flat) {
                    return true;
                }
                let n = flat.len();
                if n < 2 {
                    return n == 1 && (x - flat[0].0).hypot(y - flat[0].1) <= tol.max(2.0);
                }
                let last = if *closed { n } else { n - 1 };
                for i in 0..last {
                    let a = flat[i];
                    let b = flat[(i + 1) % n];
                    if path::dist_to_segment(x, y, a, b) <= tol.max(2.0) {
                        return true;
                    }
                }
                false
            }
            Shape::Compound {
                subpaths,
                fill_rule,
                ..
            } => {
                // Fill hit-test against all sub-contours under the compound's fill
                // rule (so a click in a hole misses, a click on solid area hits),
                // then the stroke edges so the boundary stays clickable.
                let rings: Vec<Vec<(f32, f32)>> = subpaths
                    .iter()
                    .filter(|s| s.closed)
                    .map(|s| s.flatten())
                    .collect();
                if path::point_in_rings(x, y, &rings, *fill_rule) {
                    return true;
                }
                for sp in subpaths {
                    let flat = sp.flatten();
                    let n = flat.len();
                    if n < 2 {
                        continue;
                    }
                    let last = if sp.closed { n } else { n - 1 };
                    for i in 0..last {
                        if path::dist_to_segment(x, y, flat[i], flat[(i + 1) % n]) <= tol.max(2.0) {
                            return true;
                        }
                    }
                }
                false
            }
            Shape::Text { .. } => {
                // A text object is selected by clicking anywhere inside its
                // bounding box (the way a type object's bounds pick in
                // Illustrator), which is far friendlier than requiring a click on a
                // thin glyph stroke.
                self.bounds()
                    .map(|b| path::point_in_rect(x, y, &[b.x, b.y, b.w, b.h], tol))
                    .unwrap_or(false)
            }
        }
    }
}
