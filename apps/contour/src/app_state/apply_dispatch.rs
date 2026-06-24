//! The first stage of the `Action` dispatch chain: the core inline arms.
//!
//! Split out of `mod.rs` (mechanical extraction). `App::apply` delegates here;
//! arms not handled fall through to `apply_batch10` and the rest of the chain.

use super::*;

impl App {
    /// Core inline `apply` arms. Called by [`App::apply`]; unmatched actions
    /// fall through to [`App::apply_batch10`] and the remaining chain.
    pub(super) fn apply_inline_core(&mut self, action: Action) {
        match action {
            Action::SetTool(t) => {
                // Leaving the Pen tool abandons any half-drawn path so stale
                // anchors never linger on another tool (mirrors the egui app).
                if t != Tool::Pen && !self.pen.is_empty() {
                    self.pen.clear();
                    self.host.mark_dirty();
                }
                // Leaving the Type tool commits / discards any in-progress edit.
                if t != Tool::Type && self.editing_text.is_some() {
                    self.finish_text_edit();
                }
                // Remember the pre-eyedropper tool so PickColor can revert to it.
                if t == Tool::Eyedropper && self.active != Tool::Eyedropper {
                    self.prev_tool = self.active;
                }
                self.active = t;
            }

            Action::ToggleShapeVisible(i) => {
                if i < self.doc.shapes.len() {
                    self.checkpoint();
                    self.doc.shapes[i].toggle_visible();
                    self.host.mark_dirty();
                }
            }
            Action::SelectShape(i) => {
                if i < self.doc.shapes.len() {
                    self.select_single(i);
                }
            }

            Action::HitTestSelect { x, y } => {
                match self.hit_test_topmost(x, y) {
                    Some(i) => self.select_single(i),
                    None => self.select_clear(),
                }
                // Selection itself doesn't change the rasterized artboard pixels —
                // the selection ring is drawn as a GPUI overlay, not baked in — so
                // the host stays clean.
            }

            Action::CreateShape { tool, a, b } => {
                // Each tool maps its two drag corners to geometry differently
                // (mirrors the egui app's `shape_from_drag`): Rect / Ellipse use
                // the normalized bounding box, Line uses the raw endpoints, and
                // Polygon / Star treat `a` as the centre and |a→b| as the radius.
                let shape = match tool {
                    Tool::Line => {
                        if (b[0] - a[0]).abs() < 1.0 && (b[1] - a[1]).abs() < 1.0 {
                            return;
                        }
                        self.line_shape((a[0], a[1]), (b[0], b[1]))
                    }
                    Tool::Polygon | Tool::Star => {
                        let radius = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                        if radius < 1.0 {
                            return;
                        }
                        let live = if tool == Tool::Polygon {
                            LiveShape::Polygon {
                                sides: self.poly_sides,
                                radius,
                                corner_radius: 0.0,
                            }
                        } else {
                            LiveShape::Star {
                                points: self.star_points,
                                radius,
                                inner_ratio: self.star_ratio,
                            }
                        };
                        self.live_shape((a[0], a[1]), live)
                    }
                    _ => {
                        let x = a[0].min(b[0]);
                        let y = a[1].min(b[1]);
                        let w = (b[0] - a[0]).abs();
                        let h = (b[1] - a[1]).abs();
                        // Drop a degenerate (essentially zero-area) drag.
                        if w < 1.0 && h < 1.0 {
                            return;
                        }
                        let rect = [x, y, w, h];
                        match tool {
                            Tool::Ellipse => Shape::ellipse(
                                rect,
                                self.default_fill,
                                self.default_stroke,
                                self.default_stroke_w,
                            ),
                            // Default (and Rect) → a rectangle.
                            _ => Shape::rect(
                                rect,
                                self.default_fill,
                                self.default_stroke,
                                self.default_stroke_w,
                            ),
                        }
                    }
                };
                self.checkpoint();
                self.doc.shapes.push(shape);
                self.select_single(self.doc.shapes.len() - 1);
                self.host.mark_dirty();
            }

            Action::PlaceText { x, y } => {
                // Open one coalescing group for the whole place→type→finish session
                // so it collapses to a single undo entry (idempotent with the
                // per-keystroke `begin` in `edit_text_string`).
                self.history.begin(&self.doc);
                let params = TextParams {
                    text: String::new(),
                    font_size: self.default_font_size,
                    ..Default::default()
                };
                let glyphs = crate::text::layout(&params, (x, y)).0;
                let shape = Shape::Text {
                    params,
                    origin: (x, y),
                    glyphs,
                    fill: self.default_fill,
                    fill_gradient: None,
                    stroke: self.default_stroke,
                    // New type defaults to fill only (Illustrator's default), so
                    // glyph counters read cleanly.
                    stroke_w: 0.0,
                    stroke_style: Default::default(),
                    appearance: None,
                    visible: true,
                    group: None,
                    clip: None,
                    mask: false,
                    omask: None,
                    omask_path: false,
                    omask_invert: false,
                    blend: None,
                    blend_step: false,
                    name: None,
                    locked: false,
                    layer_color: None,
                    envelope_mesh: None,
                };
                self.doc.shapes.push(shape);
                let idx = self.doc.shapes.len() - 1;
                self.select_single(idx);
                self.editing_text = Some(idx);
                self.host.mark_dirty();
            }
            Action::EditText(i) => {
                if self.doc.shapes.get(i).is_some_and(|s| s.text_params().is_some()) {
                    self.history.begin(&self.doc);
                    self.select_single(i);
                    self.editing_text = Some(i);
                }
            }
            Action::TypeChar(c) => self.edit_text_string(|s| s.push(c)),
            Action::TypeBackspace => {
                self.edit_text_string(|s| {
                    s.pop();
                });
            }
            Action::FinishText => self.finish_text_edit(),

            Action::Boolean(op) => self.apply_boolean(op),

            Action::AddToSelection { x, y } => {
                // Shift+click toggles a shape in/out of the selection set. A newly-
                // added shape moves to the end (becoming the primary); re-clicking a
                // selected shape removes it. A miss leaves the set unchanged.
                if let Some(i) = self.hit_test_topmost(x, y) {
                    if let Some(pos) = self.selection.iter().position(|&s| s == i) {
                        self.selection.remove(pos);
                    } else {
                        self.selection.push(i);
                    }
                    self.sync_legacy_selection();
                }
            }

            Action::MarqueeSelect { rect } => {
                let marquee = CoreRect::new(rect[0], rect[1], rect[2], rect[3]);
                self.selection = self
                    .doc
                    .shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.selectable() && bounds_intersect(s, &marquee))
                    .map(|(i, _)| i)
                    .collect();
                self.sync_legacy_selection();
            }

            Action::SetLiveShape(new) => {
                if let Some(i) = self.selected {
                    // Mirror the working defaults so the next new shape inherits them.
                    match new {
                        LiveShape::Polygon { sides, .. } => self.poly_sides = sides,
                        LiveShape::Star {
                            points,
                            inner_ratio,
                            ..
                        } => {
                            self.star_points = points;
                            self.star_ratio = inner_ratio;
                        }
                    }
                    if self.doc.shapes.get(i).and_then(|s| s.live_shape()).is_some() {
                        self.checkpoint();
                        if self.doc.shapes[i].set_live_shape(new) {
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            Action::SetTextSize(size) => {
                self.edit_text_params(|p| p.font_size = size.clamp(1.0, 2000.0));
            }
            Action::SetTextAlign(a) => {
                self.edit_text_params(|p| p.align = a);
            }
            Action::SetTextFont(fam) => {
                self.edit_text_params(|p| p.font_family = fam);
                self.font_dropdown_open = false;
            }
            Action::ToggleFontDropdown => {
                self.font_dropdown_open = !self.font_dropdown_open;
            }

            Action::AlignSelection(op) => self.align_selection(op),
            Action::DistributeSelection(op) => self.distribute_selection(op),

            Action::PanBy { dx, dy } => {
                self.view.offset.0 += dx;
                self.view.offset.1 += dy;
            }

            Action::ZoomBy {
                factor,
                anchor,
                viewport,
            } => {
                let old = self.view.scale;
                let new = (old * factor).clamp(View::MIN_SCALE, View::MAX_SCALE);
                if new == old {
                    return;
                }
                // Hold the anchor point fixed on screen: the viewport-local point
                // under the cursor (or the viewport centre) must map to the same
                // document point before and after the zoom. With
                // `local = offset + doc_local * scale`, keeping `local` fixed gives
                // `offset' = anchor - (anchor - offset) * new/old`.
                let (ax, ay) = anchor.unwrap_or((viewport.0 * 0.5, viewport.1 * 0.5));
                let ratio = new / old;
                self.view.offset.0 = ax - (ax - self.view.offset.0) * ratio;
                self.view.offset.1 = ay - (ay - self.view.offset.1) * ratio;
                self.view.scale = new;
            }

            Action::ResetView => {
                self.view = View::default();
            }

            Action::SetFillColor(c) => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap().set_fill_color(c);
                    self.host.mark_dirty();
                }
            }
            Action::SetStrokeColor(c) => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap().set_stroke_color(c);
                    self.host.mark_dirty();
                }
            }
            Action::SetStrokeWidth(w) => {
                if self.selected_shape().is_some() {
                    self.checkpoint();
                    self.selected_shape_mut().unwrap().set_stroke_width(w.max(0.0));
                    self.host.mark_dirty();
                }
            }
            Action::SetOpacity(o) => {
                let o = o.clamp(0.0, 1.0);
                if let Some(mut c) = self.selected_shape().and_then(|s| s.fill_color()) {
                    self.checkpoint();
                    // Write the fill-alpha channel (see `Action::SetOpacity` doc).
                    c[3] = o;
                    self.selected_shape_mut().unwrap().set_fill_color(c);
                    self.host.mark_dirty();
                }
            }

            Action::PenAddAnchor { x, y } => {
                // Click near the first anchor closes + commits the path.
                if self.pen.points.len() >= 2 {
                    let (fx, fy) = self.pen.points[0];
                    if (x - fx).hypot(y - fy) <= PEN_CLOSE_TOL {
                        self.commit_pen(true);
                        return;
                    }
                }
                self.pen.points.push((x, y));
                self.pen.handles.push((0.0, 0.0));
            }
            Action::PenSetHandle { x, y } => {
                if let Some(&(ax, ay)) = self.pen.points.last() {
                    if let Some(h) = self.pen.handles.last_mut() {
                        *h = (x - ax, y - ay);
                    }
                }
            }
            Action::PenFinish { closed } => self.commit_pen(closed),
            Action::PenCancel => self.pen.clear(),

            Action::MoveAnchor { which, x, y } => {
                if let Some(i) = self.selected {
                    if let Some(s) = self.doc.shapes.get_mut(i) {
                        if s.set_anchor(0, which, x, y) {
                            self.host.mark_dirty();
                        }
                    }
                }
            }
            Action::MoveHandle { which, x, y } => {
                if let Some(i) = self.selected {
                    if let Some(s) = self.doc.shapes.get_mut(i) {
                        if s.set_handle(0, which, x, y) {
                            self.host.mark_dirty();
                        }
                    }
                }
            }

            Action::DeleteSelected => {
                if !self.selection.is_empty() {
                    self.checkpoint();
                    // Remove highest index first so the lower ones stay valid.
                    let mut idxs: Vec<usize> = self
                        .selection
                        .iter()
                        .copied()
                        .filter(|&i| i < self.doc.shapes.len())
                        .collect();
                    idxs.sort_unstable_by(|a, b| b.cmp(a));
                    idxs.dedup();
                    for i in idxs {
                        self.doc.shapes.remove(i);
                    }
                    self.select_clear();
                    self.host.mark_dirty();
                }
            }

            Action::BeginInteraction => self.history.begin(&self.doc),
            Action::EndInteraction => {
                self.history.commit(&self.doc);
            }
            Action::Undo => {
                if let Some(prev) = self.history.undo(&self.doc) {
                    self.doc = prev;
                    self.clamp_selection();
                    self.host.mark_dirty();
                }
            }
            Action::Redo => {
                if let Some(next) = self.history.redo(&self.doc) {
                    self.doc = next;
                    self.clamp_selection();
                    self.host.mark_dirty();
                }
            }

            a => self.apply_batch10(a),
        }
    }
}
