// --- Batch 10: Gradient Mesh depth ---

/// A single control point in a gradient mesh.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeshPoint {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub tension: f32,
}

impl Default for MeshPoint {
    fn default() -> Self {
        Self { position: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0], tension: 1.0 }
    }
}

/// Configuration for the gradient mesh tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GradientMeshConfig {
    pub rows: u8,
    pub cols: u8,
    pub points: Vec<MeshPoint>,
}

impl Default for GradientMeshConfig {
    fn default() -> Self {
        Self { rows: 4, cols: 4, points: vec![] }
    }
}

// --- Batch 10: Flare Tool ---

/// Configuration for a lens-flare effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlareConfig {
    pub center: [f32; 2],
    pub brightness: f32,
    pub halo_size: f32,
    pub ray_count: u8,
    pub ray_length: f32,
    pub ring_count: u8,
    pub ring_spacing: f32,
    pub color: [f32; 4],
}

impl Default for FlareConfig {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            brightness: 100.0,
            halo_size: 50.0,
            ray_count: 10,
            ray_length: 100.0,
            ring_count: 5,
            ring_spacing: 50.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

// --- Batch 10: Pattern Brush depth ---

/// How a pattern brush tile fits along the stroke path.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum PatternBrushFit {
    #[default]
    Stretch,
    Tile,
    ApproximatePath,
    AddSpaceBetweenTiles,
    AlignToPixelGrid,
}

/// How the pattern brush colorizes its tile art.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ColorizeMethod {
    #[default]
    None,
    Tints,
    TintsAndShades,
    Hue,
    Full,
}

/// Configuration for the pattern brush.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatternBrushConfig {
    pub name: String,
    pub scale: f32,
    pub spacing: f32,
    pub colorize_method: ColorizeMethod,
    pub fit: PatternBrushFit,
    pub flip_across_path: bool,
    pub flip_along_path: bool,
}

impl Default for PatternBrushConfig {
    fn default() -> Self {
        Self {
            name: "Pattern Brush".to_string(),
            scale: 100.0,
            spacing: 100.0,
            colorize_method: ColorizeMethod::None,
            fit: PatternBrushFit::Stretch,
            flip_across_path: false,
            flip_along_path: false,
        }
    }
}

/// Configuration for the Symbol Sprayer tool.
#[derive(Debug, Clone)]
pub struct SymbolSprayConfig {
    /// Which symbol from the library to spray.
    pub symbol_id: u64,
    /// Diameter of the spray brush in document units.
    pub diameter: f32,
    /// Average number of instances per spray event (0..=10).
    pub density: f32,
    /// Positional scatter (0.0 = tightly grouped, 1.0 = fills the whole diameter).
    pub scatter: f32,
    /// Maximum rotation jitter in radians.
    pub rotation_jitter: f32,
    /// Maximum size jitter as a fraction of the base size.
    pub size_jitter: f32,
    /// Maximum opacity jitter (0.0 = no variation).
    pub opacity_jitter: f32,
}

impl Default for SymbolSprayConfig {
    fn default() -> Self {
        Self {
            symbol_id: 0,
            diameter: 80.0,
            density: 5.0,
            scatter: 0.3,
            rotation_jitter: 0.1,
            size_jitter: 0.1,
            opacity_jitter: 0.0,
        }
    }
}

// --- Batch 11: Pathfinder depth ---

/// The full set of Pathfinder shape-mode and effect operations.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PathfinderOp {
    // Shape modes
    Unite,
    Minus,
    Intersect,
    Exclude,
    // Pathfinder effects
    Divide,
    Trim,
    Merge,
    Crop,
    Outline,
    MinusBack,
}

// --- Batch 11: 3D Extrude depth ---

/// Surface shading mode for 3D Extrude & Bevel.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExtrudeSurface {
    #[default]
    PlasticShading,
    DiffuseShading,
    WireframeOnly,
    NoShading,
}

/// Cap style for 3D Extrude & Bevel.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExtrudeCapStyle {
    #[default]
    Round,
    Bevel,
    None_,
}

