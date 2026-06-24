//! PNG / raster export via `tiny-skia`. Split out of `export.rs` (mechanical).

use super::*;

// --- PNG ---------------------------------------------------------------------

fn ts_color(c: [f32; 4]) -> TsColor {
    TsColor::from_rgba(
        c[0].clamp(0.0, 1.0),
        c[1].clamp(0.0, 1.0),
        c[2].clamp(0.0, 1.0),
        c[3].clamp(0.0, 1.0),
    )
    .unwrap_or(TsColor::BLACK)
}

/// Rasterize the document to PNG bytes at size `(w, h)` (document units ==
/// output pixels), anchored at the document origin. A thin wrapper over
/// [`to_png_artboard`] used by the export tests; the editor calls
/// [`to_png_artboard`] directly with the active artboard's rectangle.
#[cfg(test)]
pub fn to_png(doc: &Document, w: f32, h: f32) -> Option<Vec<u8>> {
    to_png_artboard(doc, [0.0, 0.0, w, h])
}

/// Rasterize the document cropped to one artboard `[ox, oy, w, h]` (document
/// units == output pixels): the canvas is `w × h` and the artwork is translated
/// by `(-ox, -oy)`, so the chosen artboard's content fills the image. Returns
/// `None` on degenerate sizes / encode error.
pub fn to_png_artboard(doc: &Document, ab: [f32; 4]) -> Option<Vec<u8>> {
    render_artboard_pixmap(doc, ab).and_then(|p| p.encode_png().ok())
}

/// Rasterize the document cropped to one artboard `[ox, oy, w, h]` into a raw
/// straight-RGBA8 buffer (the active artboard at 1 px/doc-unit): returns
/// `(width, height, rgba)` where `rgba.len() == w·h·4`, white-backed and
/// non-premultiplied, ready for direct upload as a `RenderImage` / texture.
/// Shares the exact rasterization path with [`to_png_artboard`] (via
/// [`render_artboard_pixmap`]), so the bytes match the PNG export pixel-for-pixel.
/// Used by the GPUI host's CPU preview bridge. `None` on a degenerate size.
pub fn to_rgba8_artboard(doc: &Document, ab: [f32; 4]) -> Option<(u32, u32, Vec<u8>)> {
    let pixmap = render_artboard_pixmap(doc, ab)?;
    let (w, h) = (pixmap.width(), pixmap.height());
    // The pixmap is white-backed and fully opaque (alpha == 255 everywhere), so
    // its premultiplied bytes equal straight RGBA8 — `take()` hands them back
    // directly, matching the PNG encoder's pixels.
    Some((w, h, pixmap.take()))
}

/// Build the rasterized artboard [`Pixmap`] (premultiplied RGBA8, white-backed)
/// shared by [`to_png_artboard`] (which PNG-encodes it) and [`to_rgba8_artboard`]
/// (which hands back the raw pixels for the GPUI preview). Crops to one artboard
/// `[ox, oy, w, h]` (document units == output pixels), translating the artwork by
/// `(-ox, -oy)`. `None` on a degenerate size.
fn render_artboard_pixmap(doc: &Document, ab: [f32; 4]) -> Option<Pixmap> {
    // Bake placed symbol instances into plain shapes (no-op clone when none).
    let doc = &doc.flattened_for_export();
    let (ox, oy, w, h) = (ab[0], ab[1], ab[2], ab[3]);
    let pw = w.round().max(1.0) as u32;
    let ph = h.round().max(1.0) as u32;
    let mut pixmap = Pixmap::new(pw, ph)?;
    pixmap.fill(TsColor::WHITE);

    let base = Transform::from_translate(-ox, -oy);
    // Clipping masks resolved: mask paths drop out, clipped content is cropped.
    // Opacity masks resolved: the mask path drops out and its luminance is applied
    // to its content shape's alpha (via `render_shapes` / `opacity_mask_of`).
    for (i, shape) in doc.render_shapes() {
        if !shape.visible() {
            continue;
        }
        let mask = doc.opacity_mask_of(i);
        draw_shape_skia(&mut pixmap, &shape, base, mask.as_ref());
    }

    // Placed / linked raster images, composited over the shapes (matching the
    // canvas z-order). Each is drawn through its placement transform and clipped
    // by its clip ring. A no-op when the document places none.
    for img in &doc.placed_images.list {
        if !img.visible {
            continue;
        }
        draw_placed_image_skia(&mut pixmap, img, base);
    }

    Some(pixmap)
}

