//! Timeline strip — bar ruler + clip lanes per track.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App, ClipKind, TrackKind};
use crate::Tone;

const LABEL_W: f32 = 220.0;
const VISIBLE_BARS: f32 = 32.0;

pub fn render_timeline(app: &App, cx: &mut Context<Tone>) -> impl IntoElement {
    div()
        .w_full()
        .h_full()
        .bg(colors::surface_raised())
        .border_t_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        // Bar ruler
        .child(
            div()
                .w_full()
                .h(px(20.0))
                .bg(colors::surface_bg())
                .border_b_1()
                .border_color(colors::surface_border())
                .flex()
                .items_center()
                .pl(px(LABEL_W))
                .children((1..=32u32).map(|bar| {
                    div()
                        .flex_1()
                        .h_full()
                        .border_l_1()
                        .border_color(colors::surface_border())
                        .flex()
                        .items_end()
                        .pb(px(2.0))
                        .pl_1()
                        .text_size(px(8.0))
                        .text_color(colors::text_disabled())
                        .child(format!("{bar}"))
                })),
        )
        // Track lanes
        .child(
            div()
                .id("timeline-scroll")
                .flex_1()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children(
                    app.tracks
                        .iter()
                        .enumerate()
                        .map(|(lane_idx, track)| {
                            let track_name = track.name.clone();
                            let track_id = track.id;
                            let is_midi = matches!(track.kind, TrackKind::Midi | TrackKind::Instrument);
                            let piano_roll_clip = app.piano_roll_clip;
                            let kind_badge = match track.kind {
                                TrackKind::Midi => "MIDI",
                                TrackKind::Audio => "AUDIO",
                                TrackKind::Instrument => "INST",
                                TrackKind::Bus => "BUS",
                                TrackKind::Master => "MSTR",
                            };
                            let track_clips: Vec<_> = app
                                .clips
                                .iter()
                                .map(|c| (c.id, c.track_id, c.start_beat, c.duration_beats, c.name.clone(), c.muted))
                                .filter(|(_, tid, _, _, _, _)| *tid == track_id)
                                .collect();

                            div()
                                .w_full()
                                .h(px(40.0))
                                .flex()
                                .items_center()
                                .border_b_1()
                                .border_color(colors::surface_border())
                                // Track label cell
                                .child(
                                    div()
                                        .w(px(LABEL_W))
                                        .h_full()
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .border_r_1()
                                        .border_color(colors::surface_border())
                                        .flex_shrink_0()
                                        .overflow_hidden()
                                        // Kind badge
                                        .child(
                                            div()
                                                .px(px(3.0))
                                                .h(px(12.0))
                                                .bg(colors::surface_overlay())
                                                .rounded(px(2.0))
                                                .flex()
                                                .items_center()
                                                .text_size(px(7.0))
                                                .text_color(colors::text_disabled())
                                                .flex_shrink_0()
                                                .child(kind_badge),
                                        )
                                        // Track name
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_size(px(font_size::XS))
                                                .text_color(colors::text_secondary())
                                                .overflow_hidden()
                                                .child(track_name),
                                        ),
                                )
                                // Clip lane — click empty area to add clip; click clip block to open piano roll
                                .child(
                                    div()
                                        .id(("clip-lane", lane_idx))
                                        .flex_1()
                                        .h_full()
                                        .relative()
                                        .overflow_hidden()
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            // Only add a clip if the lane has none yet — avoids
                                            // a click-through problem where clicking a clip also
                                            // fires the lane handler. The clip's own on_click
                                            // handles the "already has a clip" case.
                                            let has_clip = this.app.clips.iter()
                                                .any(|c| c.track_id == track_id);
                                            if !has_clip {
                                                let clip_kind = if is_midi { ClipKind::Midi } else { ClipKind::Audio };
                                                this.app.apply(Action::AddClip {
                                                    track_id,
                                                    name: "Clip 1".to_string(),
                                                    kind: clip_kind,
                                                    start_beat: 0.0,
                                                    duration_beats: 4.0,
                                                });
                                                cx.notify();
                                            }
                                        }))
                                        // Empty lane placeholder
                                        .when(track_clips.is_empty(), |d| {
                                            d.child(
                                                div()
                                                    .absolute()
                                                    .left(px(8.0))
                                                    .top(px(4.0))
                                                    .text_size(px(font_size::XS))
                                                    .text_color(colors::text_disabled())
                                                    .child("\u{2014}"),
                                            )
                                        })
                                        .children(track_clips.iter().map(|(clip_id, _, start_beat, duration_beats, clip_name, muted)| {
                                            let clip_id = *clip_id;
                                            let start_pct = (start_beat / VISIBLE_BARS).clamp(0.0, 1.0);
                                            let width_pct = (duration_beats / VISIBLE_BARS).max(0.02);
                                            let is_selected = piano_roll_clip == Some(clip_id);

                                            div()
                                                .id(("clip", clip_id))
                                                .absolute()
                                                .left(gpui::relative(start_pct))
                                                .w(gpui::relative(width_pct))
                                                .top(px(2.0))
                                                .bottom(px(2.0))
                                                .bg(if is_selected {
                                                    colors::accent()
                                                } else {
                                                    colors::accent()
                                                })
                                                .rounded(px(2.0))
                                                .opacity(if *muted { 0.3 } else { if is_selected { 1.0 } else { 0.7 } })
                                                .overflow_hidden()
                                                .text_size(px(7.0))
                                                .text_color(gpui::rgb(0xffffff))
                                                .pl_1()
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    this.app.apply(Action::OpenPianoRoll(clip_id));
                                                    cx.notify();
                                                }))
                                                .child(clip_name.clone())
                                        })),
                                )
                        })
                        .collect::<Vec<_>>(),
                ),
        )
}
