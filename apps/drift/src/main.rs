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
mod model_manager;
mod panels;
mod welcome;

use prism_ui::PrismAssets;

use app_state::{Action, App, DriftTool, DriftOnnxModel, Fill, Stroke, StrokeCap, StrokeJoin};
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
    pub editing_prompt: bool,
    model_downloads: Vec<model_manager::ModelDownloadHandle>,
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

        // If typing into the AI prompt textarea, intercept all keys
        if self.editing_prompt {
            match ks.key.as_str() {
                "escape" | "return" => {
                    self.editing_prompt = false;
                    cx.notify();
                }
                "backspace" => {
                    let mut p = self.app.ai_motion_prompt.clone();
                    p.pop();
                    self.app.apply(Action::SetAiMotionPrompt(p));
                    cx.notify();
                }
                key if key.len() == 1 && !m.platform && !m.control => {
                    let ch = if m.shift {
                        key.to_uppercase()
                    } else {
                        key.to_string()
                    };
                    let mut p = self.app.ai_motion_prompt.clone();
                    p.push_str(&ch);
                    self.app.apply(Action::SetAiMotionPrompt(p));
                    cx.notify();
                }
                " " if !m.platform && !m.control => {
                    let mut p = self.app.ai_motion_prompt.clone();
                    p.push(' ');
                    self.app.apply(Action::SetAiMotionPrompt(p));
                    cx.notify();
                }
                _ => {}
            }
            return;
        }

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

fn drift_model_from_id(m: &model_manager::DriftModelId) -> DriftOnnxModel {
    match m {
        model_manager::DriftModelId::AnimateDiff   => DriftOnnxModel::AnimateDiff,
        model_manager::DriftModelId::FilmRife      => DriftOnnxModel::FilmRife,
        model_manager::DriftModelId::Wav2Vec2      => DriftOnnxModel::Wav2Vec2,
        model_manager::DriftModelId::StyleTransfer => DriftOnnxModel::StyleTransfer,
    }
}

impl Render for Drift {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ── Model download polling ────────────────────────────────────────────
        {
            use model_manager::DownloadEvent;
            let mut done_indices = vec![];
            for (i, handle) in self.model_downloads.iter().enumerate() {
                while let Ok(ev) = handle.rx.try_recv() {
                    let drift_model = drift_model_from_id(&handle.model_id);
                    match ev {
                        DownloadEvent::Progress { bytes_done, bytes_total } => {
                            let progress = if bytes_total > 0 {
                                bytes_done as f32 / bytes_total as f32
                            } else {
                                0.0
                            };
                            self.app.apply(Action::UpdateDriftModelDownload { model: drift_model, progress });
                            cx.notify();
                        }
                        DownloadEvent::Done => {
                            let path = handle.model_id.local_path().to_string_lossy().to_string();
                            self.app.apply(Action::CompleteDriftModelDownload { model: drift_model, local_path: path });
                            done_indices.push(i);
                            cx.notify();
                        }
                        DownloadEvent::Error(e) => {
                            log::error!("drift model download error: {e}");
                            self.app.apply(Action::ErrorDriftModelDownload { model: drift_model, message: e });
                            done_indices.push(i);
                            cx.notify();
                        }
                    }
                }
            }
            for i in done_indices.into_iter().rev() {
                self.model_downloads.swap_remove(i);
            }
        }

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

        // ── Checkerboard tile parameters ─────────────────────────────────────
        // 32px tiles → 30×17 max cells at 0.5× scale — covers full canvas
        let tile = 32.0_f32;
        let checker_cols = (stage_w / tile).ceil() as usize;
        let checker_rows = (stage_h / tile).ceil() as usize;

        // Build checkerboard cells (capped at 20×15 = 300 but we min them too).
        let mut checker_cells: Vec<gpui::Div> = Vec::with_capacity(checker_cols * checker_rows);
        for row in 0..checker_rows {
            for col in 0..checker_cols {
                let is_light = (row + col) % 2 == 0;
                checker_cells.push(
                    div()
                        .absolute()
                        .left(px(col as f32 * tile))
                        .top(px(row as f32 * tile))
                        .w(px(tile))
                        .h(px(tile))
                        .bg(if is_light {
                            gpui::rgb(0x2a2a3e)
                        } else {
                            gpui::rgb(0x222233)
                        }),
                );
            }
        }

        // ── Ruler tick parameters ─────────────────────────────────────────────
        // Horizontal ruler ticks at every 50px (canvas-space) up to stage_w.
        let h_tick_count = ((stage_w / 50.0).floor() as usize) + 1;
        // Vertical ruler ticks at every 50px (canvas-space) up to stage_h.
        let v_tick_count = ((stage_h / 50.0).floor() as usize) + 1;