/// Resolve a placed image's drawable pixels for export: an **Embedded** source
/// yields its bytes directly; a **Linked** source is re-read from its file on
/// disk via the `image` crate. `None` when degenerate or the link is
/// unreadable (the image is then simply not baked, matching the canvas, which
/// skips a link it couldn't read).
fn placed_image_pixels(img: &crate::placed_image::PlacedImage) -> Option<(u32, u32, Vec<u8>)> {
    use crate::placed_image::ImageSource;
    match &img.source {
        ImageSource::Embedded {
            width,
            height,
            rgba,
        } => Some((*width, *height, rgba.clone())),
        ImageSource::Linked { path, .. } => match image::open(path) {
            Ok(decoded) => {
                let r = decoded.to_rgba8();
                let (w, h) = (r.width(), r.height());
                Some((w, h, r.into_raw()))
            }
            Err(e) => {
                log::warn!("placed image link unreadable on export: {e}");
                None
            }
        },
    }
}

/// Composite one placed image into `pixmap` through its placement transform,
/// clipped by its clip ring, at the page's crop offset (`base` = `translate(-ox,
/// -oy)`). The image's natural pixel rect is drawn as a `Pattern`-shaded quad: a
/// tiny-skia [`Pixmap`] built from its straight-RGBA pixels (premultiplied),
/// used as a `Pattern` shader transformed by `base · placement`, filled over the
/// transformed corner quad. A clip ring restricts the fill via a [`Mask`].
pub(super) fn draw_placed_image_skia(
    pixmap: &mut Pixmap,
    img: &crate::placed_image::PlacedImage,
    base: Transform,
) {
    let Some((w, h, rgba)) = placed_image_pixels(img) else {
        return;
    };
    if w == 0 || h == 0 || rgba.len() != (w as usize * h as usize * 4) {
        return;
    }
    // Build a tiny-skia pixmap holding the image's premultiplied pixels.
    let Some(mut src) = Pixmap::new(w, h) else {
        return;
    };
    {
        let dst = src.pixels_mut();
        for (px, chunk) in dst.iter_mut().zip(rgba.chunks_exact(4)) {
            let a = chunk[3] as u32;
            // Straight sRGB → premultiplied (round-to-nearest).
            let pm = |c: u8| ((c as u32 * a + 127) / 255) as u8;
            *px = tiny_skia::PremultipliedColorU8::from_rgba(
                pm(chunk[0]),
                pm(chunk[1]),
                pm(chunk[2]),
                a as u8,
            )
            .unwrap_or_else(|| tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
        }
    }

    // The placement maps the natural rect `[0,0,w,h]` to document space; `base`
    // then applies the artboard crop. The pattern samples image pixels, so its
    // transform is the full document→page map composed with the placement.
    let t = &img.transform;
    let placement = Transform::from_row(t.a, t.b, t.c, t.d, t.e, t.f);
    let pattern_ts = base.pre_concat(placement);
    let shader = tiny_skia::Pattern::new(
        src.as_ref(),
        TsSpread::Pad,
        tiny_skia::FilterQuality::Bilinear,
        1.0,
        pattern_ts,
    );
    let mut paint = TsPaint {
        shader,
        anti_alias: true,
        ..TsPaint::default()
    };
    paint.blend_mode = tiny_skia::BlendMode::SourceOver;

    // The drawn quad: the natural rect's corners under the placement, then the
    // page crop. Filling this exact quad (vs. the whole page) keeps the
    // `Pattern::Pad` edges from smearing past the image.
    let corners = img.corners();
    let Some(quad) = skia_polyline(&corners, true) else {
        return;
    };

    // A clip ring restricts the fill to the masked region (document space; the
    // page crop maps it onto the pixmap).
    let clip_mask = img.clip.as_deref().and_then(|ring| {
        if ring.len() < 3 {
            return None;
        }
        let path = skia_polyline(ring, true)?;
        let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())?;
        mask.fill_path(&path, TsFillRule::Winding, true, base);
        Some(mask)
    });

    pixmap.fill_path(
        &quad,
        &paint,
        TsFillRule::Winding,
        base,
        clip_mask.as_ref(),
    );
}

