//! Closed Captions / Subtitles panel.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App, Caption, CaptionStyle};
use crate::panels::TextAreas;
use crate::Reel;

pub fn render(app: &App, areas: &TextAreas, cx: &mut Context<Reel>) -> impl IntoElement {
    let t = app.time;

    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    for (i, cap) in app.captions.iter().enumerate() {
        let active = t >= cap.start_secs && t < cap.end_secs;
        let start_t = cap.start_secs;
        let rm_id = gpui::SharedString::from(format!("cap-rm-{i}"));
        let row_id = gpui::SharedString::from(format!("cap-row-{i}"));
        // Editable, multi-line cue text via the persistent TextArea registry.
        // Cmd/Ctrl+Enter applies (SetCueText); falls back to a static label.
        let cue_body: gpui::AnyElement = match areas.get(&format!("cue-text-{i}")).cloned() {
            Some(a) => a.into_any_element(),
            None => div()
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child(cap.text.clone())
                .into_any_element(),
        };
        rows.push(
            div()
                .id(row_id)
                .flex().flex_row().items_start().justify_between()
                .px_3().py_1().gap_2()
                .bg(if active {
                    gpui::Rgba { r: 0.5, g: 0.4, b: 0.0, a: 0.3 }
                } else {
                    colors::surface_bg()
                })
                .child(
                    div().flex_1().flex().flex_col().gap(px(2.0))
                        .child(cue_body)
                        .child(
                            div().id(("cap-seek", i)).cursor_pointer()
                                .text_color(colors::text_secondary()).text_size(px(9.0))
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.time = start_t;
                                    root.app.host.mark_dirty();
                                    cx.notify();
                                }))
                                .child(format!("{:.2}s \u{2013} {:.2}s", cap.start_secs, cap.end_secs)))
                )
                .child(
                    div().id(rm_id).w(px(18.0)).h(px(18.0))
                        .flex().items_center().justify_center()
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_secondary())
                        .text_size(px(11.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::RemoveCaption(i));
                            cx.notify();
                        }))
                        .child("\u{d7}")
                )
                .into_any_element(),
        );
    }

    let current_t = app.time;
    div().flex().flex_col()
        .child(section_header("Captions"))
        .child(divider())
        .child(
            div().flex().flex_row().gap_1().px_3().py_1()
                .child(
                    div().id("cap-add").px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::AddCaption(Caption {
                                start_secs: current_t,
                                end_secs: current_t + 3.0,
                                text: "New Caption".into(),
                                style: CaptionStyle::default(),
                            }));
                            cx.notify();
                        }))
                        .child("+ Add")
                )
                .child(
                    div().id("cap-import").px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("SRT subtitle", &["srt"])
                                .set_title("Import SRT")
                                .pick_file()
                            {
                                root.app.apply(Action::ImportSrt(path));
                                cx.notify();
                            }
                        }))
                        .child("Import SRT")
                )
                .child(
                    div().id("cap-export").px_2().py_1().rounded_sm().bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("SRT subtitle", &["srt"])
                                .set_file_name("captions.srt")
                                .set_title("Export SRT")
                                .save_file()
                            {
                                root.app.apply(Action::ExportSrt(path));
                                cx.notify();
                            }
                        }))
                        .child("Export SRT")
                )
        )
        .children(rows)
}
