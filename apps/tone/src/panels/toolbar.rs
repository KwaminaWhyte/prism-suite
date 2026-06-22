//! Top toolbar — transport controls, BPM, time signature, metronome, project info.

use gpui::{
    div, svg, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size, Icon};

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
        .child(div().w(px(1.0)).h(px(28.0)).bg(colors::surface_border()))
        // BPM
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
                        .w(px(18.0)).h(px(18.0))
                        .bg(colors::surface_overlay()).rounded(px(2.0))
                        .flex().items_center().justify_center()
                        .text_size(px(font_size::XS)).text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::SetBpm(bpm - 1.0));
                            cx.notify();
                        }))
                        .child(svg().path(Icon::ArrowDown.path()).w(px(10.0)).h(px(10.0)).text_color(colors::text_primary())),
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
                        .w(px(18.0)).h(px(18.0))
                        .bg(colors::surface_overlay()).rounded(px(2.0))
                        .flex().items_center().justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::SetBpm(bpm + 1.0));
                            cx.notify();
                        }))
                        .child(svg().path(Icon::ArrowUp.path()).w(px(10.0)).h(px(10.0)).text_color(colors::text_primary())),
                ),
        )
        // Time signature
        .child(
            div()
                .text_size(px(font_size::SM))
                .text_color(colors::text_secondary())
                .child(format!("{}/{}", app.project.time_signature_num, app.project.time_signature_den)),
        )
        .child(div().w(px(1.0)).h(px(28.0)).bg(colors::surface_border()))
        // Transport
        .child(transport_btn("t-rewind", Icon::Rewind,       false, cx, |this, cx| { this.app.apply(Action::Rewind); cx.notify(); }))
        .child(transport_btn("t-stop",   Icon::Stop,         false, cx, |this, cx| { this.app.apply(Action::Stop);   cx.notify(); }))
        .child(play_pause_btn(playing, cx))
        .child(record_btn(recording, cx))
        .child(div().w(px(1.0)).h(px(28.0)).bg(colors::surface_border()))
        // Metronome
        .child(
            div()
                .id("metro")
                .w(px(32.0)).h(px(28.0))
                .bg(if metronome { colors::accent() } else { colors::surface_overlay() })
                .rounded(px(3.0))
                .flex().items_center().justify_center()
                .cursor_pointer()
                .on_click(cx.listener(|this, _ev, _win, cx| {
                    this.app.apply(Action::ToggleMetronome);
                    cx.notify();
                }))
                .child(svg().path(Icon::Music.path()).w(px(14.0)).h(px(14.0)).text_color(colors::text_primary())),
        )
        .child(div().w(px(1.0)).h(px(28.0)).bg(colors::surface_border()))
        // Tool selection: Select / Draw / Erase
        .child(
            div()
                .flex().items_center().gap_1()
                .child(tone_tool_btn("tool-select", Icon::Cursor,  ToneTool::Select, active_tool, cx))
                .child(tone_tool_btn("tool-draw",   Icon::Pencil,  ToneTool::Draw,   active_tool, cx))
                .child(tone_tool_btn("tool-erase",  Icon::Eraser,  ToneTool::Erase,  active_tool, cx)),
        )
        .child(div().flex_1())
        // Undo / Redo
        .child(transport_btn("t-undo", Icon::Undo, false, cx, |this, cx| { this.app.apply(Action::Undo); cx.notify(); }))
        .child(transport_btn("t-redo", Icon::Redo, false, cx, |this, cx| { this.app.apply(Action::Redo); cx.notify(); }))
        .child(div().w(px(1.0)).h(px(28.0)).bg(colors::surface_border()))
        // Key / scale
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(format!("{} {}", app.project.key, app.project.scale)),
        )
}

fn transport_btn(
    id: &'static str,
    icon: Icon,
    accent: bool,
    cx: &mut Context<Tone>,
    cb: impl Fn(&mut Tone, &mut Context<Tone>) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(id))
        .w(px(32.0)).h(px(32.0))
        .bg(if accent { colors::accent() } else { colors::surface_overlay() })
        .rounded(px(3.0))
        .flex().items_center().justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| cb(this, cx)))
        .child(svg().path(icon.path()).w(px(14.0)).h(px(14.0)).text_color(colors::text_primary()))
}

fn play_pause_btn(playing: bool, cx: &mut Context<Tone>) -> impl IntoElement {
    let icon = if playing { Icon::Pause } else { Icon::Play };
    div()
        .id("t-play")
        .w(px(36.0)).h(px(32.0))
        .bg(colors::accent()).rounded(px(3.0))
        .flex().items_center().justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            if playing { this.app.apply(Action::Pause); } else { this.app.apply(Action::Play); }
            cx.notify();
        }))
        .child(svg().path(icon.path()).w(px(15.0)).h(px(15.0)).text_color(gpui::white()))
}

fn record_btn(recording: bool, cx: &mut Context<Tone>) -> impl IntoElement {
    div()
        .id("t-record")
        .w(px(32.0)).h(px(32.0))
        .bg(if recording { gpui::rgb(0xc0392b) } else { colors::surface_overlay() })
        .rounded(px(3.0))
        .flex().items_center().justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            if recording { this.app.apply(Action::Stop); } else { this.app.apply(Action::Record); }
            cx.notify();
        }))
        .child(svg().path(Icon::Waveform.path()).w(px(14.0)).h(px(14.0)).text_color(if recording { gpui::rgb(0xffffff) } else { colors::text_primary() }))
}

fn tone_tool_btn(
    id: &'static str,
    icon: Icon,
    tool: ToneTool,
    active: ToneTool,
    cx: &mut Context<Tone>,
) -> impl IntoElement {
    let is_active = tool == active;
    div()
        .id(gpui::SharedString::from(id))
        .w(px(30.0)).h(px(28.0))
        .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
        .rounded(px(3.0))
        .flex().items_center().justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            this.app.apply(Action::SetActiveTool(tool));
            cx.notify();
        }))
        .child(svg().path(icon.path()).w(px(14.0)).h(px(14.0)).text_color(if is_active { colors::text_primary() } else { colors::text_secondary() }))
}
