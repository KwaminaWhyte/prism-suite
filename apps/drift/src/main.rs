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

use app_state::{Action, App, DriftTool, DriftOnnxModel, Fill, Stroke, StrokeCap, StrokeJoin,
    VectorPath, Keyframe, EasingKind, LayerTransform};
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
    pub editing_prompt: bool,
    model_downloads: Vec<model_manager::ModelDownloadHandle>,
    /// Drives the real-time animation tick; present only while playing.
    /// Dropping it cancels the background timer.
    playback_task: Option<gpui::Task<()>>,
}

impl Focusable for Drift {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

// ── Keyframe interpolation helpers ───────────────────────────────────────────

/// Linearly interpolate a single float property from keyframes at `frame`.
/// Falls back to `default_val` when no keyframes exist for the property.
fn kf_interpolate(keyframes: &[Keyframe], layer_id: usize, prop: &str, frame: usize, default_val: f32) -> f32 {
    let mut layer_kfs: Vec<&Keyframe> = keyframes
        .iter()
        .filter(|k| k.layer_id == layer_id && k.property == prop)
        .collect();
    if layer_kfs.is_empty() {
        return default_val;
    }
    layer_kfs.sort_by_key(|k| k.frame);

    // Before first keyframe
    if frame <= layer_kfs[0].frame {
        return layer_kfs[0].value;
    }
    // After last keyframe
    let last = layer_kfs[layer_kfs.len() - 1];
    if frame >= last.frame {
        return last.value;
    }
    // Find surrounding pair
    let after_idx = layer_kfs.iter().position(|k| k.frame > frame).unwrap_or(layer_kfs.len() - 1);
    let before = layer_kfs[after_idx - 1];
    let after  = layer_kfs[after_idx];
    let span = (after.frame - before.frame) as f32;
    if span <= 0.0 { return before.value; }
    let t = (frame - before.frame) as f32 / span;
    // Apply easing
    let t = match before.easing {
        EasingKind::EaseIn    => t * t,
        EasingKind::EaseOut   => 1.0 - (1.0 - t) * (1.0 - t),
        EasingKind::EaseInOut => if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 },
        EasingKind::Hold      => 0.0, // jump at end
        _                     => t,   // Linear / Bezier approximated as linear
    };
    before.value + (after.value - before.value) * t
}

/// Compute the animated transform for `layer_id` at `frame`, merging keyframe
/// data over the base transform stored in `app.transforms`.
fn animated_transform(app: &App, layer_id: usize, frame: usize) -> LayerTransform {
    let base = app.transforms.get(&layer_id).cloned().unwrap_or_else(LayerTransform::new);
    let kfs = &app.keyframes;
    LayerTransform {
        x:         kf_interpolate(kfs, layer_id, "x",         frame, base.x),
        y:         kf_interpolate(kfs, layer_id, "y",         frame, base.y),
        scale_x:   kf_interpolate(kfs, layer_id, "scale_x",   frame, base.scale_x),
        scale_y:   kf_interpolate(kfs, layer_id, "scale_y",   frame, base.scale_y),
        rotation:  kf_interpolate(kfs, layer_id, "rotation",  frame, base.rotation),
        opacity:   kf_interpolate(kfs, layer_id, "opacity",   frame, base.opacity),
        anchor_x:  base.anchor_x,
        anchor_y:  base.anchor_y,
    }
}

/// Convert a `Fill` to a GPUI rgba colour, with a fallback for `Fill::None`.
fn fill_to_rgba(fill: &Fill, fallback: gpui::Rgba) -> gpui::Rgba {
    match fill {
        Fill::Solid { r, g, b, a } => {
            let ri = (r.clamp(0.0, 1.0) * 255.0) as u32;
            let gi = (g.clamp(0.0, 1.0) * 255.0) as u32;
            let bi = (b.clamp(0.0, 1.0) * 255.0) as u32;
            let ai = (a.clamp(0.0, 1.0) * 255.0) as u32;
            gpui::rgba((ri << 24) | (gi << 16) | (bi << 8) | ai)
        }
        Fill::None => fallback,
        _ => fallback,
    }
}