        // ── Collect visible layers ────────────────────────────────────────────
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

        // ── Rig bone dots for the active layer ───────────────────────────────
        let active_layer_id = self.app.active_layer;
        let bone_dots: Vec<gpui::Div> = active_layer_id
            .and_then(|lid| self.app.layer_rigs.iter().find(|r| r.layer_id == lid))
            .map(|rig| {
                rig.bones
                    .iter()
                    .map(|bone| {
                        let bx = bone.x * 0.5;
                        let by = bone.y * 0.5;
                        div()
                            .absolute()
                            .left(px(bx - 5.0))
                            .top(px(by - 5.0))
                            .w(px(10.0))
                            .h(px(10.0))
                            .rounded_full()
                            .bg(gpui::rgb(0xffd700))
                            .border_1()
                            .border_color(gpui::rgb(0xffffff))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // ── Shape tool: hint label text ───────────────────────────────────────
        let active_tool = self.app.active_tool;
        let shape_hint: Option<&str> = match active_tool {
            DriftTool::Rect => Some("Click to place a Rectangle"),
            DriftTool::Ellipse => Some("Click to place an Ellipse"),
            _ => None,
        };

        // ── Ruler visibility ──────────────────────────────────────────────────
        let show_rulers = self.app.rulers.enabled;

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
            // Main workspace: left layers+inspector | center canvas | right AI panel
            // Main workspace: left layers | center canvas | right AI + Inspector
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    // Left: Layers panel stacked above Inspector
                    .child(
                        div()
                            .w(px(240.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(
                                // Layers panel takes available space
                                div()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .child(panels::render_layers(&self.app, cx)),
                            )
                            .child(panels::render_inspector(&self.app, cx)),
                    )
                    // Center: Canvas area with optional rulers
                    .child(
                        div()
                            .id("canvas")
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .bg(colors::surface_bg())
                            .flex()
                            .flex_col()
                            // Stage viewport (fills remaining space)
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .min_h(px(0.0))
                                    // ── Horizontal ruler (above stage) ───────────────
                                    .when(show_rulers, |el| {
                                        el.child(
                                            div()
                                                .w(px(stage_w + 20.0))
                                                .h(px(20.0))
                                                .flex()
                                                .flex_row()
                                                .bg(gpui::rgb(0x1e1e2e))
                                                .border_b_1()
                                                .border_color(colors::surface_border())
                                                // Corner spacer
                                                .child(
                                                    div()
                                                        .w(px(20.0))
                                                        .h(px(20.0))
                                                        .bg(gpui::rgb(0x1e1e2e)),
                                                )
                                                // Tick marks
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .h_full()
                                                        .relative()
                                                        .children((0..h_tick_count).map(|i| {
                                                            let pos = i as f32 * 50.0;
                                                            let label = format!("{}", i * 50);
                                                            div()
                                                                .absolute()
                                                                .left(px(pos))
                                                                .top(px(0.0))
                                                                .w(px(1.0))
                                                                .h(px(6.0))
                                                                .bg(colors::text_disabled())
                                                                // Label
                                                                .child(
                                                                    div()
                                                                        .absolute()
                                                                        .left(px(2.0))
                                                                        .top(px(6.0))
                                                                        .text_size(px(8.0))
                                                                        .text_color(colors::text_disabled())
                                                                        .child(label),
                                                                )
                                                        })),
                                                ),
                                        )
                                    })
                                    // ── Stage row (vertical ruler + stage) ───────────
                                    .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    // Vertical ruler (left of stage)
                                    .when(show_rulers, |el| {
                                        el.child(
                                            div()
                                                .w(px(20.0))
                                                .h(px(stage_h))
                                                .bg(gpui::rgb(0x1e1e2e))
                                                .border_r_1()
                                                .border_color(colors::surface_border())
                                                .relative()
                                                .children((0..v_tick_count).map(|i| {
                                                    let pos = i as f32 * 50.0;
                                                    let label = format!("{}", i * 50);
                                                    div()
                                                        .absolute()
                                                        .left(px(0.0))
                                                        .top(px(pos))
                                                        .w(px(6.0))
                                                        .h(px(1.0))
                                                        .bg(colors::text_disabled())
                                                        .child(
                                                            div()
                                                                .absolute()
                                                                .left(px(7.0))
                                                                .top(px(1.0))
                                                                .text_size(px(7.0))
                                                                .text_color(colors::text_disabled())
                                                                .child(label),
                                                        )
                                                })),
                                        )
                                    })
                                    // Stage canvas
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
                                            // Grid overlay — horizontal lines every 50px
                                            .children((0..=(stage_h as u32 / 50)).map(|i| {
                                                div()
                                                    .id(("grid-h", i))
                                                    .absolute()
                                                    .left(px(0.0))
                                                    .right(px(0.0))
                                                    .top(px(i as f32 * 50.0))
                                                    .h(px(1.0))
                                                    .bg(gpui::rgba(0xffffff26))
                                            }))
                                            // Grid overlay — vertical lines every 50px
                                            .children((0..=(stage_w as u32 / 50)).map(|j| {
                                                div()
                                                    .id(("grid-v", j))
                                                    .absolute()
                                                    .top(px(0.0))
                                                    .bottom(px(0.0))
                                                    .left(px(j as f32 * 50.0))
                                                    .w(px(1.0))
                                                    .bg(gpui::rgba(0xffffff26))
                                            }))
                                            // Render visible layers as colored labeled boxes
                                            // ── Checkerboard background ───────
                                            .children(checker_cells)
                                            // ── Visible layer boxes ───────────
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
                                            // ── Rig bone overlay dots ─────────
                                            .children(bone_dots)
                                            // ── Shape tool hint text ──────────
                                            .when(shape_hint.is_some(), |el| {
                                                let hint = shape_hint.unwrap_or("");
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .left(px(stage_w * 0.5 - 80.0))
                                                        .top(px(stage_h * 0.5 - 10.0))
                                                        .text_size(px(font_size::SM))
                                                        .text_color(gpui::rgba(0xffffff99))
                                                        .child(hint.to_string()),
                                                )
                                            })
                                            // ── Shape tool click-to-place ─────
                                            .on_click(cx.listener(move |this, _ev, _win, cx| {
                                                let tool = this.app.active_tool;
                                                let layer_id = this.app.active_layer.unwrap_or(0);
                                                match tool {
                                                    DriftTool::Rect => {
                                                        this.app.apply(Action::AddRectangle {
                                                            layer_id,
                                                            x: 100.0,
                                                            y: 80.0,
                                                            width: 100.0,
                                                            height: 60.0,
                                                            fill: Fill::Solid { r: 0.388, g: 0.400, b: 0.945, a: 1.0 },
                                                            stroke: Stroke {
                                                                width: 2.0,
                                                                r: 1.0,
                                                                g: 1.0,
                                                                b: 1.0,
                                                                a: 1.0,
                                                                cap: StrokeCap::Butt,
                                                                join: StrokeJoin::Miter,
                                                            },
                                                        });
                                                        cx.notify();
                                                    }
                                                    DriftTool::Ellipse => {
                                                        this.app.apply(Action::AddEllipse {
                                                            layer_id,
                                                            cx: 150.0,
                                                            cy: 110.0,
                                                            rx: 50.0,
                                                            ry: 30.0,
                                                            fill: Fill::Solid { r: 0.388, g: 0.400, b: 0.945, a: 1.0 },
                                                            stroke: Stroke {
                                                                width: 2.0,
                                                                r: 1.0,
                                                                g: 1.0,
                                                                b: 1.0,
                                                                a: 1.0,
                                                                cap: StrokeCap::Butt,
                                                                join: StrokeJoin::Miter,
                                                            },
                                                        });
                                                        cx.notify();
                                                    }
                                                    _ => {}
                                                }
                                            }))
                                            // Empty state hint
                                            .when(!has_layers, |el: gpui::Stateful<gpui::Div>| {
                                                el.flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        div()
                                                            .absolute()
                                                            .left(px(stage_w * 0.5 - 90.0))
                                                            .top(px(stage_h * 0.5 - 8.0))
                                                            .text_size(px(font_size::SM))
                                                            .text_color(colors::text_secondary())
                                                            .child(
                                                                "Click + in the Layers panel to add a layer",
                                                            ),
                                                    )
                                            }),
                                    ),
                            )
                            // Stage status bar
                            .child(
                                div()
                                    .w_full()
                                    .h(px(22.0))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .gap_4()
                                    .bg(colors::surface_raised())
                                    .border_t_1()
                                    .border_color(colors::surface_border())
                                    .child(
                                        div()
                                            .text_size(px(font_size::XS))
                                            .text_color(colors::text_secondary())
                                            .child(format!(
                                                "Stage: {}x{}px  FPS:{}  Frame:{}",
                                                self.app.document.width,
                                                self.app.document.height,
                                                self.app.document.fps as u32,
                                                self.app.current_frame,
                                            )),
                                    ),
                            ),
                    )
                    )
                    // Right: AI panel only — inspector lives in the left column
                    .child(panels::render_ai_panel(&self.app, self.editing_prompt, cx)),
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
                    Drift { app, focus, last_tick: None, editing_prompt: false, model_downloads: vec![] }
                })
            },
        )
        .expect("failed to open Drift window");

        cx.activate(true);
    });
}
