//! Inspector strips — typeable numeric fields for the inspected clip and the
//! active track.
//!
//! These wrap `prism_ui::TextField`s owned by the root [`Tone`] view (bound to
//! `numeric_clip_id` / `numeric_track_id`). Parsing + clamping happen in the
//! field `on_submit` handlers; this module only lays out the labelled rows and a
//! pair of stepper buttons per value so the existing click-to-nudge affordance
//! survives alongside typing.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size, TextField};

use crate::app_state::{Action, App};
use crate::Tone;

/// A compact clip inspector: gain (dB), pitch (st), length (beats) for the clip
/// currently open in the piano roll. Rendered in the piano-roll bottom split.
#[allow(clippy::too_many_arguments)]
pub fn render_clip_inspector(
    app: &App,
    gain_field: Entity<TextField>,
    pitch_field: Entity<TextField>,
    length_field: Entity<TextField>,
    cx: &mut Context<Tone>,
) -> impl IntoElement {
    let clip_id = app.piano_roll_clip;
    let clip = clip_id.and_then(|id| app.clips.iter().find(|c| c.id == id));

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1()
        .child(
            div()
                .text_size(px(7.0))
                .text_color(colors::text_disabled())
                .child("CLIP"),
        )
        // Gain (dB)
        .child(numeric_row(
            "GAIN",
            gain_field,
            ("clip-gain-dn", "clip-gain-up"),
            cx,
            move |this, _cx| {
                if let Some(id) = this.numeric_clip_id {
                    let cur = this.app.clips.iter().find(|c| c.id == id).map(|c| c.gain).unwrap_or(1.0);
                    let db = if cur > 0.0 { 20.0 * cur.log10() } else { -60.0 };
                    this.app.apply(Action::SetClipGainDb { clip_id: id, gain_db: db - 1.0 });
                }
            },
            move |this, _cx| {
                if let Some(id) = this.numeric_clip_id {
                    let cur = this.app.clips.iter().find(|c| c.id == id).map(|c| c.gain).unwrap_or(1.0);
                    let db = if cur > 0.0 { 20.0 * cur.log10() } else { -60.0 };
                    this.app.apply(Action::SetClipGainDb { clip_id: id, gain_db: db + 1.0 });
                }
            },
        ))
        // Pitch (semitones)
        .child(numeric_row(
            "PITCH",
            pitch_field,
            ("clip-pitch-dn", "clip-pitch-up"),
            cx,
            move |this, _cx| {
                if let Some(id) = this.numeric_clip_id {
                    let cur = this.app.clips.iter().find(|c| c.id == id).map(|c| c.pitch_shift_f32).unwrap_or(0.0);
                    this.app.apply(Action::SetClipPitchF32 { clip_id: id, semitones: cur - 1.0 });
                }
            },
            move |this, _cx| {
                if let Some(id) = this.numeric_clip_id {
                    let cur = this.app.clips.iter().find(|c| c.id == id).map(|c| c.pitch_shift_f32).unwrap_or(0.0);
                    this.app.apply(Action::SetClipPitchF32 { clip_id: id, semitones: cur + 1.0 });
                }
            },
        ))
        // Length (beats)
        .child(numeric_row(
            "LEN",
            length_field,
            ("clip-len-dn", "clip-len-up"),
            cx,
            move |this, _cx| {
                if let Some(id) = this.numeric_clip_id {
                    let cur = this.app.clips.iter().find(|c| c.id == id).map(|c| c.duration_beats).unwrap_or(4.0);
                    this.app.apply(Action::ResizeClip { id, duration_beats: cur - 1.0 });
                }
            },
            move |this, _cx| {
                if let Some(id) = this.numeric_clip_id {
                    let cur = this.app.clips.iter().find(|c| c.id == id).map(|c| c.duration_beats).unwrap_or(4.0);
                    this.app.apply(Action::ResizeClip { id, duration_beats: cur + 1.0 });
                }
            },
        ))
        // Footnote when nothing is selected
        .when(clip.is_none(), |d| {
            d.child(
                div()
                    .text_size(px(7.0))
                    .text_color(colors::text_disabled())
                    .child("Select a clip"),
            )
        })
}

