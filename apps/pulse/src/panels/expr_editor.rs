//! Expression editor pop-out — a focused panel bound to one property's
//! expression on the **selected** layer.
//!
//! Real typing: the editor now hosts a focusable [`prism_ui::TextField`] so the
//! user can type ANY rhai expression. Pressing Enter (the field's `on_submit`,
//! wired in `main.rs`) sets the expression for the bound `(layer, prop)` and
//! evaluates it; the "Evaluate" button does the same on demand. The preset chips
//! remain as quick-insert helpers — clicking one fills the text field and sets
//! the expression — but free-text entry is the primary path.
//!
//! A property selector switches which `(layer, prop)` the editor targets (stored
//! in `app.expr_editor_prop`), and the scalar result / any error from
//! `app.last_expr_result` / `app.expr_errors` is displayed.
//!
//! Reads `&App` + the shared `expr_field` entity, emits
//! [`Action`](crate::app_state::Action)s. Gated by `app.expr_editor_open`.

use gpui::{
    div, px, rgb, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, section_header, TextField};

use crate::app_state::{Action, App};
use crate::comp::Prop;
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// Accent for the expression text (matches the `expressions` panel palette).
const EXPR_ACCENT: u32 = 0xe5c07b;

/// Curated preset expressions (label, expression string) covering the common AE
/// idioms the rhai engine in `crate::comp::expr` supports. These are now
/// quick-insert helpers: clicking one fills the editable field and applies it.
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

pub fn render(app: &App, expr_field: &Entity<TextField>, cx: &mut Context<Pulse>) -> impl IntoElement {
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
            )
            .into_any_element();
    };

    let prop = app.expr_editor_prop.clone();

    // Property selector chips (drive which (layer, prop) the editor targets).
    // Switching properties syncs the text field to the new property's current
    // expression so the editable text always reflects the bound target.
    let prop_chips: Vec<gpui::AnyElement> = Prop::ALL
        .iter()
        .copied()
        .map(|p| {
            let pname = prop_key(p);
            let active = prop == pname;
            let synced_text = app
                .expressions
                .get(&(layer_id, pname.to_string()))
                .cloned()
                .unwrap_or_default();
            let field = expr_field.clone();
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
                .on_click(cx.listener(move |root, _ev, win, cx| {
                    root.app.expr_editor_prop = pname.to_string();
                    field.update(cx, |f, cx| f.set_text(synced_text.clone(), win, cx));
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    // Preset chips: each fills the editable field AND applies the expression so
    // the user sees the text appear and can keep editing it.
    let preset_chips: Vec<gpui::AnyElement> = PRESETS
        .iter()
        .enumerate()
        .map(|(i, (label, expr))| {
            let expr_s = expr.to_string();
            let prop_c = prop.clone();
            let field = expr_field.clone();
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
                .on_click(cx.listener(move |root, _ev, win, cx| {
                    let expr_s = expr_s.clone();
                    field.update(cx, |f, cx| f.set_text(expr_s.clone(), win, cx));
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
            .child("Type an expression and press Enter (or click Evaluate).")
            .into_any_element()
    };

    let prop_for_eval = prop.clone();
    let prop_for_clear = prop.clone();
    let at_time = app.time;
    let eval_field = expr_field.clone();
    let clear_field = expr_field.clone();

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
        // Editable expression text field (REAL typing).
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
                .my_1()
                .child(expr_field.clone()),
        )
        // Preset quick-insert chips.
        .child(
            div()
                .px_3()
                .pt_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Presets (quick insert)"),
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
                            // Commit whatever is currently typed, then evaluate.
                            let typed = eval_field.read(cx).text().to_string();
                            root.app.apply(Action::SetExpression {
                                layer_id,
                                prop: prop_for_eval.clone(),
                                expr: typed,
                            });
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
                        .on_click(cx.listener(move |root, _ev, win, cx| {
                            clear_field.update(cx, |f, cx| f.clear(win, cx));
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
        .into_any_element()
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
    fn typed_expression_through_app_round_trips() {
        // Drive the real action surface the way the TextField's on_submit does:
        // set a free-typed expression for (layer 0, X), then evaluate it.
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

    #[test]
    fn arbitrary_free_text_expression_evaluates() {
        // A non-preset expression a user could type by hand.
        let mut app = App::new();
        app.apply(Action::SetExpression {
            layer_id: 0,
            prop: "Y".to_string(),
            expr: "time * 50 + 10".to_string(),
        });
        app.apply(Action::EvaluateExpression {
            layer_id: 0,
            prop: "Y".to_string(),
            at_time: 2.0,
        });
        assert_eq!(app.last_expr_result, Some(110.0));
    }
}
