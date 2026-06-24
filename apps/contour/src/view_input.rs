//! `impl Contour` input handling, coordinate mapping and child-window openers.
//! Split out of `main.rs` (mechanical extraction). Methods are `pub(crate)` so
//! the `Render` impl (in `main.rs`) can wire them as event listeners.

use std::sync::Arc;

use gpui::{
    px, AppContext, Bounds, Context, KeyDownEvent, MouseButton, Pixels, Point,
    RenderImage, WindowBounds, WindowKind, WindowOptions, size,
};

use crate::app_state::{Action, NodeTarget, Tool, View};
use crate::transform::Handle;
use crate::{Contour, Drag};

impl Contour {
    /// The bridged document image (re-rasterizes only when the host is dirty).
    pub(crate) fn doc_image(&mut self) -> Arc<RenderImage> {
        // Split borrow: clone mesh_points so the host can take &doc immutably.
        let mesh_points = self.app.mesh_points.clone();
        let doc = &self.app.doc;
        self.app.host.image(doc, &mesh_points, None)
    }

    /// The canvas viewport rect (window space), or `None` before the first paint.
    pub(crate) fn viewport(&self) -> Option<Bounds<Pixels>> {
        self.viewport_bounds.get()
    }

    /// Map a window-space position to a *document-space* point through the live
    /// view transform. Inverts
    /// `win = viewport_origin + offset + (doc - artboard_origin) * scale`:
    /// `doc = artboard_origin + (win - viewport_origin - offset) / scale`.
    /// Returns `None` before the viewport has painted.
    pub(crate) fn window_to_doc(&self, pos: Point<Pixels>) -> Option<[f32; 2]> {
        let vp = self.viewport()?;
        let (vx, vy) = (f32::from(vp.origin.x), f32::from(vp.origin.y));
        let View { offset, scale } = self.app.view;
        if scale <= 0.0 {
            return None;
        }
        let (ox, oy) = self.app.host.artboard_origin(&self.app.doc);
        let dx = (f32::from(pos.x) - vx - offset.0) / scale;
        let dy = (f32::from(pos.y) - vy - offset.1) / scale;
        Some([ox + dx, oy + dy])
    }

    /// Map a window-space position to a **viewport-local** point (window px,
    /// relative to the viewport origin). Used for zoom anchors and overlay
    /// placement. Returns `None` before the viewport has painted.
    pub(crate) fn window_to_viewport(&self, pos: Point<Pixels>) -> Option<(f32, f32)> {
        let vp = self.viewport()?;
        Some((
            f32::from(pos.x) - f32::from(vp.origin.x),
            f32::from(pos.y) - f32::from(vp.origin.y),
        ))
    }

    /// Map a *document-space* point to a **viewport-local** point (window px,
    /// relative to the viewport origin) through the live view transform — the
    /// inverse of [`window_to_doc`](Self::window_to_doc). Used to place the pen
    /// preview and node-edit handle overlays. `(ox, oy)` is the artboard origin.
    pub(crate) fn doc_to_viewport(&self, p: (f32, f32), ox: f32, oy: f32) -> (f32, f32) {
        let View { offset, scale } = self.app.view;
        (offset.0 + (p.0 - ox) * scale, offset.1 + (p.1 - oy) * scale)
    }

    /// The viewport's painted size (window px), or `(0,0)` before first paint.
    pub(crate) fn viewport_size(&self) -> (f32, f32) {
        self.viewport()
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((0.0, 0.0))
    }

    /// Return the handle index (into `Handle::ALL`) if `pos` (window space) is
    /// within the 8×8 px hit radius of any scale handle of the current selection
    /// bounding box. Returns `None` when there is no selection or the cursor isn't
    /// near any handle. Only active for the Select tool.
    pub(crate) fn hit_transform_handle(&self, pos: Point<Pixels>) -> Option<u8> {
        if self.app.selection.is_empty() {
            return None;
        }
        let bbox = self.app.selection_bbox()?;
        let (ox, oy) = self.app.host.artboard_origin(&self.app.doc);
        const RADIUS: f32 = 6.0;
        for (i, handle) in Handle::ALL.iter().enumerate() {
            let (hx, hy) = handle.unit_pos();
            let doc_x = bbox[0] + hx * bbox[2];
            let doc_y = bbox[1] + hy * bbox[3];
            let (vx, vy) = self.doc_to_viewport((doc_x, doc_y), ox, oy);
            let vp_orig = self.viewport().map(|b| (f32::from(b.origin.x), f32::from(b.origin.y))).unwrap_or((0.0, 0.0));
            let wx = vp_orig.0 + vx;
            let wy = vp_orig.1 + vy;
            let dx = f32::from(pos.x) - wx;
            let dy = f32::from(pos.y) - wy;
            if dx * dx + dy * dy <= RADIUS * RADIUS {
                return Some(i as u8);
            }
        }
        None
    }

