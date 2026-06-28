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
mod host_helpers;
mod welcome;
mod windows;

use host_helpers::{dockable_wrap, paint_guides, paint_marching_ants};
use prism_ui::PrismAssets;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use app_state::App;
use app_state::Action;
use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, deferred, div, img, px, rgb, rgba, size, AppContext, Application, Bounds, Context,
    Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Pixels, Point, Render, RenderImage, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowKind, WindowOptions,
};
use prism_core::LayerId;
use prism_ui::{colors, font_size, TextField};

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

    // ── Real editable text fields (prism_ui::TextField) ──────────────────────
    /// The layer currently being renamed + the live field seeded with its name.
    /// `None` when no rename is in progress. The layers panel renders the field
    /// in place of the name label for this row; `on_submit` emits `RenameLayer`.
    rename_field: Option<(LayerId, Entity<TextField>)>,
    /// Persistent hex-color entry field for the color panel (`#RRGGBB` → color).
    hex_field: Entity<TextField>,
    /// Persistent PSD output-path field for the export panel.
    psd_path_field: Entity<TextField>,
    /// Text-tool content field: while a text run is being placed, this field's
    /// content IS the run's string (pushed via `SetTextContent` on change).
    text_tool_field: Entity<TextField>,
    /// Typeable brush-size entry (px). `on_submit` parses the number → `SetBrushSize`.
    brush_size_field: Entity<TextField>,
    /// Typeable brush-hardness entry (%). `on_submit` → `SetBrushHardness` (0..1).
    brush_hardness_field: Entity<TextField>,
    /// Typeable brush-opacity entry (%). `on_submit` → `SetBrushOpacity` (0..1).
    brush_opacity_field: Entity<TextField>,
    /// Typeable free-transform rotation entry (degrees). `on_submit` →
    /// `SetTransformRotation`.
    xform_rotation_field: Entity<TextField>,
    /// Typeable free-transform horizontal-skew entry (degrees). `on_submit` →
    /// `SetTransformSkew` (preserving the current skew-Y).
    xform_skew_field: Entity<TextField>,
}

impl Focusable for Pigment {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
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

        // Clone the field entities up front so the panel calls (which take
        // `&self.app`, an immutable borrow of `self`) don't conflict with reading
        // these `self` fields.
        let hex_field = self.hex_field.clone();
        let psd_path_field = self.psd_path_field.clone();
        let text_tool_field = self.text_tool_field.clone();
        let brush_size_field = self.brush_size_field.clone();
        let brush_hardness_field = self.brush_hardness_field.clone();
        let brush_opacity_field = self.brush_opacity_field.clone();
        let xform_rotation_field = self.xform_rotation_field.clone();
        let xform_skew_field = self.xform_skew_field.clone();
        let rename_field = self.rename_field.clone();

