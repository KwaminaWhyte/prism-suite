//! Bottom timeline — transport readout, a **scrubbable** time ruler with a live
//! playhead, and one lane row per layer carrying **keyframe diamonds**.
//!
//! Transport: the ruler strip records its painted bounds (via a `canvas`) into
//! `app.track_bounds`, then its mouse listeners map a pointer x back to a comp
//! time and emit `Action::SetTime`, so click + drag scrub the playhead and
//! re-render the bridged preview. A thin vertical marker shows the current
//! playhead position over the ruler and lanes.
//!
//! Keyframe lanes (Wave 4): each layer lane draws a diamond at every transform
//! keyframe time (the union of all transform tracks), positioned by a fractional
//! offset within the track region so it aligns with the ruler. For the
//! **selected** layer the diamonds are interactive — pressing one arms a
//! horizontal drag (`app.kf_drag`); lane-level mouse-move then re-times that key
//! via `Action::MoveKeyframe`, and the preview re-samples the interpolated value
//! live. (The per-property stopwatch that toggles animation lives in the
//! properties panel.)

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, px, relative, rgb, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, Icon, tool_button, section_header};

use pulse_app::comp::Prop;

use crate::app_state::{Action, App, KeyframeDrag, WorkAreaHandle};
use crate::Pulse;

/// Tint for the work-area band + its in/out handles (a warm amber, distinct from
/// the teal playhead/keyframes so the loop region reads apart).
const WORK_AREA: u32 = 0xd9a441;
/// Width (px) of a work-area handle's draggable hit target.
const WA_HANDLE_W: f32 = 8.0;

/// Accent for keyframe diamonds (matches the playhead teal).
const KEYFRAME: u32 = 0x37c8c0;
/// Half-size (px) of a keyframe diamond's bounding box on the lane.
const KF_HALF: f32 = 5.0;