/// Build a shape's tiny-skia [`Path`](tiny_skia::Path) (document space) and
/// whether it has a fillable region. `None` for a degenerate shape. Shared with
/// the live canvas so its effect raster matches the PNG exporter exactly.
pub(crate) fn skia_path_of(shape: &Shape) -> Option<(tiny_skia::Path, bool)> {
    match shape {
        Shape::Rect { rect, .. } => {
            let (x, y, w, h) = norm_rect(rect);
            TsRect::from_xywh(x, y, w.max(0.01), h.max(0.01))
                .map(|r| (PathBuilder::from_rect(r), true))
        }
        Shape::Ellipse { rect, .. } => {
            let (x, y, w, h) = norm_rect(rect);
            TsRect::from_xywh(x, y, w.max(0.01), h.max(0.01))
                .and_then(|r| {
                    let mut pb = PathBuilder::new();
                    pb.push_oval(r);
                    pb.finish()
                })
                .map(|p| (p, true))
        }
        Shape::Line { p0, p1, .. } => {
            let mut pb = PathBuilder::new();
            pb.move_to(p0.0, p0.1);
            pb.line_to(p1.0, p1.1);
            pb.finish().map(|p| (p, false))
        }
        Shape::Path {
            points,
            closed,
            handles,
            ..
        } => build_skia_path(points, handles, *closed).map(|p| (p, *closed)),
        // A compound path / text is one tiny-skia path with several sub-contours;
        // the fill rule (Winding / EvenOdd) is applied at fill time via
        // [`skia_fill_rule_of`] (text is always even-odd, so its counters carve).
        Shape::Compound { subpaths, .. } | Shape::Text { glyphs: subpaths, .. } => {
            let mut pb = PathBuilder::new();
            let mut any = false;
            for sp in subpaths {
                if sp.points.len() < 2 {
                    continue;
                }
                push_subpath(&mut pb, &sp.points, &sp.handles, sp.closed);
                any = true;
            }
            if !any {
                return None;
            }
            pb.finish().map(|p| (p, true))
        }
    }
}

/// Build a tiny-skia [`Path`](tiny_skia::Path) from a flat polyline (open unless
/// `closed`), or `None` if degenerate.
fn skia_polyline(pts: &[(f32, f32)], closed: bool) -> Option<tiny_skia::Path> {
    if pts.len() < 2 {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    if closed {
        pb.close();
    }
    pb.finish()
}

/// A shape's editable stroke contour (`Rect`/`Ellipse`/`Line` reduced to a
/// `Path`): the flattenable anchor points, their bezier handles, and whether the
/// contour is closed. Passed into the rasterizer so it can build per-stroke
/// align / arrowhead geometry (each stroke layer has its own width + style).
/// `None` for a compound path (multi-contour align/arrows is out of scope) or a
/// degenerate shape — those stroke the shared centered path as before.
#[derive(Clone)]
pub(crate) struct StrokeContour {
    pub points: Vec<(f32, f32)>,
    pub handles: Vec<(f32, f32)>,
    pub closed: bool,
}

impl StrokeContour {
    /// Extract the stroke contour from a shape, or `None` for a compound /
    /// empty shape (which falls back to centered stroking).
    pub fn of(shape: &Shape) -> Option<StrokeContour> {
        if matches!(shape, Shape::Compound { .. }) {
            return None;
        }
        match shape.to_path() {
            Shape::Path {
                points,
                handles,
                closed,
                ..
            } if points.len() >= 2 => Some(StrokeContour {
                points,
                handles,
                closed,
            }),
            _ => None,
        }
    }
}

/// Per-stroke baked decorations: an align-offset / arrow-trimmed stroke path and
/// the arrowhead marker geometry. Built per stroke layer from a [`StrokeContour`]
/// + that layer's width + style.
#[derive(Default)]
struct StrokeDecor {
    /// The path the main stroke follows (align-offset and/or arrow-trimmed).
    /// `None` means stroke the shared centered path unchanged.
    path: Option<tiny_skia::Path>,
    /// Filled arrowhead outlines (triangle / circle).
    arrow_fills: Vec<Vec<(f32, f32)>>,
    /// Open arrowhead arms (the chevron), stroked at the stroke width.
    arrow_strokes: Vec<Vec<(f32, f32)>>,
}

impl StrokeDecor {
    /// Build the decor for one stroke layer over `contour` at `width` + `style`.
    /// Returns an empty decor (cheap, `path == None`) when nothing special is
    /// needed (centered align, no arrows).
    fn build(contour: &StrokeContour, width: f32, style: &StrokeStyle) -> StrokeDecor {
        use crate::document::StrokeAlign;
        let needs_align = style.align != StrokeAlign::Center;
        let needs_arrows = style.has_arrows() && !contour.closed;
        if width <= 0.0 || (!needs_align && !needs_arrows) {
            return StrokeDecor::default();
        }
        let mut decor = StrokeDecor::default();
        // Align: flatten + offset the centerline.
        let flat = if needs_align {
            crate::stroke::aligned_geometry(
                &contour.points,
                &contour.handles,
                contour.closed,
                width,
                style.align,
            )
        } else {
            crate::document::flatten(&contour.points, &contour.handles, contour.closed)
        };
        let mut stroke_line = flat.clone();
        // Arrowheads (open paths only): bake markers + trim the line for filled
        // heads.
        if needs_arrows {
            let (decos, trimmed) = crate::stroke::arrow_decorations(&flat, style, width);
            if !decos.is_empty() {
                for g in decos {
                    if g.fill {
                        decor.arrow_fills.push(g.polygon);
                    }
                    for arm in g.strokes {
                        decor.arrow_strokes.push(arm);
                    }
                }
                stroke_line = trimmed;
            }
        }
        decor.path = skia_polyline(&stroke_line, contour.closed && needs_align && !needs_arrows);
        decor
    }
}

/// The tiny-skia fill rule a shape rasterizes with — `EvenOdd` for an even-odd
/// compound path, `Winding` (non-zero) for everything else (single rings always
/// fill solid).
pub(crate) fn skia_fill_rule_of(shape: &Shape) -> TsFillRule {
    match shape.fill_rule() {
        Some(document::FillRule::EvenOdd) => TsFillRule::EvenOdd,
        _ => TsFillRule::Winding,
    }
}

/// Append one sub-contour (line / cubic segments, optionally closed) to a
/// tiny-skia [`PathBuilder`], the multi-contour primitive a compound path is made
/// of. Mirrors [`build_skia_path`] but does not finish the builder.
fn push_subpath(pb: &mut PathBuilder, points: &[(f32, f32)], handles: &[(f32, f32)], closed: bool) {
    let n = points.len();
    if n < 2 {
        return;
    }
    pb.move_to(points[0].0, points[0].1);
    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = points[i];
        let b = points[(i + 1) % n];
        let ha = document::handle_at(handles, i);
        let hb = document::handle_at(handles, (i + 1) % n);
        let a_corner = ha.0 == 0.0 && ha.1 == 0.0;
        let b_corner = hb.0 == 0.0 && hb.1 == 0.0;
        if a_corner && b_corner {
            pb.line_to(b.0, b.1);
        } else {
            pb.cubic_to(a.0 + ha.0, a.1 + ha.1, b.0 - hb.0, b.1 - hb.1, b.0, b.1);
        }
    }
    if closed {
        pb.close();
    }
}

