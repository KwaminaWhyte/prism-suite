//! [`TextField`] — a reusable, focusable single-line GPUI text-input view.
//!
//! Wraps the pure [`TextInputState`] logic core with GPUI rendering and key
//! handling. It is intentionally self-contained: it handles its own key events
//! via `on_key_down` (no global `actions!` / `KeyBinding` registration is
//! required by the consumer), renders its own caret and selection highlight via
//! a custom [`Element`], and uses prism-ui's design tokens for all colors and
//! sizing.
//!
//! ## Usage
//! ```ignore
//! let field = cx.new(|cx| {
//!     TextField::new(cx)
//!         .placeholder("Search…")
//!         .initial_value("hello")
//!         .width(px(220.0))
//!         .on_change(|text, _win, _cx| { /* … */ })
//!         .on_submit(|text, _win, _cx| { /* … */ })
//! });
//! // …then render `field.clone()` as a child and focus it:
//! window.focus(&field.focus_handle(cx));
//! ```
//!
//! ## Scope / deferred work
//! * **Single-line only.** Newlines are filtered out of inserted text. A
//!   multi-line `TextArea` (wrapping the same state core, with vertical caret
//!   motion) is future work.
//! * **Clipboard** (Cmd/Ctrl + C / X / V) is wired through GPUI's
//!   `read_from_clipboard` / `write_to_clipboard`, which are available on `App`.
//! * **IME / marked text** (dead keys, CJK composition) is not handled here;
//!   that requires implementing `EntityInputHandler` + `ElementInputHandler`,
//!   which is a larger follow-up. Direct key events cover Latin input fully.
//! * **Mouse caret placement / drag-select** is future work; the element keeps
//!   the last shaped line + bounds so it can be added without an API change.

use std::ops::Range;

use gpui::{
    div, fill, point, px, size, App, Bounds, ClipboardItem, Context, CursorStyle, Element,
    ElementId, Entity, FocusHandle, Focusable, GlobalElementId, InteractiveElement, IntoElement,
    KeyDownEvent, LayoutId, ParentElement, Pixels, Render, ShapedLine, Style, Styled, TextRun,
    Window,
};

use crate::tokens::{colors, font_size, radius, spacing};

type ChangeHandler = Box<dyn Fn(&str, &mut Window, &mut App) + 'static>;
type SubmitHandler = Box<dyn Fn(&str, &mut Window, &mut App) + 'static>;

use super::state::TextInputState;

/// A focusable, single-line text input view backed by [`TextInputState`].
pub struct TextField {
    focus_handle: FocusHandle,
    state: TextInputState,
    placeholder: String,
    width: Option<Pixels>,
    on_change: Option<ChangeHandler>,
    on_submit: Option<SubmitHandler>,
}

impl TextField {
    /// Create a new, empty text field. Use the builder methods to configure it.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            state: TextInputState::new(),
            placeholder: String::new(),
            width: None,
            on_change: None,
            on_submit: None,
        }
    }

    // ── Builder ───────────────────────────────────────────────────────────

    /// Set the placeholder text shown when the field is empty.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Set the initial text value (cursor goes to the end).
    pub fn initial_value(mut self, text: impl Into<String>) -> Self {
        self.state.set_text(text);
        self
    }

    /// Fix the field's width in pixels. Defaults to filling its parent.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// Set a callback fired whenever the text content changes (any edit).
    pub fn on_change(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(handler));
        self
    }

    /// Set a callback fired when the user presses Enter.
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
    pub fn set_text(&mut self, text: impl Into<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_text(text);
        self.emit_change(window, cx);
        cx.notify();
    }

    /// Clear the content. Fires `on_change`.
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.clear();
        self.emit_change(window, cx);
        cx.notify();
    }

    /// Borrow the underlying editing state (read-only).
    pub fn state(&self) -> &TextInputState {
        &self.state
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
                    // Single-line: collapse any newlines into spaces.
                    let sanitized = text.replace(['\n', '\r'], " ");
                    self.state.insert_str(&sanitized);
                    changed = true;
                }
            }

            // ── Caret motion ──
            // Word motion when Cmd/Ctrl or Alt is held (matches OS conventions:
            // Alt+arrow on macOS, Ctrl+arrow elsewhere — accept either).
            "left" if cmd || m.alt => self.state.move_word_left(extend),
            "right" if cmd || m.alt => self.state.move_word_right(extend),
            "left" => self.state.move_left(extend),
            "right" => self.state.move_right(extend),
            "home" => self.state.home(extend),
            "end" => self.state.end(extend),

            // ── Editing ──
            "backspace" => {
                self.state.backspace();
                changed = true;
            }
            "delete" => {
                self.state.delete_forward();
                changed = true;
            }
            "enter" => {
                let text = self.state.text().to_string();
                if let Some(handler) = self.on_submit.take() {
                    handler(&text, window, cx);
                    self.on_submit = Some(handler);
                }
            }

            // ── Printable input ──
            // Prefer `key_char` (already accounts for shift + keyboard layout);
            // fall back to a single-char `key` when no modifier that would
            // suppress text entry is held.
            _ => {
                if !cmd && !m.control {
                    if let Some(text) = printable_text(ev) {
                        self.state.insert_str(&text);
                        changed = true;
                    }
                }
            }
        }

        if changed {
            self.emit_change(window, cx);
        }
        cx.notify();
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

