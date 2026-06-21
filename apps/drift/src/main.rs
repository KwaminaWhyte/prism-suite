//! Drift — GPUI host for the AI-first animation studio.
//!
//! Architecture:
//! - [`app_state::App`] owns all document, layer, keyframe, rig, and AI state.
//!   Every mutation routes through [`app_state::Action`] and [`app_state::App::apply`].
//! - Panels in `panels/` are render functions `(app: &App, cx: &mut Context<Drift>)`.
//!   They emit Actions back through `cx.listener`.
//! - The root view (`Drift`) holds the `App` and lays out the chrome: toolbar,
//!   timeline, canvas, layers panel, AI panel.
//! - A welcome window (`welcome::WelcomeView`) is opened at startup.

mod app_state;
mod panels;
mod welcome;

use prism_ui::PrismAssets;

use app_state::{Action, App};
use gpui::{
    div, px, size, AppContext, Bounds, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, StatefulInteractiveElement, Styled, Window,
    WindowBounds, WindowOptions,
};
use prism_ui::{colors, font_size};

/// The GPUI root view. Owns the shared [`App`]; panels read it and route their
/// mutations back through `app.apply` inside `cx.listener` callbacks.
pub struct Drift {
    pub app: App,
    focus: FocusHandle,
}

impl Focusable for Drift {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Drift {
    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        if m.platform && !m.alt && !m.control {
            match ks.key.as_str() {
                "z" if m.shift => {
                    // Redo (future: wire to history)
                    cx.notify();
                    return;
                }
                "z" => {
                    // Undo (future: wire to history)
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        // Spacebar: play/pause
        if !m.platform && !m.control && !m.alt && !m.shift {
            match ks.key.as_str() {
                " " => {
                    if self.app.playing {
                        self.app.apply(Action::Pause);
                    } else {
                        self.app.apply(Action::Play);
                    }
                    cx.notify();
                }
                "left" => {
                    self.app.apply(Action::StepBackward);
                    cx.notify();
                }
                "right" => {
                    self.app.apply(Action::StepForward);
                    cx.notify();
                }
                "home" => {
                    self.app.apply(Action::GoToFirstFrame);
                    cx.notify();
                }
                "end" => {
                    self.app.apply(Action::GoToLastFrame);
                    cx.notify();
                }
                _ => {}
            }
        }
    }
}

impl Render for Drift {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = self.app.current_frame;
        let total = self.app.document.duration_frames;

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| this.on_key(ev, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Top toolbar — tool buttons + playback controls
            .child(panels::render_toolbar(&self.app, cx))
            // Main workspace: left layers | center canvas | right AI panel
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    // Left: Layers panel
                    .child(panels::render_layers(&self.app, cx))
                    // Center: Canvas area
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .bg(colors::surface_bg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w(px(self.app.document.width as f32))
                                    .h(px(self.app.document.height as f32))
                                    .bg(gpui::rgb(0x1a1a2e))
                                    .border_1()
                                    .border_color(colors::surface_border())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(font_size::SM))
                                    .text_color(colors::text_secondary())
                                    .child(format!(
                                        "Frame {frame} / {total}  ·  {}x{} @ {}fps",
                                        self.app.document.width,
                                        self.app.document.height,
                                        self.app.document.fps
                                    )),
                            ),
                    )
                    // Right: AI panel
                    .child(panels::render_ai_panel(&self.app, cx)),
            )
            // Bottom: Timeline
            .child(panels::render_timeline(&self.app, cx))
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new().with_assets(PrismAssets).run(|cx: &mut gpui::App| {
        prism_ui::init(cx);

        // Open the welcome window first (900 × 560 px, centered).
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(
                    Bounds::centered(None, size(px(900.0), px(560.0)), cx),
                )),
                ..Default::default()
            },
            |_win, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    welcome::WelcomeView::new(focus)
                })
            },
        )
        .expect("failed to open Drift welcome window");

        // Open the main editor window (full display or sensible default).
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
                    let app = App::new();
                    let focus = cx.focus_handle();
                    window.focus(&focus);
                    Drift { app, focus }
                })
            },
        )
        .expect("failed to open Drift window");

        cx.activate(true);
    });
}
