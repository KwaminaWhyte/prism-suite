//! Export Presets panel.

use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, divider};

use crate::app_state::{Action, App, ExportPreset};
use crate::Reel;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    for (i, preset) in app.export_preset_list.iter().enumerate() {
        let name = preset.name.clone();
        let fmt = format!("{:?} {}x{} {:.0}fps", preset.format, preset.width, preset.height, preset.fps);
        let del_id = gpui::SharedString::from(format!("ep-del-{i}"));
        let apply_id = gpui::SharedString::from(format!("ep-apply-{i}"));
        rows.push(
            div().flex().flex_row().items_center().justify_between()
                .px_3().py_1()
                .border_b_1().border_color(colors::surface_border())
                .child(
                    div().flex().flex_col()
                        .child(div().text_color(colors::text_primary()).text_size(px(11.0)).child(name))
                        .child(div().text_color(colors::text_secondary()).text_size(px(9.0)).child(fmt))
                )
                .child(
                    div().flex().flex_row().gap_1()
                        .child(
                            div().id(apply_id).px_2().py_1().rounded_sm().bg(colors::accent())
                                .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::ExportWithPreset(i));
                                    cx.notify();
                                }))
                                .child("Apply")
                        )
                        .child(
                            div().id(del_id).px_2().py_1().rounded_sm().bg(colors::surface_raised())
                                .text_color(colors::text_secondary()).text_size(px(10.0)).cursor_pointer()
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::DeleteExportPresetNew(i));
                                    cx.notify();
                                }))
                                .child("Del")
                        )
                )
                .into_any_element()
        );
    }

    let cur_fmt = app.export_format;
    let cur_w = app.project.width;
    let cur_h = app.project.height;
    let cur_fps = app.project.fps;

    div().flex().flex_col()
        .child(section_header("Export Presets"))
        .child(divider())
        .children(rows)
        .child(
            div().px_3().py_2()
                .child(
                    div().id("ep-save").px_2().py_1().rounded_sm().bg(colors::accent())
                        .text_color(colors::text_primary()).text_size(px(10.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let name = format!("{}x{} {:.0}fps", cur_w, cur_h, cur_fps);
                            root.app.apply(Action::AddExportPreset(ExportPreset {
                                name,
                                format: cur_fmt,
                                width: cur_w,
                                height: cur_h,
                                fps: cur_fps as f64,
                                bitrate_kbps: 8000,
                            }));
                            cx.notify();
                        }))
                        .child("Save Current Settings as Preset")
                )
        )
}
