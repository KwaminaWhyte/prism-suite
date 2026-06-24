#![allow(deprecated)]
//! Contour — vector graphics editor (Illustrator analog), part of the Prism suite.

// Logic modules (formerly contour-app)
mod ai_eps;
mod align;
mod appearance;
mod arrange;
mod artboard;
mod blend;
mod boolean;
mod clip;
mod clipboard;
mod document;
mod effects;
mod envelope;
mod export;
mod export_formats;
mod extrude3d;
mod gradient_mesh;
mod eyedropper;
mod fonts;
mod gradient;
mod graph_gen;
mod graphic_styles;
mod mesh_gradient;
mod group;
mod history;
mod layers;
mod liveshape;
mod opacity_mask;
mod pathedit;
mod pattern_brush;
mod perspective;
mod placed_image;
mod recolor;
mod shapebuilder;
mod snap;
mod stroke;
mod swatches;
mod symbols;
mod text;
mod text_on_path;
mod trace;
mod trace_contour;
mod transform;
mod workspace;

// GPUI host modules (formerly contour-gpui)
mod app_state;
mod canvas_host;
mod color_picker_window;
mod document_setup_window;
mod export_window;
mod panels;
mod preferences_window;
mod rename_edit;
mod view_input;
mod welcome;

use prism_ui::{colors as ui_colors, PrismAssets};

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use app_state::{App, NodeTarget, Tool, View};
use crate::transform::Handle;
use gpui::{
    canvas, div, img, px, rgb, rgba, size, AppContext, Application, Bounds, Context, FocusHandle,
    Focusable, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels,
    Point, Render, RenderImage, StatefulInteractiveElement, Styled, Window,
    WindowBounds, WindowKind, WindowOptions,
};

use panels::{DOCK_W, STRIP_W, TOOLBAR_H};

/// Per-gesture mouse state for the canvas, tracked between mouse-down and
/// mouse-up. Held in a shared `Cell` so the (separate) down/move/up listeners
/// and the render pass (which draws the live drag preview) all see it.
///
/// Wave 8 addition: `Artboard` drag mirrors `Create` but commits via
/// `Action::AddArtboard` instead of `Action::CreateShape`.
#[derive(Clone, Copy, Debug)]
enum Drag {
    /// A Rect / Ellipse drag-create in progress. Both corners are **window
    /// space**; they're mapped to document space only when the shape is created
    /// and when drawing the live preview, so the preview tracks pan/zoom mid-drag.
    Create {
        tool: Tool,
        a: Point<Pixels>,
        b: Point<Pixels>,
    },
    /// A pan in progress (Alt+left-drag or middle-drag). `last` is the previous
    /// cursor position (window space); each move emits the delta as `PanBy`.
    Pan { last: Point<Pixels> },
    /// A Pen-tool handle drag: the user click-placed an anchor and is dragging
    /// to set its out-tangent. The anchor is already in `app.pen`; each move
    /// updates the freshest anchor's handle.
    PenHandle,
    /// A node-edit (Direct-Select) drag of one anchor or tangent-handle of the
    /// selected path. Coalesced into one undo entry (BeginInteraction at down,
    /// EndInteraction at up).
    Node { target: NodeTarget },
    /// A marquee (rubber-band) selection with the Select tool: a drag that began
    /// on empty canvas. Both corners are **window space** (mapped to document
    /// space on release for the hit test, and to viewport-local for the preview).
    Marquee { a: Point<Pixels>, b: Point<Pixels> },
    /// An Artboard tool drag: like `Create` but commits via `AddArtboard`.
    /// Both corners are **window space**.
    ArtboardDrag { a: Point<Pixels>, b: Point<Pixels> },
    /// Knife tool drag: the cut line in **window space**. Committed on release
    /// via `Action::KnifeSlice` (doc-space endpoints).
    KnifeDrag { a: Point<Pixels>, b: Point<Pixels> },
    /// A scale-handle drag on the selection bounding box. `handle_idx` matches
    /// `Handle::ALL`; `last` is the previous cursor (window space, for coalescing).
    TransformScale { handle_idx: u8, last: Point<Pixels> },
    /// Move the selection with the Select tool (mouse-down on a selected shape,
    /// then drag). `last` is the previous cursor (window space).
    MoveSelected { last: Point<Pixels> },
    /// Width tool drag: `start_x` is the drag origin (window x); dragging right
    /// widens the end, dragging left widens the start.
    WidthDrag { start_x: f32, start_mul: f32 },
    /// Shape builder drag: accumulate selection on click, apply op on release.
    /// `subtract` = Alt/Option held → subtract mode; else unite mode.
    ShapeBuilderDrag { a: Point<Pixels>, b: Point<Pixels>, subtract: bool },
}

