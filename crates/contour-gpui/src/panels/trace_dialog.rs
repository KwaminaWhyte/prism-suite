//! Image Trace inline panel — appears in the right dock when `App::trace_panel_open`.
//!
//! Provides:
//! - Threshold slider (0–255) via `−/＋` steppers
//! - Colors count stepper (2–32)
//! - "Trace" button → `Action::TraceImage` (stub: logs params, marks host dirty)
//! - "✕" close button → `Action::CloseTracePanel`
//!
//! The panel shows only when `app.trace_panel_open`; the toolbar "Image Trace"
//! button (visible when any shape is selected) sets `Action::OpenTracePanel`.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App};
use crate::panels::ACCENT;
use prism_ui::colors;
use crate::Contour;

/// Returns `None` when the panel is closed (the root view skips it with
/// `.children(trace_opt)`).
pub fn render(app: &App, cx: &mut Context<Contour>) -> Option<impl IntoElement> {
    if !app.trace_panel_open {
        return None;
    }

    let threshold = app.trace_threshold;
    let colors = app.trace_colors;
    let shape_id = app.selected.unwrap_or(0);

    // ---- Threshold stepper (0–255, step 5) --------------------------------
    let th_dec = step_btn("tr-th-dec", "−", cx, move |root, cx| {
        let next = root.app.trace_threshold.saturating_sub(5);
        root.app.apply(Action::SetTraceThreshold(next));
        cx.notify();
    });
    let th_inc = step_btn("tr-th-inc", "＋", cx, move |root, cx| {
        let next = root.app.trace_threshold.saturating_add(5);
        root.app.apply(Action::SetTraceThreshold(next));
        cx.notify();
    });
    let threshold_row = param_row("Threshold", format!("{threshold}"), th_dec, th_inc);

    // ---- Colors stepper (2–32, step 1) ------------------------------------
    let co_dec = step_btn("tr-co-dec", "−", cx, move |root, cx| {
        let next = root.app.trace_colors.saturating_sub(1).max(2);
        root.app.apply(Action::SetTraceColors(next));
        cx.notify();
    });
    let co_inc = step_btn("tr-co-inc", "＋", cx, move |root, cx| {
        let next = root.app.trace_colors.saturating_add(1).min(32);
        root.app.apply(Action::SetTraceColors(next));
        cx.notify();
    });
    let colors_row = param_row("Colors", format!("{colors}"), co_dec, co_inc);

    // ---- Trace button ------------------------------------------------------
    let trace_btn = div()
        .id("tr-trace")
        .w_full()
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(rgb(ACCENT))
        .text_color(rgb(0xffffff))
        .text_size(px(12.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::TraceImage {
                shape_id,
                threshold: root.app.trace_threshold,
                colors: root.app.trace_colors,
            });
            cx.notify();
        }))
        .child("Trace");

    // ---- Close button ------------------------------------------------------
    let close_btn = div()
        .id("tr-close")
        .px_2()
        .h(px(20.0))
        .flex()
        .items_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_secondary())
        .text_size(px(11.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::CloseTracePanel);
            cx.notify();
        }))
        .child("✕");

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(div().flex_1().text_color(colors::text_primary()).child("Image Trace"))
        .child(close_btn);

    Some(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(colors::surface_bg())
            .border_b_1()
            .border_color(colors::surface_border())
            .child(header)
            .child(threshold_row)
            .child(colors_row)
            .child(trace_btn),
    )
}

fn step_btn(
    id: &'static str,
    glyph: &'static str,
    cx: &mut Context<Contour>,
    on_click: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)))
        .child(glyph)
}

fn param_row(
    label: &'static str,
    value: String,
    dec: impl IntoElement,
    inc: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(60.0)).text_color(colors::text_secondary()).text_size(px(11.0)).child(label))
        .child(dec)
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child(value),
        )
        .child(inc)
}
