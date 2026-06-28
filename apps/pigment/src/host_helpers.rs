//! Root-view helpers split out of `main.rs` to keep it under the ~1000-line
//! limit: keyboard handling, window↔doc mapping, inline rename fields, the dock
//! detach wrapper, and the canvas overlay painters (marching ants + guides).

#![allow(unused_imports)]

use std::sync::Arc;
use std::time::Instant;

use gpui::{
    div, px, rgb, size, AppContext, Bounds, Context, Entity, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Pixels, Point, RenderImage,
    StatefulInteractiveElement, Styled, Window,
};
use prism_core::LayerId;
use prism_ui::{colors, TextField};

use super::{Action, Pigment};

impl Pigment {
    /// Handle a key press on the focused canvas. Returns nothing; mutates `app`
    /// and requests a redraw when it consumes the event. Two roles:
    /// 1. Cmd+Z / Cmd+Shift+Z (or Cmd+Y) → undo / redo, always.
    /// 2. While a Text edit is in progress, printable keys / Backspace edit the
    ///    run, and Enter/Escape commit it.
    pub(crate) fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        // --- Cmd shortcuts (Cmd on macOS = `platform`) ---
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
                "y" => {
                    self.app.apply(Action::Redo);
                    cx.notify();
                    return;
                }
                // Cmd+Shift+A → Camera Raw Filter dialog
                "a" if m.shift => {
                    self.app.apply(Action::ToggleCameraRawDialog);
                    cx.notify();
                    return;
                }
                "a" => {
                    self.app.apply(Action::SelectAll);
                    cx.notify();
                    return;
                }
                "d" => {
                    self.app.apply(Action::ClearSelection);
                    cx.notify();
                    return;
                }
                "i" => {
                    self.app.apply(Action::InvertSelection);
                    cx.notify();
                    return;
                }
                // Cmd+Shift+F → toggle Filter Gallery (mirrors Photoshop)
                "f" if m.shift => {
                    self.app.apply(Action::ToggleFilterGallery);
                    cx.notify();
                    return;
                }
                // Cmd+P → Print dialog
                "p" => {
                    self.app.apply(Action::TogglePrintDialog);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        // --- Text-tool typing ---
        if self.app.text_editing() {
            match ks.key.as_str() {
                "enter" | "escape" => {
                    self.app.commit_text();
                    cx.notify();
                }
                "backspace" => {
                    if self.app.text_input(None, true) {
                        cx.notify();
                    }
                }
                "space" => {
                    if self.app.text_input(Some(" "), false) {
                        cx.notify();
                    }
                }
                _ => {
                    if !m.platform && !m.control {
                        if let Some(c) = ks.key_char.as_deref().filter(|c| !c.is_empty()) {
                            if self.app.text_input(Some(c), false) {
                                cx.notify();
                            }
                        }
                    }
                }
            }
            return;
        }
        // --- Tool shortcuts (no modifier, not text editing) ---
        if !m.platform && !m.control && !m.alt {
            match ks.key.as_str() {
                "[" => {
                    let s = self.app.brush.size - 5.0;
                    self.app.apply(Action::SetBrushSize(s));
                    cx.notify();
                }
                "]" => {
                    let s = self.app.brush.size + 5.0;
                    self.app.apply(Action::SetBrushSize(s));
                    cx.notify();
                }
                "{" => {
                    let h = self.app.brush.hardness - 0.1;
                    self.app.apply(Action::SetBrushHardness(h));
                    cx.notify();
                }
                "}" => {
                    let h = self.app.brush.hardness + 0.1;
                    self.app.apply(Action::SetBrushHardness(h));
                    cx.notify();
                }
                "x" => {
                    self.app.apply(Action::SwapColors);
                    cx.notify();
                }
                "d" => {
                    self.app.apply(Action::ResetColors);
                    cx.notify();
                }
                "1" => { self.app.apply(Action::SetBrushOpacity(0.1)); cx.notify(); }
                "2" => { self.app.apply(Action::SetBrushOpacity(0.2)); cx.notify(); }
                "3" => { self.app.apply(Action::SetBrushOpacity(0.3)); cx.notify(); }
                "4" => { self.app.apply(Action::SetBrushOpacity(0.4)); cx.notify(); }
                "5" => { self.app.apply(Action::SetBrushOpacity(0.5)); cx.notify(); }
                "6" => { self.app.apply(Action::SetBrushOpacity(0.6)); cx.notify(); }
                "7" => { self.app.apply(Action::SetBrushOpacity(0.7)); cx.notify(); }
                "8" => { self.app.apply(Action::SetBrushOpacity(0.8)); cx.notify(); }
                "9" => { self.app.apply(Action::SetBrushOpacity(0.9)); cx.notify(); }
                _ => {}
            }
        }
    }

