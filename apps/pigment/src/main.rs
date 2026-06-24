//! Pigment — GPUI host (coexists with the eframe/egui `pigment` binary).
//!
//! Architecture, honest from day one: GPUI paints the chrome (toolbar, tools
//! strip, right dock of panels), while the document pixels come from
//! `CanvasHost`, which drives the REAL prism-canvas compositor on its own wgpu
//! device and bridges the result in as a `RenderImage`. This is the split the
//! real app keeps.
//!
//! The integration backbone:
//! - [`app_state::App`] owns everything panels read/mutate (host, doc, tool,
//!   brush, view) and exposes the single mutation choke point `App::apply`.
//! - Panels live in [`panels`] as `render(app: &App, cx: &mut Context<Pigment>)`
//!   functions and emit [`app_state::Action`]s via `cx.listener` →
//!   `root.app.apply(...)`. See `panels/mod.rs` for the verbatim convention that
//!   parallel agents follow.
//! - This root view (`Pigment`) holds the `App` and lays out the chrome around
//!   the live canvas.

mod app_state;
mod canvas_host;
mod content_aware;
mod filters;
mod filters_extra;
mod lens_correction;
mod panels;
mod perspective_warp;
mod plugin;
mod welcome;

use prism_ui::PrismAssets;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use app_state::App;
use app_state::Action;
use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, deferred, div, img, px, rgb, rgba, size, AppContext, Application, Bounds, Context,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Pixels, Point, Render, RenderImage, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowOptions,
};
use prism_ui::{colors, font_size};

use panels::{DOCK_W, STRIP_W, TOOLBAR_H};

/// The GPUI root view. Owns the shared [`App`]; panels read it and route their
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Pigment {
    app: App,
    /// The painted content rectangle of the canvas `img` (window space), captured
    /// each frame by an overlaid `canvas()` element's paint callback. Read by the
    /// mouse handlers to map window position → doc px. `Rc<Cell<_>>` because the
    /// paint callback (which receives the gpui `App`, not our view) writes it while
    /// the view reads it. `None` until the first paint.
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Keyboard focus for the canvas, so the root can receive key events
    /// (Cmd+Z / Cmd+Shift+Z undo-redo, and Text-tool typing).
    focus: FocusHandle,
    /// Cached marching-ants boundary segments (doc px, `[x0,y0,x1,y1]`), re-traced
    /// from the engine selection mask only when `sel_gen_cached` falls behind
    /// `app.selection_generation` (so we don't read the mask back every frame).
    sel_boundary: Vec<[f32; 4]>,
    /// The `App::selection_generation` value `sel_boundary` was traced at.
    sel_gen_cached: u64,
    /// The canvas `RenderImage` painted last frame. gpui's sprite atlas only
    /// frees an image's GPU tile via explicit `window.drop_image`; pigment builds
    /// a fresh `RenderImage` (new `id`) on every dirty composite, so without this
    /// each redraw leaks one atlas tile. We drop the previous image when its `id`
    /// differs from the current one (an idle/cached frame returns the same image →
    /// keep its tile).
    last_image: Option<Arc<RenderImage>>,
}

impl Focusable for Pigment {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Pigment {
    /// Handle a key press on the focused canvas. Returns nothing; mutates `app`
    /// and requests a redraw when it consumes the event. Two roles:
    /// 1. Cmd+Z / Cmd+Shift+Z (or Cmd+Y) → undo / redo, always.
    /// 2. While a Text edit is in progress, printable keys / Backspace edit the
    ///    run, and Enter/Escape commit it.
    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
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
}

impl Pigment {
    /// The bridged composite image (re-composites only when the host is dirty).
    fn doc_image(&mut self) -> Arc<RenderImage> {
        self.app.host.image()
    }