/// Accent for the playhead marker (teal, matching the egui app's playhead).
const PLAYHEAD: u32 = 0x37c8c0;
/// Width of the fixed lane-name gutter; the ruler/track starts after it so the
/// ruler's x↔time mapping aligns with the lane tracks beneath it.
const GUTTER_W: f32 = 120.0;
/// Height of the scrub ruler strip.
const RULER_H: f32 = 24.0;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];
    let dur = comp.duration.max(0.001);
    let frac = (app.time / dur).clamp(0.0, 1.0);

    // The work area (loop / render region) as fractions along the ruler, so the
    // band + in/out handles align with the playhead's x↔time mapping.
    let wa = comp.clamped_work_area();
    let wa_in = (wa.start / dur).clamp(0.0, 1.0);
    let wa_out = (wa.end / dur).clamp(0.0, 1.0);
    // Only draw a band when the area is a real trimmed sub-range; a full timeline
    // shows just the handles at the ends (matching the egui app, which hides the
    // tint when the work area spans the whole comp).
    let wa_trimmed = !wa.is_full(comp.duration);

    // The shared cell the ruler `canvas` writes its bounds into; cloned into the
    // paint closure so the root view can later map a pointer x → time.
    let track_bounds = app.track_bounds.clone();

    // Cached frame fractions: for each frame index in the host's frame_cache,
    // compute its fractional ruler position so the paint closure can draw the
    // green warm-frame strip without accessing `App` inside the closure.
    let fps = comp.fps.max(1.0);
    let cached_fracs: Vec<f32> = app
        .host
        .cached_frame_set()
        .keys()
        .map(|&fi| {
            let t = fi as f32 / fps;
            (t / dur).clamp(0.0, 1.0)
        })
        .collect();

    // One lane per layer (top = topmost). Each lane's track region paints its
    // transform keyframes as diamonds; the selected layer's diamonds also carry
    // transparent draggable hit targets so they can be slid in time.
    let lanes = (0..comp.layers.len())
        .rev()
        .map(|i| {
            let l = &comp.layers[i];
            let selected = app.selected_layer == Some(i);

            // The union of all transform tracks' key times, with the per-key
            // value-fraction along the duration. De-duplicated by time so two
            // tracks keyed on the same instant draw one diamond.
            let mut times: Vec<f32> = Vec::new();
            for prop in Prop::ALL {
                for k in &l.track(prop).keys {
                    if !times.iter().any(|t| (t - k.t).abs() < 1e-3) {
                        times.push(k.t);
                    }
                }
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let has_time_remap = l.time_remap.is_active();

            // Fractions (0..1 across the duration) for the canvas painter + hits.
            let fracs: Vec<f32> = times.iter().map(|t| (t / dur).clamp(0.0, 1.0)).collect();
            let paint_fracs = fracs.clone();

            // Canvas overlay painting a diamond at each key fraction.
            let diamonds = canvas(
                move |_bounds, _win, _cx| {},
                move |bounds, _state, window, _cx| {
                    let left = f32::from(bounds.origin.x);
                    let top = f32::from(bounds.origin.y);
                    let w = f32::from(bounds.size.width).max(1.0);
                    let h = f32::from(bounds.size.height);
                    let cy = top + h * 0.5;
                    for f in &paint_fracs {
                        let cx_px = left + f * w;
                        paint_diamond(window, cx_px, cy, KF_HALF);
                    }
                },
            )
            .absolute()
            .size_full();

            // For the selected layer, a transparent draggable hit div per key
            // that arms `kf_drag` on press (which transform track owns the key is
            // resolved at drop-time; we drag whichever track keys this instant).
            let hits: Vec<_> = if selected {
                fracs
                    .iter()
                    .enumerate()
                    .map(|(ki, &f)| {
                        let key_t = times[ki];
                        div()
                            .id(("kf-hit", i * 256 + ki))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(f))
                            .w(px(KF_HALF * 2.0))
                            .ml(px(-KF_HALF))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |root, _ev, _win, cx| {
                                    if let Some((prop, ki2)) =
                                        find_key(&root.app, i, key_t)
                                    {
                                        root.app.kf_drag = Some(KeyframeDrag {
                                            layer: i,
                                            prop,
                                            key_index: ki2,
                                        });
                                        root.app.apply(Action::Pause);
                                        cx.notify();
                                    }
                                }),
                            )
                    })
                    .collect()
            } else {
                Vec::new()
            };

            div()
                .flex()
                .items_center()
                .h(px(22.0))
                .px_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .w(px(GUTTER_W - 16.0))
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_color(colors::text_primary())
                                .text_size(px(11.0))
                                .child(l.name.clone()),
                        )
                        .when(has_time_remap, |d| {
                            d.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(9.0))
                                    .text_color(gpui::rgb(0x37c8c0u32))
                                    .child("◇"),
                            )
                        }),
                )
                .child(
                    // Lane track region: striped background + diamonds + hits.
                    div()
                        .flex_1()
                        .h_full()
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top(px(6.0))
                                .bottom(px(6.0))
                                .left_0()
                                .right_0()
                                .rounded_sm()
                                .bg(colors::surface_raised()),
                        )
                        .child(diamonds)
                        .children(hits),
                )
        })
        .collect::<Vec<_>>();

    // The scrub ruler: a click/drag target laid out as [gutter | track]. The
    // track child paints a transparent `canvas` recording its real bounds, then
    // the mouse listeners read those bounds (via `App::time_for_x`) to scrub.
    let ruler = div()
        .id("timeline-ruler")
        .flex()
        .items_center()
        .h(px(RULER_H))
        .border_b_1()
        .border_color(colors::surface_border())
        // Mouse down on the ruler starts a scrub: pause playback, jump to the
        // clicked time, and arm drag tracking.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|root, ev: &gpui::MouseDownEvent, _win, cx| {
                // A work-area handle's mouse-down fires first and arms `wa_drag`;
                // in that case the ruler must not also start a playhead scrub.
                if root.app.wa_drag.is_some() {
                    return;
                }
                root.app.scrubbing = true;
                root.app.apply(Action::Pause);
                if let Some(t) = root.app.time_for_x(f32::from(ev.position.x)) {
                    root.app.apply(Action::SetTime(t));
                }
                cx.notify();
            }),
        )
        // Drag: a work-area handle drag re-times its marker; otherwise a scrub
        // re-times the playhead to the pointer.
        .on_mouse_move(cx.listener(|root, ev: &gpui::MouseMoveEvent, _win, cx| {
            if let Some(handle) = root.app.wa_drag {
                if let Some(t) = root.app.time_for_x(f32::from(ev.position.x)) {
                    root.app.apply(match handle {
                        WorkAreaHandle::In => Action::SetWorkAreaStart(t),
                        WorkAreaHandle::Out => Action::SetWorkAreaEnd(t),
                    });
                    cx.notify();
                }
            } else if root.app.scrubbing {
                if let Some(t) = root.app.time_for_x(f32::from(ev.position.x)) {
                    root.app.apply(Action::SetTime(t));
                    cx.notify();
                }
            }
        }))
        // Release ends a scrub or a work-area drag (over the ruler or not).
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|root, _ev, _win, cx| {
                root.app.scrubbing = false;
                root.app.wa_drag = None;
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|root, _ev, _win, cx| {
                root.app.scrubbing = false;
                root.app.wa_drag = None;
                cx.notify();
            }),
        )
        // Fixed name gutter (keeps the ruler aligned with the lane tracks below).
        .child(div().w(px(GUTTER_W)).h_full())
        // The track itself: records its painted bounds into the shared cell, and
        // carries the work-area band + draggable in/out handles.
        .child(
            div()
                .flex_1()
                .h_full()
                .relative()
                .child(
                    canvas(
                        move |bounds, _win, _cx| {
                            track_bounds.set(Some(bounds));
                        },
                        |_bounds, _state, _win, _cx| {},
                    )
                    .absolute()
                    .size_full(),
                )
                // Tinted band spanning [in, out] (only when trimmed).
                .when(wa_trimmed, |d| {
                    d.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(wa_in))
                            .w(relative((wa_out - wa_in).max(0.0)))
                            .bg(rgb(WORK_AREA))
                            .opacity(0.18),
                    )
                })
                // In-point handle (left edge of the work area).
                .child(work_area_handle(WorkAreaHandle::In, wa_in, cx))
                // Out-point handle (right edge of the work area).
                .child(work_area_handle(WorkAreaHandle::Out, wa_out, cx))
                // RAM preview cache strips: a 2px tall green strip at the top of
                // the ruler for each frame that is currently in the frame cache.
                // Uses the pre-computed `cached_fracs` to avoid re-borrowing `app`.
                .children(
                    cached_fracs.iter().map(|&frac| {
                        div()
                            .absolute()
                            .top_0()
                            .h(px(2.0))
                            .left(relative(frac))
                            .w(px(2.0))
                            .bg(gpui::rgb(0x2ecc71u32))
                            .opacity(0.85)
                    }).collect::<Vec<_>>()
                )
                // Comp markers: small green ticks at each marker's fractional position.
                .children(comp.markers.iter().enumerate().map(|(idx, m)| {
                    let frac_m = (m.time / dur.max(0.001)).clamp(0.0, 1.0);
                    let color_r = m.color[0];
                    let color_g = m.color[1];
                    let color_b = m.color[2];
                    let label = m.label.clone();
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(relative(frac_m))
                        .w(px(2.0))
                        .bg(gpui::Rgba { r: color_r, g: color_g, b: color_b, a: 0.9 })
                        .id(("comp-marker", idx as u64))
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |root, _ev, _win, cx| {
                                root.app.apply(Action::RemoveCompMarker(idx));
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .text_size(px(9.0))
                                .text_color(gpui::Rgba { r: color_r, g: color_g, b: color_b, a: 1.0 })
                                .child(label),
                        )
                })),
        );

    // Dim the playhead when it sits outside a trimmed work area, so it reads that
    // the current frame won't loop in playback (playback loops the work area).
    let in_work_area = !wa_trimmed || wa.contains(app.time, comp.duration);
    let playhead_color = if in_work_area { PLAYHEAD } else { 0x2a6f6c };

    // The playhead overlay: a thin vertical rule positioned within the track
    // region (offset past the gutter by `frac` of the track width). Laid out
    // absolutely over the ruler+lanes stack so it spans every row.
    let playhead = div()
        .absolute()
        .top_0()
        .bottom_0()
        // left = gutter + frac * (track width). The track is `flex_1`, so express
        // its left edge as the gutter and use a percentage of the remaining width
        // via a nested wrapper: a left-padded full-width band then a fractional
        // marker. Simpler: place the marker inside a gutter-offset, full-width row.
        .left(px(GUTTER_W))
        .right_0()
        .child(
            div()
                .h_full()
                .w(relative(frac))
                .flex()
                .justify_end()
                .child(div().w(px(2.0)).h_full().bg(rgb(playhead_color))),
        );

    let graph_open = app.graph_open;

    // Build the body for whichever mode is active up front, so only one branch
    // touches `cx` (a single mutable borrow per element tree).
    let graph_body = graph_open.then(|| crate::panels::graph::render(app, cx));
    let graph_chips = graph_open.then(|| crate::panels::graph::property_chips(app, cx));

    // The Timeline / Graph editor-mode toggle (the egui app's mode switch).
    let graph_toggle = div()
        .id("graph-toggle")
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|root, _ev, _win, cx| {
                root.app.apply(Action::ToggleGraph);
                cx.notify();
            }),
        )
        .child(tool_button(Icon::Adjustment, graph_open));

    div()
        .size_full()
        .flex()
        .flex_col()
        // Timeline header: transport controls + time readout + graph toggle + chips.
        .child({
            let playing = app.playing;
            let looping = app.loop_enabled;
            let fps = comp.fps.max(1.0);
            let step = 1.0 / fps;
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .h(px(34.0))
                .border_b_1()
                .border_color(colors::surface_border())
                // Go to start
                .child(
                    div()
                        .id("tl-go-start")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::GoToStart);
                            cx.notify();
                        }))
                        .child(prism_ui::icon(Icon::Rewind, 11.0)),
                )
                // Step-back
                .child(
                    div()
                        .id("tl-step-back")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::StepTime(-step));
                            cx.notify();
                        }))
                        .child(prism_ui::icon(Icon::ArrowLeft, 11.0)),
                )
                // Play/Pause
                .child(
                    div()
                        .id("tl-play")
                        .w(px(28.0)).h(px(28.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::accent())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::accent_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::TogglePlay);
                            cx.notify();
                        }))
                        .child(prism_ui::icon(if playing { Icon::Pause } else { Icon::Play }, 13.0)),
                )
                // Step-forward
                .child(
                    div()
                        .id("tl-step-fwd")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::StepTime(step));
                            cx.notify();
                        }))
                        .child(prism_ui::icon(Icon::ArrowRight, 11.0)),
                )
                // Go to end
                .child(
                    div()
                        .id("tl-go-end")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::GoToEnd);
                            cx.notify();
                        }))
                        .child(prism_ui::icon(Icon::FastForward, 11.0)),
                )
                // Loop toggle
                .child(
                    div()
                        .id("tl-loop")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(if looping { colors::accent() } else { colors::surface_overlay() })
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleLoop);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_color(if looping { gpui::rgb(0xffffff) } else { colors::text_secondary() })
                                .text_size(px(10.0))
                                .child("⟳"),
                        ),
                )
                .child(section_header(if graph_open { "Graph" } else { "Timeline" }))
                .child(graph_toggle)
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(11.0))
                        .child(format!(
                            "{:.2}s / {:.2}s  @ {:.0} fps",
                            app.time, comp.duration, comp.fps,
                        )),
                )
                .when_some(graph_chips, |d, chips| d.child(chips))
        })
        // Body: the value-curve graph editor, or the ruler + keyframe lanes.
        .when_some(graph_body, |d, body| d.child(body))
        .when(!graph_open, |d| {
            // Ruler + lanes, with the playhead drawn as an overlay across both.
            // This wrapper also services an in-flight keyframe drag: while
            // `kf_drag` is armed, pointer motion re-times the dragged key and a
            // release drops it (mirroring the ruler's scrub move/up handling).
            d.child(
                div()
                    .flex_1()
                    .relative()
                    .on_mouse_move(cx.listener(|root, ev: &gpui::MouseMoveEvent, _win, cx| {
                        if let Some(d) = root.app.kf_drag {
                            if let Some(t) = root.app.time_for_x(f32::from(ev.position.x)) {
                                root.app.apply(Action::MoveKeyframe {
                                    prop: d.prop,
                                    key_index: d.key_index,
                                    time: t,
                                });
                                cx.notify();
                            }
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|root, _ev, _win, cx| {
                            if root.app.kf_drag.take().is_some() {
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|root, _ev, _win, cx| {
                            if root.app.kf_drag.take().is_some() {
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            // Ruler stays fixed at the top; the lanes below scroll
                            // vertically so every layer row is reachable when the
                            // layer count exceeds the timeline's fixed height.
                            .child(ruler)
                            .child(
                                div()
                                    .id("tl-lanes")
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_col()
                                    .children(lanes),
                            ),
                    )
                    .child(playhead),
            )
        })
}

/// A draggable work-area handle (in or out marker) positioned at fraction `f`
/// along the ruler track. Mouse-down arms `app.wa_drag` and pauses playback; the
/// ruler's mouse-move then re-times the marker. A right-click resets the work
/// area to the full timeline (After Effects' "double-click work-area bar"
/// gesture, simplified). The visible bar is a thin amber rule centred on the
/// fraction; the hit target is a wider transparent column so it's easy to grab.
fn work_area_handle(
    handle: WorkAreaHandle,
    f: f32,
    cx: &mut Context<Pulse>,
) -> impl IntoElement {
    let id = match handle {
        WorkAreaHandle::In => "wa-handle-in",
        WorkAreaHandle::Out => "wa-handle-out",
    };
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .left(relative(f))
        .w(px(WA_HANDLE_W))
        .ml(px(-WA_HANDLE_W * 0.5))
        .cursor_pointer()
        // Arm the drag (resolved as in/out by `handle`); pause so the marker
        // doesn't chase a moving playhead while being placed.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, _ev, _win, cx| {
                root.app.wa_drag = Some(handle);
                root.app.apply(Action::Pause);
                cx.notify();
            }),
        )
        // Right-click resets the work area to the whole timeline.
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|root, _ev, _win, cx| {
                root.app.apply(Action::ResetWorkArea);
                cx.notify();
            }),
        )
        // The visible rule: a thin amber bar centred in the hit column.
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(WA_HANDLE_W * 0.5 - 1.0))
                .w(px(2.0))
                .bg(rgb(WORK_AREA)),
        )
}

