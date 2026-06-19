//! History panel — scrollable undo step list with snapshot support.
//!
//! Each row shows: step number + action label. Current state is highlighted;
//! clicking a row emits `Action::UndoTo(idx)`. A "Snapshot" button saves the
//! current state as a named snapshot.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, Div, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, section_header};

use crate::app_state::{Action, App};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let labels = &app.history_labels;
    let current = labels.len();

    let rows: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(idx, label)| {
            let is_current = idx + 1 == current;
            let row_bg = if is_current {
                colors::surface_overlay()
            } else {
                colors::surface_raised()
            };
            let text_color = if is_current {
                colors::text_primary()
            } else {
                colors::text_secondary()
            };
            let display = format!("{}. {}", idx + 1, label);
            div()
                .id(("hist-row", idx as u64))
                .flex()
                .items_center()
                .px_2()
                .py_1()
                .bg(row_bg)
                .border_b_1()
                .border_color(colors::surface_border())
                .text_size(px(font_size::XS))
                .text_color(text_color)
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::UndoTo(idx));
                    cx.notify();
                }))
                .child(display)
        })
        .collect();

    let snap_rows: Vec<_> = app.snapshots.iter().enumerate().map(|(i, (name, _))| {
        let display = format!("[snap] {}", name);
        div()
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(colors::surface_border())
            .text_size(px(font_size::XS))
            .text_color(colors::text_secondary())
            .child(div().flex_1().child(display))
            .child(
                div()
                    .id(("snap-label", i as u64))
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .child(format!("#{}", i + 1))
            )
    }).collect();

    let snap_count = app.snapshots.len();

    div()
        .w_full()
        .flex()
        .flex_col()
        .bg(colors::surface_raised())
        .child(section_header("History"))
        .child(
            div()
                .px_2()
                .py_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child(format!("{current} steps")),
                )
                .child(
                    div()
                        .id("history-snapshot-btn")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .text_size(px(font_size::XS))
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let name = format!("Snapshot {}", root.app.snapshots.len() + 1);
                            root.app.apply(Action::CreateSnapshot(name));
                            cx.notify();
                        }))
                        .child("+ Snapshot")
                )
        )
        .child(divider())
        .children(rows)
        .when(labels.is_empty(), |s: Div| {
            s.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .child("No history yet"),
            )
        })
        .when(!snap_rows.is_empty(), |d| {
            d.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .child(format!("Snapshots ({snap_count})"))
            )
            .children(snap_rows)
        })
}
