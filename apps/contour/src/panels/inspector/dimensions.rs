//! Inspector — stroke-width and opacity rows. Split out of `inspector.rs`.

use super::*;

/// Stroke-width stepper row: `−  Stroke W  value  ＋`, clamped ≥ 0.
pub(super) fn width_row(w: f32, cx: &mut Context<Contour>) -> impl IntoElement {
    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        div()
            .id((suffix, 0u64))
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
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                if let Some(s) = root.app.selected_shape() {
                    let next = (s.stroke_width() + delta).max(0.0);
                    root.app.apply(Action::SetStrokeWidth(next));
                    cx.notify();
                }
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(56.0))
                .text_color(colors::text_secondary())
                .child("Stroke W"),
        )
        .child(mk("sw-dec", "−", -WIDTH_STEP))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{w:.1}")),
        )
        .child(mk("sw-inc", "＋", WIDTH_STEP))
}

/// Opacity stepper row: `−  Opacity  value  ＋`, clamped [0,1]. Mirrors the fill
/// alpha (see module docs).
pub(super) fn opacity_row(o: f32, cx: &mut Context<Contour>) -> impl IntoElement {
    let pct = (o.clamp(0.0, 1.0) * 100.0).round() as u32;

    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        div()
            .id((suffix, 0u64))
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
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                if let Some(cur) = root.app.selected_fill() {
                    let next = (cur[3] + delta).clamp(0.0, 1.0);
                    root.app.apply(Action::SetOpacity(next));
                    cx.notify();
                }
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(56.0)).text_color(colors::text_secondary()).child("Opacity"))
        .child(mk("op-dec", "−", -OPACITY_STEP))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{pct}%")),
        )
        .child(mk("op-inc", "＋", OPACITY_STEP))
}
