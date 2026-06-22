use super::types_colors::ColorHarmonyRule;

/// Configuration for the scatter brush: copies of a symbol placed at regular
/// intervals along a drawn path, with optional size and rotation jitter.
#[derive(Clone, Debug)]
pub struct ScatterBrushConfig {
    /// The symbol to scatter (looked up in `App.symbol_lib`).
    pub symbol_id: u64,
    /// Distance between successive copies along the path (document units).
    pub spacing: f32,
    /// Fractional size variation (0.0 = uniform, 1.0 = ±100 %).
    pub size_jitter: f32,
    /// Rotational variation in degrees (0.0 = no variation).
    pub rotation_jitter: f32,
}

/// How glyphs are spaced when flowing along a path.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextOnPathSpacing {
    /// Let the renderer space glyphs automatically (default).
    #[default]
    Auto,
    /// Fixed advance between every glyph.
    Fixed,
    /// Optically balance the apparent spacing.
    Optical,
}

/// Extended configuration for the Recolor Artwork panel.
#[derive(Debug, Clone)]
pub struct RecolorConfig {
    /// Harmony rule used when rotating hues.
    pub harmony_rule: ColorHarmonyRule,
    /// If `true`, shapes with pure-black fills are left untouched.
    pub preserve_black: bool,
    /// If `true`, shapes with pure-white fills are left untouched.
    pub preserve_white: bool,
    /// When `true`, hue offsets are shuffled randomly instead of by rule.
    pub randomize: bool,
    /// Uniform brightness multiplier applied to all fills (1.0 = no change).
    pub brightness_scale: f32,
}

impl Default for RecolorConfig {
    fn default() -> Self {
        Self {
            harmony_rule: ColorHarmonyRule::Analogous,
            preserve_black: true,
            preserve_white: true,
            randomize: false,
            brightness_scale: 1.0,
        }
    }
}

// --- Batch 10: Variable Fonts & OpenType ---

/// A single variable-font axis value for a text shape.
#[derive(Debug, Clone)]
pub struct VariableAxisValue {
    pub axis_tag: String,   // e.g. "wght", "wdth", "ital", "slnt", "opsz"
    pub value: f32,
}

/// OpenType feature flags for a text shape.
#[derive(Debug, Clone)]
pub struct OpenTypeFeatures {
    pub ligatures: bool,
    pub discretionary_ligatures: bool,
    pub historical_ligatures: bool,
    pub contextual_alternates: bool,
    pub small_caps: bool,
    pub all_small_caps: bool,
    pub stylistic_set: Option<u8>,   // 1–20
    pub ordinals: bool,
    pub fractions: bool,
    pub slashed_zero: bool,
    pub tabular_figures: bool,
    pub proportional_figures: bool,
    pub superscript: bool,
    pub subscript: bool,
}

impl Default for OpenTypeFeatures {
    fn default() -> Self {
        Self {
            ligatures: true,
            discretionary_ligatures: false,
            historical_ligatures: false,
            contextual_alternates: true,
            small_caps: false,
            all_small_caps: false,
            stylistic_set: None,
            ordinals: false,
            fractions: false,
            slashed_zero: false,
            tabular_figures: false,
            proportional_figures: true,
            superscript: false,
            subscript: false,
        }
    }
}

// --- Batch 10: Character & Paragraph Panel ---

/// Kerning mode for a character.
#[derive(Debug, Clone, PartialEq)]
pub enum KerningMode {
    Auto,
    Optical,
    Manual(f32),
}

impl Default for KerningMode {
    fn default() -> Self { KerningMode::Auto }
}

/// Per-shape character style (tracking, kerning, baseline shift, scale, decoration).
#[derive(Debug, Clone)]
pub struct CharacterStyle {
    pub tracking: f32,          // em units, default 0.0
    pub kerning: KerningMode,
    pub baseline_shift: f32,    // points, default 0.0
    pub horizontal_scale: f32,  // %, default 100.0
    pub vertical_scale: f32,    // %, default 100.0
    pub underline: bool,
    pub strikethrough: bool,
    pub language: String,       // e.g. "en-US"
}

impl Default for CharacterStyle {
    fn default() -> Self {
        Self {
            tracking: 0.0,
            kerning: KerningMode::Auto,
            baseline_shift: 0.0,
            horizontal_scale: 100.0,
            vertical_scale: 100.0,
            underline: false,
            strikethrough: false,
            language: "en-US".to_string(),
        }
    }
}

/// Paragraph text alignment including justify variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParaAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
    JustifyAll,
    ForceJustify,
}

/// Per-shape paragraph style.
#[derive(Debug, Clone)]
pub struct ParagraphStyle {
    pub alignment: ParaAlignment,
    pub space_before: f32,
    pub space_after: f32,
    pub first_line_indent: f32,
    pub left_indent: f32,
    pub right_indent: f32,
    pub hyphenation: bool,
    pub keep_lines_together: bool,
    pub justify_last_line: bool,
    pub tab_stops: Vec<f32>,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            alignment: ParaAlignment::Left,
            space_before: 0.0,
            space_after: 0.0,
            first_line_indent: 0.0,
            left_indent: 0.0,
            right_indent: 0.0,
            hyphenation: false,
            keep_lines_together: false,
            justify_last_line: false,
            tab_stops: Vec::new(),
        }
    }
}
