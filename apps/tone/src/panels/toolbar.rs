//! Top toolbar — transport controls, BPM, time signature, metronome, project info.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App, ToneTool};
use crate::Tone;

pub fn render_toolbar(app: &App, cx: &mut Context<Tone>) -> impl IntoElement {
    let playing = app.playing;
    let recording = app.recording;
    let metronome = app.metronome_enabled;
    let bpm = app.project.bpm;
    let active_tool = app.active_tool;

    div()
        .w_full()
        .h(px(48.0))
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .flex()
        .items_center()
        .px_4()
        .gap_3()
        // Project name
        .child(
            div()
                .text_size(px(font_size::SM))
                .text_color(colors::text_primary())
                .child(app.project.name.clone()),
        )
        .child(
            div()
                .w(px(1.0))
                .h(px(28.0))
                .bg(colors::surface_border()),
        )
        // BPM display
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("BPM"),
                )
                .child(
                    div()
                        .id("bpm-dec")
                        .w(px(18.0))
                        .h(px(18.0))
                        .bg(colors::surface_overlay())
                        .rounded(px(2.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::SetBpm(bpm - 1.0));
                            cx.notify();
                        }))
                        .child("−"),
                )
                .child(
                    div()
                        .w(px(44.0))
                        .text_size(px(font_size::MD))
                        .text_color(colors::text_primary())
                        .child(format!("{:.0}", bpm)),
                )
                .child(
                    div()
                        .id("bpm-inc")
                        .w(px(18.0))
                        .h(px(18.0))
                        .bg(colors::surface_overlay())
                        .rounded(px(2.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::SetBpm(bpm + 1.0));
                            cx.notify();
                        }))
                        .child("+"),
                ),
        )
        // Time signature
        .child(
            div()
                .text_size(px(font_size::SM))
                .text_color(colors::text_secondary())
                .child(format!(
                    "{}/{}",
                    app.project.time_signature_num, app.project.time_signature_den
                )),
        )
        .child(
            div()
                .w(px(1.0))
                .h(px(28.0))
                .bg(colors::surface_border()),
        )
        // Transport: rewind
        .child(
            div()
                .id("rewind")
                .w(px(32.0))
                .h(px(32.0))
                .bg(colors::surface_overlay())
                .rounded(px(3.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .on_click(cx.listener(|this, _ev, _win, cx| {
                    this.app.apply(Action::Rewind);
                    cx.notify();
                }))
                .child("\u{23EE}"),
        )
        // Stop
        .child(
            div()
                .id("stop")
                .w(px(32.0))
                .h(px(32.0))
                .bg(colors::surface_overlay())
                .rounded(px(3.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .on_click(cx.listener(|this, _ev, _win, cx| {
                    this.app.apply(Action::Stop);
                    cx.notify();
                }))
                .child("\u{23F9}"),
        )
        // Play/Pause
        .child(
            div()
                .id("play")
                .w(px(36.0))
                .h(px(32.0))
                .bg(colors::accent())
                .rounded(px(3.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .text_color(gpui::rgb(0xffffff))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    if playing {
                        this.app.apply(Action::Pause);
                    } else {
                        this.app.apply(Action::Play);
                    }
                    cx.notify();
                }))
                .child(if playing { "\u{23F8}" } else { "\u{25B6}" }),
        )
        // Record
        .child(
            div()
                .id("record")
                .w(px(32.0))
                .h(px(32.0))
                .bg(if recording {
                    gpui::rgb(0xc0392b)
                } else {
                    colors::surface_overlay()
                })
                .rounded(px(3.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    if recording {
                        this.app.apply(Action::Stop);
                    } else {
                        this.app.apply(Action::Record);
                    }
                    cx.notify();
                }))
                .child("\u{23FA}"),
        )
        // Metronome
        .child(
            div()
                .w(px(1.0))
                .h(px(28.0))
                .bg(colors::surface_border()),
        )
        .child(
            div()
                .id("metro")
                .px_2()
                .h(px(28.0))
                .bg(if metronome {
                    colors::accent()
                } else {
                    colors::surface_overlay()
                })
                .rounded(px(3.0))
                .flex()
                .items_center()
                .text_size(px(font_size::XS))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .on_click(cx.listener(|this, _ev, _win, cx| {
                    this.app.apply(Action::ToggleMetronome);
                    cx.notify();
                }))
                .child("Metro"),
        )
        // Separator
        .child(
            div()
                .w(px(1.0))
                .h(px(28.0))
                .bg(colors::surface_border()),
        )
        // Tool selection: Select / Draw / Erase
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id("tool-select")
                        .px_2()
                        .h(px(28.0))
                        .bg(if active_tool == ToneTool::Select {
                            colors::accent()
                        } else {
                            colors::surface_overlay()
                        })
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::SetActiveTool(ToneTool::Select));
                            cx.notify();
                        }))
                        .child("Select"),
                )
                .child(
                    div()
                        .id("tool-draw")
                        .px_2()
                        .h(px(28.0))
                        .bg(if active_tool == ToneTool::Draw {
                            colors::accent()
                        } else {
                            colors::surface_overlay()
                        })
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::SetActiveTool(ToneTool::Draw));
                            cx.notify();
                        }))
                        .child("Draw"),
                )
                .child(
                    div()
                        .id("tool-erase")
                        .px_2()
                        .h(px(28.0))
                        .bg(if active_tool == ToneTool::Erase {
                            colors::accent()
                        } else {
                            colors::surface_overlay()
                        })
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::SetActiveTool(ToneTool::Erase));
                            cx.notify();
                        }))
                        .child("Erase"),
                ),
        )
        // Spacer + key/scale
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(format!("{} {}", app.project.key, app.project.scale)),
        )
}
