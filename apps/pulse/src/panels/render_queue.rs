//! Render queue panel — batch export jobs for the GPUI host.
//!
//! Displays pending exports as a list; each row shows the comp name, output
//! path (truncated), a status badge (Pending / Rendering N% / Done / Failed),
//! and a remove button. "Add to Queue" appends the active comp; "Render All"
//! processes the queue serially via `Action::RenderAll`.

use gpui::{div, px, rgb, Context, Entity, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};
use gpui::prelude::FluentBuilder;

use prism_ui::{colors, TextField};

use crate::app_state::{Action, App, RenderFormat, RenderJob, RenderJobStatus};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

pub fn render(app: &App, render_path_field: &Entity<TextField>, cx: &mut Context<Pulse>) -> impl IntoElement {
    // Build output preset rows before div chain.
    let preset_rows: Vec<gpui::AnyElement> = app
        .output_presets
        .iter()
        .enumerate()
        .map(|(i, preset)| {
            let name = preset.name.clone();
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(name),
                )
                .child(
                    div()
                        .id(("rq-preset-load", i))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .text_size(px(9.0))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .child("Apply")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::LoadOutputPreset(i));
                            cx.notify();
                        })),
                )
                .into_any_element()
        })
        .collect();

    let has_presets = !preset_rows.is_empty();

    // Format picker chips — one per RenderFormat so all variants are constructed.
    let fmt_chips: Vec<gpui::AnyElement> = RenderFormat::ALL
        .iter()
        .copied()
        .map(|fmt| {
            let active_fmt = app
                .render_queue
                .first()
                .map(|j| j.format == fmt)
                .unwrap_or(false);
            let label = fmt.label();
            div()
                .id(("rq-fmt", fmt as usize))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if active_fmt { rgb(BG_ACTIVE) } else { colors::surface_raised() })
                .text_size(px(9.0))
                .text_color(if active_fmt {
                    colors::text_primary()
                } else {
                    colors::text_secondary()
                })
                .child(label)
                .into_any_element()
        })
        .collect();

    let header = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(11.0))
                .child("Render Queue"),
        )
        .child(
            div()
                .id("rq-add-all")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_size(px(10.0))
                .text_color(colors::text_secondary())
                .cursor_pointer()
                .child("All Comps")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::AddAllCompsToQueue);
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("rq-add")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_size(px(10.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .child("+ Add")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::AddToRenderQueue);
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("rq-render-all")
                .flex_none()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(BG_ACTIVE))
                .text_size(px(10.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .child("Render All")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::RenderAll);
                    cx.notify();
                })),
        );

    let rows: Vec<gpui::AnyElement> = app
        .render_queue
        .iter()
        .enumerate()
        .map(|(i, job)| queue_row(cx, i, job))
        .collect();

    let body = if rows.is_empty() {
        div()
            .px_3()
            .py_2()
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .child("No jobs queued. Use + Add or All Comps.")
            .into_any_element()
    } else {
        div().flex().flex_col().children(rows).into_any_element()
    };

    let fmt_row = div()
        .flex()
        .flex_wrap()
        .gap_1()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors::surface_border())
        .children(fmt_chips);

    // Editable output path (REAL typing — Enter applies via on_submit).
    let path_row = div()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Output path"),
        )
        .child(render_path_field.clone());

    let mut panel = div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(fmt_row)
        .child(path_row)
        .child(body);

    if has_presets {
        panel = panel
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .text_size(px(10.0))
                    .child("Output Presets"),
            )
            .children(preset_rows);
    }

    panel
}

fn queue_row(cx: &mut Context<Pulse>, i: usize, job: &RenderJob) -> gpui::AnyElement {
    let (status_text, status_color) = match &job.status {
        RenderJobStatus::Pending => ("Pending", colors::text_secondary()),
        RenderJobStatus::Rendering(f) => {
            let _ = f;
            ("Rendering…", rgb(0x37c8c0u32))
        }
        RenderJobStatus::Done => ("Done", rgb(0x98c379u32)),
        RenderJobStatus::Failed(msg) => {
            let _ = msg;
            ("Failed", rgb(0xe06c75u32))
        }
    };

    let format_label = format!("{} (.{})", job.format.label(), job.format.extension());
    let format_color = match job.format {
        RenderFormat::Mp4H264 => rgb(0x61afef_u32),
        RenderFormat::Mp4H265 => rgb(0x56b6c2_u32),
        RenderFormat::ProResProxy => rgb(0xc678dd_u32),
        RenderFormat::Gif => rgb(0xe5c07b_u32),
    };

    let path_str = job.output_path.to_string_lossy();
    let truncated = if path_str.len() > 28 {
        format!("…{}", &path_str[path_str.len() - 27..])
    } else {
        path_str.to_string()
    };

    let progress_frac = match &job.status {
        RenderJobStatus::Rendering(f) => *f,
        RenderJobStatus::Done => 1.0,
        _ => 0.0,
    };

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .child(
                    div()
                        .text_color(colors::text_primary())
                        .text_size(px(10.0))
                        .child(job.comp_name.clone()),
                )
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(9.0))
                        .child(truncated),
                )
                .when(progress_frac > 0.0 && progress_frac < 1.0, |d: gpui::Div| {
                    d.child(
                        div()
                            .h(px(2.0))
                            .w_full()
                            .bg(colors::surface_raised())
                            .rounded_md()
                            .child(
                                div()
                                    .h(px(2.0))
                                    .w(px(60.0 * progress_frac))
                                    .bg(rgb(0x37c8c0))
                                    .rounded_md(),
                            ),
                    )
                }),
        )
        .child(
            div()
                .px_1()
                .py_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_color(format_color)
                .text_size(px(9.0))
                .child(format_label.clone()),
        )
        .child(
            div()
                .px_1()
                .py_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_color(status_color)
                .text_size(px(9.0))
                .child(status_text),
        )
        .child(
            div()
                .id(("rq-remove", i))
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(BG_ACTIVE))
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .cursor_pointer()
                .child("×")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::RemoveFromRenderQueue(i));
                    cx.notify();
                })),
        )
        .into_any_element()
}
