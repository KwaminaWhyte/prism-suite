//! Welcome screen for Drift — an OS-level child window opened at launch.
//!
//! Left sidebar: app name, subtitle, "New Animation", "Open File...", recent files list.
//! Right column: preset grid (resolution / fps combinations).
//!
//! Any action button closes this window via `win.remove_window()`.

use gpui::{
    div, px, ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};
use prism_ui::{colors, font_size};

pub struct WelcomeView {
    focus: FocusHandle,
}

impl Focusable for WelcomeView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl WelcomeView {
    pub fn new(focus: FocusHandle) -> Self {
        Self { focus }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sidebar action button — full-width, hover highlight.
fn sidebar_btn(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .px(px(12.0))
        .py(px(7.0))
        .text_size(px(font_size::MD))
        .text_color(colors::text_primary())
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(|s: gpui::StyleRefinement| s.bg(colors::tool_hover()))
        .on_click(on_click)
        .child(label)
}

/// Preset card for the right-column grid.
fn preset_card(
    id: &'static str,
    name: &'static str,
    subtitle: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_1()
        .min_w(px(180.0))
        .p(px(10.0))
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .rounded(px(5.0))
        .cursor_pointer()
        .hover(|s: gpui::StyleRefinement| s.bg(colors::tool_hover()))
        .on_click(on_click)
        .child(
            div()
                .text_size(px(font_size::MD))
                .text_color(colors::text_primary())
                .child(name),
        )
        .child(
            div()
                .mt(px(3.0))
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(subtitle),
        )
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for WelcomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ----------------------------------------------------------------
        // Left sidebar
        // ----------------------------------------------------------------
        let left_col = div()
            .w(px(260.0))
            .h_full()
            .bg(colors::surface_raised())
            .border_r_1()
            .border_color(colors::surface_border())
            .flex()
            .flex_col()
            .p(px(16.0))
            .gap(px(2.0))
            // App name
            .child(
                div()
                    .text_size(px(28.0))
                    .text_color(colors::accent())
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Drift"),
            )
            // Subtitle
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_secondary())
                    .mb(px(12.0))
                    .child("AI-first animation studio"),
            )
            // Divider
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(colors::surface_border())
                    .my(px(8.0)),
            )
            // New Animation button
            .child(sidebar_btn(
                "welcome-new",
                "New Animation",
                cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                    win.remove_window();
                }),
            ))
            // Open File button
            .child(sidebar_btn(
                "welcome-open",
                "Open File...",
                cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                    win.remove_window();
                }),
            ))
            // Divider
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(colors::surface_border())
                    .my(px(8.0)),
            )
            // Recent files label
            .child(
                div()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .mb(px(4.0))
                    .child("RECENT FILES"),
            )
            // Placeholder recent file rows
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent files)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent files)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent files)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent files)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent files)"),
            );

        // ----------------------------------------------------------------
        // Right column — preset cards
        // ----------------------------------------------------------------
        let right_col = div()
            .flex_1()
            .h_full()
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Section header
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_secondary())
                    .mb(px(4.0))
                    .child("NEW ANIMATION PRESETS"),
            )
            // Row 1
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    .child(preset_card(
                        "preset-hd24",
                        "HD 24fps",
                        "1920\u{00d7}1080 / 24fps",
                        cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                            win.remove_window();
                        }),
                    ))
                    .child(preset_card(
                        "preset-hd30",
                        "HD 30fps",
                        "1920\u{00d7}1080 / 30fps",
                        cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                            win.remove_window();
                        }),
                    )),
            )
            // Row 2
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    .child(preset_card(
                        "preset-720p",
                        "720p",
                        "1280\u{00d7}720 / 24fps",
                        cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                            win.remove_window();
                        }),
                    ))
                    .child(preset_card(
                        "preset-4k",
                        "4K",
                        "4096\u{00d7}2304 / 24fps",
                        cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                            win.remove_window();
                        }),
                    )),
            )
            // Row 3
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    .child(preset_card(
                        "preset-gif",
                        "GIF / Web",
                        "512\u{00d7}512 / 24fps",
                        cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                            win.remove_window();
                        }),
                    ))
                    .child(preset_card(
                        "preset-custom",
                        "Custom...",
                        "Set your own size & fps",
                        cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                            win.remove_window();
                        }),
                    )),
            );

        // ----------------------------------------------------------------
        // Root layout
        // ----------------------------------------------------------------
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .child(left_col)
            .child(right_col)
    }
}
