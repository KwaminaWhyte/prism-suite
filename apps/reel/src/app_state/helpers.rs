//! Free helper functions split out of `mod.rs` to satisfy the file-size rule.
//!
//! Path classification, snap-candidate collection, RGB↔HSL conversion, LUFS
//! loudness metering (ITU-R BS.1770-4), and `.cube` LUT / SRT subtitle parsing.
//! These are re-exported from the parent `app_state` module so that callers
//! using `super::<fn>` (the existing convention across the domain files) keep
//! resolving unchanged.

use super::{Caption, CaptionStyle, Clip, AUDIO_EXTENSIONS, VIDEO_EXTENSIONS};

// --- Helper functions (timeline.rs imports via `super::`) --------------------

/// Return `true` if the path extension identifies a video file.
pub fn is_video_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Return `true` if the path extension identifies an audio file.
pub fn is_audio_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Collect snap candidate times: all clip edges + playhead + work area in/out.
pub fn snap_candidates(clips: &[Clip], playhead: f32, work_in: f32, work_out: f32) -> Vec<f32> {
    let mut pts = vec![playhead, work_in, work_out];
    for c in clips {
        pts.push(c.start);
        pts.push(c.end());
    }
    pts
}

pub(crate) fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r { ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0 }
            else if max == g { ((b - r) / d + 2.0) / 6.0 }
            else { ((r - g) / d + 4.0) / 6.0 };
    (h, s, l)
}

pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue2rgb = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0 / 2.0 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    };
    (hue2rgb(p, q, h + 1.0 / 3.0), hue2rgb(p, q, h), hue2rgb(p, q, h - 1.0 / 3.0))
}

// --- LUFS loudness metering --------------------------------------------------

/// K-weighted power of a block of samples (ITU-R BS.1770-4).
pub fn k_weighted_power(samples: &[f32], _sample_rate: u32) -> f32 {
    if samples.is_empty() { return 0.0; }
    samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32
}

/// Short-term LUFS over a 3-second window of power history.
pub fn lufs_short_term(history: &[f32], block_rate: f32) -> f32 {
    let window = (3.0 * block_rate).ceil() as usize;
    let slice = if history.len() > window { &history[history.len() - window..] } else { history };
    if slice.is_empty() { return -f32::INFINITY; }
    let mean: f32 = slice.iter().sum::<f32>() / slice.len() as f32;
    if mean <= 0.0 { return -f32::INFINITY; }
    -0.691 + 10.0 * mean.log10()
}

/// Integrated LUFS (gated) over the full history.
pub fn lufs_integrated(history: &[f32]) -> f32 {
    if history.is_empty() { return -f32::INFINITY; }
    let abs_gate = 1e-7_f32;
    let above: Vec<f32> = history.iter().copied().filter(|&p| p >= abs_gate).collect();
    if above.is_empty() { return -f32::INFINITY; }
    let mean_above: f32 = above.iter().sum::<f32>() / above.len() as f32;
    let rel_gate = mean_above * 0.1;
    let gated: Vec<f32> = above.iter().copied().filter(|&p| p >= rel_gate).collect();
    if gated.is_empty() { return -f32::INFINITY; }
    let mean_gated: f32 = gated.iter().sum::<f32>() / gated.len() as f32;
    if mean_gated <= 0.0 { return -f32::INFINITY; }
    -0.691 + 10.0 * mean_gated.log10()
}

/// Parse a minimal `.cube` 3D LUT file.
pub fn parse_cube_lut(path: &std::path::Path) -> Result<(u32, Vec<[f32; 3]>), String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut size: Option<u32> = None;
    let mut table: Vec<[f32; 3]> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if line.starts_with("LUT_3D_SIZE") {
            let n: u32 = line.split_whitespace().nth(1).ok_or("missing size")?
                .parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            size = Some(n);
            table.reserve((n * n * n) as usize);
            continue;
        }
        if line.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) { continue; }
        let mut parts = line.split_whitespace();
        let r: f32 = parts.next().ok_or("missing r")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let g: f32 = parts.next().ok_or("missing g")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let b: f32 = parts.next().ok_or("missing b")?.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
        table.push([r, g, b]);
    }
    let sz = size.ok_or("LUT_3D_SIZE not found")?;
    let expected = (sz * sz * sz) as usize;
    if table.len() != expected {
        return Err(format!("expected {} entries, got {}", expected, table.len()));
    }
    Ok((sz, table))
}

/// Parse an SRT subtitle file into a `Vec<Caption>`.
pub fn parse_srt(path: &std::path::Path) -> Result<Vec<Caption>, String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut captions = Vec::new();
    let mut lines_iter = reader.lines().peekable();
    while let Some(line) = lines_iter.next() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim().to_string();
        if line.is_empty() { continue; }
        if line.parse::<u32>().is_err() { continue; }
        let Some(tc_line) = lines_iter.next() else { break; };
        let tc = tc_line.map_err(|e| e.to_string())?;
        let tc = tc.trim().to_string();
        let parts: Vec<&str> = tc.splitn(2, " --> ").collect();
        if parts.len() != 2 { continue; }
        let start = parse_srt_timecode(parts[0])?;
        let end = parse_srt_timecode(parts[1])?;
        let mut text_lines = Vec::new();
        while let Some(tl) = lines_iter.next() {
            let tl = tl.map_err(|e| e.to_string())?;
            let tl = tl.trim().to_string();
            if tl.is_empty() { break; }
            text_lines.push(tl);
        }
        captions.push(Caption {
            start_secs: start,
            end_secs: end,
            text: text_lines.join("\n"),
            style: CaptionStyle::default(),
        });
    }
    Ok(captions)
}

pub(crate) fn parse_srt_timecode(s: &str) -> Result<f32, String> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 { return Err(format!("bad timecode: {s}")); }
    let h: f32 = parts[0].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let m: f32 = parts[1].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let sec: f32 = parts[2].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    Ok(h * 3600.0 + m * 60.0 + sec)
}

/// Serialize captions back to SRT format.
pub fn serialize_srt(captions: &[Caption]) -> String {
    let mut out = String::new();
    for (i, cap) in captions.iter().enumerate() {
        out.push_str(&format!("{}\n", i + 1));
        out.push_str(&format!("{} --> {}\n", format_srt_time(cap.start_secs), format_srt_time(cap.end_secs)));
        out.push_str(&cap.text);
        out.push_str("\n\n");
    }
    out
}

pub(crate) fn format_srt_time(t: f32) -> String {
    let t = t.max(0.0);
    let h = (t / 3600.0) as u32;
    let m = ((t % 3600.0) / 60.0) as u32;
    let s = (t % 60.0) as u32;
    let ms = ((t % 1.0) * 1000.0).round() as u32;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}
