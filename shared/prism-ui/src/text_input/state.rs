//! [`TextInputState`] — the pure, GPUI-free logic core of the text-input widget.
//!
//! This module owns *only* the editing model: the text buffer, the cursor
//! position, and an optional selection. It performs no rendering and has no
//! dependency on `gpui`, which makes every operation exhaustively unit-testable
//! in isolation (see the `#[cfg(test)]` block at the bottom).
//!
//! ## Invariants
//! * `cursor` is a **byte** index into `text` that always lands on a UTF-8
//!   char boundary. Every public operation preserves this.
//! * `selection_anchor`, when `Some`, is also a byte index on a char boundary.
//!   The active selection is the (possibly empty) range between `anchor` and
//!   `cursor`; an empty range is treated as "no selection".
//! * All offsets are clamped into `0..=text.len()`; no operation can panic on
//!   multi-byte (UTF-8) input.

/// Pure editing state for a single-line text input.
///
/// Holds the text buffer plus a cursor and optional selection anchor, both as
/// byte offsets on char boundaries. This type is deliberately free of any UI
/// or GPUI types so it can be unit-tested without a window or GPU.
#[derive(Clone, Debug, Default)]
pub struct TextInputState {
    /// The full text content.
    text: String,
    /// Cursor position as a byte index into `text`, always on a char boundary.
    cursor: usize,
    /// Selection anchor as a byte index. When `Some`, the active selection runs
    /// between this anchor and `cursor`. An anchor equal to the cursor means an
    /// empty (and therefore inactive) selection.
    selection_anchor: Option<usize>,
}

impl TextInputState {
    /// Create an empty state with the cursor at offset 0 and no selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a state pre-populated with `text`, cursor at the end, no selection.
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            selection_anchor: None,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    /// The current text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The cursor position as a byte index (always on a char boundary).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selection anchor, if a selection is active.
    pub fn selection_anchor(&self) -> Option<usize> {
        self.selection_anchor
    }

