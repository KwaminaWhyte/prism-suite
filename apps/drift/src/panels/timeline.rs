//! Timeline panel — frame ruler, layer tracks, and transport bar.
//!
//! Layout (top → bottom):
//!   1. Transport bar  — frame readout, in/out points, loop toggle
//!   2. Frame ruler    — tick marks at every second (fps interval)
//!   3. Layer tracks   — one row per layer, clip bar + keyframe dots

use crate::app_state::{Action, App, EasingKind};
use crate::Drift;
use gpui::{div, px, relative, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size};

pub fn render_timeline(app: &App, viewport_width: f32, cx: &mut Context<Drift>) -> impl IntoElement {
    // Name column is 240 px; the track/ruler content takes the rest.
    // We receive viewport_width from the caller so ruler ticks and keyframe dots
    // share the same coordinate space.
    let track_w = (viewport_width - 240.0).max(1.0);
    let total = app.document.duration_frames;
    let current = app.current_frame;
    let fps = app.document.fps as usize;
    let loop_on = app.loop_playback;

    // Pre-collect keyframes per layer for rendering dots.
    // Vec of (layer_id, frame, pct_pos) for all keyframes.
    let kf_positions: Vec<(usize, f32)> = app
        .keyframes
        .iter()
        .map(|kf| {
            let pct = kf.frame as f32 / total.max(1) as f32;
            (kf.layer_id, pct)
        })
        .collect();

    div()
        .w_full()
        .h(px(180.0))
        .bg(colors::surface_raised())
        .border_t_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        // 1. Transport bar
        .child(
            div()
                .w_full()
                .h(px(32.0))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child(format!(
                            "TIMELINE  Frame {current}/{total}  |  In:{}  Out:{}",
                            app.in_point, app.out_point
                        )),
                )
                // BPM and frame-rate info
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child(format!(
                            "BPM: {:.1}  FPS: {}",
                            app.beat_sync.bpm,
                            fps
                        )),
                )
                .child(div().flex_1())
                // Stop button
                .child(
                    div()
                        .id("stop-btn")
                        .px_2()
                        .h(px(20.0))
                        .bg(colors::surface_overlay())
                        .rounded(px(2.0))
                        .flex()
                        .items_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::Stop);
                            cx.notify();
                        }))
                        .child("■"),
                )
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child(format!("Loop: {}", if loop_on { "ON" } else { "OFF" })),
                )
                // Loop toggle button
                .child(
                    div()
                        .id("loop-toggle")
                        .px_2()
                        .h(px(20.0))
                        .bg(if loop_on {
                            colors::accent()
                        } else {
                            colors::surface_overlay()
                        })
                        .rounded(px(2.0))
                        .flex()
                        .items_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::ToggleLoop);
                            cx.notify();
                        }))
                        .child("Loop"),
                ),
        )
        // 2. Frame ruler — tick at every second, click-to-seek
        .child({
            let step = fps.max(1);
            let max_ticks = total / step + 1;
            let denom = total.max(1) as f32;
            // Pre-compute pixel offsets (within the flex_1 content area) so GPUI
            // never has to resolve relative() fractions — px() is unambiguous.
            let playhead_px = (current as f32 / denom) * track_w;
            div()
                .id("frame-ruler")
                .w_full()
                .h(px(20.0))
                .bg(colors::surface_bg())
                .border_b_1()
                .border_color(colors::surface_border())
                .flex()
                .items_center()
                .cursor_pointer()
                // 240 px name-column spacer — mirrors track row layout
                .child(div().w(px(240.0)).flex_shrink_0())
                // Ruler content area — same effective width as keyframe dot tracks
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .relative()
                        // Playhead line at exact pixel position
                        .child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .bottom(px(0.0))
                                .left(px(playhead_px))
                                .w(px(1.5))
                                .bg(gpui::rgba(0xff6b6bff)),
                        )
                        // Tick labels at exact pixel positions — 1:1 with keyframe dots
                        .children((0..=max_ticks.min(60)).map(|i| {
                            let f = i * step;
                            let x = (f as f32 / denom) * track_w;
                            div()
                                .absolute()
                                .left(px(x))
                                .top(px(3.0))
                                .text_size(px(8.0))
                                .text_color(colors::text_disabled())
                                .child(format!("{f}"))
                        })),
                )
                // Click-to-seek — track_w matches the flex_1 content area exactly
                .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _win, cx| {
                    let pos = ev.position();
                    let click_x = f32::from(pos.x);
                    let track_x = (click_x - 240.0).max(0.0);
                    let fraction = (track_x / track_w).clamp(0.0, 1.0);
                    let new_frame = (fraction * total as f32) as usize;
                    this.app.apply(Action::SetCurrentFrame(new_frame));
                    cx.notify();
                }))
        })
        // 3. Layer tracks
        .child(
            div()
                .id("timeline-tracks")
                .flex_1()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children(app.layers.iter().map(|layer| {
                    let layer_id = layer.id;
                    let in_f = layer.start_frame;
                    let out_f = layer.end_frame;
                    let name = layer.name.clone();
                    let denom = total.max(1) as f32;
                    let pct_start = in_f as f32 / denom;
                    let pct_width = (out_f.saturating_sub(in_f)) as f32 / denom;

                    // Keyframe dots for this layer
                    let layer_kf_pcts: Vec<f32> = kf_positions
                        .iter()
                        .filter(|(lid, _)| *lid == layer_id)
                        .map(|(_, pct)| *pct)
                        .collect();

                    div()
                        .w_full()
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .border_b_1()
                        .border_color(colors::surface_border())
                        // Layer name cell
                        .child(
                            div()
                                .w(px(240.0))
                                .px_2()
                                .text_size(px(font_size::XS))
                                .text_color(colors::text_secondary())
                                .overflow_hidden()
                                .child(name),
                        )
                        // Track bar area — click to add keyframe at current frame
                        .child(
                            div()
                                .id(SharedString::from(format!("track-{layer_id}")))
                                .flex_1()
                                .h_full()
                                .relative()
                                .cursor_pointer()
                                // Clip bar
                                .child(
                                    div()
                                        .absolute()
                                        .left(relative(pct_start))
                                        .w(relative(pct_width))
                                        .top(px(2.0))
                                        .bottom(px(2.0))
                                        .bg(colors::accent())
                                        .rounded(px(2.0))
                                        .opacity(0.7),
                                )
                                // Keyframe dots
                                .children(layer_kf_pcts.iter().enumerate().map(|(i, pct)| {
                                    let kf_pct = *pct;
                                    div()
                                        .id(SharedString::from(format!("kf-dot-{layer_id}-{i}")))
                                        .absolute()
                                        .left(relative(kf_pct))
                                        .top(px(7.0))
                                        .w(px(8.0))
                                        .h(px(8.0))
                                        .bg(gpui::rgb(0xfbbf24))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(gpui::rgb(0x1a1a2e))
                                }))
                                // Click to add keyframes at current frame — records
                                // actual current transform values for x, y, and opacity.
                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                    let frame = this.app.current_frame;
                                    let t = this.app.transforms.get(&layer_id);
                                    let (cur_x, cur_y, cur_opacity) = t
                                        .map(|t| (t.x, t.y, t.opacity))
                                        .unwrap_or((0.0, 0.0, 1.0));
                                    for (prop, val) in [
                                        ("x",       cur_x),
                                        ("y",       cur_y),
                                        ("opacity", cur_opacity),
                                    ] {
                                        this.app.apply(Action::AddKeyframe {
                                            layer_id,
                                            property: prop.to_string(),
                                            frame,
                                            value: val,
                                            easing: EasingKind::Linear,
                                        });
                                    }
                                    cx.notify();
                                })),
                        )
                })),
        )
}
