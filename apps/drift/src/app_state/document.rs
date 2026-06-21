use super::{App, Action};

/// Top-level document metadata.
#[derive(Clone, Debug)]
pub struct DriftDocument {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_frames: usize,
    pub background_color: String,
    pub name: String,
}

impl DriftDocument {
    pub fn new() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 24.0,
            duration_frames: 240,
            background_color: "#000000".to_string(),
            name: "Untitled".to_string(),
        }
    }
}

impl App {
    pub fn apply_document(&mut self, action: Action) {
        match action {
            Action::SetDocumentWidth(w) => {
                self.document.width = w;
            }
            Action::SetDocumentHeight(h) => {
                self.document.height = h;
            }
            Action::SetDocumentFps(fps) => {
                self.document.fps = fps.clamp(1.0, 120.0);
            }
            Action::SetDocumentDuration(frames) => {
                self.document.duration_frames = frames;
            }
            Action::SetDocumentBg(color) => {
                self.document.background_color = color;
            }
            Action::SetDocumentName(name) => {
                self.document.name = name;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_new_app_defaults() {
        let a = app();
        assert_eq!(a.document.width, 1920);
        assert_eq!(a.document.height, 1080);
        assert_eq!(a.document.fps, 24.0);
        assert_eq!(a.document.duration_frames, 240);
        assert_eq!(a.document.name, "Untitled");
        assert!(a.layers.is_empty());
        assert!(a.keyframes.is_empty());
        assert!(!a.playing);
    }

    #[test]
    fn test_set_document_width() {
        let mut a = app();
        a.apply(Action::SetDocumentWidth(3840));
        assert_eq!(a.document.width, 3840);
    }

    #[test]
    fn test_set_document_height() {
        let mut a = app();
        a.apply(Action::SetDocumentHeight(2160));
        assert_eq!(a.document.height, 2160);
    }

    #[test]
    fn test_set_document_fps_clamp_high() {
        let mut a = app();
        a.apply(Action::SetDocumentFps(999.0));
        assert_eq!(a.document.fps, 120.0);
    }

    #[test]
    fn test_set_document_fps_clamp_low() {
        let mut a = app();
        a.apply(Action::SetDocumentFps(0.0));
        assert_eq!(a.document.fps, 1.0);
    }

    #[test]
    fn test_set_document_fps_normal() {
        let mut a = app();
        a.apply(Action::SetDocumentFps(30.0));
        assert_eq!(a.document.fps, 30.0);
    }

    #[test]
    fn test_set_document_duration() {
        let mut a = app();
        a.apply(Action::SetDocumentDuration(480));
        assert_eq!(a.document.duration_frames, 480);
    }

    #[test]
    fn test_set_document_bg() {
        let mut a = app();
        a.apply(Action::SetDocumentBg("#ffffff".to_string()));
        assert_eq!(a.document.background_color, "#ffffff");
    }

    #[test]
    fn test_set_document_name() {
        let mut a = app();
        a.apply(Action::SetDocumentName("My Animation".to_string()));
        assert_eq!(a.document.name, "My Animation");
    }
}
