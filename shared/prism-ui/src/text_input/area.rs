//! [`TextArea`] — a reusable, focusable multi-line GPUI text-input view.
//!
//! The multi-line companion to [`TextField`](super::field::TextField). It wraps
//! the same pure [`TextInputState`] logic core (no edit logic is duplicated),
//! adds vertical caret motion (Up/Down preserving the visual column), and
//! renders one shaped line per `'\n'`-delimited line with a caret and selection
//! highlight using prism-ui design tokens.
//!
//! ## Usage
//! ```ignore
//! let area = cx.new(|cx| {
//!     TextArea::new(cx)
//!         .placeholder("Notes…")
//!         .initial_value("first line\nsecond line")
//!         .width(px(360.0))
//!         .rows(6)
//!         .on_change(|text, _win, _cx| { /* … */ })
//!         .on_submit(|text, _win, _cx| { /* Cmd/Ctrl+Enter */ })
//! });
//! window.focus(&area.focus_handle(cx));
//! ```
//!
//! ## Key handling
//! * Printable chars insert; **plain Enter inserts a newline**.
//! * **Cmd/Ctrl+Enter** fires `on_submit` (since plain Enter is reserved for
//!   newlines in a multi-line editor).
//! * Up/Down move between lines (column-preserving); +Shift extends selection.
//! * Left/Right move by char; +Cmd/Alt by word; +Shift extends.
//! * Home/End are **line-relative** (start/end of the current line); +Shift
//!   extends.
//! * Cmd/Ctrl+A select-all; Cmd/Ctrl + C / X / V clipboard (paste keeps
//!   newlines, unlike the single-line field which collapses them).
//!
//! ## Scope / deferred work
//! * **Vertical scroll** is a simple line-offset: when the caret would fall
//!   outside the visible `rows`, the first visible line scrolls so the caret
//!   stays in view. There is no smooth/pixel scrolling and no scrollbar.
//! * **IME / marked text** (dead keys, CJK composition) is not handled, same as
//!   `TextField`; it requires `EntityInputHandler`, a larger follow-up.
//! * **Mouse caret placement / drag-select** is future work.

use std::ops::Range;

use gpui::{
    div, fill, point, px, size, App, Bounds, ClipboardItem, Context, CursorStyle, Element,
    ElementId, Entity, FocusHandle, Focusable, GlobalElementId, InteractiveElement, IntoElement,
    KeyDownEvent, LayoutId, ParentElement, Pixels, Render, ShapedLine, Style, Styled, TextRun,
    Window,
};

use crate::tokens::{colors, font_size, radius, spacing};

use super::field::printable_text;
use super::state::TextInputState;

type ChangeHandler = Box<dyn Fn(&str, &mut Window, &mut App) + 'static>;
type SubmitHandler = Box<dyn Fn(&str, &mut Window, &mut App) + 'static>;

/// Default number of visible text rows when neither `.rows()` nor `.height()`
/// is specified.
const DEFAULT_ROWS: usize = 4;

/// A focusable, multi-line text input view backed by [`TextInputState`].
pub struct TextArea {
    focus_handle: FocusHandle,
    state: TextInputState,
    placeholder: String,
    width: Option<Pixels>,
    /// Number of visible text rows. Drives the component height (multiplied by
    /// the line height) unless an explicit `height` is set.
    rows: usize,
    /// Explicit pixel height override. When set it wins over `rows`.
    height: Option<Pixels>,
    /// Index of the first visible line (vertical scroll offset, in lines).
    scroll_line: usize,
    on_change: Option<ChangeHandler>,
    on_submit: Option<SubmitHandler>,
}

