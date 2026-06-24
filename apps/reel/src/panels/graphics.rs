//! Essential Graphics panel — the Motion Graphics Template (.mogrt) library.
//!
//! Lists every template in `app.mogr_templates`, lets the user select one
//! ([`Action::SetActiveMogr`]), edit the active template's exposed parameters,
//! and instantiate it onto the selected clip ([`Action::ApplyMogrToClip`]).
//! Parameter editing follows the codebase's click-only convention: numbers use
//! +/- steppers ([`Action::SetMogrParamNumber`]); colours cycle through a small
//! palette ([`Action::SetMogrParamColor`]); booleans toggle; text params show
//! their value and seed a placeholder ([`Action::SetMogrParamText`]) since the
//! host has no inline text-input widget yet.
//!
//! A toolbar "Graphics" button toggles `app.mogr_library_open`
//! ([`Action::ToggleMogrLibrary`]); this panel renders only when that is set.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App, MogrParam, MogrParamValue, MogrTemplate};
use crate::Reel;

/// A small palette colours cycle through when clicked.
const PALETTE: [[f32; 4]; 6] = [
    [1.0, 1.0, 1.0, 1.0],
    [1.0, 0.2, 0.2, 1.0],
    [0.2, 0.8, 0.4, 1.0],
    [0.3, 0.5, 1.0, 1.0],
    [1.0, 0.8, 0.2, 1.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn next_palette(c: [f32; 4]) -> [f32; 4] {
    let idx = PALETTE
        .iter()
        .position(|p| (p[0] - c[0]).abs() < 0.02 && (p[1] - c[1]).abs() < 0.02 && (p[2] - c[2]).abs() < 0.02)
        .unwrap_or(usize::MAX);
    PALETTE[(idx.wrapping_add(1)) % PALETTE.len()]
}

/// A starter template added by the "+ New Template" button: a lower-third with
/// a couple of exposed params so the param editor has something to drive.
fn starter_template(n: usize) -> MogrTemplate {
    MogrTemplate {
        name: format!("Lower Third {n}"),
        file_path: std::path::PathBuf::new(),
        duration_frames: 150,
        params: vec![
            MogrParam { name: "Title".into(), value: MogrParamValue::Text("Title".into()) },
            MogrParam { name: "Size".into(), value: MogrParamValue::Number(48.0) },
            MogrParam { name: "Color".into(), value: MogrParamValue::Color([1.0, 1.0, 1.0, 1.0]) },
            MogrParam { name: "Shadow".into(), value: MogrParamValue::Boolean(true) },
        ],
    }
}

fn template_row(
    idx: usize,
    tmpl: &MogrTemplate,
    active: bool,
    selected_clip: Option<usize>,
    cx: &mut Context<Reel>,
) -> gpui::AnyElement {
    let name = tmpl.name.clone();
    let pcount = tmpl.params.len();

    let apply_btn = div()
        .id(gpui::SharedString::from(format!("mogr-apply-{idx}")))
        .px(px(6.0))
        .py(px(2.0))
        .rounded_sm()
        .bg(if selected_clip.is_some() { colors::accent() } else { colors::surface_overlay() })
        .text_color(if selected_clip.is_some() { colors::text_primary() } else { colors::text_disabled() })
        .text_size(px(9.0))
        .when(selected_clip.is_some(), |el| {
            let clip_idx = selected_clip.unwrap();
            el.cursor_pointer().on_click(cx.listener(move |root, _e, _w, cx| {
                root.app.apply(Action::ApplyMogrToClip { clip_idx, template_idx: idx });
                cx.notify();
            }))
        })
        .child("Apply");

    let del_btn = div()
        .id(gpui::SharedString::from(format!("mogr-del-{idx}")))
        .px(px(6.0))
        .py(px(2.0))
        .rounded_sm()
        .bg(colors::surface_raised())
        .text_color(colors::text_secondary())
        .text_size(px(9.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _e, _w, cx| {
            root.app.apply(Action::RemoveMogrTemplate(idx));
            cx.notify();
        }))
        .child("Del");

    div()
        .id(gpui::SharedString::from(format!("mogr-row-{idx}")))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .py(px(4.0))
        .gap(px(4.0))
        .border_b_1()
        .border_color(colors::surface_border())
        .bg(if active { colors::surface_overlay() } else { colors::surface_raised() })
        .cursor_pointer()
        .on_click(cx.listener(move |root, _e, _w, cx| {
            root.app.apply(Action::SetActiveMogr(Some(idx)));
            cx.notify();
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().text_color(colors::text_primary()).text_size(px(11.0)).child(name))
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(9.0))
                        .child(format!("{pcount} params")),
                ),
        )
        .child(div().flex().flex_row().gap(px(3.0)).child(apply_btn).child(del_btn))
        .into_any_element()
}

fn param_row(
    template_idx: usize,
    param_idx: usize,
    param: &MogrParam,
    cx: &mut Context<Reel>,
) -> gpui::AnyElement {
    let name = param.name.clone();
    let control: gpui::AnyElement = match &param.value {
        MogrParamValue::Text(t) => {
            let shown = if t.is_empty() { "(empty)".to_string() } else { t.clone() };
            let seed = format!("{name} text");
            // Click seeds/cycles a placeholder so the action is exercised.
            div()
                .id(gpui::SharedString::from(format!("mp-text-{template_idx}-{param_idx}")))
                .px(px(6.0))
                .py(px(2.0))
                .min_w(px(96.0))
                .rounded_sm()
                .bg(colors::surface_overlay())
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .hover(|s| s.bg(colors::tool_hover()))
                .on_click(cx.listener(move |root, _e, _w, cx| {
                    root.app.apply(Action::SetMogrParamText {
                        template_idx, param_idx, text: seed.clone(),
                    });
                    cx.notify();
                }))
                .child(shown)
                .into_any_element()
        }
        MogrParamValue::Number(v) => {
            let v = *v;
            let dec = Action::SetMogrParamNumber { template_idx, param_idx, value: v - 1.0 };
            let inc = Action::SetMogrParamNumber { template_idx, param_idx, value: v + 1.0 };
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(3.0))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("mp-num-dec-{template_idx}-{param_idx}")))
                        .w(px(18.0)).h(px(16.0))
                        .flex().items_center().justify_center()
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_primary()).text_size(px(11.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _e, _w, cx| {
                            root.app.apply(dec.clone());
                            cx.notify();
                        }))
                        .child("\u{2212}"),
                )
                .child(
                    div()
                        .px(px(6.0)).py(px(1.0))
                        .min_w(px(40.0))
                        .rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0))
                        .child(format!("{v:.0}")),
                )
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("mp-num-inc-{template_idx}-{param_idx}")))
                        .w(px(18.0)).h(px(16.0))
                        .flex().items_center().justify_center()
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_primary()).text_size(px(11.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _e, _w, cx| {
                            root.app.apply(inc.clone());
                            cx.notify();
                        }))
                        .child("+"),
                )
                .into_any_element()
        }
        MogrParamValue::Color(c) => {
            let c = *c;
            let swatch = gpui::Rgba { r: c[0], g: c[1], b: c[2], a: 1.0 };
            div()
                .id(gpui::SharedString::from(format!("mp-color-{template_idx}-{param_idx}")))
                .w(px(28.0)).h(px(16.0))
                .rounded_sm()
                .bg(swatch)
                .border_1().border_color(colors::surface_border())
                .cursor_pointer()
                .on_click(cx.listener(move |root, _e, _w, cx| {
                    root.app.apply(Action::SetMogrParamColor {
                        template_idx, param_idx, color: next_palette(c),
                    });
                    cx.notify();
                }))
                .into_any_element()
        }
        MogrParamValue::Boolean(b) => {
            // No `SetMogrParamBoolean` action exists, so this is a read-only
            // state badge rather than an interactive toggle.
            let b = *b;
            div()
                .px(px(8.0)).py(px(2.0))
                .rounded_sm()
                .bg(if b { colors::success() } else { colors::surface_overlay() })
                .text_color(colors::text_primary()).text_size(px(9.0))
                .child(if b { "On" } else { "Off" })
                .into_any_element()
        }
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .py(px(3.0))
        .child(div().text_color(colors::text_secondary()).text_size(px(10.0)).child(name))
        .child(control)
        .into_any_element()
}

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let active = app.active_mogr;
    let selected_clip = app.selected;

    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    for (i, t) in app.mogr_templates.iter().enumerate() {
        rows.push(template_row(i, t, active == Some(i), selected_clip, cx));
    }
    if rows.is_empty() {
        rows.push(
            div()
                .px(px(8.0)).py(px(6.0))
                .text_color(colors::text_disabled()).text_size(px(10.0))
                .child("No templates. Add one below.")
                .into_any_element(),
        );
    }

    // Active template's parameter editor.
    let mut param_rows: Vec<gpui::AnyElement> = Vec::new();
    if let Some(ai) = active {
        if let Some(t) = app.mogr_templates.get(ai) {
            for (pi, p) in t.params.iter().enumerate() {
                param_rows.push(param_row(ai, pi, p, cx));
            }
        }
    }

    let add_btn = div()
        .id("mogr-add")
        .mx(px(8.0)).my(px(6.0))
        .px(px(8.0)).py(px(3.0))
        .rounded_sm()
        .bg(colors::accent())
        .text_color(colors::text_primary()).text_size(px(10.0))
        .cursor_pointer()
        .hover(|s| s.bg(colors::accent_hover()))
        .on_click(cx.listener(move |root, _e, _w, cx| {
            let n = root.app.mogr_templates.len() + 1;
            root.app.apply(Action::AddMogrTemplate(starter_template(n)));
            cx.notify();
        }))
        .child("+ New Template");

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Essential Graphics"))
        .child(divider())
        .children(rows)
        .child(add_btn)
        .when(!param_rows.is_empty(), |el| {
            el.child(divider())
                .child(
                    div()
                        .px(px(8.0)).py(px(3.0))
                        .text_color(colors::text_disabled()).text_size(px(9.0))
                        .child("PARAMETERS"),
                )
                .children(param_rows)
        })
}
