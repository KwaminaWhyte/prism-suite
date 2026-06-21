/// Stroke alignment relative to the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeAlignment {
    Center,
    Inside,
    Outside,
}

/// Arrowhead style for stroked paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowHead {
    None,
    Arrow,
    OpenArrow,
    Circle,
    Square,
}

/// Font weight for the character panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontWeight {
    #[default]
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontWeight {
    pub const ALL: [FontWeight; 4] = [
        FontWeight::Regular,
        FontWeight::Bold,
        FontWeight::Italic,
        FontWeight::BoldItalic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FontWeight::Regular => "Regular",
            FontWeight::Bold => "Bold",
            FontWeight::Italic => "Italic",
            FontWeight::BoldItalic => "Bold Italic",
        }
    }
}

/// Graph / chart kind for the graph tool stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphKind {
    Bar,
    Line,
    Pie,
    Scatter,
}

impl GraphKind {
    pub fn label(self) -> &'static str {
        match self {
            GraphKind::Bar => "Bar",
            GraphKind::Line => "Line",
            GraphKind::Pie => "Pie",
            GraphKind::Scatter => "Scatter",
        }
    }
}

/// Named stroke-width profile presets for the width tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthProfilePreset {
    Uniform,
    TaperIn,
    TaperOut,
    BulgeCenter,
    TaperBoth,
}

impl WidthProfilePreset {
    pub const ALL: [WidthProfilePreset; 5] = [
        WidthProfilePreset::Uniform,
        WidthProfilePreset::TaperIn,
        WidthProfilePreset::TaperOut,
        WidthProfilePreset::BulgeCenter,
        WidthProfilePreset::TaperBoth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WidthProfilePreset::Uniform => "Uniform",
            WidthProfilePreset::TaperIn => "Taper In",
            WidthProfilePreset::TaperOut => "Taper Out",
            WidthProfilePreset::BulgeCenter => "Bulge",
            WidthProfilePreset::TaperBoth => "Taper Both",
        }
    }

    /// Returns `(start_mul, end_mul)` for `StrokeStyle.width_profile`.
    pub fn profile(self) -> (f32, f32) {
        match self {
            WidthProfilePreset::Uniform => (1.0, 1.0),
            WidthProfilePreset::TaperIn => (0.1, 1.0),
            WidthProfilePreset::TaperOut => (1.0, 0.1),
            WidthProfilePreset::BulgeCenter => (0.5, 0.5),
            WidthProfilePreset::TaperBoth => (0.1, 0.1),
        }
    }
}
