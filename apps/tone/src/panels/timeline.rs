//! Timeline strip — bar ruler + clip lanes per track.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size};

use crate::app_state::App;
use crate::Tone;

const LABEL_W: f32 = 220.0;
const VISIBLE_BARS: f32 = 32.0;

pub fn render_timeline(app: &App, _cx: &mut Context<Tone>) -> impl IntoElement {
    div()
        .w_full()
        .h(px(140.0))
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
                        .map(|track| {
                            let track_name = track.name.clone();
                            let track_id = track.id;
                            let track_clips: Vec<_> = app
                                .clips
                                .iter()
                                .filter(|c| c.track_id == track_id)
                                .collect();

                            div()
                                .w_full()
                                .h(px(24.0))
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
                                        .border_r_1()
                                        .border_color(colors::surface_border())
                                        .flex_shrink_0()
                                        .text_size(px(font_size::XS))
                                        .text_color(colors::text_secondary())
                                        .overflow_hidden()
                                        .child(track_name),
                                )
                                // Clip blocks in relative-positioned container
                                .child(
                                    div()
                                        .flex_1()
                                        .h_full()
                                        .relative()
                                        .overflow_hidden()
                                        .children(track_clips.iter().map(|clip| {
                                            let start_pct =
                                                (clip.start_beat / VISIBLE_BARS).clamp(0.0, 1.0);
                                            let width_pct =
                                                (clip.duration_beats / VISIBLE_BARS).max(0.02);
                                            let clip_name = clip.name.clone();
                                            let muted = clip.muted;

                                            div()
                                                .absolute()
                                                .left(gpui::relative(start_pct))
                                                .w(gpui::relative(width_pct))
                                                .top(px(2.0))
                                                .bottom(px(2.0))
                                                .bg(colors::accent())
                                                .rounded(px(2.0))
                                                .opacity(if muted { 0.3 } else { 0.7 })
                                                .overflow_hidden()
                                                .text_size(px(7.0))
                                                .text_color(gpui::rgb(0xffffff))
                                                .pl_1()
                                                .child(clip_name)
                                        })),
                                )
                        })
                        .collect::<Vec<_>>(),
                ),
        )
}
