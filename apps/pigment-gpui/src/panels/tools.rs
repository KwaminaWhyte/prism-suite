//! Tools strip — left vertical strip of tool buttons.
//!
//! Renders `Tool::ALL` as a dense vertical stack of square icon buttons inside
//! the ~56px left strip laid out by `main.rs`. Each button shows the tool's
//! corresponding `prism_ui::Icon` via `prism_ui::tool_button`. The active tool
//! gets the accent highlight (`colors::tool_active()`); idle buttons get the
//! raised surface background and animate to the hover wash on pointer-over. Thin
//! separators group the palette (selection / paint / retouch / vector-shape / …).
//!
//! The only interaction is the `SetTool` Action round-trip: a click on a button
//! routes through `root.app.apply(Action::SetTool(t))` + `cx.notify()` (the
//! convention documented in `panels/mod.rs`). `app` is read-only.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, Div, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, tool_button, Icon};

use crate::app_state::{Action, App, Tool};
use crate::Pigment;

/// Map each Tool variant to its prism-ui Icon.
fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Move        => Icon::Move,
        Tool::MoveLayer   => Icon::Move,
        Tool::Brush       => Icon::Brush,
        Tool::Eraser      => Icon::Eraser,
        Tool::Clone       => Icon::Clone,
        Tool::Heal        => Icon::Heal,
        Tool::Dodge       => Icon::Brush,    // brighten brush
        Tool::Burn        => Icon::Pencil,   // darken brush
        Tool::Smudge      => Icon::Eraser,   // blend brush
        Tool::Fill        => Icon::Fill,
        Tool::Eyedropper  => Icon::Eyedropper,
        Tool::SelectRect  => Icon::Rect,
        Tool::SelectEllipse => Icon::Ellipse,
        Tool::Lasso       => Icon::Lasso,
        Tool::MagicWand   => Icon::Wand,
        Tool::Transform   => Icon::Cursor,
        Tool::Crop        => Icon::Crop,
        Tool::Text        => Icon::Text,
        Tool::Pen         => Icon::Pen,
        Tool::ShapeRect   => Icon::Rect,
        Tool::ShapeEllipse => Icon::Ellipse,
        Tool::Gradient    => Icon::Gradient,
        Tool::Slice       => Icon::Crop,
        Tool::Liquify     => Icon::Wand,
    }
}

/// Index of the first tool in each palette group. A thin separator is drawn
/// before every group boundary except the first. Indices refer to positions in
/// `Tool::ALL`:
///   0 Move, 1 MoveLayer | 2 Brush, 3 Eraser | 4 Clone, 5 Heal, 6 Dodge, 7 Burn, 8 Smudge |
///   9 Fill, 10 Eyedropper | 11 Rect, 12 Ellipse, 13 Lasso, 14 Wand |
///   15 Transform, 16 Crop | 17 Text, 18 Pen | 19 RectS, 20 EllpS | 21 Gradient
const GROUP_STARTS: [usize; 9] = [0, 2, 4, 9, 11, 15, 17, 19, 21];

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let active = app.active;

    let mut items = Vec::with_capacity(Tool::ALL.len() + GROUP_STARTS.len());

    for (idx, tool) in Tool::ALL.into_iter().enumerate() {
        // Thin separator before each group boundary (skip the very first).
        if idx != 0 && GROUP_STARTS.contains(&idx) {
            items.push(
                div()
                    .id(("tool-sep", idx as u64))
                    .w(px(28.0))
                    .h(px(1.0))
                    .my_1()
                    .bg(colors::surface_border())
                    .into_any_element(),
            );
        }

        let is_active = tool == active;
        let icon = tool_icon(tool);

        let button = div()
            .id(("tool", idx as u64))
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetTool(tool));
                cx.notify();
            }))
            .when(!is_active, |s: gpui::Stateful<Div>| {
                s.hover(|s| s.bg(colors::tool_hover()))
            })
            .child(tool_button(icon, is_active));

        items.push(button.into_any_element());
    }

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .py_2()
        .children(items)
}