/// Full configuration for the 3D Extrude & Bevel effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtrudeConfig {
    pub depth: f32,
    pub rotation_x: f32,
    pub rotation_y: f32,
    pub rotation_z: f32,
    pub perspective: f32,
    pub surface: ExtrudeSurface,
    pub cap_style: ExtrudeCapStyle,
    pub bevel_height: f32,
    pub bevel_extent: bool,
    pub map_art: bool,
    pub light_intensity: f32,
    pub ambient_light: f32,
    pub specular_highlight: f32,
    pub gloss: f32,
}

impl Default for ExtrudeConfig {
    fn default() -> Self {
        Self {
            depth: 50.0,
            rotation_x: -26.0,
            rotation_y: -38.0,
            rotation_z: 0.0,
            perspective: 0.0,
            surface: ExtrudeSurface::PlasticShading,
            cap_style: ExtrudeCapStyle::Round,
            bevel_height: 4.0,
            bevel_extent: false,
            map_art: false,
            light_intensity: 100.0,
            ambient_light: 50.0,
            specular_highlight: 70.0,
            gloss: 25.0,
        }
    }
}

// --- Batch 11: Chart depth ---

/// A single data series for the chart tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChartDataSet {
    pub label: String,
    pub values: Vec<f64>,
    pub color: [f32; 4],
}

impl Default for ChartDataSet {
    fn default() -> Self {
        Self { label: "Series 1".to_string(), values: vec![], color: [0.2, 0.5, 0.9, 1.0] }
    }
}

/// Full chart layout and data configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChartConfig {
    pub datasets: Vec<ChartDataSet>,
    pub category_labels: Vec<String>,
    pub title: String,
    pub show_legend: bool,
    pub show_grid: bool,
    pub value_axis_min: Option<f64>,
    pub value_axis_max: Option<f64>,
    pub column_width: f32,
    pub cluster_width: f32,
    pub shadow: bool,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            datasets: vec![],
            category_labels: vec![],
            title: String::new(),
            show_legend: true,
            show_grid: false,
            value_axis_min: None,
            value_axis_max: None,
            column_width: 90.0,
            cluster_width: 80.0,
            shadow: false,
        }
    }
}

// --- Batch 11: Envelope Distort depth ---

/// Whether the envelope editor is editing the envelope mesh or the contents.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum EnvelopeEditMode {
    #[default]
    Envelope,
    Contents,
}

/// Warp preset styles for Envelope Distort.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum EnvelopeWarpStyle {
    #[default]
    None_,
    Arc,
    ArcLower,
    ArcUpper,
    Arch,
    Bulge,
    ShellLower,
    ShellUpper,
    Flag,
    Wave,
    Fish,
    Rise,
    FishEye,
    Inflate,
    Squeeze,
    Twist,
}

/// Configuration for the Envelope Distort warp.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnvelopeConfig {
    pub warp_style: EnvelopeWarpStyle,
    pub horizontal: bool,
    pub bend: f32,
    pub h_distortion: f32,
    pub v_distortion: f32,
    pub fidelity: f32,
    pub edit_mode: EnvelopeEditMode,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            warp_style: EnvelopeWarpStyle::None_,
            horizontal: true,
            bend: 50.0,
            h_distortion: 0.0,
            v_distortion: 0.0,
            fidelity: 50.0,
            edit_mode: EnvelopeEditMode::Envelope,
        }
    }
}

// --- Batch 10: Variable Fonts ---

/// A single OpenType variation axis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontAxis {
    pub tag: String,
    pub min: f32,
    pub max: f32,
    pub value: f32,
}

impl Default for FontAxis {
    fn default() -> Self {
        Self { tag: "wght".to_string(), min: 100.0, max: 900.0, value: 400.0 }
    }
}

/// Configuration for variable-font axis editing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VariableFontConfig {
    pub axes: Vec<FontAxis>,
    pub preview_text: String,
}

impl Default for VariableFontConfig {
    fn default() -> Self {
        Self { axes: vec![], preview_text: "Sphinx of black quartz".to_string() }
    }
}
