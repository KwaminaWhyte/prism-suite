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