    /// `true` when the text buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// `true` when there is a non-empty active selection.
    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    /// The active selection as an ordered `(start, end)` byte range, or `None`
    /// when there is no selection or the selection is empty.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some((anchor.min(self.cursor), anchor.max(self.cursor)))
    }

    /// The currently selected text, if any.
    pub fn selected_text(&self) -> Option<&str> {
        let (start, end) = self.selection_range()?;
        Some(&self.text[start..end])
    }

    // ── Mutation: text content ────────────────────────────────────────────

    /// Replace the entire buffer, place the cursor at the end, clear selection.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.selection_anchor = None;
    }

    /// Clear the buffer entirely (empty text, cursor at 0, no selection).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.selection_anchor = None;
    }

    /// Insert a single character at the cursor, replacing any active selection.
    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    /// Insert a string at the cursor, replacing any active selection. The
    /// cursor ends up immediately after the inserted text.
    pub fn insert_str(&mut self, s: &str) {
        if self.has_selection() {
            self.delete_selection();
        }
        let at = self.cursor;
        self.text.insert_str(at, s);
        self.cursor = at + s.len();
        self.selection_anchor = None;
    }

    /// Delete the character before the cursor. If a selection is active, delete
    /// the selection instead. No-op at the start of an empty selection.
    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    /// Delete the character after the cursor. If a selection is active, delete
    /// the selection instead. No-op at the end of the buffer.
    pub fn delete_forward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.next_boundary(self.cursor);
        self.text.replace_range(self.cursor..next, "");
    }

    /// Delete the active selection (if any) and place the cursor at its start.
    /// No-op when there is no selection.
    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.text.replace_range(start..end, "");
            self.cursor = start;
        }
        self.selection_anchor = None;
    }

    // ── Mutation: selection ───────────────────────────────────────────────

    /// Select the whole buffer (anchor at 0, cursor at end). No-op visual
    /// effect on empty text, but anchor/cursor are still reset.
    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.text.len();
    }

    /// Collapse / clear any active selection without moving the cursor.
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    // ── Mutation: cursor motion ───────────────────────────────────────────
    //
    // Each motion takes `extend: bool` (Shift held). When extending, the anchor
    // is established at the current cursor (if not already set) and preserved;
    // when not extending, an active selection is collapsed to the appropriate
    // edge and the anchor is cleared.

    /// Move the cursor one grapheme/char left (or collapse selection to its
    /// left edge when not extending).
    pub fn move_left(&mut self, extend: bool) {
        if !extend {
            if let Some((start, _)) = self.selection_range() {
                self.cursor = start;
                self.selection_anchor = None;
                return;
            }
        }
        self.before_move(extend);
        self.cursor = self.prev_boundary(self.cursor);
        self.after_move(extend);
    }

    /// Move the cursor one grapheme/char right (or collapse selection to its
    /// right edge when not extending).
    pub fn move_right(&mut self, extend: bool) {
        if !extend {
            if let Some((_, end)) = self.selection_range() {
                self.cursor = end;
                self.selection_anchor = None;
                return;
            }
        }
        self.before_move(extend);
        self.cursor = self.next_boundary(self.cursor);
        self.after_move(extend);
    }

    /// Move the cursor to the start of the previous word.
    pub fn move_word_left(&mut self, extend: bool) {
        self.before_move(extend);
        self.cursor = self.prev_word_boundary(self.cursor);
        self.after_move(extend);
    }

    /// Move the cursor to the end of the next word.
    pub fn move_word_right(&mut self, extend: bool) {
        self.before_move(extend);
        self.cursor = self.next_word_boundary(self.cursor);
        self.after_move(extend);
    }

    /// Move the cursor to the start of the line (offset 0).
    pub fn home(&mut self, extend: bool) {
        self.before_move(extend);
        self.cursor = 0;
        self.after_move(extend);
    }

    /// Move the cursor to the end of the line (offset `text.len()`).
    pub fn end(&mut self, extend: bool) {
        self.before_move(extend);
        self.cursor = self.text.len();
        self.after_move(extend);
    }

    // ── Multi-line API (additive) ─────────────────────────────────────────
    //
    // These helpers treat the buffer as a sequence of lines split on `'\n'`
    // (the `'\n'` itself is *not* part of any line). They are used by the
    // multi-line `TextArea` view and leave all single-line behavior above
    // untouched. Every offset they produce lands on a UTF-8 char boundary.

    /// Byte offset of the start of the line containing `byte`.
    ///
    /// This is one past the previous `'\n'`, or 0 when `byte` is on the first
    /// line. `byte` is clamped into `0..=text.len()`.
    pub fn line_start(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        match self.text[..byte].rfind('\n') {
            Some(nl) => nl + 1,
            None => 0,
        }
    }

    /// Byte offset of the end of the line containing `byte` — i.e. the offset
    /// of the next `'\n'`, or `text.len()` when this is the last line. The
    /// returned offset points *at* the newline, never past it.
    pub fn line_end(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        match self.text[byte..].find('\n') {
            Some(rel) => byte + rel,
            None => self.text.len(),
        }
    }

    /// Number of lines in the buffer (always ≥ 1). Equals one plus the count
    /// of `'\n'` characters; a trailing newline yields a final empty line.
    pub fn line_count(&self) -> usize {
        self.text.bytes().filter(|&b| b == b'\n').count() + 1
    }

    /// Iterate the buffer's lines (the text between newlines, excluding the
    /// `'\n'`s). A trailing newline yields a final empty `""` line, matching
    /// the semantics of [`line_count`](Self::line_count) and the renderer.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        // `str::split('\n')` already yields a trailing "" after a final '\n'
        // and a single "" for an empty buffer — exactly what we want.
        self.text.split('\n')
    }

    /// The visual column of the cursor: the number of *chars* (not bytes)
    /// between the start of the cursor's line and the cursor itself.
    pub fn column(&self) -> usize {
        let ls = self.line_start(self.cursor);
        self.text[ls..self.cursor].chars().count()
    }

    /// Move the cursor to the start of its current line. Distinct from
    /// [`home`](Self::home) (which goes to offset 0 of the whole buffer) so
    /// that single-line callers keep their existing `home`/`end` behavior.
    pub fn line_home(&mut self, extend: bool) {
        self.before_move(extend);
        self.cursor = self.line_start(self.cursor);
        self.after_move(extend);
    }

    /// Move the cursor to the end of its current line (before any `'\n'`).
    pub fn line_end_move(&mut self, extend: bool) {
        self.before_move(extend);
        self.cursor = self.line_end(self.cursor);
        self.after_move(extend);
    }

    /// Insert a newline at the cursor, splitting the current line. Identical to
    /// `insert_char('\n')`; provided as an explicit, self-documenting helper
    /// for the multi-line editor.
    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Move the cursor up one visual line, preserving its column (char count
    /// from the line start), clamped to the target line's length.
    ///
    /// At the first line this is a no-op for the line position, but it still
    /// honors `extend` (collapsing or extending the selection as appropriate).
    pub fn move_up(&mut self, extend: bool) {
        self.before_move(extend);
        let col = self.column();
        let cur_start = self.line_start(self.cursor);
        if cur_start == 0 {
            // Already on the first line: clamp to its start (mirrors editors
            // where Up on line 1 moves to the very beginning).
            self.cursor = 0;
        } else {
            // The previous line ends at the '\n' just before our line start.
            let prev_end = cur_start - 1; // offset of that '\n'
            let prev_start = self.line_start(prev_end);
            self.cursor = self.offset_for_column(prev_start, prev_end, col);
        }
        self.after_move(extend);
    }

    /// Move the cursor down one visual line, preserving its column (char count
    /// from the line start), clamped to the target line's length.
    ///
    /// At the last line this clamps the cursor to the line's end while still
    /// honoring `extend`.
    pub fn move_down(&mut self, extend: bool) {
        self.before_move(extend);
        let col = self.column();
        let cur_end = self.line_end(self.cursor);
        if cur_end >= self.text.len() {
            // Already on the last line: clamp to its end.
            self.cursor = self.text.len();
        } else {
            // The next line starts just past the '\n' at `cur_end`.
            let next_start = cur_end + 1;
            let next_end = self.line_end(next_start);
            self.cursor = self.offset_for_column(next_start, next_end, col);
        }
        self.after_move(extend);
    }

    /// Resolve the byte offset for the `col`-th char within the line
    /// `[start, end]`, clamped to the line's length. `start` and `end` must be
    /// char boundaries delimiting one line (no interior `'\n'`).
    fn offset_for_column(&self, start: usize, end: usize, col: usize) -> usize {
        let mut offset = start;
        for _ in 0..col {
            if offset >= end {
                return end;
            }
            offset = self.next_boundary(offset);
        }
        offset.min(end)
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    /// Called before a motion: if extending and no anchor is set, drop an
    /// anchor at the current cursor; if not extending, clear the anchor.
    fn before_move(&mut self, extend: bool) {
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
    }

    /// Called after a motion: if extending, an anchor equal to the new cursor
    /// means an empty selection — clear it to keep `has_selection()` honest.
    fn after_move(&mut self, extend: bool) {
        if extend {
            if self.selection_anchor == Some(self.cursor) {
                self.selection_anchor = None;
            }
        }
    }

    /// The byte offset of the char boundary immediately before `offset`.
    /// Returns 0 when already at the start.
    fn prev_boundary(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        let mut i = offset - 1;
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    /// The byte offset of the char boundary immediately after `offset`.
    /// Returns `text.len()` when already at the end.
    fn next_boundary(&self, offset: usize) -> usize {
        let len = self.text.len();
        if offset >= len {
            return len;
        }
        let mut i = offset + 1;
        while i < len && !self.text.is_char_boundary(i) {
            i += 1;
        }
        i
    }

    /// Walk left over any run of whitespace, then over a run of "word" chars,
    /// stopping at the start of the previous word. Punctuation counts as its
    /// own non-word run so the cursor stops between words and punctuation.
    fn prev_word_boundary(&self, offset: usize) -> usize {
        let mut i = offset;
        // Skip whitespace immediately to the left.
        while i > 0 {
            let p = self.prev_boundary(i);
            if Self::char_at(&self.text, p).is_some_and(|c| c.is_whitespace()) {
                i = p;
            } else {
                break;
            }
        }
        if i == 0 {
            return 0;
        }
        // Determine the class of the char to the left and consume that run.
        let p = self.prev_boundary(i);
        let word = Self::char_at(&self.text, p).is_some_and(Self::is_word_char);
        while i > 0 {
            let p = self.prev_boundary(i);
            match Self::char_at(&self.text, p) {
                Some(c) if Self::is_word_char(c) == word && !c.is_whitespace() => i = p,
                _ => break,
            }
        }
        i
    }

    /// Walk right over a run of "word" (or non-word) chars, then over trailing
    /// whitespace, stopping at the start of the next word.
    fn next_word_boundary(&self, offset: usize) -> usize {
        let len = self.text.len();
        let mut i = offset;
        // Skip whitespace immediately to the right.
        while i < len && Self::char_at(&self.text, i).is_some_and(|c| c.is_whitespace()) {
            i = self.next_boundary(i);
        }
        if i >= len {
            return len;
        }
        // Consume the run of chars sharing the class of the char at the cursor.
        let word = Self::char_at(&self.text, i).is_some_and(Self::is_word_char);
        while i < len {
            match Self::char_at(&self.text, i) {
                Some(c) if Self::is_word_char(c) == word && !c.is_whitespace() => {
                    i = self.next_boundary(i)
                }
                _ => break,
            }
        }
        // Then skip trailing whitespace so the cursor lands at the next word.
        while i < len && Self::char_at(&self.text, i).is_some_and(|c| c.is_whitespace()) {
            i = self.next_boundary(i);
        }
        i
    }

    /// The `char` starting at byte offset `at`, or `None` if at/over the end.
    fn char_at(text: &str, at: usize) -> Option<char> {
        text[at..].chars().next()
    }

    /// A "word" character: alphanumeric or underscore. Everything else
    /// (punctuation, symbols, whitespace) is treated as a separator.
    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }
}

/// Unit tests live in a sibling file (`state_tests.rs`) to keep this source
/// file under the ~1000-line limit, while remaining the `tests` child module
/// of `state` so they retain access to private items via `use super::*`.
#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
