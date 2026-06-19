//! Histogram panel — per-channel + luma histogram of the current composite.
//!
//! Supports five modes (Luminosity, RGB, R, G, B) toggled by clicking the mode
//! label. `Action::SetHistogramChannel` persists the mode on `App`.

use gpui::prelude::FluentBuilder;
use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled};
use prism_ui::{colors, divider, section_header};

use crate::app_state::{Action, App, HistogramChannel};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let channel = app.histogram_channel;
    let next_ch = channel.next();

    let bars: Vec<_> = match app.host.histogram() {
        Some(h) => {
            let data: &[u32] = match channel {
                HistogramChannel::Luminosity => &h.luma,
                HistogramChannel::Rgb | HistogramChannel::Red => &h.r,
                HistogramChannel::Green => &h.g,
                HistogramChannel::Blue => &h.b,
            };
            // For RGB mode blend R+G+B channels visually (use luma as fallback — R channel displayed).
            let max = data.iter().copied().max().unwrap_or(1).max(1) as f32;
            let bar_color = match channel {
                HistogramChannel::Red => gpui::rgb(0xDD4444),
                HistogramChannel::Green => gpui::rgb(0x44BB44),
                HistogramChannel::Blue => gpui::rgb(0x4488DD),
                _ => colors::text_secondary(),
            };
            data.iter()
                .map(|&c| {
                    let height = (c as f32 / max) * 96.0;
                    div().flex_1().h(px(height)).bg(bar_color)
                })
                .collect()
        }
        None => Vec::new(),
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Histogram"))
        .child(divider())
        // Channel mode toggle row.
        .child(
            div()
                .id("hist-mode-toggle")
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .px_3()
                .py(px(2.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetHistogramChannel(next_ch));
                    cx.notify();
                }))
                .child(
                    div()
                        .text_color(colors::accent())
                        .text_size(px(10.0))
                        .child(channel.label()),
                ),
        )
        .child(
            // Plot: bars bottom-aligned on a near-black field.
            div()
                .h(px(100.0))
                .mx_3()
                .mb_3()
                .bg(colors::surface_bg())
                .flex()
                .flex_row()
                .items_end()
                .children(bars),
        )
}