/// The GPUI root view. Owns the shared [`App`]; panels read it and route their
/// mutations back through `app.apply` inside `cx.listener` callbacks.
struct Contour {
    app: App,
    /// The painted content rectangle of the **canvas viewport** (the dark field
    /// the artboard floats on), in window space, captured each frame by an
    /// overlaid `canvas()` paint callback. The view transform's `offset` is
    /// relative to this rect's origin, so it's the anchor for every window↔doc
    /// map. `Rc<Cell<_>>` because the paint callback writes it while the view
    /// reads it; `None` until the first paint.
    viewport_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The active canvas gesture, if any (drag-create or pan). Shared between the
    /// mouse listeners and the render pass (which draws the live create preview).
    drag: Rc<Cell<Option<Drag>>>,
    /// Keyboard focus for the root view, so the canvas receives key events
    /// (Cmd+Z / Cmd+Shift+Z undo-redo, Delete, Escape / Enter to finish a pen).
    focus: FocusHandle,
    /// The canvas `RenderImage` painted in the previous frame, retained so its
    /// GPU sprite-atlas tile can be released via `window.drop_image` when a new
    /// `RenderImage` (new `image.id`) replaces it. Without this, every redraw
    /// that re-rasterizes builds a fresh image and leaks its old atlas tile.
    last_image: Option<Arc<RenderImage>>,
    /// In-progress inline rename of a Layers / Symbols / Artboards row, if any.
    /// Holds the real focused `TextField` whose submit dispatches the rename.
    rename: Option<rename_edit::RenameEdit>,
    /// Focused `TextField` for editing the *content* of the text object in
    /// `app.editing_text`, if a Type-tool edit session is open. The field's
    /// `on_change` dispatches [`Action::SetTextObjectContent`].
    text_edit: Option<rename_edit::TextEdit>,
}

impl Render for Contour {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Reconcile the inline text-content field with the engine: if the edit
        // session ended elsewhere (tool switch dispatched `FinishText`, undo, …)
        // drop the now-stale focused field so it doesn't linger on the canvas.
        if self.text_edit.is_some()
            && (self.app.editing_text.is_none() || self.app.active != Tool::Type)
        {
            self.text_edit = None;
        }

        // Tick cursor blink; keep requesting frames while text is being edited.
        if self.app.editing_text.is_some() {
            self.app.tick_cursor();
            cx.notify();
        } else {
            self.app.cursor_blink_on = false;
            self.app.cursor_blink_tick = 0;
        }

        // Rasterize first so panels read fresh host state (preview size).
        let doc = self.doc_image();
        // Release the previous frame's canvas image GPU sprite-atlas tile when a
        // new `RenderImage` replaces it (the host re-rasterizes into a fresh
        // image with a new `id` on every dirty redraw — pan/zoom/edit/notify).
        // Skip the drop when the host returned the same cached image (id match),
        // so an unchanged redraw keeps its still-painted tile.
        if let Some(prev) = self.last_image.take() {
            if prev.id != doc.id {
                let _ = window.drop_image(prev);
            }
        }
        self.last_image = Some(doc.clone());
        let (dw, dh) = (self.app.host.doc_w as f32, self.app.host.doc_h as f32);
        let View { offset, scale } = self.app.view;
        // The preview image, scaled by zoom and placed at the view offset (both in
        // viewport-local window px).
        let (img_w, img_h) = (dw * scale, dh * scale);

