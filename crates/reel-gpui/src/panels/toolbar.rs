//! Top toolbar — app title, import, view toggles, export.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, Icon};
use prism_ui::components::tool_button;

use crate::app_state::{Action, App, AUDIO_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::Reel;

macro_rules! icon_btn {
    ($id:expr, $icon:expr, $active:expr, $cx:expr, $listener:expr) => {
        div()
            .id($id)
            .cursor_pointer()
            .on_click($cx.listener($listener))
            .child(tool_button($icon, $active))
    };
}

pub fn render(
    app: &App,
    export_label: Option<String>,
    cx: &mut Context<Reel>,
) -> impl IntoElement {
    let frame = app.project.frame_at(app.time);
    let audio_playing = app.audio_playing;

    let show_mixer = app.show_mixer;
    let mixer_btn = icon_btn!(
        "mixer", Icon::Speaker, show_mixer, cx,
        move |root, _ev, _win, cx| { root.app.apply(Action::ToggleMixer); cx.notify(); }
    );

    let scopes_on = app.scopes_open;
    let scopes_btn = icon_btn!(
        "scopes", Icon::Histogram, scopes_on, cx,
        move |root, _ev, _win, cx| { root.app.apply(Action::ToggleScopes); cx.notify(); }
    );

    let bins_on = app.bins_open;
    let bins_btn = div()
        .id("bins")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center()
        .bg(if bins_on { colors::accent() } else { colors::surface_overlay() })
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleBins);
            cx.notify();
        }))
        .child("Bins");

    let seq_settings_on = app.show_sequence_settings;
    let seq_settings_btn = div()
        .id("seq-settings")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center()
        .bg(if seq_settings_on { colors::accent() } else { colors::surface_overlay() })
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleSequenceSettings);
            cx.notify();
        }))
        .child("Seq");

    let show_cc = app.show_captions_panel;
    let cc_btn = div()
        .id("cc-btn")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center()
        .bg(if show_cc { colors::accent() } else { colors::surface_overlay() })
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleCaptionsPanel);
            cx.notify();
        }))
        .child("CC");

    let show_markers = app.show_markers_panel;
    let markers_btn = div()
        .id("markers-btn")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center()
        .bg(if show_markers { colors::accent() } else { colors::surface_overlay() })
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleMarkersPanel);
            cx.notify();
        }))
        .child("Markers");

    let show_ep = app.show_export_presets;
    let export_presets_btn = div()
        .id("export-presets-btn")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center()
        .bg(if show_ep { colors::accent() } else { colors::surface_overlay() })
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleExportPresets);
            cx.notify();
        }))
        .child("Export\u{25be}");

    let dual_on = app.dual_viewer;
    let dual_btn = icon_btn!(
        "dual-viewer", Icon::Waveform, dual_on, cx,
        move |root, _ev, _win, cx| { root.app.apply(Action::ToggleDualViewer); cx.notify(); }
    );

    let exporting = export_label.is_some();
    let default_name = format!("{}.mp4", crate::export::sanitize_stem(&app.project.name));
    let export = div()
        .id("export")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center().gap_1()
        .bg(if exporting { colors::surface_overlay() } else { colors::accent() })
        .text_color(if exporting { colors::text_disabled() } else { colors::text_primary() })
        .text_size(px(11.0))
        .when(!exporting, |el| {
            el.cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("MP4 video", &["mp4"])
                        .set_file_name(&default_name)
                        .set_title("Export MP4")
                        .save_file()
                    {
                        root.start_export(path);
                        cx.notify();
                    }
                }))
        })
        .child(
            svg().path(Icon::Film.path()).w(px(14.0)).h(px(14.0))
                .text_color(if exporting { colors::text_disabled() } else { colors::text_primary() }),
        )
        .child("Export");

    let import = div()
        .id("import")
        .px_2().py_1()
        .rounded_md()
        .flex().items_center().gap_1()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(11.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            let image_exts = prism_io::SUPPORTED_EXTENSIONS;
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Video", VIDEO_EXTENSIONS)
                .add_filter("Audio", AUDIO_EXTENSIONS)
                .add_filter("Images", image_exts)
                .set_title("Import media")
                .pick_file()
            {
                root.app.apply(Action::ImportMedia(path));
                cx.notify();
            }
        }))
        .child(
            svg().path(Icon::Import.path()).w(px(14.0)).h(px(14.0))
                .text_color(colors::text_primary()),
        )
        .child("Import");

    let sep = || div().w(px(1.0)).h(px(20.0)).bg(colors::surface_border());

    let cur_fmt = app.export_format;
    let fmt_btns: Vec<gpui::AnyElement> = [
        ("MP4",        crate::app_state::ExportFormat::H264Mp4,        0usize),
        ("ProRes P",   crate::app_state::ExportFormat::ProResProxy,    1usize),
        ("ProRes 422", crate::app_state::ExportFormat::ProRes422,      2usize),
        ("GIF",        crate::app_state::ExportFormat::Gif,            3usize),
    ]
    .into_iter()
    .map(|(label, fmt, idx)| {
        let active = cur_fmt == fmt;
        div()
            .id(("fmt-btn", idx))
            .px(px(5.0))
            .py(px(2.0))
            .rounded_sm()
            .cursor_pointer()
            .bg(if active { colors::accent() } else { colors::surface_overlay() })
            .text_color(colors::text_primary())
            .text_size(px(10.0))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetExportFormat(fmt));
                cx.notify();
            }))
            .child(label)
            .into_any_element()
    })
    .collect();

    div()
        .size_full()
        .flex().items_center()
        .px_3().gap_2()
        .child(div().text_color(colors::text_primary()).child("Reel"))
        .child(
            div().text_color(colors::text_secondary()).text_size(px(11.0))
                .child(app.project.name.clone()),
        )
        .child(sep())
        .child(import)
        .child(sep())
        .child(mixer_btn)
        .child(scopes_btn)
        .child(dual_btn)
        .child(bins_btn)
        .child(sep())
        .child(seq_settings_btn)
        .child(cc_btn)
        .child(markers_btn)
        .child(sep())
        .child(export_presets_btn)
        .child(sep())
        .children(fmt_btns)
        .child(sep())
        .child(export)
        .child(
            div().text_color(colors::text_secondary()).text_size(px(11.0))
                .child(format!("{:.2}s · f{}", app.time, frame)),
        )
        .children(if audio_playing {
            Some(div().text_color(colors::text_primary()).text_size(px(11.0)).child("\u{266a}"))
        } else { None })
        .children(export_label.map(|label| {
            div().text_color(colors::text_primary()).text_size(px(11.0)).child(label)
        }))
}
