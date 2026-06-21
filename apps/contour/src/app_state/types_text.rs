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
