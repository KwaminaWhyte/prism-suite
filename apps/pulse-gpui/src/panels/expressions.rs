//! Expression editor stub — a small text-input bar that shows/edits per-property
//! expressions for the **selected** layer.
//!
//! When the user right-clicks a property stopwatch an "Add Expression" action
//! sets a placeholder expression (emitting `Action::SetExpression`). This panel
//! then shows an expression row for every property on the selected layer that has
//! a non-empty expression, along with a small "×" clear button.
//!
//! The evaluate-and-apply logic lives in `pulse_app::comp::expr` (the rhai engine)
//! via the track's `.expression` field — this panel only lets the user see and
//! clear the expression; inline editing of the text requires a text-input widget
//! that GPUI 0.2.2 provides via `TextInput`, which is left as a follow-up.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// Accent for active expression rows.
const EXPR_ACCENT: u32 = 0xe5c07b;

/// Render the expression editor panel. Shows one row per active expression on
/// the selected layer. Empty panel (with header) when nothing is selected or no
/// expressions are set.
pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().child("Expressions"))
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("= per-prop"),
        );

    let Some(idx) = app.selected_layer else {
        return div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(colors::surface_border())
            .child(header)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_color(colors::text_secondary())
                    .text_size(px(10.0))
                    .child("Select a layer."),
            );
    };

    // Collect the expressions for this layer (sorted by prop name for stability).
    let mut exprs: Vec<(String, String)> = app
        .expressions
        .iter()
        .filter(|((li, _), _)| *li == idx)
        .map(|((_, prop), expr)| (prop.clone(), expr.clone()))
        .collect();
    exprs.sort_by(|a, b| a.0.cmp(&b.0));

    if exprs.is_empty() {
        return div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(colors::surface_border())
            .child(header)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_color(colors::text_secondary())
                    .text_size(px(10.0))
                    .child("No expressions on this layer."),
            );
    }

    let rows = exprs
        .into_iter()
        .enumerate()
        .map(|(ri, (prop, expr))| {
            let prop_c = prop.clone();
            let expr_c = expr.clone();
            div()
                .flex()
                .flex_col()
                .mx_2()
                .my_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        // "=" badge.
                        .child(
                            div()
                                .text_color(rgb(EXPR_ACCENT))
                                .text_size(px(11.0))
                                .child("="),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_color(colors::text_primary())
                                .text_size(px(11.0))
                                .child(prop.clone()),
                        )
                        // Clear expression button.
                        .child(
                            div()
                                .id(("expr-clear", idx * 256 + ri))
                                .w(px(18.0))
                                .h(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .bg(rgb(BG_ACTIVE))
                                .text_color(colors::text_primary())
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .child("×")
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::SetExpression {
                                        layer_id: idx,
                                        prop: prop_c.clone(),
                                        expr: String::new(), // empty = clear
                                    });
                                    cx.notify();
                                })),
                        ),
                )
                // The expression text (read-only readout; editing is a follow-up).
                .child(
                    div()
                        .px_2()
                        .pb_1()
                        .text_color(rgb(EXPR_ACCENT))
                        .text_size(px(10.0))
                        .child(expr_c),
                )
        })
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .children(rows)
}

