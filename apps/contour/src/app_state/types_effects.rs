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

// --- Batch 10: Blend Tool ---

/// How intermediate steps are distributed in a blend object.
#[derive(Debug, Clone, PartialEq)]
pub enum BlendSpacing {
    SpecifiedSteps(u32),
    SpecifiedDistance(f32),
    SmoothColor,
}

impl Default for BlendSpacing {
    fn default() -> Self { BlendSpacing::SpecifiedSteps(5) }
}

/// Orientation of the blend (relative to page or to the spine path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendOrientation {
    #[default]
    AlignToPage,
    AlignToPath,
}

/// A blend object interpolating between exactly two shapes.
#[derive(Debug, Clone)]
pub struct BlendObject {
    pub id: usize,
    pub name: String,
    pub shape_ids: Vec<usize>,       // exactly 2 source shapes
    pub spacing: BlendSpacing,
    pub orientation: BlendOrientation,
    pub spine_path_id: Option<usize>, // replaced spine path, if any
}

// --- Batch 10: 3D Effects (Extrude & Bevel, Revolve) ---

/// Bevel profile for 3D Extrude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BevelKind {
    #[default]
    None,
    Classic,
    Round,
    StepUp,
    Tall,
    Complex,
}

/// Whether bevel is inset or outset from the shape edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BevelExtent {
    #[default]
    Inn,
    Out,
}

/// Surface shading mode for 3D effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceShading {
    WireFrame,
    NoShading,
    DiffuseShading,
    #[default]
    PlasticShading,
}

/// Per-shape 3D Extrude & Bevel configuration.
#[derive(Debug, Clone)]
pub struct Extrude3D {
    pub depth: f32,
    pub bevel_kind: BevelKind,
    pub bevel_height: f32,
    pub bevel_extent: BevelExtent,
    pub surface: SurfaceShading,
    pub rotate_x: f32,
    pub rotate_y: f32,
    pub rotate_z: f32,
    pub perspective: f32,
    pub cap: bool,
    pub light_intensity: f32,
    pub ambient_light: f32,
    pub highlight_intensity: f32,
    pub highlight_size: f32,
    pub blend_steps: u32,
    pub draw_hidden_faces: bool,
    pub preserve_spot_colors: bool,
}

impl Default for Extrude3D {
    fn default() -> Self {
        Self {
            depth: 50.0,
            bevel_kind: BevelKind::None,
            bevel_height: 4.0,
            bevel_extent: BevelExtent::Inn,
            surface: SurfaceShading::PlasticShading,
            rotate_x: -26.0,
            rotate_y: -38.0,
            rotate_z: 0.0,
            perspective: 0.0,
            cap: true,
            light_intensity: 100.0,
            ambient_light: 50.0,
            highlight_intensity: 70.0,
            highlight_size: 90.0,
            blend_steps: 25,
            draw_hidden_faces: false,
            preserve_spot_colors: false,
        }
    }
}

/// Which edge a revolve rotates around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RevolveFrom {
    #[default]
    LeftEdge,
    RightEdge,
}

/// Per-shape 3D Revolve configuration.
#[derive(Debug, Clone)]
pub struct Revolve3D {
    pub angle: f32,
    pub offset: f32,
    pub from: RevolveFrom,
    pub cap: bool,
    pub surface: SurfaceShading,
    pub rotate_x: f32,
    pub rotate_y: f32,
    pub rotate_z: f32,
}

impl Default for Revolve3D {
    fn default() -> Self {
        Self {
            angle: 360.0,
            offset: 0.0,
            from: RevolveFrom::LeftEdge,
            cap: true,
            surface: SurfaceShading::PlasticShading,
            rotate_x: 0.0,
            rotate_y: -26.0,
            rotate_z: 0.0,
        }
    }
}

// --- Batch 10: PDF Export State ---

/// PDF/A and PDF/X standards compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfStandard {
    #[default]
    None,
    PdfA1b,
    PdfA2b,
    PdfX1a,
    PdfX3,
    PdfX4,
}

/// Acrobat compatibility level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfCompatibility {
    Pdf13,
    Pdf14,
    #[default]
    Pdf15,
    Pdf16,
    Pdf17,
}

/// Output colour space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfColorSpace {
    #[default]
    Rgb,
    Cmyk,
    Grayscale,
}

/// Printer's marks included in the export.
#[derive(Debug, Clone, Default)]
pub struct PdfMarks {
    pub trim: bool,
    pub bleed: bool,
    pub reg: bool,
    pub color_bars: bool,
    pub page_info: bool,
}

/// Which layers are exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfLayerVisibility {
    #[default]
    AsInDocument,
    AllVisible,
    AllHidden,
}

/// Full PDF export configuration (mirrors Illustrator's Save As PDF dialog).
#[derive(Debug, Clone)]
pub struct PdfExportConfig {
    pub standard: PdfStandard,
    pub compatibility: PdfCompatibility,
    pub embed_fonts: bool,
    pub subset_fonts: bool,
    pub compress_text_and_line_art: bool,
    pub flatten_transparency: bool,
    pub resolution: f32,
    pub color_space: PdfColorSpace,
    pub output_intent: String,
    pub include_bleed: bool,
    pub bleed_top: f32,
    pub bleed_bottom: f32,
    pub bleed_left: f32,
    pub bleed_right: f32,
    pub marks: PdfMarks,
    pub layer_visibility: PdfLayerVisibility,
    pub require_password: bool,
    pub user_password: String,
    pub owner_password: String,
    pub allow_printing: bool,
    pub allow_editing: bool,
    pub allow_copying: bool,
}

impl Default for PdfExportConfig {
    fn default() -> Self {
        Self {
            standard: PdfStandard::None,
            compatibility: PdfCompatibility::Pdf15,
            embed_fonts: true,
            subset_fonts: true,
            compress_text_and_line_art: true,
            flatten_transparency: false,
            resolution: 300.0,
            color_space: PdfColorSpace::Rgb,
            output_intent: String::new(),
            include_bleed: false,
            bleed_top: 3.0,
            bleed_bottom: 3.0,
            bleed_left: 3.0,
            bleed_right: 3.0,
            marks: PdfMarks::default(),
            layer_visibility: PdfLayerVisibility::AsInDocument,
            require_password: false,
            user_password: String::new(),
            owner_password: String::new(),
            allow_printing: true,
            allow_editing: false,
            allow_copying: false,
        }
    }
}
