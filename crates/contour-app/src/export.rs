//! Export the document to external formats: SVG (vector) and PNG (raster).
//!
//! Both exporters iterate the document in paint order (bottom-up) and skip
//! hidden shapes. SVG emits standard `rect`/`ellipse`/`line`/`path` elements;
//! PNG rasterizes via `tiny-skia` into a `Pixmap` sized to the artboard.

use crate::appearance::{Appearance, BlendMode, Effect, Paint};
// NOTE: `Paint` here is the Appearance paint enum; tiny-skia's `Paint` is used
// fully-qualified as `tiny_skia::Paint` in the rasterizer to avoid the clash.
use crate::document::{self, Document, LineCap, LineJoin, Shape, StrokeStyle};
use crate::gradient::{Gradient, GradientKind, SpreadMode};
use tiny_skia::{
    Color as TsColor, FillRule as TsFillRule, GradientStop as TsStop, LineCap as TsCap,
    LineJoin as TsJoin, LinearGradient, Paint as TsPaint, PathBuilder, Pixmap, Point as TsPoint,
    RadialGradient, Rect as TsRect, Shader, SpreadMode as TsSpread, Stroke, StrokeDash, Transform,
};

// --- SVG ---------------------------------------------------------------------

/// Serialize the whole document to a standalone SVG string cropped to one
/// artboard `[ox, oy, w, h]` in document units: the viewBox is `0 0 w h` and the
/// artwork is translated by `(-ox, -oy)` so the chosen artboard's content lands
/// at the SVG origin (matching Illustrator's per-artboard SVG export). Gradient
/// fills are emitted as `<linearGradient>` / `<radialGradient>` defs (one per
/// gradient-filled shape, in user space mapped to the shape's bounding box) and
/// referenced via `fill="url(#…)"`.
pub fn to_svg_artboard(doc: &Document, ab: [f32; 4]) -> String {
    let (ox, oy, w, h) = (ab[0], ab[1], ab[2], ab[3]);
    let body_inner = svg_body(doc);
    let mut s = String::new();
    s.push_str(&svg_open_tag(doc, w, h));
    let translate = ox != 0.0 || oy != 0.0;
    if translate {
        s.push_str(&format!("  <g transform=\"translate({},{})\">\n", -ox, -oy));
    }
    s.push_str(&body_inner);
    if translate {
        s.push_str("  </g>\n");
    }
    s.push_str("</svg>\n");
    s
}

/// Serialize the whole document to a standalone SVG string sized to `(w, h)` in
/// document units, anchored at the document origin (the artboard-at-origin
/// case). A thin wrapper over [`svg_body`] used by the export tests;
/// [`to_svg_artboard`] is the path the editor takes (it adds the crop offset).
#[cfg(test)]
pub fn to_svg(doc: &Document, w: f32, h: f32) -> String {
    let mut s = String::new();
    s.push_str(&svg_open_tag(doc, w, h));
    s.push_str(&svg_body(doc));
    s.push_str("</svg>\n");
    s
}

/// The `<svg>` opening tag (with `width`/`height`/`viewBox`). The `xlink`
/// namespace is declared **only** when the document carries a placed image
/// (whose `<image>` element uses `xlink:href`), so a document with none exports a
/// byte-identical header to before placed-image support.
fn svg_open_tag(doc: &Document, w: f32, h: f32) -> String {
    let xlink = if doc.placed_images.list.iter().any(|i| i.visible) {
        " xmlns:xlink=\"http://www.w3.org/1999/xlink\""
    } else {
        ""
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"{xlink} width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\">\n"
    )
}

/// Build the inner SVG body (`<defs>` for gradients + the shape elements) shared
/// by [`to_svg`] and [`to_svg_artboard`]. Does not emit the `<svg>` wrapper.
fn svg_body(doc: &Document) -> String {
    // Bake placed symbol instances into plain shapes so they export like any
    // other artwork (a no-op clone when there are none).
    let doc = &doc.flattened_for_export();
    // Build the <defs> for every gradient (one per gradient-painted fill/stroke
    // layer) and the layered shape elements. Clipping masks are resolved before
    // emission: a clip mask path drops out, and clipped content is emitted
    // already cropped to the mask outline (so the SVG matches the canvas without
    // needing `<clipPath>` plumbing).
    //
    // Each shape's *effective* Appearance is walked bottom-to-top: a separate SVG
    // element is emitted for every visible fill (filled, no stroke) then every
    // visible stroke (fill=none), so a stacked object becomes a stack of paint
    // layers and a legacy single-fill/stroke object emits one fill + one stroke.
    let mut defs = String::new();
    let mut body = String::new();
    for (i, shape) in doc.render_shapes() {
        if !shape.visible() {
            continue;
        }
        let bbox = shape.bounds().map(|b| [b.x, b.y, b.w, b.h]).unwrap_or([0.0; 4]);
        let appearance = shape.effective_appearance();
        let geom = svg_geom(&shape);
        let mut grad_n = 0usize;
        // Accumulate this shape's paint elements separately so an effect filter
        // can wrap the whole stack in one `<g filter="url(#…)">`.
        let mut shape_body = String::new();

        // Fills, bottom-to-top (only on a fillable geometry).
        if geom.fillable {
            for fill in &appearance.fills {
                if !fill.visible || fill.opacity <= 0.0 {
                    continue;
                }
                let (fill_attr, grad) = paint_to_svg_fill(&fill.paint, fill.opacity);
                let grad_id = grad.map(|g| {
                    let id = format!("grad{i}_{grad_n}");
                    grad_n += 1;
                    defs.push_str("    ");
                    defs.push_str(&gradient_def(&id, &g, &bbox));
                    defs.push('\n');
                    id
                });
                let mut attrs = match grad_id {
                    Some(id) => format!(" fill=\"url(#{id})\""),
                    None => fill_attr,
                };
                attrs.push_str(" stroke=\"none\"");
                attrs.push_str(&blend_style_attr(fill.blend));
                shape_body.push_str("  ");
                shape_body.push_str(&geom.fill_element(&attrs));
                shape_body.push('\n');
            }
        }
        // Strokes, bottom-to-top.
        for stroke in &appearance.strokes {
            if !stroke.visible || stroke.opacity <= 0.0 || stroke.width <= 0.0 {
                continue;
            }
            let (stroke_attr, grad) = paint_to_svg_stroke(&stroke.paint, stroke.opacity);
            let stroke_paint = match grad {
                Some(g) => {
                    let id = format!("grad{i}_{grad_n}");
                    grad_n += 1;
                    defs.push_str("    ");
                    defs.push_str(&gradient_def(&id, &g, &bbox));
                    defs.push('\n');
                    format!(" stroke=\"url(#{id})\"")
                }
                None => stroke_attr,
            };
            let mut attrs = String::from(" fill=\"none\"");
            attrs.push_str(&stroke_paint);
            attrs.push_str(&stroke_geom_attrs(stroke.width, &stroke.style));
            attrs.push_str(&blend_style_attr(stroke.blend));
            // Align-stroke offset and/or arrowhead markers: when present, emit the
            // baked geometry (a `<path>` for the offset / trimmed centerline plus
            // separate marker paths) so the SVG matches the canvas + PNG. The plain
            // case keeps emitting the shared `<rect>`/`<ellipse>`/`<line>`/`<path>`.
            let contour = StrokeContour::of(&shape);
            let svg_decor = contour
                .as_ref()
                .and_then(|c| svg_stroke_decor(c, stroke, &attrs, &stroke_paint));
            match svg_decor {
                Some(markup) => {
                    shape_body.push_str(&markup);
                }
                None => {
                    shape_body.push_str("  ");
                    shape_body.push_str(&geom.element(&attrs));
                    shape_body.push('\n');
                }
            }
        }

        // Opacity mask: emit a luminance `<mask>` def (the mask shape painted in
        // greyscale; white reveals, black hides) and reference it on the content's
        // group, so the SVG masks natively. Inverted masks add an `invert` filter.
        let mask_attr = doc.opacity_mask_of(i).map(|(mask_shape, invert)| {
            let mid = format!("om{i}");
            defs.push_str("    ");
            defs.push_str(&opacity_mask_def(&mid, &mask_shape, invert));
            defs.push('\n');
            format!(" mask=\"url(#{mid})\"")
        });

        // Live effects: emit a standard SVG `<filter>` (feGaussianBlur /
        // feDropShadow) and wrap the shape's paint stack in a group referencing
        // it, so the exported SVG renders the effect natively in any viewer.
        let filter_attr = if appearance.has_active_effects() {
            let fid = format!("fx{i}");
            defs.push_str("    ");
            defs.push_str(&effect_filter_def(&fid, &appearance.effects));
            defs.push('\n');
            Some(format!(" filter=\"url(#{fid})\""))
        } else {
            None
        };

        if filter_attr.is_some() || mask_attr.is_some() {
            // One group carries both the filter and the mask (mask outside the
            // filter so the effect spill is masked too, matching the raster path).
            let attrs = format!(
                "{}{}",
                filter_attr.unwrap_or_default(),
                mask_attr.unwrap_or_default()
            );
            body.push_str(&format!("  <g{attrs}>\n"));
            body.push_str(&shape_body);
            body.push_str("  </g>\n");
        } else {
            body.push_str(&shape_body);
        }
    }

    // Placed / linked raster images, emitted over the shapes (matching the
    // canvas z-order). Each becomes an `<image>` element: an Embedded source is a
    // base64 PNG `data:` URI; a Linked source is an `href` to its file path. A
    // clip ring becomes a `<clipPath>` def the image references. The placement
    // transform maps the natural rect into the document via `transform=`. A no-op
    // when the document places no images (back-compat: byte-identical output).
    for img in &doc.placed_images.list {
        if !img.visible {
            continue;
        }
        if let Some(markup) = svg_placed_image(img, &mut defs) {
            body.push_str(&markup);
        }
    }

    let mut s = String::new();
    if !defs.is_empty() {
        s.push_str("  <defs>\n");
        s.push_str(&defs);
        s.push_str("  </defs>\n");
    }
    s.push_str(&body);
    s
}

