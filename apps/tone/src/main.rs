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
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
    WindowBounds, WindowOptions,
};
use prism_ui::colors;
use welcome::WelcomeView;

/// The GPUI root view. Owns the shared [`App`]; panels read it and route
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Tone {
    app: App,
    focus: FocusHandle,
    last_tick: Option<std::time::Instant>,
    editing_ai_prompt: bool,
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
        // ── Transport tick ──────────────────────────────────────────
        if self.app.playing {
            let now = std::time::Instant::now();
            if let Some(last) = self.last_tick {
                let elapsed = now.duration_since(last).as_secs_f32();
                let beats_per_sec = self.app.project.bpm / 60.0;
                let new_beat = self.app.playhead_beat + elapsed * beats_per_sec;
                self.app.apply(crate::app_state::Action::SetPlayheadBeat(new_beat));
            }
            self.last_tick = Some(now);
            cx.notify();
        } else {
            self.last_tick = None;
        }

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Transport toolbar
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| {
                let ks = &ev.keystroke;
                let m = &ks.modifiers;
                // AI prompt capture mode
                if this.editing_ai_prompt {
                    match ks.key.as_str() {
                        "escape" | "return" => { this.editing_ai_prompt = false; cx.notify(); }
                        "backspace" => {
                            let mut p = this.app.ai_prompt.clone(); p.pop();
                            this.app.apply(crate::app_state::Action::SetAiPrompt(p)); cx.notify();
                        }
                        " " if !m.platform && !m.control => {
                            let mut p = this.app.ai_prompt.clone(); p.push(' ');
                            this.app.apply(crate::app_state::Action::SetAiPrompt(p)); cx.notify();
                        }
                        key if key.len() == 1 && !m.platform && !m.control => {
                            let ch = if m.shift { key.to_uppercase() } else { key.to_string() };
                            let mut p = this.app.ai_prompt.clone(); p.push_str(&ch);
                            this.app.apply(crate::app_state::Action::SetAiPrompt(p)); cx.notify();
                        }
                        _ => {}
                    }
                    return;
                }
                if m.platform && !m.alt && !m.control {
                    match ks.key.as_str() {
                        "z" if m.shift => { this.app.apply(crate::app_state::Action::Redo); cx.notify(); }
                        "z" => { this.app.apply(crate::app_state::Action::Undo); cx.notify(); }
                        _ => {}
                    }
                }
                if !m.platform && !m.control && !m.alt && !m.shift {
                    match ks.key.as_str() {
                        " " => {
                            if this.app.playing { this.app.apply(crate::app_state::Action::Pause); }
                            else { this.app.apply(crate::app_state::Action::Play); }
                            cx.notify();
                        }
                        _ => {}
                    }
                }
            }))
            .child(panels::render_toolbar(&self.app, cx))
            // Main workspace row: left (arrangement + piano roll) | right (AI panel)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    // Left: arrangement fills space, piano roll splits below when clip active
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            // Arrangement — primary flex-1
                            .child(
                                div()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .child(panels::render_timeline(&self.app, cx)),
                            )
                            // Piano roll — conditional 260px bottom split
                            .when(has_active_clip, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .h(px(260.0))
                                        .flex()
                                        .flex_row()
                                        .border_t_1()
                                        .border_color(colors::surface_border())
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
                                        .child(panels::render_piano_roll(&self.app, cx)),
                                )
                            }),
                    )
                    // Right: AI panel
                    .child(panels::render_ai_panel(&self.app, self.editing_ai_prompt, cx)),
            )
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
                        Tone { app: App::new(), focus, last_tick: None, editing_ai_prompt: false }
                    })
                },
            )
            .expect("failed to open main window");

            cx.activate(true);
        });
}
