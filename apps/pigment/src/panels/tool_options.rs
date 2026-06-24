//! Tool-options bar — a full-width strip under the toolbar exposing the active
//! tool's parameters, replacing the hard-coded egui defaults the host shipped
//! with. The bar is contextual: paint tools (Brush/Eraser/Clone/Heal) show
//! size/hardness/opacity; Fill and Magic-Wand show tolerance + contiguous;
//! Gradient shows the dither toggle; Text shows the point size; everything else
//! shows a short hint.
//!
//! Like the rest of the host, gpui 0.2.2 has no native slider, so each numeric
//! param is a compact `−  LABEL value  +` stepper row (matching the color panel's
//! idiom). Every control emits an [`Action`] through `root.app.apply(..) +
//! cx.notify()` (the `panels/mod.rs` convention); `app` is read-only.

use gpui::{
    div, px, Context, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size, TextField};

use crate::app_state::{Action, App, HealMode, LiquifyMode, Tool};
use crate::Pigment;

pub fn render(
    app: &App,
    text_tool_field: &Entity<TextField>,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .px_3()
        .py_1()
        .h(px(34.0))
        .text_color(colors::text_primary())
        .text_size(px(font_size::SM));

    // Active-tool label on the left so the bar always reads clearly.
    row = row.child(
        div()
            .min_w(px(56.0))
            .text_color(colors::text_secondary())
            .child(app.active.label().to_string()),
    );

    match app.active {
        Tool::Brush | Tool::Eraser | Tool::Clone => {
            row = row
                .child(stepper(
                    "brush-size",
                    "Size",
                    format!("{:.0}", app.brush.size),
                    Action::SetBrushSize(app.brush.size - 2.0),
                    Action::SetBrushSize(app.brush.size + 2.0),
                    cx,
                ))
                .child(stepper(
                    "brush-hard",
                    "Hardness",
                    format!("{:.0}%", app.brush.hardness * 100.0),
                    Action::SetBrushHardness(app.brush.hardness - 0.05),
                    Action::SetBrushHardness(app.brush.hardness + 0.05),
                    cx,
                ))
                .child(stepper(
                    "brush-op",
                    "Opacity",
                    format!("{:.0}%", app.brush.opacity * 100.0),
                    Action::SetBrushOpacity(app.brush.opacity - 0.05),
                    Action::SetBrushOpacity(app.brush.opacity + 0.05),
                    cx,
                ));
        }
        Tool::Heal => {
            let heal_r = app.heal_radius;
            let heal_mode = app.heal_mode;
            let mode_btn = |key: &'static str, label: &'static str, mode: HealMode, cx: &mut Context<Pigment>| {
                let is_active = heal_mode == mode;
                div()
                    .id(SharedString::from(format!("heal-mode-{key}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(font_size::XS))
                    .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
                    .border_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::SetHealMode(mode));
                        cx.notify();
                    }))
                    .child(label)
            };
            let btn_normal = mode_btn("normal", "Normal", HealMode::Normal, cx);
            let btn_replace = mode_btn("replace", "Replace", HealMode::Replace, cx);
            let btn_content = mode_btn("content", "Content", HealMode::Content, cx);
            row = row
                .child(stepper(
                    "heal-radius",
                    "Radius",
                    format!("{heal_r}px"),
                    Action::SetHealRadius(heal_r.saturating_sub(2)),
                    Action::SetHealRadius(heal_r + 2),
                    cx,
                ))
                .child(div().text_color(colors::text_secondary()).text_size(px(font_size::XS)).child("Mode"))
                .child(btn_normal)
                .child(btn_replace)
                .child(btn_content);
        }
        Tool::Fill | Tool::MagicWand => {
            row = row
                .child(stepper(
                    "fill-tol",
                    "Tolerance",
                    format!("{:.0}%", app.fill_tolerance * 100.0),
                    Action::SetFillTolerance(app.fill_tolerance - 0.02),
                    Action::SetFillTolerance(app.fill_tolerance + 0.02),
                    cx,
                ))
                .child(toggle(
                    "fill-contig",
                    "Contiguous",
                    app.fill_contiguous,
                    Action::ToggleFillContiguous,
                    cx,
                ));
        }
        Tool::Gradient => {
            row = row.child(toggle(
                "grad-dither",
                "Dither",
                app.gradient_dither,
                Action::ToggleGradientDither,
                cx,
            ));
        }
        Tool::Text => {
            row = row.child(stepper(
                "text-size",
                "Size",
                format!("{:.0}px", app.text_size),
                Action::SetTextSize(app.text_size - 2.0),
                Action::SetTextSize(app.text_size + 2.0),
                cx,
            ));
            // While a text run is being placed, show a real editable field whose
            // content is the run's string. `on_change` (wired at creation in
            // main.rs) pushes the full string into the layer via SetTextContent,
            // so the user types real text onto the canvas. Click the canvas with
            // the Text tool first to start a run.
            if app.text_editing() {
                row = row
                    .child(
                        div()
                            .text_color(colors::text_secondary())
                            .text_size(px(font_size::XS))
                            .child("Text"),
                    )
                    .child(div().w(px(280.0)).child(text_tool_field.clone()));
            } else {
                row = row.child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child("Click the canvas to start typing"),
                );
            }
        }
        Tool::Dodge | Tool::Burn => {
            let is_dodge = app.active == Tool::Dodge;
            let strength_label = if is_dodge { "Exposure" } else { "Exposure" };
            let strength_hint = if is_dodge { "(dodge)" } else { "(burn)" };
            row = row
                .child(stepper(
                    "dodge-size",
                    "Size",
                    format!("{:.0}px", app.dodge_size),
                    Action::SetDodgeSize(app.dodge_size - 2.0),
                    Action::SetDodgeSize(app.dodge_size + 2.0),
                    cx,
                ))
                .child(stepper(
                    "dodge-strength",
                    strength_label,
                    format!("{:.0}%", app.dodge_strength * 100.0),
                    Action::SetDodgeStrength(app.dodge_strength - 0.05),
                    Action::SetDodgeStrength(app.dodge_strength + 0.05),
                    cx,
                ))
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child(strength_hint),
                );
        }
        Tool::Smudge => {
            row = row
                .child(stepper(
                    "smudge-size",
                    "Size",
                    format!("{:.0}px", app.dodge_size),
                    Action::SetDodgeSize(app.dodge_size - 2.0),
                    Action::SetDodgeSize(app.dodge_size + 2.0),
                    cx,
                ))
                .child(stepper(
                    "smudge-strength",
                    "Strength",
                    format!("{:.0}%", app.smudge_strength * 100.0),
                    Action::SetSmudgeStrength(app.smudge_strength - 0.05),
                    Action::SetSmudgeStrength(app.smudge_strength + 0.05),
                    cx,
                ));
        }
        Tool::Transform => {
            let tx = app.xform_translate;
            let sc = app.xform_scale;
            row = row
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child(format!(
                            "X {:.0}  Y {:.0}  Scale {:.0}%",
                            tx[0], tx[1], sc * 100.0
                        )),
                )
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child("Drag to translate · Shift+drag to scale"),
                );
        }
        Tool::Crop => {
            let crop_info = if let Some(r) = app.crop_rect {
                let w = (r[2] - r[0]).abs();
                let h = (r[3] - r[1]).abs();
                format!("{:.0} × {:.0} px", w, h)
            } else {
                "Drag to define crop region".to_string()
            };
            row = row.child(
                div()
                    .text_color(colors::text_secondary())
                    .child(crop_info),
            );
        }
        Tool::Liquify => {
            let liq_mode = app.liquify_mode;
            let mode_btn = |key: &'static str, label: &'static str, mode: LiquifyMode, cx: &mut Context<Pigment>| {
                let is_active = liq_mode == mode;
                div()
                    .id(SharedString::from(format!("liq-mode-{key}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(font_size::XS))
                    .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
                    .border_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::SetLiquifyMode(mode));
                        cx.notify();
                    }))
                    .child(label)
            };
            let btn_warp   = mode_btn("warp",   "Warp",   LiquifyMode::Warp,   cx);
            let btn_twirl  = mode_btn("twirl",  "Twirl",  LiquifyMode::Twirl,  cx);
            let btn_pucker = mode_btn("pucker", "Pucker", LiquifyMode::Pucker, cx);
            let btn_bloat  = mode_btn("bloat",  "Bloat",  LiquifyMode::Bloat,  cx);
            row = row
                .child(stepper(
                    "liq-size",
                    "Size",
                    format!("{:.0}px", app.dodge_size),
                    Action::SetDodgeSize(app.dodge_size - 2.0),
                    Action::SetDodgeSize(app.dodge_size + 2.0),
                    cx,
                ))
                .child(div().text_color(colors::text_secondary()).text_size(px(font_size::XS)).child("Mode"))
                .child(btn_warp)
                .child(btn_twirl)
                .child(btn_pucker)
                .child(btn_bloat);
        }
        Tool::SelectRect | Tool::SelectEllipse | Tool::Lasso => {
            row = row.child(
                div()
                    .text_color(colors::text_secondary())
                    .child("Shift adds · Alt subtracts · Shift+Alt intersects"),
            );
        }
        _ => {
            row = row.child(
                div()
                    .text_color(colors::text_secondary())
                    .child("No options for this tool"),
            );
        }
    }

    row
}

