//! Panel seam for the GPUI host.
//!
//! ## The convention every panel follows (parallel-port spec)
//!
//! Each panel is a free function in its own module with this EXACT signature:
//!
//! ```ignore
//! pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement
//! ```
//!
//! - `app: &App` is **read-only**. A panel reads project/tool/playhead/view
//!   state from it to build its element. A panel must NEVER mutate `App`
//!   directly.
//! - `cx: &mut Context<Reel>` is the root view's context. A panel uses it to
//!   build click/event listeners that emit [`Action`]s (see below).
//!
//! ### How a panel emits an Action (copy this verbatim)
//!
//! Attach a listener built with `cx.listener` to any stateful element (an
//! element gets a click listener after `.id(...)`):
//!
//! ```ignore
//! div()
//!     .id("track-eye-toggle")        // stateful: required before .on_click
//!     .cursor_pointer()
//!     .on_click(cx.listener(move |root, _ev, _win, cx| {
//!         root.app.apply(Action::ToggleTrackEnabled(ti));
//!         cx.notify();               // request a redraw
//!     }))
//! ```
//!
//! The listener closure receives `root: &mut Reel` (the root view, which owns
//! `app: App`), so it calls `root.app.apply(action)` — the single mutation choke
//! point — then `cx.notify()` to schedule a re-render. Capture any per-row data
//! (e.g. a track index) by value with `move`.
//!
//! Each `.id(...)` must be unique within a frame; for per-row elements derive it
//! from the row's stable key.
//!
//! ### Layout
//!
//! The root view (`main.rs`) places panels into the chrome:
//! - [`toolbar`]   — top bar (full width).
//! - center        — the preview (`app.host.image(...)`), owned by the root view.
//! - right dock    — [`inspector`] (read-only clip detail) over [`tracks`]
//!   (the clips/tracks list).
//! - bottom strip  — [`timeline`] (a placeholder strip this pass).
//!
//! Panels return only their own element; the root view owns the surrounding
//! flex containers and sizes the dock / bar / strip.

pub mod bins;
pub mod captions;
pub mod color_wheels;
pub mod curves;
pub mod export_presets;
pub mod graphics;
pub mod inspector;
pub mod markers;
pub mod mixer;
pub mod render_bar;
pub mod scopes;
pub mod sequence_settings;
pub mod timeline;
pub mod toolbar;
pub mod tracks;
pub mod viewer;
pub mod workspaces;

use std::collections::HashMap;

use gpui::{div, px, Entity, IntoElement, ParentElement, Styled};
use prism_ui::{colors, TextField};

/// Shared chrome metrics so every panel/stub agrees on the layout.
pub const DOCK_W: f32 = 280.0;
pub const TOOLBAR_H: f32 = 40.0;
pub const TIMELINE_H: f32 = 160.0;

/// The root view's registry of persistent, focusable [`TextField`] views, keyed
/// by a stable string (e.g. `"clip-name-3"`). Panels that expose real typing
/// take this read-only so they can render the field for a given key. The root
/// view (`Reel`) owns the map and creates each field via `Reel::text_field`;
/// see `main.rs`. Looking a key up that wasn't pre-created returns `None`, in
/// which case the panel falls back to a non-editable label.
pub type TextFields = HashMap<String, Entity<TextField>>;

/// A labeled placeholder body used by stub panels until their real content
/// lands. Renders the title and a hint inside a section-colored box so the
/// window layout is correct now and an agent just replaces the body. (Kept as a
/// ready helper for the next wave of panels; not all current panels use it.)
#[allow(dead_code)]
pub(crate) fn placeholder(title: &str, hint: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(div().text_color(colors::text_primary()).child(title.to_string()))
        .child(
            div()
                .min_h(px(48.0))
                .bg(colors::surface_overlay())
                .rounded_md()
                .p_2()
                .text_color(colors::text_secondary())
                .child(hint.to_string()),
        )
}
