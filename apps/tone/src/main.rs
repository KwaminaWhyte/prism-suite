//! Tone — GPUI host for the AI-first music creation app.
//!
//! Architecture: GPUI drives the chrome (toolbar, piano roll canvas, mixer
//! dock, timeline strip). All application state lives in [`app_state::App`]
//! and is mutated exclusively through [`app_state::Action`]s emitted by
//! panels and routed through `App::apply`. No wgpu custom passes — Tone's
//! piano roll and waveform displays are CPU-rasterised into GPUI surfaces,
//! matching the Reel pattern.

mod app_state;
mod panels;
mod welcome;

use app_state::App;
use gpui::{
    div, px, size, AppContext, Bounds, Context, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, WindowBounds,
    WindowOptions,
};
use prism_ui::colors;
use welcome::WelcomeView;

/// The GPUI root view. Owns the shared [`App`]; panels read it and route
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Tone {
    app: App,
    focus: FocusHandle,
}

impl Focusable for Tone {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Tone {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui::prelude::FluentBuilder;
        let has_active_clip = self.app.piano_roll_clip.is_some();

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Transport toolbar
            .child(panels::render_toolbar(&self.app, cx))
            // Primary view: arrangement (clip lanes, track list) — fills remaining space
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(panels::render_timeline(&self.app, cx)),
            )
            // Piano roll — only visible when a clip is focused (bottom split, 260px)
            .when(has_active_clip, |d| {
                d.child(
                    div()
                        .w_full()
                        .h(px(260.0))
                        .flex()
                        .flex_row()
                        .border_t_1()
                        .border_color(colors::surface_border())
                        // Left: track stub (aligns with arrangement label column)
                        .child(
                            div()
                                .w(px(panels::TRACKS_W))
                                .h_full()
                                .flex_shrink_0()
                                .bg(colors::surface_raised())
                                .border_r_1()
                                .border_color(colors::surface_border())
                                .flex()
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(colors::text_disabled())
                                        .child("PIANO ROLL"),
                                ),
                        )
                        // Right: piano roll canvas
                        .child(panels::render_piano_roll(&self.app, cx)),
                )
            })
            // Mixer strip — always visible at bottom
            .child(panels::render_mixer(&self.app, cx))
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new()
        .with_assets(prism_ui::PrismAssets)
        .run(|cx: &mut gpui::App| {
            prism_ui::init(cx);

            // Welcome window
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(900.0), px(560.0)),
                        cx,
                    ))),
                    ..Default::default()
                },
                |_window, cx| cx.new(|_cx| WelcomeView::new()),
            )
            .expect("failed to open welcome window");

            // Main editor window
            let bounds = cx
                .primary_display()
                .map(|d| d.bounds())
                .unwrap_or_else(|| Bounds::centered(None, size(px(1600.0), px(1000.0)), cx));
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        let focus = cx.focus_handle();
                        window.focus(&focus);
                        Tone { app: App::new(), focus }
                    })
                },
            )
            .expect("failed to open main window");

            cx.activate(true);
        });
}