    /// Map a window-space position to doc px using the last painted canvas rect,
    /// clamped to the document. Returns `None` if the canvas hasn't painted yet
    /// or has zero size.
    fn window_to_doc(&self, pos: Point<Pixels>) -> Option<[f32; 2]> {
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
}

/// Wraps a dock panel with a small "⤢" detach button in the header.
/// Returns a plain div — floating/detached state is shown via a deferred
/// overlay added separately to the root view.
fn dockable_wrap(
    name: &'static str,
    content: impl IntoElement + 'static,
    _app: &App,
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

impl Render for Pigment {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Composite first so panels read fresh engine state (the histogram binds
        // to `host.histogram()`, populated by `image()`).
        let doc = self.doc_image();

        // Free the previous frame's atlas tile when the composite produced a new
        // `RenderImage`. gpui only releases an image's GPU tile on explicit
        // `drop_image`; an idle/cached frame returns the same `id`, so we keep it.
        if let Some(prev) = self.last_image.take() {
            if prev.id != doc.id {
                let _ = window.drop_image(prev);
            }
        }
        self.last_image = Some(doc.clone());
        let zoom = self.app.view.zoom.max(0.05);
        let (w, h) = (self.app.host.doc_w as f32 * zoom, self.app.host.doc_h as f32 * zoom);

        // Re-trace the marching-ants boundary only when the selection changed
        // (the generation counter advanced), so we don't read the GPU mask back
        // every animation frame — just reuse the cached doc-px segments.
        if self.sel_gen_cached != self.app.selection_generation {
            self.sel_boundary = self.app.host.selection_boundary();
            self.sel_gen_cached = self.app.selection_generation;
        }
        // Snapshot the boundary + doc size for the paint closure (it receives the
        // gpui `App`, not our view, so it can't borrow `self`). `Rc` keeps the
        // clone cheap when the selection is large.
        let boundary = Rc::new(self.sel_boundary.clone());
        // Use the native doc dimensions (not the canvas window-px size) so the
        // scale factor `bw / doc_w` in paint_marching_ants correctly gives `zoom`,
        // mapping doc-px boundary segments to window-px positions.
        let (doc_w, doc_h) = (self.app.host.doc_w as f32, self.app.host.doc_h as f32);
        // Keep ants animating while a selection is shown.
        let animate = !self.sel_boundary.is_empty();

        // Autosave: check if the interval has elapsed and write if so.
        self.app.maybe_autosave();

        // Tick cursor blink when text editing is active.
        let text_editing = self.app.text_editing();
        if text_editing {
            self.app.tick_cursor();
        } else {
            self.app.cursor_blink_on = false;
            self.app.cursor_blink_tick = 0;
        }
        let cursor_blink_on = self.app.cursor_blink_on;
        let cursor_doc_pos = self.app.text_cursor_doc_pos();

        // Batch 4: autosave banner and print/plugin overlays.
        let autosave_pending = self.app.autosave_restore_pending;

        // Build panel elements (read-only &App + cx for Action listeners).
        let app = &self.app;
        let toolbar = panels::toolbar::render(app, cx);
        let tool_options = panels::tool_options::render(app, cx);
        let tools = panels::tools::render(app, cx);
        let color = panels::color::render(app, cx);
        let adjustments = panels::adjustments::render(app, cx);
        let histogram = panels::histogram::render(app, cx);
        let channels = panels::channels::render(app, cx);
        let history = panels::history::render(app, cx);
        let plugins = panels::plugins::render(app, cx);
        let layers = panels::layers::render(app, cx);
        let layer_style_panel = if app.style_panel_open {
            Some(panels::layer_style::render(app, cx))
        } else {
            None
        };

        // Filter gallery overlay — rendered via the dedicated panel module.
        let filter_gallery_overlay: Option<_> = if app.filter_gallery_open {
            Some(panels::filter_gallery::render(app, cx))
        } else {
            None
        };

        // Camera Raw overlay — rendered via the dedicated panel module when open.
        let camera_raw_overlay: Option<_> = if app.camera_raw_open {
            Some(panels::camera_raw::render(app, cx))
        } else {
            None
        };

        // Print dialog overlay.
        let print_overlay: Option<_> = if app.show_print_dialog {
            Some(panels::print::render(app, cx))
        } else {
            None
        };

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| this.on_key(ev, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Top toolbar (full width).
            .child(
                div()
                    .w_full()
                    .h(px(TOOLBAR_H))
                    .bg(colors::surface_raised())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(toolbar),
            )
            // Tool-options strip (full width, under the toolbar): the active tool's
            // parameters (brush size/hardness/opacity, fill tolerance, …).
            .child(
                div()
                    .w_full()
                    .bg(colors::surface_raised())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(tool_options),
            )
            // Autosave restore banner — shown when a newer autosave exists on disk.
            .when(autosave_pending, |d| {
                d.child(
                    div()
                        .w_full()
                        .px_4()
                        .py_2()
                        .bg(rgba(0xf59e0b33))
                        .border_b_1()
                        .border_color(rgba(0xf59e0bcc))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .text_size(px(font_size::SM))
                        .child(
                            div()
                                .flex_1()
                                .text_color(colors::text_primary())
                                .child("Autosave found — restore unsaved changes?")
                        )
                        .child(
                            div()
                                .id("autosave-restore")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(rgba(0x22c55ecc))
                                .text_color(colors::text_primary())
                                .cursor_pointer()
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::RestoreAutosave);
                                    cx.notify();
                                }))
                                .child("Restore")
                        )
                        .child(
                            div()
                                .id("autosave-dismiss")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_secondary())
                                .cursor_pointer()
                                .on_click(cx.listener(|root, _ev, _win, cx| {
                                    root.app.apply(Action::DismissAutosave);
                                    cx.notify();
                                }))
                                .child("Dismiss")
                        )
                )
            })
            // Workspace: tools strip | canvas | dock.
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h(px(0.0))
                    // Left tools strip.
                    .child(
                        div()
                            .id("left-strip")
                            .flex_shrink_0()
                            .w(px(STRIP_W))
                            .h_full()
                            .overflow_y_scroll()
                            .bg(colors::surface_raised())
                            .border_r_1()
                            .border_color(colors::surface_border())
                            .child(tools),
                    )
                    // Center canvas (doc centered on a dark field). The doc image
                    // sits in a stateful, sized `div` that owns the mouse input:
                    // press → begin_stroke, drag → continue_stroke, release →
                    // end_stroke. An overlaid `canvas()` element captures the
                    // painted content rect each frame so the handlers can map
                    // window position → doc px.
                    .child(
                        div()
                            .flex_1()
                            // Clip the work-area to its own column. Without this the
                            // doc-sized inner div below overflows the flex row when the
                            // image (doc px × zoom) is wider/taller than the available
                            // space: GPUI does not clip un-`overflow_hidden` children, so
                            // the overflow both pushes the right dock partly off the
                            // window's right edge (BUG 1) and paints up over the top bar
                            // (BUG 2). reel/contour clip their center column the same way.
                            .overflow_hidden()
                            .h_full()
                            .bg(colors::surface_bg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .id("canvas")
                                    .relative()
                                    .flex_shrink_0()
                                    .w(px(w))
                                    .h(px(h))
                                    // Image sits first (bottom layer); the canvas
                                    // overlay is second so it paints on top.
                                    .child(img(doc).w(px(w)).h(px(h)))
                                    // Capture the painted rect (window space) into
                                    // the shared cell for the handlers to read.
                                    // Must be AFTER img() so marching-ants and
                                    // guides are drawn on top of the image.
                                    .child(
                                        canvas(
                                            {
                                                let cell = self.canvas_bounds.clone();
                                                move |bounds, _win, _cx| {
                                                    cell.set(Some(bounds));
                                                    bounds
                                                }
                                            },
                                            {
                                                let boundary = boundary.clone();
                                                let guides_h = app.guides_h.clone();
                                                let guides_v = app.guides_v.clone();
                                                let guides_visible = app.guides_visible;
                                                move |_bounds, painted, win, _cx| {
                                                    paint_marching_ants(
                                                        win, painted, &boundary, doc_w, doc_h,
                                                    );
                                                    if guides_visible {
                                                        paint_guides(
                                                            win, painted, &guides_h, &guides_v,
                                                            doc_w, doc_h,
                                                        );
                                                    }
                                                    if animate || text_editing {
                                                        win.request_animation_frame();
                                                    }
                                                }
                                            },
                                        )
                                        .absolute()
                                        .inset_0()
                                        .size_full(),
                                    )
                                    // Slice overlays — blue semi-transparent rects.
                                    .children(self.app.slices.iter().map(|s| {
                                        let [sx, sy, sw, sh] = s.rect;
                                        div()
                                            .absolute()
                                            .left(px(sx * zoom))
                                            .top(px(sy * zoom))
                                            .w(px(sw * zoom))
                                            .h(px(sh * zoom))
                                            .border_1()
                                            .border_color(rgba(0x4488ffcc))
                                            .bg(rgba(0x4488ff22))
                                    }))
                                    // In-progress slice drag preview.
                                    .when_some(self.app.slice_drag_start, |d, start| {
                                        if let Some(end) = self.app.last_drag {
                                            let x = start[0].min(end[0]) * zoom;
                                            let y = start[1].min(end[1]) * zoom;
                                            let sw = (start[0] - end[0]).abs() * zoom;
                                            let sh = (start[1] - end[1]).abs() * zoom;
                                            d.child(
                                                div()
                                                    .absolute()
                                                    .left(px(x))
                                                    .top(px(y))
                                                    .w(px(sw))
                                                    .h(px(sh))
                                                    .border_1()
                                                    .border_color(rgba(0x88bbffee))
                                                    .bg(rgba(0x88bbff11)),
                                            )
                                        } else {
                                            d
                                        }
                                    })
                                    // Text cursor blink overlay.
                                    .when(cursor_blink_on, |d| {
                                        if let Some([cx_doc, cy_doc]) = cursor_doc_pos {
                                            let cursor_h = (self.app.text_size * zoom).max(12.0);
                                            d.child(
                                                div()
                                                    .absolute()
                                                    .left(px(cx_doc * zoom))
                                                    .top(px(cy_doc * zoom))
                                                    .w(px(1.5))
                                                    .h(px(cursor_h))
                                                    .bg(rgba(0xffffffcc)),
                                            )
                                        } else {
                                            d
                                        }
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, ev: &gpui::MouseDownEvent, window, cx| {
                                            // Keep keyboard focus on the canvas so
                                            // Cmd+Z / text typing reach our handler.
                                            window.focus(&this.focus);
                                            if let Some(doc) = this.window_to_doc(ev.position) {
                                                this.app.begin_drag(
                                                    doc,
                                                    ev.modifiers.alt,
                                                    ev.modifiers.shift,
                                                );
                                                cx.notify();
                                            }
                                        }),
                                    )
                                    .on_mouse_move(cx.listener(
                                        |this, ev: &gpui::MouseMoveEvent, _win, cx| {
                                            if ev.dragging() {
                                                if let Some(doc) = this.window_to_doc(ev.position) {
                                                    this.app.continue_drag(doc);
                                                    cx.notify();
                                                }
                                            }
                                        },
                                    ))
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, cx| {
                                            this.app.end_drag();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    )
                    // Right dock: color / adjustments / histogram / layers.
                    .child(
                        div()
                            .id("right-dock")
                            .flex_shrink_0()
                            .w(px(DOCK_W))
                            .min_w(px(220.0))
                            .h_full()
                            .overflow_x_hidden()
                            .bg(colors::surface_raised())
                            .border_l_1()
                            .border_color(colors::surface_border())
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(dockable_wrap("Color", color, app, cx))
                            .child(dockable_wrap("Adjustments", adjustments, app, cx))
                            .child(dockable_wrap("Histogram", histogram, app, cx))
                            .child(dockable_wrap("Channels", channels, app, cx))
                            .child(dockable_wrap("History", history, app, cx))
                            .child(dockable_wrap("Plugins", plugins, app, cx))
                            .child(dockable_wrap("Layers", layers, app, cx))
                            .when_some(layer_style_panel, |s: Stateful<gpui::Div>, p| s.child(p)),
                    ),
            )
            .children(filter_gallery_overlay)
            .children(camera_raw_overlay)
            .children(print_overlay)
    }
}

