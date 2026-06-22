pub mod toolbar;
pub mod layers;
pub mod timeline;
pub mod ai_panel;
pub mod inspector;

pub use toolbar::render_toolbar;
pub use layers::render_layers;
pub use timeline::render_timeline;
pub use ai_panel::render_ai_panel;
pub use inspector::render_inspector;

pub const LAYERS_W: f32 = 240.0;
pub const AI_PANEL_W: f32 = 260.0;
pub const TOOLBAR_H: f32 = 44.0;
pub const TIMELINE_H: f32 = 180.0;
