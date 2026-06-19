//! Top toolbar — app title, comp name, undo/redo, import, export buttons, audio toggle, RAM preview.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use gpui::relative;

use prism_ui::{colors, Icon, tool_button};

use crate::app_state::{Action, App};
use crate::Pulse;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];

    let can_undo = app.can_undo();
    let can_redo = app.can_redo();
    let live_out = app.live_output_enabled;

    div()
        .size_full()
        .flex()
        .items_center()
        .px_3()
        .gap_4()
        // App identity
        .child(div().text_color(colors::text_primary()).child("Pulse"))
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(comp.name.clone()),
        )
        .child(div().w(px(1.0)).h(px(18.0)).bg(colors::surface_border()))
        // Undo / Redo — dimmed when unavailable
        .child(
            div()
                .id("edit-undo")
                .w(px(28.0)).h(px(28.0))
                .flex().items_center().justify_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(if can_undo { colors::surface_overlay() } else { colors::surface_raised() })
                .when(can_undo, |d| d.cursor_pointer())
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    if root.app.can_undo() { root.app.apply(Action::Undo); cx.notify(); }
                }))
                .child(prism_ui::icon(
                    Icon::Undo,
                    14.0,
                )),
        )
        .child(
            div()
                .id("edit-redo")
                .w(px(28.0)).h(px(28.0))
                .flex().items_center().justify_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(if can_redo { colors::surface_overlay() } else { colors::surface_raised() })
                .when(can_redo, |d| d.cursor_pointer())
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    if root.app.can_redo() { root.app.apply(Action::Redo); cx.notify(); }
                }))
                .child(prism_ui::icon(Icon::Redo, 14.0)),
        )
        .child(div().w(px(1.0)).h(px(18.0)).bg(colors::surface_border()))
        // Import video
        .child(
            div()
                .id("import-video")
                .px_2().h(px(26.0))
                .flex().items_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(colors::accent())
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .cursor_pointer()
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ImportVideoFootage); cx.notify();
                }))
                .child("+ Import"),
        )
        // Export (PNG sequence)
        .child(
            div()
                .id("file-export")
                .cursor_pointer()
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    if root.app.export.is_none() {
                        let range = root.app.default_export_range();
                        root.app.start_export(range);
                        cx.notify();
                    }
                }))
                .child(tool_button(Icon::Export, false)),
        )
        // Export MP4
        .child(
            div()
                .id("file-export-mp4")
                .cursor_pointer()
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    if root.app.export_mp4_progress.is_none() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Export MP4…")
                            .add_filter("MP4 video", &["mp4"])
                            .save_file()
                        {
                            root.app.apply(Action::ExportMp4(path)); cx.notify();
                        }
                    }
                }))
                .child(tool_button(Icon::Film, false)),
        )
        // Export GIF
        .child(
            div()
                .id("file-export-gif")
                .px_2().h(px(26.0))
                .flex().items_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(colors::surface_overlay())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    if root.app.export_gif_progress.is_none() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Export GIF…")
                            .add_filter("GIF animation", &["gif"])
                            .save_file()
                        {
                            root.app.apply(Action::ExportGif(path)); cx.notify();
                        }
                    }
                }))
                .child("GIF"),
        )
        .children(mp4_progress_bar(app))
        .children(export_status(app, cx))
        .children(app.export_gif_progress.map(|p| {
            div().text_color(colors::text_secondary()).text_size(px(10.0))
                .child(format!("GIF {:.0}%", p * 100.0))
        }))
        .child(div().w(px(1.0)).h(px(18.0)).bg(colors::surface_border()))
        // Audio toggle
        .child(
            div()
                .id("audio-toggle")
                .cursor_pointer()
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ToggleAudioPreview); cx.notify();
                }))
                .child(tool_button(
                    if app.audio_preview_enabled { Icon::Speaker } else { Icon::Mute },
                    app.audio_preview_enabled,
                )),
        )
        // Comp Settings
        .child(
            div()
                .id("comp-settings-btn")
                .cursor_pointer()
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ToggleCompSettings); cx.notify();
                }))
                .child(tool_button(Icon::Settings, app.comp_settings_open)),
        )
        // 3D Camera shortcut button — clears layer selection so the properties
        // panel switches to the camera section.
        .child(
            div()
                .id("cam-3d-btn")
                .px_2().h(px(26.0))
                .flex().items_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(colors::surface_overlay())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child("3D Cam")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    // Deselect the layer so the properties panel shows the camera.
                    root.app.selected_layer = None;
                    cx.notify();
                })),
        )
        // RAM Preview (compact)
        .child({
            let label = if app.ram_preview_complete {
                if app.ram_preview_playing { "■ RAM" } else { "▶ RAM" }
            } else { "RAM" };
            let is_playing = app.ram_preview_playing;
            let is_complete = app.ram_preview_complete;
            div()
                .id("ram-preview-btn")
                .px_2().h(px(26.0))
                .flex().items_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(if is_playing { rgb(0x2ecc71u32) } else { colors::surface_raised() })
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child(label)
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    if is_complete {
                        if is_playing { root.app.ram_preview_playing = false; }
                        else { root.app.apply(Action::PlayRamPreview); }
                    } else { root.app.apply(Action::BuildRamPreview); }
                    cx.notify();
                }))
        })
        // Live Output toggle
        .child(
            div()
                .id("live-out-btn")
                .px_2().h(px(26.0))
                .flex().items_center()
                .rounded(px(prism_ui::radius::MD))
                .bg(if live_out { rgb(0xe5c07bu32) } else { colors::surface_raised() })
                .text_color(if live_out { rgb(0x1a1a2eu32) } else { colors::text_secondary() })
                .text_size(px(10.0))
                .cursor_pointer()
                .child("Live Out")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ToggleLiveOutput);
                    cx.notify();
                })),
        )
}