/// SVG markup for one placed image (its `<image>` element + any `<clipPath>` def
/// pushed onto `defs`), or `None` for a degenerate / unresolvable image. The
/// element carries the image at its natural pixel size, placed by the
/// placement [`Affine`] as a `transform=` matrix:
///
/// - **Embedded** → a base64 PNG `data:` URI in `href` / `xlink:href` (so the
///   SVG is self-contained).
/// - **Linked** → an `href` / `xlink:href` to the file path (the SVG references
///   the external file, like Illustrator's linked SVG export).
///
/// A clip ring is emitted as a `<clipPath>` of one `<path>` (the ring polygon in
/// **document** space, since the image element is transformed but the clip path
/// is authored in document space) referenced via `clip-path="url(#…)"`.
fn svg_placed_image(
    img: &crate::placed_image::PlacedImage,
    defs: &mut String,
) -> Option<String> {
    let (w, h) = img.natural_size();
    if w == 0 || h == 0 {
        return None;
    }
    let href = placed_image_href(img)?;
    // A clip ring lives in document space; emit it as a clipPath the image's
    // transformed element references. `clipPathUnits="userSpaceOnUse"` keeps the
    // ring's coordinates in the document frame regardless of the image transform.
    let clip_attr = if let Some(ring) = img.clip.as_deref() {
        if ring.len() >= 3 {
            let cid = format!("imgclip{}", img.id);
            let d = polyline_d(ring, true);
            defs.push_str(&format!(
                "    <clipPath id=\"{cid}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{d}\" /></clipPath>\n"
            ));
            format!(" clip-path=\"url(#{cid})\"")
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let transform = img
        .svg_transform()
        .map(|m| format!(" transform=\"{m}\""))
        .unwrap_or_default();
    Some(format!(
        "  <image{clip_attr} x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" \
         preserveAspectRatio=\"none\" href=\"{href}\" xlink:href=\"{href}\"{transform} />\n"
    ))
}

/// The `href` value for a placed image's `<image>` element: a base64 PNG
/// `data:` URI for an **Embedded** source, or the file path for a **Linked**
/// source. `None` when an embedded source can't be PNG-encoded.
fn placed_image_href(img: &crate::placed_image::PlacedImage) -> Option<String> {
    use crate::placed_image::ImageSource;
    match &img.source {
        ImageSource::Embedded {
            width,
            height,
            rgba,
        } => {
            let png = encode_png_rgba(*width, *height, rgba)?;
            Some(format!("data:image/png;base64,{}", base64_encode(&png)))
        }
        ImageSource::Linked { path, .. } => Some(path.to_string_lossy().into_owned()),
    }
}

/// Encode straight-RGBA8 pixels (`w·h·4` bytes) to in-memory PNG bytes, reusing
/// the `image` crate already in the graph. `None` on a size mismatch / encode
/// error.
fn encode_png_rgba(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || rgba.len() != (w as usize * h as usize * 4) {
        return None;
    }
    let buf: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba.to_vec())?;
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}

/// Standard base64 (RFC 4648, `+/` alphabet, `=` padding). A tiny self-contained
/// encoder so a placed image's embedded pixels become a `data:` URI without
/// pulling in a base64 crate. Pure and unit-tested.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// An SVG geometry token: emits the shape's element (`<rect>`, `<ellipse>`,
/// `<line>`, or `<path>`) with arbitrary paint attributes spliced in, so the
/// Appearance walker can emit the same outline once per fill / stroke layer.
struct SvgGeom {
    /// The element kind + geometry attributes (everything but paint).
    head: String,
    /// Whether the geometry has a fillable region (closed).
    fillable: bool,
    /// `fill-rule` attribute string for a compound path's fill layers (empty for
    /// non-zero / single-ring geometry, so default output stays compact).
    fill_rule_attr: &'static str,
}

impl SvgGeom {
    /// `<tag geom… {paint} />`.
    fn element(&self, paint: &str) -> String {
        format!("{}{} />", self.head, paint)
    }

    /// `<tag geom… {paint} {fill-rule} />` — for a *fill* layer, so an even-odd
    /// compound path carves its holes in any SVG viewer.
    fn fill_element(&self, paint: &str) -> String {
        format!("{}{}{} />", self.head, paint, self.fill_rule_attr)
    }
}

/// Build the geometry token for a shape (no paint).
fn svg_geom(shape: &Shape) -> SvgGeom {
    match shape {
        Shape::Rect { rect, .. } => {
            let (x, y, w, h) = norm_rect(rect);
            SvgGeom {
                head: format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\""),
                fillable: true,
                fill_rule_attr: "",
            }
        }
        Shape::Ellipse { rect, .. } => {
            let (x, y, w, h) = norm_rect(rect);
            let (cx, cy) = (x + w * 0.5, y + h * 0.5);
            let (rx, ry) = (w * 0.5, h * 0.5);
            SvgGeom {
                head: format!("<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\""),
                fillable: true,
                fill_rule_attr: "",
            }
        }
        Shape::Line { p0, p1, .. } => SvgGeom {
            head: format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
                p0.0, p0.1, p1.0, p1.1
            ),
            fillable: false,
            fill_rule_attr: "",
        },
        Shape::Path {
            points,
            handles,
            closed,
            ..
        } => {
            let d = path_d(points, handles, *closed);
            SvgGeom {
                head: format!("<path d=\"{d}\""),
                fillable: *closed,
                fill_rule_attr: "",
            }
        }
        Shape::Compound {
            subpaths,
            fill_rule,
            ..
        } => {
            // One `<path>` whose `d` concatenates every sub-contour, with the
            // compound fill rule so holes carve correctly.
            let mut d = String::new();
            for sp in subpaths {
                if sp.points.is_empty() {
                    continue;
                }
                if !d.is_empty() {
                    d.push(' ');
                }
                d.push_str(&path_d(&sp.points, &sp.handles, sp.closed));
            }
            let fill_rule_attr = match fill_rule {
                document::FillRule::EvenOdd => " fill-rule=\"evenodd\"",
                document::FillRule::NonZero => " fill-rule=\"nonzero\"",
            };
            SvgGeom {
                head: format!("<path d=\"{d}\""),
                fillable: true,
                fill_rule_attr,
            }
        }
        Shape::Text { glyphs, .. } => {
            // Text exports as one even-odd `<path>` concatenating every glyph
            // contour, so glyph counters render as holes in any SVG viewer.
            let mut d = String::new();
            for sp in glyphs {
                if sp.points.is_empty() {
                    continue;
                }
                if !d.is_empty() {
                    d.push(' ');
                }
                d.push_str(&path_d(&sp.points, &sp.handles, sp.closed));
            }
            SvgGeom {
                head: format!("<path d=\"{d}\""),
                fillable: true,
                fill_rule_attr: " fill-rule=\"evenodd\"",
            }
        }
    }
}

/// SVG `fill` attribute for a paint layer (with opacity folded into the colour
/// alpha / `fill-opacity`). Returns the attribute string and, for a gradient,
/// the (opacity-scaled) gradient so the caller can emit a def.
fn paint_to_svg_fill(paint: &Paint, opacity: f32) -> (String, Option<Gradient>) {
    match paint {
        Paint::Solid(c) => {
            let mut a = format!(" fill=\"{}\"", hex(*c));
            let eff = c[3] * opacity;
            if eff < 1.0 {
                a.push_str(&format!(" fill-opacity=\"{:.3}\"", eff.clamp(0.0, 1.0)));
            }
            (a, None)
        }
        Paint::Gradient(g) => (String::new(), Some(scale_grad(g, opacity))),
    }
}

