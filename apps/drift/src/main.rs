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
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size};

/// The GPUI root view. Owns the shared [`App`]; panels read it and route their
/// mutations back through `app.apply` inside `cx.listener` callbacks.
pub struct Drift {
    pub app: App,
    focus: FocusHandle,
    last_tick: Option<std::time::Instant>,
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
                    self.app.apply(Action::Redo);
                    cx.notify();
                    return;
                }
                "z" => {
                    self.app.apply(Action::Undo);
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
                "delete" | "backspace" => {
                    if let Some(lid) = self.app.active_layer {
                        self.app.apply(Action::DeleteLayer(lid));
                        cx.notify();
                    }
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
        // ── Transport tick ────────────────────────────────────────────────────
        if self.app.playing {
            let now = std::time::Instant::now();
            if let Some(last) = self.last_tick {
                let elapsed = now.duration_since(last).as_secs_f32();
                let fps = self.app.document.fps as f32;
                let frames = (elapsed * fps) as usize;
                if frames > 0 {
                    let max_frame = self.app.document.duration_frames.saturating_sub(1);
                    let new_frame = self.app.current_frame + frames;
                    if new_frame >= max_frame {
                        self.app.current_frame = 0; // loop back
                    } else {
                        self.app.current_frame = new_frame;
                    }
                }
            }
            self.last_tick = Some(now);
            cx.notify();
        } else {
            self.last_tick = None;
        }

        let has_layers = !self.app.layers.is_empty();
        let stage_w = self.app.document.width as f32 * 0.5;
        let stage_h = self.app.document.height as f32 * 0.5;

        // Collect visible layers for canvas rendering.
        let visible_layers: Vec<_> = self
            .app
            .layers
            .iter()
            .filter(|l| l.visible)
            .enumerate()
            .map(|(idx, layer)| {
                let layer_id = layer.id;
                let is_active = self.app.active_layer == Some(layer_id);
                let (tx, ty) = self
                    .app
                    .transforms
                    .get(&layer_id)
                    .map(|t| (t.x, t.y))
                    .unwrap_or((0.0, 0.0));
                let opacity = self
                    .app
                    .transforms
                    .get(&layer_id)
                    .map(|t| t.opacity)
                    .unwrap_or(1.0);
                let layer_color = match idx % 6 {
                    0 => gpui::rgb(0x6366f1),
                    1 => gpui::rgb(0x22d3ee),
                    2 => gpui::rgb(0xf59e0b),
                    3 => gpui::rgb(0x10b981),
                    4 => gpui::rgb(0xf43f5e),
                    _ => gpui::rgb(0xa78bfa),
                };
                let name = layer.name.clone();
                (layer_id, is_active, tx, ty, opacity, layer_color, name)
            })
            .collect();

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
                            .id("canvas")
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .bg(colors::surface_bg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .id("stage")
                                    .w(px(stage_w))
                                    .h(px(stage_h))
                                    .bg(gpui::rgb(0x1a1a2e))
                                    .border_1()
                                    .border_color(colors::surface_border())
                                    .relative()
                                    .overflow_hidden()
                                    // Render visible layers as colored labeled boxes
                                    .children(visible_layers.iter().map(
                                        |(layer_id, is_active, tx, ty, opacity, layer_color, name)| {
                                            let layer_id = *layer_id;
                                            let is_active = *is_active;
                                            let left = tx * 0.5 + 60.0;
                                            let top = ty * 0.5 + 40.0;
                                            div()
                                                .id(("layer-vis", layer_id))
                                                .absolute()
                                                .left(px(left))
                                                .top(px(top))
                                                .w(px(120.0))
                                                .h(px(80.0))
                                                .bg(*layer_color)
                                                .opacity(*opacity)
                                                .border_2()
                                                .border_color(if is_active {
                                                    gpui::rgb(0xffffff)
                                                } else {
                                                    gpui::rgba(0xffffff22)
                                                })
                                                .rounded(px(4.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(10.0))
                                                .text_color(gpui::rgb(0xffffff))
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                    this.app.apply(Action::SetActiveLayer(layer_id));
                                                    cx.notify();
                                                }))
                                                .child(name.clone())
                                        },
                                    ))
                                    // Empty state hint
                                    .when(!has_layers, |el: gpui::Stateful<gpui::Div>| {
                                        el.flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .text_size(px(font_size::SM))
                                                    .text_color(colors::text_secondary())
                                                    .child(
                                                        "Click + in the Layers panel to add a layer",
                                                    ),
                                            )
                                    }),
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
                    Drift { app, focus, last_tick: None }
                })
            },
        )
        .expect("failed to open Drift window");

        cx.activate(true);
    });
}
