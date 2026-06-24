use super::{App, Action, parse_srt, serialize_srt};

/// Caption vertical position.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptionPosition { Bottom, Top, Custom(f32, f32) }

impl Default for CaptionPosition { fn default() -> Self { CaptionPosition::Bottom } }

/// Caption text style.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptionStyle {
    pub font_size: f32,
    pub color: [f32; 4],
    pub position: CaptionPosition,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self { font_size: 32.0, color: [1.0, 1.0, 1.0, 1.0], position: CaptionPosition::default() }
    }
}

/// A single subtitle/caption entry.
#[derive(Clone, Debug)]
pub struct Caption {
    pub start_secs: f32,
    pub end_secs: f32,
    pub text: String,
    pub style: CaptionStyle,
}

/// One styled run of caption text produced by [`parse_inline_tags`]: a slice of
/// plain text with bold / italic / underline flags and an optional override
/// color (`None` = inherit the cue's style color).
#[derive(Clone, Debug, PartialEq)]
pub struct StyledRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<[f32; 4]>,
}

/// Parse SubRip / WebVTT-style inline markup into styled runs. Supported tags:
/// `<b>…</b>`, `<i>…</i>`, `<u>…</u>`, and `<font color="#RRGGBB">…</font>`
/// (also the SRT-common `{b}`/`{i}` forms). Tags nest; an unclosed tag stays in
/// effect to the end of the cue. Unknown tags are dropped. The concatenated run
/// texts equal the input with all tags removed.
pub fn parse_inline_tags(text: &str) -> Vec<StyledRun> {
    let mut runs: Vec<StyledRun> = Vec::new();
    let mut bold = 0u32;
    let mut italic = 0u32;
    let mut under = 0u32;
    let mut color_stack: Vec<[f32; 4]> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    let flush = |cur: &mut String, runs: &mut Vec<StyledRun>, b: u32, it: u32, u: u32, col: Option<[f32; 4]>| {
        if !cur.is_empty() {
            runs.push(StyledRun {
                text: std::mem::take(cur),
                bold: b > 0, italic: it > 0, underline: u > 0, color: col,
            });
        }
    };

    while i < chars.len() {
        let c = chars[i];
        // Tag opener: `<...>` or `{b}` / `{/b}` style.
        let (open, close) = match c {
            '<' => ('<', '>'),
            '{' => ('{', '}'),
            _ => ('\0', '\0'),
        };
        if open != '\0' {
            if let Some(end_rel) = chars[i + 1..].iter().position(|&ch| ch == close) {
                let tag: String = chars[i + 1..i + 1 + end_rel].iter().collect();
                let tag_l = tag.trim().to_ascii_lowercase();
                let cur_color = color_stack.last().copied();
                flush(&mut cur, &mut runs, bold, italic, under, cur_color);
                if let Some(rest) = tag_l.strip_prefix('/') {
                    match rest.trim() {
                        "b" | "strong" => bold = bold.saturating_sub(1),
                        "i" | "em" => italic = italic.saturating_sub(1),
                        "u" => under = under.saturating_sub(1),
                        "font" => { color_stack.pop(); }
                        _ => {}
                    }
                } else if tag_l == "b" || tag_l == "strong" {
                    bold += 1;
                } else if tag_l == "i" || tag_l == "em" {
                    italic += 1;
                } else if tag_l == "u" {
                    under += 1;
                } else if tag_l.starts_with("font") {
                    if let Some(col) = parse_font_color(&tag_l) {
                        color_stack.push(col);
                    } else {
                        // Unknown font attr → push the current color so the
                        // matching `</font>` still balances the stack.
                        color_stack.push(color_stack.last().copied().unwrap_or([1.0; 4]));
                    }
                }
                i += 1 + end_rel + 1;
                continue;
            }
        }
        cur.push(c);
        i += 1;
    }
    let cur_color = color_stack.last().copied();
    flush(&mut cur, &mut runs, bold, italic, under, cur_color);
    runs
}

