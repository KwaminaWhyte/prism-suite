//! Pulse — GPUI host (coexists with the eframe/egui `pulse` binary).
//!
//! Architecture, honest from day one: GPUI paints the chrome (toolbar, tools
//! strip, right dock, bottom timeline), while the preview pixels come from
//! `CanvasHost`, which drives the REAL Pulse CPU software compositor
//! (`crate::render::render_preview_frame`) and bridges the result in as a
//! `RenderImage`. Unlike the sibling Pigment host there is NO wgpu — Pulse
//! composites on the CPU.
//!
//! The integration backbone:
//! - [`app_state::App`] owns everything panels read/mutate (project/comp model,
//!   playhead time, tool, host) and exposes the single mutation choke point
//!   `App::apply`.
//! - Panels live in [`panels`] as `render(app: &App, cx: &mut Context<Pulse>)`
//!   functions and emit [`app_state::Action`]s via `cx.listener` ->
//!   `root.app.apply(...)`. See `panels/mod.rs` for the verbatim convention that
//!   parallel agents follow.
//! - This root view (`Pulse`) holds the `App` and lays out the chrome around the
//!   bridged preview.

// Logic modules (formerly pulse-app)
mod comp;
mod gizmo;
mod render;

mod app_state;
mod canvas_host;
mod effect_params;
mod export;
mod gpui_effects;
mod panels;

use std::sync::Arc;

use app_state::{Action, App};
use gpui::{
    div, px, rgb, size, AppContext, Bounds, Context, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, RenderImage,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowKind, WindowOptions,
};
use gpui::prelude::FluentBuilder;

use panels::preview_panel;
use panels::{DOCK_W, STRIP_W, TIMELINE_H, TOOLBAR_H};

/// The GPUI root view. Owns the shared [`App`]; panels read it and route their
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Pulse {
    app: App,
    /// Focus handle for the root, so it receives key events (Cmd+Z / Cmd+Shift+Z
    /// for undo / redo). Focused once when the window opens.
    focus: FocusHandle,
    /// The preview image painted on the **previous** frame. GPUI's sprite atlas
    /// only releases an image's GPU tile via an explicit `window.drop_image`, so
    /// during playback — where a fresh `RenderImage` (new id) is produced every
    /// frame — the atlas would grow without bound (the 500 MB → multi-GB leak).
    /// We hold the last painted image and drop its atlas tile once a new-id frame
    /// replaces it, bounding the atlas to ~one preview frame.
    last_image: Option<Arc<RenderImage>>,
}

impl Pulse {
    /// The bridged preview image (re-renders only when the host is dirty).
    /// When RAM preview is playing, returns the cached BGRA8 frame instead.
    fn preview_image(&mut self) -> Arc<RenderImage> {
        if self.app.ram_preview_playing && self.app.ram_preview_complete {
            let fi = self.app.ram_preview_frame;
            if let Some(bgra) = self.app.ram_preview.get(&fi).cloned() {
                let w = self.app.host.preview_w;
                let h = self.app.host.preview_h;
                let ci = self.app.active_comp_index();
                let fps = self.app.project.comps[ci].fps.max(1.0);
                let wa = self.app.active_work_area();
                let end_frame = (wa.end * fps).ceil() as u32;
                let next = fi + 1;
                self.app.ram_preview_frame = if next >= end_frame {
                    (wa.start * fps).round() as u32
                } else {
                    next
                };
                if let Some(buf) = image::RgbaImage::from_raw(w, h, bgra) {
                    return Arc::new(RenderImage::new([image::Frame::new(buf)]));
                }
            }
        }
        let time = self.app.time;
        let gpui_effects = &self.app.gpui_effects;
        let selected_layer = self.app.selected_layer;
        let playing = self.app.playing;
        self.app.host.image(&self.app.project, time, gpui_effects, selected_layer, playing, self.app.roi)
    }
}

impl Render for Pulse {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drive the play loop: while playing, advance the playhead by the real
        // elapsed time and ask GPUI to redraw on the next animation frame. The
        // `request_animation_frame` re-notifies this view next frame, so the loop
        // self-sustains (and naturally stops once `tick` returns without playing,
        // since we only re-arm while `app.playing`). This is the CPU-host analog
        // of egui's `ctx.request_repaint()` inside the playback block.
        if self.app.playing {
            self.app.tick();
            window.request_animation_frame();
        }

