//! `impl Shape`: constructors, layer metadata/flags, outline, text and live-
//! shape methods. Split out of `document/mod.rs` (mechanical extraction).

use super::*;

impl Shape {
    /// A solid-filled, optionally stroked rectangle `[x, y, w, h]` with every
    /// set-membership / mask / live field defaulted off. A convenience
    /// constructor (the editor builds rects inline in `app::tools`); used by the
    /// GPUI host to seed a sample document without restating every field.
    pub fn rect(rect: [f32; 4], fill: [f32; 4], stroke: [f32; 4], stroke_w: f32) -> Shape {
        Shape::Rect {
            rect,
            fill,
            fill_gradient: None,
            stroke,
            stroke_w,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        }
    }

    /// A solid-filled, optionally stroked ellipse inscribed in `[x, y, w, h]`,
    /// with every set-membership / mask / live field defaulted off. Companion to
    /// [`Shape::rect`]; used by the GPUI host's sample document.
    pub fn ellipse(rect: [f32; 4], fill: [f32; 4], stroke: [f32; 4], stroke_w: f32) -> Shape {
        Shape::Ellipse {
            rect,
            fill,
            fill_gradient: None,
            stroke,
            stroke_w,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        }
    }

    /// A solid-filled, optionally stroked editable [`Shape::Path`] from anchor
    /// `points`, per-anchor out-tangent `handles` (an empty vec is filled with
    /// corners), and a `closed` flag, with every set-membership / mask / live
    /// field defaulted off. Companion to [`Shape::rect`] / [`Shape::ellipse`];
    /// the GPUI host's Pen tool commits a freshly drawn path through here so it
    /// doesn't have to restate every variant field.
    pub fn path(
        points: Vec<(f32, f32)>,
        mut handles: Vec<(f32, f32)>,
        closed: bool,
        fill: [f32; 4],
        stroke: [f32; 4],
        stroke_w: f32,
    ) -> Shape {
        handles.resize(points.len(), (0.0, 0.0));
        Shape::Path {
            points,
            closed,
            fill,
            fill_gradient: None,
            stroke,
            stroke_w,
            stroke_style: StrokeStyle::default(),
            handles,
            live: None,
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask: None,
            omask_path: false,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        }
    }

