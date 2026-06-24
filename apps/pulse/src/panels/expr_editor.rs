//! Expression editor pop-out — a focused panel bound to one property's
//! expression on the **selected** layer.
//!
//! GPUI 0.2.2 ships no free-form text-input widget (the sibling `expressions`
//! panel notes inline editing as a follow-up), so this editor drives the
//! expression string through curated **preset** chips — the standard After
//! Effects expression idioms (`wiggle`, `loopOut`, `time*100`, `valueAtTime`,
//! …) — each emitting [`Action::SetExpression`] for the bound property. A
//! property selector switches which `(layer, prop)` the editor targets (stored
//! in `app.expr_editor_prop`), the current expression text is shown read-only,
//! "Evaluate" runs the rhai engine via [`Action::EvaluateExpression`], and the
//! scalar result / any error from `app.last_expr_result` /
//! `app.expr_errors` is displayed.
//!
//! Reads `&App`, emits [`Action`](crate::app_state::Action)s. Gated by
//! `app.expr_editor_open` (toggled from the toolbar).

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, section_header};

use crate::app_state::{Action, App};
use crate::comp::Prop;
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// Accent for the expression text (matches the `expressions` panel palette).
const EXPR_ACCENT: u32 = 0xe5c07b;

/// Curated preset expressions (label, expression string) covering the common AE
/// idioms the rhai engine in `crate::comp::expr` supports.
const PRESETS: &[(&str, &str)] = &[
    ("wiggle(2, 30)", "wiggle(2, 30)"),
    ("loopOut(\"cycle\")", "loopOut(\"cycle\")"),
    ("loopOut(\"pingpong\")", "loopOut(\"pingpong\")"),
    ("time * 100", "time * 100"),
    ("valueAtTime(time-0.5)", "valueAtTime(time - 0.5)"),
    ("linear remap", "linear(time, 0, 1, 0, 100)"),
    ("ease remap", "ease(time, 0, 1, 0, 100)"),
    ("value", "value"),
];

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().child(section_header("Expression Editor")))
        .child(
            div()
                .id("expred-close")
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
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.expr_editor_open = false;
                    cx.notify();
                })),
        );

    let Some(layer_id) = app.selected_layer else {
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
                    .child("Select a layer to edit its expressions."),
            );
    };

    let prop = app.expr_editor_prop.clone();

    // Property selector chips (drive which (layer, prop) the editor targets).
    let prop_chips: Vec<gpui::AnyElement> = Prop::ALL
        .iter()
        .copied()
        .map(|p| {
            let pname = prop_key(p);
            let active = prop == pname;
            div()
                .id(("expred-prop", p as usize))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if active { rgb(BG_ACTIVE) } else { colors::surface_overlay() })
                .text_size(px(9.0))
                .text_color(if active { colors::text_primary() } else { colors::text_secondary() })
                .cursor_pointer()
                .child(p.label())
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.expr_editor_prop = pname.to_string();
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    // Current expression text for the bound (layer, prop).
    let current = app
        .expressions
        .get(&(layer_id, prop.clone()))
        .cloned()
        .unwrap_or_default();

    // Preset chips: each sets the bound property's expression.
    let preset_chips: Vec<gpui::AnyElement> = PRESETS
        .iter()
        .enumerate()
        .map(|(i, (label, expr))| {
            let expr_s = expr.to_string();
            let prop_c = prop.clone();
            div()
                .id(("expred-preset", i))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_overlay())
                .text_size(px(9.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .child(*label)
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetExpression {
                        layer_id,
                        prop: prop_c.clone(),
                        expr: expr_s.clone(),
                    });
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    // The result / error readout.
    let err = app.expr_errors.get(&(layer_id, prop.clone())).cloned();
    let result_box = if let Some(e) = err {
        div()
            .mx_2()
            .my_1()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(colors::surface_raised())
            .text_color(rgb(0xe06c75u32))
            .text_size(px(10.0))
            .child(format!("Error: {e}"))
            .into_any_element()
    } else if let Some(r) = app.last_expr_result {
        div()
            .mx_2()
            .my_1()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(colors::surface_raised())
            .text_color(rgb(0x98c379u32))
            .text_size(px(10.0))
            .child(format!("Result: {r:.4}"))
            .into_any_element()
    } else {
        div()
            .mx_2()
            .my_1()
            .px_2()
            .py_1()
            .text_color(colors::text_secondary())
            .text_size(px(9.0))
            .child("Evaluate to compute the result at the current time.")
            .into_any_element()
    };

    let prop_for_eval = prop.clone();
    let prop_for_clear = prop.clone();
    let at_time = app.time;

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        // Layer + prop selector.
        .child(
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child(format!("Layer {layer_id} · property:")),
        )
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(prop_chips))
        // Current expression text (read-only readout).
        .child(
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Expression"),
        )
        .child(
            div()
                .mx_2()
                .min_h(px(40.0))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0x161617u32))
                .text_color(rgb(EXPR_ACCENT))
                .text_size(px(11.0))
                .child(if current.is_empty() {
                    "(none — pick a preset below)".to_string()
                } else {
                    current
                }),
        )
        // Preset chips.
        .child(
            div()
                .px_3()
                .pt_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Presets"),
        )
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(preset_chips))
        // Action row: Evaluate + Clear.
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(
                    div()
                        .id("expred-eval")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(colors::accent())
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .child("Evaluate")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::EvaluateExpression {
                                layer_id,
                                prop: prop_for_eval.clone(),
                                at_time,
                            });
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("expred-clear")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .text_color(colors::text_secondary())
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .child("Clear")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::SetExpression {
                                layer_id,
                                prop: prop_for_clear.clone(),
                                expr: String::new(),
                            });
                            cx.notify();
                        })),
                ),
        )
        .child(result_box)
}

/// The expression-engine property key for a [`Prop`] (matches the names the
/// engine in `expressions.rs::prop_from_name` resolves).
fn prop_key(p: Prop) -> &'static str {
    match p {
        Prop::X => "X",
        Prop::Y => "Y",
        Prop::Scale => "Scale",
        Prop::Rotation => "Rotation",
        Prop::Opacity => "Opacity",
        Prop::AnchorX => "AnchorX",
        Prop::AnchorY => "AnchorY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prop_key_covers_every_prop_uniquely() {
        let keys: Vec<&str> = Prop::ALL.iter().copied().map(prop_key).collect();
        assert_eq!(keys.len(), 7);
        // Keys must be distinct so the (layer, prop) expression map never collides.
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "prop keys must be unique");
    }

    #[test]
    fn presets_are_non_empty_expressions() {
        assert!(!PRESETS.is_empty());
        for (label, expr) in PRESETS {
            assert!(!label.is_empty());
            assert!(!expr.trim().is_empty(), "preset expr must be a real expression");
        }
    }

    #[test]
    fn evaluating_a_preset_through_app_round_trips() {
        // Drive the real action surface: set an expression for (layer 0, X) from a
        // preset string, then evaluate it through the engine.
        let mut app = App::new();
        app.apply(Action::SetExpression {
            layer_id: 0,
            prop: "X".to_string(),
            expr: "time * 100".to_string(),
        });
        app.apply(Action::EvaluateExpression {
            layer_id: 0,
            prop: "X".to_string(),
            at_time: 2.0,
        });
        assert_eq!(app.last_expr_result, Some(200.0));
    }
}