/// A simple inline progress bar for the synchronous MP4 encode (0..100%).
fn mp4_progress_bar(app: &App) -> Vec<gpui::AnyElement> {
    let Some(frac) = app.export_mp4_progress else {
        return Vec::new();
    };
    let chip = div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(colors::surface_raised())
        .text_color(colors::text_primary())
        .text_size(px(11.0))
        .relative()
        .child(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .h(px(2.0))
                .w(relative(frac))
                .bg(rgb(0x37c8c0)),
        )
        .child(format!("MP4 {:.0}%…", frac * 100.0))
        .into_any_element();
    vec![chip]
}

/// The export progress readout: a live "Exporting N/M…" while the worker runs,
/// or a final "Exported M frames" / "Export failed: …" once finished (clicking
/// the finished message dismisses it). Returns nothing when no export is active.
fn export_status(app: &App, cx: &mut Context<Pulse>) -> Vec<gpui::AnyElement> {
    let Some(progress) = app.export.as_ref() else {
        return Vec::new();
    };
    let s = progress.snapshot();
    let (label, color) = if !s.finished {
        (format!("Exporting {}/{}…", s.done, s.total), colors::text_primary())
    } else if let Some(err) = &s.error {
        (format!("Export failed: {err}"), gpui::Rgba { r: 0.878, g: 0.424, b: 0.459, a: 1.0 })
    } else {
        (format!("Exported {} frames → {}", s.done, s.dir.display()), gpui::Rgba { r: 0.596, g: 0.765, b: 0.475, a: 1.0 })
    };
    let finished = s.finished;
    let frac = s.fraction();
    let chip = div()
        .id("export-status")
        .relative()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(colors::surface_raised())
        .text_color(color)
        .text_size(px(11.0))
        .when(finished, |d| d.cursor_pointer())
        // Click the finished chip to dismiss it (a no-op while still running).
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.clear_finished_export();
            cx.notify();
        }))
        // A thin progress fill under the text while the worker runs.
        .when(!finished, |d| {
            d.child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .h(px(2.0))
                    .w(relative(frac))
                    .bg(rgb(0x37c8c0)),
            )
        })
        .child(label)
        .into_any_element();
    vec![chip]
}

