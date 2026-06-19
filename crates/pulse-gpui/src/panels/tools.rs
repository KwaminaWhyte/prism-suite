//! Left tools strip — one button per [`Tool`], emitting `Action::SetTool`.
//!
//! STUB: the tool set is a starter subset (Select / Hand / Pen); the active tool
//! is highlighted. A later wave attaches real per-tool behavior in the preview.

use gpui::{
    div, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, px,
};

use prism_ui::{Icon, colors, components::tool_button};

use crate::app_state::{Action, App, Tool};
use crate::Pulse;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let active = app.active;
    let buttons = Tool::ALL
        .iter()
        .copied()
        .map(|t| {
            let icon = match t {
                Tool::Select      => Icon::Cursor,
                Tool::Hand        => Icon::Move,
                Tool::Pen         => Icon::Pen,
                Tool::AnchorPoint => Icon::Move,
                Tool::Puppet      => Icon::Pen,
            };
            let is_active = t == active;
            let _label = t.label();
            div()
                .id(("tool", t as usize))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetTool(t));
                    cx.notify();
                }))
                .child(tool_button(icon, is_active))
        })
        .collect::<Vec<_>>();

    // "Add Null" button — emits AddNullLayer, styled like a secondary tool button
    let add_null = div()
        .id("add-null")
        .cursor_pointer()
        .w(px(28.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(colors::text_secondary())
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::AddNullLayer);
            cx.notify();
        }))
        .child("N");

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .pt_2()
        .children(buttons)
        .child(add_null)
}
