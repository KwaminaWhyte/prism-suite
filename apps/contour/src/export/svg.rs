//! SVG (vector) export. Split out of `export.rs` (mechanical extraction).

use super::*;

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
pub(super) fn placed_image_href(img: &crate::placed_image::PlacedImage) -> Option<String> {
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
pub(super) fn base64_encode(bytes: &[u8]) -> String {
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

