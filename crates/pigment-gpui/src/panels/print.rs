//! Print dialog panel — floating overlay for printing the current canvas.

use gpui::{
    div, px, rgba, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, section_header};

use crate::app_state::{Action, App};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let paper_size = app.print_paper_size.clone();
    let landscape = app.print_landscape;
    let scale_mode = app.print_scale_mode.clone();
    let color_space = app.print_color_space.clone();

    let paper_sizes = ["A4", "Letter", "Legal", "A3", "Custom"];
    let color_spaces = ["sRGB", "AdobeRGB", "Display P3"];
    let scale_modes = ["Fit to Page", "100%", "Custom %"];

    let paper_btns: Vec<_> = paper_sizes.iter().enumerate().map(|(i, &name)| {
        let is_active = name == paper_size.as_str();
        let name_s = name.to_string();
        div()
            .id(SharedString::from(format!("print-paper-{i}")))
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(font_size::XS))
            .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetPrintPaperSize(name_s.clone()));
                cx.notify();
            }))
            .child(name)
    }).collect();

    let color_btns: Vec<_> = color_spaces.iter().enumerate().map(|(i, &name)| {
        let is_active = name == color_space.as_str();
        let name_s = name.to_string();
        div()
            .id(SharedString::from(format!("print-cs-{i}")))
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(font_size::XS))
            .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetPrintColorSpace(name_s.clone()));
                cx.notify();
            }))
            .child(name)
    }).collect();

    let scale_btns: Vec<_> = scale_modes.iter().enumerate().map(|(i, &name)| {
        let is_active = name == scale_mode.as_str();
        let name_s = name.to_string();
        div()
            .id(SharedString::from(format!("print-scale-{i}")))
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(font_size::XS))
            .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetPrintScaleMode(name_s.clone()));
                cx.notify();
            }))
            .child(name)
    }).collect();

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000088))
        .child(
            div()
                .w(px(400.0))
                .bg(colors::surface_raised())
                .rounded_lg()
                .border_1()
                .border_color(colors::surface_border())
                .flex()
                .flex_col()
                .child(section_header("Print"))
                .child(divider())
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(px(font_size::XS)).text_color(colors::text_secondary()).child("Paper Size"))
                        .child(div().flex().flex_row().gap_1().flex_wrap().children(paper_btns))
                )
                .child(divider())
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().text_size(px(font_size::XS)).text_color(colors::text_secondary()).child("Orientation"))
                        .child(
                            div()
                                .id("print-portrait")
                                .px_2().py_1()
                                .rounded_md()
                                .text_size(px(font_size::XS))
                                .bg(if !landscape { colors::tool_active() } else { colors::surface_overlay() })
                                .border_1()
                                .border_color(colors::surface_border())
                                .text_color(colors::text_primary())
                                .cursor_pointer()
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::SetPrintLandscape(false));
                                    cx.notify();
                                }))
                                .child("Portrait")
                        )
                        .child(
                            div()
                                .id("print-landscape")
                                .px_2().py_1()
                                .rounded_md()
                                .text_size(px(font_size::XS))
                                .bg(if landscape { colors::tool_active() } else { colors::surface_overlay() })
                                .border_1()
                                .border_color(colors::surface_border())
                                .text_color(colors::text_primary())
                                .cursor_pointer()
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::SetPrintLandscape(true));
                                    cx.notify();
                                }))
                                .child("Landscape")
                        )
                )
                .child(divider())
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(px(font_size::XS)).text_color(colors::text_secondary()).child("Scale"))
                        .child(div().flex().flex_row().gap_1().flex_wrap().children(scale_btns))
                )
                .child(divider())
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(px(font_size::XS)).text_color(colors::text_secondary()).child("Print Space"))
                        .child(div().flex().flex_row().gap_1().flex_wrap().children(color_btns))
                )
                .child(divider())
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .justify_end()
                        .child(
                            div()
                                .id("print-cancel")
                                .px_3().py_1()
                                .rounded_md()
                                .bg(colors::surface_overlay())
                                .border_1()
                                .border_color(colors::surface_border())
                                .text_color(colors::text_secondary())
                                .text_size(px(font_size::SM))
                                .cursor_pointer()
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::TogglePrintDialog);
                                    cx.notify();
                                }))
                                .child("Cancel")
                        )
                        .child(
                            div()
                                .id("print-submit")
                                .px_3().py_1()
                                .rounded_md()
                                .bg(colors::tool_active())
                                .border_1()
                                .border_color(colors::surface_border())
                                .text_color(colors::text_primary())
                                .text_size(px(font_size::SM))
                                .cursor_pointer()
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::DoPrint);
                                    cx.notify();
                                }))
                                .child("Print")
                        )
                )
        )
}
