//! Welcome screen for Reel — an OS-level child window opened at launch.
//!
//! Shown before the main editor window is focused. Clicking any action
//! (new project, open, or a preset card) closes this window via
//! `win.remove_window()`.

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, div, px,
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
                    .child("Reel"),
            )
            // Subtitle
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_secondary())
                    .mb(px(12.0))
                    .child("Professional video editor"),
            )
            // Divider
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(colors::surface_border())
                    .my(px(8.0)),
            )
            // New Project button
            .child(
                div()
                    .id("welcome-new")
                    .w_full()
                    .px(px(12.0))
                    .py(px(7.0))
                    .text_size(px(font_size::MD))
                    .text_color(colors::text_primary())
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::tool_hover()))
                    .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                        win.remove_window();
                    }))
                    .child("New Project"),
            )
            // Open Project button
            .child(
                div()
                    .id("welcome-open")
                    .w_full()
                    .px(px(12.0))
                    .py(px(7.0))
                    .text_size(px(font_size::MD))
                    .text_color(colors::text_primary())
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::tool_hover()))
                    .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                        win.remove_window();
                    }))
                    .child("Open Project..."),
            )
            // Divider
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(colors::surface_border())
                    .my(px(8.0)),
            )
            // Recent projects label
            .child(
                div()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .mb(px(4.0))
                    .child("RECENT PROJECTS"),
            )
            // Five placeholder recent rows
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent projects)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent projects)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent projects)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent projects)"),
            )
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent projects)"),
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
                    .child("NEW PROJECT SETTINGS"),
            )
            // Row 1
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    // HD preset
                    .child(
                        div()
                            .id("preset-hd")
                            .flex_1()
                            .min_w(px(180.0))
                            .p(px(10.0))
                            .bg(colors::surface_raised())
                            .border_1()
                            .border_color(colors::surface_border())
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                                win.remove_window();
                            }))
                            .child(
                                div()
                                    .text_size(px(font_size::MD))
                                    .text_color(colors::text_primary())
                                    .child("HD"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("1920×1080 / 23.97 fps"),
                            ),
                    )
                    // 4K preset
                    .child(
                        div()
                            .id("preset-4k")
                            .flex_1()
                            .min_w(px(180.0))
                            .p(px(10.0))
                            .bg(colors::surface_raised())
                            .border_1()
                            .border_color(colors::surface_border())
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                                win.remove_window();
                            }))
                            .child(
                                div()
                                    .text_size(px(font_size::MD))
                                    .text_color(colors::text_primary())
                                    .child("4K"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("3840×2160 / 24 fps"),
                            ),
                    ),
            )
            // Row 2
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    // 720p preset
                    .child(
                        div()
                            .id("preset-720p")
                            .flex_1()
                            .min_w(px(180.0))
                            .p(px(10.0))
                            .bg(colors::surface_raised())
                            .border_1()
                            .border_color(colors::surface_border())
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                                win.remove_window();
                            }))
                            .child(
                                div()
                                    .text_size(px(font_size::MD))
                                    .text_color(colors::text_primary())
                                    .child("720p"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("1280×720 / 60 fps"),
                            ),
                    )
                    // Vertical preset
                    .child(
                        div()
                            .id("preset-vertical")
                            .flex_1()
                            .min_w(px(180.0))
                            .p(px(10.0))
                            .bg(colors::surface_raised())
                            .border_1()
                            .border_color(colors::surface_border())
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                                win.remove_window();
                            }))
                            .child(
                                div()
                                    .text_size(px(font_size::MD))
                                    .text_color(colors::text_primary())
                                    .child("Vertical"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("1080×1920 / 30 fps"),
                            ),
                    ),
            )
            // Row 3
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    // DCI 2K preset
                    .child(
                        div()
                            .id("preset-dci")
                            .flex_1()
                            .min_w(px(180.0))
                            .p(px(10.0))
                            .bg(colors::surface_raised())
                            .border_1()
                            .border_color(colors::surface_border())
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                                win.remove_window();
                            }))
                            .child(
                                div()
                                    .text_size(px(font_size::MD))
                                    .text_color(colors::text_primary())
                                    .child("DCI 2K"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("2048×1080 / 24 fps"),
                            ),
                    )
                    // Custom preset
                    .child(
                        div()
                            .id("preset-custom")
                            .flex_1()
                            .min_w(px(180.0))
                            .p(px(10.0))
                            .bg(colors::surface_raised())
                            .border_1()
                            .border_color(colors::surface_border())
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|_this, _ev: &ClickEvent, win, _cx| {
                                win.remove_window();
                            }))
                            .child(
                                div()
                                    .text_size(px(font_size::MD))
                                    .text_color(colors::text_primary())
                                    .child("Custom..."),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("Set your own resolution & fps"),
                            ),
                    ),
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
