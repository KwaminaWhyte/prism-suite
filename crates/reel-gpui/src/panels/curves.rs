//! RGB Curves color panel — per-channel tone curve editor.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App};
use crate::Reel;

pub fn render_section(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let channel = app.curves_channel;

    // Tab buttons: Master / R / G / B
    let tab_labels = ["Master", "R", "G", "B"];
    let mut tabs: Vec<gpui::AnyElement> = Vec::new();
    for (ti, label) in tab_labels.iter().enumerate() {
        let ti = ti as u8;
        let active = channel == ti;
        let text_color = match ti {
            1 => gpui::Rgba { r: 1.0, g: 0.3, b: 0.3, a: 1.0 },
            2 => gpui::Rgba { r: 0.3, g: 0.9, b: 0.3, a: 1.0 },
            3 => gpui::Rgba { r: 0.4, g: 0.6, b: 1.0, a: 1.0 },
            _ => gpui::Rgba { r: 0.9, g: 0.9, b: 0.9, a: 1.0 },
        };
        tabs.push(
            div()
                .id(("curve-tab", ti as usize))
                .px_2().py_1()
                .rounded_sm()
                .cursor_pointer()
                .bg(if active { colors::accent() } else { colors::surface_overlay() })
                .text_color(text_color)
                .text_size(px(10.0))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetCurvesChannel(ti));
                    cx.notify();
                }))
                .child(*label)
                .into_any_element(),
        );
    }

    let active_pts: Vec<[f32; 2]> = match channel {
        1 => app.rgb_curves.red.clone(),
        2 => app.rgb_curves.green.clone(),
        3 => app.rgb_curves.blue.clone(),
        _ => app.rgb_curves.master.clone(),
    };

    // Control point stepper rows
    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    for (pi, pt) in active_pts.iter().enumerate() {
        let pt_in = pt[0];
        let pt_out = pt[1];
        let ch = channel;

        let dec_in_id = gpui::SharedString::from(format!("cv-{ch}-{pi}-di"));
        let inc_in_id = gpui::SharedString::from(format!("cv-{ch}-{pi}-ii"));
        let dec_out_id = gpui::SharedString::from(format!("cv-{ch}-{pi}-do"));
        let inc_out_id = gpui::SharedString::from(format!("cv-{ch}-{pi}-io"));

        let row = div().flex().items_center().justify_between().px_3().py(px(2.0)).gap_1()
            .child(div().text_color(colors::text_secondary()).text_size(px(10.0)).child(format!("P{}", pi + 1)))
            .child(
                div().flex().items_center().gap_1()
                    .child(div().text_color(colors::text_secondary()).text_size(px(9.0)).child("In:"))
                    .child(
                        div().id(dec_in_id).w(px(16.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_sm().bg(colors::surface_raised())
                            .text_color(colors::text_primary()).text_size(px(11.0)).cursor_pointer()
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                let mut nc = root.app.rgb_curves.clone();
                                let target = match ch { 1 => &mut nc.red, 2 => &mut nc.green, 3 => &mut nc.blue, _ => &mut nc.master };
                                if pi < target.len() { target[pi][0] = (target[pi][0] - 0.05).clamp(0.0, 1.0); }
                                root.app.apply(Action::SetRgbCurves(nc));
                                cx.notify();
                            }))
                            .child("\u{25c4}")
                    )
                    .child(div().text_color(colors::text_primary()).text_size(px(10.0)).min_w(px(30.0)).child(format!("{:.2}", pt_in)))
                    .child(
                        div().id(inc_in_id).w(px(16.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_sm().bg(colors::surface_raised())
                            .text_color(colors::text_primary()).text_size(px(11.0)).cursor_pointer()
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                let mut nc = root.app.rgb_curves.clone();
                                let target = match ch { 1 => &mut nc.red, 2 => &mut nc.green, 3 => &mut nc.blue, _ => &mut nc.master };
                                if pi < target.len() { target[pi][0] = (target[pi][0] + 0.05).clamp(0.0, 1.0); }
                                root.app.apply(Action::SetRgbCurves(nc));
                                cx.notify();
                            }))
                            .child("\u{25ba}")
                    )
                    .child(div().text_color(colors::text_secondary()).text_size(px(9.0)).child("Out:"))
                    .child(
                        div().id(dec_out_id).w(px(16.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_sm().bg(colors::surface_raised())
                            .text_color(colors::text_primary()).text_size(px(11.0)).cursor_pointer()
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                let mut nc = root.app.rgb_curves.clone();
                                let target = match ch { 1 => &mut nc.red, 2 => &mut nc.green, 3 => &mut nc.blue, _ => &mut nc.master };
                                if pi < target.len() { target[pi][1] = (target[pi][1] - 0.05).clamp(0.0, 1.0); }
                                root.app.apply(Action::SetRgbCurves(nc));
                                cx.notify();
                            }))
                            .child("\u{25bc}")
                    )
                    .child(div().text_color(colors::text_primary()).text_size(px(10.0)).min_w(px(30.0)).child(format!("{:.2}", pt_out)))
                    .child(
                        div().id(inc_out_id).w(px(16.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_sm().bg(colors::surface_raised())
                            .text_color(colors::text_primary()).text_size(px(11.0)).cursor_pointer()
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                let mut nc = root.app.rgb_curves.clone();
                                let target = match ch { 1 => &mut nc.red, 2 => &mut nc.green, 3 => &mut nc.blue, _ => &mut nc.master };
                                if pi < target.len() { target[pi][1] = (target[pi][1] + 0.05).clamp(0.0, 1.0); }
                                root.app.apply(Action::SetRgbCurves(nc));
                                cx.notify();
                            }))
                            .child("\u{25b2}")
                    )
            )
            .into_any_element();
        rows.push(row);
    }

    let add_id = gpui::SharedString::from(format!("cv-add-{channel}"));
    let reset_id = gpui::SharedString::from(format!("cv-reset-{channel}"));
    let ch = channel;

    div().flex().flex_col()
        .child(section_header("RGB Curves"))
        .child(divider())
        .child(div().flex().flex_row().gap_1().px_3().py_1().children(tabs))
        .children(rows)
        .child(
            div().flex().flex_row().gap_1().px_3().py_1()
                .child(
                    div().id(add_id).px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let mut nc = root.app.rgb_curves.clone();
                            let target = match ch { 1 => &mut nc.red, 2 => &mut nc.green, 3 => &mut nc.blue, _ => &mut nc.master };
                            target.push([0.5, 0.5]);
                            target.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
                            root.app.apply(Action::SetRgbCurves(nc));
                            cx.notify();
                        }))
                        .child("+ Point")
                )
                .child(
                    div().id(reset_id).px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_secondary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let mut nc = root.app.rgb_curves.clone();
                            let identity = vec![[0.0f32, 0.0], [1.0, 1.0]];
                            let target = match ch { 1 => &mut nc.red, 2 => &mut nc.green, 3 => &mut nc.blue, _ => &mut nc.master };
                            *target = identity;
                            root.app.apply(Action::SetRgbCurves(nc));
                            cx.notify();
                        }))
                        .child("Reset")
                )
        )
}