impl TextArea {
    /// Create a new, empty text area. Use the builder methods to configure it.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            state: TextInputState::new(),
            placeholder: String::new(),
            width: None,
            rows: DEFAULT_ROWS,
            height: None,
            scroll_line: 0,
            on_change: None,
            on_submit: None,
        }
    }

    // ── Builder ───────────────────────────────────────────────────────────

    /// Set the placeholder text shown when the area is empty.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Set the initial text value (cursor goes to the end). Newlines allowed.
    pub fn initial_value(mut self, text: impl Into<String>) -> Self {
        self.state.set_text(text);
        self
    }

    /// Fix the area's width in pixels. Defaults to filling its parent.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the number of visible text rows (drives the height). Ignored if an
    /// explicit `height` is also set.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    /// Set an explicit pixel height for the content area, overriding `rows`.
    pub fn height(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    /// Set a callback fired whenever the text content changes (any edit).
    pub fn on_change(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(handler));
        self
    }

    /// Set a callback fired when the user presses **Cmd/Ctrl+Enter** (plain
    /// Enter inserts a newline in a multi-line editor).
    pub fn on_submit(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }

    // ── Public state access ───────────────────────────────────────────────

    /// The current text content.
    pub fn text(&self) -> &str {
        self.state.text()
    }

    /// Replace the content programmatically (cursor → end, selection cleared).
    /// Fires `on_change`.
    pub fn set_text(
        &mut self,
        text: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.set_text(text);
        self.scroll_line = 0;
        self.emit_change(window, cx);
        cx.notify();
    }

    /// Clear the content. Fires `on_change`.
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.clear();
        self.scroll_line = 0;
        self.emit_change(window, cx);
        cx.notify();
    }

    /// Borrow the underlying editing state (read-only).
    pub fn state(&self) -> &TextInputState {
        &self.state
    }

    /// The number of visible rows configured for this area.
    pub fn visible_rows(&self) -> usize {
        self.rows
    }

    // ── Key handling ──────────────────────────────────────────────────────

    fn on_key_down(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        // `secondary` is Cmd on macOS, Ctrl elsewhere — the platform-correct
        // modifier for editing shortcuts.
        let cmd = m.secondary();
        let extend = m.shift;
        let mut changed = false;

        match ks.key.as_str() {
            // ── Clipboard / select-all (platform shortcut held) ──
            "a" if cmd => {
                self.state.select_all();
            }
            "c" if cmd => {
                if let Some(sel) = self.state.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(sel.to_string()));
                }
            }
            "x" if cmd => {
                if let Some(sel) = self.state.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(sel.to_string()));
                    self.state.delete_selection();
                    changed = true;
                }
            }
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    // Multi-line: keep newlines, but normalize CRLF/CR to '\n'.
                    let sanitized = text.replace("\r\n", "\n").replace('\r', "\n");
                    self.state.insert_str(&sanitized);
                    changed = true;
                }
            }

            // ── Caret motion ──
            // Word motion when Cmd/Ctrl or Alt is held (matches OS conventions).
            "left" if cmd || m.alt => self.state.move_word_left(extend),
            "right" if cmd || m.alt => self.state.move_word_right(extend),
            "left" => self.state.move_left(extend),
            "right" => self.state.move_right(extend),
            "up" => self.state.move_up(extend),
            "down" => self.state.move_down(extend),
            // Home/End are line-relative in a multi-line editor.
            "home" => self.state.line_home(extend),
            "end" => self.state.line_end_move(extend),

            // ── Editing ──
            "backspace" => {
                self.state.backspace();
                changed = true;
            }
            "delete" => {
                self.state.delete_forward();
                changed = true;
            }
            // Plain Enter inserts a newline; Cmd/Ctrl+Enter submits.
            "enter" => {
                if cmd {
                    let text = self.state.text().to_string();
                    if let Some(handler) = self.on_submit.take() {
                        handler(&text, window, cx);
                        self.on_submit = Some(handler);
                    }
                } else {
                    self.state.insert_newline();
                    changed = true;
                }
            }

            // ── Printable input ──
            _ => {
                if !cmd && !m.control {
                    if let Some(text) = printable_text(ev) {
                        self.state.insert_str(&text);
                        changed = true;
                    }
                }
            }
        }

        // Keep the caret in view after any motion or edit.
        self.scroll_caret_into_view();

        if changed {
            self.emit_change(window, cx);
        }
        cx.notify();
    }

    /// Adjust `scroll_line` so the cursor's line stays within the visible
    /// window of `rows` lines. Simple line-granularity offset (no smooth
    /// scroll); see the module docs.
    fn scroll_caret_into_view(&mut self) {
        let caret_line = self.cursor_line_index();
        let rows = self.rows.max(1);
        if caret_line < self.scroll_line {
            self.scroll_line = caret_line;
        } else if caret_line >= self.scroll_line + rows {
            self.scroll_line = caret_line + 1 - rows;
        }
    }

    /// The zero-based line index the cursor currently sits on.
    fn cursor_line_index(&self) -> usize {
        let cursor = self.state.cursor();
        self.state.text()[..cursor]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
    }

    /// Invoke the `on_change` callback with the current text, if one is set.
    fn emit_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.state.text().to_string();
        if let Some(handler) = self.on_change.take() {
            handler(&text, window, cx);
            self.on_change = Some(handler);
        }
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let border_color = if focused {
            colors::accent()
        } else {
            colors::surface_border()
        };

        // Height: explicit override, else rows * line height + vertical padding.
        let line_height = window.line_height();
        let content_height = self
            .height
            .unwrap_or_else(|| line_height * (self.rows.max(1) as f32));

        let mut root = div()
            .key_context("PrismTextArea")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .h(content_height + px(spacing::MD * 2.0))
            .px(px(spacing::MD))
            .py(px(spacing::MD))
            .rounded(px(radius::MD))
            .bg(colors::surface_overlay())
            .border_1()
            .border_color(border_color)
            .text_size(px(font_size::MD))
            .text_color(colors::text_primary())
            .overflow_hidden();

        if let Some(w) = self.width {
            root = root.w(w);
        } else {
            root = root.w_full();
        }

        root.child(TextAreaElement {
            area: cx.entity(),
        })
    }
}

