//! Session View panel — Ableton-style grid of clip slots.
//!
//! Rows = scenes, Columns = tracks. Each cell shows a clip if one is assigned
//! to that (scene, track) pair, or an empty "+" placeholder.
//!
//! NOTE: This panel is implemented and unit-tested but not yet wired into the
//! main layout (there is no Arrangement/Session view-mode toggle yet), so the
//! render fn and its layout constants read as dead code for now. Allowed at the
//! module level until the next UI wave wires it in.
#![allow(dead_code)]

use gpui::{
    div, px, AnyElement, Context, Element, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App};
use crate::Tone;

const SLOT_H: f32 = 48.0;
const HEADER_H: f32 = 24.0;
// Minimum slot width; columns are fixed at this width
const SLOT_MIN_W: f32 = 100.0;

pub fn render_session_view(app: &App, cx: &mut Context<Tone>) -> impl IntoElement {
    // Collect tracks and scenes once to avoid repeated borrows
    let tracks: Vec<(usize, String)> = app
        .tracks
        .iter()
        .map(|t| (t.id, t.name.clone()))
        .collect();

    let scenes: Vec<(usize, String)> = app
        .scenes
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();

    // Build (scene_id, track_id) -> clip_id map from scene slots
    let mut slot_map: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();
    for scene in &app.scenes {
        for slot in &scene.slots {
            if let Some(cid) = slot.clip_id {
                slot_map.insert((scene.id, slot.track_id), cid);
            }
        }
    }

    // Build clip name lookup
    let clip_names: std::collections::HashMap<usize, String> = app
        .clips
        .iter()
        .map(|c| (c.id, c.name.clone()))
        .collect();

    let active_scene = app.active_scene_id;
    let has_scenes = !scenes.is_empty();

    div()
        .w_full()
        .h_full()
        .bg(colors::surface_bg())
        .flex()
        .flex_col()
        .overflow_hidden()
        // Column headers: track names
        .child(
            div()
                .w_full()
                .h(px(HEADER_H))
                .bg(colors::surface_raised())
                .border_b_1()
                .border_color(colors::surface_border())
                .flex()
                .flex_row()
                // Scene label column placeholder
                .child(
                    div()
                        .w(px(80.0))
                        .flex_shrink_0()
                        .h_full()
                        .border_r_1()
                        .border_color(colors::surface_border())
                        .flex()
                        .items_center()
                        .px_2()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child("SCENE"),
                )
                .children(tracks.iter().map(|(_, tname)| {
                    div()
                        .w(px(SLOT_MIN_W))
                        .flex_shrink_0()
                        .h_full()
                        .border_r_1()
                        .border_color(colors::surface_border())
                        .flex()
                        .items_center()
                        .px_2()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .overflow_hidden()
                        .child(tname.clone())
                })),
        )
        // Scene rows + clip slots
        .child(
            div()
                .id("session-scroll")
                .flex_1()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children({
                    let rows: Vec<AnyElement> = scenes
                        .iter()
                        .enumerate()
                        .map(|(row_idx, (scene_id, scene_name))| {
                            let scene_id = *scene_id;
                            let is_active = active_scene == Some(scene_id);
                            let scene_name = scene_name.clone();

                            // Build slot cells for each track
                            let slot_cells: Vec<AnyElement> = tracks
                                .iter()
                                .enumerate()
                                .map(|(col_idx, (track_id, _))| {
                                    let track_id = *track_id;
                                    let maybe_clip =
                                        slot_map.get(&(scene_id, track_id)).copied();
                                    let clip_name = maybe_clip
                                        .and_then(|cid| clip_names.get(&cid))
                                        .cloned()
                                        .unwrap_or_default();
                                    let has_clip = maybe_clip.is_some();
                                    let clip_id_for_cb = maybe_clip.unwrap_or(0);

                                    div()
                                        .id(("slot", row_idx * 1000 + col_idx))
                                        .w(px(SLOT_MIN_W))
                                        .flex_shrink_0()
                                        .h_full()
                                        .border_r_1()
                                        .border_color(colors::surface_border())
                                        .bg(if has_clip {
                                            colors::surface_overlay()
                                        } else {
                                            colors::surface_bg()
                                        })
                                        .flex()
                                        .items_center()
                                        .px_2()
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            if has_clip {
                                                this.app
                                                    .apply(Action::OpenPianoRoll(clip_id_for_cb));
                                                cx.notify();
                                            }
                                        }))
                                        // Clip block (only shown when clip exists)
                                        .when(has_clip, |d| {
                                            d.child(
                                                div()
                                                    .w_full()
                                                    .h(px(32.0))
                                                    .bg(colors::accent())
                                                    .rounded(px(3.0))
                                                    .flex()
                                                    .items_center()
                                                    .px_1()
                                                    .text_size(px(font_size::XS))
                                                    .text_color(gpui::rgb(0xffffff))
                                                    .overflow_hidden()
                                                    .child(clip_name),
                                            )
                                        })
                                        // Empty placeholder (only shown when no clip)
                                        .when(!has_clip, |d| {
                                            d.child(
                                                div()
                                                    .text_size(px(font_size::SM))
                                                    .text_color(colors::text_disabled())
                                                    .child("+"),
                                            )
                                        })
                                        .into_any()
                                })
                                .collect();

                            div()
                                .w_full()
                                .h(px(SLOT_H))
                                .flex()
                                .flex_row()
                                .border_b_1()
                                .border_color(colors::surface_border())
                                // Scene label cell
                                .child(
                                    div()
                                        .id(("scene-label", row_idx))
                                        .w(px(80.0))
                                        .flex_shrink_0()
                                        .h_full()
                                        .border_r_1()
                                        .border_color(colors::surface_border())
                                        .bg(if is_active {
                                            colors::surface_overlay()
                                        } else {
                                            colors::surface_raised()
                                        })
                                        .flex()
                                        .items_center()
                                        .px_2()
                                        .text_size(px(font_size::XS))
                                        .text_color(if is_active {
                                            colors::text_primary()
                                        } else {
                                            colors::text_secondary()
                                        })
                                        .overflow_hidden()
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                                            this.app.apply(Action::LaunchScene { scene_id });
                                            cx.notify();
                                        }))
                                        .child(scene_name),
                                )
                                // Clip slots
                                .children(slot_cells)
                                .into_any()
                        })
                        .collect();
                    rows
                }),
        )
        // Empty state when no scenes exist
        .when(!has_scenes, |d| {
            d.child(
                div()
                    .w_full()
                    .h(px(HEADER_H + SLOT_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .child("No scenes \u{2014} add a scene to use Session View"),
            )
        })
}