/// Paint a filled keyframe diamond centred at `(cx_px, cy)` with half-extent `r`.
fn paint_diamond(window: &mut gpui::Window, cx_px: f32, cy: f32, r: f32) {
    use gpui::{point, px, Path};
    let mut path = Path::new(point(px(cx_px), px(cy - r)));
    path.line_to(point(px(cx_px + r), px(cy)));
    path.line_to(point(px(cx_px), px(cy + r)));
    path.line_to(point(px(cx_px - r), px(cy)));
    path.line_to(point(px(cx_px), px(cy - r)));
    window.paint_path(path, rgb(KEYFRAME));
}

/// Find which transform track of layer `i` carries a key at (or within EPS of)
/// time `t`, returning that `(prop, key_index)`. Picks the first track in
/// `Prop::ALL` order that keys this instant — enough to drive a time-drag (the
/// diamond is the union of all tracks keyed here; dragging re-times whichever it
/// resolves to). `None` if the layer/key is gone.
fn find_key(app: &App, i: usize, t: f32) -> Option<(Prop, usize)> {
    const EPS: f32 = 1e-3;
    let ci = app.active_comp_index();
    let layer = app.project.comps[ci].layers.get(i)?;
    for prop in Prop::ALL {
        if let Some(ki) = layer
            .track(prop)
            .keys
            .iter()
            .position(|k| (k.t - t).abs() < EPS)
        {
            return Some((prop, ki));
        }
    }
    None
}
