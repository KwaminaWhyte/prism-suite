//! Mixer panel — horizontal strip of channel strips, one per track.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App, TrackKind};
use crate::Tone;

pub fn render_mixer(app: &App, cx: &mut Context<Tone>) -> impl IntoElement {
    div()
        .w_full()
        .h(px(120.0))
        .bg(colors::surface_raised())
        .border_t_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_row()
        .overflow_hidden()
        // Channel strips
        .children(
            app.tracks
                .iter()
                .enumerate()
                .map(|(i, track)| {
                    let track_id = track.id;
                    let muted = track.muted;
                    let soloed = track.solo;
                    let volume = track.volume;
                    let name = track.name.clone();
                    let kind_badge = match track.kind {
                        TrackKind::Midi => "MIDI",
                        TrackKind::Audio => "AUDIO",
                        TrackKind::Instrument => "INST",
                        TrackKind::Bus => "BUS",
                        TrackKind::Master => "MSTR",
                    };
                    let _ = kind_badge;

                    // Fader height: volume is 0..=2.0, normalize to 0..=1.0 for display
                    // cap at 1.0 (unity) for normal range
                    let fader_ratio = (volume / 2.0).clamp(0.0, 1.0);
                    // fader container is 50px tall; fader fill height
                    let fader_container_h = 50.0_f32;
                    let fader_fill_h = fader_ratio * fader_container_h;

                    div()
                        .id(("channel-strip", i))
                        .w(px(64.0))
                        .h_full()
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_1()
                        .px(px(4.0))
                        .border_r_1()
                        .border_color(colors::surface_border())
                        .gap(px(3.0))
                        // Track name (top)
                        .child(
                            div()
                                .w_full()
                                .text_size(px(7.0))
                                .text_color(colors::text_secondary())
                                .overflow_hidden()
                                .child(name),
                        )
                        // Fader container
                        .child(
                            div()
                                .w(px(8.0))
                                .h(px(fader_container_h))
                                .bg(colors::surface_bg())
                                .rounded(px(2.0))
                                .flex()
                                .flex_col()
                                .justify_end()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(fader_fill_h))
                                        .bg(colors::accent())
                                        .rounded(px(2.0)),
                                ),
                        )
                        // Volume label + +/- buttons
                        .child(
                            div()
                                .text_size(px(7.0))
                                .text_color(colors::text_disabled())
                                .child(format!("VOL:{:.0}%", volume * 100.0)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .id(("vol-down", i))
                                        .w(px(14.0))
                                        .h(px(14.0))
                                        .bg(colors::surface_overlay())
                                        .rounded(px(2.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(9.0))
                                        .text_color(colors::text_primary())
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            let v = this.app.tracks.iter().find(|t| t.id == track_id).map(|t| t.volume).unwrap_or(1.0);
                                            this.app.apply(Action::SetTrackVolume { id: track_id, volume: (v - 0.1).max(0.0) });
                                            cx.notify();
                                        }))
                                        .child("−"),
                                )
                                .child(
                                    div()
                                        .id(("vol-up", i))
                                        .w(px(14.0))
                                        .h(px(14.0))
                                        .bg(colors::surface_overlay())
                                        .rounded(px(2.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(9.0))
                                        .text_color(colors::text_primary())
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            let v = this.app.tracks.iter().find(|t| t.id == track_id).map(|t| t.volume).unwrap_or(1.0);
                                            this.app.apply(Action::SetTrackVolume { id: track_id, volume: (v + 0.1).min(2.0) });
                                            cx.notify();
                                        }))
                                        .child("+"),
                                ),
                        )
                        // Mute button
                        .child(
                            div()
                                .id(("mixer-mute", i))
                                .w(px(20.0))
                                .h(px(14.0))
                                .bg(if muted { colors::accent() } else { colors::surface_overlay() })
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
                        // Solo button
                        .child(
                            div()
                                .id(("mixer-solo", i))
                                .w(px(20.0))
                                .h(px(14.0))
                                .bg(if soloed {
                                    gpui::rgb(0xf1c40f)
                                } else {
                                    colors::surface_overlay()
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
                        // Track number (bottom)
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(font_size::XS))
                                .text_color(colors::text_disabled())
                                .child(format!("#{}", i + 1)),
                        )
                })
                .collect::<Vec<_>>(),
        )
}
