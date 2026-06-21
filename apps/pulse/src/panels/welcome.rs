//! Welcome screen overlay shown on first launch.
//!
//! Renders as an absolutely-positioned full-screen semi-transparent backdrop
//! with a centered 860×540 panel. Uses the same `render(app, cx)` convention
//! as other Pulse panels. The caller wraps it in `.when(app.welcome_visible, |d|
//! d.child(welcome::render(app, cx)))` so it sits on top of the main chrome.

use gpui::{
    div, px, rgb, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::Pulse;

/// Composition presets shown in the right panel of the welcome screen.
struct Preset {
    label: &'static str,
    sub: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset { label: "1920×1080",  sub: "HD / 24 fps"       },
    Preset { label: "3840×2160",  sub: "4K UHD / 24 fps"   },
    Preset { label: "1080×1080",  sub: "Square / 30 fps"   },
    Preset { label: "1280×720",   sub: "720p / 60 fps"     },
    Preset { label: "4096×2160",  sub: "4K Cinema / 23.97 fps" },
    Preset { label: "Custom…",    sub: "Set your own size" },
];

/// Accent purple used for the Pulse logo and CTA buttons.
const ACCENT: u32 = 0x7c5cbf;
/// Sidebar background — a hair darker than the surface.
const SIDEBAR_BG: u32 = 0x18181f;
/// Right-panel background.
const PANEL_BG: u32 = 0x1f1f28;
/// Card background for preset buttons.
const CARD_BG: u32 = 0x2a2a36;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let _ = app; // read-only reference kept for future state reads

    // Full-screen backdrop — absorbs pointer events so the chrome beneath
    // is not accidentally clicked while the welcome screen is up.
    div()
        .absolute()
        .inset_0()
        .size_full()
        .bg(rgba(0x00000099_u32))
        // Centre the dialog card.
        .flex()
        .items_center()
        .justify_center()
        // The dialog card.
        .child(
            div()
                .w(px(860.0))
                .h(px(540.0))
                .rounded(px(12.0))
                .overflow_hidden()
                .flex()
                .flex_row()
                // ── Left sidebar ──────────────────────────────────────────────
                .child(
                    div()
                        .w(px(250.0))
                        .h_full()
                        .flex_shrink_0()
                        .bg(rgb(SIDEBAR_BG))
                        .flex()
                        .flex_col()
                        .p(px(24.0))
                        .gap_4()
                        // App name
                        .child(
                            div()
                                .text_size(px(32.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(rgb(ACCENT))
                                .child("Pulse"),
                        )
                        // Subtitle
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors::text_secondary())
                                .mt(px(-12.0))
                                .child("Motion graphics & VFX compositor"),
                        )
                        // Separator
                        .child(
                            div()
                                .w_full()
                                .h(px(1.0))
                                .bg(colors::surface_border()),
                        )
                        // New Composition button
                        .child(
                            div()
                                .id("welcome-new-comp")
                                .w_full()
                                .h(px(36.0))
                                .rounded(px(6.0))
                                .bg(rgb(ACCENT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .text_size(px(13.0))
                                .text_color(rgb(0xffffff))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::NewComposition);
                                    cx.notify();
                                }))
                                .child("New Composition"),
                        )
                        // Open Project button
                        .child(
                            div()
                                .id("welcome-open-project")
                                .w_full()
                                .h(px(36.0))
                                .rounded(px(6.0))
                                .bg(rgb(CARD_BG))
                                .border_1()
                                .border_color(colors::surface_border())
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .text_size(px(13.0))
                                .text_color(colors::text_primary())
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::OpenProject);
                                    cx.notify();
                                }))
                                .child("Open Project…"),
                        )
                        // Separator
                        .child(
                            div()
                                .w_full()
                                .h(px(1.0))
                                .bg(colors::surface_border()),
                        )
                        // Recent projects label
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(colors::text_secondary())
                                .font_weight(gpui::FontWeight::BOLD)
                                .child("RECENT PROJECTS"),
                        )
                        // Recent project rows (placeholder)
                        .children((0_usize..5).map(|i| {
                            div()
                                .id(("recent-row", i))
                                .w_full()
                                .h(px(28.0))
                                .flex()
                                .items_center()
                                .px_2()
                                .rounded(px(4.0))
                                .text_size(px(12.0))
                                .text_color(colors::text_secondary())
                                .child("(No recent projects)")
                        }))
                        // Push remaining space down
                        .child(div().flex_1())
                        // Dismiss / close link
                        .child(
                            div()
                                .id("welcome-dismiss")
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(colors::text_secondary())
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::DismissWelcome);
                                    cx.notify();
                                }))
                                .child("Skip"),
                        ),
                )
                // ── Right panel ───────────────────────────────────────────────
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .bg(rgb(PANEL_BG))
                        .flex()
                        .flex_col()
                        .p(px(28.0))
                        .gap_4()
                        // Section header
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(colors::text_primary())
                                .child("New composition settings"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors::text_secondary())
                                .mt(px(-8.0))
                                .child("Choose a preset or enter a custom size to get started."),
                        )
                        // Preset grid — 2 columns
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_3()
                                .children(PRESETS.iter().enumerate().map(|(i, preset)| {
                                    div()
                                        .id(("preset-btn", i))
                                        .w(px(175.0))
                                        .h(px(72.0))
                                        .rounded(px(8.0))
                                        .bg(rgb(CARD_BG))
                                        .border_1()
                                        .border_color(colors::surface_border())
                                        .flex()
                                        .flex_col()
                                        .justify_center()
                                        .px(px(14.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|root, _ev, _win, cx| {
                                            root.app.apply(Action::NewComposition);
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(colors::text_primary())
                                                .child(preset.label),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(colors::text_secondary())
                                                .mt(px(2.0))
                                                .child(preset.sub),
                                        )
                                })),
                        )
                        // Push remaining space down
                        .child(div().flex_1())
                        // Footer note
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(colors::text_secondary())
                                .child("All settings can be changed later via Composition > Composition Settings."),
                        ),
                ),
        )
}
