//! Panel seam for the GPUI host.
//!
//! ## The convention every panel follows (parallel-port spec)
//!
//! Each panel is a free function in its own module with this EXACT signature:
//!
//! ```ignore
//! pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement
//! ```
//!
//! - `app: &App` is **read-only**. A panel reads tool/brush/view/document state
//!   from it to build its element. A panel must NEVER mutate `App` directly.
//! - `cx: &mut Context<Pigment>` is the root view's context. A panel uses it to
//!   build click/event listeners that emit [`Action`]s (see below).
//!
//! ### How a panel emits an Action (copy this verbatim)
//!
//! Attach a listener built with `cx.listener` to any stateful element (an
//! element gets a click listener after `.id(...)`):
//!
//! ```ignore
//! div()
//!     .id("layer-vis-toggle")        // stateful: required before .on_click
//!     .cursor_pointer()
//!     .on_click(cx.listener(move |root, _ev, _win, cx| {
//!         root.app.apply(Action::ToggleLayerVisible(id));
//!         cx.notify();               // request a redraw
//!     }))
//! ```
//!
//! The listener closure receives `root: &mut Pigment` (the root view, which owns
//! `app: App`), so it calls `root.app.apply(action)` — the single mutation choke
//! point — then `cx.notify()` to schedule a re-render. Capture any per-row data
//! (e.g. a `LayerId`) by value with `move`. That is the ENTIRE round-trip; no
//! panel needs to touch `App` internals or the host directly.
//!
//! Each `.id(...)` must be unique within a frame; for per-row elements derive it
//! from the row's stable key, e.g. `("layer-vis", id.0)`.
//!
//! ### Layout
//!
//! The root view (`main.rs`) places panels into the chrome:
//! - [`toolbar`]  — top bar (full width).
//! - [`tools`]    — left vertical strip.
//! - center       — the canvas (`app.host.image()`), owned by the root view.
//! - right dock   — [`color`], [`adjustments`], [`histogram`], [`layers`]
//!   stacked top-to-bottom.
//!
//! Panels return only their own element; the root view owns the surrounding
//! flex containers and sizes the dock/strip/bar.

pub mod adjust_edit;
pub mod adjustments;
pub mod camera_raw;
pub mod channels;
pub mod color;
pub mod filter_gallery;
pub mod histogram;
pub mod history;
pub mod layer_comps;
pub mod layer_style;
pub mod layers;
pub mod navigator;
pub mod plugins;
pub mod print;
pub mod tool_options;
pub mod toolbar;
pub mod tools;
pub mod vanishing_point;

use gpui::{div, px, IntoElement, ParentElement, Styled};
use prism_ui::colors;

/// Shared chrome metrics so every panel/stub agrees on the layout.
pub const DOCK_W: f32 = 280.0;
pub const STRIP_W: f32 = 56.0;
pub const TOOLBAR_H: f32 = 40.0;

/// A labeled placeholder body used by stub panels until their real content
/// lands. Renders the title and a hint inside a section-colored box so the
/// window layout is correct now and an agent just replaces the body.
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
