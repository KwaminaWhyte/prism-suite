//! Tracks + clips panel — the real read-only panel reading project state.
//!
//! WORKING EXAMPLE of the Action round-trip. Two interactions are wired:
//!   - Click a track's eye dot → `Action::ToggleTrackEnabled(ti)` (re-samples
//!     the program frame).
//!   - Click a clip row        → `Action::SelectClip(i)`.
//! Both go through `root.app.apply(...)` + `cx.notify()` (see `panels/mod.rs`).
//!
//! Tracks are listed top-down (highest track index first, mirroring the layer
//! stack), each followed by its clips. A clip active at the playhead is badged
//! "live".

use gpui::{
    div, px, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, badge, section_header, Icon};

use crate::app_state::{Action, App};
use crate::panels::TextFields;
use crate::Reel;

pub fn render(app: &App, fields: &TextFields, cx: &mut Context<Reel>) -> impl IntoElement {
    let t = app.time;
    let selected = app.selected;

    // Build, per track (top track first), a header row + its clip rows.
    let mut rows: Vec<gpui::AnyElement> = Vec::new();
    for ti in (0..app.project.tracks.len()).rev() {
        let track = &app.project.tracks[ti];
        let eye_color = if track.enabled { colors::text_primary() } else { colors::text_disabled() };

        // Track header: name + eye toggle (Eye / EyeOff icon).
        rows.push(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .bg(colors::surface_overlay())
                .border_b_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .child(
                    div()
                        .id(("track-eye", ti))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleTrackEnabled(ti));
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path(if track.enabled { Icon::Eye.path() } else { Icon::EyeOff.path() })
                                .w(px(14.0))
                                .h(px(14.0))
                                .text_color(eye_color),
                        ),
                )
                .child(
                    div().flex_1().children(
                        fields.get(&format!("track-name-{ti}")).cloned()
                    ).when(
                        !fields.contains_key(&format!("track-name-{ti}")),
                        |d| d.child(track.name.clone()),
                    ),
                )
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(format!("track {ti}")),
                )
                .into_any_element(),
        );

        // Clip rows on this track, left to right.
        let mut on_track: Vec<(usize, &crate::app_state::Clip)> = app
            .project
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| c.track == ti)
            .collect();
        on_track.sort_by(|a, b| a.1.start.partial_cmp(&b.1.start).unwrap());

        for (i, clip) in on_track {
            let is_sel = Some(i) == selected;
            let is_live = clip.covers(t) && track.enabled;
            let swatch = clip.source.block_color();

            rows.push(
                div()
                    .id(("clip-row", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl(px(28.0))
                    .pr_3()
                    .py_2()
                    .bg(if is_sel { colors::tool_hover() } else { colors::surface_raised() })
                    .text_color(colors::text_primary())
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::SelectClip(i));
                        cx.notify();
                    }))
                    .child(
                        div().w(px(10.0)).h(px(10.0)).rounded_sm().bg(gpui::rgb(rgb_u32(swatch))),
                    )
                    .child(
                        div().flex_1().text_size(px(12.0)).children(
                            fields.get(&format!("clip-name-{i}")).cloned()
                        ).when(
                            !fields.contains_key(&format!("clip-name-{i}")),
                            |d| d.child(clip.name.clone()),
                        ),
                    )
                    .child(
                        div()
                            .text_color(colors::text_secondary())
                            .text_size(px(10.0))
                            .child(format!("{:.1}s", clip.duration)),
                    )
                    .when(is_live, |d| {
                        d.child(badge("live"))
                    })
                    .into_any_element(),
            );
        }
    }

    div()
        .flex_1()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(section_header("Tracks & Clips"))
        .children(rows)
}

/// Pack a straight-sRGB `[r, g, b]` 0..1 into a 0xRRGGBB for `rgb(...)`.
fn rgb_u32(c: [f32; 3]) -> u32 {
    let to = |x: f32| ((x.clamp(0.0, 1.0) * 255.0).round() as u32) & 0xff;
    (to(c[0]) << 16) | (to(c[1]) << 8) | to(c[2])
}