    /// The bridged composite image (re-composites only when the host is dirty).
    pub(crate) fn doc_image(&mut self) -> Arc<RenderImage> {
        self.app.host.image()
    }

    /// Map a window-space position to doc px using the last painted canvas rect,
    /// clamped to the document. Returns `None` if the canvas hasn't painted yet
    /// or has zero size.
    pub(crate) fn window_to_doc(&self, pos: Point<Pixels>) -> Option<[f32; 2]> {
        let b = self.canvas_bounds.get()?;
        let (ox, oy) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (bw, bh) = (f32::from(b.size.width), f32::from(b.size.height));
        if bw <= 0.0 || bh <= 0.0 {
            return None;
        }
        let (dw, dh) = (self.app.host.doc_w as f32, self.app.host.doc_h as f32);
        // The img is drawn at the doc's native size (no fit/zoom yet), so the map
        // is a straight translate. We still scale by bw/dw to be robust if that
        // changes. Clamp into the doc so off-edge drags don't paint out of range.
        let x = ((f32::from(pos.x) - ox) / bw * dw).clamp(0.0, dw);
        let y = ((f32::from(pos.y) - oy) / bh * dh).clamp(0.0, dh);
        Some([x, y])
    }

    /// Begin renaming layer `id`: build a fresh `TextField` seeded with the
    /// current name, focus it, and store it so the layers panel renders it in
    /// place of the label. `on_submit` emits `RenameLayer` against the main view
    /// and clears the rename field (committing the edit).
    pub(crate) fn start_rename(&mut self, id: LayerId, name: String, window: &mut Window, cx: &mut Context<Self>) {
        // Weak handle so the submit closure can dispatch back to this view.
        let weak = cx.entity().downgrade();
        let field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("Layer name")
                .initial_value(name)
                .on_submit(move |text, _win, app| {
                    let text = text.to_string();
                    if let Some(entity) = weak.upgrade() {
                        entity.update(app, |root, cx| {
                            root.app.apply(Action::RenameLayer { id, name: text });
                            root.rename_field = None;
                            cx.notify();
                        });
                    }
                })
        });
        window.focus(&field.focus_handle(cx));
        self.rename_field = Some((id, field));
    }

    /// Programmatically set the PSD path field's text (e.g. after a Browse…
    /// native dialog returns a path).
    pub(crate) fn set_psd_path_field(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.psd_path_field.update(cx, |f, cx| {
            f.set_text(path, window, cx);
        });
    }
}

/// Wraps a dock panel with a small "⤢" detach button in the header.
/// Returns a plain div — floating/detached state is shown via a deferred
/// overlay added separately to the root view.
pub(crate) fn dockable_wrap(
    name: &'static str,
    content: impl IntoElement + 'static,
    _app: &super::App,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .px_2()
                .child(
                    div()
                        .id(name)
                        .text_size(px(10.0))
                        .text_color(colors::text_secondary())
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::DetachPanel(name));
                            cx.notify();
                        }))
                        .child("⤢")
                )
        )
        .child(content)
}

