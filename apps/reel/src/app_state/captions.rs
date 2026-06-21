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
