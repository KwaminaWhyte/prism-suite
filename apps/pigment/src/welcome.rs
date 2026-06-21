//! Welcome screen — a secondary OS-level window shown on launch when no
//! document is open. Mirrors Photoshop's Home Screen in spirit: app identity on
//! the left, template cards on the right. Closes itself when the user clicks
//! "New Document" or "Open File…".

use gpui::{
    div, px, Focusable, FocusHandle, IntoElement, ParentElement, Render, Styled, Window,
    Context, InteractiveElement, StatefulInteractiveElement,
};
use prism_ui::{colors, font_size};

pub struct WelcomeView {
    pub focus: FocusHandle,
}

impl WelcomeView {
    pub fn new(focus: FocusHandle) -> Self {
        Self { focus }
    }
}

impl Focusable for WelcomeView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

// ── template card ─────────────────────────────────────────────────────────────

fn template_card(name: &'static str) -> impl IntoElement {
    div()
        .w(px(180.0))
        .h(px(120.0))
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.border_color(colors::accent()))
        .child(
            div()
                .text_size(px(font_size::SM))
                .text_color(colors::text_secondary())
                .child(name),
        )
}

// ── render ───────────────────────────────────────────────────────────────────

impl Render for WelcomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_row()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // ── left column: identity + actions + recents ─────────────────
            .child(
                div()
                    .w(px(280.0))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .px(px(24.0))
                    .py(px(32.0))
                    .gap_3()
                    .bg(colors::surface_raised())
                    .border_r_1()
                    .border_color(colors::surface_border())
                    // App title
                    .child(
                        div()
                            .text_size(px(28.0))
                            .text_color(colors::accent())
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Pigment"),
                    )
                    // Subtitle
                    .child(
                        div()
                            .text_size(px(font_size::SM))
                            .text_color(colors::text_secondary())
                            .mb_3()
                            .child("Professional raster image editor"),
                    )
                    // Divider
                    .child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(colors::surface_border())
                            .mb_1(),
                    )
                    // New Document button
                    .child(
                        div()
                            .id("welcome-new-doc")
                            .w_full()
                            .px_3()
                            .py(px(7.0))
                            .rounded_md()
                            .bg(colors::accent())
                            .text_size(px(font_size::MD))
                            .text_color(colors::text_primary())
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::accent_hover()))
                            .on_click(cx.listener(|_this, _ev, win, cx| {
                                win.remove_window();
                            }))
                            .child("New Document"),
                    )
                    // Open File button
                    .child(
                        div()
                            .id("welcome-open-file")
                            .w_full()
                            .px_3()
                            .py(px(7.0))
                            .rounded_md()
                            .bg(colors::surface_overlay())
                            .text_size(px(font_size::MD))
                            .text_color(colors::text_secondary())
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::surface_border()))
                            .on_click(cx.listener(|_this, _ev, win, cx| {
                                win.remove_window();
                            }))
                            .child("Open File…"),
                    )
                    // Divider
                    .child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(colors::surface_border())
                            .mt_2()
                            .mb_1(),
                    )
                    // Recent Files header
                    .child(
                        div()
                            .text_size(px(font_size::XS))
                            .text_color(colors::text_disabled())
                            .mb_1()
                            .child("RECENT FILES"),
                    )
                    // Five placeholder recent-file rows (empty state message)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(font_size::SM))
                                    .text_color(colors::text_disabled())
                                    .px_2()
                                    .py_1()
                                    .child("No recent files"),
                            ),
                    ),
            )
            // ── right column: template grid ───────────────────────────────
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .px(px(32.0))
                    .py(px(32.0))
                    .gap_4()
                    // Header
                    .child(
                        div()
                            .text_size(px(font_size::LG))
                            .text_color(colors::text_primary())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Start from a template"),
                    )
                    // 2×3 grid of template cards
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Row 1
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_3()
                                    .child(template_card("Photo Editing"))
                                    .child(template_card("Illustration"))
                                    .child(template_card("Web Design")),
                            )
                            // Row 2
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_3()
                                    .child(template_card("Print"))
                                    .child(template_card("Social Media"))
                                    .child(template_card("Custom Size")),
                            ),
                    ),
            )
    }
}
