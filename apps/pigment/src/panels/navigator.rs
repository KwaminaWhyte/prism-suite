//! Navigator panel + multi-document tab bar.
//!
//! Two related view-management surfaces, both following the panel convention
//! (`render(app: &App, cx: &mut Context<Pigment>)`, emitting `Action`s via
//! `cx.listener`):
//!
//!   * [`render_tabs`] — the horizontal document-tab strip placed above the
//!     canvas. Reads `app.doc_tabs`; emits `ActivateDocTab` / `CloseDocTab` and a
//!     "+" that emits `OpenDocTab`.
//!   * [`render`] — the Navigator dock panel: a bird's-eye proxy rect over the
//!     document plus zoom/pan controls. Reads `app.navigator`; emits
//!     `NavigatorZoom` / `NavigatorPan`.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App};
use crate::Pigment;

/// Compact square icon-style button bound to an [`Action`].
fn ctrl_btn(
    cx: &mut Context<Pigment>,
    id: (&'static str, u64),
    label: &'static str,
    action: impl Fn() -> Action + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .text_size(px(font_size::SM))
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(action());
            cx.notify();
        }))
        .child(label)
}

/// The multi-document tab strip. One tab per open document; the active tab is
/// accented. Each tab activates on click and has a small close affordance; a
/// trailing "+" opens a new untitled document tab sized to the current doc.
pub fn render_tabs(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let active = app.doc_tabs.active;
    let (new_w, new_h) = (app.host.doc_w, app.host.doc_h);
    let next_num = app.doc_tabs.next_id + 1;

    let tabs = app.doc_tabs.tabs.iter().enumerate().map(|(i, t)| {
        let is_active = i == active;
        let id = t.id;
        let dirty = t.dirty;
        let title = t.title.clone();

        let close_id = id;
        let close = div()
            .id(("doctab-close", id))
            .w(px(14.0))
            .h(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .text_size(px(font_size::XS))
            .text_color(colors::text_secondary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_border()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::CloseDocTab(close_id));
                cx.notify();
            }))
            .child(if dirty { "•" } else { "✕" });

        div()
            .id(("doctab", id))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_2()
            .h(px(26.0))
            .rounded_md()
            .cursor_pointer()
            .map(|d| {
                if is_active {
                    d.bg(colors::surface_overlay())
                        .text_color(colors::text_primary())
                } else {
                    d.text_color(colors::text_secondary())
                        .hover(|s| s.bg(colors::surface_bg()))
                }
            })
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::ActivateDocTab(id));
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .child(title),
            )
            .child(close)
    });

    let add = div()
        .id("doctab-add")
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_size(px(font_size::MD))
        .text_color(colors::text_secondary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::OpenDocTab {
                title: format!("Untitled-{next_num}"),
                width: new_w.max(1),
                height: new_h.max(1),
            });
            cx.notify();
        }))
        .child("+");

    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(3.0))
        .children(tabs)
        .child(add)
}

/// The Navigator dock panel: a proxy thumbnail of the document with the current
/// viewport rect drawn on top, plus zoom and pan controls.
pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let nav = app.navigator;
    let [dw, dh] = nav.doc_size;
    let [vx, vy, vw, vh] = nav.view_rect;
    let zoom_pct = format!("{:.0}%", nav.zoom * 100.0);

    // Fit the document into a fixed proxy box, preserving aspect.
    const BOX_W: f32 = 220.0;
    const BOX_H: f32 = 130.0;
    let scale = (BOX_W / dw.max(1.0)).min(BOX_H / dh.max(1.0));
    let pw = dw * scale;
    let ph = dh * scale;
    // Viewport rect in proxy-box px.
    let rect_x = vx * scale;
    let rect_y = vy * scale;
    let rect_w = (vw * scale).max(2.0);
    let rect_h = (vh * scale).max(2.0);
    // Pan step = 10% of the document extent.
    let pan_x = (dw * 0.1).max(1.0);
    let pan_y = (dh * 0.1).max(1.0);

    let proxy = div()
        .relative()
        .w(px(pw))
        .h(px(ph))
        .bg(colors::surface_bg())
        .border_1()
        .border_color(colors::surface_border())
        .rounded_sm()
        // Viewport indicator.
        .child(
            div()
                .absolute()
                .left(px(rect_x))
                .top(px(rect_y))
                .w(px(rect_w))
                .h(px(rect_h))
                .border_1()
                .border_color(colors::accent())
                .bg(gpui::rgba(0x7c5af522)),
        );

    let zoom_out = ctrl_btn(cx, ("nav-zoom-out", 0), "−", move || {
        Action::NavigatorZoom(nav.zoom * 0.8)
    });
    let zoom_in = ctrl_btn(cx, ("nav-zoom-in", 0), "+", move || {
        Action::NavigatorZoom(nav.zoom * 1.25)
    });
    let zoom_fit = ctrl_btn(cx, ("nav-zoom-fit", 0), "Fit", || Action::NavigatorZoom(1.0));

    let pan_left = ctrl_btn(cx, ("nav-pan", 0), "◄", move || Action::NavigatorPan {
        dx: -pan_x,
        dy: 0.0,
    });
    let pan_right = ctrl_btn(cx, ("nav-pan", 1), "►", move || Action::NavigatorPan {
        dx: pan_x,
        dy: 0.0,
    });
    let pan_up = ctrl_btn(cx, ("nav-pan", 2), "▲", move || Action::NavigatorPan {
        dx: 0.0,
        dy: -pan_y,
    });
    let pan_down = ctrl_btn(cx, ("nav-pan", 3), "▼", move || Action::NavigatorPan {
        dx: 0.0,
        dy: pan_y,
    });

    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .text_color(colors::text_primary())
                .text_size(px(font_size::SM))
                .child("Navigator"),
        )
        .child(div().flex().justify_center().child(proxy))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(zoom_out)
                        .child(
                            div()
                                .min_w(px(44.0))
                                .flex()
                                .justify_center()
                                .text_size(px(font_size::SM))
                                .text_color(colors::text_primary())
                                .child(zoom_pct),
                        )
                        .child(zoom_in)
                        .child(zoom_fit),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(pan_left)
                        .child(pan_up)
                        .child(pan_down)
                        .child(pan_right),
                ),
        )
}
