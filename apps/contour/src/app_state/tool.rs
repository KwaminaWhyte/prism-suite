//! The active editing tool enum + its display metadata.
//!
//! Split out of `mod.rs` to keep that file under the ~1000-line limit. `Tool` is
//! a self-contained UI enum (no `App` dependency); the tools strip reads
//! `Tool::ALL` / `label` / `glyph`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    DirectSelect,
    Rect,
    Ellipse,
    Line,
    Polygon,
    Star,
    Pen,
    Artboard,
    Eyedropper,
    ShapeBuilder,
    Type,
    /// Drag a line across the canvas to split intersected shapes at the cut.
    Knife,
    /// Drag along a path stroke to vary its width (width profile tool).
    Width,
    /// Click two shapes to blend between them with interpolated steps.
    Blend,
    /// Insert a chart / graph as shapes (stub).
    Graph,
    /// Fill enclosed regions formed by crossing paths.
    LivePaint,
    /// Apply a 4-corner perspective warp to a shape.
    PerspectiveDistort,
    /// Warp a shape using a configurable grid mesh.
    Envelope,
    /// Scallop distort warp tool (like Illustrator's Scallop).
    Scallop,
    /// Crystallize distort warp tool.
    Crystallize,
    /// Wrinkle distort warp tool.
    Wrinkle,
}

impl Tool {
    /// Short label for the tools strip / toolbar.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::DirectSelect => "Direct",
            Tool::Rect => "Rect",
            Tool::Ellipse => "Ellipse",
            Tool::Line => "Line",
            Tool::Polygon => "Polygon",
            Tool::Star => "Star",
            Tool::Pen => "Pen",
            Tool::Artboard => "Artboard",
            Tool::Eyedropper => "Eyedrop",
            Tool::ShapeBuilder => "Builder",
            Tool::Type => "Type",
            Tool::Knife => "Knife",
            Tool::Width => "Width",
            Tool::Blend => "Blend",
            Tool::Graph => "Graph",
            Tool::LivePaint => "Live Paint",
            Tool::PerspectiveDistort => "Persp",
            Tool::Envelope => "Envelope",
            Tool::Scallop => "Scallop",
            Tool::Crystallize => "Crystal",
            Tool::Wrinkle => "Wrinkle",
        }
    }

    /// A one- or two-letter glyph for the compact left tools strip.
    pub fn glyph(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::DirectSelect => "A",
            Tool::Rect => "R",
            Tool::Ellipse => "O",
            Tool::Line => "/",
            Tool::Polygon => "P",
            Tool::Star => "*",
            Tool::Pen => "✎",
            Tool::Artboard => "□",
            Tool::Eyedropper => "I",
            Tool::ShapeBuilder => "M",
            Tool::Type => "T",
            Tool::Knife => "K",
            Tool::Width => "W",
            Tool::Blend => "B",
            Tool::Graph => "G",
            Tool::LivePaint => "L",
            Tool::PerspectiveDistort => "D",
            Tool::Envelope => "E",
            Tool::Scallop => "~",
            Tool::Crystallize => "#",
            Tool::Wrinkle => "≈",
        }
    }

    /// Stable ordering for the tools strip (matches the egui palette grouping).
    pub const ALL: [Tool; 22] = [
        Tool::Select,
        Tool::DirectSelect,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Line,
        Tool::Polygon,
        Tool::Star,
        Tool::Pen,
        Tool::Artboard,
        Tool::Eyedropper,
        Tool::ShapeBuilder,
        Tool::Type,
        Tool::Knife,
        Tool::Width,
        Tool::Blend,
        Tool::Graph,
        Tool::LivePaint,
        Tool::PerspectiveDistort,
        Tool::Envelope,
        Tool::Scallop,
        Tool::Crystallize,
        Tool::Wrinkle,
    ];
}
