//! Timeline — the BOTTOM strip beneath the preview.
//!
//! A ruler + per-track lanes drawing each clip as a positioned block (left/width
//! from its `start`/`duration`), with a red playhead marker at the current time.
//!
//! WIRED:
//! - **Scrub-to-seek.** Pressing or dragging the empty lane body maps the
//!   pointer x to a timeline time and emits [`Action::Seek`], sliding the
//!   playhead with the pointer.
//! - **Clip move / trim (Select tool).** Each clip block is interactive: a
//!   `mouse_down` near its **left edge** starts a head trim ([`Action::TrimClipIn`]),
//!   near its **right edge** a tail trim ([`Action::TrimClipOut`]), and anywhere
//!   in the **body** a move ([`Action::MoveClip`], grab-offset preserved). The
//!   active drag lives in a shared [`App::clip_drag`] cell; the overlay's
//!   `mouse_move`/`mouse_up` translate the pointer time into the matching edit
//!   each frame and clear the drag on release. Every edit is frame-snapped and
//!   clamped to the source's available range in `App::apply`.
//! - **Razor / split (Razor tool).** With the Razor tool active, a `mouse_down`
//!   on a clip splits it at the pointer time ([`Action::SplitClip`]); the
//!   toolbar's **Split** button razors every clip under the playhead.
//!
//! Layout: the lanes draw labels + empty bodies; a single absolute **overlay**
//! on top hosts every clip block (positioned by track row + time), the playhead,
//! and all pointer interaction — so the clip blocks are topmost and receive the
//! `mouse_down` while the overlay body is the seek/drag catch-all beneath them.
//! The overlay's lane-body screen bounds are recorded each frame by a `canvas`
//! so the listeners can map an absolute window x back to a fraction of the
//! sequence.

use gpui::{
    canvas, div, px, relative, rgb, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, icon, section_header, Icon};

use crate::app_state::{Action, App, ClipDrag, ClipDragKind, Tool, TransitionKind, WipeDir,
    DIP_BLACK, DIP_WHITE};
use crate::Reel;

/// Height of a single track lane.
const LANE_H: f32 = 36.0;

