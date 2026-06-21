/// One artboard entry with a stable id, name, and rect.
#[derive(Clone, Debug)]
pub struct ArtboardEntry {
    /// Stable id (never reused within a session).
    pub id: u64,
    /// Human-visible label.
    pub name: String,
    /// `[x, y, w, h]` in document space.
    pub rect: [f32; 4],
}

impl ArtboardEntry {
    pub fn new(id: u64, name: impl Into<String>, rect: [f32; 4]) -> Self {
        Self { id, name: name.into(), rect }
    }
}

/// Two-point perspective grid overlay drawn on the canvas.
pub struct PerspectiveGrid {
    pub vp1: (f32, f32),
    pub vp2: (f32, f32),
    pub horizon_y: f32,
    pub visible: bool,
}

impl PerspectiveGrid {
    pub fn default_for(canvas_w: f32, canvas_h: f32) -> Self {
        Self {
            vp1: (canvas_w * 0.15, canvas_h * 0.5),
            vp2: (canvas_w * 0.85, canvas_h * 0.5),
            horizon_y: canvas_h * 0.5,
            visible: true,
        }
    }
}
