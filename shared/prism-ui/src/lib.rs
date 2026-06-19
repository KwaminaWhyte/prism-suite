pub mod tokens;
pub mod icons;
pub mod components;
pub mod asset_source;

pub use tokens::{colors, spacing, radius, font_size};
pub use icons::Icon;
pub use components::{
    badge, card, divider, icon, icon_colored, label, section_header, tool_button,
};
pub use asset_source::PrismAssets;

/// Initialize prism-ui global state.
/// Call this inside `Application::run` before opening any window.
pub fn init(_cx: &mut gpui::App) {}
