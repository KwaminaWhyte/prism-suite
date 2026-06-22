use super::*;

#[derive(Clone, Debug)]
pub struct ShapeGroupTransform {
    pub anchor: (f32, f32),
    pub position: (f32, f32),
    pub scale: (f32, f32),
    pub rotation: f32,
    pub skew: f32,
    pub skew_axis: f32,
    pub opacity: f32,
}

impl Default for ShapeGroupTransform {
    fn default() -> Self {
        Self {
            anchor: (0.0, 0.0),
            position: (0.0, 0.0),
            scale: (1.0, 1.0),
            rotation: 0.0,
            skew: 0.0,
            skew_axis: 0.0,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MergeMode {
    Normal,
    Add,
    Subtract,
    Intersect,
    ExcludeIntersections,
}

#[derive(Clone, Debug)]
pub enum TrimMultiple {
    Simultaneously,
    Individually,
}

#[derive(Clone, Debug)]
pub enum ShapeItemKind {
    Rectangle { size: (f32, f32), roundness: f32, position: (f32, f32) },
    Ellipse { size: (f32, f32), position: (f32, f32) },
    Star {
        points: u32,
        inner_radius: f32,
        outer_radius: f32,
        inner_roundness: f32,
        outer_roundness: f32,
        rotation: f32,
        position: (f32, f32),
    },
    Path { vertices: Vec<(f32, f32)>, closed: bool },
    Merge { mode: MergeMode },
    Trim { start: f32, end: f32, offset: f32, multiple: TrimMultiple },
    Twist { angle: f32, center: (f32, f32) },
    Repeater {
        copies: u32,
        offset: f32,
        anchor: (f32, f32),
        position: (f32, f32),
        scale: (f32, f32),
        rotation: f32,
        start_opacity: f32,
        end_opacity: f32,
    },
}

#[derive(Clone, Debug)]
pub struct ShapeLayerGroup {
    pub id: usize,
    pub name: String,
    pub transform: ShapeGroupTransform,
    pub items: Vec<ShapeItemKind>,
}