        // While an export worker is running, keep re-rendering so the toolbar's
        // progress readout tracks the background thread frame-by-frame.
        if self
            .app
            .export
            .as_ref()
            .is_some_and(|p| !p.is_finished())
        {
            window.request_animation_frame();
        }

        // Audio preview stub: animate the waveform placeholder while playing.
        if self.app.audio_preview_enabled && self.app.playing {
            window.request_animation_frame();
        }

        // RAM preview playback: keep animating while playing.
        if self.app.ram_preview_playing && self.app.ram_preview_complete {
            window.request_animation_frame();
        }

        // Render the preview frame first so the host's preview dimensions are
        // fresh for the layout below.
        let preview = self.preview_image();
        // Release the previous frame's GPU atlas tile. gpui's sprite atlas never
        // evicts on its own — without this, every playback frame leaks one tile
        // and RAM/VRAM climbs unbounded. Only drop when the id actually changed
        // (a cached/paused frame returns the same image — keep its single tile).
        if let Some(prev) = self.last_image.take() {
            if prev.id != preview.id {
                let _ = window.drop_image(prev);
            }
        }
        self.last_image = Some(preview.clone());
        if self.app.live_output_enabled {
            self.app.host.write_live_output();
        }
        // Display size for the preview: fit the comp's aspect into the center
        // work area (window minus the left strip, right dock, top toolbar, bottom
        // timeline), so the preview fills the playground instead of sitting tiny
        // at its native capped pixel size. The gizmo / pointer mapping derives
        // from the painted image bounds, so scaling the display stays correct.
        let comp_w = (self.app.host.preview_w as f32).max(1.0);
        let comp_h = (self.app.host.preview_h as f32).max(1.0);
        let vp = window.viewport_size();
        let avail_w = (f32::from(vp.width) - STRIP_W - DOCK_W).max(64.0);
        let avail_h = (f32::from(vp.height) - TOOLBAR_H - TIMELINE_H).max(64.0);
        let fit = (avail_w * 0.94 / comp_w)
            .min(avail_h * 0.94 / comp_h)
            .max(0.05);
        let (w, h) = (comp_w * fit, comp_h * fit);

        // Build panel elements (read-only &App + cx for Action listeners).
        let app = &self.app;
        let toolbar = panels::toolbar::render(app, cx);
        let tools = panels::tools::render(app, cx);
        let layers = panels::layers::render(app, cx);
        let properties = panels::properties::render(app, cx);
        let effects = panels::effects::render(app, cx);
        let expressions = panels::expressions::render(app, cx);
        let expr_controls = panels::expr_controls::render(app, cx);
        let render_queue = panels::render_queue::render(app, cx);
        let comp_settings = panels::comp_settings::render(app, cx);
        let timeline = panels::timeline::render(app, cx);

