//! Welcome screen — shown on first launch before a project is opened.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window,
};
use prism_ui::{colors, font_size};

static TEMPLATES: &[(&str, &str)] = &[
    ("Electronic", "128 BPM"),
    ("Hip-Hop", "90 BPM"),
    ("Rock", "120 BPM"),
    ("Ambient", "75 BPM"),
    ("Jazz", "100 BPM"),
    ("Custom...", ""),
];

pub struct WelcomeView;

impl WelcomeView {
    pub fn new() -> Self {
        Self
    }
}

impl Render for WelcomeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(colors::surface_bg())
            .font_family(".SystemUIFont")
            // Left column
            .child(
                div()
                    .w(px(260.0))
                    .h_full()
                    .bg(colors::surface_raised())
                    .border_r_1()
                    .border_color(colors::surface_border())
                    .flex()
                    .flex_col()
                    .px_6()
                    .pt_8()
                    .pb_6()
                    // App title
                    .child(
                        div()
                            .text_size(px(28.0))
                            .text_color(colors::accent())
                            .child("Tone"),
                    )
                    .child(
                        div()
                            .text_size(px(font_size::SM))
                            .text_color(colors::text_secondary())
                            .mt_1()
                            .mb_4()
                            .child("AI-first music creation"),
                    )
                    // Separator
                    .child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(colors::surface_border())
                            .mb_4(),
                    )
                    // New Project button
                    .child(
                        div()
                            .id("new-project")
                            .w_full()
                            .h(px(36.0))
                            .bg(colors::accent())
                            .rounded(px(4.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(font_size::SM))
                            .text_color(gpui::rgb(0xffffff))
                            .cursor_pointer()
                            .mb_2()
                            .child("New Project"),
                    )
                    // Open Project button
                    .child(
                        div()
                            .id("open-project")
                            .w_full()
                            .h(px(36.0))
                            .bg(colors::surface_overlay())
                            .rounded(px(4.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(font_size::SM))
                            .text_color(colors::text_primary())
                            .cursor_pointer()
                            .mb_6()
                            .child("Open Project..."),
                    )
                    // Separator
                    .child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(colors::surface_border())
                            .mb_4(),
                    )
                    // Recent projects
                    .child(
                        div()
                            .text_size(px(font_size::XS))
                            .text_color(colors::text_secondary())
                            .mb_2()
                            .child("RECENT PROJECTS"),
                    )
                    .child(
                        div()
                            .text_size(px(font_size::SM))
                            .text_color(colors::text_disabled())
                            .child("(No recent projects)"),
                    ),
            )
            // Right column
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .px_8()
                    .pt_8()
                    .pb_6()
                    .child(
                        div()
                            .text_size(px(font_size::LG))
                            .text_color(colors::text_primary())
                            .mb_4()
                            .child("Start with a template"),
                    )
                    // Template grid (2 columns)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_3()
                            .children(TEMPLATES.iter().enumerate().map(|(i, (name, bpm))| {
                                let label = if bpm.is_empty() {
                                    name.to_string()
                                } else {
                                    format!("{}\n{}", name, bpm)
                                };
                                div()
                                    .id(("template", i))
                                    .w(px(180.0))
                                    .h(px(80.0))
                                    .bg(colors::surface_raised())
                                    .border_1()
                                    .border_color(colors::surface_border())
                                    .rounded(px(6.0))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_size(px(font_size::SM))
                                            .text_color(colors::text_primary())
                                            .child(name.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(font_size::XS))
                                            .text_color(colors::text_secondary())
                                            .mt_1()
                                            .child(bpm.to_string()),
                                    )
                            })),
                    ),
            )
    }
}
