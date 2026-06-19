//! Composition settings inline panel (Wave 9).
//!
//! Shows a stepper UI for Width, Height, FPS, Duration (seconds). Changes are
//! staged in `App::pending_comp_settings` and only applied when "Apply" is
//! pressed (`Action::ApplyCompSettings`), so the live preview doesn't change
//! until the user confirms.

use gpui::{div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};

use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    if !app.comp_settings_open {
        return div().into_any_element();
    }

    let pending = match app.pending_comp_settings.as_ref() {
        Some(p) => p.clone(),
        None => return div().into_any_element(),
    };

    let w = pending.width;
    let h = pending.height;
    let fps = pending.fps;
    let dur = pending.duration_secs;

    let row = |label: &'static str, value: String, dec_action: Action, inc_action: Action, cx: &mut Context<Pulse>| {
        let dec = dec_action;
        let inc = inc_action;
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(
                div()
                    .w(px(60.0))
                    .text_color(colors::text_secondary())
                    .text_size(px(10.0))
                    .child(label),
            )
            .child(
                div()
                    .id((label, 0usize))
                    .w(px(20.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(rgb(BG_ACTIVE))
                    .text_color(colors::text_primary())
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .child("−")
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(dec.clone());
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .text_color(colors::text_primary())
                    .text_size(px(10.0))
                    .child(value),
            )
            .child(
                div()
                    .id((label, 1usize))
                    .w(px(20.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(rgb(BG_ACTIVE))
                    .text_color(colors::text_primary())
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .child("+")
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(inc.clone());
                        cx.notify();
                    })),
            )
    };

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .flex()
                .items_center()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .child(div().flex_1().text_size(px(11.0)).child("Comp Settings"))
                .child(
                    div()
                        .id("cs-close")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .text_size(px(10.0))
                        .text_color(colors::text_secondary())
                        .cursor_pointer()
                        .child("× Close")
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleCompSettings);
                            cx.notify();
                        })),
                ),
        )
        .child(row("Width", format!("{w}px"), Action::SetPendingCompWidth(w.saturating_sub(1).max(1)), Action::SetPendingCompWidth(w + 1), cx))
        .child(row("Height", format!("{h}px"), Action::SetPendingCompHeight(h.saturating_sub(1).max(1)), Action::SetPendingCompHeight(h + 1), cx))
        .child(row("FPS", format!("{fps:.1}"), Action::SetPendingCompFps(fps - 1.0), Action::SetPendingCompFps(fps + 1.0), cx))
        .child(row("Duration", format!("{dur:.1}s"), Action::SetPendingCompDuration(dur - 0.5), Action::SetPendingCompDuration(dur + 0.5), cx))
        .child(
            div()
                .px_3()
                .py_2()
                .child(
                    div()
                        .id("cs-apply")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(0x37c8c0))
                        .text_color(rgb(0x1e1e1e))
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .child("Apply")
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::ApplyCompSettings);
                            cx.notify();
                        })),
                ),
        )
        .into_any_element()
}