/// Build the GPUI div elements for all vector paths belonging to a visible layer.
fn render_vector_paths(
    paths: &[VectorPath],
    layer_id: usize,
    tx: f32,
    ty: f32,
    opacity: f32,
    layer_color: gpui::Rgba,
    scale: f32,
) -> Vec<gpui::Div> {
    paths
        .iter()
        .filter(|p| p.layer_id == layer_id)
        .map(|path| {
            let fill_color = fill_to_rgba(&path.fill, gpui::rgba(
                ((layer_color.r * 255.0) as u32) << 24
                | ((layer_color.g * 255.0) as u32) << 16
                | ((layer_color.b * 255.0) as u32) << 8
                | 0xcc,
            ));
            let stroke_color = gpui::rgba(
                ((path.stroke.r * 255.0) as u32) << 24
                | ((path.stroke.g * 255.0) as u32) << 16
                | ((path.stroke.b * 255.0) as u32) << 8
                | 0xff,
            );
            if let Some((rx, ry, rw, rh)) = path.as_rect() {
                div()
                    .absolute()
                    .left(px((rx + tx) * scale))
                    .top(px((ry + ty) * scale))
                    .w(px(rw.max(1.0) * scale))
                    .h(px(rh.max(1.0) * scale))
                    .bg(fill_color)
                    .border_1()
                    .border_color(stroke_color)
                    .opacity(opacity)
            } else if path.is_ellipse() {
                let (bx, by, bw, bh) = path.bbox();
                div()
                    .absolute()
                    .left(px((bx + tx) * scale))
                    .top(px((by + ty) * scale))
                    .w(px(bw.max(1.0) * scale))
                    .h(px(bh.max(1.0) * scale))
                    .rounded_full()
                    .bg(fill_color)
                    .border_1()
                    .border_color(stroke_color)
                    .opacity(opacity)
            } else {
                // Generic path: render bounding box outline
                let (bx, by, bw, bh) = path.bbox();
                div()
                    .absolute()
                    .left(px((bx + tx) * scale))
                    .top(px((by + ty) * scale))
                    .w(px(bw.max(1.0) * scale))
                    .h(px(bh.max(1.0) * scale))
                    .bg(fill_color)
                    .border_1()
                    .border_color(stroke_color)
                    .opacity(opacity)
            }
        })
        .collect()
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
                // Tool shortcuts (advertised in hint bar)
                "v" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Select));
                    cx.notify();
                }
                "m" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Move));
                    cx.notify();
                }
                "p" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Pen));
                    cx.notify();
                }
                "r" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Rect));
                    cx.notify();
                }
                "e" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Ellipse));
                    cx.notify();
                }
                // Arrow-key nudge when Move tool is active
                "up" | "down" | "left" | "right"
                    if self.app.active_tool == DriftTool::Move =>
                {
                    if let Some(id) = self.app.active_layer {
                        let t = self.app.transforms.get(&id)
                            .cloned()
                            .unwrap_or_else(crate::app_state::LayerTransform::new);
                        let step = if ks.modifiers.shift { 10.0_f32 } else { 1.0_f32 };
                        let (nx, ny) = match ks.key.as_str() {
                            "up"    => (t.x, t.y - step),
                            "down"  => (t.x, t.y + step),
                            "left"  => (t.x - step, t.y),
                            _       => (t.x + step, t.y),
                        };
                        self.app.apply(Action::SetLayerPosition { id, x: nx, y: ny });
                        cx.notify();
                    }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        // Spawn a background timer when playback starts; drop it when it stops.
        // cx.notify() inside render() does NOT reliably cause GPUI to re-render
        // continuously without external events, so we use an async timer task.
        if self.app.playing && self.playback_task.is_none() {
            let fps = self.app.document.fps;
            // Poll every ~10 ms; use wall-clock elapsed to gate actual frame advances.
            // macOS timer resolution is ~10 ms, so we cannot rely on a 41 ms timer
            // firing exactly once per frame — instead we measure real elapsed time.
            let poll_interval = std::time::Duration::from_millis(10);
            self.playback_task = Some(cx.spawn_in(window, async move |entity: gpui::WeakEntity<Drift>, cx: &mut gpui::AsyncWindowContext| {
                let mut last_advance = std::time::Instant::now();
                let frame_duration = std::time::Duration::from_secs_f64(1.0 / fps as f64);
                loop {
                    cx.background_executor().timer(poll_interval).await;
                    let elapsed = last_advance.elapsed();
                    if elapsed < frame_duration {
                        continue; // not yet time for next frame
                    }
                    // How many frames have accumulated (handles lag/catch-up)?
                    let frames = (elapsed.as_secs_f64() / frame_duration.as_secs_f64()) as usize;
                    let frames = frames.min(4); // cap catch-up to avoid jumps
                    last_advance += frame_duration * frames as u32;
                    let keep_going = entity.update(cx, |drift: &mut Drift, cx: &mut gpui::Context<Drift>| {
                        if drift.app.playing {
                            let max = drift.app.document.duration_frames;
                            let new = drift.app.current_frame + frames;
                            drift.app.current_frame = if new >= max { new % max } else { new };
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    }).unwrap_or(false);
                    if !keep_going { break; }
                }
            }));
        } else if !self.app.playing {
            self.playback_task = None;
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

        // ── Collect visible layers (keyframe-interpolated transforms) ─────────
        let current_frame = self.app.current_frame;
        let visible_layers: Vec<_> = self
            .app
            .layers
            .iter()
            .filter(|l| l.visible)
            .enumerate()
            .map(|(idx, layer)| {
                let layer_id = layer.id;
                let is_active = self.app.active_layer == Some(layer_id);
                let t = animated_transform(&self.app, layer_id, current_frame);
                let layer_color = match idx % 6 {
                    0 => gpui::rgba(0x6366f1ff),
                    1 => gpui::rgba(0x22d3eeff),
                    2 => gpui::rgba(0xf59e0bff),
                    3 => gpui::rgba(0x10b981ff),
                    4 => gpui::rgba(0xf43f5eff),
                    _ => gpui::rgba(0xa78bfaff),
                };
                let name = layer.name.clone();
                let has_shapes = self.app.vector_paths.iter().any(|p| p.layer_id == layer_id);
                (layer_id, is_active, t.x, t.y, t.opacity, layer_color, name, has_shapes)
            })
            .collect();

        // ── Build shape divs for all visible layers ───────────────────────────
        let stage_scale = 0.5_f32;
        let mut shape_divs: Vec<gpui::Div> = Vec::new();
        for (layer_id, _is_active, tx, ty, opacity, layer_color, _name, _has_shapes) in &visible_layers {
            let mut layer_shapes = render_vector_paths(
                &self.app.vector_paths,
                *layer_id,
                *tx,
                *ty,
                *opacity,
                *layer_color,
                stage_scale,
            );
            shape_divs.append(&mut layer_shapes);
        }

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
                                            // ── Checkerboard background ───────
                                            .children(checker_cells)
                                            // ── Placeholder boxes for layers with no shapes ──
                                            .children(visible_layers.iter().filter(|(.., has_shapes)| !has_shapes).map(
                                                |(layer_id, is_active, tx, ty, opacity, layer_color, name, _has_shapes)| {
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
                                            // ── Actual vector shapes ──────────
                                            .children(shape_divs)
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
                                            .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _win, cx| {
                                                let tool = this.app.active_tool;
                                                let layer_id = this.app.active_layer.unwrap_or(0);
                                                // ev.position() is in stage-div pixels (0.5× scale).
                                                // Multiply by 2 to get full-resolution canvas coords.
                                                let pos = ev.position();
                                                let canvas_x = f32::from(pos.x) * 2.0;
                                                let canvas_y = f32::from(pos.y) * 2.0;
                                                // Pick a fill color based on how many paths already exist
                                                let n = this.app.vector_paths.len();
                                                let (r, g, b) = match n % 6 {
                                                    0 => (0.388, 0.400, 0.945), // indigo
                                                    1 => (0.133, 0.827, 0.933), // cyan
                                                    2 => (0.961, 0.620, 0.043), // amber
                                                    3 => (0.063, 0.725, 0.506), // emerald
                                                    4 => (0.957, 0.247, 0.369), // rose
                                                    _ => (0.655, 0.545, 0.980), // violet
                                                };
                                                let white_stroke = Stroke {
                                                    width: 2.0, r: 1.0, g: 1.0, b: 1.0, a: 0.5,
                                                    cap: StrokeCap::Butt, join: StrokeJoin::Miter,
                                                };
                                                match tool {
                                                    DriftTool::Rect => {
                                                        this.app.apply(Action::AddRectangle {
                                                            layer_id,
                                                            x: canvas_x - 60.0,
                                                            y: canvas_y - 40.0,
                                                            width: 120.0,
                                                            height: 80.0,
                                                            fill: Fill::Solid { r, g, b, a: 0.85 },
                                                            stroke: white_stroke,
                                                        });
                                                        cx.notify();
                                                    }
                                                    DriftTool::Ellipse => {
                                                        this.app.apply(Action::AddEllipse {
                                                            layer_id,
                                                            cx: canvas_x,
                                                            cy: canvas_y,
                                                            rx: 60.0,
                                                            ry: 40.0,
                                                            fill: Fill::Solid { r, g, b, a: 0.85 },
                                                            stroke: white_stroke,
                                                        });
                                                        cx.notify();
                                                    }
                                                    DriftTool::Move => {
                                                        // Click-to-position: move the active layer
                                                        // so its origin lands at the clicked point.
                                                        this.app.apply(Action::SetLayerPosition {
                                                            id: layer_id,
                                                            x: canvas_x,
                                                            y: canvas_y,
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

        // Open the main editor window first (full display or sensible default).
        // Welcome window opens AFTER so it appears on top of the editor.
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
                    let mut app = App::new();
                    let focus = cx.focus_handle();
                    window.focus(&focus);
                    for (model_id, drift_model) in [
                        (model_manager::DriftModelId::AnimateDiff,   DriftOnnxModel::AnimateDiff),
                        (model_manager::DriftModelId::FilmRife,      DriftOnnxModel::FilmRife),
                        (model_manager::DriftModelId::Wav2Vec2,      DriftOnnxModel::Wav2Vec2),
                        (model_manager::DriftModelId::StyleTransfer, DriftOnnxModel::StyleTransfer),
                    ] {
                        if model_id.is_downloaded() {
                            app.apply(Action::CompleteDriftModelDownload {
                                model: drift_model,
                                local_path: model_id.local_path().to_string_lossy().to_string(),
                            });
                        }
                    }
                    Drift { app, focus, editing_prompt: false, model_downloads: vec![], playback_task: None }
                })
            },
        )
        .expect("failed to open Drift window");

        // Open the welcome window second (900 × 560 px, centered) so it
        // appears on top of the full-display editor window.
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

        cx.activate(true);
    });
}
