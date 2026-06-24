/// Measurement unit for the Document Setup / rulers. The ruler scale is the
/// number of document points (1/72") one unit represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum DocUnit {
    /// 1 pt = 1 document point (Contour's native unit).
    #[default]
    Points,
    /// 1 px = 1 pt (web convention).
    Pixels,
    /// 1 pica = 12 pt.
    Picas,
    /// 1 in = 72 pt.
    Inches,
    /// 1 mm = 72/25.4 pt.
    Millimeters,
    /// 1 cm = 720/25.4 pt.
    Centimeters,
}

#[allow(dead_code)] // panel-facing unit conversion / labels
impl DocUnit {
    /// How many document points one of this unit equals.
    pub fn points_per_unit(self) -> f32 {
        match self {
            DocUnit::Points | DocUnit::Pixels => 1.0,
            DocUnit::Picas => 12.0,
            DocUnit::Inches => 72.0,
            DocUnit::Millimeters => 72.0 / 25.4,
            DocUnit::Centimeters => 720.0 / 25.4,
        }
    }

    /// Short suffix for the panel / status bar.
    pub fn label(self) -> &'static str {
        match self {
            DocUnit::Points => "pt",
            DocUnit::Pixels => "px",
            DocUnit::Picas => "pc",
            DocUnit::Inches => "in",
            DocUnit::Millimeters => "mm",
            DocUnit::Centimeters => "cm",
        }
    }
}

/// The document's colour working space (informational + export hint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum DocColorMode {
    /// Screen / web (additive).
    #[default]
    Rgb,
    /// Print (subtractive).
    Cmyk,
}

#[allow(dead_code)] // panel-facing label
impl DocColorMode {
    pub fn label(self) -> &'static str {
        match self {
            DocColorMode::Rgb => "RGB",
            DocColorMode::Cmyk => "CMYK",
        }
    }
}

/// **Document Setup** model: the default artboard dimensions, ruler unit, colour
/// mode, and per-side bleed. Dimensions and bleed are stored in **document
/// points** (the native unit) regardless of the display unit, so geometry is
/// always unambiguous; the unit only affects how the panel formats numbers.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DocumentSetup {
    /// Default artboard width in document points.
    pub width: f32,
    /// Default artboard height in document points.
    pub height: f32,
    /// Display / ruler unit.
    pub unit: DocUnit,
    /// Colour working space.
    pub color_mode: DocColorMode,
    /// Per-side bleed in document points: `[top, right, bottom, left]`.
    pub bleed: [f32; 4],
}

#[allow(dead_code)] // panel-facing helpers (bleed extents, unit conversion)
impl DocumentSetup {
    /// A US-Letter-ish default (Illustrator's default new doc is 612×792 pt).
    pub fn new() -> Self {
        Self {
            width: 612.0,
            height: 792.0,
            unit: DocUnit::Points,
            color_mode: DocColorMode::Rgb,
            bleed: [0.0; 4],
        }
    }

    /// The full bleed-extended size `(w, h)` in points (artboard + left+right /
    /// top+bottom bleed).
    pub fn bleed_size(&self) -> (f32, f32) {
        let [t, r, b, l] = self.bleed;
        (self.width + l + r, self.height + t + b)
    }

    /// Convert a value entered in the display `unit` to document points.
    pub fn to_points(&self, value: f32) -> f32 {
        value * self.unit.points_per_unit()
    }

    /// Convert a document-point value to the display `unit`.
    pub fn from_points(&self, points: f32) -> f32 {
        points / self.unit.points_per_unit()
    }
}

impl Default for DocumentSetup {
    fn default() -> Self {
        Self::new()
    }
}

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
