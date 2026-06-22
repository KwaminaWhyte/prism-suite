//! Welcome screen for Reel — an OS-level child window opened at launch.
//!
//! Shown before the main editor window is focused. Clicking any action
//! (new project, open, or a preset card) dispatches the corresponding Action
//! to the main Reel entity and then closes this window.

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::Action;
use crate::Reel;

pub struct WelcomeView {
    focus: FocusHandle,
    app_entity: WeakEntity<Reel>,
}

impl Focusable for WelcomeView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl WelcomeView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Reel>) -> Self {
        Self { focus, app_entity }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for WelcomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ae = self.app_entity.clone();
        let ae_open = self.app_entity.clone();

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
                    .on_click(cx.listener(move |_this, _ev: &ClickEvent, win, cx| {
                        if let Some(entity) = ae.upgrade() {
                            entity.update(cx, |app, cx| {
                                // Default new-project: HD 1920×1080 at 23.97 fps
                                app.app.apply(Action::NewProject {
                                    width: 1920,
                                    height: 1080,
                                    frame_rate: 23.976,
                                });
                                cx.notify();
                            });
                        }
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
                    .on_click(cx.listener(move |_this, _ev: &ClickEvent, win, cx| {
                        if let Some(entity) = ae_open.upgrade() {
                            entity.update(cx, |app, cx| {
                                app.app.apply(Action::OpenFile);
                                cx.notify();
                            });
                        }
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
            // Single empty state row
            .child(
                div()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_disabled())
                    .px(px(4.0))
                    .py(px(5.0))
                    .child("(No recent projects)"),
            );

        // ----------------------------------------------------------------
        // Preset card helper (inline closure captures app_entity clone per card)
        // ----------------------------------------------------------------
        let make_card = |id: &'static str, label: &'static str, sub: &'static str,
                         width: u32, height: u32, fps: f64,
                         entity: WeakEntity<Reel>,
                         cx: &mut Context<WelcomeView>| {
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
                .hover(|s| s.bg(colors::tool_hover()))
                .on_click(cx.listener(move |_this, _ev: &ClickEvent, win, cx| {
                    if let Some(e) = entity.upgrade() {
                        e.update(cx, |app, cx| {
                            app.app.apply(Action::NewProject { width, height, frame_rate: fps });
                            cx.notify();
                        });
                    }
                    win.remove_window();
                }))
                .child(
                    div()
                        .text_size(px(font_size::MD))
                        .text_color(colors::text_primary())
                        .child(label),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child(sub),
                )
        };

        // ----------------------------------------------------------------
        // Right column — preset cards
        // ----------------------------------------------------------------
        let ae1 = self.app_entity.clone();
        let ae2 = self.app_entity.clone();
        let ae3 = self.app_entity.clone();
        let ae4 = self.app_entity.clone();
        let ae5 = self.app_entity.clone();
        let ae6 = self.app_entity.clone();

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
                    .child(make_card("preset-hd", "HD", "1920×1080 / 23.97 fps",
                        1920, 1080, 23.976, ae1, cx))
                    .child(make_card("preset-4k", "4K", "3840×2160 / 24 fps",
                        3840, 2160, 24.0, ae2, cx)),
            )
            // Row 2
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    .child(make_card("preset-720p", "720p", "1280×720 / 60 fps",
                        1280, 720, 60.0, ae3, cx))
                    .child(make_card("preset-vertical", "Vertical", "1080×1920 / 30 fps",
                        1080, 1920, 30.0, ae4, cx)),
            )
            // Row 3
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.0))
                    .child(make_card("preset-dci", "DCI 2K", "2048×1080 / 24 fps",
                        2048, 1080, 24.0, ae5, cx))
                    .child(make_card("preset-custom", "Custom...", "Set your own resolution & fps",
                        1920, 1080, 24.0, ae6, cx)),
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
