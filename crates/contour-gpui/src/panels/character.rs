//! Character & paragraph panel (Wave 11).
//!
//! Controls: font family selector, font size stepper, font weight toggle,
//! letter-spacing (tracking), line-height (leading), and text alignment.
//! All mutations route through `Action::Set*` variants → `App::apply`.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use contour_app::text::TextAlign;

use crate::app_state::{Action, App, FontWeight};
use crate::panels::{ui_divider, ui_section_header};
use prism_ui::colors;
use crate::Contour;

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let font_family = app.font_family.clone();
    let font_weight = app.font_weight;
    let letter_spacing = app.letter_spacing;
    let line_height = app.line_height;
    let text_align = app.text_align;
    let font_size = app.default_font_size;

    // Font family presets
    const PRESETS: &[&str] = &["Helvetica", "Arial", "Georgia", "Courier", "Impact"];

    let presets_row = div()
        .flex()
        .flex_wrap()
        .gap_1()
        .px_3()
        .pb_1()
        .children(PRESETS.iter().enumerate().map(|(i, &fam)| {
            let fam_s = fam.to_string();
            let fam_s2 = fam_s.clone();
            let is_active = font_family == fam;
            let bg = if is_active { colors::accent() } else { colors::surface_raised() };
            div()
                .id(("char-preset", i))
                .px_2()
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(bg)
                .rounded_md()
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetFontFamily(fam_s.clone()));
                    cx.notify();
                }))
                .child(fam_s2)
        }));

    // Font weight buttons
    let weight_row = div()
        .flex()
        .gap_1()
        .px_3()
        .pb_2()
        .children(FontWeight::ALL.iter().enumerate().map(|(i, &w)| {
            let is_active = w == font_weight;
            let bg = if is_active { colors::accent() } else { colors::surface_raised() };
            div()
                .id(("char-weight", i))
                .px_2()
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(bg)
                .rounded_md()
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetFontWeight(w));
                    cx.notify();
                }))
                .child(w.label())
        }));

    // Text alignment buttons
    let align_row = div()
        .flex()
        .gap_1()
        .px_3()
        .pb_2()
        .children(TextAlign::ALL.iter().enumerate().map(|(i, &a)| {
            let is_active = a == text_align;
            let bg = if is_active { colors::accent() } else { colors::surface_raised() };
            div()
                .id(("char-align", i))
                .px_2()
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(bg)
                .rounded_md()
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetParaAlign(a));
                    cx.notify();
                }))
                .child(a.label())
        }));

    div()
        .flex()
        .flex_col()
        .bg(colors::surface_bg())
        .border_b_1()
        .border_color(colors::surface_border())
        .child(ui_section_header("Character"))
        .child(ui_divider())
        // Font family label + presets
        .child(
            div()
                .px_3()
                .pt_2()
                .pb_1()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(format!("Family: {font_family}")),
        )
        .child(presets_row)
        // Font size stepper
        .child(stepper_row(
            "Font Size",
            format!("{:.1} pt", font_size),
            cx,
            move |root, cx| {
                root.app.apply(Action::SetFontSize(font_size - 1.0));
                cx.notify();
            },
            move |root, cx| {
                root.app.apply(Action::SetFontSize(font_size + 1.0));
                cx.notify();
            },
        ))
        // Font weight
        .child(
            div()
                .px_3()
                .pt_1()
                .pb_1()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child("Weight"),
        )
        .child(weight_row)
        // Letter spacing stepper
        .child(stepper_row(
            "Tracking",
            format!("{:.0}", letter_spacing),
            cx,
            move |root, cx| {
                root.app.apply(Action::SetLetterSpacing(letter_spacing - 5.0));
                cx.notify();
            },
            move |root, cx| {
                root.app.apply(Action::SetLetterSpacing(letter_spacing + 5.0));
                cx.notify();
            },
        ))
        // Line height stepper
        .child(stepper_row(
            "Leading",
            format!("{:.2}×", line_height),
            cx,
            move |root, cx| {
                root.app.apply(Action::SetLineHeight(line_height - 0.1));
                cx.notify();
            },
            move |root, cx| {
                root.app.apply(Action::SetLineHeight(line_height + 0.1));
                cx.notify();
            },
        ))
        // Text alignment
        .child(
            div()
                .px_3()
                .pt_1()
                .pb_1()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child("Alignment"),
        )
        .child(align_row)
        // Variable Font axes
        .child(
            div()
                .px_3()
                .pt_2()
                .pb_1()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child("Variable Font"),
        )
        .child(variable_font_section(app, cx))
}

fn variable_font_section(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let text_params = app.selected.and_then(|i| {
        use contour_app::document::Shape;
        app.doc.shapes.get(i).and_then(|s| {
            if let Shape::Text { params, .. } = s { Some(params.clone()) } else { None }
        })
    });

    let axis_val = |tag: &str, default: f32| -> f32 {
        text_params.as_ref()
            .and_then(|p| p.font_axes.get(tag).copied())
            .unwrap_or(default)
    };

    let wght = axis_val("wght", 400.0);
    let wdth = axis_val("wdth", 100.0);
    let slnt = axis_val("slnt", 0.0);

    div()
        .flex()
        .flex_col()
        .pb_2()
        .child(stepper_row(
            "Weight",
            format!("{:.0}", wght),
            cx,
            move |root, cx| {
                root.app.apply(Action::SetFontAxis { axis: "wght".to_string(), value: (wght - 10.0).max(100.0) });
                cx.notify();
            },
            move |root, cx| {
                root.app.apply(Action::SetFontAxis { axis: "wght".to_string(), value: (wght + 10.0).min(900.0) });
                cx.notify();
            },
        ))
        .child(stepper_row(
            "Width",
            format!("{:.0}", wdth),
            cx,
            move |root, cx| {
                root.app.apply(Action::SetFontAxis { axis: "wdth".to_string(), value: (wdth - 5.0).max(50.0) });
                cx.notify();
            },
            move |root, cx| {
                root.app.apply(Action::SetFontAxis { axis: "wdth".to_string(), value: (wdth + 5.0).min(200.0) });
                cx.notify();
            },
        ))
        .child(stepper_row(
            "Slant",
            format!("{:.0}°", slnt),
            cx,
            move |root, cx| {
                root.app.apply(Action::SetFontAxis { axis: "slnt".to_string(), value: (slnt - 1.0).max(-15.0) });
                cx.notify();
            },
            move |root, cx| {
                root.app.apply(Action::SetFontAxis { axis: "slnt".to_string(), value: (slnt + 1.0).min(15.0) });
                cx.notify();
            },
        ))
}

/// A labeled stepper row (label | value | − | +).
fn stepper_row(
    label: &'static str,
    value: String,
    cx: &mut Context<Contour>,
    on_dec: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
    on_inc: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    let dec_id = (label, 0u32);
    let inc_id = (label, 1u32);
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .child(
            div()
                .flex_1()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(label),
        )
        .child(
            div()
                .min_w(px(48.0))
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child(value),
        )
        .child(
            div()
                .id(dec_id)
                .w(px(20.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(colors::surface_raised())
                .rounded_md()
                .text_color(colors::text_primary())
                .text_size(px(13.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| on_dec(root, cx)))
                .child("−"),
        )
        .child(
            div()
                .id(inc_id)
                .w(px(20.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(colors::surface_raised())
                .rounded_md()
                .text_color(colors::text_primary())
                .text_size(px(13.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| on_inc(root, cx)))
                .child("+"),
        )
}
