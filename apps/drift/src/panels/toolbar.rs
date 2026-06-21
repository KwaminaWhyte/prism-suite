//! Toolbar panel for Drift — tool buttons + playback controls.
//!
//! Left cluster: tool buttons (Select, Move, Pen, Rect, Ellipse).
//! Center: playback transport (|< < ▶/⏸ > >|) + frame counter + FPS readout.
//! Right: document name.

use crate::app_state::{Action, App};
use crate::Drift;
use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size};

pub fn render_toolbar(app: &App, cx: &mut Context<Drift>) -> impl IntoElement {
    let playing = app.playing;
    let current = app.current_frame;
    let total = app.document.duration_frames;
    let fps = app.document.fps as u32;
    let doc_name = app.document.name.clone();

    div()
        .w_full()
        .h(px(44.0))
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .flex()
        .items_center()
        .px_3()
        .gap_2()
        // Left: tool buttons
        .child(tool_btn("V", "Select"))
        .child(tool_btn("M", "Move"))
        .child(tool_btn("P", "Pen"))
        .child(tool_btn("R", "Rect"))
        .child(tool_btn("E", "Ellipse"))
        // Divider
        .child(
            div()
                .w(px(1.0))
                .h(px(24.0))
                .bg(colors::surface_border())
                .mx_2(),
        )
        // Center: playback controls
        .child(playback_btn("|<"))
        .child(playback_btn("<"))
        .child(play_pause_btn(playing, cx))
        .child(playback_btn(">"))
        .child(playback_btn(">|"))
        // Frame counter
        .child(
            div()
                .mx_3()
                .text_size(px(font_size::SM))
                .text_color(colors::text_primary())
                .child(format!("{current} / {total}")),
        )
        // FPS readout
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(format!("{fps}fps")),
        )
        // Spacer
        .child(div().flex_1())
        // Right: doc name
        .child(
            div()
                .text_size(px(font_size::SM))
                .text_color(colors::text_secondary())
                .child(doc_name),
        )
}

fn tool_btn(label: &'static str, _title: &'static str) -> impl IntoElement {
    div()
        .id(SharedString::from(label.to_string()))
        .w(px(28.0))
        .h(px(28.0))
        .bg(colors::surface_overlay())
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(font_size::XS))
        .text_color(colors::text_primary())
        .cursor_pointer()
        .child(label)
}

fn playback_btn(label: &'static str) -> impl IntoElement {
    div()
        .id(SharedString::from(label.to_string()))
        .px_2()
        .h(px(28.0))
        .bg(colors::surface_overlay())
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(font_size::XS))
        .text_color(colors::text_primary())
        .cursor_pointer()
        .child(label)
}

fn play_pause_btn(playing: bool, cx: &mut Context<Drift>) -> impl IntoElement {
    let label = if playing { "⏸" } else { "▶" };
    div()
        .id("play-pause")
        .px_3()
        .h(px(28.0))
        .bg(colors::accent())
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(font_size::SM))
        .text_color(gpui::white())
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            if playing {
                this.app.apply(Action::Pause);
            } else {
                this.app.apply(Action::Play);
            }
            cx.notify();
        }))
        .child(label)
}
