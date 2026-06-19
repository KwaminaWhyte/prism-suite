//! Source Viewer panel — shows a source clip's playhead, in/out points, and
//! insert-to-timeline controls.
//!
//! Shown when `app.dual_viewer` is true. Reads `app.source_*` fields and
//! emits source-viewer actions (`SeekSource`, `ToggleSourcePlay`, `SetSourceIn`,
//! `SetSourceOut`, `InsertFromSource`).

use gpui::prelude::FluentBuilder;
use gpui::{div, px, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, Icon};

use crate::app_state::{Action, App};
use crate::Reel;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let source_t = app.source_playhead;
    let source_in = app.source_in;
    let source_out = app.source_out;
    let source_playing = app.source_playing;
    let dur = app.project.duration.max(0.001);

    let play_icon = if source_playing { Icon::Pause } else { Icon::Play };

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Source Viewer"))
        .child(
            div()
                .flex()
                .flex_col()
                .p_2()
                .gap_1()
                // Timecode readout.
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(format!(
                            "{:.2}s  In:{:.2}s  Out:{:.2}s",
                            source_t, source_in, source_out
                        )),
                )
                // Transport row.
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        // Play/pause.
                        .child(
                            div()
                                .id("src-play")
                                .cursor_pointer()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(colors::surface_overlay())
                                .flex()
                                .items_center()
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::ToggleSourcePlay);
                                    cx.notify();
                                }))
                                .child(
                                    svg()
                                        .path(play_icon.path())
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .text_color(colors::text_primary()),
                                ),
                        )
                        // Mark in.
                        .child(
                            div()
                                .id("src-in")
                                .cursor_pointer()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_primary())
                                .text_size(px(10.0))
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::SetSourceIn(root.app.source_playhead));
                                    cx.notify();
                                }))
                                .child("Mark In"),
                        )
                        // Mark out.
                        .child(
                            div()
                                .id("src-out")
                                .cursor_pointer()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_primary())
                                .text_size(px(10.0))
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::SetSourceOut(root.app.source_playhead));
                                    cx.notify();
                                }))
                                .child("Mark Out"),
                        )
                        // Insert at playhead.
                        .child(
                            div()
                                .id("src-insert")
                                .w(px(28.0))
                                .h(px(28.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .rounded_md()
                                .bg(colors::accent())
                                .when(app.source_clip.is_some(), |el| {
                                    el.on_click(cx.listener(move |root, _ev, _win, cx| {
                                        if let Some(clip_id) = root.app.source_clip {
                                            root.app.apply(Action::InsertFromSource {
                                                clip_id,
                                                in_t: root.app.source_in,
                                                out_t: root.app.source_out,
                                            });
                                            cx.notify();
                                        }
                                    }))
                                })
                                .child(
                                    svg()
                                        .path(Icon::Add.path())
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .text_color(colors::text_primary()),
                                ),
                        ),
                )
                // Mini scrub bar.
                .child(
                    div()
                        .id("src-scrub")
                        .w_full()
                        .h(px(6.0))
                        .bg(colors::surface_overlay())
                        .rounded_sm()
                        .relative()
                        .cursor_pointer()
                        .child(
                            // In-out range highlight.
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(gpui::relative((source_in / dur).clamp(0.0, 1.0)))
                                .w(gpui::relative(
                                    ((source_out - source_in) / dur).clamp(0.0, 1.0),
                                ))
                                .bg(colors::accent())
                                .opacity(0.4),
                        )
                        .child(
                            // Playhead pip.
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(gpui::relative((source_t / dur).clamp(0.0, 1.0)))
                                .w(px(2.0))
                                .bg(colors::danger()),
                        ),
                )
                // Multicam group controls (when multicam mode is active)
                .when(app.multicam_mode, |el| {
                    let n_groups = app.multicam_groups.len();
                    let group_items: Vec<gpui::AnyElement> = app.multicam_groups
                        .iter()
                        .enumerate()
                        .map(|(gi, grp)| {
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .text_color(colors::text_secondary())
                                        .text_size(px(10.0))
                                        .child(format!("Group {} ({} clips, angle {})",
                                            gi, grp.clips.len(), grp.active_angle)),
                                )
                                .into_any_element()
                        })
                        .collect();
                    el.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .pt_1()
                            .child(
                                div()
                                    .text_color(colors::text_secondary())
                                    .text_size(px(10.0))
                                    .child(format!("Multicam ({} groups)", n_groups)),
                            )
                            .child(
                                div()
                                    .id("create-mcam-group")
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(colors::accent())
                                    .text_color(colors::text_primary())
                                    .text_size(px(10.0))
                                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                                        root.app.apply(Action::CreateMulticamGroup {
                                            name: format!("Group {}", root.app.multicam_groups.len() + 1),
                                        });
                                        cx.notify();
                                    }))
                                    .child("+ Create Group"),
                            )
                            .children(group_items),
                    )
                }),
        )
}
