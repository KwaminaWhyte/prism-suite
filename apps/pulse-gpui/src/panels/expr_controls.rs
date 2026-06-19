//! Expression Controls panel — shows and edits per-comp expression control layers.
//!
//! ExpressionControl layers carry a typed value (Slider, Angle, Checkbox, Color,
//! Point) that expressions on other layers can reference. This panel lists all
//! controls and lets the user adjust their values, and provides "Add Control"
//! buttons for each kind.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::colors;

use crate::app_state::{Action, App, ExprControl, ExprControlKind, ExprControlValue};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];

    // Collect control rows: (layer_idx, layer_name, ctrl)
    let mut controls: Vec<(usize, String, ExprControl)> = comp
        .layers
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            app.expr_controls
                .get(&i)
                .map(|ctrl| (i, l.name.clone(), ctrl.clone()))
        })
        .collect();
    controls.sort_by_key(|(i, _, _)| *i);

    // Pre-build control rows before div chain.
    let ctrl_rows: Vec<gpui::AnyElement> = controls
        .into_iter()
        .map(|(layer_idx, name, ctrl)| {
            let badge = ctrl.kind.label();
            let value_text = match &ctrl.value {
                ExprControlValue::Slider(v) => format!("{v:.1}"),
                ExprControlValue::Angle(v) => format!("{v:.1}°"),
                ExprControlValue::Checkbox(b) => if *b { "ON".into() } else { "off".into() },
                ExprControlValue::Color([r, g, b, _]) => {
                    format!("rgb({:.2},{:.2},{:.2})", r, g, b)
                }
                ExprControlValue::Point(x, y) => format!("({x:.1}, {y:.1})"),
            };
            let is_checkbox = matches!(ctrl.value, ExprControlValue::Checkbox(_));
            let checked = matches!(ctrl.value, ExprControlValue::Checkbox(true));

            let mut row = div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(colors::surface_border())
                // Kind badge
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(rgb(0x3e4a6a_u32))
                        .text_color(rgb(0x89b4fa_u32))
                        .text_size(px(10.0))
                        .child(badge),
                )
                // Layer name
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(name),
                )
                // Value display / toggle
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(value_text),
                );

            // Stepper −/+ for numeric kinds; toggle for checkbox.
            if is_checkbox {
                let toggle_val = ExprControlValue::Checkbox(!checked);
                row = row.child(
                    div()
                        .id(("exc-toggle", layer_idx))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(if checked { rgb(BG_ACTIVE) } else { colors::surface_raised() })
                        .text_color(if checked {
                            rgb(0x37c8c0_u32)
                        } else {
                            colors::text_secondary()
                        })
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .child(if checked { "ON" } else { "off" })
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::SetExprControlValue {
                                layer_idx,
                                value: toggle_val.clone(),
                            });
                            cx.notify();
                        })),
                );
            } else {
                // Decrement button
                let kind = ctrl.kind;
                row = row
                    .child(
                        div()
                            .id(("exc-dec", layer_idx))
                            .w(px(20.0))
                            .h(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(colors::surface_raised())
                            .text_color(colors::text_primary())
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .child("−")
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                if let Some(ctrl) = root.app.expr_controls.get(&layer_idx) {
                                    let new_val = step_value(&ctrl.value, kind, -1.0);
                                    root.app.apply(Action::SetExprControlValue {
                                        layer_idx,
                                        value: new_val,
                                    });
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .id(("exc-inc", layer_idx))
                            .w(px(20.0))
                            .h(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(colors::surface_raised())
                            .text_color(colors::text_primary())
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .child("+")
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                if let Some(ctrl) = root.app.expr_controls.get(&layer_idx) {
                                    let new_val = step_value(&ctrl.value, kind, 1.0);
                                    root.app.apply(Action::SetExprControlValue {
                                        layer_idx,
                                        value: new_val,
                                    });
                                    cx.notify();
                                }
                            })),
                    );
            }

            row.into_any_element()
        })
        .collect();

    // "Add Control" buttons — one per kind.
    let add_btns: Vec<gpui::AnyElement> = ExprControlKind::ALL
        .iter()
        .copied()
        .map(|kind| {
            div()
                .id(("exc-add", kind as usize))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child(format!("+ {}", kind.name()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::AddExpressionControl(kind));
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().text_size(px(11.0)).child("Expression Controls"));

    let add_row = div()
        .flex()
        .flex_wrap()
        .gap_1()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(colors::surface_border())
        .children(add_btns);

    let body: gpui::AnyElement = if ctrl_rows.is_empty() {
        div()
            .px_3()
            .py_2()
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .child("No controls. Add one below.")
            .into_any_element()
    } else {
        div().flex().flex_col().children(ctrl_rows).into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(body)
        .child(add_row)
}

/// Step a control value up or down by one unit (kind-appropriate).
fn step_value(val: &ExprControlValue, kind: ExprControlKind, dir: f32) -> ExprControlValue {
    match (val, kind) {
        (ExprControlValue::Slider(v), ExprControlKind::Slider) => {
            ExprControlValue::Slider((v + dir).clamp(0.0, 100.0))
        }
        (ExprControlValue::Angle(v), ExprControlKind::Angle) => {
            ExprControlValue::Angle(v + dir * 5.0)
        }
        (ExprControlValue::Color([r, g, b, a]), ExprControlKind::Color) => {
            ExprControlValue::Color([(r + dir * 0.05).clamp(0.0, 1.0), *g, *b, *a])
        }
        (ExprControlValue::Point(x, y), ExprControlKind::Point) => {
            ExprControlValue::Point(x + dir * 10.0, *y)
        }
        _ => val.clone(),
    }
}