        // Build panel elements (read-only &App + cx for Action listeners).
        let app = &self.app;
        let toolbar = panels::toolbar::render(app, cx);
        let tool_options = panels::tool_options::render(
            app,
            &text_tool_field,
            &brush_size_field,
            &brush_hardness_field,
            &brush_opacity_field,
            &xform_rotation_field,
            &xform_skew_field,
            cx,
        );
        let tools = panels::tools::render(app, cx);
        let color = panels::color::render(app, &hex_field, cx);
        let adjustments = panels::adjustments::render(app, cx);
        let histogram = panels::histogram::render(app, cx);
        let channels = panels::channels::render(app, cx);
        let history = panels::history::render(app, cx);
        let plugins = panels::plugins::render(app, cx);
        let navigator = panels::navigator::render(app, cx);
        let doc_tabs = panels::navigator::render_tabs(app, cx);
        let psd_export = panels::psd_export::render(app, &psd_path_field, cx);
        let layers = panels::layers::render(app, rename_field.as_ref(), cx);
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
            // Document-tab strip (multi-doc navigator): one tab per open doc,
            // active tab accented, trailing "+" opens a new tab.
            .child(
                div()
                    .w_full()
                    .bg(colors::surface_bg())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(doc_tabs),
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
                                                // Rich guide model (wave 3): split the
                                                // `GuideState` guides into horizontal (y)
                                                // and vertical (x) doc-px positions so the
                                                // existing line painter can draw them.
                                                let gs_visible = app.guide_state.visible;
                                                let mut gs_h: Vec<f32> = Vec::new();
                                                let mut gs_v: Vec<f32> = Vec::new();
                                                for g in &app.guide_state.guides {
                                                    match g.orientation {
                                                        app_state::GuideOrientation::Horizontal => {
                                                            gs_h.push(g.position)
                                                        }
                                                        app_state::GuideOrientation::Vertical => {
                                                            gs_v.push(g.position)
                                                        }
                                                    }
                                                }
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
                                                    if gs_visible {
                                                        paint_guides(
                                                            win, painted, &gs_h, &gs_v,
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
                                                let was_text = this.app.text_editing();
                                                this.app.begin_drag(
                                                    doc,
                                                    ev.modifiers.alt,
                                                    ev.modifiers.shift,
                                                );
                                                // A Text-tool click placed a fresh run:
                                                // seed the content field with the run's
                                                // current string (empty for a new run) and
                                                // focus it so the user types real text.
                                                // Deferred so `set_text`'s `on_change`
                                                // (which dispatches back into THIS view)
                                                // runs after this listener releases the
                                                // entity — avoiding a reentrant update.
                                                if !was_text && this.app.text_editing() {
                                                    let seed =
                                                        this.app.text_content().to_string();
                                                    let field = this.text_tool_field.clone();
                                                    window.defer(cx, move |window, cx| {
                                                        field.update(cx, |f, cx| {
                                                            f.set_text(seed, window, cx);
                                                        });
                                                        let fh = field.focus_handle(cx);
                                                        window.focus(&fh);
                                                    });
                                                }
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
                            .child(dockable_wrap("Navigator", navigator, app, cx))
                            .child(dockable_wrap("Color", color, app, cx))
                            .child(dockable_wrap("Adjustments", adjustments, app, cx))
                            .child(dockable_wrap("Histogram", histogram, app, cx))
                            .child(dockable_wrap("Channels", channels, app, cx))
                            .child(dockable_wrap("History", history, app, cx))
                            .child(dockable_wrap("Plugins", plugins, app, cx))
                            .child(dockable_wrap("Export PSD", psd_export, app, cx))
                            .child(dockable_wrap("Layers", layers, app, cx))
                            .when_some(layer_style_panel, |s: Stateful<gpui::Div>, p| s.child(p)),
                    ),
            )
            .children(filter_gallery_overlay)
            .children(camera_raw_overlay)
            .children(print_overlay)
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new().with_assets(PrismAssets).run(|cx: &mut gpui::App| {
        prism_ui::init(cx);

        // Open the main editor window first so we can capture a WeakEntity to
        // pass into the welcome screen. The welcome window is opened second so
        // it appears on top of the editor (Floating kind ensures z-ordering).
        let bounds = cx.primary_display()
            .map(|d| d.bounds())
            .unwrap_or_else(|| Bounds::centered(None, size(px(1600.0), px(1000.0)), cx));

        let main_entity = cx.open_window(
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

                    // ── Build the persistent editable text fields ──
                    // Each captures a weak handle to THIS view (resolves once
                    // construction completes) so its submit/change closure can
                    // dispatch an Action back through `App::apply`.

                    // Hex color: parse `#RRGGBB` on Enter and set the brush color.
                    let weak_hex: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let hex_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("#RRGGBB")
                            .on_submit(move |text, _win, app| {
                                if let Some(color) = panels::color::parse_hex_color(text) {
                                    if let Some(entity) = weak_hex.upgrade() {
                                        entity.update(app, |root, cx| {
                                            root.app.apply(Action::SetBrushColor(color));
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                    });

                    // PSD output path: feed `SetPsdExportPath` on Enter.
                    let weak_psd: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let psd_path_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("/path/to/output.psd")
                            .on_submit(move |text, _win, app| {
                                let path = text.to_string();
                                if let Some(entity) = weak_psd.upgrade() {
                                    entity.update(app, |root, cx| {
                                        root.app.apply(Action::SetPsdExportPath(path));
                                        cx.notify();
                                    });
                                }
                            })
                    });

                    // Text-tool content: push the full run string into the active
                    // text layer on every change (real typing onto the canvas).
                    let weak_text: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let weak_text_submit: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let text_tool_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("Type your text…")
                            .on_change(move |text, _win, app| {
                                let content = text.to_string();
                                if let Some(entity) = weak_text.upgrade() {
                                    entity.update(app, |root, cx| {
                                        root.app.apply(Action::SetTextContent(content));
                                        cx.notify();
                                    });
                                }
                            })
                            // Enter commits the run (drops the edit handle); the
                            // rasterized pixels stay on the layer.
                            .on_submit(move |_text, _win, app| {
                                if let Some(entity) = weak_text_submit.upgrade() {
                                    entity.update(app, |root, cx| {
                                        root.app.commit_text();
                                        cx.notify();
                                    });
                                }
                            })
                    });

                    // Brush param fields — type a number + Enter to set the value.
                    // Each parses tolerantly (units stripped) and dispatches the
                    // existing Set* action, which clamps in `App::apply`.
                    let weak_bsize: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let brush_size_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("px")
                            .width(px(56.0))
                            .on_submit(move |text, _win, app| {
                                if let Some(v) = panels::num_input::parse_f32(text) {
                                    if let Some(entity) = weak_bsize.upgrade() {
                                        entity.update(app, |root, cx| {
                                            root.app.apply(Action::SetBrushSize(v));
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                    });
                    let weak_bhard: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let brush_hardness_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("%")
                            .width(px(56.0))
                            .on_submit(move |text, _win, app| {
                                if let Some(v) = panels::num_input::parse_percent_fraction(text) {
                                    if let Some(entity) = weak_bhard.upgrade() {
                                        entity.update(app, |root, cx| {
                                            root.app.apply(Action::SetBrushHardness(v));
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                    });
                    let weak_bopac: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let brush_opacity_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("%")
                            .width(px(56.0))
                            .on_submit(move |text, _win, app| {
                                if let Some(v) = panels::num_input::parse_percent_fraction(text) {
                                    if let Some(entity) = weak_bopac.upgrade() {
                                        entity.update(app, |root, cx| {
                                            root.app.apply(Action::SetBrushOpacity(v));
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                    });

                    // Free-transform rotation + skew-X fields. Rotation submits
                    // SetTransformRotation; skew-X submits SetTransformSkew while
                    // preserving the live skew-Y (read from the model on submit).
                    let weak_rot: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let xform_rotation_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("°")
                            .width(px(56.0))
                            .on_submit(move |text, _win, app| {
                                if let Some(v) = panels::num_input::parse_f32(text) {
                                    if let Some(entity) = weak_rot.upgrade() {
                                        entity.update(app, |root, cx| {
                                            root.app.apply(Action::SetTransformRotation(v));
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                    });
                    let weak_skew: gpui::WeakEntity<Pigment> = cx.weak_entity();
                    let xform_skew_field = cx.new(|cx| {
                        TextField::new(cx)
                            .placeholder("°")
                            .width(px(56.0))
                            .on_submit(move |text, _win, app| {
                                if let Some(v) = panels::num_input::parse_f32(text) {
                                    if let Some(entity) = weak_skew.upgrade() {
                                        entity.update(app, |root, cx| {
                                            let skew_y = root.app.xform_skew_y_deg;
                                            root.app.apply(Action::SetTransformSkew {
                                                skew_x: v,
                                                skew_y,
                                            });
                                            cx.notify();
                                        });
                                    }
                                }
                            })
                    });

                    Pigment {
                        app,
                        canvas_bounds: Rc::new(Cell::new(None)),
                        focus,
                        sel_boundary: Vec::new(),
                        sel_gen_cached: u64::MAX, // force a first trace
                        last_image: None,
                        rename_field: None,
                        hex_field,
                        psd_path_field,
                        text_tool_field,
                        brush_size_field,
                        brush_hardness_field,
                        brush_opacity_field,
                        xform_rotation_field,
                        xform_skew_field,
                    }
                })
            },
        )
        .expect("failed to open window");

        // Open the welcome screen as a Floating OS-level window so it sits on
        // top of the editor. Pass a WeakEntity so its buttons can dispatch
        // Actions to the main Pigment view before closing themselves.
        let weak_main = main_entity
            .entity(cx)
            .expect("failed to get main entity")
            .downgrade();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(
                    Bounds::centered(None, size(px(900.0), px(580.0)), cx),
                )),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus = cx.focus_handle();
                win.focus(&focus);
                cx.new(|_cx| welcome::WelcomeView::new(focus, weak_main))
            },
        )
        .expect("failed to open welcome window");

        cx.activate(true);
    });
}
