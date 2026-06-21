//! Tone — GPUI host for the AI-first music creation app.
//!
//! Architecture: GPUI drives the chrome (toolbar, piano roll canvas, mixer
//! dock, timeline strip). All application state lives in [`app_state::App`]
//! and is mutated exclusively through [`app_state::Action`]s emitted by
//! panels and routed through `App::apply`. No wgpu custom passes — Tone's
//! piano roll and waveform displays are CPU-rasterised into GPUI surfaces,
//! matching the Reel pattern.

mod app_state;

use app_state::App;
use gpui::{
    div, px, size, AppContext, Application, Bounds, Context, IntoElement, ParentElement, Render,
    Styled, Window, WindowBounds, WindowOptions,
};
use prism_ui::colors;

/// The GPUI root view. Owns the shared [`App`]; panels read it and route
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Tone {
    app: App,
}

impl Tone {
    fn new() -> Self {
        Self { app: App::new() }
    }
}

impl Render for Tone {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let _app = &self.app;

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Placeholder toolbar
            .child(
                div()
                    .w_full()
                    .h(px(44.0))
                    .bg(colors::surface_raised())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(colors::text_primary())
                            .child(format!(
                                "Tone — {} — {} BPM",
                                self.app.project.name, self.app.project.bpm as u32
                            )),
                    ),
            )
            // Placeholder workspace
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(colors::text_disabled())
                            .child("Tone — AI-first music creation"),
                    ),
            )
            // Placeholder timeline strip
            .child(
                div()
                    .w_full()
                    .h(px(120.0))
                    .bg(colors::surface_raised())
                    .border_t_1()
                    .border_color(colors::surface_border()),
            )
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
            let bounds = cx
                .primary_display()
                .map(|d| d.bounds())
                .unwrap_or_else(|| {
                    Bounds::centered(None, size(px(1600.0), px(1000.0)), cx)
                });
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| cx.new(|_cx| Tone::new()),
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}
