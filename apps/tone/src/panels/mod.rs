pub mod ai_panel;
pub mod inspector;
pub mod toolbar;
pub mod tracks;
pub mod piano_roll;
pub mod timeline;
pub mod mixer;
pub mod session_view;

pub use ai_panel::render_ai_panel;
pub use inspector::{render_clip_inspector, render_track_inspector};
pub use toolbar::render_toolbar;
pub use tracks::render_tracks;
pub use piano_roll::render_piano_roll;
pub use timeline::render_timeline;
pub use mixer::render_mixer;
// Session View (Ableton-style scene/clip grid) is implemented and tested, but
// not yet wired into the main layout — there is no Arrangement/Session view-mode
// toggle slot for it yet. Kept exported and ready for the next UI wave.
#[allow(unused_imports)]
pub use session_view::render_session_view;

pub const TRACKS_W: f32 = 220.0;
pub const TOOLBAR_H: f32 = 48.0;
pub const TIMELINE_H: f32 = 140.0;
pub const MIXER_H: f32 = 120.0;