        // Build panel elements (read-only &App + cx for Action listeners).
        let app = &self.app;
        let toolbar = panels::toolbar::render(app, cx);
        let tools = panels::tools::render(app, cx);
        let inspector = panels::inspector::render(app, cx);
        let character = panels::character::render(app, cx);
        let rename = self.rename.as_ref();
        let layers = panels::layers::render(app, rename, cx);
        let symbols = panels::symbols::render(app, rename, cx);
        let artboards_panel = panels::artboards::render(app, rename, cx);
        let trace_opt = panels::trace_dialog::render(app, cx);
        let recolor = panels::recolor::render(app, cx);
        // Selection ring overlay (viewport-local), mapped doc → viewport.
        let (ox, oy) = self.app.host.artboard_origin(&self.app.doc);
        let ring = |bounds: Option<[f32; 4]>, color: u32| {
            bounds.map(|[bx, by, bw, bh]| {
                let lx = offset.0 + (bx - ox) * scale;
                let ly = offset.1 + (by - oy) * scale;
                div()
                    .absolute()
                    .left(px(lx))
                    .top(px(ly))
                    .w(px((bw * scale).max(1.0)))
                    .h(px((bh * scale).max(1.0)))
                    .border_2()
                    .border_color(rgb(color))
            })
        };
        // Multi-selection: one thin ring per member, plus a brighter union ring
        // around the whole group. A single selection collapses to the usual ring.
        let member_rings: Vec<gpui::AnyElement> = if self.app.selection.len() > 1 {
            self.app
                .selected_member_bounds()
                .into_iter()
                .filter_map(|b| ring(Some(b), 0x7aa2f7).map(|d| d.border_1().into_any_element()))
                .collect()
        } else {
            Vec::new()
        };
        let group_ring = if self.app.selection.len() > 1 {
            ring(self.app.selection_bbox(), panels::ACCENT)
        } else {
            ring(self.app.selected_bounds(), panels::ACCENT)
        };
        // The boolean op's second operand gets a distinct (amber) ring (single-
        // pair Shift+click case; suppressed once a marquee group is shown).
        let secondary_ring = if self.app.selection.len() == 2 {
            ring(self.app.secondary_bounds(), 0xf59e0b)
        } else {
            None
        };

