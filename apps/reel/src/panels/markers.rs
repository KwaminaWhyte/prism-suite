//! Markers panel — timeline marker list with In/Out/Chapter/Comment.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App, MarkerKind};
use crate::Reel;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let work_in = app.work_area_in;
    let work_out = app.work_area_out;

    let kinds: &[(&str, MarkerKind)] = &[
        ("In", MarkerKind::InPoint),
        ("Out", MarkerKind::OutPoint),
        ("Chapter", MarkerKind::Chapter),
        ("Comment", MarkerKind::Comment),
    ];
    let mut add_btns: Vec<gpui::AnyElement> = Vec::new();
    for (ki, (label, kind)) in kinds.iter().enumerate() {
        let kind = kind.clone();
        let label = *label;
        add_btns.push(
            div().id(("mk-add", ki))
                .px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let t = root.app.time;
                    root.app.apply(Action::AddMarkerAt {
                        time: t,
                        name: label.to_string(),
                        kind: kind.clone(),
                    });
                    cx.notify();
                }))
                .child(label)
                .into_any_element()
        );
    }

    let set_in_id = gpui::SharedString::from("mk-set-in");
    let set_out_id = gpui::SharedString::from("mk-set-out");

    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    for (i, marker) in app.gpui_markers.iter().enumerate() {
        let jump_t = marker.time_secs;
        let color = gpui::Rgba { r: marker.color[0], g: marker.color[1], b: marker.color[2], a: 1.0 };
        let rm_id = gpui::SharedString::from(format!("mk-rm-{i}"));
        let row_id = gpui::SharedString::from(format!("mk-row-{i}"));
        rows.push(
            div().id(row_id)
                .flex().flex_row().items_center().gap_2().px_3().py_1()
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.time = jump_t;
                    root.app.host.mark_dirty();
                    cx.notify();
                }))
                .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(color))
                .child(
                    div().flex_1().flex().flex_col()
                        .child(div().text_color(colors::text_primary()).text_size(px(11.0)).child(marker.name.clone()))
                        .child(div().text_color(colors::text_secondary()).text_size(px(9.0))
                            .child(format!("{} @ {:.2}s", marker.kind.label(), marker.time_secs)))
                )
                .child(
                    div().id(rm_id).w(px(18.0)).h(px(18.0))
                        .flex().items_center().justify_center()
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_secondary())
                        .text_size(px(11.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::RemoveMarker(i));
                            cx.notify();
                        }))
                        .child("\u{d7}")
                )
                .into_any_element()
        );
    }

    div().flex().flex_col()
        .child(section_header("Markers"))
        .child(divider())
        .child(div().flex().flex_row().flex_wrap().gap_1().px_3().py_1().children(add_btns))
        .child(divider())
        .child(
            div().flex().flex_row().items_center().gap_2().px_3().py_1()
                .child(
                    div().id(set_in_id).px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let t = root.app.time;
                            root.app.apply(Action::SetInPoint(t));
                            cx.notify();
                        }))
                        .child("Mark In")
                )
                .child(div().text_color(colors::text_secondary()).text_size(px(10.0))
                    .child(format!("{:.2}s", work_in)))
                .child(
                    div().id(set_out_id).px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let t = root.app.time;
                            root.app.apply(Action::SetOutPoint(t));
                            cx.notify();
                        }))
                        .child("Mark Out")
                )
                .child(div().text_color(colors::text_secondary()).text_size(px(10.0))
                    .child(format!("{:.2}s", work_out)))
        )
        .child(divider())
        .children(rows)
}
