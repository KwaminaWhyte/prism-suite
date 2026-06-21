pub mod toolbar;
pub mod tracks;
pub mod piano_roll;
pub mod timeline;

pub use toolbar::render_toolbar;
pub use tracks::render_tracks;
pub use piano_roll::render_piano_roll;
pub use timeline::render_timeline;

pub const TRACKS_W: f32 = 220.0;
pub const TOOLBAR_H: f32 = 48.0;
pub const TIMELINE_H: f32 = 140.0;
