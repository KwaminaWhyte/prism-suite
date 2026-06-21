use crate::document::{Document, Shape};
use prism_core::geometry::Rect as CoreRect;

// ============================================================================
// SVG export helpers (Wave 9)
// ============================================================================

pub(super) fn shape_to_svg(shape: &crate::document::Shape) -> String {
    use crate::document::Shape;
    match shape {
        Shape::Rect { rect, fill, stroke, stroke_w, .. } => {
            let [x, y, w, h] = rect;
            let fill_s = rgba_to_hex(*fill);
            let stroke_s = rgba_to_hex(*stroke);
            format!(
                "  <rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" \
                 fill=\"{fill_s}\" stroke=\"{stroke_s}\" stroke-width=\"{stroke_w:.1}\"/>\n"
            )
        }
        Shape::Ellipse { rect, fill, stroke, stroke_w, .. } => {
            let [x, y, w, h] = rect;
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let rx = w * 0.5;
            let ry = h * 0.5;
            let fill_s = rgba_to_hex(*fill);
            let stroke_s = rgba_to_hex(*stroke);
            format!(
                "  <ellipse cx=\"{cx:.1}\" cy=\"{cy:.1}\" rx=\"{rx:.1}\" ry=\"{ry:.1}\" \
                 fill=\"{fill_s}\" stroke=\"{stroke_s}\" stroke-width=\"{stroke_w:.1}\"/>\n"
            )
        }
        Shape::Text { params, origin, fill, .. } => {
            let fill_s = rgba_to_hex(*fill);
            let text = params.text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!(
                "  <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" \
                 fill=\"{fill_s}\">{text}</text>\n",
                origin.0, origin.1, params.font_size
            )
        }
        Shape::Path { points, closed, fill, stroke, stroke_w, handles, .. } => {
            if points.is_empty() {
                return String::new();
            }
            let fill_s = rgba_to_hex(*fill);
            let stroke_s = rgba_to_hex(*stroke);
            let d = path_to_svg_d(points, handles, *closed);
            format!(
                "  <path d=\"{d}\" fill=\"{fill_s}\" stroke=\"{stroke_s}\" \
                 stroke-width=\"{stroke_w:.1}\"/>\n"
            )
        }
        Shape::Line { p0, p1, stroke, stroke_w, .. } => {
            let stroke_s = rgba_to_hex(*stroke);
            format!(
                "  <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"{stroke_s}\" stroke-width=\"{stroke_w:.1}\"/>\n",
                p0.0, p0.1, p1.0, p1.1
            )
        }
        _ => String::new(),
    }
}

