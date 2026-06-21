/// Art-brush colorization mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArtBrushColorize {
    #[default]
    None,
    Tints,
    HueShift,
}

/// Art-brush configuration: paint a stretchable symbol art along a path.
#[derive(Clone, Debug)]
pub struct ArtBrushConfig {
    /// The symbol artwork to stretch along the path.
    pub symbol_id: u64,
    /// Scale factor applied to the symbol width (height scales with the path width).
    pub width_scale: f32,
    /// Colorization mode.
    pub colorize: ArtBrushColorize,
    /// Flip the brush art across the path's normal axis.
    pub flip: bool,
}