    /// Short human label for the layer list.
    pub fn label(&self) -> &'static str {
        match self {
            Shape::Rect { .. } => "Rectangle",
            Shape::Ellipse { .. } => "Ellipse",
            Shape::Line { .. } => "Line",
            // A live polygon / star labels itself so the layer row reads
            // "Polygon" / "Star" rather than the generic "Path".
            Shape::Path { live: Some(ls), .. } => ls.label(),
            Shape::Path { .. } => "Path",
            Shape::Compound { .. } => "Compound Path",
            Shape::Text { .. } => "Type",
        }
    }

    /// Whether the shape is drawn / exported. Hidden shapes are skipped.
    pub fn visible(&self) -> bool {
        match self {
            Shape::Rect { visible, .. }
            | Shape::Ellipse { visible, .. }
            | Shape::Line { visible, .. }
            | Shape::Path { visible, .. }
            | Shape::Compound { visible, .. }
            | Shape::Text { visible, .. } => *visible,
        }
    }

    /// Flip the visibility flag.
    pub fn toggle_visible(&mut self) {
        match self {
            Shape::Rect { visible, .. }
            | Shape::Ellipse { visible, .. }
            | Shape::Line { visible, .. }
            | Shape::Path { visible, .. }
            | Shape::Compound { visible, .. }
            | Shape::Text { visible, .. } => *visible = !*visible,
        }
    }

    /// Whether the shape is **locked**. A locked shape still renders, but it is
    /// excluded from selection, hit-testing, and editing (Illustrator's lock).
    pub fn locked(&self) -> bool {
        match self {
            Shape::Rect { locked, .. }
            | Shape::Ellipse { locked, .. }
            | Shape::Line { locked, .. }
            | Shape::Path { locked, .. }
            | Shape::Compound { locked, .. }
            | Shape::Text { locked, .. } => *locked,
        }
    }

    /// Set the shape's locked flag.
    pub fn set_locked(&mut self, v: bool) {
        match self {
            Shape::Rect { locked, .. }
            | Shape::Ellipse { locked, .. }
            | Shape::Line { locked, .. }
            | Shape::Path { locked, .. }
            | Shape::Compound { locked, .. }
            | Shape::Text { locked, .. } => *locked = v,
        }
    }

    /// Flip the locked flag.
    pub fn toggle_locked(&mut self) {
        let v = self.locked();
        self.set_locked(!v);
    }

    /// Whether the shape can take part in selection / hit-testing / editing: it
    /// must be both **visible** and **unlocked**. The single predicate the canvas
    /// pick paths and the Layers panel share so the two gates never drift apart.
    pub fn selectable(&self) -> bool {
        self.visible() && !self.locked()
    }

    /// The shape's user-set Layers-panel name, if it has one (`None` falls back to
    /// the generic [`label`](Self::label)).
    pub fn name(&self) -> Option<&str> {
        match self {
            Shape::Rect { name, .. }
            | Shape::Ellipse { name, .. }
            | Shape::Line { name, .. }
            | Shape::Path { name, .. }
            | Shape::Compound { name, .. }
            | Shape::Text { name, .. } => name.as_deref(),
        }
    }

    /// Set (or clear, with an empty string) the shape's Layers-panel name. A blank
    /// name is stored as `None` so the row falls back to the type label.
    pub fn set_name(&mut self, n: &str) {
        let value = {
            let t = n.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        match self {
            Shape::Rect { name, .. }
            | Shape::Ellipse { name, .. }
            | Shape::Line { name, .. }
            | Shape::Path { name, .. }
            | Shape::Compound { name, .. }
            | Shape::Text { name, .. } => *name = value,
        }
    }

    /// The name to show in the Layers panel: the user-set name when present, else
    /// a text object's (first-line) string, else the generic type label.
    pub fn display_name(&self) -> String {
        if let Some(n) = self.name() {
            return n.to_string();
        }
        if let Shape::Text { params, .. } = self {
            let first = params.text.lines().next().unwrap_or("").trim();
            if !first.is_empty() {
                let truncated: String = first.chars().take(24).collect();
                return truncated;
            }
        }
        self.label().to_string()
    }

    /// The shape's Layers-panel colour swatch, if one has been set.
    pub fn layer_color(&self) -> Option<[f32; 4]> {
        match self {
            Shape::Rect { layer_color, .. }
            | Shape::Ellipse { layer_color, .. }
            | Shape::Line { layer_color, .. }
            | Shape::Path { layer_color, .. }
            | Shape::Compound { layer_color, .. }
            | Shape::Text { layer_color, .. } => *layer_color,
        }
    }

    /// Set (or clear, with `None`) the shape's Layers-panel colour swatch.
    pub fn set_layer_color(&mut self, c: Option<[f32; 4]>) {
        match self {
            Shape::Rect { layer_color, .. }
            | Shape::Ellipse { layer_color, .. }
            | Shape::Line { layer_color, .. }
            | Shape::Path { layer_color, .. }
            | Shape::Compound { layer_color, .. }
            | Shape::Text { layer_color, .. } => *layer_color = c,
        }
    }

    /// The shape's group id, if it belongs to one. Shapes sharing an id form a
    /// group that selects / moves / transforms as a unit.
    pub fn group(&self) -> Option<u64> {
        match self {
            Shape::Rect { group, .. }
            | Shape::Ellipse { group, .. }
            | Shape::Line { group, .. }
            | Shape::Path { group, .. }
            | Shape::Compound { group, .. }
            | Shape::Text { group, .. } => *group,
        }
    }

    /// Set (or clear, with `None`) the shape's group membership.
    pub fn set_group(&mut self, g: Option<u64>) {
        match self {
            Shape::Rect { group, .. }
            | Shape::Ellipse { group, .. }
            | Shape::Line { group, .. }
            | Shape::Path { group, .. }
            | Shape::Compound { group, .. }
            | Shape::Text { group, .. } => *group = g,
        }
    }

    /// Clear every *set-membership* tag (group / clip / mask / opacity-mask /
    /// blend) so the shape is a self-contained unit. Used when capturing a
    /// **symbol master** from a selection: the master's geometry and paint are
    /// kept, but its links into the source document's clip/blend/group sets are
    /// dropped so an instance never collides with those sets at resolve time.
    pub fn clear_set_membership(&mut self) {
        self.set_group(None);
        self.set_clip(None);
        self.set_mask(false);
        self.set_omask(None);
        self.set_omask_path(false);
        self.set_omask_invert(false);
        self.set_blend(None);
        self.set_blend_step(false);
    }

    /// The shape's clip-set id, if it belongs to a clipping mask. Shapes sharing
    /// an id form one clip set; the one with [`is_mask`](Self::is_mask) confines
    /// the rest.
    pub fn clip(&self) -> Option<u64> {
        match self {
            Shape::Rect { clip, .. }
            | Shape::Ellipse { clip, .. }
            | Shape::Line { clip, .. }
            | Shape::Path { clip, .. }
            | Shape::Compound { clip, .. }
            | Shape::Text { clip, .. } => *clip,
        }
    }

    /// Set (or clear, with `None`) the shape's clip-set membership.
    pub fn set_clip(&mut self, c: Option<u64>) {
        match self {
            Shape::Rect { clip, .. }
            | Shape::Ellipse { clip, .. }
            | Shape::Line { clip, .. }
            | Shape::Path { clip, .. }
            | Shape::Compound { clip, .. }
            | Shape::Text { clip, .. } => *clip = c,
        }
    }

    /// Whether this shape is the masking path of its clip set.
    pub fn is_mask(&self) -> bool {
        match self {
            Shape::Rect { mask, .. }
            | Shape::Ellipse { mask, .. }
            | Shape::Line { mask, .. }
            | Shape::Path { mask, .. }
            | Shape::Compound { mask, .. }
            | Shape::Text { mask, .. } => *mask,
        }
    }

    /// Flag (or unflag) this shape as the masking path of its clip set.
    pub fn set_mask(&mut self, m: bool) {
        match self {
            Shape::Rect { mask, .. }
            | Shape::Ellipse { mask, .. }
            | Shape::Line { mask, .. }
            | Shape::Path { mask, .. }
            | Shape::Compound { mask, .. }
            | Shape::Text { mask, .. } => *mask = m,
        }
    }

    /// Clear both clip-set tags (id + mask flag), releasing the shape from any
    /// clipping mask. Used by `Object → Clipping Mask → Release`.
    pub fn clear_clip(&mut self) {
        self.set_clip(None);
        self.set_mask(false);
    }

    /// The shape's opacity-mask set id, if it belongs to one. Shapes sharing an
    /// id form one opacity-mask set; the one with [`is_omask`](Self::is_omask)
    /// supplies the luminance that drives the others' alpha.
    pub fn omask(&self) -> Option<u64> {
        match self {
            Shape::Rect { omask, .. }
            | Shape::Ellipse { omask, .. }
            | Shape::Line { omask, .. }
            | Shape::Path { omask, .. }
            | Shape::Compound { omask, .. }
            | Shape::Text { omask, .. } => *omask,
        }
    }

    /// Set (or clear, with `None`) the shape's opacity-mask set membership.
    pub fn set_omask(&mut self, m: Option<u64>) {
        match self {
            Shape::Rect { omask, .. }
            | Shape::Ellipse { omask, .. }
            | Shape::Line { omask, .. }
            | Shape::Path { omask, .. }
            | Shape::Compound { omask, .. }
            | Shape::Text { omask, .. } => *omask = m,
        }
    }

    /// Whether this shape is the luminance mask of its opacity-mask set.
    pub fn is_omask(&self) -> bool {
        match self {
            Shape::Rect { omask_path, .. }
            | Shape::Ellipse { omask_path, .. }
            | Shape::Line { omask_path, .. }
            | Shape::Path { omask_path, .. }
            | Shape::Compound { omask_path, .. }
            | Shape::Text { omask_path, .. } => *omask_path,
        }
    }

    /// Flag (or unflag) this shape as the luminance mask of its opacity-mask set.
    pub fn set_omask_path(&mut self, m: bool) {
        match self {
            Shape::Rect { omask_path, .. }
            | Shape::Ellipse { omask_path, .. }
            | Shape::Line { omask_path, .. }
            | Shape::Path { omask_path, .. }
            | Shape::Compound { omask_path, .. }
            | Shape::Text { omask_path, .. } => *omask_path = m,
        }
    }

    /// Whether this shape's opacity mask is inverted (black reveals, white hides).
    pub fn omask_invert(&self) -> bool {
        match self {
            Shape::Rect { omask_invert, .. }
            | Shape::Ellipse { omask_invert, .. }
            | Shape::Line { omask_invert, .. }
            | Shape::Path { omask_invert, .. }
            | Shape::Compound { omask_invert, .. }
            | Shape::Text { omask_invert, .. } => *omask_invert,
        }
    }

    /// Set whether this shape's opacity mask is inverted.
    pub fn set_omask_invert(&mut self, v: bool) {
        match self {
            Shape::Rect { omask_invert, .. }
            | Shape::Ellipse { omask_invert, .. }
            | Shape::Line { omask_invert, .. }
            | Shape::Path { omask_invert, .. }
            | Shape::Compound { omask_invert, .. }
            | Shape::Text { omask_invert, .. } => *omask_invert = v,
        }
    }

    /// Clear all opacity-mask tags (id + mask flag + invert), releasing the shape
    /// from any opacity mask. Used by `Object ▸ Opacity Mask ▸ Release`.
    pub fn clear_omask(&mut self) {
        self.set_omask(None);
        self.set_omask_path(false);
        self.set_omask_invert(false);
    }

    /// The shape's blend-set id, if it belongs to one. Shapes sharing an id form
    /// one blend run (the two ends plus generated intermediate steps).
    pub fn blend(&self) -> Option<u64> {
        match self {
            Shape::Rect { blend, .. }
            | Shape::Ellipse { blend, .. }
            | Shape::Line { blend, .. }
            | Shape::Path { blend, .. }
            | Shape::Compound { blend, .. }
            | Shape::Text { blend, .. } => *blend,
        }
    }

    /// Set (or clear, with `None`) the shape's blend-set membership.
    pub fn set_blend(&mut self, b: Option<u64>) {
        match self {
            Shape::Rect { blend, .. }
            | Shape::Ellipse { blend, .. }
            | Shape::Line { blend, .. }
            | Shape::Path { blend, .. }
            | Shape::Compound { blend, .. }
            | Shape::Text { blend, .. } => *blend = b,
        }
    }

    /// Whether this shape is a generated intermediate *step* of its blend set (as
    /// opposed to one of the two original ends). Release deletes steps, keeps ends.
    pub fn is_blend_step(&self) -> bool {
        match self {
            Shape::Rect { blend_step, .. }
            | Shape::Ellipse { blend_step, .. }
            | Shape::Line { blend_step, .. }
            | Shape::Path { blend_step, .. }
            | Shape::Compound { blend_step, .. }
            | Shape::Text { blend_step, .. } => *blend_step,
        }
    }

    /// Flag (or unflag) this shape as a generated blend step.
    pub fn set_blend_step(&mut self, s: bool) {
        match self {
            Shape::Rect { blend_step, .. }
            | Shape::Ellipse { blend_step, .. }
            | Shape::Line { blend_step, .. }
            | Shape::Path { blend_step, .. }
            | Shape::Compound { blend_step, .. }
            | Shape::Text { blend_step, .. } => *blend_step = s,
        }
    }

    /// Clear both blend tags (id + step flag), releasing the shape from any blend
    /// set. Used by `Object ▸ Blend ▸ Release` on the surviving ends.
    pub fn clear_blend(&mut self) {
        self.set_blend(None);
        self.set_blend_step(false);
    }

    /// This shape's clip tag pair, for the pure [`clip`](crate::clip) helpers.
    pub fn clip_tag(&self) -> crate::clip::ClipTag {
        crate::clip::ClipTag::new(self.clip(), self.is_mask())
    }

    /// The shape's filled outline as a single closed document-space polygon (the
    /// input both boolean ops and clipping masks consume). `Rect`/`Ellipse`
    /// sample their outline; a closed `Path` flattens its (possibly bezier)
    /// outline; an open `Path` or a `Line` has no fillable region and returns
    /// `None`.
    pub fn outline_polygon(&self) -> Option<Vec<(f32, f32)>> {
        let pts: Vec<(f32, f32)> = match self {
            Shape::Rect { rect, .. } => {
                let (x, y, w, h) = (rect[0], rect[1], rect[2], rect[3]);
                vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
            }
            Shape::Ellipse { rect, .. } => {
                let cx = rect[0] + rect[2] * 0.5;
                let cy = rect[1] + rect[3] * 0.5;
                let rx = rect[2] * 0.5;
                let ry = rect[3] * 0.5;
                (0..64)
                    .map(|i| {
                        let t = i as f32 / 64.0 * std::f32::consts::TAU;
                        (cx + rx * t.cos(), cy + ry * t.sin())
                    })
                    .collect()
            }
            Shape::Path {
                points,
                closed,
                handles,
                ..
            } => {
                if !*closed {
                    return None;
                }
                path::flatten(points, handles, true)
            }
            Shape::Compound { subpaths, .. } => {
                // The outline polygon is the compound's *outer* ring — the
                // largest-area closed sub-contour — so boolean ops / clip masks
                // that consume a single ring treat the compound by its outer
                // boundary. (Hit-testing / rendering use every sub-contour under
                // the fill rule where holes matter.)
                let outer = subpaths
                    .iter()
                    .filter(|s| s.closed)
                    .map(|s| (s.signed_area().abs(), s))
                    .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(_, s)| s)?;
                outer.flatten()
            }
            Shape::Text { glyphs, .. } => {
                // Like a compound: the outer ring is the largest-area glyph
                // contour, so a boolean / clip op against text uses its overall
                // silhouette's biggest piece.
                let outer = glyphs
                    .iter()
                    .filter(|s| s.closed)
                    .map(|s| (s.signed_area().abs(), s))
                    .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(_, s)| s)?;
                outer.flatten()
            }
            Shape::Line { .. } => return None,
        };
        (pts.len() >= 3).then_some(pts)
    }

    /// The editable text parameters, if this is a text object.
    pub fn text_params(&self) -> Option<&TextParams> {
        match self {
            Shape::Text { params, .. } => Some(params),
            _ => None,
        }
    }

    /// Replace this text object's parameters and re-lay-out its glyph cache from
    /// `params` + `origin`. No-op (returns `false`) on a non-text shape. Editing
    /// the string / size / alignment routes through here so the cached outlines
    /// always match the editable model.
    pub fn set_text_params(&mut self, new: TextParams) -> bool {
        if let Shape::Text {
            params,
            origin,
            glyphs,
            ..
        } = self
        {
            *params = new;
            *glyphs = crate::text::layout(params, *origin).0;
            true
        } else {
            false
        }
    }

    /// Re-run text layout into the glyph cache (after an origin change, or to
    /// repair a loaded document whose cache may be stale / absent). No-op on a
    /// non-text shape.
    pub fn text_relayout(&mut self) {
        if let Shape::Text {
            params,
            origin,
            glyphs,
            ..
        } = self
        {
            *glyphs = crate::text::layout(params, *origin).0;
        }
    }

    /// The live-shape parameters (polygon / star), if this path is a live shape.
    pub fn live_shape(&self) -> Option<LiveShape> {
        match self {
            Shape::Path { live, .. } => *live,
            _ => None,
        }
    }

    /// The centre about which a live shape regenerates: the centroid of its
    /// current anchor points (stable under translation, so a moved polygon stays
    /// put when an edit re-generates it). `None` if not a live path / no points.
    pub(super) fn live_center(&self) -> Option<(f32, f32)> {
        if let Shape::Path {
            points,
            live: Some(_),
            ..
        } = self
        {
            if points.is_empty() {
                return None;
            }
            let n = points.len() as f32;
            let (sx, sy) = points
                .iter()
                .fold((0.0f32, 0.0f32), |(ax, ay), &(x, y)| (ax + x, ay + y));
            Some((sx / n, sy / n))
        } else {
            None
        }
    }

    /// Replace this path's live-shape parameters and regenerate its outline about
    /// the current centre. No-op (returns `false`) on a non-live path. Editing a
    /// count / radius / inner-ratio routes through here so the cached geometry
    /// always matches the editable parameters (mirrors [`set_text_params`]).
    ///
    /// [`set_text_params`]: Self::set_text_params
    pub fn set_live_shape(&mut self, new: LiveShape) -> bool {
        let Some(center) = self.live_center() else {
            return false;
        };
        if let Shape::Path {
            points,
            handles,
            closed,
            live,
            ..
        } = self
        {
            let (pts, hs) = new.outline(center);
            *points = pts;
            *handles = hs;
            *closed = true;
            *live = Some(new);
            true
        } else {
            false
        }
    }

    /// Drop the live-shape parameters (if any), demoting a polygon / star to a
    /// plain editable path. Called whenever an anchor / handle is edited directly,
    /// since the hand-edited geometry no longer matches the parameters (this is
    /// Illustrator's behaviour: reshaping a live shape's points expands it).
    pub(super) fn drop_live(&mut self) {
        if let Shape::Path { live, .. } = self {
            *live = None;
        }
    }

    /// Lift a text object's cached glyph outlines into a real editable
    /// [`Shape::Compound`] (Illustrator's *Convert to Outlines*), inheriting the
    /// text's paint / style / membership and filling under the even-odd rule so
    /// glyph counters stay as holes. Returns the original shape unchanged if it is
    /// not a text object.
    pub fn text_to_outlines(&self) -> Shape {
        if let Shape::Text {
            params,
            origin,
            glyphs,
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
        } = self
        {
            // Prefer the live cache; fall back to a fresh layout if it is empty
            // (e.g. a hand-edited file) so convert never yields nothing for
            // non-empty text.
            let subpaths = if glyphs.is_empty() {
                crate::text::layout(params, *origin).0
            } else {
                glyphs.clone()
            };
            Shape::Compound {
                subpaths,
                fill_rule: FillRule::EvenOdd,
                fill: *fill,
                fill_gradient: fill_gradient.clone(),
                stroke: *stroke,
                stroke_w: *stroke_w,
                stroke_style: stroke_style.clone(),
                appearance: appearance.clone(),
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
        } else {
            self.clone()
        }
    }

}
