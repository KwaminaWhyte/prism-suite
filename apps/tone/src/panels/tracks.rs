//! Track list panel — left sidebar showing tracks with M/S/R controls.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App, TrackKind};
use crate::Tone;

pub fn render_tracks(app: &App, cx: &mut Context<Tone>) -> impl IntoElement {
    div()
        .w(px(220.0))
        .h_full()
        .bg(colors::surface_raised())
        .border_r_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        // Header
        .child(
            div()
                .w_full()
                .h(px(28.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("TRACKS"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        // [A] Audio track button
                        .child(
                            div()
                                .id("add-audio-track")
                                .w(px(20.0))
                                .h(px(20.0))
                                .bg(colors::surface_overlay())
                                .rounded(px(2.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(7.0))
                                .text_color(colors::text_secondary())
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _ev, _win, cx| {
                                    this.app.apply(Action::AddTrack(TrackKind::Audio));
                                    cx.notify();
                                }))
                                .child("A"),
                        )
                        // [M] MIDI track button
                        .child(
                            div()
                                .id("add-midi-track")
                                .w(px(20.0))
                                .h(px(20.0))
                                .bg(colors::surface_overlay())
                                .rounded(px(2.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(7.0))
                                .text_color(colors::text_secondary())
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _ev, _win, cx| {
                                    this.app.apply(Action::AddTrack(TrackKind::Midi));
                                    cx.notify();
                                }))
                                .child("M"),
                        ),
                ),
        )
        // Track rows
        .child(
            div()
                .id("tracks-scroll")
                .flex_1()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children(
                    app.tracks
                        .iter()
                        .enumerate()
                        .map(|(idx, track)| {
                            let track_id = track.id;
                            let muted = track.muted;
                            let soloed = track.solo;
                            let armed = track.armed;
                            let name = track.name.clone();
                            let is_active = app.active_track == Some(track_id);
                            let kind_char = match track.kind {
                                TrackKind::Audio => "A",
                                TrackKind::Midi => "M",
                                TrackKind::Instrument => "I",
                                TrackKind::Bus => "B",
                                TrackKind::Master => "\u{2605}",
                            };
                            let vol_display = format!("{:.0}", track.volume * 100.0);

                            div()
                                .id(("track-row", idx))
                                .w_full()
                                .h(px(56.0))
                                .px_2()
                                .py_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .bg(if is_active {
                                    colors::surface_overlay()
                                } else {
                                    colors::surface_raised()
                                })
                                .border_b_1()
                                .border_color(colors::surface_border())
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                    this.app.apply(Action::SetActiveTrack(Some(track_id)));
                                    cx.notify();
                                }))
                                // Top row: kind badge + name
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .w(px(14.0))
                                                .h(px(14.0))
                                                .bg(colors::surface_bg())
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(7.0))
                                                .text_color(colors::text_secondary())
                                                .child(kind_char),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_size(px(font_size::XS))
                                                .text_color(if is_active {
                                                    colors::text_primary()
                                                } else {
                                                    colors::text_secondary()
                                                })
                                                .overflow_hidden()
                                                .child(name),
                                        ),
                                )
                                // Bottom row: M/S/R buttons + volume
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .id(("mute", idx))
                                                .w(px(16.0))
                                                .h(px(16.0))
                                                .bg(if muted {
                                                    colors::accent()
                                                } else {
                                                    colors::surface_bg()
                                                })
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(7.0))
                                                .text_color(if muted {
                                                    gpui::rgb(0xffffff)
                                                } else {
                                                    colors::text_disabled()
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    this.app.apply(Action::SetTrackMute {
                                                        id: track_id,
                                                        muted: !muted,
                                                    });
                                                    cx.notify();
                                                }))
                                                .child("M"),
                                        )
                                        .child(
                                            div()
                                                .id(("solo", idx))
                                                .w(px(16.0))
                                                .h(px(16.0))
                                                .bg(if soloed {
                                                    gpui::rgb(0xf1c40f)
                                                } else {
                                                    colors::surface_bg()
                                                })
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(7.0))
                                                .text_color(if soloed {
                                                    gpui::rgb(0x000000)
                                                } else {
                                                    colors::text_disabled()
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    this.app.apply(Action::SetTrackSolo {
                                                        id: track_id,
                                                        solo: !soloed,
                                                    });
                                                    cx.notify();
                                                }))
                                                .child("S"),
                                        )
                                        .child(
                                            div()
                                                .id(("arm", idx))
                                                .w(px(16.0))
                                                .h(px(16.0))
                                                .bg(if armed {
                                                    gpui::rgb(0xe74c3c)
                                                } else {
                                                    colors::surface_bg()
                                                })
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(7.0))
                                                .text_color(if armed {
                                                    gpui::rgb(0xffffff)
                                                } else {
                                                    colors::text_disabled()
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    this.app.apply(Action::SetTrackArm {
                                                        id: track_id,
                                                        armed: !armed,
                                                    });
                                                    cx.notify();
                                                }))
                                                .child("R"),
                                        )
                                        .child(div().flex_1())
                                        .child(
                                            div()
                                                .text_size(px(7.0))
                                                .text_color(colors::text_disabled())
                                                .child(vol_display),
                                        )
                                        .child(
                                            div()
                                                .id(("vol-down", idx))
                                                .w(px(14.0))
                                                .h(px(14.0))
                                                .bg(colors::surface_overlay())
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(8.0))
                                                .text_color(colors::text_primary())
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    let v = this.app.tracks.iter().find(|t| t.id == track_id).map(|t| t.volume).unwrap_or(1.0);
                                                    this.app.apply(Action::SetTrackVolume { id: track_id, volume: (v - 0.1).max(0.0) });
                                                    cx.notify();
                                                }))
                                                .child("\u{2212}"),
                                        )
                                        .child(
                                            div()
                                                .id(("vol-up", idx))
                                                .w(px(14.0))
                                                .h(px(14.0))
                                                .bg(colors::surface_overlay())
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(8.0))
                                                .text_color(colors::text_primary())
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    let v = this.app.tracks.iter().find(|t| t.id == track_id).map(|t| t.volume).unwrap_or(1.0);
                                                    this.app.apply(Action::SetTrackVolume { id: track_id, volume: (v + 0.1).min(2.0) });
                                                    cx.notify();
                                                }))
                                                .child("+"),
                                        )
                                        .child(
                                            div()
                                                .id(("del-track", idx))
                                                .w(px(20.0))
                                                .h(px(14.0))
                                                .bg(colors::surface_overlay())
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(7.0))
                                                .text_color(colors::text_disabled())
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    this.app.apply(Action::DeleteTrack(track_id));
                                                    cx.notify();
                                                }))
                                                .child("\u{00d7}"),
                                        ),
                                )
                        })
                        .collect::<Vec<_>>(),
                ),
        )
}
