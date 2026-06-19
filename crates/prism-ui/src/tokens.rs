//! Design tokens for the Prism suite UI.

pub mod colors {
    use gpui::Rgba;

    // Surfaces
    pub fn surface_bg() -> Rgba        { hex("#1a1a1c") }
    pub fn surface_raised() -> Rgba    { hex("#222226") }
    pub fn surface_overlay() -> Rgba   { hex("#2a2a2f") }
    pub fn surface_border() -> Rgba    { hex("#333338") }

    // Text
    pub fn text_primary() -> Rgba      { hex("#f0f0f2") }
    pub fn text_secondary() -> Rgba    { hex("#a0a0a8") }
    pub fn text_disabled() -> Rgba     { hex("#555560") }

    // Accent (purple-indigo)
    pub fn accent() -> Rgba            { hex("#7c5af5") }
    pub fn accent_hover() -> Rgba      { hex("#9070ff") }
    pub fn accent_pressed() -> Rgba    { hex("#6347d4") }

    // Status
    pub fn success() -> Rgba           { hex("#3dc97e") }
    pub fn warning() -> Rgba           { hex("#f5a623") }
    pub fn danger() -> Rgba            { hex("#f05555") }

    // Tool
    pub fn tool_active() -> Rgba       { hex("#7c5af5") }
    pub fn tool_hover() -> Rgba        { hex("#2e2e36") }

    fn hex(s: &str) -> Rgba {
        let s = s.trim_start_matches('#');
        let n = u32::from_str_radix(s, 16).unwrap_or(0);
        let r = ((n >> 16) & 0xff) as u8;
        let g = ((n >> 8) & 0xff) as u8;
        let b = (n & 0xff) as u8;
        Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }
}

pub mod spacing {
    pub const XS: f32 = 2.0;
    pub const SM: f32 = 4.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
    pub const XL: f32 = 16.0;
    pub const XXL: f32 = 24.0;
}

pub mod radius {
    pub const SM: f32 = 3.0;
    pub const MD: f32 = 5.0;
    pub const LG: f32 = 8.0;
    pub const PILL: f32 = 999.0;
}

pub mod font_size {
    pub const XS: f32 = 10.0;
    pub const SM: f32 = 11.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 13.0;
    pub const XL: f32 = 15.0;
    pub const TITLE: f32 = 18.0;
}