pub(super) fn draw_shape_skia(pixmap: &mut Pixmap, shape: &Shape, id: Transform, omask: Option<&(Shape, bool)>) {
    // Gradient geometry maps onto the shape's document-space bounding box.
    let bbox = shape
        .bounds()
        .map(|b| [b.x, b.y, b.w, b.h])
        .unwrap_or([0.0; 4]);
    let Some((path, fillable)) = skia_path_of(shape) else {
        return;
    };
    let fill_rule = skia_fill_rule_of(shape);
    let appearance = shape.effective_appearance();
    let mask = omask.and_then(OpacityMaskInput::of);
    let contour = StrokeContour::of(shape);

    // Fast path: no live effects, no opacity mask → paint the stack straight onto
    // the page (blend layers still composite, handled inside paint_appearance_skia).
    if !appearance.has_active_effects() && mask.is_none() {
        paint_appearance_skia(
            pixmap,
            &path,
            fillable,
            fill_rule,
            &bbox,
            &appearance,
            id,
            contour.as_ref(),
        );
        return;
    }

    // Effects and/or an opacity mask present: rasterize the fill/stroke stack into
    // a padded scratch pixmap (at the page's pixel scale, here 1 px/doc-unit
    // because the page `id` transform is a pure translate), apply the effect stack
    // and the mask, then draw the processed raster back onto the page at the right
    // offset. `id` is a pure `translate(-ox, -oy)` so its translation gives the
    // artboard crop offset.
    if let Some(layer) = render_shape_layer_masked(
        &path,
        fillable,
        fill_rule,
        &bbox,
        &appearance,
        1.0,
        mask.as_ref(),
        contour.as_ref(),
    ) {
        let tx = id.tx; // = -ox (artboard crop)
        let ty = id.ty;
        let dst_x = (layer.doc_origin.0 + tx).round() as i32;
        let dst_y = (layer.doc_origin.1 + ty).round() as i32;
        pixmap.draw_pixmap(
            dst_x,
            dst_y,
            layer.pixmap.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
}

/// A rasterized shape layer + where to place it: the processed `pixmap` and the
/// **document-space** coordinate of its top-left pixel (`doc_origin`). Callers
/// map `doc_origin` to their own surface (page pixels for PNG, screen pixels for
/// the canvas) at the same `scale` they passed in.
pub(crate) struct ShapeLayer {
    pub pixmap: Pixmap,
    pub doc_origin: (f32, f32),
}

/// A resolved opacity-mask input for the rasterizer: the mask shape's tiny-skia
/// `path`, whether it has a fillable region, its document-space `bbox` (for any
/// gradient), its effective `appearance` (the luminance source), and whether the
/// mask is inverted. The mask is rasterized into the same scratch as the artwork
/// and multiplied into its alpha by luminance. Owns its appearance so callers can
/// build it from a transient [`Shape::effective_appearance`].
pub(crate) struct OpacityMaskInput {
    pub path: tiny_skia::Path,
    pub fillable: bool,
    pub fill_rule: TsFillRule,
    pub bbox: [f32; 4],
    pub appearance: Appearance,
    pub invert: bool,
}

impl OpacityMaskInput {
    /// Build the mask input for a resolved `(mask_shape, invert)` pair, or `None`
    /// if the mask shape is degenerate. Shared by PNG export and the canvas.
    pub(crate) fn of(mask: &(Shape, bool)) -> Option<Self> {
        let (mask_shape, invert) = mask;
        let (path, fillable) = skia_path_of(mask_shape)?;
        let bbox = mask_shape
            .bounds()
            .map(|b| [b.x, b.y, b.w, b.h])
            .unwrap_or([0.0; 4]);
        Some(Self {
            path,
            fillable,
            fill_rule: skia_fill_rule_of(mask_shape),
            bbox,
            appearance: mask_shape.effective_appearance(),
            invert: *invert,
        })
    }
}

/// Rasterize a shape's effective appearance (fills + strokes) into a padded
/// scratch pixmap at `scale` px/doc-unit, then apply its live effect stack.
/// A thin no-mask wrapper over [`render_shape_layer_masked`], retained for the
/// export tests.
#[cfg(test)]
pub(crate) fn render_shape_layer(
    path: &tiny_skia::Path,
    fillable: bool,
    bbox: &[f32; 4],
    appearance: &Appearance,
    scale: f32,
) -> Option<ShapeLayer> {
    render_shape_layer_masked(
        path,
        fillable,
        TsFillRule::Winding,
        bbox,
        appearance,
        scale,
        None,
        None,
    )
}

/// Rasterize a shape's effective appearance into a padded scratch pixmap, then
/// apply its live effect stack and, last, any opacity mask. Returns the processed
/// layer + its document-space placement, or `None` for a degenerate size. Shared
/// by PNG export and the live canvas so the two surfaces composite effects,
/// blends and masks identically.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_shape_layer_masked(
    path: &tiny_skia::Path,
    fillable: bool,
    fill_rule: TsFillRule,
    bbox: &[f32; 4],
    appearance: &Appearance,
    scale: f32,
    mask: Option<&OpacityMaskInput>,
    contour: Option<&StrokeContour>,
) -> Option<ShapeLayer> {
    let pad = appearance.effect_pad();
    // Padded document-space rect covering the artwork + the effects' spill.
    let dx = bbox[0] - pad;
    let dy = bbox[1] - pad;
    let dw = bbox[2] + 2.0 * pad;
    let dh = bbox[3] + 2.0 * pad;
    let pw = (dw * scale).ceil().max(1.0) as u32;
    let ph = (dh * scale).ceil().max(1.0) as u32;
    // Guard against absurd allocations (e.g. a pathological zoom).
    if pw > 8192 || ph > 8192 {
        return None;
    }
    let mut layer = crate::effects::transparent_pixmap(pw, ph);
    // Map document space into the scratch pixmap: translate the padded origin to
    // (0,0), then scale to pixels.
    let t = Transform::from_scale(scale, scale).post_translate(-dx * scale, -dy * scale);
    paint_appearance_skia(&mut layer, path, fillable, fill_rule, bbox, appearance, t, contour);
    crate::effects::apply_effects(&mut layer, &appearance.effects, scale);
    // Opacity mask: rasterize the mask shape's luminance into a same-size scratch
    // (same transform, so it registers pixel-for-pixel with the artwork), then
    // multiply it into the artwork's alpha. Applied last so it masks the final
    // composited result (artwork + effects), as Illustrator does.
    if let Some(m) = mask {
        let mut mask_pm = crate::effects::transparent_pixmap(pw, ph);
        paint_appearance_skia(
            &mut mask_pm,
            &m.path,
            m.fillable,
            m.fill_rule,
            &m.bbox,
            &m.appearance,
            t,
            None,
        );
        crate::effects::apply_luminance_mask(&mut layer, &mask_pm, m.invert);
    }
    Some(ShapeLayer {
        pixmap: layer,
        doc_origin: (dx, dy),
    })
}

/// Rasterize an [`Appearance`] stack onto `path`: fills bottom-to-top (only when
/// `fillable`), then strokes bottom-to-top, each scaled by its per-item opacity.
///
/// **Blend modes really composite now.** A `Normal` layer is drawn straight onto
/// `pixmap` with `tiny-skia` source-over (the fast path). A non-`Normal` layer is
/// rasterized alone into a transparent scratch pixmap (same size as `pixmap`,
/// same transform) and then composited onto `pixmap` with the separable
/// Porter-Duff blend math in [`crate::effects::composite_blended`], so it blends
/// against everything painted beneath it — closing the long-standing "stored but
/// not composited" Appearance gap.
#[allow(clippy::too_many_arguments)]
fn paint_appearance_skia(
    pixmap: &mut Pixmap,
    path: &tiny_skia::Path,
    fillable: bool,
    fill_rule: TsFillRule,
    bbox: &[f32; 4],
    appearance: &Appearance,
    transform: Transform,
    contour: Option<&StrokeContour>,
) {
    let (w, h) = (pixmap.width(), pixmap.height());
    // Paint one layer's `paint`+`draw` either straight (Normal) or via a blended
    // scratch composite. `draw` rasterizes onto whichever pixmap it is handed.
    let paint_layer = |pixmap: &mut Pixmap,
                       blend: crate::appearance::BlendMode,
                       draw: &dyn Fn(&mut Pixmap)| {
        if !blend.is_separable_blend() {
            draw(pixmap);
            return;
        }
        // Non-Normal: isolate this layer on a transparent scratch, then blend it
        // over the accumulated backdrop.
        let mut scratch = crate::effects::transparent_pixmap(w, h);
        draw(&mut scratch);
        crate::effects::composite_blended(pixmap, &scratch, blend);
    };

    if fillable {
        for fill in &appearance.fills {
            if !fill.visible || fill.opacity <= 0.0 {
                continue;
            }
            let mut paint = TsPaint::default();
            // Scratch + owned gradient must outlive `paint` (a conic Pattern
            // shader borrows the scratch pixmap), so both are declared here.
            let mut grad_scratch: Option<Pixmap> = None;
            let grad_owned;
            match &fill.paint {
                Paint::Solid(c) => {
                    let c = scale_alpha(*c, fill.opacity);
                    if c[3] <= 0.0 {
                        continue;
                    }
                    paint.set_color(ts_color(c));
                }
                Paint::Gradient(g) => {
                    grad_owned = scale_grad(g, fill.opacity);
                    match gradient_shader(&grad_owned, bbox, &mut grad_scratch) {
                        Some(s) => paint.shader = s,
                        None => continue,
                    }
                }
            }
            paint.anti_alias = true;
            let draw = |pm: &mut Pixmap| {
                pm.fill_path(path, &paint, fill_rule, transform, None);
            };
            paint_layer(pixmap, fill.blend, &draw);
        }
    }
    for stroke in &appearance.strokes {
        if !stroke.visible || stroke.opacity <= 0.0 || stroke.width <= 0.0 {
            continue;
        }
        let mut paint = TsPaint::default();
        // Scratch + owned gradient must outlive `paint` (conic Pattern borrows
        // the scratch pixmap).
        let mut grad_scratch: Option<Pixmap> = None;
        let grad_owned;
        match &stroke.paint {
            Paint::Solid(c) => {
                let c = scale_alpha(*c, stroke.opacity);
                if c[3] <= 0.0 {
                    continue;
                }
                paint.set_color(ts_color(c));
            }
            Paint::Gradient(g) => {
                grad_owned = scale_grad(g, stroke.opacity);
                match gradient_shader(&grad_owned, bbox, &mut grad_scratch) {
                    Some(s) => paint.shader = s,
                    None => continue,
                }
            }
        }
        paint.anti_alias = true;
        let s = Stroke {
            width: stroke.width.max(0.01),
            miter_limit: stroke.style.miter_limit.max(1.0),
            line_cap: ts_cap(stroke.style.cap),
            line_join: ts_join(stroke.style.join),
            dash: stroke
                .style
                .normalized_dash()
                .and_then(|runs| StrokeDash::new(runs, stroke.style.dash_offset)),
        };
        // Per-stroke align / arrowhead decorations (each layer has its own width
        // + style). Empty (path == None) for a centered, arrow-less stroke.
        let decor = contour.map(|c| StrokeDecor::build(c, stroke.width, &stroke.style));
        // Align-stroke offset / arrow-trimmed path replaces the centered shared
        // path for stroking; falls back to the shared path.
        let stroke_path: &tiny_skia::Path = decor
            .as_ref()
            .and_then(|d| d.path.as_ref())
            .unwrap_or(path);
        // The arrowhead markers are filled / stroked with the stroke colour
        // (solid stroke path), no dashes.
        let head_paint = {
            let mut p = TsPaint::default();
            p.anti_alias = true;
            if let Paint::Solid(c) = &stroke.paint {
                p.set_color(ts_color(scale_alpha(*c, stroke.opacity)));
            } else {
                p.shader = paint.shader.clone();
            }
            p
        };
        let arm_stroke = Stroke {
            width: stroke.width.max(0.01),
            miter_limit: stroke.style.miter_limit.max(1.0),
            line_cap: ts_cap(stroke.style.cap),
            line_join: ts_join(stroke.style.join),
            dash: None,
        };
        let draw = |pm: &mut Pixmap| {
            pm.stroke_path(stroke_path, &paint, &s, transform, None);
            if let Some(d) = decor.as_ref() {
                for poly in &d.arrow_fills {
                    if let Some(p) = skia_polyline(poly, true) {
                        pm.fill_path(&p, &head_paint, TsFillRule::Winding, transform, None);
                    }
                }
                for arm in &d.arrow_strokes {
                    if let Some(p) = skia_polyline(arm, false) {
                        pm.stroke_path(&p, &head_paint, &arm_stroke, transform, None);
                    }
                }
            }
        };
        paint_layer(pixmap, stroke.blend, &draw);
    }
}

/// Multiply a straight-sRGB RGBA colour's alpha by `opacity`.
fn scale_alpha(mut c: [f32; 4], opacity: f32) -> [f32; 4] {
    c[3] = (c[3] * opacity).clamp(0.0, 1.0);
    c
}

/// Map our gradient [`SpreadMode`] to tiny-skia's.
fn ts_spread(mode: SpreadMode) -> TsSpread {
    match mode {
        SpreadMode::Pad => TsSpread::Pad,
        SpreadMode::Repeat => TsSpread::Repeat,
        SpreadMode::Reflect => TsSpread::Reflect,
    }
}

/// Sub-stops emitted per segment when expanding a perceptual gradient for
/// tiny-skia (which interpolates stops in straight sRGB). Matches the SVG path.
const PERCEPTUAL_SKIA_SAMPLES: usize = 16;

/// Pure mapping from our [`Gradient`] (modeled on the shared
/// `prism_core::gradient`) to tiny-skia [`GradientStop`]s: expand for perceptual
/// interpolation ([`Gradient::render_stops`]), then convert each straight-sRGB
/// stop colour to a tiny-skia [`Color`](TsColor). Factored out so the mapping is
/// unit-testable without rasterizing.
pub(super) fn ts_stops(g: &Gradient, samples: usize) -> Vec<TsStop> {
    g.render_stops(samples)
        .iter()
        .map(|s| TsStop::new(s.offset, ts_color(s.color)))
        .collect()
}

/// Build a tiny-skia gradient [`Shader`] for `g` over the bounding box `bbox`.
/// Returns `None` if the gradient is degenerate (tiny-skia falls back to the
/// solid fill in that case).
///
/// Perceptual (linear-light) gradients are pre-expanded into straight-sRGB
/// sub-stops ([`Gradient::render_stops`]) so tiny-skia's sRGB-space stop
/// interpolation reproduces the linear-light ramp. **Angle (conic) gradients have
/// no native tiny-skia shader**, so the conic sweep is rasterized into a `bbox`-
/// sized pixmap via the shared [`prism_core::gradient`] primitive and returned as
/// a `Pattern` shader (borrowing `scratch`, which must outlive the shader).
fn gradient_shader<'a>(
    g: &Gradient,
    bbox: &[f32; 4],
    scratch: &'a mut Option<Pixmap>,
) -> Option<Shader<'a>> {
    if g.kind == GradientKind::Angle {
        return conic_pattern(g, bbox, scratch);
    }
    let stops = ts_stops(g, PERCEPTUAL_SKIA_SAMPLES);
    if stops.is_empty() {
        return None;
    }
    let mode = ts_spread(g.spread);
    match g.kind {
        GradientKind::Linear => {
            let (a, b) = crate::gradient::linear_endpoints(bbox, g.angle);
            LinearGradient::new(
                TsPoint::from_xy(a.0, a.1),
                TsPoint::from_xy(b.0, b.1),
                stops,
                mode,
                Transform::identity(),
            )
        }
        GradientKind::Radial => {
            let ((cx, cy), r) = crate::gradient::radial_params(bbox);
            RadialGradient::new(
                TsPoint::from_xy(cx, cy),
                TsPoint::from_xy(cx, cy),
                r,
                stops,
                mode,
                Transform::identity(),
            )
        }
        GradientKind::Angle => unreachable!("handled above"),
    }
}

