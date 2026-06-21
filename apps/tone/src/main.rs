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
        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .child(panels::render_toolbar(&self.app, cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    .child(panels::render_tracks(&self.app, cx))
                    .child(panels::render_piano_roll(&self.app, cx)),
            )
            .child(panels::render_timeline(&self.app, cx))
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