/// A compact track inspector: typeable volume (%) and pan (L/C/R) for the active
/// track. Rendered at the left of the mixer strip.
pub fn render_track_inspector(
    app: &App,
    vol_field: Entity<TextField>,
    pan_field: Entity<TextField>,
    cx: &mut Context<Tone>,
) -> impl IntoElement {
    let active = app.active_track;
    let name = active
        .and_then(|id| app.tracks.iter().find(|t| t.id == id))
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "No track".to_string());

    div()
        .w(px(132.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1()
        .border_r_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .text_size(px(7.0))
                .text_color(colors::text_secondary())
                .overflow_hidden()
                .child(name),
        )
        .child(numeric_row(
            "VOL",
            vol_field,
            ("trk-vol-dn", "trk-vol-up"),
            cx,
            move |this, _cx| {
                if let Some(id) = this.numeric_track_id {
                    let v = this.app.tracks.iter().find(|t| t.id == id).map(|t| t.volume).unwrap_or(1.0);
                    this.app.apply(Action::SetTrackVolume { id, volume: (v - 0.05).max(0.0) });
                }
            },
            move |this, _cx| {
                if let Some(id) = this.numeric_track_id {
                    let v = this.app.tracks.iter().find(|t| t.id == id).map(|t| t.volume).unwrap_or(1.0);
                    this.app.apply(Action::SetTrackVolume { id, volume: (v + 0.05).min(2.0) });
                }
            },
        ))
        .child(numeric_row(
            "PAN",
            pan_field,
            ("trk-pan-dn", "trk-pan-up"),
            cx,
            move |this, _cx| {
                if let Some(id) = this.numeric_track_id {
                    let p = this.app.tracks.iter().find(|t| t.id == id).map(|t| t.pan).unwrap_or(0.0);
                    this.app.apply(Action::SetTrackPan { id, pan: (p - 0.1).max(-1.0) });
                }
            },
            move |this, _cx| {
                if let Some(id) = this.numeric_track_id {
                    let p = this.app.tracks.iter().find(|t| t.id == id).map(|t| t.pan).unwrap_or(0.0);
                    this.app.apply(Action::SetTrackPan { id, pan: (p + 0.1).min(1.0) });
                }
            },
        ))
}

/// A labelled numeric row: `LABEL [−] <field> [+]`. The +/− steppers nudge the
/// value and re-seed the relevant fields via `sync_numeric_fields` on the next
/// render; the field itself parses on Enter.
#[allow(clippy::too_many_arguments)]
fn numeric_row(
    label: &'static str,
    field: Entity<TextField>,
    ids: (&'static str, &'static str),
    cx: &mut Context<Tone>,
    on_dec: impl Fn(&mut Tone, &mut Context<Tone>) + 'static,
    on_inc: impl Fn(&mut Tone, &mut Context<Tone>) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .w(px(30.0))
                .flex_shrink_0()
                .text_size(px(7.0))
                .text_color(colors::text_disabled())
                .child(label),
        )
        .child(stepper(ids.0, "−", cx, on_dec))
        .child(field)
        .child(stepper(ids.1, "+", cx, on_inc))
}

fn stepper(
    id: &'static str,
    glyph: &'static str,
    cx: &mut Context<Tone>,
    cb: impl Fn(&mut Tone, &mut Context<Tone>) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(id))
        .w(px(14.0))
        .h(px(14.0))
        .flex_shrink_0()
        .bg(colors::surface_overlay())
        .rounded(px(2.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(font_size::XS))
        .text_color(colors::text_primary())
        .cursor_pointer()
        .on_click(cx.listener(move |this, _ev, _win, cx| {
            cb(this, cx);
            cx.notify();
        }))
        .child(glyph)
}
