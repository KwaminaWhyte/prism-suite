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

pub fn render_timeline(app: &App, cx: &mut Context<Drift>) -> impl IntoElement {
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
                .child(div().flex_1())
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
        // 2. Frame ruler — tick at every second
        .child({
            let step = fps.max(1);
            let max_ticks = total / step + 1;
            div()
                .w_full()
                .h(px(20.0))
                .bg(colors::surface_bg())
                .border_b_1()
                .border_color(colors::surface_border())
                .flex()
                .items_center()
                .px(px(240.0)) // offset for layer name column
                .children((0..=max_ticks.min(60)).map(|i| {
                    let f = i * step;
                    div()
                        .flex_1()
                        .text_size(px(8.0))
                        .text_color(colors::text_disabled())
                        .child(format!("{f}"))
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
                                // Click to add keyframe at current frame
                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                    let frame = this.app.current_frame;
                                    this.app.apply(Action::AddKeyframe {
                                        layer_id,
                                        property: "position_x".to_string(),
                                        frame,
                                        value: 0.0,
                                        easing: EasingKind::Linear,
                                    });
                                    cx.notify();
                                })),
                        )
                })),
        )
}
