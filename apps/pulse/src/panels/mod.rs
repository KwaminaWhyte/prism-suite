//! Panel seam for the GPUI host (mirrors the sibling Pigment app's convention).
//!
//! ## The convention every panel follows (parallel-port spec)
//!
//! Each panel is a free function in its own module with this EXACT signature:
//!
//! ```ignore
//! pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement
//! ```
//!
//! - `app: &App` is **read-only**. A panel reads tool/transport/document state
//!   from it to build its element. A panel must NEVER mutate `App` directly.
//! - `cx: &mut Context<Pulse>` is the root view's context. A panel uses it to
//!   build click/event listeners that emit [`Action`](crate::app_state::Action)s.
//!
//! ### How a panel emits an Action (copy this verbatim)
//!
//! Attach a listener built with `cx.listener` to any stateful element (an
//! element gets a click listener after `.id(...)`):
//!
//! ```ignore
//! div()
//!     .id(("layer-vis", i))          // stateful: required before .on_click
//!     .cursor_pointer()
//!     .on_click(cx.listener(move |root, _ev, _win, cx| {
//!         root.app.apply(Action::ToggleLayerVisible(i));
//!         cx.notify();               // request a redraw
//!     }))
//! ```
//!
//! The listener closure receives `root: &mut Pulse` (the root view, which owns
//! `app: App`), so it calls `root.app.apply(action)` — the single mutation choke
//! point — then `cx.notify()` to schedule a re-render. Capture any per-row data
//! (e.g. a layer index) by value with `move`. That is the ENTIRE round-trip.
//!
//! Each `.id(...)` must be unique within a frame; for per-row elements derive it
//! from the row's stable key, e.g. `("layer-vis", i)`.
//!
//! ### Layout
//!
//! The root view (`main.rs`) places panels into the chrome:
//! - [`toolbar`]  — top bar (full width).
//! - [`tools`]    — left vertical strip.
//! - center       — the preview (`app.host.image(...)`), owned by the root view.
//! - right dock   — [`layers`] (more panels stack here per wave).
//! - bottom       — [`timeline`] placeholder strip.
//!
//! Panels return only their own element; the root view owns the surrounding
//! flex containers and sizes the dock/strip/bar/timeline.

pub mod comp_settings;
pub mod effects;
pub mod expr_controls;
pub mod expr_editor;
pub mod expressions;
pub mod graph;
pub mod keying_lights;
pub mod layers;
pub mod output_module;
pub mod parse;
pub mod preferences;
pub mod preview_panel;
pub mod properties;
pub mod render_queue;
pub mod timeline;
pub mod toolbar;
pub mod tools;
pub mod welcome;

use gpui::{div, px, IntoElement, ParentElement, Styled};
use prism_ui::colors;

/// Shared chrome metrics so every panel/stub agrees on the layout.
pub const DOCK_W: f32 = 260.0;
pub const STRIP_W: f32 = 56.0;
pub const TOOLBAR_H: f32 = 40.0;
pub const TIMELINE_H: f32 = 240.0;

/// Active / selected row highlight (no prism-ui token yet — keep as literal).
pub const BG_ACTIVE: u32 = 0x2e2e36;

/// A labeled placeholder body used by stub panels until their real content
/// lands. Renders the title and a hint inside a section-colored box so the
/// window layout is correct now and an agent just replaces the body.
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
                .bg(colors::surface_raised())
                .rounded_md()
                .p_2()
                .text_color(colors::text_secondary())
                .child(hint.to_string()),
        )
}
