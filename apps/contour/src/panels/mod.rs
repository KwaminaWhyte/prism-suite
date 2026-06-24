//! Panel seam for the GPUI host (mirrors `pigment-gpui/src/panels/mod.rs`).
//!
//! ## The convention every panel follows (parallel-port spec)
//!
//! Each panel is a free function in its own module with this EXACT signature:
//!
//! ```ignore
//! pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement
//! ```
//!
//! - `app: &App` is **read-only**. A panel reads tool/selection/document state
//!   from it to build its element. A panel must NEVER mutate `App` directly.
//! - `cx: &mut Context<Contour>` is the root view's context. A panel uses it to
//!   build click/event listeners that emit [`Action`](crate::app_state::Action)s.
//!
//! ### How a panel emits an Action (copy this verbatim)
//!
//! Attach a listener built with `cx.listener` to any stateful element (an
//! element gets a click listener after `.id(...)`):
//!
//! ```ignore
//! div()
//!     .id(("shape-vis", i))          // stateful: required before .on_click
//!     .cursor_pointer()
//!     .on_click(cx.listener(move |root, _ev, _win, cx| {
//!         root.app.apply(Action::ToggleShapeVisible(i));
//!         cx.notify();               // request a redraw
//!     }))
//! ```
//!
//! The listener closure receives `root: &mut Contour` (the root view, which owns
//! `app: App`), so it calls `root.app.apply(action)` — the single mutation choke
//! point — then `cx.notify()`. That is the ENTIRE round-trip; no panel touches
//! `App` internals or the host directly.
//!
//! ### Layout
//!
//! The root view (`main.rs`) places panels into the chrome:
//! - [`toolbar`] — top bar (full width).
//! - [`tools`]   — left vertical strip.
//! - center      — the canvas (`app.host.image(&app.doc)`), owned by the root.
//! - right dock  — [`layers`] (the one real dock panel this pass ships).

pub mod artboards;
pub mod character;
pub mod graphic_styles;
pub mod inspector;
pub mod layers;
pub mod recolor;
pub mod symbols;
pub mod toolbar;
pub mod tools;
pub mod trace_dialog;
pub mod welcome;

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::colors;

use crate::numeric_edit::{NumericEdit, NumericTarget};
use crate::Contour;

// Re-export the design system tokens and components for panels.
pub use prism_ui::{
    colors as ui_colors,
    divider as ui_divider,
    section_header as ui_section_header,
    tool_button,
    Icon,
};

/// Shared chrome metrics so every panel/stub agrees on the layout.
pub const DOCK_W: f32 = 260.0;
pub const STRIP_W: f32 = 56.0;
pub const TOOLBAR_H: f32 = 40.0;

/// Accent hex constant (used in overlays that need a u32 for `rgb()`).
pub const ACCENT: u32 = 0x7c5af5;

/// A typeable numeric value cell shared by the inspector and Character panels.
///
/// Renders the live focused `TextField` when `numeric` is the active inline edit
/// for `target`; otherwise a clickable value label (`display`) that begins an
/// inline edit seeded with `edit_seed` on click. `id` must be unique per cell.
pub(crate) fn numeric_cell(
    id: (&'static str, u64),
    target: NumericTarget,
    display: String,
    edit_seed: String,
    numeric: Option<&NumericEdit>,
    cx: &mut Context<Contour>,
) -> gpui::AnyElement {
    use gpui::IntoElement as _;
    if let Some(edit) = numeric.filter(|e| e.target == target) {
        // Live field — the user is typing into it right now.
        edit.field.clone().into_any_element()
    } else {
        div()
            .id(id)
            .min_w(px(56.0))
            .px_1()
            .flex()
            .justify_center()
            .rounded_sm()
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_raised()))
            .on_click(cx.listener(move |root, _ev, win, cx| {
                root.begin_numeric_edit(target, edit_seed.clone(), win, cx);
            }))
            .child(display)
            .into_any_element()
    }
}

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