/// Paint the active selection's boundary as an animated dashed outline (marching
/// ants) over the canvas. `boundary` is the engine-traced edge segments in doc px
/// (`[x0,y0,x1,y1]`); `painted` is the canvas content rect in window space at the
/// doc's native size, so doc px map by a straight scale. The dash phase advances
/// with wall-clock time, and the caller schedules the next animation frame so the
/// ants keep crawling. Drawn as short alternating black/white quads (cheap, no
/// `Path` stroking) — a 1px-thick run of dashes along each unit edge.
fn paint_marching_ants(
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
fn paint_guides(
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
    static ANT_START: std::time::Instant = std::time::Instant::now();
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new().with_assets(PrismAssets).run(|cx: &mut gpui::App| {
        prism_ui::init(cx);
        // Open the welcome screen as a separate OS-level window. It closes
        // itself when the user clicks "New Document" or "Open File…".
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(
                    Bounds::centered(None, size(px(900.0), px(580.0)), cx),
                )),
                ..Default::default()
            },
            |win, cx| {
                let focus = cx.focus_handle();
                win.focus(&focus);
                cx.new(|_cx| welcome::WelcomeView::new(focus))
            },
        )
        .expect("failed to open welcome window");

        let bounds = cx.primary_display()
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
                    // Synthetic-stroke smoke test (guarded). Proves the brush →
                    // paint_dabs → recomposite path executes end-to-end without a
                    // real mouse, before the window even shows. Set PIGMENT_PAINT_TEST
                    // to enable. Drives a short diagonal stroke across doc center.
                    if std::env::var_os("PIGMENT_PAINT_TEST").is_some() {
                        let (w, h) = (app.host.doc_w as f32, app.host.doc_h as f32);
                        app.begin_stroke([w * 0.25, h * 0.25]);
                        app.continue_stroke([w * 0.5, h * 0.5]);
                        app.continue_stroke([w * 0.75, h * 0.75]);
                        app.end_stroke();
                        // Force a recomposite + readback so we can confirm the
                        // painted pixels actually round-tripped through the engine.
                        // Sample the doc-center BGRA8 pixel (on the stroke path):
                        // the brush is rgb(20,120,230), so a successful unmasked
                        // paint shows BGRA≈(230,120,20,255) — strong blue, not the
                        // placeholder background.
                        let img = app.host.image();
                        let (iw, ih) = (img.size(0).width.0 as usize, img.size(0).height.0 as usize);
                        if let Some(bytes) = img.as_bytes(0) {
                            let idx = ((ih / 2) * iw + iw / 2) * 4;
                            if idx + 3 < bytes.len() {
                                log::info!(
                                    "PAINT_TEST: synthetic stroke painted, recomposited {iw}x{ih}; \
                                     center pixel BGRA=({},{},{},{}) (brush rgb(20,120,230))",
                                    bytes[idx],
                                    bytes[idx + 1],
                                    bytes[idx + 2],
                                    bytes[idx + 3]
                                );
                            }
                        }
                    }
                    let focus = cx.focus_handle();
                    window.focus(&focus);
                    Pigment {
                        app,
                        canvas_bounds: Rc::new(Cell::new(None)),
                        focus,
                        sel_boundary: Vec::new(),
                        sel_gen_cached: u64::MAX, // force a first trace
                        last_image: None,
                    }
                })
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