// ── Custom element: multi-line text + caret + selection ─────────────────────
//
// Like `TextFieldElement`, a dedicated `Element` is the reliable way to place a
// caret at an exact byte offset using shaped-line geometry (`x_for_index`).
// This one shapes one line per `'\n'`-delimited line, stacks them vertically,
// and draws a per-line selection highlight + a caret on the cursor's line.

struct TextAreaElement {
    area: Entity<TextArea>,
}

struct LineLayout {
    shaped: ShapedLine,
}

struct PrepaintState {
    lines: Vec<LineLayout>,
    /// Selection highlight quads (one per affected visible line).
    selection: Vec<gpui::PaintQuad>,
    cursor: Option<gpui::PaintQuad>,
    line_height: Pixels,
}

impl IntoElement for TextAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextAreaElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = gpui::relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let area = self.area.read(cx);
        let content = area.state.text().to_string();
        let cursor_byte = area.state.cursor();
        let selection: Option<Range<usize>> = area.state.selection_range().map(|(s, e)| s..e);
        let placeholder = area.placeholder.clone();
        let focused = area.focus_handle.is_focused(window);
        let scroll_line = area.scroll_line;
        let rows = area.rows.max(1);

        let style = window.text_style();
        let font = style.font();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        let is_empty = content.is_empty();

        // Build the list of (start, text) for each line of the buffer. When the
        // buffer is empty, render the placeholder as the single line 0.
        let mut line_layouts: Vec<LineLayout> = Vec::new();
        let mut selection_quads: Vec<gpui::PaintQuad> = Vec::new();
        let mut cursor_quad: Option<gpui::PaintQuad> = None;

        // Iterate lines with their byte offsets.
        let mut byte = 0usize;
        // Use split so a trailing '\n' yields a final empty line.
        let raw_lines: Vec<&str> = content.split('\n').collect();
        for (idx, line_text) in raw_lines.iter().enumerate() {
            let line_start = byte;
            let line_len = line_text.len();
            let line_end = line_start + line_len;

            // Only shape & lay out visible lines for efficiency.
            let visible = idx >= scroll_line && idx < scroll_line + rows;
            if visible {
                let row = idx - scroll_line;
                let top = bounds.top() + line_height * (row as f32);

                let (display, color) = if is_empty {
                    (placeholder.clone(), colors::text_disabled().into())
                } else {
                    ((*line_text).to_string(), style.color)
                };
                // Defensive: GPUI's single-line `shape_line` panics on an embedded
                // newline. Per-line body content never holds one, but a multi-line
                // placeholder would — flatten any CR/LF to spaces before shaping so
                // a stray newline can never crash the render.
                let display = if display.contains('\n') || display.contains('\r') {
                    display.replace(['\n', '\r'], " ")
                } else {
                    display
                };
                let run = TextRun {
                    len: display.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped =
                    window
                        .text_system()
                        .shape_line(display.into(), font_size, &[run], None);

                // Selection highlight intersected with this line.
                if let Some(sel) = &selection {
                    let seg_start = sel.start.max(line_start);
                    let seg_end = sel.end.min(line_end);
                    // Selection spanning into/over the newline: if the selection
                    // extends past this line's end, draw to the line's far edge.
                    let spans_newline = sel.end > line_end && idx + 1 < raw_lines.len();
                    if seg_start < seg_end || spans_newline {
                        let local_start = seg_start.saturating_sub(line_start).min(line_len);
                        let local_end = if spans_newline {
                            line_len
                        } else {
                            seg_end.saturating_sub(line_start).min(line_len)
                        };
                        let x0 = shaped.x_for_index(local_start);
                        let x1 = shaped.x_for_index(local_end);
                        // Give zero-width (newline-only) selections a small
                        // visible sliver so empty selected lines still read.
                        let x1 = if spans_newline && x1 <= x0 {
                            x0 + px(4.0)
                        } else {
                            x1
                        };
                        selection_quads.push(fill(
                            Bounds::from_corners(
                                point(bounds.left() + x0, top),
                                point(bounds.left() + x1, top + line_height),
                            ),
                            accent_selection(),
                        ));
                    }
                }

                // Caret on this line (only when there's no active selection).
                // The cursor belongs to this line when it's within
                // [line_start, line_end]. At a boundary offset (== the '\n' or
                // end of buffer) it sits at this line's end; the very next line
                // also matches at its start, so we additionally require that —
                // for an interior boundary — this is the line whose end equals
                // the cursor only when the cursor isn't the start of a later
                // line. Drawing the caret on the earlier line's end is correct
                // for "cursor before the newline"; GPUI Up/Down keep them
                // distinct via the column model.
                if selection.is_none()
                    && cursor_byte >= line_start
                    && cursor_byte <= line_end
                    && cursor_quad.is_none()
                {
                    let local = cursor_byte.saturating_sub(line_start).min(line_len);
                    let cursor_x = shaped.x_for_index(local);
                    cursor_quad = focused.then(|| {
                        fill(
                            Bounds::new(
                                point(bounds.left() + cursor_x, top),
                                size(px(1.5), line_height),
                            ),
                            colors::accent(),
                        )
                    });
                }

                line_layouts.push(LineLayout { shaped });
            }

            byte = line_end + 1; // +1 for the '\n' separator
        }

        PrepaintState {
            lines: line_layouts,
            selection: selection_quads,
            cursor: cursor_quad,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Paint selection highlights beneath the text.
        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }

        let line_height = prepaint.line_height;

        // Lines were collected in visible order, so the first stored line maps
        // to row 0 (the top of the visible window) and so on.
        let lines = std::mem::take(&mut prepaint.lines);
        for (row, layout) in lines.into_iter().enumerate() {
            let top = bounds.top() + line_height * (row as f32);
            let origin = point(bounds.left(), top);
            let _ = layout.shaped.paint(origin, line_height, window, cx);
        }

        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

/// A translucent accent fill for the selection highlight.
fn accent_selection() -> gpui::Rgba {
    let a = colors::accent();
    gpui::Rgba { a: 0.35, ..a }
}
