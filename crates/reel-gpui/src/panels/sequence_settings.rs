//! Sequence Settings dialog — a floating overlay showing and editing sequence
//! parameters: resolution, frame rate, audio sample rate, color space.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App, ColorSpace};
use crate::Reel;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let w = app.project.width;
    let h = app.project.height;
    let fps = app.project.fps;
    let sr = app.sequence_sample_rate;
    let cs = app.sequence_color_space;

    // Pre-build rows
    let mut rows: Vec<gpui::AnyElement> = Vec::new();

    // Resolution steppers
    rows.push(section_header("Resolution").into_any_element());
    rows.push(divider().into_any_element());
    rows.push(
        stepper_row("ss-width", "Width", format!("{}", w),
            Action::SetSequenceSize { w: w.saturating_sub(80).max(1), h },
            Action::SetSequenceSize { w: w + 80, h },
            cx,
        ).into_any_element()
    );
    rows.push(
        stepper_row("ss-height", "Height", format!("{}", h),
            Action::SetSequenceSize { w, h: h.saturating_sub(45).max(1) },
            Action::SetSequenceSize { w, h: h + 45 },
            cx,
        ).into_any_element()
    );

    // FPS steppers
    rows.push(section_header("Frame Rate").into_any_element());
    rows.push(divider().into_any_element());
    // Common FPS presets
    let fps_presets: Vec<gpui::AnyElement> = [23.976f32, 24.0, 25.0, 29.97, 30.0, 60.0]
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let active = (fps - f).abs() < 0.05;
            let label = format!("{:.3}", f)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string();
            div()
                .id(("ss-fps", i))
                .px_2().py(px(2.0))
                .rounded_sm()
                .cursor_pointer()
                .bg(if active { colors::accent() } else { colors::surface_overlay() })
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetFrameRate(f));
                    cx.notify();
                }))
                .child(label)
                .into_any_element()
        })
        .collect();
    rows.push(
        div().flex().flex_wrap().gap_1().px_3().py_1().children(fps_presets).into_any_element()
    );

    // Sample rate
    rows.push(section_header("Sample Rate").into_any_element());
    rows.push(divider().into_any_element());
    let sr_presets: Vec<gpui::AnyElement> = [44100u32, 48000, 96000]
        .into_iter()
        .enumerate()
        .map(|(i, rate)| {
            let active = sr == rate;
            let label = format!("{}kHz", rate / 1000);
            div()
                .id(("ss-sr", i))
                .px_2().py(px(2.0))
                .rounded_sm()
                .cursor_pointer()
                .bg(if active { colors::accent() } else { colors::surface_overlay() })
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetSampleRate(rate));
                    cx.notify();
                }))
                .child(label)
                .into_any_element()
        })
        .collect();
    rows.push(
        div().flex().flex_wrap().gap_1().px_3().py_1().children(sr_presets).into_any_element()
    );

    // Color space
    rows.push(section_header("Color Space").into_any_element());
    rows.push(divider().into_any_element());
    let cs_presets: Vec<gpui::AnyElement> = [
        (ColorSpace::Rec709, "Rec.709"),
        (ColorSpace::Rec2020, "Rec.2020"),
        (ColorSpace::SRGB, "sRGB"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (c, label))| {
        let active = cs == c;
        div()
            .id(("ss-cs", i))
            .px_2().py(px(2.0))
            .rounded_sm()
            .cursor_pointer()
            .bg(if active { colors::accent() } else { colors::surface_overlay() })
            .text_color(colors::text_primary())
            .text_size(px(10.0))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetColorSpace(c));
                cx.notify();
            }))
            .child(label)
            .into_any_element()
    })
    .collect();
    rows.push(
        div().flex().flex_wrap().gap_1().px_3().py_1().children(cs_presets).into_any_element()
    );

    // Close button
    let close_btn = div()
        .id("ss-close")
        .px_3().py_1()
        .rounded_sm()
        .cursor_pointer()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(11.0))
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::ToggleSequenceSettings);
            cx.notify();
        }))
        .child("Close");

    // Floating overlay panel
    div()
        .absolute()
        .top(px(50.0))
        .left(px(50.0))
        .w(px(280.0))
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .rounded_md()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div().text_color(colors::text_primary()).text_size(px(12.0))
                        .child("Sequence Settings"),
                )
                .child(close_btn),
        )
        .children(rows)
}

fn stepper_row(
    id: &'static str,
    label: &'static str,
    value: String,
    dec_action: Action,
    inc_action: Action,
    cx: &mut Context<Reel>,
) -> impl IntoElement {
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
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("{id}-dec")))
                        .w(px(20.0)).h(px(18.0))
                        .flex().items_center().justify_center()
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _e, _w, cx| {
                            root.app.apply(dec_action.clone());
                            cx.notify();
                        }))
                        .child("\u{2212}"),
                )
                .child(
                    div()
                        .px_2().py(px(2.0))
                        .rounded_sm()
                        .bg(colors::surface_overlay())
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .min_w(px(48.0))
                        .child(value),
                )
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("{id}-inc")))
                        .w(px(20.0)).h(px(18.0))
                        .flex().items_center().justify_center()
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _e, _w, cx| {
                            root.app.apply(inc_action.clone());
                            cx.notify();
                        }))
                        .child("+"),
                ),
        )
}