/// Rasterize a conic (angle) gradient into a `bbox`-sized pixmap (in document
/// coordinates, offset to the bbox origin via the returned `Pattern` transform)
/// and return it as a tiny-skia `Pattern` shader. The per-pixel conic sweep
/// (with optional dither) reuses [`crate::gradient::angle_param`] + the
/// gradient's own [`color_at`], so it tracks the perceptual / sRGB toggle and the
/// canvas preview exactly. `scratch` owns the pixmap so the borrowed shader can
/// outlive this call.
fn conic_pattern<'a>(
    g: &Gradient,
    bbox: &[f32; 4],
    scratch: &'a mut Option<Pixmap>,
) -> Option<Shader<'a>> {
    let w = bbox[2].ceil().max(1.0) as u32;
    let h = bbox[3].ceil().max(1.0) as u32;
    let mut pm = Pixmap::new(w, h)?;
    {
        let data = pm.pixels_mut();
        for y in 0..h {
            for x in 0..w {
                // Document-space sample point (pixmap origin == bbox origin).
                let px = bbox[0] + x as f32 + 0.5;
                let py = bbox[1] + y as f32 + 0.5;
                let mut t = crate::gradient::angle_param(bbox, g.angle, px, py);
                if g.dither {
                    t = (t + (bayer8(x, y) - 0.5) / 255.0).clamp(0.0, 1.0);
                }
                let c = g.color_at(t);
                let a = c[3].clamp(0.0, 1.0);
                // tiny-skia `PremultipliedColorU8` expects premultiplied bytes.
                let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                data[(y * w + x) as usize] =
                    tiny_skia::PremultipliedColorU8::from_rgba(
                        to8(c[0] * a),
                        to8(c[1] * a),
                        to8(c[2] * a),
                        to8(a),
                    )
                    .unwrap_or_else(|| {
                        tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap()
                    });
            }
        }
    }
    *scratch = Some(pm);
    let pm_ref = scratch.as_ref().unwrap().as_ref();
    Some(tiny_skia::Pattern::new(
        pm_ref,
        TsSpread::Pad,
        tiny_skia::FilterQuality::Bilinear,
        1.0,
        // Shift the bbox-local pattern into document space.
        Transform::from_translate(bbox[0], bbox[1]),
    ))
}