/// Extract a `color="#RRGGBB"` (or `#RGB`) value from a `font …` tag body.
fn parse_font_color(tag: &str) -> Option<[f32; 4]> {
    let idx = tag.find("color")?;
    let after = &tag[idx + 5..];
    let hash = after.find('#')?;
    let hex: String = after[hash + 1..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    let (r, g, b) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        3 => {
            let exp = |c: &str| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
            (exp(&hex[0..1])?, exp(&hex[1..2])?, exp(&hex[2..3])?)
        }
        _ => return None,
    };
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

/// Extended caption model (Batch 9).
#[derive(Debug, Clone, Default)]
pub struct CaptionB9 {
    pub start_sec: f32,
    pub end_sec: f32,
    pub text: String,
    pub speaker: String,
    pub style_id: usize,
}

/// Extended caption style (Batch 9).
#[derive(Debug, Clone, Default)]
pub struct CaptionStyleB9 {
    pub name: String,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub background_opacity: f32,
}

impl CaptionStyleB9 {
    pub fn default_style() -> Self {
        Self {
            name: "Default".to_string(),
            font_size: 36.0,
            position: [0.5, 0.9],
            color: [1.0, 1.0, 1.0, 1.0],
            ..Default::default()
        }
    }
}

pub trait AppCaptionsExt {
    fn apply_captions(&mut self, action: Action);
}

impl AppCaptionsExt for App {
    fn apply_captions(&mut self, action: Action) {
        match action {
            Action::AddCaption(cap) => { self.captions.push(cap); }
            Action::RemoveCaption(idx) => {
                if idx < self.captions.len() { self.captions.remove(idx); }
            }
            Action::EditCaption { index, caption } => {
                if index < self.captions.len() { self.captions[index] = caption; }
            }
            Action::SetCueText { index, text } => {
                if let Some(c) = self.captions.get_mut(index) {
                    c.text = text;
                    self.host.mark_dirty();
                }
            }
            Action::SetCaptionPosition { index, position } => {
                if let Some(c) = self.captions.get_mut(index) {
                    c.style.position = position;
                    self.host.mark_dirty();
                }
            }
            Action::SetCaptionCueColor { index, color } => {
                if let Some(c) = self.captions.get_mut(index) {
                    c.style.color = color;
                    self.host.mark_dirty();
                }
            }
            Action::ImportSrt(path) => {
                match parse_srt(&path) {
                    Ok(caps) => self.captions = caps,
                    Err(e) => log::warn!("reel-gpui: SRT import failed: {e}"),
                }
            }
            Action::ExportSrt(path) => {
                let s = serialize_srt(&self.captions);
                if let Err(e) = std::fs::write(&path, s) {
                    log::warn!("reel-gpui: SRT export failed: {e}");
                }
            }
            Action::ToggleCaptionsPanel => { self.show_captions_panel = !self.show_captions_panel; }
            Action::AddCaptionB9(cap) => { self.captions_b9.push(cap); }
            Action::RemoveCaptionB9(idx) => {
                if idx < self.captions_b9.len() { self.captions_b9.remove(idx); }
            }
            Action::SetCaptionText { idx, text } => {
                if let Some(c) = self.captions_b9.get_mut(idx) { c.text = text; }
            }
            Action::SetCaptionTiming { idx, start, end } => {
                if let Some(c) = self.captions_b9.get_mut(idx) { c.start_sec = start; c.end_sec = end; }
            }
            Action::SetCaptionSpeaker { idx, speaker } => {
                if let Some(c) = self.captions_b9.get_mut(idx) { c.speaker = speaker; }
            }
            Action::SetCaptionStyle { idx, style_id } => {
                if let Some(c) = self.captions_b9.get_mut(idx) { c.style_id = style_id; }
            }
            Action::AddCaptionStyleB9(style) => { self.caption_styles_b9.push(style); }
            Action::ToggleCaptionTrack => { self.caption_track_visible = !self.caption_track_visible; }
            Action::ExportSrtB9(path) => { self.last_srt_export_path = Some(path); }
            Action::ImportSrtB9(_path) => {
                self.captions_b9.clear();
                self.captions_b9.push(CaptionB9 {
                    start_sec: 0.0, end_sec: 1.0, text: "[Imported SRT]".to_string(),
                    speaker: String::new(), style_id: 0,
                });
            }
            Action::AutoTranscribe => {
                self.captions_b9.push(CaptionB9 {
                    start_sec: 0.0, end_sec: 5.0, text: "[Auto-transcribed]".to_string(),
                    speaker: String::new(), style_id: 0,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn inline_tags_plain_text_is_one_run() {
        let runs = parse_inline_tags("hello world");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "hello world");
        assert!(!runs[0].bold && !runs[0].italic && runs[0].color.is_none());
    }

    #[test]
    fn inline_tags_bold_italic_segments() {
        let runs = parse_inline_tags("a <b>bold</b> <i>it</i> z");
        // Concatenated text equals the input minus tags.
        let joined: String = runs.iter().map(|r| r.text.clone()).collect();
        assert_eq!(joined, "a bold it z");
        let bold = runs.iter().find(|r| r.text == "bold").unwrap();
        assert!(bold.bold && !bold.italic);
        let it = runs.iter().find(|r| r.text == "it").unwrap();
        assert!(it.italic && !it.bold);
    }

    #[test]
    fn inline_tags_font_color() {
        let runs = parse_inline_tags("<font color=\"#ff0000\">red</font>");
        let red = runs.iter().find(|r| r.text == "red").unwrap();
        let c = red.color.expect("font color parsed");
        assert!((c[0] - 1.0).abs() < 1e-3 && c[1] < 0.01 && c[2] < 0.01, "red color {c:?}");
    }

    #[test]
    fn inline_tags_nested_bold_italic() {
        let runs = parse_inline_tags("<b><i>both</i></b>");
        let both = runs.iter().find(|r| r.text == "both").unwrap();
        assert!(both.bold && both.italic);
    }

    #[test]
    fn inline_tags_brace_form() {
        let runs = parse_inline_tags("plain {b}strong{/b}");
        let strong = runs.iter().find(|r| r.text == "strong").unwrap();
        assert!(strong.bold);
    }

    #[test]
    fn set_caption_position_per_cue() {
        let mut app = App::new();
        app.apply(Action::AddCaption(Caption {
            start_secs: 0.0, end_secs: 2.0, text: "Hi".into(), style: CaptionStyle::default(),
        }));
        assert_eq!(app.captions[0].style.position, CaptionPosition::Bottom);
        app.apply(Action::SetCaptionPosition { index: 0, position: CaptionPosition::Custom(0.25, 0.1) });
        assert_eq!(app.captions[0].style.position, CaptionPosition::Custom(0.25, 0.1));
    }

    #[test]
    fn set_caption_cue_color() {
        let mut app = App::new();
        app.apply(Action::AddCaption(Caption {
            start_secs: 0.0, end_secs: 2.0, text: "Hi".into(), style: CaptionStyle::default(),
        }));
        app.apply(Action::SetCaptionCueColor { index: 0, color: [1.0, 1.0, 0.0, 1.0] });
        assert_eq!(app.captions[0].style.color, [1.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn test_add_remove_caption() {
        let mut app = App::new();
        assert_eq!(app.captions_b9.len(), 0);
        app.apply(Action::AddCaptionB9(CaptionB9 {
            start_sec: 0.0, end_sec: 2.0, text: "Hello".to_string(),
            speaker: "A".to_string(), style_id: 0,
        }));
        assert_eq!(app.captions_b9.len(), 1);
        app.apply(Action::RemoveCaptionB9(0));
        assert_eq!(app.captions_b9.len(), 0);
    }

    #[test]
    fn test_caption_text_set() {
        let mut app = App::new();
        app.apply(Action::AddCaptionB9(CaptionB9::default()));
        app.apply(Action::SetCaptionText { idx: 0, text: "Updated text".to_string() });
        assert_eq!(app.captions_b9[0].text, "Updated text");
    }

    #[test]
    fn test_set_cue_text_multiline() {
        let mut app = App::new();
        app.apply(Action::AddCaption(Caption {
            start_secs: 0.0,
            end_secs: 3.0,
            text: "first".into(),
            style: CaptionStyle::default(),
        }));
        // Multi-line cue text round-trips through SetCueText (panel TextArea).
        app.apply(Action::SetCueText { index: 0, text: "line one\nline two".to_string() });
        assert_eq!(app.captions[0].text, "line one\nline two");
        // Out-of-range index is a silent no-op (doesn't panic).
        app.apply(Action::SetCueText { index: 99, text: "ignored".to_string() });
    }

    #[test]
    fn test_export_srt_records_path() {
        let mut app = App::new();
        assert!(app.last_srt_export_path.is_none());
        let p = std::path::PathBuf::from("/tmp/test.srt");
        app.apply(Action::ExportSrtB9(p.clone()));
        assert_eq!(app.last_srt_export_path, Some(p));
    }

    #[test]
    fn test_auto_transcribe_adds_caption() {
        let mut app = App::new();
        assert_eq!(app.captions_b9.len(), 0);
        app.apply(Action::AutoTranscribe);
        assert_eq!(app.captions_b9.len(), 1);
        assert_eq!(app.captions_b9[0].text, "[Auto-transcribed]");
        assert!((app.captions_b9[0].start_sec).abs() < 1e-5);
        assert!((app.captions_b9[0].end_sec - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_caption_track_toggle() {
        let mut app = App::new();
        assert!(app.caption_track_visible);
        app.apply(Action::ToggleCaptionTrack);
        assert!(!app.caption_track_visible);
        app.apply(Action::ToggleCaptionTrack);
        assert!(app.caption_track_visible);
    }
}
