// --- Batch 8: Image Trace mode ---

/// Image trace color mode for the extended Image Trace tool.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ImageTraceMode {
    #[default]
    Color,
    Grayscale,
    BlackWhite,
    Outlined,
    // Extended modes (Wave N)
    BlackAndWhite,
    Color3,
    Color6,
    Color16,
    Photo,
    Logo,
    Sketch,
    Silhouette,
    Technical,
}

/// Extended configuration for the Image Trace panel (Wave N).
#[derive(Debug, Clone)]
pub struct ImageTraceConfig {
    pub mode: ImageTraceMode,
    pub threshold: u8,
    pub colors: u8,
    pub paths: u8,
    pub corners: u8,
    pub noise: u8,
    pub method: String,
    pub fills: bool,
    pub strokes: bool,
    pub snap_curves: bool,
    pub ignore_white: bool,
}

impl ImageTraceConfig {
    pub fn new() -> Self {
        Self {
            mode: ImageTraceMode::Color6,
            threshold: 128,
            colors: 6,
            paths: 50,
            corners: 75,
            noise: 25,
            method: "Abutting".into(),
            fills: true,
            strokes: false,
            snap_curves: false,
            ignore_white: false,
        }
    }
}

impl Default for ImageTraceConfig {
    fn default() -> Self { Self::new() }
}

/// A single image-trace result (live or expanded).
#[derive(Debug, Clone)]
pub struct ImageTraceResult {
    pub source_image_id: usize,
    pub config: ImageTraceConfig,
    pub path_count: usize,
    pub expanded: bool,
}

// --- Wave N: Perspective Grid (extended) ---

/// The type of perspective grid overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerspectiveGridType {
    OnePoint,
    #[default]
    TwoPoint,
    ThreePoint,
}

/// One drawing plane of the perspective grid.
#[derive(Debug, Clone)]
pub struct PerspectivePlane {
    pub active: bool,
    pub color: String,
}

/// Full configuration for the extended Perspective Grid overlay.
#[derive(Debug, Clone)]
pub struct PerspectiveGridConfig {
    pub grid_type: PerspectiveGridType,
    pub visible: bool,
    pub snap: bool,
    pub left_plane: PerspectivePlane,
    pub right_plane: PerspectivePlane,
    pub floor_plane: PerspectivePlane,
    pub cell_size: f32,
    pub opacity: u8,
    pub vanishing_point_left: (f32, f32),
    pub vanishing_point_right: (f32, f32),
}

impl PerspectiveGridConfig {
    pub fn new() -> Self {
        Self {
            grid_type: PerspectiveGridType::TwoPoint,
            visible: false,
            snap: true,
            left_plane: PerspectivePlane { active: true, color: "#2196F3".into() },
            right_plane: PerspectivePlane { active: true, color: "#4CAF50".into() },
            floor_plane: PerspectivePlane { active: false, color: "#FF9800".into() },
            cell_size: 50.0,
            opacity: 50,
            vanishing_point_left: (-500.0, 0.0),
            vanishing_point_right: (500.0, 0.0),
        }
    }
}

impl Default for PerspectiveGridConfig {
    fn default() -> Self { Self::new() }
}

// --- Wave N: Global Swatches ---

/// A global or spot color swatch.
#[derive(Debug, Clone)]
pub struct GlobalSwatch {
    pub id: usize,
    pub name: String,
    pub color: String,
    pub is_global: bool,
    pub is_spot: bool,
    pub usage_count: usize,
}

/// A named group of swatches.
#[derive(Debug, Clone)]
pub struct SwatchGroup {
    pub id: usize,
    pub name: String,
    pub swatch_ids: Vec<usize>,
}

// --- Wave N: Artboards (extended) ---

/// A named artboard with position, size, and preset.
#[derive(Debug, Clone)]
pub struct Artboard {
    pub id: usize,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub preset: String,
}

impl Artboard {
    pub fn new(id: usize, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            id,
            name: format!("Artboard {}", id + 1),
            x,
            y,
            width: w,
            height: h,
            preset: "Custom".into(),
        }
    }
}

// --- Batch 8: Graph Tool types ---

/// Graph / chart type for the extended graph tool (Batch 8).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum GraphType {
    #[default]
    Column,
    Bar,
    Pie,
    Line,
    Scatter,
}

/// Graph data model: values, labels, and layout parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphData {
    pub graph_type: GraphType,
    pub cols: usize,
    pub rows: usize,
    pub values: Vec<f32>,
    pub labels: Vec<String>,
}

impl Default for GraphData {
    fn default() -> Self {
        Self {
            graph_type: GraphType::Column,
            cols: 3,
            rows: 2,
            values: vec![10.0, 20.0, 30.0, 15.0, 25.0, 35.0],
            labels: vec![],
        }
    }
}
