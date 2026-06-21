//! Piano roll panel — piano keys on the left, MIDI note grid in the center.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size};

use crate::app_state::App;
use crate::Tone;

const PIANO_KEY_W: f32 = 36.0;
const NOTE_ROW_H: f32 = 12.0;
const NUM_KEYS: usize = 48; // 4 octaves visible
const VISIBLE_BEATS: f32 = 16.0;

static NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

fn is_black_key(midi: usize) -> bool {
    matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
}

pub fn render_piano_roll(app: &App, _cx: &mut Context<Tone>) -> impl IntoElement {
    // Capture midi notes for the focused clip
    let clip_id = app.piano_roll_clip;

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        // Beat ruler
        .child(
            div()
                .w_full()
                .h(px(24.0))
                .bg(colors::surface_bg())
                .border_b_1()
                .border_color(colors::surface_border())
                .flex()
                .items_center()
                .pl(px(PIANO_KEY_W))
                .children((1..=16u32).map(|beat| {
                    div()
                        .flex_1()
                        .h_full()
                        .border_l_1()
                        .border_color(colors::surface_border())
                        .flex()
                        .items_end()
                        .pb_1()
                        .pl_1()
                        .text_size(px(8.0))
                        .text_color(colors::text_disabled())
                        .child(format!("{beat}"))
                })),
        )
        // Piano keys + note grid
        .child(
            div()
                .id("piano-roll-scroll")
                .flex_1()
                .flex()
                .flex_row()
                .overflow_y_scroll()
                .overflow_hidden()
                // Piano keys (left strip)
                .child(
                    div()
                        .w(px(PIANO_KEY_W))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .bg(colors::surface_raised())
                        .border_r_1()
                        .border_color(colors::surface_border())
                        .children((0..NUM_KEYS).rev().map(|i| {
                            let midi_note = 48 + i;
                            let note_in_oct = midi_note % 12;
                            let is_black = is_black_key(midi_note);
                            let is_c = note_in_oct == 0;
                            let label = if is_c {
                                format!(
                                    "{}{}",
                                    NOTE_NAMES[note_in_oct],
                                    (midi_note / 12) as i32 - 1
                                )
                            } else {
                                String::new()
                            };

                            div()
                                .w_full()
                                .h(px(NOTE_ROW_H))
                                .bg(if is_black {
                                    gpui::rgb(0x111114)
                                } else {
                                    colors::surface_raised()
                                })
                                .border_b_1()
                                .border_color(colors::surface_border())
                                .flex()
                                .items_center()
                                .justify_end()
                                .pr_1()
                                .child(
                                    div()
                                        .text_size(px(7.0))
                                        .text_color(colors::text_disabled())
                                        .child(label),
                                )
                        })),
                )
                // Note grid
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .children((0..NUM_KEYS).rev().map(|i| {
                            let midi_note = (48 + i) as u8;
                            let note_in_oct = midi_note % 12;
                            let is_black = is_black_key(midi_note as usize);
                            let is_c = note_in_oct == 0;

                            div()
                                .w_full()
                                .h(px(NOTE_ROW_H))
                                .bg(if is_black {
                                    gpui::rgb(0x151518)
                                } else {
                                    colors::surface_bg()
                                })
                                .border_b_1()
                                .border_color(if is_c {
                                    colors::surface_overlay()
                                } else {
                                    colors::surface_border()
                                })
                                .relative()
                        })),
                ),
        )
        // Empty state overlay when no clip is focused
        .when(clip_id.is_none(), |d| {
            d.child(
                div()
                    .absolute()
                    .w_full()
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .child("Double-click a MIDI clip to edit"),
            )
        })
}