/// Paint the active selection's boundary as an animated dashed outline (marching
/// ants) over the canvas. `boundary` is the engine-traced edge segments in doc px
/// (`[x0,y0,x1,y1]`); `painted` is the canvas content rect in window space at the
/// doc's native size, so doc px map by a straight scale. The dash phase advances
/// with wall-clock time, and the caller schedules the next animation frame so the
/// ants keep crawling. Drawn as short alternating black/white quads (cheap, no
/// `Path` stroking) — a 1px-thick run of dashes along each unit edge.
pub(crate) fn paint_marching_ants(
    win: &mut Window,
    painted: Bounds<Pixels>,
    boundary: &[[f32; 4]],
    doc_w: f32,
    doc_h: f32,
) {
    if boundary.is_empty() || doc_w <= 0.0 || doc_h <= 0.0 {
        return;
    }
    let (ox, oy) = (f32::from(painted.origin.x), f32::from(painted.origin.y));
    let (bw, bh) = (f32::from(painted.size.width), f32::from(painted.size.height));
    let (sx, sy) = (bw / doc_w, bh / doc_h);
    // Animated dash phase: ~12 px/s crawl, period = dash + gap (6 px), anchored to
    // a process-start monotonic clock so it advances smoothly each frame.
    let t = ANT_START.with(|s| s.elapsed().as_secs_f32());
    let phase = (t * 12.0).rem_euclid(6.0);
    let dash = 3.0_f32; // px on
    let period = 6.0_f32; // px on+off

    for &[x0, y0, x1, y1] in boundary {
        // Edge endpoints in window px.
        let (wx0, wy0) = (ox + x0 * sx, oy + y0 * sy);
        let (wx1, wy1) = (ox + x1 * sx, oy + y1 * sy);
        let dx = wx1 - wx0;
        let dy = wy1 - wy0;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.01 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        // March alternating black/white dashes along [0, len), offset by the
        // animated phase. Drawing both colours ensures the ants are visible on
        // any background.
        let mut d = -phase;
        while d < len {
            // Black "on" dash.
            let a = d.max(0.0);
            let b = (d + dash).min(len);
            if b > a {
                let (px0, py0) = (wx0 + ux * a, wy0 + uy * a);
                let (px1, py1) = (wx0 + ux * b, wy0 + uy * b);
                let min_x = px0.min(px1);
                let min_y = py0.min(py1);
                let qw = (px1 - px0).abs().max(1.0);
                let qh = (py1 - py0).abs().max(1.0);
                win.paint_quad(gpui::fill(
                    Bounds { origin: Point { x: px(min_x), y: px(min_y) }, size: size(px(qw), px(qh)) },
                    rgb(0x000000),
                ));
            }
            // White "off" gap.
            let ga = (d + dash).max(0.0);
            let gb = (d + period).min(len);
            if gb > ga {
                let (px0, py0) = (wx0 + ux * ga, wy0 + uy * ga);
                let (px1, py1) = (wx0 + ux * gb, wy0 + uy * gb);
                let min_x = px0.min(px1);
                let min_y = py0.min(py1);
                let qw = (px1 - px0).abs().max(1.0);
                let qh = (py1 - py0).abs().max(1.0);
                win.paint_quad(gpui::fill(
                    Bounds { origin: Point { x: px(min_x), y: px(min_y) }, size: size(px(qw), px(qh)) },
                    rgb(0xFFFFFF),
                ));
            }
            d += period;
        }
    }
}

/// Draw horizontal and vertical guide lines over the canvas in cyan (matching
/// Photoshop's guide color). Each guide is a 1px-thick quad spanning the full
/// canvas width/height. `guides_h` / `guides_v` are doc-px positions.
pub(crate) fn paint_guides(
    win: &mut Window,
    painted: Bounds<Pixels>,
    guides_h: &[f32],
    guides_v: &[f32],
    doc_w: f32,
    doc_h: f32,
) {
    if (doc_w <= 0.0) || (doc_h <= 0.0) {
        return;
    }
    let ox = f32::from(painted.origin.x);
    let oy = f32::from(painted.origin.y);
    let bw = f32::from(painted.size.width);
    let bh = f32::from(painted.size.height);
    let sx = bw / doc_w;
    let sy = bh / doc_h;
    let guide_color = gpui::Rgba { r: 0.0, g: 0.85, b: 1.0, a: 0.8 };

    for &gy in guides_h {
        let wy = oy + gy * sy;
        win.paint_quad(gpui::fill(
            Bounds {
                origin: Point { x: px(ox), y: px(wy) },
                size: size(px(bw), px(1.0)),
            },
            guide_color,
        ));
    }
    for &gx in guides_v {
        let wx = ox + gx * sx;
        win.paint_quad(gpui::fill(
            Bounds {
                origin: Point { x: px(wx), y: px(oy) },
                size: size(px(1.0), px(bh)),
            },
            guide_color,
        ));
    }
}

thread_local! {
    /// Process-relative monotonic clock anchor for the marching-ants phase.
    static ANT_START: Instant = Instant::now();
}
