//! Render-status bar + proxy status — a thin strip above the timeline.
//!
//! Premiere paints a coloured render bar over the time ruler showing which
//! sections have a cached preview render (green = rendered, yellow = will need
//! render). Reel models the previewable span with the work-area in/out points;
//! this bar draws the full sequence span and fills the work-area sub-range as
//! the "rendered" region, with the playhead overlaid.
//!
//! On the right it surfaces the proxy workflow: a toggle that switches program
//! playback to attached proxies ([`Action::ToggleProxyPlayback`], reading
//! `app.toggle_proxy_enabled`), a button to open the proxy-ingest panel
//! ([`Action::ToggleProxyIngestPanel`]), and a live count of how many clip
//! proxies are queued vs. attached from `app.clip_proxies`.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::Reel;

pub const RENDER_BAR_H: f32 = 22.0;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let dur = app.project.duration.max(0.001);
    let playhead = (app.time / dur).clamp(0.0, 1.0);

    // Work-area span → fraction of the sequence; this is the "rendered"
    // (preview-cached) region the bar fills green.
    let wa_in = (app.work_area_in / dur).clamp(0.0, 1.0);
    let wa_out = (app.work_area_out / dur).clamp(0.0, 1.0);
    let (wa_lo, wa_hi) = if wa_out > wa_in { (wa_in, wa_out) } else { (0.0, 0.0) };
    let rendered_frac = (wa_hi - wa_lo).max(0.0);

    // Proxy status.
    let proxy_on = app.toggle_proxy_enabled;
    let proxy_count = app.clip_proxies.len();
    let proxy_attached = app.clip_proxies.iter().filter(|p| p.attached).count();

    // The render-track lane: a full-width strip with a green sub-range showing
    // the rendered (work-area) region, plus the playhead tick.
    let render_track = div()
        .id("render-track")
        .relative()
        .flex_1()
        .h(px(8.0))
        .rounded_sm()
        .bg(colors::surface_overlay())
        .border_1()
        .border_color(colors::surface_border())
        .overflow_hidden()
        // Rendered region (work area).
        .when(rendered_frac > 0.0, |el| {
            el.child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .bottom(px(0.0))
                    .left(gpui::relative(wa_lo))
                    .w(gpui::relative(rendered_frac))
                    .bg(colors::success()),
            )
        })
        // Playhead tick.
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .left(gpui::relative(playhead))
                .w(px(2.0))
                .bg(colors::accent()),
        );

    // Proxy playback toggle.
    let proxy_toggle = div()
        .id("proxy-toggle")
        .px(px(8.0))
        .py(px(2.0))
        .rounded_sm()
        .cursor_pointer()
        .bg(if proxy_on { colors::accent() } else { colors::surface_overlay() })
        .text_color(if proxy_on { colors::text_primary() } else { colors::text_secondary() })
        .text_size(px(10.0))
        .hover(|s| s.bg(if proxy_on { colors::accent_hover() } else { colors::tool_hover() }))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleProxyPlayback);
            cx.notify();
        }))
        .child(if proxy_on { "Proxy: On" } else { "Proxy: Off" });

    // Proxy ingest panel toggle.
    let ingest_btn = div()
        .id("proxy-ingest")
        .px(px(8.0))
        .py(px(2.0))
        .rounded_sm()
        .cursor_pointer()
        .bg(if app.proxy_ingest_open { colors::accent() } else { colors::surface_overlay() })
        .text_color(colors::text_secondary())
        .text_size(px(10.0))
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleProxyIngestPanel);
            cx.notify();
        }))
        .child("Ingest\u{2026}");

    // Proxy queue / attached count.
    let queue_label = div()
        .text_color(colors::text_secondary())
        .text_size(px(9.0))
        .child(format!("{proxy_attached}/{proxy_count} proxies"));

    div()
        .w_full()
        .h(px(RENDER_BAR_H))
        .px(px(8.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .text_color(colors::text_disabled())
                .text_size(px(9.0))
                .child("RENDER"),
        )
        .child(render_track)
        .child(queue_label)
        .child(proxy_toggle)
        .child(ingest_btn)
}