/// SVG `stroke` colour attribute (sans width/dash) for a stroke layer.
fn paint_to_svg_stroke(paint: &Paint, opacity: f32) -> (String, Option<Gradient>) {
    match paint {
        Paint::Solid(c) => {
            let mut a = format!(" stroke=\"{}\"", hex(*c));
            let eff = c[3] * opacity;
            if eff < 1.0 {
                a.push_str(&format!(" stroke-opacity=\"{:.3}\"", eff.clamp(0.0, 1.0)));
            }
            (a, None)
        }
        Paint::Gradient(g) => (String::new(), Some(scale_grad(g, opacity))),
    }
}

/// Width + caps/joins/dashes attributes for a stroke layer (mirrors the legacy
/// [`paint_attrs`] stroke half, minus the colour).
fn stroke_geom_attrs(width: f32, style: &StrokeStyle) -> String {
    let mut a = format!(" stroke-width=\"{width}\"");
    if style.cap != LineCap::Butt {
        a.push_str(&format!(" stroke-linecap=\"{}\"", style.cap.svg()));
    }
    if style.join != LineJoin::Miter {
        a.push_str(&format!(" stroke-linejoin=\"{}\"", style.join.svg()));
    } else if (style.miter_limit - 4.0).abs() > 1e-3 {
        a.push_str(&format!(" stroke-miterlimit=\"{}\"", style.miter_limit));
    }
    if let Some(runs) = style.normalized_dash() {
        let list = runs
            .iter()
            .map(|v| format!("{v}"))
            .collect::<Vec<_>>()
            .join(",");
        a.push_str(&format!(" stroke-dasharray=\"{list}\""));
        if style.dash_offset != 0.0 {
            a.push_str(&format!(" stroke-dashoffset=\"{}\"", style.dash_offset));
        }
    }
    a
}

/// Clone a gradient with every stop alpha scaled by `opacity`.
fn scale_grad(g: &Gradient, opacity: f32) -> Gradient {
    let mut g = g.clone();
    for s in g.stops.iter_mut() {
        s.color[3] = (s.color[3] * opacity).clamp(0.0, 1.0);
    }
    g
}

/// Number of sub-stops to emit per gradient segment when a perceptual gradient is
/// expanded for SVG / tiny-skia (both interpolate stops in straight-sRGB). 16 is
/// visually indistinguishable from a continuous linear-light ramp.
const PERCEPTUAL_SVG_SAMPLES: usize = 16;

/// Emit a `<linearGradient>` / `<radialGradient>` def for `g` mapped onto the
/// bounding box `bbox`, in user-space coordinates so the geometry is exact.
///
/// Perceptual (linear-light) gradients are pre-expanded into many straight-sRGB
/// sub-stops ([`Gradient::render_stops`]) so SVG's sRGB stop interpolation
/// reproduces the linear-light ramp. **Angle (conic) gradients have no SVG 1.1
/// equivalent**, so they are approximated by a `linearGradient` oriented at the
/// gradient's angle — a documented limitation (canvas + PNG render the true
/// conic sweep; SVG falls back to a directional ramp).
fn gradient_def(id: &str, g: &Gradient, bbox: &[f32; 4]) -> String {
    let stops: String = g
        .render_stops(PERCEPTUAL_SVG_SAMPLES)
        .iter()
        .map(|st| {
            let mut s = format!(
                "<stop offset=\"{:.4}\" stop-color=\"{}\"",
                st.offset,
                hex(st.color)
            );
            if st.color[3] < 1.0 {
                s.push_str(&format!(" stop-opacity=\"{:.3}\"", st.color[3]));
            }
            s.push_str(" />");
            s
        })
        .collect::<Vec<_>>()
        .join("");
    let spread = if g.spread == SpreadMode::Pad {
        String::new()
    } else {
        format!(" spreadMethod=\"{}\"", g.spread.svg())
    };
    match g.kind {
        // Linear, and the Angle fallback (no SVG conic), both emit a linear def.
        GradientKind::Linear | GradientKind::Angle => {
            let (a, b) = crate::gradient::linear_endpoints(bbox, g.angle);
            format!(
                "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" \
                 x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{spread}>{stops}</linearGradient>",
                a.0, a.1, b.0, b.1
            )
        }
        GradientKind::Radial => {
            let ((cx, cy), r) = crate::gradient::radial_params(bbox);
            format!(
                "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" \
                 cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"{spread}>{stops}</radialGradient>"
            )
        }
    }
}

/// Emit a standard SVG `<filter>` for a live-effect stack, chaining one filter
/// primitive per active [`Effect`] (bottom-to-top, matching the canvas / PNG
/// order). Drop Shadow → `feDropShadow`; Gaussian Blur → `feGaussianBlur`. Each
/// primitive consumes the previous one's `result`, and the filter region is
/// widened (`x/y/width/height`) so soft edges aren't clipped. SVG's Gaussian
/// `stdDeviation` ≈ our box-blur radius, so the visual reach matches closely.
fn effect_filter_def(id: &str, effects: &[Effect]) -> String {
    let active: Vec<&Effect> = effects.iter().filter(|e| e.is_active()).collect();
    // Region margin large enough for the widest effect's spill (as a fraction of
    // the filtered object's bbox — SVG filter regions are in objectBoundingBox
    // units by default, so a flat 50% padding is a safe generous default).
    let mut prims = String::new();
    let mut prev: Option<String> = None;
    for (n, e) in active.iter().enumerate() {
        let result = format!("e{n}");
        let in_attr = match &prev {
            Some(p) => format!(" in=\"{p}\""),
            None => " in=\"SourceGraphic\"".to_string(),
        };
        match e {
            Effect::GaussianBlur { radius } => {
                prims.push_str(&format!(
                    "<feGaussianBlur{in_attr} stdDeviation=\"{radius}\" result=\"{result}\" />"
                ));
            }
            Effect::DropShadow {
                dx,
                dy,
                blur,
                color,
                opacity,
            } => {
                let flood = (color[3] * opacity).clamp(0.0, 1.0);
                prims.push_str(&format!(
                    "<feDropShadow{in_attr} dx=\"{dx}\" dy=\"{dy}\" stdDeviation=\"{blur}\" \
                     flood-color=\"{}\" flood-opacity=\"{:.3}\" result=\"{result}\" />",
                    hex(*color),
                    flood
                ));
            }
            Effect::Extrude3D {
                depth,
                angle_deg,
                color,
            } => {
                // Simulate extrude via offset composite: flood the extrude color
                // behind the source at (dx, dy) derived from depth + angle.
                let rad = angle_deg.to_radians();
                let dx = depth * rad.cos();
                let dy = depth * rad.sin();
                prims.push_str(&format!(
                    "<feDropShadow{in_attr} dx=\"{dx:.1}\" dy=\"{dy:.1}\" \
                     stdDeviation=\"0\" flood-color=\"{}\" flood-opacity=\"1\" result=\"{result}\" />",
                    hex(*color)
                ));
            }
            Effect::Glow { radius, color, opacity } => {
                let flood = (color[3] * opacity).clamp(0.0, 1.0);
                prims.push_str(&format!(
                    "<feDropShadow{in_attr} dx=\"0\" dy=\"0\" stdDeviation=\"{radius}\" \
                     flood-color=\"{}\" flood-opacity=\"{:.3}\" result=\"{result}\" />",
                    hex(*color),
                    flood
                ));
            }
        }
        prev = Some(result);
    }
    format!(
        "<filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\">{prims}</filter>"
    )
}

/// Emit a luminance `<mask>` def for an opacity mask: the mask shape is painted
/// (its effective fill swatch as the greyscale luminance source) inside a
/// `mask-type="luminance"` element, so any SVG viewer multiplies the masked
/// content's alpha by the mask's luminance — white reveals, black hides. An
/// inverted mask wraps the painted geometry so `1 − luminance` drives the alpha
/// (achieved by flooding the mask region white and subtracting the shape).
fn opacity_mask_def(id: &str, mask_shape: &Shape, invert: bool) -> String {
    let geom = svg_geom(mask_shape);
    let ap = mask_shape.effective_appearance();
    // Representative greyscale colour: the top visible fill's swatch (or white so
    // a stroke-only mask still reveals where it paints).
    let swatch = ap
        .fills
        .iter()
        .rev()
        .find(|f| f.visible && f.opacity > 0.0)
        .map(|f| f.paint.swatch())
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let body = if invert {
        // White backdrop minus the mask shape painted black → inverted luminance.
        format!(
            "<rect x=\"-100%\" y=\"-100%\" width=\"300%\" height=\"300%\" fill=\"#ffffff\" />\
             {}",
            geom.element(" fill=\"#000000\" stroke=\"none\"")
        )
    } else {
        geom.element(&format!(" fill=\"{}\" stroke=\"none\"", hex(swatch)))
    };
    format!("<mask id=\"{id}\" mask-type=\"luminance\">{body}</mask>")
}