/// A `−  LABEL value  +` stepper. The decrement/increment carry their own
/// pre-computed [`Action`]s (clamping happens in `App::apply`).
fn stepper(
    key: &'static str,
    label: &'static str,
    value: String,
    dec: Action,
    inc: Action,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let btn = |suffix: &'static str, glyph: &'static str, action: Action, cx: &mut Context<Pigment>| {
        div()
            .id(SharedString::from(format!("{key}-{suffix}")))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_overlay())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(action.clone());
                cx.notify();
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(div().text_color(colors::text_secondary()).child(label))
        .child(btn("opt-dec", "−", dec, cx))
        .child(
            div()
                .min_w(px(40.0))
                .flex()
                .justify_center()
                .child(value),
        )
        .child(btn("opt-inc", "+", inc, cx))
}

/// A `LABEL [on/off]` checkbox-style toggle.
fn toggle(
    key: &'static str,
    label: &'static str,
    on: bool,
    action: Action,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let bg = if on {
        colors::tool_active()
    } else {
        colors::surface_overlay()
    };
    let txt = if on { "On" } else { "Off" };
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(div().text_color(colors::text_secondary()).child(label))
        .child(
            div()
                .id(SharedString::from(format!("opt-toggle-{key}")))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(bg)
                .border_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .text_size(px(font_size::XS))
                .cursor_pointer()
                .hover(|s| s.bg(colors::tool_hover()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(action.clone());
                    cx.notify();
                }))
                .child(txt),
        )
}