pub(super) fn rgba_to_hex(c: [f32; 4]) -> String {
    let r = (c[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub(super) fn path_to_svg_d(points: &[(f32, f32)], handles: &[(f32, f32)], closed: bool) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = format!("M {:.1} {:.1}", points[0].0, points[0].1);
    for i in 1..points.len() {
        let p = points[i];
        let prev = points[i - 1];
        let h_prev = handles.get(i - 1).copied().unwrap_or((0.0, 0.0));
        let h_cur = handles.get(i).copied().unwrap_or((0.0, 0.0));
        if h_prev == (0.0, 0.0) && h_cur == (0.0, 0.0) {
            d.push_str(&format!(" L {:.1} {:.1}", p.0, p.1));
        } else {
            let cp1 = (prev.0 + h_prev.0, prev.1 + h_prev.1);
            let cp2 = (p.0 - h_cur.0, p.1 - h_cur.1);
            d.push_str(&format!(
                " C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                cp1.0, cp1.1, cp2.0, cp2.1, p.0, p.1
            ));
        }
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

// ============================================================================
// SVG import helpers (Wave 9)
// ============================================================================

pub(super) fn import_svg(
    path: &std::path::Path,
) -> Result<Vec<crate::document::Shape>, Box<dyn std::error::Error>> {
    use crate::document::Shape;
    let content = std::fs::read_to_string(path)?;
    let mut shapes = Vec::new();

    fn attr(tag: &str, name: &str) -> Option<String> {
        let pattern = format!("{}=\"", name);
        let start = tag.find(&pattern)? + pattern.len();
        let end = tag[start..].find('"')? + start;
        Some(tag[start..end].to_string())
    }

    fn parse_color(s: &str) -> [f32; 4] {
        let s = s.trim();
        if s.starts_with('#') && s.len() == 7 {
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(0) as f32 / 255.0;
            [r, g, b, 1.0]
        } else if s.starts_with('#') && s.len() == 4 {
            // Short hex #rgb
            let r = u8::from_str_radix(&s[1..2].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..3].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&s[3..4].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            [r, g, b, 1.0]
        } else {
            match s {
                "none" | "transparent" => [0.0, 0.0, 0.0, 0.0],
                "black" => [0.0, 0.0, 0.0, 1.0],
                "white" => [1.0, 1.0, 1.0, 1.0],
                "red" => [1.0, 0.0, 0.0, 1.0],
                "green" => [0.0, 0.5, 0.0, 1.0],
                "blue" => [0.0, 0.0, 1.0, 1.0],
                "yellow" => [1.0, 1.0, 0.0, 1.0],
                "orange" => [1.0, 0.647, 0.0, 1.0],
                "purple" | "violet" => [0.5, 0.0, 0.5, 1.0],
                "cyan" | "aqua" => [0.0, 1.0, 1.0, 1.0],
                "magenta" | "fuchsia" => [1.0, 0.0, 1.0, 1.0],
                "gray" | "grey" => [0.5, 0.5, 0.5, 1.0],
                "silver" => [0.753, 0.753, 0.753, 1.0],
                "darkblue" => [0.0, 0.0, 0.545, 1.0],
                "darkgreen" => [0.0, 0.392, 0.0, 1.0],
                "darkred" => [0.545, 0.0, 0.0, 1.0],
                _ => [0.0, 0.0, 0.0, 1.0],
            }
        }
    }

    fn pf(s: &str) -> f32 {
        s.trim().parse().unwrap_or(0.0)
    }

    let default_fill = [0.2, 0.55, 0.9, 1.0];
    let default_stroke = [0.1, 0.2, 0.35, 1.0];

    // Stack of group ids: each `<g>` pushes a new group id; `</g>` pops.
    let mut group_stack: Vec<u64> = Vec::new();
    let mut next_gid: u64 = 1;

    let mut i = 0;
    while i < content.len() {
        let Some(start) = content[i..].find('<') else { break };
        let abs_start = i + start;
        let Some(end_rel) = content[abs_start..].find('>') else { break };
        let tag = &content[abs_start..abs_start + end_rel + 1];
        i = abs_start + end_rel + 1;

        // Handle group open/close.
        if tag.starts_with("<g") && !tag.starts_with("<gradient") {
            group_stack.push(next_gid);
            next_gid += 1;
            continue;
        }
        if tag.starts_with("</g") {
            group_stack.pop();
            continue;
        }

        let current_group = group_stack.last().copied();

        let fill = attr(tag, "fill")
            .map(|s| parse_color(&s))
            .unwrap_or(default_fill);
        let stroke = attr(tag, "stroke")
            .map(|s| parse_color(&s))
            .unwrap_or(default_stroke);
        let stroke_w = attr(tag, "stroke-width")
            .map(|s| pf(&s))
            .unwrap_or(1.0);

        // Helper to set group on a freshly pushed shape.
        let set_group = |shapes: &mut Vec<Shape>, gid: Option<u64>| {
            if let (Some(gid), Some(s)) = (gid, shapes.last_mut()) {
                s.set_group(Some(gid));
            }
        };

        if tag.starts_with("<rect") {
            let x = attr(tag, "x").map(|s| pf(&s)).unwrap_or(0.0);
            let y = attr(tag, "y").map(|s| pf(&s)).unwrap_or(0.0);
            let w = attr(tag, "width").map(|s| pf(&s)).unwrap_or(0.0);
            let h = attr(tag, "height").map(|s| pf(&s)).unwrap_or(0.0);
            if w > 0.0 && h > 0.0 {
                shapes.push(Shape::rect([x, y, w, h], fill, stroke, stroke_w));
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<line") && !tag.starts_with("<lineargradient") {
            let x1 = attr(tag, "x1").map(|s| pf(&s)).unwrap_or(0.0);
            let y1 = attr(tag, "y1").map(|s| pf(&s)).unwrap_or(0.0);
            let x2 = attr(tag, "x2").map(|s| pf(&s)).unwrap_or(0.0);
            let y2 = attr(tag, "y2").map(|s| pf(&s)).unwrap_or(0.0);
            // SVG lines have no fill; use stroke color (fall back to fill if stroke is transparent).
            let line_stroke = if stroke[3] == 0.0 { fill } else { stroke };
            shapes.push(Shape::Line {
                p0: (x1, y1),
                p1: (x2, y2),
                stroke: line_stroke,
                stroke_w,
                stroke_style: Default::default(),
                appearance: None,
                visible: true,
                group: current_group,
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
            // set_group already handled via group: current_group above
        } else if tag.starts_with("<ellipse") {
            let cx = attr(tag, "cx").map(|s| pf(&s)).unwrap_or(0.0);
            let cy = attr(tag, "cy").map(|s| pf(&s)).unwrap_or(0.0);
            let rx = attr(tag, "rx").map(|s| pf(&s)).unwrap_or(0.0);
            let ry = attr(tag, "ry").map(|s| pf(&s)).unwrap_or(0.0);
            if rx > 0.0 && ry > 0.0 {
                shapes.push(Shape::ellipse(
                    [cx - rx, cy - ry, rx * 2.0, ry * 2.0],
                    fill,
                    stroke,
                    stroke_w,
                ));
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<circle") {
            let cx = attr(tag, "cx").map(|s| pf(&s)).unwrap_or(0.0);
            let cy = attr(tag, "cy").map(|s| pf(&s)).unwrap_or(0.0);
            let r = attr(tag, "r").map(|s| pf(&s)).unwrap_or(0.0);
            if r > 0.0 {
                shapes.push(Shape::ellipse(
                    [cx - r, cy - r, r * 2.0, r * 2.0],
                    fill,
                    stroke,
                    stroke_w,
                ));
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<text") {
            let x = attr(tag, "x").map(|s| pf(&s)).unwrap_or(0.0);
            let y = attr(tag, "y").map(|s| pf(&s)).unwrap_or(0.0);
            let font_size = attr(tag, "font-size").map(|s| pf(&s)).unwrap_or(16.0);
            let text_start = abs_start + end_rel + 1;
            let text_end = content[text_start..]
                .find("</text>")
                .map(|e| text_start + e)
                .unwrap_or(text_start);
            let text = content[text_start..text_end].trim().to_string();
            if !text.is_empty() {
                let params = crate::text::TextParams {
                    text: text.clone(),
                    font_size,
                    ..Default::default()
                };
                let glyphs = crate::text::layout(&params, (x, y)).0;
                shapes.push(Shape::Text {
                    params,
                    origin: (x, y),
                    glyphs,
                    fill,
                    fill_gradient: None,
                    stroke,
                    stroke_w,
                    stroke_style: Default::default(),
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
                set_group(&mut shapes, current_group);
            }
        } else if tag.starts_with("<path") {
            if let Some(d) = attr(tag, "d") {
                let (pts, hs) = parse_svg_path_d(&d);
                if pts.len() >= 2 {
                    let closed = d.trim_end().ends_with('Z') || d.trim_end().ends_with('z');
                    shapes.push(Shape::path(pts, hs, closed, fill, stroke, stroke_w));
                    set_group(&mut shapes, current_group);
                }
            }
        }
    }
    Ok(shapes)
}

pub(super) fn parse_svg_path_d(d: &str) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut handles: Vec<(f32, f32)> = Vec::new();
    let mut nums: Vec<f32> = Vec::new();
    let mut cur_cmd = ' ';
    let mut cur_x = 0.0f32;
    let mut cur_y = 0.0f32;

    fn flush(
        nums: &mut Vec<f32>,
        cmd: char,
        cur_x: &mut f32,
        cur_y: &mut f32,
        points: &mut Vec<(f32, f32)>,
        handles: &mut Vec<(f32, f32)>,
    ) {
        match cmd.to_ascii_uppercase() {
            'M' | 'L' => {
                if nums.len() >= 2 {
                    *cur_x = nums[0];
                    *cur_y = nums[1];
                    points.push((*cur_x, *cur_y));
                    handles.push((0.0, 0.0));
                }
            }
            'C' => {
                if nums.len() >= 6 {
                    let end_x = nums[4];
                    let end_y = nums[5];
                    if let Some(h) = handles.last_mut() {
                        *h = (nums[0] - *cur_x, nums[1] - *cur_y);
                    }
                    let h_new = (end_x - nums[2], end_y - nums[3]);
                    *cur_x = end_x;
                    *cur_y = end_y;
                    points.push((*cur_x, *cur_y));
                    handles.push(h_new);
                }
            }
            _ => {}
        }
        nums.clear();
    }

    let mut token = String::new();
    for ch in d.chars() {
        if ch.is_alphabetic() {
            if !token.is_empty() {
                if let Ok(v) = token.parse::<f32>() {
                    nums.push(v);
                }
                token.clear();
            }
            if cur_cmd != ' ' || ch.to_ascii_uppercase() == 'M' {
                flush(&mut nums, cur_cmd, &mut cur_x, &mut cur_y, &mut points, &mut handles);
            }
            cur_cmd = ch;
        } else if ch == ',' || ch == ' ' {
            if !token.is_empty() {
                if let Ok(v) = token.parse::<f32>() {
                    nums.push(v);
                }
                token.clear();
            }
        } else if ch == '-' && !token.is_empty() {
            if let Ok(v) = token.parse::<f32>() {
                nums.push(v);
            }
            token.clear();
            token.push(ch);
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        if let Ok(v) = token.parse::<f32>() {
            nums.push(v);
        }
    }
    flush(&mut nums, cur_cmd, &mut cur_x, &mut cur_y, &mut points, &mut handles);

    (points, handles)
}

pub(super) fn rand_group_id(_doc: &crate::document::Document) -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}

// --- Batch 8: serde default helpers ---
pub(super) fn default_image_trace_threshold() -> f32 { 128.0 }
pub(super) fn default_image_trace_colors() -> u8 { 6 }
pub(super) fn default_omask_id_counter() -> u64 { 1 }
pub(super) fn default_graph_style_fill() -> [f32; 4] { [0.2, 0.5, 0.9, 1.0] }

/// Whether two straight-sRGB RGBA colours are close enough to be considered the
/// same fill for the Recolor panel (tolerance 1/255 ≈ 0.004 per channel).
pub(super) fn colors_approx_equal(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.01)
}

/// Expand or contract a closed polygon ring by `distance` document units using
/// the averaged-normal (Minkwoski-sum approximation) method. Each vertex is
/// displaced outward (positive) or inward (negative) along the averaged
/// unit normal of its two adjacent edges. Winding order is detected via signed
/// area so normals always point outward regardless of CW / CCW orientation.
pub(super) fn offset_polygon(points: &[(f32, f32)], distance: f32) -> Vec<(f32, f32)> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    // Signed area (shoelace): positive = CCW in math space (+y up);
    // negative = CW in math space = CW in screen space (+y down) which is what
    // Contour's rect / to_path() produces.
    let signed_area: f32 = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            points[i].0 * points[j].1 - points[j].0 * points[i].1
        })
        .sum::<f32>()
        * 0.5;
    // In screen space (+y down), a CW-wound polygon has positive signed area.
    // The outward normal is the right-side normal (ey, −ex) for CW and the
    // left-side (−ey, ex) for CCW. We pick `sign` so that `sign * (−ey, ex)`
    // always points outward: −1 for CW (positive area), +1 for CCW.
    let sign = if signed_area > 0.0 { -1.0_f32 } else { 1.0_f32 };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = points[(i + n - 1) % n];
        let cur  = points[i];
        let next = points[(i + 1) % n];
        // Edge vectors (cur − prev and next − cur).
        let (ex0, ey0) = (cur.0 - prev.0, cur.1 - prev.1);
        let (ex1, ey1) = (next.0 - cur.0, next.1 - cur.1);
        // Left-side normals (−ey, ex); multiplied by sign → outward.
        let len0 = (ex0 * ex0 + ey0 * ey0).sqrt().max(1e-9);
        let len1 = (ex1 * ex1 + ey1 * ey1).sqrt().max(1e-9);
        let n0 = (sign * -ey0 / len0, sign * ex0 / len0);
        let n1 = (sign * -ey1 / len1, sign * ex1 / len1);
        // Average outward normal, renormalized.
        let nx = (n0.0 + n1.0) * 0.5;
        let ny = (n0.1 + n1.1) * 0.5;
        let nlen = (nx * nx + ny * ny).sqrt().max(1e-9);
        out.push((cur.0 + nx / nlen * distance, cur.1 + ny / nlen * distance));
    }
    out
}

/// Interpolate a position along a polyline given arc-length distances at each
/// vertex. Returns the linearly interpolated point at arc-length `dist`.
pub(super) fn sample_polyline(path: &[[f32; 2]], arc: &[f32], dist: f32) -> (f32, f32) {
    let n = path.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    if n == 1 {
        return (path[0][0], path[0][1]);
    }
    // Binary search for the segment containing `dist`.
    let dist = dist.clamp(0.0, arc[n - 1]);
    let seg = arc
        .windows(2)
        .position(|w| w[0] <= dist && dist <= w[1])
        .unwrap_or(n - 2);
    let seg_len = arc[seg + 1] - arc[seg];
    if seg_len < 1e-9 {
        return (path[seg][0], path[seg][1]);
    }
    let t = (dist - arc[seg]) / seg_len;
    let (ax, ay) = (path[seg][0], path[seg][1]);
    let (bx, by) = (path[seg + 1][0], path[seg + 1][1]);
    (ax + t * (bx - ax), ay + t * (by - ay))
}

/// Warp a `Shape` (already reduced via [`Shape::to_path`]) through the perspective
/// homography defined by `corners` relative to `bbox`. Pushes every anchor and
/// bezier out-handle of each contour through the projective map (preserving curves),
/// returning the distorted `Path` / `Compound`. `None` if the shape has no warpable
/// geometry. Used by `Action::ApplyPerspectiveDistort`.
pub(super) fn warp_shape_perspective(
    path: &Shape,
    bbox: [f32; 4],
    corners: &[[f32; 2]; 4],
) -> Option<Shape> {
    use crate::document::{Shape as S, SubPath};
    use crate::perspective::{warp_handles, warp_points};
    match path {
        S::Path {
            points,
            handles,
            closed,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            if points.len() < 2 {
                return None;
            }
            let wpts = warp_points(points, bbox, corners);
            let whandles = warp_handles(points, handles, &wpts, bbox, corners);
            let mut shape =
                Shape::path(wpts, whandles, *closed, *fill, *stroke, *stroke_w);
            if let S::Path { stroke_style: ss, .. } = &mut shape {
                *ss = stroke_style.clone();
            }
            Some(shape)
        }
        S::Compound {
            subpaths,
            fill_rule,
            fill,
            stroke,
            stroke_w,
            stroke_style,
            ..
        } => {
            let warped: Vec<SubPath> = subpaths
                .iter()
                .map(|sp| {
                    let wp = warp_points(&sp.points, bbox, corners);
                    let wh = warp_handles(&sp.points, &sp.handles, &wp, bbox, corners);
                    SubPath {
                        points: wp,
                        handles: wh,
                        closed: sp.closed,
                    }
                })
                .collect();
            Some(Shape::Compound {
                subpaths: warped,
                fill_rule: *fill_rule,
                fill: *fill,
                fill_gradient: None,
                stroke: *stroke,
                stroke_w: *stroke_w,
                stroke_style: stroke_style.clone(),
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
            })
        }
        _ => None,
    }
}

/// Whether a shape's axis-aligned bounds intersect the marquee rectangle (used
/// by `MarqueeSelect`). A shape with no finite bounds (an empty path) never hits.
pub(super) fn bounds_intersect(shape: &Shape, marquee: &CoreRect) -> bool {
    let Some(b) = shape.bounds() else { return false };
    b.x < marquee.x + marquee.w
        && b.x + b.w > marquee.x
        && b.y < marquee.y + marquee.h
        && b.y + b.h > marquee.y
}

/// A small starter document (a few overlapping shapes on the default artboard) so
/// the GPUI preview and the Layers panel have real content to show. The egui app
/// opens with an empty document; this host seeds one purely so the migration
/// skeleton is visible end-to-end.
pub(super) fn sample_document() -> Document {
    let mut doc = Document::new();
    // Colors are straight sRGB RGBA in 0..1 (Contour's document convention).
    doc.shapes.push(Shape::rect(
        [120.0, 120.0, 420.0, 300.0],
        [0.20, 0.55, 0.90, 1.0], // blue fill
        [0.10, 0.20, 0.35, 1.0],
        6.0,
    ));
    doc.shapes.push(Shape::ellipse(
        [360.0, 240.0, 380.0, 320.0],
        [0.95, 0.45, 0.25, 0.85], // orange, semi-transparent
        [0.40, 0.15, 0.05, 1.0],
        4.0,
    ));
    doc.shapes.push(Shape::rect(
        [220.0, 340.0, 260.0, 200.0],
        [0.30, 0.80, 0.45, 0.90], // green
        [0.10, 0.30, 0.18, 1.0],
        3.0,
    ));
    doc
}