/// Extract printable text from a key event, filtering out control keys and
/// newlines. Returns `None` when the event carries no insertable character.
fn printable_text(ev: &KeyDownEvent) -> Option<String> {
    let ks = &ev.keystroke;
    // `key_char` is set by GPUI to the character that would be typed (honoring
    // shift and keyboard layout). When absent, accept a bare single-char key.
    let raw = ks
        .key_char
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| (ks.key.chars().count() == 1).then(|| ks.key.clone()))?;
    // Reject control characters and newlines (single-line field).
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(_window);
        let border_color = if focused {
            colors::accent()
        } else {
            colors::surface_border()
        };

        let mut root = div()
            .key_context("PrismTextField")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .items_center()
            .h(px(28.0))
            .px(px(spacing::MD))
            .rounded(px(radius::MD))
            .bg(colors::surface_overlay())
            .border_1()
            .border_color(border_color)
            .text_size(px(font_size::MD))
            .text_color(colors::text_primary());

        if let Some(w) = self.width {
            root = root.w(w);
        } else {
            root = root.w_full();
        }

        root.child(TextFieldElement {
            field: cx.entity(),
        })
    }
}

// ── Custom element: text + caret + selection ────────────────────────────────
//
// A dedicated `Element` is the only reliable way to place a caret at an exact
// byte offset, because it needs the shaped-line geometry (`x_for_index`) that
// only becomes available during prepaint. This mirrors the canonical gpui
// `input.rs` example but draws with prism-ui design tokens.

struct TextFieldElement {
    field: Entity<TextField>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<gpui::PaintQuad>,
    selection: Option<gpui::PaintQuad>,
}

impl IntoElement for TextFieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextFieldElement {
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
        style.size.height = window.line_height().into();
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
        let field = self.field.read(cx);
        let content = field.state.text().to_string();
        let cursor_byte = field.state.cursor();
        let selection: Option<Range<usize>> =
            field.state.selection_range().map(|(s, e)| s..e);
        let placeholder = field.placeholder.clone();
        let focused = field.focus_handle.is_focused(window);

        let style = window.text_style();
        let (display_text, text_color) = if content.is_empty() {
            (placeholder, colors::text_disabled().into())
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text.into(), font_size, &[run], None);

        // Vertically center a single line of text within the bounds.
        let line_height = window.line_height();
        let v_pad = ((bounds.size.height - line_height) / 2.0).max(px(0.0));
        let top = bounds.top() + v_pad;

        let (selection_quad, cursor_quad) = if let Some(sel) = selection {
            let quad = fill(
                Bounds::from_corners(
                    point(bounds.left() + line.x_for_index(sel.start), top),
                    point(
                        bounds.left() + line.x_for_index(sel.end),
                        top + line_height,
                    ),
                ),
                accent_selection(),
            );
            (Some(quad), None)
        } else {
            let cursor_x = line.x_for_index(cursor_byte);
            let quad = fill(
                Bounds::new(
                    point(bounds.left() + cursor_x, top),
                    size(px(1.5), line_height),
                ),
                colors::accent(),
            );
            // Only show the caret when focused.
            (None, focused.then_some(quad))
        };

        PrepaintState {
            line: Some(line),
            cursor: cursor_quad,
            selection: selection_quad,
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
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        let line_height = window.line_height();
        let v_pad = ((bounds.size.height - line_height) / 2.0).max(px(0.0));
        let origin = point(bounds.left(), bounds.top() + v_pad);
        let _ = line.paint(origin, line_height, window, cx);
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
