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

/// Grid overlay configuration.
#[derive(Clone, Debug)]
pub struct GridConfig {
    pub enabled: bool,
    pub snap: bool,
    /// Grid cell size in pixels.
    pub size: f32,
    /// Hex color string, e.g. "#333333".
    pub color: String,
    /// Subdivisions per cell.
    pub subdivisions: u32,
}

impl GridConfig {
    pub fn new() -> Self {
        Self {
            enabled: false,
            snap: false,
            size: 10.0,
            color: "#333333".to_string(),
            subdivisions: 4,
        }
    }
}

/// Unit used for ruler display.
#[derive(Clone, Debug, PartialEq)]
pub enum RulerUnit {
    Pixels,
    Inches,
    Centimeters,
    Points,
}

/// Ruler overlay configuration.
#[derive(Clone, Debug)]
pub struct RulerConfig {
    pub enabled: bool,
    pub unit: RulerUnit,
    pub origin_x: f32,
    pub origin_y: f32,
}

impl RulerConfig {
    pub fn new() -> Self {
        Self {
            enabled: false,
            unit: RulerUnit::Pixels,
            origin_x: 0.0,
            origin_y: 0.0,
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
            // Grid
            Action::SetGridEnabled(v) => {
                self.grid.enabled = v;
            }
            Action::SetGridSnap(v) => {
                self.grid.snap = v;
            }
            Action::SetGridSize(size) => {
                self.grid.size = size.max(1.0);
            }
            Action::SetGridColor(color) => {
                self.grid.color = color;
            }
            Action::SetGridSubdivisions(n) => {
                self.grid.subdivisions = n.max(1);
            }
            // Rulers
            Action::ToggleRulers => {
                self.rulers.enabled = !self.rulers.enabled;
            }
            Action::SetRulerOrigin { x, y } => {
                self.rulers.origin_x = x;
                self.rulers.origin_y = y;
            }
            Action::SetRulerUnit(unit) => {
                self.rulers.unit = unit;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::RulerUnit;

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

    // Grid tests
    #[test]
    fn test_grid_defaults() {
        let a = app();
        assert!(!a.grid.enabled);
        assert!(!a.grid.snap);
        assert_eq!(a.grid.size, 10.0);
        assert_eq!(a.grid.color, "#333333");
        assert_eq!(a.grid.subdivisions, 4);
    }

    #[test]
    fn test_set_grid_enabled() {
        let mut a = app();
        a.apply(Action::SetGridEnabled(true));
        assert!(a.grid.enabled);
        a.apply(Action::SetGridEnabled(false));
        assert!(!a.grid.enabled);
    }

    #[test]
    fn test_set_grid_snap() {
        let mut a = app();
        a.apply(Action::SetGridSnap(true));
        assert!(a.grid.snap);
    }

    #[test]
    fn test_set_grid_size() {
        let mut a = app();
        a.apply(Action::SetGridSize(25.0));
        assert_eq!(a.grid.size, 25.0);
    }

    #[test]
    fn test_set_grid_size_min_one() {
        let mut a = app();
        a.apply(Action::SetGridSize(0.0));
        assert_eq!(a.grid.size, 1.0);
    }

    #[test]
    fn test_set_grid_color() {
        let mut a = app();
        a.apply(Action::SetGridColor("#aabbcc".to_string()));
        assert_eq!(a.grid.color, "#aabbcc");
    }

    #[test]
    fn test_set_grid_subdivisions() {
        let mut a = app();
        a.apply(Action::SetGridSubdivisions(8));
        assert_eq!(a.grid.subdivisions, 8);
    }

    #[test]
    fn test_set_grid_subdivisions_min_one() {
        let mut a = app();
        a.apply(Action::SetGridSubdivisions(0));
        assert_eq!(a.grid.subdivisions, 1);
    }

    // Ruler tests
    #[test]
    fn test_ruler_defaults() {
        let a = app();
        assert!(!a.rulers.enabled);
        assert_eq!(a.rulers.unit, RulerUnit::Pixels);
        assert_eq!(a.rulers.origin_x, 0.0);
        assert_eq!(a.rulers.origin_y, 0.0);
    }

    #[test]
    fn test_toggle_rulers() {
        let mut a = app();
        a.apply(Action::ToggleRulers);
        assert!(a.rulers.enabled);
        a.apply(Action::ToggleRulers);
        assert!(!a.rulers.enabled);
    }

    #[test]
    fn test_set_ruler_origin() {
        let mut a = app();
        a.apply(Action::SetRulerOrigin { x: 100.0, y: 50.0 });
        assert_eq!(a.rulers.origin_x, 100.0);
        assert_eq!(a.rulers.origin_y, 50.0);
    }

    #[test]
    fn test_set_ruler_unit_inches() {
        let mut a = app();
        a.apply(Action::SetRulerUnit(RulerUnit::Inches));
        assert_eq!(a.rulers.unit, RulerUnit::Inches);
    }

    #[test]
    fn test_set_ruler_unit_centimeters() {
        let mut a = app();
        a.apply(Action::SetRulerUnit(RulerUnit::Centimeters));
        assert_eq!(a.rulers.unit, RulerUnit::Centimeters);
    }

    #[test]
    fn test_set_ruler_unit_points() {
        let mut a = app();
        a.apply(Action::SetRulerUnit(RulerUnit::Points));
        assert_eq!(a.rulers.unit, RulerUnit::Points);
    }
}