/// Normalized Bayer 8×8 ordered-dither value in `[0, 1)` for pixel `(x, y)` — the
/// same matrix the shared `prism_core::gradient` uses, so the dither pattern is
/// consistent across the suite.
fn bayer8(x: u32, y: u32) -> f32 {
    const M: [[u8; 8]; 8] = [
        [0, 32, 8, 40, 2, 34, 10, 42],
        [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38],
        [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41],
        [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37],
        [63, 31, 55, 23, 61, 29, 53, 21],
    ];
    let v = M[(y & 7) as usize][(x & 7) as usize];
    (v as f32 + 0.5) / 64.0
}

/// Map our document [`LineCap`] to tiny-skia's.
fn ts_cap(cap: LineCap) -> TsCap {
    match cap {
        LineCap::Butt => TsCap::Butt,
        LineCap::Round => TsCap::Round,
        LineCap::Square => TsCap::Square,
    }
}

/// Map our document [`LineJoin`] to tiny-skia's.
fn ts_join(join: LineJoin) -> TsJoin {
    match join {
        LineJoin::Miter => TsJoin::Miter,
        LineJoin::Round => TsJoin::Round,
        LineJoin::Bevel => TsJoin::Bevel,
    }
}

fn build_skia_path(
    points: &[(f32, f32)],
    handles: &[(f32, f32)],
    closed: bool,
) -> Option<tiny_skia::Path> {
    let n = points.len();
    if n < 2 {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = points[i];
        let b = points[(i + 1) % n];
        let ha = document::handle_at(handles, i);
        let hb = document::handle_at(handles, (i + 1) % n);
        let a_corner = ha.0 == 0.0 && ha.1 == 0.0;
        let b_corner = hb.0 == 0.0 && hb.1 == 0.0;
        if a_corner && b_corner {
            pb.line_to(b.0, b.1);
        } else {
            pb.cubic_to(a.0 + ha.0, a.1 + ha.1, b.0 - hb.0, b.1 - hb.1, b.0, b.1);
        }
    }
    if closed {
        pb.close();
    }
    pb.finish()
}
