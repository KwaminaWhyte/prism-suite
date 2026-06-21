//! Drift — GPUI host for the AI-first animation studio.
//!
//! Architecture:
//! - [`app_state::App`] owns all document, layer, keyframe, rig, and AI state.
//!   Every mutation routes through [`app_state::Action`] and [`app_state::App::apply`].
//! - Panels (to be added) live as `render(app: &App, cx: &mut Context<Drift>)` functions
//!   and emit Actions back through `cx.listener`.
//! - The root view (`Drift`) holds the `App` and lays out the chrome: toolbar,
//!   timeline, canvas, layers panel, AI panel.

mod app_state;

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
struct Drift {
    app: App,
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
        let doc_name = self.app.document.name.clone();

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| this.on_key(ev, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Top toolbar
            .child(
                div()
                    .w_full()
                    .h(px(40.0))
                    .bg(colors::surface_raised())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .flex()
                    .items_center()
                    .px_4()
                    .gap_4()
                    .text_size(px(font_size::SM))
                    .child(
                        div()
                            .text_color(colors::text_primary())
                            .child(format!("Drift — {doc_name}"))
                    )
            )
            // Main workspace: left layers | center canvas | right AI panel
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    // Left: Layers panel
                    .child(
                        div()
                            .id("layers-panel")
                            .w(px(240.0))
                            .h_full()
                            .bg(colors::surface_raised())
                            .border_r_1()
                            .border_color(colors::surface_border())
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("LAYERS")
                            )
                            .children(
                                self.app.layers.iter().map(|l| {
                                    let is_active = self.app.active_layer == Some(l.id);
                                    let bg = if is_active {
                                        colors::surface_overlay()
                                    } else {
                                        colors::surface_raised()
                                    };
                                    div()
                                        .px_3()
                                        .py_1()
                                        .text_size(px(font_size::SM))
                                        .bg(bg)
                                        .child(l.name.clone())
                                })
                            )
                    )
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
                                    ))
                            )
                    )
                    // Right: AI panel
                    .child(
                        div()
                            .w(px(260.0))
                            .h_full()
                            .bg(colors::surface_raised())
                            .border_l_1()
                            .border_color(colors::surface_border())
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_size(px(font_size::XS))
                                    .text_color(colors::text_secondary())
                                    .child("AI TOOLS")
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .text_size(px(font_size::SM))
                                    .text_color(colors::text_primary())
                                    .child(format!(
                                        "Motion prompt: {}",
                                        if self.app.ai_motion_prompt.is_empty() {
                                            "(none)".to_string()
                                        } else {
                                            self.app.ai_motion_prompt.clone()
                                        }
                                    ))
                            )
                    )
            )
            // Bottom: Timeline
            .child(
                div()
                    .w_full()
                    .h(px(160.0))
                    .bg(colors::surface_raised())
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_size(px(font_size::XS))
                            .text_color(colors::text_secondary())
                            .child(format!(
                                "TIMELINE  |  Frame: {frame}  |  In: {}  Out: {}  |  Loop: {}",
                                self.app.in_point,
                                self.app.out_point,
                                if self.app.loop_playback { "ON" } else { "OFF" }
                            ))
                    )
            )
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new().with_assets(PrismAssets).run(|cx: &mut gpui::App| {
        prism_ui::init(cx);
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
