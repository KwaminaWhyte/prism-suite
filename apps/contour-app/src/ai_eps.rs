//! Basic AI / EPS importer: parses PostScript path operators and colour
//! commands from text-format AI (CS4 and earlier) and EPS files, yielding
//! a flat `Vec<Shape>` the caller appends to the document.
//!
//! Supports: moveto (`m`/`M`), lineto (`l`/`L`), curveto (`c`/`C`),
//! closepath (`h`/`H` or `closepath`), setrgbcolor (`g`/`G`/`rg`/`RG`),
//! fill (`f`/`F`), stroke (`S`/`s`).  Binary AI (.ai from CS5+) returns an
//! error pointing users to re-export as SVG.

use crate::document::Shape;

/// Import shapes from an AI or EPS text file.  Returns `Err` on binary AI or
/// unreadable files.
pub fn import(path: &std::path::Path) -> Result<Vec<Shape>, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    // Binary AI starts with `%PDF` or has NUL bytes in the first 512 bytes.
    let header = &bytes[..bytes.len().min(512)];
    if header.contains(&0u8) || header.starts_with(b"%PDF") {
        return Err(
            "Binary AI (CS5+) not supported — re-export as SVG or use an older AI/EPS version."
                .into(),
        );
    }

    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "File is not valid UTF-8 — may be a binary format.".to_string())?;

    parse_ps(text)
}

fn parse_ps(src: &str) -> Result<Vec<Shape>, String> {
    let mut shapes: Vec<Shape> = Vec::new();

    // Current path state.
    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut handles: Vec<(f32, f32)> = Vec::new();
    let mut current: (f32, f32) = (0.0, 0.0);

    // Current colour state (straight sRGB RGBA).
    let mut fill_color: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    let mut stroke_color: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    let mut stroke_w: f32 = 1.0;

    // Tokenise: split on whitespace, skip PostScript comments.
    let tokens: Vec<&str> = src
        .lines()
        .filter(|l| !l.trim_start().starts_with('%'))
        .flat_map(|l| l.split_whitespace())
        .collect();

    let mut i = 0usize;
    while i < tokens.len() {
        let tok = tokens[i];
        match tok {
            // moveto
            "m" | "M" if i >= 2 => {
                let y = parse_f32(tokens[i - 1]);
                let x = parse_f32(tokens[i - 2]);
                current = (x, y);
                points.push(current);
                handles.push((0.0, 0.0));
            }
            // lineto
            "l" | "L" if i >= 2 => {
                let y = parse_f32(tokens[i - 1]);
                let x = parse_f32(tokens[i - 2]);
                current = (x, y);
                points.push(current);
                handles.push((0.0, 0.0));
            }
            // curveto: 6 args before token
            "c" | "C" if i >= 6 => {
                let y3 = parse_f32(tokens[i - 1]);
                let x3 = parse_f32(tokens[i - 2]);
                let y2 = parse_f32(tokens[i - 3]);
                let x2 = parse_f32(tokens[i - 4]);
                let y1 = parse_f32(tokens[i - 5]);
                let x1 = parse_f32(tokens[i - 6]);
                // Store out-tangent of previous anchor as offset.
                if let Some(last_h) = handles.last_mut() {
                    *last_h = (x1 - current.0, y1 - current.1);
                }
                // New anchor at (x3,y3); in-tangent = -(x2-x3, y2-y3).
                current = (x3, y3);
                points.push(current);
                handles.push((x3 - x2, y3 - y2));
            }
            // closepath
            "h" | "H" | "closepath" => {
                commit_path(&mut shapes, &mut points, &mut handles, true, fill_color, stroke_color, stroke_w);
            }
            // fill stroke combined
            "b" | "B" => {
                commit_path(&mut shapes, &mut points, &mut handles, true, fill_color, stroke_color, stroke_w);
            }
            // fill only
            "f" | "F" => {
                commit_path(&mut shapes, &mut points, &mut handles, false, fill_color, [0.0,0.0,0.0,0.0], 0.0);
            }
            // stroke only
            "S" | "s" => {
                commit_path(&mut shapes, &mut points, &mut handles, false, [0.0,0.0,0.0,0.0], stroke_color, stroke_w);
            }
            // setrgbcolor for fill: `r g b rg`
            "rg" if i >= 3 => {
                let b = parse_f32(tokens[i - 1]);
                let g = parse_f32(tokens[i - 2]);
                let r = parse_f32(tokens[i - 3]);
                fill_color = [r, g, b, 1.0];
            }
            // setrgbcolor for stroke: `r g b RG`
            "RG" if i >= 3 => {
                let b = parse_f32(tokens[i - 1]);
                let g = parse_f32(tokens[i - 2]);
                let r = parse_f32(tokens[i - 3]);
                stroke_color = [r, g, b, 1.0];
            }
            // setgray fill: `g g`
            "g" if i >= 1 => {
                if let Ok(v) = tokens[i - 1].parse::<f32>() {
                    fill_color = [v, v, v, 1.0];
                }
            }
            // setgray stroke: `g G`
            "G" if i >= 1 => {
                if let Ok(v) = tokens[i - 1].parse::<f32>() {
                    stroke_color = [v, v, v, 1.0];
                }
            }
            // setlinewidth: `w w`
            "w" if i >= 1 => {
                if let Ok(v) = tokens[i - 1].parse::<f32>() {
                    stroke_w = v;
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Commit any open path.
    if !points.is_empty() {
        commit_path(&mut shapes, &mut points, &mut handles, false, fill_color, stroke_color, stroke_w);
    }

    Ok(shapes)
}

fn commit_path(
    shapes: &mut Vec<Shape>,
    points: &mut Vec<(f32, f32)>,
    handles: &mut Vec<(f32, f32)>,
    closed: bool,
    fill: [f32; 4],
    stroke: [f32; 4],
    stroke_w: f32,
) {
    if points.len() >= 2 {
        shapes.push(Shape::path(
            std::mem::take(points),
            std::mem::take(handles),
            closed,
            fill,
            stroke,
            stroke_w,
        ));
    } else {
        points.clear();
        handles.clear();
    }
}

fn parse_f32(s: &str) -> f32 {
    s.parse().unwrap_or(0.0)
}