    /// Begin a canvas gesture on left/middle press: drag-create for the Rect /
    /// Ellipse tools, otherwise pan (Alt+left or middle) or click-select.
    pub(crate) fn on_canvas_down(
        &mut self,
        ev: &gpui::MouseDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let middle = ev.button == MouseButton::Middle;
        let pan_modifier = ev.modifiers.alt || ev.modifiers.platform;

        // A canvas press outside the Type tool commits any open text-content edit.
        if self.text_edit().is_some() && self.app.active != Tool::Type {
            self.end_text_edit(cx);
        }

        // Pan takes precedence (middle-drag always pans; Alt+left pans).
        if middle || (ev.button == MouseButton::Left && pan_modifier) {
            self.drag.set(Some(Drag::Pan { last: ev.position }));
            return;
        }

        if ev.button != MouseButton::Left {
            return;
        }

        match self.app.active {
            Tool::Rect | Tool::Ellipse | Tool::Line | Tool::Polygon | Tool::Star => {
                self.drag.set(Some(Drag::Create {
                    tool: self.app.active,
                    a: ev.position,
                    b: ev.position,
                }));
            }
            Tool::Type => {
                // Click an existing text object to re-edit it, else place a new one.
                // Either way, open a focused TextField on the resulting object so
                // the user types the actual string (re-shaping the glyphs live).
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    // Commit any prior edit first so its object isn't left dangling.
                    if self.text_edit().is_some() {
                        self.end_text_edit(cx);
                    }
                    if let Some(i) = self.app.hit_text(x, y) {
                        self.app.apply(Action::EditText(i));
                    } else {
                        self.app.apply(Action::PlaceText { x, y });
                    }
                    if let Some(idx) = self.app.editing_text {
                        let current = self
                            .app
                            .doc
                            .shapes
                            .get(idx)
                            .and_then(|s| s.text_params())
                            .map(|p| p.text.clone())
                            .unwrap_or_default();
                        self.begin_text_edit(idx, current, window, cx);
                    }
                    cx.notify();
                }
            }
            Tool::Pen => {
                // Place an anchor (or close + commit if it lands on the first
                // anchor), then arm a handle-drag on the freshest anchor so a
                // press-drag sets its out-tangent.
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    let pen_len = self.app.pen.points.len();
                    self.app.apply(Action::PenAddAnchor { x, y });
                    // If the path didn't close (it grew by one anchor), arm the
                    // handle drag; a close drains the pen and shrinks it instead.
                    if self.app.pen.points.len() > pen_len {
                        self.drag.set(Some(Drag::PenHandle));
                    }
                    cx.notify();
                }
            }
            Tool::DirectSelect => {
                // Grab an anchor / handle of the selected path, else re-pick the
                // path (or shape) under the cursor like the Select tool.
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    if let Some(target) = self.app.hit_node(x, y) {
                        self.app.apply(Action::BeginInteraction);
                        self.drag.set(Some(Drag::Node { target }));
                    } else {
                        self.app.apply(Action::HitTestSelect { x, y });
                    }
                    cx.notify();
                }
            }
            Tool::Artboard => {
                // Start an artboard drag — like a shape-create drag but commits
                // via `Action::AddArtboard` on release.
                self.drag.set(Some(Drag::ArtboardDrag {
                    a: ev.position,
                    b: ev.position,
                }));
            }
            Tool::Knife => {
                self.drag.set(Some(Drag::KnifeDrag {
                    a: ev.position,
                    b: ev.position,
                }));
            }
            Tool::Width => {
                let start_mul = self.app.selected
                    .and_then(|i| self.app.doc.shapes.get(i))
                    .map(|s| s.stroke_style().width_profile.0)
                    .unwrap_or(1.0);
                self.drag.set(Some(Drag::WidthDrag {
                    start_x: f32::from(ev.position.x),
                    start_mul,
                }));
            }
            Tool::Blend => {
                // On click, select the clicked shape as blend operand.
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    self.app.apply(Action::HitTestSelect { x, y });
                    cx.notify();
                }
            }
            Tool::Eyedropper => {
                // Sample the canvas pixel under the click and pick its colour.
                if let Some([dx, dy]) = self.window_to_doc(ev.position) {
                    let (ox, oy) = self.app.host.artboard_origin(&self.app.doc);
                    let px = (dx - ox).max(0.0) as u32;
                    let py = (dy - oy).max(0.0) as u32;
                    let rgba = self.app.host.sample_pixel(px, py);
                    self.app.apply(Action::PickColor(rgba));
                    cx.notify();
                }
            }
            Tool::ShapeBuilder => {
                // Each click adds the shape under the cursor to the accumulating
                // selection. On mouse-up (on_canvas_up) the builder op is applied.
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    self.app.apply(Action::AddToSelection { x, y });
                    self.drag.set(Some(Drag::ShapeBuilderDrag {
                        a: ev.position,
                        b: ev.position,
                        subtract: ev.modifiers.alt,
                    }));
                    cx.notify();
                }
            }
            _ => {
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    // Shift+click toggles a shape in/out of the multi-selection.
                    if ev.modifiers.shift {
                        self.app.apply(Action::AddToSelection { x, y });
                    } else if let Some(h_idx) = self.hit_transform_handle(ev.position) {
                        // Clicked a scale/transform handle — begin handle drag.
                        if let Some(bbox) = self.app.selection_bbox() {
                            self.app.apply(Action::BeginTransformScale {
                                handle_idx: h_idx,
                                doc: [x, y],
                                bbox,
                            });
                            self.drag.set(Some(Drag::TransformScale {
                                handle_idx: h_idx,
                                last: ev.position,
                            }));
                        }
                    } else if self.app.hit_selected(x, y) {
                        // Click on an already-selected shape → begin move drag.
                        self.drag.set(Some(Drag::MoveSelected { last: ev.position }));
                    } else if self.app.hit_at(x, y).is_some() {
                        // Plain click on unselected shape → (re)pick it.
                        self.app.apply(Action::HitTestSelect { x, y });
                    } else {
                        // Press on empty canvas → marquee (clears selection on click).
                        self.drag.set(Some(Drag::Marquee {
                            a: ev.position,
                            b: ev.position,
                        }));
                    }
                    cx.notify();
                }
            }
        }
    }

    /// Continue the active gesture while a button is held.
    pub(crate) fn on_canvas_move(&mut self, ev: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        match self.drag.get() {
            Some(Drag::Create { tool, a, .. }) if ev.dragging() => {
                self.drag.set(Some(Drag::Create {
                    tool,
                    a,
                    b: ev.position,
                }));
                cx.notify();
            }
            Some(Drag::Pan { last }) if ev.dragging() => {
                let dx = f32::from(ev.position.x) - f32::from(last.x);
                let dy = f32::from(ev.position.y) - f32::from(last.y);
                self.app.apply(Action::PanBy { dx, dy });
                self.drag.set(Some(Drag::Pan { last: ev.position }));
                cx.notify();
            }
            Some(Drag::PenHandle) if ev.dragging() => {
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    self.app.apply(Action::PenSetHandle { x, y });
                    cx.notify();
                }
            }
            Some(Drag::Node { target }) if ev.dragging() => {
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    match target {
                        NodeTarget::Anchor(which) => {
                            self.app.apply(Action::MoveAnchor { which, x, y })
                        }
                        NodeTarget::Handle(which) => {
                            self.app.apply(Action::MoveHandle { which, x, y })
                        }
                    }
                    cx.notify();
                }
            }
            Some(Drag::Marquee { a, .. }) if ev.dragging() => {
                self.drag.set(Some(Drag::Marquee { a, b: ev.position }));
                cx.notify();
            }
            Some(Drag::ArtboardDrag { a, .. }) if ev.dragging() => {
                self.drag.set(Some(Drag::ArtboardDrag { a, b: ev.position }));
                cx.notify();
            }
            Some(Drag::KnifeDrag { a, .. }) if ev.dragging() => {
                self.drag.set(Some(Drag::KnifeDrag { a, b: ev.position }));
                cx.notify();
            }
            Some(Drag::TransformScale { handle_idx, .. }) if ev.dragging() => {
                if let Some([x, y]) = self.window_to_doc(ev.position) {
                    self.app.apply(Action::TransformScaleDrag {
                        doc: [x, y],
                        uniform: ev.modifiers.shift,
                    });
                    self.drag.set(Some(Drag::TransformScale {
                        handle_idx,
                        last: ev.position,
                    }));
                    cx.notify();
                }
            }
            Some(Drag::MoveSelected { last }) if ev.dragging() => {
                let dx = f32::from(ev.position.x) - f32::from(last.x);
                let dy = f32::from(ev.position.y) - f32::from(last.y);
                let scale = self.app.view.scale;
                if dx.abs() > 0.5 || dy.abs() > 0.5 {
                    self.app.apply(Action::MoveSelection {
                        dx: dx / scale,
                        dy: dy / scale,
                    });
                    self.drag.set(Some(Drag::MoveSelected { last: ev.position }));
                    cx.notify();
                }
            }
            Some(Drag::WidthDrag { start_x, start_mul }) if ev.dragging() => {
                // Horizontal drag maps ±200px → ±1× width multiplier.
                let delta = (f32::from(ev.position.x) - start_x) / 200.0;
                let end_mul = (start_mul + delta).max(0.05);
                self.app.apply(Action::SetWidthProfile { start: start_mul, end: end_mul });
                self.drag.set(Some(Drag::WidthDrag { start_x, start_mul }));
                cx.notify();
            }
            Some(Drag::ShapeBuilderDrag { a, subtract, .. }) if ev.dragging() => {
                self.drag.set(Some(Drag::ShapeBuilderDrag {
                    a,
                    b: ev.position,
                    subtract,
                }));
                cx.notify();
            }
            _ => {}
        }
    }

    /// Finish the active gesture on release: commit a drag-created shape, or end
    /// the pan.
    pub(crate) fn on_canvas_up(&mut self, cx: &mut Context<Self>) {
        match self.drag.take() {
            Some(Drag::Create { tool, a, b }) => {
                if let (Some(da), Some(db)) = (self.window_to_doc(a), self.window_to_doc(b)) {
                    self.app.apply(Action::CreateShape {
                        tool,
                        a: da,
                        b: db,
                    });
                    cx.notify();
                }
            }
            Some(Drag::Pan { .. }) => cx.notify(),
            // Pen handle-drag just ends; the anchor and its handle stay and the
            // path keeps accepting more anchors (finished via Enter / double-
            // click / clicking the first anchor).
            Some(Drag::PenHandle) => cx.notify(),
            // A node anchor/handle drag finalizes its coalesced undo entry (a
            // no-op drag is dropped by History::commit).
            Some(Drag::Node { .. }) => {
                self.app.apply(Action::EndInteraction);
                cx.notify();
            }
            // Marquee release: select every shape intersecting the dragged rect.
            // A degenerate (no-drag) marquee clears the selection like a plain
            // empty-canvas click.
            Some(Drag::Marquee { a, b }) => {
                if let (Some(da), Some(db)) = (self.window_to_doc(a), self.window_to_doc(b)) {
                    let x = da[0].min(db[0]);
                    let y = da[1].min(db[1]);
                    let w = (da[0] - db[0]).abs();
                    let h = (da[1] - db[1]).abs();
                    if w < 2.0 && h < 2.0 {
                        self.app.apply(Action::HitTestSelect { x: da[0], y: da[1] });
                    } else {
                        self.app.apply(Action::MarqueeSelect {
                            rect: [x, y, w, h],
                        });
                    }
                    cx.notify();
                }
            }
            // Artboard release: commit the dragged rect as a new artboard.
            // Degenerate (< 2 doc units) drags are dropped.
            Some(Drag::ArtboardDrag { a, b }) => {
                if let (Some(da), Some(db)) = (self.window_to_doc(a), self.window_to_doc(b)) {
                    let x = da[0].min(db[0]);
                    let y = da[1].min(db[1]);
                    let w = (da[0] - db[0]).abs();
                    let h = (da[1] - db[1]).abs();
                    if w >= 2.0 || h >= 2.0 {
                        self.app.apply(Action::AddArtboard([x, y, w, h]));
                        cx.notify();
                    }
                }
            }
            // Knife release: commit the slice.
            Some(Drag::KnifeDrag { a, b }) => {
                if let (Some(da), Some(db)) = (self.window_to_doc(a), self.window_to_doc(b)) {
                    let dx = da[0] - db[0];
                    let dy = da[1] - db[1];
                    if dx.abs() >= 1.0 || dy.abs() >= 1.0 {
                        self.app.apply(Action::KnifeSlice {
                            start: (da[0], da[1]),
                            end: (db[0], db[1]),
                        });
                        cx.notify();
                    }
                }
            }
            // Transform scale release: drag steps committed live; just clear snapshot.
            Some(Drag::TransformScale { .. }) => {
                self.app.xform_snapshot.clear();
                self.app.xform_handle = None;
                cx.notify();
            }
            // Move-selected release: MoveSelection actions already checkpointed.
            Some(Drag::MoveSelected { .. }) => {
                cx.notify();
            }
            // Width-tool drag release: profile already applied live.
            Some(Drag::WidthDrag { .. }) => {
                cx.notify();
            }
            // Shape builder release: apply the geometry op if ≥2 shapes selected.
            Some(Drag::ShapeBuilderDrag { subtract, .. }) => {
                if self.app.selection.len() >= 2 {
                    self.app.apply(Action::ApplyShapeBuilder { subtract });
                }
                cx.notify();
            }
            None => {}
        }
    }

    /// Root-view keyboard handling: undo / redo, delete the selection, and the
    /// pen finish / cancel keys. Returns whether the key was consumed (so the
    /// caller can `cx.notify()`).
    pub(crate) fn on_key_down(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        let cmd = m.platform || m.control;
        let key = ks.key.as_str();

        // When a focused content `TextField` is open (Type tool), it owns text
        // input via its own key handler and dispatches `SetTextObjectContent`.
        // The root only handles Escape to commit + close the session.
        if self.text_edit().is_some() {
            if key == "escape" && !cmd {
                self.end_text_edit(cx);
            }
            return;
        }

        // Legacy text-edit mode (no focused field): keystrokes feed the point-type
        // object's string — printable keys append, Backspace deletes, Enter inserts
        // a newline, and Escape finishes. Cmd-chords (undo/redo) pass through.
        if self.app.editing_text.is_some() && !cmd {
            match key {
                "escape" => self.app.apply(Action::FinishText),
                "backspace" | "delete" => self.app.apply(Action::TypeBackspace),
                "enter" => self.app.apply(Action::TypeChar('\n')),
                _ => {
                    // Insert the produced character(s) for a printable key.
                    if let Some(s) = ks.key_char.as_ref().filter(|s| {
                        !s.is_empty() && s.chars().all(|c| !c.is_control())
                    }) {
                        for c in s.chars() {
                            self.app.apply(Action::TypeChar(c));
                        }
                    } else {
                        return;
                    }
                }
            }
            cx.notify();
            return;
        }

        match key {
            // Cmd+Shift+Z → redo; Cmd+Z → undo.
            "z" if cmd && m.shift => self.app.apply(Action::Redo),
            "z" if cmd => self.app.apply(Action::Undo),
            // Cmd+Y → redo (Windows-style alias).
            "y" if cmd => self.app.apply(Action::Redo),
            // Cmd+Shift+G → ungroup; Cmd+G → group.
            "g" if cmd && m.shift => self.app.apply(Action::UngroupSelected),
            "g" if cmd => self.app.apply(Action::GroupSelected),
            // Cmd+[ → send backward; Cmd+] → bring forward.
            "[" if cmd => {
                if let Some(id) = self.app.selected {
                    self.app.apply(Action::MoveLayerOrder { id, delta: -1 });
                }
            }
            "]" if cmd => {
                if let Some(id) = self.app.selected {
                    self.app.apply(Action::MoveLayerOrder { id, delta: 1 });
                }
            }
            // Finish an in-progress pen path: Enter leaves it open; double-click /
            // first-anchor click close it.
            "enter" if self.app.active == Tool::Pen && !self.app.pen.is_empty() => {
                self.app.apply(Action::PenFinish { closed: false });
            }
            // Escape abandons an in-progress pen path.
            "escape" if !self.app.pen.is_empty() => self.app.apply(Action::PenCancel),
            // Escape exits isolation mode (Wave 11).
            "escape" if self.app.isolation_group.is_some() => self.app.apply(Action::ExitIsolation),
            // Delete / Backspace removes the selected shape (Select / node tools).
            "delete" | "backspace" if self.app.selected.is_some() => {
                self.app.apply(Action::DeleteSelected)
            }
            // Tool shortcuts (no modifier).
            "v" if !cmd && !m.shift => self.app.apply(Action::SetTool(Tool::Select)),
            "a" if !cmd && !m.shift => self.app.apply(Action::SetTool(Tool::DirectSelect)),
            "p" if !cmd && !m.shift => self.app.apply(Action::SetTool(Tool::Pen)),
            "e" if !cmd && !m.shift => self.app.apply(Action::SetTool(Tool::Ellipse)),
            "r" if !cmd && !m.shift => self.app.apply(Action::SetTool(Tool::Rect)),
            "t" if !cmd && !m.shift => self.app.apply(Action::SetTool(Tool::Type)),
            "i" if !cmd && !m.shift => {
                if self.app.active != Tool::Eyedropper {
                    self.app.prev_tool = self.app.active;
                }
                self.app.apply(Action::SetTool(Tool::Eyedropper));
            }
            _ => return,
        }
        cx.notify();
    }

    /// Zoom on scroll, holding the point under the cursor fixed.
    pub(crate) fn on_canvas_scroll(&mut self, ev: &gpui::ScrollWheelEvent, cx: &mut Context<Self>) {
        // One "line" / ~20px of vertical scroll → ~10% zoom. `pixel_delta` folds
        // trackpad (pixel) and wheel (line) deltas into a common pixel unit.
        let dy = f32::from(ev.delta.pixel_delta(px(20.0)).y);
        if dy == 0.0 {
            return;
        }
        let factor = (1.0_f32 + dy / 200.0).clamp(0.5, 2.0);
        let anchor = self.window_to_viewport(ev.position);
        let viewport = self.viewport_size();
        self.app.apply(Action::ZoomBy {
            factor,
            anchor,
            viewport,
        });
        cx.notify();
    }

    /// Open the floating **Document Setup** window. It receives a
    /// `WeakEntity<Contour>` so its controls dispatch Actions into this view and
    /// read `app.doc_setup` back. `WindowKind::Floating` per CLAUDE.md.
    pub(crate) fn open_document_setup(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let bounds = Bounds::centered(None, size(px(520.0), px(400.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus = cx.focus_handle();
                win.focus(&focus);
                cx.new(|cx| crate::document_setup_window::DocumentSetupView::new(focus, weak, cx))
            },
        );
    }

    /// Open the floating **Export** window (format picker + destination).
    pub(crate) fn open_export(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let bounds = Bounds::centered(None, size(px(640.0), px(480.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus = cx.focus_handle();
                win.focus(&focus);
                cx.new(|cx| crate::export_window::ExportView::new(focus, weak, cx))
            },
        );
    }

    /// Open the floating **Color Picker** window (HSB/RGB/CMYK/Hex).
    pub(crate) fn open_color_picker(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let bounds = Bounds::centered(None, size(px(280.0), px(360.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus = cx.focus_handle();
                win.focus(&focus);
                cx.new(|cx| crate::color_picker_window::ColorPickerView::new(focus, weak, cx))
            },
        );
    }

    /// Open the floating **Preferences** window (undo / snap / grid / unit).
    pub(crate) fn open_preferences(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let bounds = Bounds::centered(None, size(px(720.0), px(560.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                ..Default::default()
            },
            |win, cx| {
                let focus = cx.focus_handle();
                win.focus(&focus);
                cx.new(|_cx| crate::preferences_window::PreferencesView::new(focus, weak))
            },
        );
    }
}

