//! Text layer domain: rich text layers and SVG import jobs.

use super::{App, Action};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Clone, Debug)]
pub struct DriftTextLayer {
    pub id: usize,
    pub layer_id: usize,
    pub content: String,
    pub font_family: String,
    pub font_size: f32,
    pub color: u32,
    pub align: TextAlign,
    pub style: TextStyle,
    pub line_height: f32,
    pub letter_spacing: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SvgImportStatus {
    Pending,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct SvgImportJob {
    pub id: usize,
    pub path: String,
    pub layer_id: usize,
    pub status: SvgImportStatus,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_text_layer(&mut self, action: Action) {
        match action {
            Action::AddTextLayer { layer_id, content, font_family, font_size } => {
                let id = self.next_text_layer_id;
                self.next_text_layer_id += 1;
                self.text_layers.push(DriftTextLayer {
                    id,
                    layer_id,
                    content,
                    font_family,
                    font_size,
                    color: 0xffffff,
                    align: TextAlign::Left,
                    style: TextStyle { bold: false, italic: false, underline: false },
                    line_height: 1.2,
                    letter_spacing: 0.0,
                });
            }
            Action::RemoveTextLayer { text_id } => {
                self.text_layers.retain(|t| t.id != text_id);
            }
            Action::SetTextContent { text_id, content } => {
                if let Some(t) = self.text_layers.iter_mut().find(|t| t.id == text_id) {
                    t.content = content;
                }
            }
            Action::SetTextFont { text_id, family, size } => {
                if let Some(t) = self.text_layers.iter_mut().find(|t| t.id == text_id) {
                    t.font_family = family;
                    t.font_size = size;
                }
            }
            Action::SetTextColor { text_id, color } => {
                if let Some(t) = self.text_layers.iter_mut().find(|t| t.id == text_id) {
                    t.color = color;
                }
            }
            Action::SetTextAlign { text_id, align } => {
                if let Some(t) = self.text_layers.iter_mut().find(|t| t.id == text_id) {
                    t.align = align;
                }
            }
            Action::SetTextStyle { text_id, style } => {
                if let Some(t) = self.text_layers.iter_mut().find(|t| t.id == text_id) {
                    t.style = style;
                }
            }
            Action::SetTextSpacing { text_id, line_height, letter_spacing } => {
                if let Some(t) = self.text_layers.iter_mut().find(|t| t.id == text_id) {
                    t.line_height = line_height;
                    t.letter_spacing = letter_spacing;
                }
            }
            Action::QueueSvgImport { path, layer_id } => {
                let id = self.next_svg_job_id;
                self.next_svg_job_id += 1;
                self.svg_import_jobs.push(SvgImportJob {
                    id,
                    path,
                    layer_id,
                    status: SvgImportStatus::Pending,
                });
            }
            Action::CompleteSvgImport { job_id } => {
                if let Some(j) = self.svg_import_jobs.iter_mut().find(|j| j.id == job_id) {
                    j.status = SvgImportStatus::Done;
                }
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{TextAlign, TextStyle, SvgImportStatus};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_text_layer_defaults() {
        let mut a = app();
        a.apply(Action::AddTextLayer {
            layer_id: 1,
            content: "Hello".to_string(),
            font_family: "Inter".to_string(),
            font_size: 24.0,
        });
        assert_eq!(a.text_layers.len(), 1);
        let t = &a.text_layers[0];
        assert_eq!(t.content, "Hello");
        assert_eq!(t.font_family, "Inter");
        assert_eq!(t.font_size, 24.0);
        assert_eq!(t.color, 0xffffff);
        assert_eq!(t.align, TextAlign::Left);
        assert_eq!(t.line_height, 1.2);
        assert_eq!(t.letter_spacing, 0.0);
    }

    #[test]
    fn test_add_text_layer_increments_id() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "A".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        a.apply(Action::AddTextLayer { layer_id: 2, content: "B".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        assert_eq!(a.text_layers[0].id, 1);
        assert_eq!(a.text_layers[1].id, 2);
    }

    #[test]
    fn test_remove_text_layer() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "X".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::RemoveTextLayer { text_id: id });
        assert!(a.text_layers.is_empty());
    }

    #[test]
    fn test_set_text_content() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "Old".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::SetTextContent { text_id: id, content: "New".to_string() });
        assert_eq!(a.text_layers[0].content, "New");
    }

    #[test]
    fn test_set_text_font() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "T".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::SetTextFont { text_id: id, family: "Roboto".to_string(), size: 36.0 });
        assert_eq!(a.text_layers[0].font_family, "Roboto");
        assert_eq!(a.text_layers[0].font_size, 36.0);
    }

    #[test]
    fn test_set_text_color() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "T".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::SetTextColor { text_id: id, color: 0xff0000 });
        assert_eq!(a.text_layers[0].color, 0xff0000);
    }

    #[test]
    fn test_set_text_align() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "T".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::SetTextAlign { text_id: id, align: TextAlign::Center });
        assert_eq!(a.text_layers[0].align, TextAlign::Center);
    }

    #[test]
    fn test_set_text_style() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "T".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::SetTextStyle { text_id: id, style: TextStyle { bold: true, italic: true, underline: false } });
        assert!(a.text_layers[0].style.bold);
        assert!(a.text_layers[0].style.italic);
    }

    #[test]
    fn test_set_text_spacing() {
        let mut a = app();
        a.apply(Action::AddTextLayer { layer_id: 1, content: "T".to_string(), font_family: "Arial".to_string(), font_size: 12.0 });
        let id = a.text_layers[0].id;
        a.apply(Action::SetTextSpacing { text_id: id, line_height: 1.5, letter_spacing: 2.0 });
        assert_eq!(a.text_layers[0].line_height, 1.5);
        assert_eq!(a.text_layers[0].letter_spacing, 2.0);
    }

    #[test]
    fn test_queue_svg_import() {
        let mut a = app();
        a.apply(Action::QueueSvgImport { path: "/tmp/icon.svg".to_string(), layer_id: 5 });
        assert_eq!(a.svg_import_jobs.len(), 1);
        assert_eq!(a.svg_import_jobs[0].status, SvgImportStatus::Pending);
        assert_eq!(a.svg_import_jobs[0].path, "/tmp/icon.svg");
    }

    #[test]
    fn test_complete_svg_import() {
        let mut a = app();
        a.apply(Action::QueueSvgImport { path: "/tmp/icon.svg".to_string(), layer_id: 5 });
        let id = a.svg_import_jobs[0].id;
        a.apply(Action::CompleteSvgImport { job_id: id });
        assert_eq!(a.svg_import_jobs[0].status, SvgImportStatus::Done);
    }

    #[test]
    fn test_queue_multiple_svg_imports_increment_ids() {
        let mut a = app();
        a.apply(Action::QueueSvgImport { path: "a.svg".to_string(), layer_id: 1 });
        a.apply(Action::QueueSvgImport { path: "b.svg".to_string(), layer_id: 2 });
        assert_eq!(a.svg_import_jobs[0].id, 1);
        assert_eq!(a.svg_import_jobs[1].id, 2);
    }
}