        // Transform scale handles: 8 squares around the selection bbox, shown
        // for the Select tool whenever at least one shape is selected.
        let xform_handles: Vec<gpui::AnyElement> =
            if matches!(self.app.active, Tool::Select) && !self.app.selection.is_empty() {
                if let Some(bbox) = self.app.selection_bbox() {
                    Handle::ALL
                        .iter()
                        .map(|h| {
                            let (hx, hy) = h.unit_pos();
                            let doc_x = bbox[0] + hx * bbox[2];
                            let doc_y = bbox[1] + hy * bbox[3];
                            let vx = offset.0 + (doc_x - ox) * scale;
                            let vy = offset.1 + (doc_y - oy) * scale;
                            div()
                                .absolute()
                                .left(px(vx - 4.0))
                                .top(px(vy - 4.0))
                                .w(px(8.0))
                                .h(px(8.0))
                                .bg(rgb(0xffffff))
                                .border_1()
                                .border_color(rgb(panels::ACCENT))
                                .into_any_element()
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

        // Live marquee rectangle preview (viewport-local).
        let marquee_preview = match self.drag.get() {
            Some(Drag::Marquee { a, b }) => self
                .window_to_viewport(a)
                .zip(self.window_to_viewport(b))
                .map(|((ax, ay), (bx, by))| {
                    let (lx, ly) = (ax.min(bx), ay.min(by));
                    let (w, h) = ((ax - bx).abs(), (ay - by).abs());
                    div()
                        .absolute()
                        .left(px(lx))
                        .top(px(ly))
                        .w(px(w))
                        .h(px(h))
                        .border_1()
                        .border_color(rgb(0x7aa2f7))
                        .bg(rgba(0x7aa2f722))
                }),
            _ => None,
        };

        // Live drag-create preview overlay (viewport-local). Drawn from the live
        // window-space corners so it tracks the cursor and any mid-drag pan/zoom.
        let create_preview = match self.drag.get() {
            Some(Drag::Create { a, b, .. }) => {
                self.window_to_viewport(a).zip(self.window_to_viewport(b)).map(
                    |((ax, ay), (bx, by))| {
                        let (lx, ly) = (ax.min(bx), ay.min(by));
                        let (w, h) = ((ax - bx).abs(), (ay - by).abs());
                        div()
                            .absolute()
                            .left(px(lx))
                            .top(px(ly))
                            .w(px(w))
                            .h(px(h))
                            .border_1()
                            .border_color(rgb(panels::ACCENT))
                            .bg(rgba(0x3b82f622))
                    },
                )
            }
            _ => None,
        };

        // Artboard overlays: white-filled rects with a label above each one
        // (viewport-local, mapped doc → viewport). Uses named artboard_entries
        // when present, falling back to legacy artboards for compat.
        let artboard_overlays: Vec<gpui::AnyElement> = if !self.app.artboard_entries.is_empty() {
            self.app
                .artboard_entries
                .iter()
                .map(|entry| {
                    let [bx, by, bw, bh] = entry.rect;
                    let lx = offset.0 + (bx - ox) * scale;
                    let ly = offset.1 + (by - oy) * scale;
                    let lw = (bw * scale).max(1.0);
                    let lh = (bh * scale).max(1.0);
                    let border_color = if self.app.active_artboard == Some(entry.id) {
                        rgb(0x7c5af5)
                    } else {
                        rgb(0x888888)
                    };
                    let name = entry.name.clone();
                    div()
                        .absolute()
                        .left(px(lx))
                        .top(px(ly - 16.0))
                        .w(px(lw))
                        .child(
                            div()
                                .text_color(rgb(0xaaaaaa))
                                .text_size(px(10.0))
                                .child(name),
                        )
                        .child(
                            div()
                                .w(px(lw))
                                .h(px(lh))
                                .bg(rgb(0xffffff))
                                .border_1()
                                .border_color(border_color),
                        )
                        .into_any_element()
                })
                .collect()
        } else {
            self.app
                .artboards
                .iter()
                .enumerate()
                .map(|(i, &[bx, by, bw, bh])| {
                    let lx = offset.0 + (bx - ox) * scale;
                    let ly = offset.1 + (by - oy) * scale;
                    let lw = (bw * scale).max(1.0);
                    let lh = (bh * scale).max(1.0);
                    div()
                        .absolute()
                        .left(px(lx))
                        .top(px(ly - 16.0))
                        .w(px(lw))
                        .child(
                            div()
                                .text_color(rgb(0xaaaaaa))
                                .text_size(px(10.0))
                                .child(format!("Artboard {}", i + 1)),
                        )
                        .child(
                            div()
                                .w(px(lw))
                                .h(px(lh))
                                .bg(rgb(0xffffff))
                                .border_1()
                                .border_color(rgb(0x888888)),
                        )
                        .into_any_element()
                })
                .collect()
        };

        // Artboard drag preview.
        let artboard_preview = match self.drag.get() {
            Some(Drag::ArtboardDrag { a, b }) => self
                .window_to_viewport(a)
                .zip(self.window_to_viewport(b))
                .map(|((ax, ay), (bx, by))| {
                    let (lx, ly) = (ax.min(bx), ay.min(by));
                    let (w, h) = ((ax - bx).abs(), (ay - by).abs());
                    div()
                        .absolute()
                        .left(px(lx))
                        .top(px(ly))
                        .w(px(w))
                        .h(px(h))
                        .border_1()
                        .border_color(rgb(0x888888))
                        .bg(rgba(0xffffff22))
                }),
            _ => None,
        };

        // Pen in-progress overlay: a dot at every placed anchor plus its tangent
        // knobs (viewport-local, mapped doc → viewport so it tracks pan/zoom).
        let mut pen_overlay: Vec<gpui::AnyElement> = Vec::new();
        for (i, &p) in self.app.pen.points.iter().enumerate() {
            let (vx, vy) = self.doc_to_viewport(p, ox, oy);
            // Anchor: filled square; the first anchor is highlighted (close target).
            let first = i == 0;
            pen_overlay.push(dot(vx, vy, 7.0, panels::ACCENT, first).into_any_element());
            let h = self.app.pen.handles.get(i).copied().unwrap_or((0.0, 0.0));
            if h.0 != 0.0 || h.1 != 0.0 {
                let (ox2, oy2) = self.doc_to_viewport((p.0 + h.0, p.1 + h.1), ox, oy);
                let (ix2, iy2) = self.doc_to_viewport((p.0 - h.0, p.1 - h.1), ox, oy);
                pen_overlay.push(dot(ox2, oy2, 5.0, 0x9ac7ff, false).into_any_element());
                pen_overlay.push(dot(ix2, iy2, 5.0, 0x9ac7ff, false).into_any_element());
            }
        }

        // Node-edit (Direct-Select) overlay: anchors + tangent knobs of the
        // selected path, so the user can see what is grabbable.
        let mut node_overlay: Vec<gpui::AnyElement> = Vec::new();
        if self.app.active == Tool::DirectSelect {
            if let Some((points, handles)) = self.app.selected_path_nodes() {
                for (i, &p) in points.iter().enumerate() {
                    let (vx, vy) = self.doc_to_viewport(p, ox, oy);
                    let h = handles.get(i).copied().unwrap_or((0.0, 0.0));
                    if h.0 != 0.0 || h.1 != 0.0 {
                        let (ox2, oy2) = self.doc_to_viewport((p.0 + h.0, p.1 + h.1), ox, oy);
                        let (ix2, iy2) = self.doc_to_viewport((p.0 - h.0, p.1 - h.1), ox, oy);
                        node_overlay.push(dot(ox2, oy2, 5.0, 0x9ac7ff, false).into_any_element());
                        node_overlay.push(dot(ix2, iy2, 5.0, 0x9ac7ff, false).into_any_element());
                    }
                    node_overlay.push(dot(vx, vy, 7.0, panels::ACCENT, false).into_any_element());
                }
            }
        }

        // Perspective grid overlay: two vanishing-point rays + horizon line.
        let mut perspective_overlay: Vec<gpui::AnyElement> = Vec::new();
        if let Some(ref grid) = self.app.perspective_grid {
            if grid.visible {
                let (vp1x, vp1y) = self.doc_to_viewport(grid.vp1, ox, oy);
                let (vp2x, vp2y) = self.doc_to_viewport(grid.vp2, ox, oy);
                let hy = self.doc_to_viewport((0.0, grid.horizon_y), ox, oy).1;
                // Horizon line
                perspective_overlay.push(
                    div().absolute().left(px(0.0)).top(px(hy)).w_full().h(px(1.0))
                        .bg(rgba(0xffffff44))
                        .into_any_element(),
                );
                // VP1 dot
                perspective_overlay.push(
                    dot(vp1x, vp1y, 9.0, 0x44aaff, true).into_any_element(),
                );
                // VP2 dot
                perspective_overlay.push(
                    dot(vp2x, vp2y, 9.0, 0xff8844, true).into_any_element(),
                );
            }
        }

        // Knife drag preview: a thin line from a to b (viewport-local).
        let knife_preview = match self.drag.get() {
            Some(Drag::KnifeDrag { a, b }) => {
                self.window_to_viewport(a).zip(self.window_to_viewport(b)).map(
                    |((ax, ay), (bx, by))| {
                        let min_x = ax.min(bx);
                        let min_y = ay.min(by);
                        let w = (ax - bx).abs().max(1.0);
                        let h = (ay - by).abs().max(1.0);
                        div()
                            .absolute()
                            .left(px(min_x))
                            .top(px(min_y))
                            .w(px(w))
                            .h(px(h))
                            .border_1()
                            .border_color(rgb(0xff6644))
                    },
                )
            }
            _ => None,
        };

        // Isolation mode: semi-transparent overlay to indicate isolation is active.
        let isolation_overlay = if self.app.isolation_group.is_some() {
            Some(
                div()
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000033)),
            )
        } else {
            None
        };

        // Blinking text cursor: visible when editing_text is Some and blink is on.
        // origin.y is the TOP of the text bounding box in document space.
        // Cursor height = font_size * zoom so it spans the full glyph height.
        let text_cursor = if self.app.cursor_blink_on {
            self.app.editing_text.and_then(|idx| {
                use crate::document::Shape;
                let zoom = self.app.view.scale;
                self.app.doc.shapes.get(idx).and_then(|s| {
                    if let Shape::Text { params, origin, .. } = s {
                        let text_len = params.text.len() as f32;
                        // Approximate char width at this zoom; 8.0 is a rough em-width
                        // for the default font at font_size=24.
                        let char_w = (params.font_size / 24.0) * 8.0 * zoom;
                        let doc_x = origin.0 + text_len * (params.font_size / 24.0) * 8.0;
                        let doc_y = origin.1;
                        let (vx, vy) = self.doc_to_viewport((doc_x, doc_y), ox, oy);
                        let cursor_h = (params.font_size * zoom).max(12.0);
                        let _ = char_w; // used for future precise placement
                        Some(
                            div()
                                .absolute()
                                .left(px(vx))
                                .top(px(vy))
                                .w(px(1.5))
                                .h(px(cursor_h))
                                .bg(rgb(0xffffff)),
                        )
                    } else {
                        None
                    }
                })
            })
        } else {
            None
        };

        // Inline text-content editor: a focused TextField floating just above the
        // text object's origin while a Type-tool edit session is open. Typing into
        // it dispatches `SetTextObjectContent`, re-shaping the on-canvas glyphs.
        let text_edit_overlay = self.text_edit().map(|te| {
            use crate::document::Shape;
            let (mut vx, mut vy) = (offset.0 + 8.0, offset.1 + 8.0);
            if let Some(Shape::Text { origin, .. }) = self.app.doc.shapes.get(te.idx) {
                let (px_, py_) = self.doc_to_viewport(*origin, ox, oy);
                vx = px_;
                vy = (py_ - 34.0).max(4.0);
            }
            div()
                .absolute()
                .left(px(vx))
                .top(px(vy))
                .child(te.field.clone())
        });

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| {
                this.on_key_down(ev, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(ui_colors::surface_bg())
            .text_color(ui_colors::text_primary())
            .font_family(".SystemUIFont")
            // Top toolbar (full width).
            .child(
                div()
                    .w_full()
                    .h(px(TOOLBAR_H))
                    .bg(ui_colors::surface_bg())
                    .border_b_1()
                    .border_color(ui_colors::surface_border())
                    .child(toolbar),
            )
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
                            .w(px(STRIP_W))
                            .h_full()
                            .bg(ui_colors::surface_bg())
                            .border_r_1()
                            .border_color(ui_colors::surface_border())
                            .child(tools),
                    )
                    // Center canvas viewport (artboard floating on a dark field).
                    // This stateful, clipped div owns the canvas mouse + scroll
                    // input and hosts the (pan/zoom-transformed) preview image plus
                    // the selection-ring and live-drag overlays. An overlaid
                    // `canvas()` captures the viewport's painted rect each frame so
                    // the handlers have a live window↔doc map.
                    .child(
                        div()
                            .id("canvas")
                            .flex_1()
                            .h_full()
                            .relative()
                            .overflow_hidden()
                            .bg(ui_colors::surface_overlay())
                            .child(
                                canvas(
                                    {
                                        let cell = self.viewport_bounds.clone();
                                        move |bounds, _win, _cx| cell.set(Some(bounds))
                                    },
                                    |_bounds, _state, _win, _cx| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            // The bridged preview, scaled + offset by the view.
                            .child(
                                img(doc)
                                    .absolute()
                                    .left(px(offset.0))
                                    .top(px(offset.1))
                                    .w(px(img_w))
                                    .h(px(img_h)),
                            )
                            .children(artboard_overlays)
                            .children(artboard_preview)
                            .children(member_rings)
                            .children(group_ring)
                            .children(secondary_ring)
                            .children(xform_handles)
                            .children(create_preview)
                            .children(marquee_preview)
                            .children(node_overlay)
                            .children(pen_overlay)
                            .children(perspective_overlay)
                            .children(knife_preview)
                            .children(isolation_overlay)
                            .children(text_cursor)
                            .children(text_edit_overlay)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, ev: &gpui::MouseDownEvent, win, cx| {
                                    this.on_canvas_down(ev, win, cx);
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Middle,
                                cx.listener(|this, ev: &gpui::MouseDownEvent, win, cx| {
                                    this.on_canvas_down(ev, win, cx);
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                |this, ev: &gpui::MouseMoveEvent, _win, cx| {
                                    this.on_canvas_move(ev, cx);
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, cx| {
                                    this.on_canvas_up(cx);
                                }),
                            )
                            .on_mouse_up(
                                MouseButton::Middle,
                                cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, cx| {
                                    this.on_canvas_up(cx);
                                }),
                            )
                            // Release-outside: a drag that ends off the viewport
                            // still commits / cancels cleanly.
                            .on_mouse_up_out(
                                MouseButton::Left,
                                cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, cx| {
                                    this.on_canvas_up(cx);
                                }),
                            )
                            .on_scroll_wheel(cx.listener(
                                |this, ev: &gpui::ScrollWheelEvent, _win, cx| {
                                    this.on_canvas_scroll(ev, cx);
                                },
                            )),
                    )
                    // Right dock: inspector → character → layers → symbols → artboards → trace → recolor.
                    .child(
                        div()
                            .id("right-dock")
                            .flex_shrink_0()
                            .w(px(DOCK_W))
                            .h_full()
                            .bg(ui_colors::surface_bg())
                            .border_l_1()
                            .border_color(ui_colors::surface_border())
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(inspector)
                            .child(character)
                            .child(layers)
                            .child(symbols)
                            .child(artboards_panel)
                            .children(trace_opt)
                            .child(recolor),
                    ),
            )
    }
}

impl Focusable for Contour {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// A small centered overlay marker (anchor / handle knob) at the viewport-local
/// point `(cx, cy)`, `size` window px across, in `color`. `emphasize` draws a
/// thicker ring (used for the pen path's first anchor — the close target).
fn dot(cx: f32, cy: f32, size: f32, color: u32, emphasize: bool) -> impl IntoElement {
    let half = size * 0.5;
    let base = div()
        .absolute()
        .left(px(cx - half))
        .top(px(cy - half))
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .bg(ui_colors::surface_bg())
        .border_color(rgb(color));
    if emphasize {
        base.border_2()
    } else {
        base.border_1()
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    gpui::Application::new().with_assets(PrismAssets).run(|cx: &mut gpui::App| {
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
                    // Focus the root so the canvas receives keyboard events
                    // (Cmd+Z / Cmd+Shift+Z, Delete, pen Enter / Escape) immediately.
                    window.focus(&focus);
                    Contour {
                        app: App::new(),
                        viewport_bounds: Rc::new(Cell::new(None)),
                        drag: Rc::new(Cell::new(None)),
                        focus,
                        last_image: None,
                        rename: None,
                        text_edit: None,
                    }
                })
            },
        )
        .expect("failed to open window");

        let weak_contour = main_handle
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
                cx.new(|_cx| welcome::WelcomeView::new(focus, weak_contour))
            },
        )
        .expect("failed to open Contour welcome window");

        cx.activate(true);
    });
}