        div()
            .track_focus(&self.focus)
            .key_context("Pulse")
            // Undo / redo keyboard shortcuts. We match the raw keystroke here
            // (gpui 0.2.2's secondary modifier is Cmd on macOS) rather than the
            // global keymap so the host has zero action-registration boilerplate.
            .on_key_down(cx.listener(|root, ev: &KeyDownEvent, _win, cx| {
                let ks = &ev.keystroke;
                if ks.key == "z" && ks.modifiers.secondary() {
                    if ks.modifiers.shift {
                        root.app.apply(Action::Redo);
                    } else {
                        root.app.apply(Action::Undo);
                    }
                    cx.notify();
                }
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(prism_ui::colors::surface_bg())
            .text_color(prism_ui::colors::text_primary())
            .font_family(".SystemUIFont")
            // Top toolbar (full width).
            .child(
                div()
                    .w_full()
                    .h(px(TOOLBAR_H))
                    .bg(prism_ui::colors::surface_bg())
                    .border_b_1()
                    .border_color(prism_ui::colors::surface_border())
                    .child(toolbar),
            )
            // Workspace: tools strip | preview | dock.
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    // Left tools strip.
                    .child(
                        div()
                            .w(px(STRIP_W))
                            .h_full()
                            .bg(prism_ui::colors::surface_bg())
                            .border_r_1()
                            .border_color(prism_ui::colors::surface_border())
                            .child(tools),
                    )
                    // Center preview (comp frame centered on a dark field) with
                    // the on-canvas transform gizmo + click-to-select overlay.
                    .child({
                        let audio_enabled = self.app.audio_preview_enabled;
                        let playing = self.app.playing;
                        let t = self.app.time;
                        div()
                            .flex_1()
                            // Let the center column shrink below the fixed 640px
                            // preview width and clip it, instead of forcing the
                            // workspace row wider than the window and shoving the
                            // fixed-width right dock off the right edge.
                            .min_w(px(0.0))
                            .h_full()
                            .overflow_hidden()
                            .bg(prism_ui::colors::surface_bg())
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .child(preview_panel::render(&self.app, preview, w, h, cx))
                            .when(audio_enabled, |d| {
                                d.child(audio_waveform_placeholder(playing, t))
                            })
                    })
                    // Right dock: layers (more panels stack here per wave).
                    .child(
                        div()
                            .id("right-dock")
                            .w(px(DOCK_W))
                            .flex_shrink_0()
                            .h_full()
                            .bg(prism_ui::colors::surface_bg())
                            .border_l_1()
                            .border_color(prism_ui::colors::surface_border())
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(comp_settings)
                            .child(properties)
                            .child(effects)
                            .child(expressions)
                            .child(expr_controls)
                            .child(render_queue)
                            .child(layers),
                    ),
            )
            // Bottom timeline placeholder strip (full width).
            .child(
                div()
                    .w_full()
                    .h(px(TIMELINE_H))
                    .bg(prism_ui::colors::surface_bg())
                    .border_t_1()
                    .border_color(prism_ui::colors::surface_border())
                    .child(timeline),
            )
    }
}

/// A pulsing waveform placeholder shown when audio preview is enabled.
/// The bars animate based on the current playhead time.
fn audio_waveform_placeholder(playing: bool, t: f32) -> impl IntoElement {
    let bars: Vec<gpui::AnyElement> = (0..12)
        .map(|i| {
            let phase = (t * 4.0 + i as f32 * 0.5).sin().abs();
            let h = if playing { 4.0 + phase * 16.0 } else { 4.0 };
            div()
                .w(px(3.0))
                .h(px(h))
                .mx(px(1.0))
                .rounded_sm()
                .bg(rgb(0x37c8c0))
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .items_end()
        .justify_center()
        .w_full()
        .h(px(28.0))
        .bg(prism_ui::colors::surface_raised())
        .px_2()
        .py_1()
        .child(
            div()
                .flex()
                .items_end()
                .gap_px()
                .children(bars),
        )
        .child(
            div()
                .ml_2()
                .text_size(px(9.0))
                .text_color(prism_ui::colors::text_secondary())
                .child("♪ Audio Preview (stub)"),
        )
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new().with_assets(prism_ui::PrismAssets).run(|cx: &mut gpui::App| {
        prism_ui::init(cx);
        let bounds = cx.primary_display()
            .map(|d| d.bounds())
            .unwrap_or_else(|| Bounds::centered(None, size(px(1600.0), px(1000.0)), cx));
        let main_handle = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    // Focus the root so it receives the undo/redo key events.
                    window.focus(&focus);
                    Pulse {
                        app: App::new(),
                        focus,
                        last_image: None,
                    }
                })
            },
        )
        .expect("failed to open window");

        let weak_pulse = main_handle
            .entity(cx)
            .expect("failed to get main entity")
            .downgrade();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(
                    Bounds::centered(None, size(px(900.0), px(560.0)), cx),
                )),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |window, cx| {
                let focus = cx.focus_handle();
                window.focus(&focus);
                cx.new(|_cx| panels::welcome::WelcomeView::new(focus, weak_pulse))
            },
        )
        .expect("failed to open Pulse welcome window");

        cx.activate(true);
    });
}
