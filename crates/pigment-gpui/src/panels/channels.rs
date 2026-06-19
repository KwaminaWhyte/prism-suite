//! Channels panel — R/G/B/A display-channel visibility toggles.
//!
//! Shows 4 rows (Red, Green, Blue, Alpha). Each row: a colored swatch, the
//! channel name, and an eye-toggle button. Clicking the toggle emits
//! `Action::ToggleChannel(ch)` which flips `app.channel_visibility[ch]` and
//! calls `host.set_channel_mask(...)` → recomposite with that channel zeroed.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, rgb, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, icon_colored, section_header, Icon};

use crate::app_state::{Action, App};
use crate::Pigment;

/// sRGB swatch colors for each channel row (kept as raw rgb values for colored swatches).
const CHANNEL_COLORS: [u32; 4] = [0xcc4444, 0x44cc44, 0x4488cc, 0x888888];
/// Display labels for each channel.
const CHANNEL_NAMES: [&str; 4] = ["Red", "Green", "Blue", "Alpha"];

/// Render the Channels panel. Shows one row per channel with a colored swatch,
/// a label, and a visibility-toggle eye icon button.
pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let vis = app.channel_visibility;

    let rows = div().flex().flex_col().children((0usize..4).map(|ch| {
        let visible = vis[ch];
        let color = CHANNEL_COLORS[ch];
        let name = CHANNEL_NAMES[ch];
        let vis_icon = if visible { Icon::Eye } else { Icon::EyeOff };
        let vis_color = if visible {
            colors::text_primary()
        } else {
            colors::text_disabled()
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py(px(3.0))
            .border_b_1()
            .border_color(colors::surface_border())
            .when(visible, |d: Div| d.bg(colors::surface_overlay()))
            // Colored channel swatch.
            .child(div().w(px(12.0)).h(px(12.0)).rounded_sm().bg(rgb(color)))
            // Channel name.
            .child(
                div()
                    .flex_1()
                    .text_size(px(font_size::SM))
                    .text_color(if visible { colors::text_primary() } else { colors::text_secondary() })
                    .child(name),
            )
            // Eye toggle button.
            .child(
                div()
                    .id(SharedString::from(format!("ch-eye-{ch}")))
                    .w(px(20.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::tool_hover()))
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::ToggleChannel(ch));
                        cx.notify();
                    }))
                    .child(icon_colored(vis_icon, 12.0, vis_color)),
            )
    }));

    div()
        .w_full()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .bg(colors::surface_raised())
        .child(section_header("Channels"))
        .child(divider())
        .child(rows)
}
