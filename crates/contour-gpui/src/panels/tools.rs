//! Left tools strip — one icon button per [`Tool`], the active one highlighted.
//!
//! Uses `prism_ui::tool_button` (Icon, active) so every button shares the
//! design-system chrome: 36×36 rounded tile, accent bg when active, SVG icon
//! centred at 16 px.  Clicking a tool emits `Action::SetTool(tool)`.

use gpui::{
    div, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App, Tool};
use crate::panels::{tool_button, Icon};
use crate::Contour;

/// Map a `Tool` to the closest `Icon` from the prism-ui set.
fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Select       => Icon::Cursor,
        Tool::DirectSelect => Icon::Cursor,
        Tool::Rect         => Icon::Rect,
        Tool::Ellipse      => Icon::Ellipse,
        Tool::Line         => Icon::Line,
        Tool::Polygon      => Icon::Polygon,
        Tool::Star         => Icon::Star,
        Tool::Pen          => Icon::Pen,
        Tool::Artboard     => Icon::Grid,
        Tool::Eyedropper   => Icon::Eyedropper,
        Tool::ShapeBuilder => Icon::Shape,
        Tool::Type         => Icon::Text,
        Tool::Knife        => Icon::Scissors,
        Tool::Width        => Icon::Wand,
        Tool::Blend        => Icon::Transition,
        Tool::Graph              => Icon::Grid,
        Tool::LivePaint          => Icon::Shape,
        Tool::PerspectiveDistort => Icon::Wand,
        Tool::Envelope           => Icon::Wand,
    }
}

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let active = app.active;

    let buttons = Tool::ALL
        .iter()
        .copied()
        .enumerate()
        .map(|(i, tool)| {
            let is_active = tool == active;
            div()
                .id(("tool-btn", i))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetTool(tool));
                    cx.notify();
                }))
                .child(tool_button(tool_icon(tool), is_active))
        })
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .py_2()
        .children(buttons)
}
