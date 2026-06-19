//! 3-way color wheels panel section: Lift / Gamma / Gain R/G/B steppers.
//!
//! Rendered as a section inside the inspector when a visual clip is selected.
//! The "wheels" are shown as R/G/B steppers for each of the three wheels —
//! simpler than actual circular UI but functionally equivalent for Batch 3.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App};
use crate::Reel;

/// Render the Color Wheels section for the inspector panel.
/// Returns a div element containing the Lift / Gamma / Gain steppers.
pub fn render_section(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let cw = app.color_wheels;

    // Pre-build all stepper rows into a Vec before the div chain.
    let mut rows: Vec<gpui::AnyElement> = Vec::new();

    rows.push(section_header("Color Wheels").into_any_element());
    rows.push(divider().into_any_element());

    // Lift column label
    rows.push(
        div()
            .px_3()
            .py_1()
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .child("Lift (Shadows)")
            .into_any_element(),
    );

    // Lift R, G, B steppers
    for ch in 0..3usize {
        let ch_label = ["R", "G", "B"][ch];
        let val = cw.lift[ch];
        rows.push(cw_stepper(
            &format!("cw-lift-{ch}"),
            ch_label,
            format!("{:.2}", val),
            cx,
            move |root, _ev, _win, cx| {
                let mut new_cw = root.app.color_wheels;
                new_cw.lift[ch] = (new_cw.lift[ch] - 0.01).clamp(-1.0, 1.0);
                root.app.apply(Action::SetColorWheels(new_cw));
                cx.notify();
            },
            move |root, _ev, _win, cx| {
                let mut new_cw = root.app.color_wheels;
                new_cw.lift[ch] = (new_cw.lift[ch] + 0.01).clamp(-1.0, 1.0);
                root.app.apply(Action::SetColorWheels(new_cw));
                cx.notify();
            },
        ).into_any_element());
    }

    // Gamma column label
    rows.push(
        div()
            .px_3()
            .py_1()
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .child("Gamma (Mids)")
            .into_any_element(),
    );

    // Gamma R, G, B steppers
    for ch in 0..3usize {
        let ch_label = ["R", "G", "B"][ch];
        let val = cw.gamma[ch];
        rows.push(cw_stepper(
            &format!("cw-gamma-{ch}"),
            ch_label,
            format!("{:.2}", val),
            cx,
            move |root, _ev, _win, cx| {
                let mut new_cw = root.app.color_wheels;
                new_cw.gamma[ch] = (new_cw.gamma[ch] - 0.05).max(0.01);
                root.app.apply(Action::SetColorWheels(new_cw));
                cx.notify();
            },
            move |root, _ev, _win, cx| {
                let mut new_cw = root.app.color_wheels;
                new_cw.gamma[ch] = (new_cw.gamma[ch] + 0.05).min(4.0);
                root.app.apply(Action::SetColorWheels(new_cw));
                cx.notify();
            },
        ).into_any_element());
    }

    // Gain column label
    rows.push(
        div()
            .px_3()
            .py_1()
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .child("Gain (Highlights)")
            .into_any_element(),
    );

    // Gain R, G, B steppers
    for ch in 0..3usize {
        let ch_label = ["R", "G", "B"][ch];
        let val = cw.gain[ch];
        rows.push(cw_stepper(
            &format!("cw-gain-{ch}"),
            ch_label,
            format!("{:.2}", val),
            cx,
            move |root, _ev, _win, cx| {
                let mut new_cw = root.app.color_wheels;
                new_cw.gain[ch] = (new_cw.gain[ch] - 0.05).max(0.0);
                root.app.apply(Action::SetColorWheels(new_cw));
                cx.notify();
            },
            move |root, _ev, _win, cx| {
                let mut new_cw = root.app.color_wheels;
                new_cw.gain[ch] = (new_cw.gain[ch] + 0.05).min(4.0);
                root.app.apply(Action::SetColorWheels(new_cw));
                cx.notify();
            },
        ).into_any_element());
    }

    div()
        .flex()
        .flex_col()
        .children(rows)
}

/// A labeled stepper row (−/value/+) for a color wheel channel.
fn cw_stepper(
    id: &str,
    label: &'static str,
    value: String,
    cx: &mut Context<Reel>,
    dec: impl Fn(&mut Reel, &gpui::ClickEvent, &mut gpui::Window, &mut Context<Reel>) + 'static,
    inc: impl Fn(&mut Reel, &gpui::ClickEvent, &mut gpui::Window, &mut Context<Reel>) + 'static,
) -> impl IntoElement {
    let dec_id = gpui::SharedString::from(format!("{id}-dec"));
    let inc_id = gpui::SharedString::from(format!("{id}-inc"));
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_1()
        .gap_2()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id(dec_id)
                        .w(px(20.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .on_click(cx.listener(dec))
                        .child("\u{2212}"),
                )
                .child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded_sm()
                        .bg(colors::surface_overlay())
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .min_w(px(40.0))
                        .child(value),
                )
                .child(
                    div()
                        .id(inc_id)
                        .w(px(20.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .on_click(cx.listener(inc))
                        .child("+"),
                ),
        )
}