/// A `style="mix-blend-mode:…"` attribute for a non-`Normal` paint layer (empty
/// for `Normal`), so an exported fill / stroke composites in any SVG viewer the
/// same way the canvas and PNG do. `Normal` emits nothing (the default).
fn blend_style_attr(blend: BlendMode) -> String {
    if blend.is_separable_blend() {
        format!(" style=\"mix-blend-mode:{}\"", blend.css())
    } else {
        String::new()
    }
}

/// `[f32;4]` straight sRGB -> `#rrggbb`.
fn hex(c: [f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn norm_rect(rect: &[f32; 4]) -> (f32, f32, f32, f32) {
    let x = rect[0].min(rect[0] + rect[2]);
    let y = rect[1].min(rect[1] + rect[3]);
    (x, y, rect[2].abs(), rect[3].abs())
}

/// Build the SVG `d` attribute for a path, emitting `C` (cubic) commands for
/// curved segments and `L` for straight ones.
fn path_d(points: &[(f32, f32)], handles: &[(f32, f32)], closed: bool) -> String {
    let n = points.len();
    if n == 0 {
        return String::new();
    }
    let mut d = format!("M {} {}", points[0].0, points[0].1);
    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = points[i];
        let b = points[(i + 1) % n];
        let ha = document::handle_at(handles, i);
        let hb = document::handle_at(handles, (i + 1) % n);
        let a_corner = ha.0 == 0.0 && ha.1 == 0.0;
        let b_corner = hb.0 == 0.0 && hb.1 == 0.0;
        if a_corner && b_corner {
            d.push_str(&format!(" L {} {}", b.0, b.1));
        } else {
            let c1 = (a.0 + ha.0, a.1 + ha.1);
            let c2 = (b.0 - hb.0, b.1 - hb.1);
            d.push_str(&format!(
                " C {} {} {} {} {} {}",
                c1.0, c1.1, c2.0, c2.1, b.0, b.1
            ));
        }
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

/// An SVG path `d` for a flat polyline (closed appends `Z`).
fn polyline_d(pts: &[(f32, f32)], closed: bool) -> String {
    if pts.is_empty() {
        return String::new();
    }
    let mut d = format!("M {} {}", pts[0].0, pts[0].1);
    for p in &pts[1..] {
        d.push_str(&format!(" L {} {}", p.0, p.1));
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

/// SVG markup for a decorated stroke layer (align offset and/or arrowheads), or
/// `None` when the layer needs no decoration (the caller emits the shared
/// element). `stroke_attrs` is the full stroke attribute string (paint + width +
/// caps/joins/dashes + blend); `stroke_paint` is just the colour attribute (for
/// the marker fills / arms). Emits the offset / trimmed centerline as one
/// `<path>` plus a `<path>` per arrowhead marker.
fn svg_stroke_decor(
    contour: &StrokeContour,
    stroke: &crate::appearance::Stroke,
    stroke_attrs: &str,
    stroke_paint: &str,
) -> Option<String> {
    use crate::document::StrokeAlign;
    let style = &stroke.style;
    let needs_align = style.align != StrokeAlign::Center;
    let needs_arrows = style.has_arrows() && !contour.closed;
    if !needs_align && !needs_arrows {
        return None;
    }
    let flat = if needs_align {
        crate::stroke::aligned_geometry(
            &contour.points,
            &contour.handles,
            contour.closed,
            stroke.width,
            style.align,
        )
    } else {
        crate::document::flatten(&contour.points, &contour.handles, contour.closed)
    };
    let mut line = flat.clone();
    let mut markers = String::new();
    // The arrow fill/arm colour: reuse the stroke paint colour attribute as a
    // `fill` for filled heads and a `stroke` for the chevron arms.
    let fill_color = stroke_paint.replacen("stroke=", "fill=", 1);
    if needs_arrows {
        let (decos, trimmed) = crate::stroke::arrow_decorations(&flat, style, stroke.width);
        if !decos.is_empty() {
            line = trimmed;
            for g in decos {
                if g.fill {
                    markers.push_str(&format!(
                        "  <path d=\"{}\"{} stroke=\"none\" />\n",
                        polyline_d(&g.polygon, true),
                        fill_color
                    ));
                }
                for arm in g.strokes {
                    markers.push_str(&format!(
                        "  <path d=\"{}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linecap=\"{}\" />\n",
                        polyline_d(&arm, false),
                        stroke_paint,
                        stroke.width,
                        style.cap.svg()
                    ));
                }
            }
        }
    }
    let close_main = contour.closed && needs_align && !needs_arrows;
    let mut out = format!(
        "  <path d=\"{}\"{} />\n",
        polyline_d(&line, close_main),
        stroke_attrs
    );
    out.push_str(&markers);
    Some(out)
}

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
fn draw_placed_image_skia(
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

fn draw_shape_skia(pixmap: &mut Pixmap, shape: &Shape, id: Transform, omask: Option<&(Shape, bool)>) {
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
fn ts_stops(g: &Gradient, samples: usize) -> Vec<TsStop> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Shape;

    fn sample_doc() -> Document {
        Document {
            shapes: vec![
                Shape::Rect {
                    rect: [10.0, 10.0, 40.0, 30.0],
                    fill: [1.0, 0.0, 0.0, 1.0],
                    fill_gradient: None,
                    stroke: [0.0, 0.0, 0.0, 1.0],
                    stroke_w: 2.0,
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
                },
                Shape::Path {
                    points: vec![(60.0, 60.0), (90.0, 60.0), (90.0, 90.0)],
                    closed: true,
                    fill: [0.0, 0.0, 1.0, 1.0],
                    fill_gradient: None,
                    stroke: [0.0, 0.0, 0.0, 1.0],
                    stroke_w: 1.0,
                    stroke_style: StrokeStyle::default(),
                    appearance: None,
                    handles: vec![(10.0, 0.0), (0.0, 0.0), (0.0, 0.0)],
                    live: None,
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
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn svg_contains_elements_and_curve() {
        let svg = to_svg(&sample_doc(), 200.0, 200.0);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<path"));
        assert!(svg.contains(" C "), "curved segment should emit a cubic");
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn svg_skips_hidden_shapes() {
        let mut doc = sample_doc();
        if let Shape::Rect { visible, .. } = &mut doc.shapes[0] {
            *visible = false;
        }
        let svg = to_svg(&doc, 200.0, 200.0);
        assert!(!svg.contains("<rect"));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn png_encodes_nonempty() {
        let bytes = to_png(&sample_doc(), 200.0, 200.0).expect("png should encode");
        assert!(bytes.len() > 8);
        // PNG magic signature.
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    /// A non-origin artboard crop offsets the SVG body with a `translate(...)`
    /// group and sizes the viewBox to the artboard, not the document origin.
    #[test]
    fn svg_artboard_crop_offsets_body() {
        let svg = to_svg_artboard(&sample_doc(), [120.0, 40.0, 200.0, 150.0]);
        assert!(
            svg.contains("viewBox=\"0 0 200 150\""),
            "viewBox sized to the artboard: {svg}"
        );
        assert!(
            svg.contains("translate(-120,-40)"),
            "artwork translated to the artboard origin: {svg}"
        );
        // The origin case adds no translate group.
        let at_origin = to_svg_artboard(&sample_doc(), [0.0, 0.0, 200.0, 150.0]);
        assert!(!at_origin.contains("translate"), "no offset at origin");
    }

    /// A cropped PNG still encodes to a valid image at the artboard pixel size.
    #[test]
    fn png_artboard_crop_encodes() {
        let bytes =
            to_png_artboard(&sample_doc(), [50.0, 25.0, 64.0, 48.0]).expect("png should encode");
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    fn dashed_rect() -> Document {
        Document {
            shapes: vec![Shape::Rect {
                rect: [10.0, 10.0, 80.0, 60.0],
                fill: [0.0, 0.0, 0.0, 0.0],
                fill_gradient: None,
                stroke: [0.0, 0.0, 0.0, 1.0],
                stroke_w: 4.0,
                stroke_style: StrokeStyle {
                    cap: LineCap::Round,
                    join: LineJoin::Round,
                    miter_limit: 4.0,
                    dash: vec![12.0, 6.0],
                    dash_offset: 3.0,
                    ..StrokeStyle::default()
                },
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
            }],
            ..Default::default()
        }
    }

    #[test]
    fn svg_emits_dash_and_cap_attrs() {
        let svg = to_svg(&dashed_rect(), 200.0, 200.0);
        assert!(svg.contains("stroke-dasharray=\"12,6\""), "svg: {svg}");
        assert!(svg.contains("stroke-dashoffset=\"3\""), "svg: {svg}");
        assert!(svg.contains("stroke-linecap=\"round\""), "svg: {svg}");
        assert!(svg.contains("stroke-linejoin=\"round\""), "svg: {svg}");
    }

    #[test]
    fn svg_omits_default_stroke_attrs() {
        // A solid butt/miter stroke must not emit cap/join/dash attributes.
        let svg = to_svg(&sample_doc(), 200.0, 200.0);
        assert!(!svg.contains("stroke-linecap"));
        assert!(!svg.contains("stroke-linejoin"));
        assert!(!svg.contains("stroke-dasharray"));
    }

    #[test]
    fn png_encodes_dashed_stroke() {
        // Dashed/round-cap stroking must not crash the rasterizer.
        let bytes = to_png(&dashed_rect(), 120.0, 100.0).expect("png should encode");
        assert!(bytes.len() > 8);
    }

    // --- Align stroke + arrowheads ------------------------------------------

    /// An open line with an end triangle arrowhead and a start circle.
    fn arrowed_line() -> Document {
        Document {
            shapes: vec![Shape::Line {
                p0: (20.0, 50.0),
                p1: (180.0, 50.0),
                stroke: [0.0, 0.0, 0.0, 1.0],
                stroke_w: 6.0,
                stroke_style: StrokeStyle {
                    start_arrow: crate::document::Arrowhead::Circle,
                    end_arrow: crate::document::Arrowhead::Triangle,
                    arrow_scale: 1.5,
                    ..StrokeStyle::default()
                },
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
            }],
            ..Default::default()
        }
    }

    /// A rect stroked with the requested align.
    fn aligned_rect(align: crate::document::StrokeAlign) -> Document {
        Document {
            shapes: vec![Shape::Rect {
                rect: [40.0, 40.0, 100.0, 80.0],
                fill: [0.0, 0.0, 0.0, 0.0],
                fill_gradient: None,
                stroke: [0.0, 0.0, 0.0, 1.0],
                stroke_w: 12.0,
                stroke_style: StrokeStyle {
                    align,
                    ..StrokeStyle::default()
                },
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
            }],
            ..Default::default()
        }
    }

    #[test]
    fn svg_emits_arrowhead_marker_geometry() {
        let svg = to_svg(&arrowed_line(), 200.0, 100.0);
        // The decorated line emits its centerline + the markers as `<path>`s
        // (baked geometry), not the bare `<line>` element.
        assert!(
            svg.matches("<path").count() >= 3,
            "expected baked centerline + 2 marker paths, got: {svg}"
        );
    }

    #[test]
    fn png_encodes_arrowheads() {
        let bytes = to_png(&arrowed_line(), 200.0, 100.0).expect("png should encode");
        assert!(bytes.len() > 8);
    }

    #[test]
    fn svg_align_emits_offset_path_not_bare_rect() {
        use crate::document::StrokeAlign;
        // A non-center align replaces the `<rect>` stroke element with a baked
        // offset `<path>`; center keeps the plain `<rect>`.
        let outside = to_svg(&aligned_rect(StrokeAlign::Outside), 200.0, 200.0);
        assert!(outside.contains("<path"), "outside should bake a path: {outside}");
        let center = to_svg(&aligned_rect(StrokeAlign::Center), 200.0, 200.0);
        // Center keeps the rect element (no baked stroke path needed).
        assert!(center.contains("<rect"), "center keeps rect: {center}");
    }

    #[test]
    fn png_align_inside_vs_outside_differ() {
        use crate::document::StrokeAlign;
        // Inside vs outside place the stroke band on opposite sides of the path,
        // so the rasterized output must differ.
        let inside = to_png(&aligned_rect(StrokeAlign::Inside), 200.0, 200.0).unwrap();
        let outside = to_png(&aligned_rect(StrokeAlign::Outside), 200.0, 200.0).unwrap();
        assert_ne!(inside, outside, "inside/outside align should render differently");
    }

    fn gradient_doc(kind: GradientKind) -> Document {
        Document {
            shapes: vec![Shape::Rect {
                rect: [0.0, 0.0, 100.0, 100.0],
                fill: [0.5, 0.5, 0.5, 1.0],
                fill_gradient: Some(Gradient::two_stop(
                    kind,
                    [1.0, 0.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0, 1.0],
                )),
                stroke: [0.0, 0.0, 0.0, 0.0],
                stroke_w: 0.0,
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
            }],
            ..Default::default()
        }
    }

    #[test]
    fn svg_emits_linear_gradient_def_and_ref() {
        let svg = to_svg(&gradient_doc(GradientKind::Linear), 100.0, 100.0);
        assert!(svg.contains("<defs>"), "svg: {svg}");
        // Gradient defs are now named per-layer: grad{shape}_{layer}.
        assert!(svg.contains("<linearGradient id=\"grad0_0\""), "svg: {svg}");
        assert!(
            svg.contains("gradientUnits=\"userSpaceOnUse\""),
            "svg: {svg}"
        );
        assert!(svg.contains("<stop offset="), "svg: {svg}");
        // The shape's fill layer references the def rather than a solid colour.
        assert!(svg.contains("fill=\"url(#grad0_0)\""), "svg: {svg}");
        assert!(
            !svg.contains("fill=\"#808080\""),
            "svg should not use solid"
        );
    }

    #[test]
    fn svg_emits_radial_gradient_def() {
        let svg = to_svg(&gradient_doc(GradientKind::Radial), 100.0, 100.0);
        assert!(svg.contains("<radialGradient id=\"grad0_0\""), "svg: {svg}");
        assert!(svg.contains("fill=\"url(#grad0_0)\""), "svg: {svg}");
    }

    #[test]
    fn svg_angle_gradient_falls_back_to_linear_def() {
        // SVG 1.1 has no conic gradient, so Angle is exported as a linear def
        // (documented limitation) — still a valid, referenced gradient.
        let svg = to_svg(&gradient_doc(GradientKind::Angle), 100.0, 100.0);
        assert!(svg.contains("<linearGradient id=\"grad0_0\""), "svg: {svg}");
        assert!(svg.contains("fill=\"url(#grad0_0)\""), "svg: {svg}");
        assert!(!svg.contains("<radialGradient"), "angle != radial");
    }

    #[test]
    fn svg_perceptual_gradient_expands_into_many_stops() {
        // A perceptual gradient is pre-expanded into many sRGB sub-stops so SVG's
        // sRGB stop interpolation reproduces the linear-light ramp; an sRGB one
        // keeps just its authored stops.
        let mut doc = gradient_doc(GradientKind::Linear);
        let count_stops = |svg: &str| svg.matches("<stop offset=").count();
        if let Some(g) = doc.shapes[0].fill_gradient().cloned() {
            // sRGB: exactly the 2 authored stops.
            let mut srgb = g.clone();
            srgb.interpolation = crate::gradient::Interpolation::Srgb;
            doc.shapes[0].set_fill_gradient(Some(srgb));
            let svg = to_svg(&doc, 100.0, 100.0);
            assert_eq!(count_stops(&svg), 2, "sRGB keeps authored stops");
            // Perceptual: expanded to PERCEPTUAL_SVG_SAMPLES+1 stops.
            let mut perc = g;
            perc.interpolation = crate::gradient::Interpolation::Perceptual;
            doc.shapes[0].set_fill_gradient(Some(perc));
            let svg = to_svg(&doc, 100.0, 100.0);
            assert!(count_stops(&svg) > 2, "perceptual expands the stop list");
        } else {
            panic!("gradient_doc should have a gradient");
        }
    }

    #[test]
    fn ts_stops_maps_and_expands_correctly() {
        // The pure mapping converts every Contour stop into one tiny-skia stop;
        // its colour-space behaviour is the gradient's `render_stops` expansion
        // (verified per-colour in gradient.rs). Here we lock the cardinality and
        // the empty-gradient guard, since tiny-skia's stop fields are private.
        let srgb = Gradient {
            interpolation: crate::gradient::Interpolation::Srgb,
            ..Gradient::two_stop(GradientKind::Linear, [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.5])
        };
        assert_eq!(ts_stops(&srgb, 16).len(), 2, "sRGB maps 1:1");
        assert_eq!(
            ts_stops(&srgb, 16).len(),
            srgb.render_stops(16).len(),
            "one tiny-skia stop per Contour stop"
        );

        // Perceptual gradient: expanded to samples+1 tiny-skia stops.
        let perc = Gradient {
            interpolation: crate::gradient::Interpolation::Perceptual,
            ..Gradient::two_stop(GradientKind::Linear, [0.0; 4], [1.0, 1.0, 1.0, 1.0])
        };
        assert_eq!(ts_stops(&perc, 8).len(), 9, "perceptual expands");

        // An empty gradient yields no stops (the shader builder then bails out).
        let empty = Gradient {
            stops: vec![],
            ..Gradient::default()
        };
        assert!(ts_stops(&empty, 16).is_empty());
    }

    #[test]
    fn png_renders_angle_gradient_conic_sweep() {
        // An Angle (conic) gradient sweeps around the centre, so opposite radial
        // directions land at different parameters → different colours. Compare a
        // point above the centre with one to the right.
        let doc = gradient_doc(GradientKind::Angle);
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &doc.shapes[0], Transform::identity(), None);
        let px = |x: u32, y: u32| {
            let p = pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        // Right of centre and below centre sit at different sweep angles.
        let right = px(95, 50);
        let down = px(50, 95);
        assert_ne!(right, down, "conic sweep should vary by angle: {right:?} {down:?}");
    }

    #[test]
    fn png_renders_gradient_fill() {
        // A linear red→blue gradient should leave the left edge reddish and the
        // right edge bluish in the rasterized output.
        let doc = gradient_doc(GradientKind::Linear);
        let bytes = to_png(&doc, 100.0, 100.0).expect("png should encode");
        assert!(bytes.len() > 8);

        // Re-rasterize to a pixmap directly so we can sample pixels.
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &doc.shapes[0], Transform::identity(), None);
        let px = |x: u32, y: u32| {
            let p = pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        let (lr, _, lb) = px(2, 50);
        let (rr, _, rb) = px(97, 50);
        // Left is more red than blue; right is more blue than red.
        assert!(lr > lb, "left should be reddish: {lr},{lb}");
        assert!(rb > rr, "right should be bluish: {rr},{rb}");
    }

    /// A shape with two stacked fills + two strokes emits a paint layer per item
    /// in the SVG (bottom-to-top), so the stack survives export.
    #[test]
    fn svg_emits_stacked_paint_layers() {
        use crate::appearance::{Appearance, Fill, Stroke as AppStroke};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 50.0, 50.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 1.0],
            stroke_w: 1.0,
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
        };
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([1.0, 0.0, 0.0, 1.0]),
                Fill::solid([0.0, 1.0, 0.0, 1.0]),
            ],
            strokes: vec![
                AppStroke::solid([0.0, 0.0, 1.0, 1.0], 2.0),
                AppStroke::solid([1.0, 1.0, 1.0, 1.0], 6.0),
            ],
            effects: vec![],
        }));
        let doc = Document {
            shapes: vec![s],
            ..Default::default()
        };
        let svg = to_svg(&doc, 100.0, 100.0);
        // Two fill colours + two stroke colours present as separate elements.
        assert!(svg.matches("<rect").count() == 4, "4 paint layers: {svg}");
        assert!(svg.contains("fill=\"#ff0000\""), "bottom fill: {svg}");
        assert!(svg.contains("fill=\"#00ff00\""), "top fill: {svg}");
        assert!(svg.contains("stroke=\"#0000ff\""), "bottom stroke: {svg}");
        assert!(svg.contains("stroke=\"#ffffff\""), "top stroke: {svg}");
    }

    /// A stacked PNG paints the top fill over the bottom one (last-on-top), so the
    /// centre samples the topmost opaque fill's colour.
    #[test]
    fn png_renders_top_of_fill_stack() {
        use crate::appearance::{Appearance, Fill};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 100.0, 100.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
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
        };
        // Bottom red, top opaque green → centre reads green.
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([1.0, 0.0, 0.0, 1.0]),
                Fill::solid([0.0, 1.0, 0.0, 1.0]),
            ],
            strokes: vec![],
            effects: vec![],
        }));
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        let p = pixmap.pixel(50, 50).unwrap();
        assert!(
            p.green() > p.red() && p.green() > p.blue(),
            "top green fill wins: {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    // --- Blend-mode compositing ---------------------------------------------

    /// A stacked PNG with a Multiply top fill must darken where it overlaps the
    /// bottom fill (Multiply composites against the backdrop, not source-over).
    #[test]
    fn png_blend_multiply_darkens_against_backdrop() {
        use crate::appearance::{Appearance, BlendMode, Fill};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 100.0, 100.0],
            fill: [1.0, 1.0, 1.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
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
        };
        // Bottom 60% grey, top 60% grey Multiply → 0.36 grey (much darker than
        // either layer alone, which a source-over top fill could never produce).
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([0.6, 0.6, 0.6, 1.0]),
                Fill {
                    paint: Paint::Solid([0.6, 0.6, 0.6, 1.0]),
                    opacity: 1.0,
                    blend: BlendMode::Multiply,
                    visible: true,
                },
            ],
            strokes: vec![],
            effects: vec![],
        }));
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        let p = pixmap.pixel(50, 50).unwrap();
        let expected = (0.36_f32 * 255.0).round() as i32;
        assert!(
            (p.red() as i32 - expected).abs() <= 6,
            "multiply should darken to ~{expected}, got {}",
            p.red()
        );
    }

    /// SVG export tags a non-Normal paint layer with `mix-blend-mode` so it
    /// composites in any viewer; a Normal layer emits no blend style.
    #[test]
    fn svg_emits_mix_blend_mode_for_non_normal() {
        use crate::appearance::{Appearance, BlendMode, Fill};
        let mut s = Shape::Rect {
            rect: [0.0, 0.0, 50.0, 50.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
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
        };
        s.set_appearance(Some(Appearance {
            fills: vec![
                Fill::solid([1.0, 0.0, 0.0, 1.0]), // Normal: no style
                Fill {
                    paint: Paint::Solid([0.0, 0.0, 1.0, 1.0]),
                    opacity: 1.0,
                    blend: BlendMode::Screen,
                    visible: true,
                },
            ],
            strokes: vec![],
            effects: vec![],
        }));
        let doc = Document {
            shapes: vec![s],
            ..Default::default()
        };
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(
            svg.contains("mix-blend-mode:screen"),
            "non-normal layer gets a blend style: {svg}"
        );
        // Exactly one blend style (the Normal layer emits none).
        assert_eq!(svg.matches("mix-blend-mode").count(), 1, "svg: {svg}");
    }

    // --- Opacity masks ------------------------------------------------------

    fn omask_rect(rect: [f32; 4], fill: [f32; 4], omask: Option<u64>, mask: bool) -> Shape {
        Shape::Rect {
            rect,
            fill,
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
            stroke_style: StrokeStyle::default(),
            appearance: None,
            visible: true,
            group: None,
            clip: None,
            mask: false,
            omask,
            omask_path: mask,
            omask_invert: false,
            blend: None,
            blend_step: false,
            name: None,
            locked: false,
            layer_color: None,
            envelope_mesh: None,
        }
    }

    /// PNG: a content shape masked by a half-covering white rect is opaque under
    /// the mask and erased outside it (luminance·coverage drives alpha).
    #[test]
    fn png_opacity_mask_reveals_under_mask_hides_outside() {
        let mut doc = Document::new();
        doc.shapes.clear();
        // Red content fills the left 100×100; a white mask covers only its left
        // half (0..50). Under the mask → red shows; right of it → erased to white.
        doc.shapes
            .push(omask_rect([0.0, 0.0, 100.0, 100.0], [1.0, 0.0, 0.0, 1.0], Some(0), false));
        doc.shapes
            .push(omask_rect([0.0, 0.0, 50.0, 100.0], [1.0, 1.0, 1.0, 1.0], Some(0), true));

        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        for (i, shape) in doc.render_shapes() {
            let mask = doc.opacity_mask_of(i);
            draw_shape_skia(&mut pixmap, &shape, Transform::identity(), mask.as_ref());
        }
        // Under the white mask (x=25): red shows through.
        let under = pixmap.pixel(25, 50).unwrap();
        assert!(
            under.red() > 200 && under.green() < 80 && under.blue() < 80,
            "under mask should be red: {},{},{}",
            under.red(),
            under.green(),
            under.blue()
        );
        // Outside the mask (x=75): content hidden → page white shows.
        let outside = pixmap.pixel(75, 50).unwrap();
        assert!(
            outside.red() > 240 && outside.green() > 240 && outside.blue() > 240,
            "outside mask should be white page: {},{},{}",
            outside.red(),
            outside.green(),
            outside.blue()
        );
    }

    /// SVG: an opacity-masked shape emits a luminance `<mask>` def and references
    /// it on the content's group.
    #[test]
    fn svg_emits_opacity_mask_def_and_ref() {
        let mut doc = Document::new();
        doc.shapes.clear();
        doc.shapes
            .push(omask_rect([0.0, 0.0, 100.0, 100.0], [1.0, 0.0, 0.0, 1.0], Some(0), false));
        doc.shapes
            .push(omask_rect([0.0, 0.0, 50.0, 100.0], [1.0, 1.0, 1.0, 1.0], Some(0), true));
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(svg.contains("<mask id=\"om0\""), "mask def: {svg}");
        assert!(svg.contains("mask-type=\"luminance\""), "luminance mask: {svg}");
        assert!(svg.contains("mask=\"url(#om0)\""), "masked group: {svg}");
        // The mask path itself is not emitted as a normal painted shape (only one
        // <rect> for the content's fill, inside the masked group).
    }

    // --- Live effects -------------------------------------------------------

    fn effect_shape(effects: Vec<Effect>) -> Shape {
        use crate::appearance::{Appearance, Fill};
        let mut s = Shape::Rect {
            rect: [40.0, 40.0, 40.0, 40.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
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
        };
        s.set_appearance(Some(Appearance {
            fills: vec![Fill::solid([1.0, 0.0, 0.0, 1.0])],
            strokes: vec![],
            effects,
        }));
        s
    }

    /// A shape with a drop shadow + blur emits an SVG `<filter>` with the
    /// matching primitives and wraps the paint stack in a filtered group.
    #[test]
    fn svg_emits_effect_filter() {
        let doc = Document {
            shapes: vec![effect_shape(vec![
                Effect::drop_shadow(),
                Effect::GaussianBlur { radius: 5.0 },
            ])],
            ..Default::default()
        };
        let svg = to_svg(&doc, 200.0, 200.0);
        assert!(svg.contains("<filter id=\"fx0\""), "filter def: {svg}");
        assert!(svg.contains("<feDropShadow"), "drop-shadow primitive: {svg}");
        assert!(svg.contains("<feGaussianBlur"), "blur primitive: {svg}");
        assert!(svg.contains("filter=\"url(#fx0)\""), "filtered group: {svg}");
    }

    /// A shape with no active effect emits no filter (back-compat: plain output).
    #[test]
    fn svg_no_filter_without_effects() {
        let doc = Document {
            shapes: vec![effect_shape(vec![Effect::GaussianBlur { radius: 0.0 }])],
            ..Default::default()
        };
        let svg = to_svg(&doc, 200.0, 200.0);
        assert!(!svg.contains("<filter"), "no filter for inactive fx: {svg}");
    }

    /// A drop-shadow PNG still encodes, and the shadow paints pixels *outside*
    /// the shape's tight bounds (down-right of it), proving the effect raster is
    /// composited onto the page.
    #[test]
    fn png_drop_shadow_paints_outside_bounds() {
        let doc = Document {
            shapes: vec![effect_shape(vec![Effect::DropShadow {
                dx: 8.0,
                dy: 8.0,
                blur: 3.0,
                color: [0.0, 0.0, 0.0, 1.0],
                opacity: 1.0,
            }])],
            ..Default::default()
        };
        let bytes = to_png(&doc, 200.0, 200.0).expect("png should encode");
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
        // Re-rasterize to sample. Shape spans doc (40,40)-(80,80) on a white page;
        // a point just past the bottom-right corner should be darkened by the
        // shadow (not pure white).
        let mut pixmap = Pixmap::new(200, 200).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &doc.shapes[0], Transform::identity(), None);
        let p = pixmap.pixel(85, 85).unwrap();
        assert!(
            p.red() < 250 && p.green() < 250 && p.blue() < 250,
            "shadow should darken just outside the shape: {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    /// An effect layer's placement: the rasterized layer's `doc_origin` sits at
    /// the padded top-left of the shape (bbox minus the effect padding).
    #[test]
    fn render_shape_layer_pads_bounds() {
        let s = effect_shape(vec![Effect::GaussianBlur { radius: 4.0 }]);
        let (path, fillable) = skia_path_of(&s).unwrap();
        let bbox = s.bounds().map(|b| [b.x, b.y, b.w, b.h]).unwrap();
        let ap = s.effective_appearance();
        let layer = render_shape_layer(&path, fillable, &bbox, &ap, 1.0).unwrap();
        // pad = 3·radius = 12, so the origin is shifted up-left by 12 from (40,40).
        assert!((layer.doc_origin.0 - 28.0).abs() < 1.0, "origin x");
        assert!((layer.doc_origin.1 - 28.0).abs() < 1.0, "origin y");
        // The layer is the padded shape (40 + 2·12 = 64 units) → ~64 px at 1×.
        assert!(layer.pixmap.width() >= 64);
    }

    // --- Compound paths -----------------------------------------------------

    /// A compound path: a 30×30 outer ring with a 10×10 inner hole, even-odd.
    fn donut_compound() -> Shape {
        use crate::document::{FillRule, SubPath};
        Shape::Compound {
            subpaths: vec![
                SubPath::ring(vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]),
                SubPath::ring(vec![
                    (10.0, 10.0),
                    (20.0, 10.0),
                    (20.0, 20.0),
                    (10.0, 20.0),
                ]),
            ],
            fill_rule: FillRule::EvenOdd,
            fill: [1.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
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

    /// SVG export emits a compound path as one `<path>` with `fill-rule="evenodd"`
    /// whose `d` contains both sub-contours (two `M` move commands).
    #[test]
    fn svg_compound_path_has_fill_rule_and_two_subpaths() {
        let doc = Document {
            shapes: vec![donut_compound()],
            ..Default::default()
        };
        let svg = to_svg(&doc, 40.0, 40.0);
        assert!(svg.contains("fill-rule=\"evenodd\""), "even-odd attr: {svg}");
        assert!(svg.contains("<path"), "one path element: {svg}");
        // Two sub-contours → two `M` commands in the single path's `d`.
        let d_start = svg.find("d=\"").expect("has d");
        let d_end = svg[d_start + 3..].find('"').unwrap() + d_start + 3;
        let d = &svg[d_start + 3..d_end];
        assert_eq!(d.matches('M').count(), 2, "two move-tos (outer + hole): {d}");
    }

    /// PNG export rasterizes the even-odd hole: the frame is filled (red) but the
    /// centre (inside the hole) shows the white page.
    #[test]
    fn png_compound_path_renders_the_hole() {
        let s = donut_compound();
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        // On the frame (5,5): red fill.
        let frame = pixmap.pixel(5, 5).unwrap();
        assert!(
            frame.red() > 200 && frame.green() < 80 && frame.blue() < 80,
            "frame is red: {},{},{}",
            frame.red(),
            frame.green(),
            frame.blue()
        );
        // In the hole centre (15,15): even-odd → empty → white page shows.
        let hole = pixmap.pixel(15, 15).unwrap();
        assert!(
            hole.red() > 240 && hole.green() > 240 && hole.blue() > 240,
            "hole shows the white page: {},{},{}",
            hole.red(),
            hole.green(),
            hole.blue()
        );
    }

    /// A non-zero compound (same-wound rings) fills the centre (no hole carved).
    #[test]
    fn png_compound_non_zero_fills_centre() {
        use crate::document::FillRule;
        let mut s = donut_compound();
        if let Shape::Compound { fill_rule, .. } = &mut s {
            *fill_rule = FillRule::NonZero;
        }
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &s, Transform::identity(), None);
        // Same-wound rings + non-zero → the centre is filled red, not a hole.
        let centre = pixmap.pixel(15, 15).unwrap();
        assert!(
            centre.red() > 200 && centre.green() < 80,
            "non-zero fills the centre: {},{},{}",
            centre.red(),
            centre.green(),
            centre.blue()
        );
    }

    /// A text object exports as an even-odd `<path>` of glyph outlines in SVG, and
    /// rasterizes to non-blank pixels in PNG (composing / exporting like any
    /// vector).
    #[test]
    fn text_exports_to_svg_and_png() {
        let params = crate::text::TextParams {
            text: "Ag".to_string(),
            font_size: 60.0,
            align: crate::text::TextAlign::Left,
            font_family: None,
            font_axes: Default::default(),
        };
        let glyphs = crate::text::layout(&params, (20.0, 20.0)).0;
        let text = Shape::Text {
            params,
            origin: (20.0, 20.0),
            glyphs,
            fill: [0.0, 0.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 1.0],
            stroke_w: 0.0,
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
        };
        let doc = Document {
            shapes: vec![text.clone()],
            ..Default::default()
        };
        // SVG: a single even-odd path carrying the glyph contours.
        let svg = to_svg(&doc, 200.0, 120.0);
        assert!(svg.contains("<path"), "text emits a path element");
        assert!(
            svg.contains("fill-rule=\"evenodd\""),
            "text fills even-odd so counters carve"
        );
        // PNG: the text rasterizes to at least some non-white ink.
        let mut pixmap = Pixmap::new(200, 120).unwrap();
        pixmap.fill(TsColor::WHITE);
        draw_shape_skia(&mut pixmap, &text, Transform::identity(), None);
        let inked = pixmap
            .pixels()
            .iter()
            .any(|p| p.red() < 200 || p.green() < 200 || p.blue() < 200);
        assert!(inked, "rasterized text should leave ink on the page");
    }

    // --- Placed / linked images --------------------------------------------

    use crate::placed_image::{ImageSource, PlacedImage};

    /// A solid-colour embedded source `w×h` (straight RGBA8).
    fn solid_embedded(w: u32, h: u32, rgba: [u8; 4]) -> ImageSource {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            px.extend_from_slice(&rgba);
        }
        ImageSource::Embedded {
            width: w,
            height: h,
            rgba: px,
        }
    }

    /// A document with one embedded placed image at `(x, y)`, no shapes.
    fn placed_doc(source: ImageSource, x: f32, y: f32) -> Document {
        let mut doc = Document {
            shapes: vec![],
            ..Default::default()
        };
        doc.placed_images.place("img", source, x, y);
        doc
    }

    /// An embedded placed image exports as an `<image>` with a base64 PNG
    /// `data:` URI in its href.
    #[test]
    fn svg_embedded_image_emits_data_uri() {
        let doc = placed_doc(solid_embedded(4, 4, [255, 0, 0, 255]), 10.0, 20.0);
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(svg.contains("<image"), "image element: {svg}");
        assert!(
            svg.contains("href=\"data:image/png;base64,"),
            "embedded → data URI: {svg}"
        );
        // The placement matrix is carried on the element.
        assert!(svg.contains("matrix(1 0 0 1 10 20)"), "transform: {svg}");
        // The xlink namespace is declared once a placed image is present.
        assert!(svg.contains("xmlns:xlink"), "xlink ns declared: {svg}");
    }

    /// A linked placed image exports as an `<image>` whose href is the file path
    /// (not a data URI).
    #[test]
    fn svg_linked_image_emits_href_path() {
        let src = ImageSource::Linked {
            path: std::path::PathBuf::from("/tmp/photo.png"),
            width: 8,
            height: 6,
        };
        let svg = to_svg(&placed_doc(src, 0.0, 0.0), 100.0, 100.0);
        assert!(svg.contains("<image"), "image element: {svg}");
        assert!(
            svg.contains("href=\"/tmp/photo.png\""),
            "linked → path href: {svg}"
        );
        assert!(!svg.contains("data:image"), "linked is not a data URI: {svg}");
    }

    /// A clipped placed image emits a `<clipPath>` def the `<image>` references.
    #[test]
    fn svg_clipped_image_emits_clip_path() {
        let mut doc = placed_doc(solid_embedded(100, 100, [0, 0, 255, 255]), 0.0, 0.0);
        doc.placed_images.list[0].set_clip(vec![
            (10.0, 10.0),
            (50.0, 10.0),
            (50.0, 50.0),
            (10.0, 50.0),
        ]);
        let svg = to_svg(&doc, 100.0, 100.0);
        let cid = format!("imgclip{}", doc.placed_images.list[0].id);
        assert!(svg.contains(&format!("<clipPath id=\"{cid}\"")), "clipPath def: {svg}");
        assert!(
            svg.contains("clipPathUnits=\"userSpaceOnUse\""),
            "clip in document space: {svg}"
        );
        assert!(svg.contains(&format!("clip-path=\"url(#{cid})\"")), "ref: {svg}");
    }

    /// A document with no placed images exports a byte-identical SVG to before
    /// placed-image support (no `<image>`, no xlink namespace, no clipPath).
    #[test]
    fn svg_without_placed_images_is_unchanged() {
        let svg = to_svg(&sample_doc(), 200.0, 200.0);
        assert!(!svg.contains("<image"), "no image element");
        assert!(!svg.contains("xmlns:xlink"), "no xlink namespace");
        assert!(!svg.contains("clipPath"), "no clip path");
        // The header is exactly the legacy one.
        assert!(svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\""
        ));
    }

    /// A hidden placed image is not emitted (and leaves the header unchanged).
    #[test]
    fn svg_hidden_placed_image_is_omitted() {
        let mut doc = placed_doc(solid_embedded(4, 4, [255, 0, 0, 255]), 0.0, 0.0);
        doc.placed_images.list[0].visible = false;
        let svg = to_svg(&doc, 100.0, 100.0);
        assert!(!svg.contains("<image"), "hidden image omitted: {svg}");
        assert!(!svg.contains("xmlns:xlink"), "no xlink for hidden-only doc");
    }

    /// PNG export composites a placed image into the right pixels: a red 40×40
    /// image placed at (30,30) paints red there and leaves the rest white.
    #[test]
    fn png_bakes_placed_image_into_pixels() {
        let doc = placed_doc(solid_embedded(40, 40, [255, 0, 0, 255]), 30.0, 30.0);
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        for img in &doc.placed_images.list {
            draw_placed_image_skia(&mut pixmap, img, Transform::identity());
        }
        // Inside the image (centre at 50,50): red.
        let inside = pixmap.pixel(50, 50).unwrap();
        assert!(
            inside.red() > 200 && inside.green() < 60 && inside.blue() < 60,
            "image pixel is red: {},{},{}",
            inside.red(),
            inside.green(),
            inside.blue()
        );
        // Outside the image (5,5): the white page.
        let outside = pixmap.pixel(5, 5).unwrap();
        assert!(
            outside.red() > 240 && outside.green() > 240 && outside.blue() > 240,
            "outside is white page: {},{},{}",
            outside.red(),
            outside.green(),
            outside.blue()
        );
        // The full export still encodes a valid PNG.
        let bytes = to_png(&doc, 100.0, 100.0).expect("png encodes");
        assert_eq!(&bytes[0..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }

    /// PNG export composites the image in z-order *over* the vector artwork: an
    /// opaque image atop a shape shows the image's colour where they overlap.
    #[test]
    fn png_image_composites_over_shapes_in_z_order() {
        let mut doc = placed_doc(solid_embedded(100, 100, [0, 0, 255, 255]), 0.0, 0.0);
        // A green rect *under* the image (placed images draw last / on top).
        doc.shapes.push(Shape::Rect {
            rect: [0.0, 0.0, 100.0, 100.0],
            fill: [0.0, 1.0, 0.0, 1.0],
            fill_gradient: None,
            stroke: [0.0, 0.0, 0.0, 0.0],
            stroke_w: 0.0,
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
        });
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        let base = Transform::identity();
        for (_, shape) in doc.render_shapes() {
            draw_shape_skia(&mut pixmap, &shape, base, None);
        }
        for img in &doc.placed_images.list {
            draw_placed_image_skia(&mut pixmap, img, base);
        }
        // The opaque blue image wins over the green rect beneath it.
        let p = pixmap.pixel(50, 50).unwrap();
        assert!(
            p.blue() > 200 && p.green() < 60,
            "image (blue) on top of shape (green): {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    /// PNG export honours a clip ring: pixels outside the clip stay the page
    /// colour even though the image's quad covers them.
    #[test]
    fn png_clipped_image_only_paints_inside_clip() {
        let mut doc = placed_doc(solid_embedded(100, 100, [255, 0, 0, 255]), 0.0, 0.0);
        // Clip to a 40×40 box at (10,10)..(50,50).
        doc.placed_images.list[0].set_clip(vec![
            (10.0, 10.0),
            (50.0, 10.0),
            (50.0, 50.0),
            (10.0, 50.0),
        ]);
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        pixmap.fill(TsColor::WHITE);
        for img in &doc.placed_images.list {
            draw_placed_image_skia(&mut pixmap, img, Transform::identity());
        }
        // Inside the clip (30,30): red.
        let inside = pixmap.pixel(30, 30).unwrap();
        assert!(inside.red() > 200 && inside.green() < 60, "inside clip is red");
        // Outside the clip but inside the image quad (80,80): page white.
        let outside = pixmap.pixel(80, 80).unwrap();
        assert!(
            outside.red() > 240 && outside.green() > 240 && outside.blue() > 240,
            "outside clip is white page: {},{},{}",
            outside.red(),
            outside.green(),
            outside.blue()
        );
    }

    /// A document with no placed images rasterizes a byte-identical PNG to before
    /// (the placed-image bake loop is a no-op when the list is empty).
    #[test]
    fn png_without_placed_images_is_unchanged() {
        let doc = sample_doc();
        // Same doc, but force-clear the (already empty) placed-image list to make
        // the "no images" path explicit.
        let mut bare = doc.clone();
        bare.placed_images.list.clear();
        assert_eq!(
            to_png(&doc, 200.0, 200.0),
            to_png(&bare, 200.0, 200.0),
            "an empty placed-image list does not change the raster"
        );
    }

    /// base64 encodes the RFC 4648 test vectors (alphabet + padding).
    #[test]
    fn base64_matches_rfc_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    /// The href selection is by source kind: Embedded → data URI, Linked → path.
    #[test]
    fn placed_image_href_selects_by_source_kind() {
        let embedded = PlacedImage::new(0, "a", solid_embedded(2, 2, [1, 2, 3, 255]), 0.0, 0.0);
        let href = placed_image_href(&embedded).unwrap();
        assert!(href.starts_with("data:image/png;base64,"), "embedded → data");

        let linked = PlacedImage::new(
            1,
            "b",
            ImageSource::Linked {
                path: std::path::PathBuf::from("/a/b.png"),
                width: 2,
                height: 2,
            },
            0.0,
            0.0,
        );
        assert_eq!(placed_image_href(&linked).unwrap(), "/a/b.png", "linked → path");
    }
}