/// Pointer slop, in window pixels, for grabbing a clip's left/right edge to trim
/// (vs. grabbing the body to move). Converted to a time threshold per drag.
const EDGE_GRAB_PX: f32 = 8.0;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let p = &app.project;
    let dur = p.duration.max(0.001);
    let playhead_frac = (app.time / dur).clamp(0.0, 1.0);
    let snap_frac = app.snap_point.map(|t| (t / dur).clamp(0.0, 1.0));
    let n_tracks = p.tracks.len();

    // Shared cell the scrub-region `canvas` recorder writes and the listeners read.
    let bounds_cell = app.timeline_bounds.clone();

    // Lane rows (top track first): label gutter + empty body backdrop. The clip
    // blocks are NOT drawn here — they live in the overlay so they stay topmost.
    let lanes = (0..n_tracks)
        .rev()
        .map(|ti| {
            let track = &p.tracks[ti];
            let label_color = if track.enabled { colors::text_primary() } else { colors::text_disabled() };
            div()
                .flex()
                .h(px(LANE_H))
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .w(px(56.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .px_2()
                        .bg(colors::surface_raised())
                        .border_r_1()
                        .border_color(colors::surface_border())
                        .text_color(label_color)
                        .text_size(px(11.0))
                        .child(track.name.clone()),
                )
                .child(div().flex_1().h_full().bg(colors::surface_overlay()))
        })
        .collect::<Vec<_>>();

    // Clip blocks for the overlay, positioned by track row (top track first) and
    // by time. Each carries its global clip index so its drag edits the right clip.
    let blocks = p
        .clips
        .iter()
        .enumerate()
        .filter(|(_, c)| c.track < n_tracks)
        .map(|(idx, c)| {
            let row = n_tracks - 1 - c.track; // top track first
            let top = row as f32 * LANE_H + 4.0;
            let left = (c.start / dur).clamp(0.0, 1.0);
            let width = (c.duration / dur).clamp(0.0, 1.0);
            let sw = c.source.block_color();
            let block_rgb = (((sw[0].clamp(0.0, 1.0) * 255.0) as u32) << 16)
                | (((sw[1].clamp(0.0, 1.0) * 255.0) as u32) << 8)
                | ((sw[2].clamp(0.0, 1.0) * 255.0) as u32);
            let is_selected = app.selected == Some(idx);
            let border = if is_selected { 0xffd84d } else { 0x000000 };
            let clip_start = c.start;
            let clip_end = c.end();
            let speed = c.speed;
            let reversed = c.reversed;
            let has_proxy = c.proxy_path.is_some();
            let base_name = c.name.clone();
            let name = if (speed - 1.0).abs() > 0.01 || reversed || has_proxy {
                let spd = if (speed - 1.0).abs() > 0.01 {
                    format!(" [{:.1}×]", speed)
                } else {
                    String::new()
                };
                let rev = if reversed { "⟵ " } else { "" };
                let proxy = if has_proxy { " [P]" } else { "" };
                format!("{}{}{}{}", rev, base_name, spd, proxy)
            } else {
                base_name
            };
            let fade_in = c.fade_in;
            let fade_out = c.fade_out;
            let clip_dur = c.duration;
            div()
                .id(("clip", idx))
                .absolute()
                .top(px(top))
                .h(px(LANE_H - 8.0))
                .left(relative(left))
                .w(relative(width))
                .rounded_sm()
                .bg(rgb(block_rgb))
                .border_1()
                .border_color(rgb(border))
                .overflow_hidden()
                .cursor_pointer()
                // Grab: decide move / trim-in / trim-out from where in the block
                // (in time) the pointer landed, or split under the Razor tool.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |root, ev: &gpui::MouseDownEvent, _win, cx| {
                        let Some(pt) = root.app.timeline_x_to_time(f32::from(ev.position.x))
                        else {
                            return;
                        };
                        root.app.apply(Action::SelectClip(idx));

                        if root.app.active == Tool::Razor {
                            root.app.apply(Action::SplitClip { index: idx, t: pt });
                            cx.notify();
                            return;
                        }

                        let edge_dt = root
                            .app
                            .timeline_dx_to_dt(EDGE_GRAB_PX)
                            .unwrap_or(0.0)
                            .abs();
                        let near_left = (pt - clip_start).abs() <= edge_dt;
                        let near_right = (clip_end - pt).abs() <= edge_dt;
                        let kind = match root.app.active {
                            Tool::RippleTrim => {
                                if near_left {
                                    ClipDragKind::RippleTrimIn
                                } else if near_right {
                                    ClipDragKind::RippleTrimOut
                                } else {
                                    ClipDragKind::Move
                                }
                            }
                            Tool::RollTrim => {
                                if near_left || near_right {
                                    ClipDragKind::RollTrim
                                } else {
                                    ClipDragKind::Move
                                }
                            }
                            Tool::RateStretch => {
                                if near_right {
                                    ClipDragKind::RateStretch
                                } else {
                                    ClipDragKind::Move
                                }
                            }
                            _ => {
                                if near_left {
                                    ClipDragKind::TrimIn
                                } else if near_right {
                                    ClipDragKind::TrimOut
                                } else {
                                    ClipDragKind::Move
                                }
                            }
                        };
                        // For RollTrim, grab_offset is repurposed to track the last
                        // pointer position (delta computation in mouse_move). For all
                        // other kinds it keeps the normal clip_start - pt meaning.
                        // For RateStretch, grab_offset carries clip_start for duration calc.
                        let grab_offset = if kind == ClipDragKind::RollTrim {
                            pt
                        } else if kind == ClipDragKind::RateStretch {
                            clip_start
                        } else {
                            clip_start - pt
                        };
                        root.app.clip_drag.set(Some(ClipDrag {
                            index: idx,
                            kind,
                            grab_offset,
                        }));
                        cx.notify();
                    }),
                )
                // Fade-in overlay: semi-transparent strip at the clip's left edge,
                // tinted with text_primary at low alpha.
                .children(if fade_in > 0.0 && clip_dur > 0.0 {
                    let fi_frac = (fade_in / clip_dur).clamp(0.0, 1.0);
                    vec![div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(relative(fi_frac))
                        .bg(rgb(0xffffff))
                        .opacity(0.25)
                        .into_any_element()]
                } else {
                    vec![]
                })
                // Fade-out overlay: semi-transparent strip at the clip's right edge.
                .children(if fade_out > 0.0 && clip_dur > 0.0 {
                    let fo_frac = (fade_out / clip_dur).clamp(0.0, 1.0);
                    vec![div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(relative(fo_frac))
                        .bg(rgb(0xffffff))
                        .opacity(0.25)
                        .into_any_element()]
                } else {
                    vec![]
                })
                .child(
                    div()
                        .px_1()
                        .text_size(px(10.0))
                        .text_color(rgb(0x111111))
                        .child(name),
                )
        })
        .collect::<Vec<_>>();

    // Build tool-button and transition-chip Vecs before the div chain
    // so cx isn't moved into inline map closures.
    let tl_tools: Vec<_> = Tool::ALL.iter().enumerate().map(|(idx, &t)| {
        let is_active = t == app.active;
        div()
            .id(("tl-tool", idx))
            .px(px(5.0)).py(px(2.0))
            .rounded_sm()
            .text_size(px(10.0))
            .cursor_pointer()
            .bg(if is_active { colors::tool_active() } else { colors::surface_overlay() })
            .text_color(colors::text_primary())
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetTool(t));
                cx.notify();
            }))
            .child(t.label())
    }).collect();

    let snap_enabled = app.snap_enabled;
    let snap_btn = div()
        .id("tl-snap")
        .px(px(5.0)).py(px(2.0))
        .rounded_sm()
        .text_size(px(10.0))
        .cursor_pointer()
        .bg(if snap_enabled { colors::tool_active() } else { colors::surface_overlay() })
        .text_color(colors::text_primary())
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::ToggleSnap);
            cx.notify();
        }))
        .child(if snap_enabled { "\u{1f9f2}" } else { "snap" });

    let selected = app.selected;
    let tl_chips: Vec<_> = [
        ("Split",  None::<TransitionKind>),
        ("~",      Some(TransitionKind::CrossDissolve)),
        ("Dip",    Some(TransitionKind::DipToColor(DIP_BLACK))),
        ("Wipe",   Some(TransitionKind::Wipe(WipeDir::Left))),
    ].into_iter().enumerate().map(|(i, (label, kind))| {
        div()
            .id(("tl-trans", i))
            .px(px(4.0)).py(px(2.0))
            .rounded_sm()
            .border_1()
            .border_color(colors::surface_border())
            .bg(colors::surface_overlay())
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                if kind.is_none() {
                    root.app.apply(Action::SplitAtPlayhead);
                } else if let (Some(k), Some(index)) = (kind, selected) {
                    root.app.apply(Action::AddTransition { index, kind: k });
                }
                cx.notify();
            }))
            .child(label)
    }).collect();

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(colors::surface_raised())
        // Header row.
        .child({
            let playing = app.playing;
            let dur = p.duration.max(0.001);
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .h(px(32.0))
                .border_b_1()
                .border_color(colors::surface_border())
                // Go to start
                .child(
                    div()
                        .id("reel-go-start")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            root.app.apply(Action::Seek(0.0));
                            cx.notify();
                        }))
                        .child(icon(Icon::Rewind, 11.0)),
                )
                // Step back
                .child(
                    div()
                        .id("reel-step-back")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            let fps = root.app.project.fps;
                            root.app.apply(Action::StepBy(-1.0 / fps));
                            cx.notify();
                        }))
                        .child(icon(Icon::ArrowLeft, 11.0)),
                )
                // Play / Pause
                .child(
                    div()
                        .id("reel-play")
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
                        .child(icon(if playing { Icon::Pause } else { Icon::Play }, 13.0)),
                )
                // Step forward
                .child(
                    div()
                        .id("reel-step-fwd")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(|root, _ev, _win, cx| {
                            let fps = root.app.project.fps;
                            root.app.apply(Action::StepBy(1.0 / fps));
                            cx.notify();
                        }))
                        .child(icon(Icon::ArrowRight, 11.0)),
                )
                // Go to end
                .child(
                    div()
                        .id("reel-go-end")
                        .w(px(22.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::Seek(dur));
                            cx.notify();
                        }))
                        .child(icon(Icon::FastForward, 11.0)),
                )
                .child(
                    div().w(px(1.0)).h(px(16.0)).bg(colors::surface_border()),
                )
                // Edit tool buttons: Select / Razor / Slip / Hand / Ripple / Roll
                .children(tl_tools)
                .child(
                    div().w(px(1.0)).h(px(16.0)).bg(colors::surface_border()),
                )
                // Transition chips: Split | ~ | Dip | Wipe
                .children(tl_chips)
                .child(
                    div().w(px(1.0)).h(px(16.0)).bg(colors::surface_border()),
                )
                .child(snap_btn)
                .child(section_header("Timeline"))
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(format!("{:.1}s · f{}", app.time, p.frame_at(app.time))),
                )
        })
        .child(divider())
        // Jog shuttle strip — drag left/right to scrub the playhead.
        .child({
            let jog_bounds_cell = app.timeline_bounds.clone();
            div()
                .id("jog-shuttle")
                .w_full()
                .h(px(18.0))
                .flex_none()
                .relative()
                .bg(rgb(0x1a1a1a))
                .border_b_1()
                .border_color(colors::surface_border())
                .cursor_col_resize()
                // Tick marks at each whole second.
                .children({
                    let n_ticks = dur.floor() as usize;
                    (0..=n_ticks).map(|i| {
                        let frac = (i as f32) / dur;
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .w(px(1.0))
                            .left(relative(frac.clamp(0.0, 1.0)))
                            .bg(rgb(0x333333))
                    }).collect::<Vec<_>>()
                })
                // Playhead position marker.
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(px(2.0))
                        .left(relative(playhead_frac))
                        .bg(rgb(0xff3333)),
                )
                // Press → seek.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |root, ev: &gpui::MouseDownEvent, _win, cx| {
                        if let Some(t) = root.app.timeline_x_to_time(f32::from(ev.position.x)) {
                            root.app.apply(Action::Seek(t));
                            cx.notify();
                        }
                        let _ = jog_bounds_cell.get();
                    }),
                )
                // Drag → continuous scrub.
                .on_mouse_move(cx.listener(|root, ev: &gpui::MouseMoveEvent, _win, cx| {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        return;
                    }
                    if let Some(t) = root.app.timeline_x_to_time(f32::from(ev.position.x)) {
                        root.app.apply(Action::Seek(t));
                        cx.notify();
                    }
                }))
        })
        // Lanes + interactive overlay (clip blocks + playhead).
        .child(
            div()
                .relative()
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .children(lanes)
                // Interactive overlay over the lane body (past the 56px gutter):
                // hosts the clip blocks (topmost — they get the press), the
                // playhead, and the seek / drag catch-all handlers.
                .child(
                    div()
                        .id("timeline-overlay")
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(56.0))
                        .right_0()
                        // Record the laid-out scrub-region bounds each frame.
                        .child(canvas(
                            move |bounds, _window, _cx| bounds_cell.set(Some(bounds)),
                            |_bounds, _state, _window, _cx| {},
                        ))
                        // Press on the empty body → seek. (A press that landed on
                        // a clip set `clip_drag` in the block handler, which fires
                        // first as the topmost target, so skip the seek then.)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|root, ev: &gpui::MouseDownEvent, _win, cx| {
                                if root.app.clip_drag.get().is_some() {
                                    return;
                                }
                                if let Some(t) =
                                    root.app.timeline_x_to_time(f32::from(ev.position.x))
                                {
                                    root.app.apply(Action::Seek(t));
                                    cx.notify();
                                }
                            }),
                        )
                        // Drag with the left button held → apply the active clip
                        // edit if one is in flight, else continuous scrub.
                        .on_mouse_move(cx.listener(
                            |root, ev: &gpui::MouseMoveEvent, _win, cx| {
                                if ev.pressed_button != Some(MouseButton::Left) {
                                    return;
                                }
                                let Some(pt) =
                                    root.app.timeline_x_to_time(f32::from(ev.position.x))
                                else {
                                    return;
                                };
                                match root.app.clip_drag.get() {
                                    Some(drag) => {
                                        let action = match drag.kind {
                                            ClipDragKind::Move => Action::MoveClipDrag {
                                                track_idx: 0,
                                                raw_t: pt + drag.grab_offset,
                                            },
                                            ClipDragKind::TrimIn => Action::TrimClipIn {
                                                index: drag.index,
                                                t: pt,
                                            },
                                            ClipDragKind::TrimOut => Action::TrimClipOut {
                                                index: drag.index,
                                                t: pt,
                                            },
                                            ClipDragKind::RippleTrimIn => {
                                                Action::RippleTrimClipIn {
                                                    index: drag.index,
                                                    t: pt,
                                                }
                                            }
                                            ClipDragKind::RippleTrimOut => {
                                                Action::RippleTrimClipOut {
                                                    index: drag.index,
                                                    t: pt,
                                                }
                                            }
                                            ClipDragKind::RollTrim => {
                                                // delta = pointer movement since last frame;
                                                // we use the grab_offset field to carry the
                                                // last-known timeline position.
                                                let prev = drag.grab_offset; // repurposed as last_pt
                                                let delta = pt - prev;
                                                // Update drag cell with new last_pt.
                                                root.app.clip_drag.set(Some(ClipDrag {
                                                    grab_offset: pt,
                                                    ..drag
                                                }));
                                                Action::RollTrimEdit {
                                                    index: drag.index,
                                                    delta,
                                                }
                                            }
                                            ClipDragKind::RateStretch => {
                                                // Drag the right edge to change speed (rate stretch).
                                                // grab_offset carries the clip's start time.
                                                let clip_start = drag.grab_offset; // repurposed as clip.start
                                                let new_duration = (pt - clip_start).max(0.1);
                                                Action::RateStretchClip {
                                                    index: drag.index,
                                                    new_duration,
                                                }
                                            }
                                        };
                                        root.app.apply(action);
                                        cx.notify();
                                    }
                                    None => {
                                        root.app.apply(Action::Seek(pt));
                                        cx.notify();
                                    }
                                }
                            },
                        ))
                        // Release → end any clip drag.
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|root, _ev: &gpui::MouseUpEvent, _win, cx| {
                                root.app.apply(Action::EndClipDrag);
                                cx.notify();
                            }),
                        )
                        .children(blocks)
                        // The thin red playhead bar, positioned by fraction.
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(playhead_frac))
                                .w(px(2.0))
                                .bg(colors::danger()),
                        )
                        // Snap indicator: thin warning-yellow line at the snap point (visible during drags).
                        .children(snap_frac.map(|frac| {
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(frac))
                                .w(px(1.0))
                                .bg(colors::warning())
                        }))
                        // Chapter markers: orange ticks on the timeline ruler.
                        .children(app.chapter_markers.iter().enumerate().map(|(idx, (t, label))| {
                            let frac = (t / dur.max(0.001)).clamp(0.0, 1.0);
                            let label = label.clone();
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(frac))
                                .w(px(2.0))
                                .bg(gpui::Rgba { r: 1.0, g: 0.55, b: 0.0, a: 0.9 })
                                .id(("chapter", idx as u64))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |root, _ev, _win, cx| {
                                        root.app.apply(Action::RemoveChapterMarker(idx));
                                        cx.notify();
                                    }),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .text_size(px(9.0))
                                        .text_color(gpui::Rgba { r: 1.0, g: 0.55, b: 0.0, a: 1.0 })
                                        .child(label),
                                )
                        })),
                ),
        )
}
