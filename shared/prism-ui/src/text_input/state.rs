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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ──────────────────────────────────────────────────────

    #[test]
    fn new_is_empty() {
        let s = TextInputState::new();
        assert_eq!(s.text(), "");
        assert_eq!(s.cursor(), 0);
        assert!(s.is_empty());
        assert!(!s.has_selection());
    }

    #[test]
    fn with_text_places_cursor_at_end() {
        let s = TextInputState::with_text("hello");
        assert_eq!(s.text(), "hello");
        assert_eq!(s.cursor(), 5);
        assert!(!s.has_selection());
    }

    // ── Insert ────────────────────────────────────────────────────────────

    #[test]
    fn insert_char_at_end() {
        let mut s = TextInputState::with_text("ab");
        s.insert_char('c');
        assert_eq!(s.text(), "abc");
        assert_eq!(s.cursor(), 3);
    }

    #[test]
    fn insert_char_in_middle() {
        let mut s = TextInputState::with_text("ac");
        s.move_left(false); // cursor between a and c
        s.insert_char('b');
        assert_eq!(s.text(), "abc");
        assert_eq!(s.cursor(), 2);
    }

    #[test]
    fn insert_str_appends() {
        let mut s = TextInputState::new();
        s.insert_str("hello");
        assert_eq!(s.text(), "hello");
        assert_eq!(s.cursor(), 5);
    }

    #[test]
    fn insert_replaces_selection() {
        let mut s = TextInputState::with_text("hello world");
        s.home(false);
        for _ in 0..5 {
            s.move_right(true); // select exactly "hello"
        }
        assert_eq!(s.selected_text(), Some("hello"));
        s.insert_str("hi");
        assert_eq!(s.text(), "hi world");
        assert_eq!(s.cursor(), 2);
        assert!(!s.has_selection());
    }

    // ── Backspace ─────────────────────────────────────────────────────────

    #[test]
    fn backspace_at_end() {
        let mut s = TextInputState::with_text("abc");
        s.backspace();
        assert_eq!(s.text(), "ab");
        assert_eq!(s.cursor(), 2);
    }

    #[test]
    fn backspace_in_middle() {
        let mut s = TextInputState::with_text("abc");
        s.move_left(false); // between b and c
        s.backspace(); // remove b
        assert_eq!(s.text(), "ac");
        assert_eq!(s.cursor(), 1);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut s = TextInputState::with_text("abc");
        s.home(false);
        s.backspace();
        assert_eq!(s.text(), "abc");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut s = TextInputState::with_text("abcdef");
        s.home(false);
        s.move_right(true);
        s.move_right(true);
        s.move_right(true); // select "abc"
        s.backspace();
        assert_eq!(s.text(), "def");
        assert_eq!(s.cursor(), 0);
    }

    // ── Delete forward ──────────────────────────────────────────────────────

    #[test]
    fn delete_forward_at_start() {
        let mut s = TextInputState::with_text("abc");
        s.home(false);
        s.delete_forward();
        assert_eq!(s.text(), "bc");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn delete_forward_at_end_is_noop() {
        let mut s = TextInputState::with_text("abc");
        s.delete_forward();
        assert_eq!(s.text(), "abc");
        assert_eq!(s.cursor(), 3);
    }

    #[test]
    fn delete_forward_deletes_selection() {
        let mut s = TextInputState::with_text("abcdef");
        s.home(false);
        s.end(true); // select all via shift-end
        s.delete_forward();
        assert_eq!(s.text(), "");
    }

    // ── Cursor motion ───────────────────────────────────────────────────────

    #[test]
    fn move_left_right_clamp() {
        let mut s = TextInputState::with_text("ab");
        s.home(false);
        s.move_left(false); // clamp at 0
        assert_eq!(s.cursor(), 0);
        s.end(false);
        s.move_right(false); // clamp at len
        assert_eq!(s.cursor(), 2);
    }

    #[test]
    fn move_left_collapses_selection_to_start() {
        let mut s = TextInputState::with_text("abcdef");
        s.home(false);
        s.move_right(true);
        s.move_right(true);
        s.move_right(true); // select "abc", cursor at 3
        s.move_left(false); // should collapse to start (0), not move to 2
        assert_eq!(s.cursor(), 0);
        assert!(!s.has_selection());
    }

    #[test]
    fn move_right_collapses_selection_to_end() {
        let mut s = TextInputState::with_text("abcdef");
        s.home(false);
        s.move_right(true);
        s.move_right(true); // select "ab", cursor at 2
        s.move_right(false); // collapse to end (2)
        assert_eq!(s.cursor(), 2);
        assert!(!s.has_selection());
    }

    #[test]
    fn home_and_end() {
        let mut s = TextInputState::with_text("hello");
        s.home(false);
        assert_eq!(s.cursor(), 0);
        s.end(false);
        assert_eq!(s.cursor(), 5);
    }

    // ── Word motion ───────────────────────────────────────────────────────

    #[test]
    fn word_right_across_spaces() {
        let mut s = TextInputState::with_text("foo bar baz");
        s.home(false);
        s.move_word_right(false);
        assert_eq!(s.cursor(), 4); // start of "bar"
        s.move_word_right(false);
        assert_eq!(s.cursor(), 8); // start of "baz"
        s.move_word_right(false);
        assert_eq!(s.cursor(), 11); // end of buffer
    }

    #[test]
    fn word_left_across_spaces() {
        let mut s = TextInputState::with_text("foo bar baz");
        s.end(false);
        s.move_word_left(false);
        assert_eq!(s.cursor(), 8); // start of "baz"
        s.move_word_left(false);
        assert_eq!(s.cursor(), 4); // start of "bar"
        s.move_word_left(false);
        assert_eq!(s.cursor(), 0); // start of "foo"
    }

    #[test]
    fn word_motion_stops_at_punctuation() {
        let mut s = TextInputState::with_text("foo.bar");
        s.home(false);
        s.move_word_right(false);
        // Stops after "foo" (punctuation is a separate class).
        assert_eq!(s.cursor(), 3);
        s.move_word_right(false);
        // Crosses the "." separator run, lands at start of "bar".
        assert_eq!(s.cursor(), 4);
    }

    // ── Selection ─────────────────────────────────────────────────────────

    #[test]
    fn shift_select_then_type() {
        let mut s = TextInputState::with_text("hello");
        s.home(false);
        s.move_right(true);
        s.move_right(true); // select "he"
        assert_eq!(s.selected_text(), Some("he"));
        s.insert_char('X');
        assert_eq!(s.text(), "Xllo");
        assert_eq!(s.cursor(), 1);
    }

    #[test]
    fn select_all_then_delete() {
        let mut s = TextInputState::with_text("anything here");
        s.select_all();
        assert_eq!(s.selected_text(), Some("anything here"));
        s.delete_selection();
        assert_eq!(s.text(), "");
        assert_eq!(s.cursor(), 0);
        assert!(!s.has_selection());
    }

    #[test]
    fn select_all_then_backspace() {
        let mut s = TextInputState::with_text("clobber me");
        s.select_all();
        s.backspace();
        assert_eq!(s.text(), "");
    }

    #[test]
    fn selection_range_is_ordered_when_reversed() {
        let mut s = TextInputState::with_text("abcdef");
        s.end(false);
        s.move_left(true);
        s.move_left(true); // anchor at 6, cursor at 4 (reversed)
        assert_eq!(s.selection_range(), Some((4, 6)));
        assert_eq!(s.selected_text(), Some("ef"));
    }

    #[test]
    fn clear_selection_keeps_cursor() {
        let mut s = TextInputState::with_text("abcdef");
        s.select_all();
        let c = s.cursor();
        s.clear_selection();
        assert!(!s.has_selection());
        assert_eq!(s.cursor(), c);
    }

    #[test]
    fn empty_selection_is_not_a_selection() {
        let mut s = TextInputState::with_text("abc");
        s.move_left(true); // cursor 2, anchor 3 -> non-empty
        assert!(s.has_selection());
        s.move_right(true); // back to 3, anchor 3 -> empty -> cleared
        assert!(!s.has_selection());
    }

    // ── set_text / clear ──────────────────────────────────────────────────

    #[test]
    fn set_text_moves_cursor_to_end_and_clears_selection() {
        let mut s = TextInputState::with_text("old");
        s.select_all();
        s.set_text("brand new");
        assert_eq!(s.text(), "brand new");
        assert_eq!(s.cursor(), 9);
        assert!(!s.has_selection());
    }

    #[test]
    fn clear_resets_everything() {
        let mut s = TextInputState::with_text("stuff");
        s.select_all();
        s.clear();
        assert_eq!(s.text(), "");
        assert_eq!(s.cursor(), 0);
        assert!(!s.has_selection());
    }

    // ── UTF-8 safety ────────────────────────────────────────────────────────

    #[test]
    fn utf8_cursor_never_splits_multibyte_char() {
        // "café" — the 'é' is 2 bytes (0xC3 0xA9), total len 5.
        let mut s = TextInputState::with_text("café");
        assert_eq!(s.text().len(), 5);
        s.move_left(false); // skip over 'é' as a unit -> offset 3
        assert_eq!(s.cursor(), 3);
        assert!(s.text().is_char_boundary(s.cursor()));
        s.move_left(false); // -> 2 ('f')
        assert_eq!(s.cursor(), 2);
        assert!(s.text().is_char_boundary(s.cursor()));
    }

    #[test]
    fn utf8_backspace_removes_whole_char() {
        let mut s = TextInputState::with_text("café");
        s.backspace(); // removes 'é' (2 bytes), not half of it
        assert_eq!(s.text(), "caf");
        assert_eq!(s.cursor(), 3);
    }

    #[test]
    fn utf8_delete_forward_removes_whole_char() {
        let mut s = TextInputState::with_text("café!");
        s.home(false);
        s.move_right(false);
        s.move_right(false);
        s.move_right(false); // before 'é'
        assert_eq!(s.cursor(), 3);
        s.delete_forward(); // remove 'é'
        assert_eq!(s.text(), "caf!");
    }

    #[test]
    fn emoji_cursor_moves_never_panic() {
        // Each emoji here is a 4-byte scalar value.
        let mut s = TextInputState::with_text("a😀b😀c");
        // Walk fully left, char by char, asserting boundaries each step.
        for _ in 0..10 {
            s.move_left(false);
            assert!(s.text().is_char_boundary(s.cursor()));
        }
        assert_eq!(s.cursor(), 0);
        // Walk fully right.
        for _ in 0..10 {
            s.move_right(false);
            assert!(s.text().is_char_boundary(s.cursor()));
        }
        assert_eq!(s.cursor(), s.text().len());
    }

    #[test]
    fn emoji_insert_and_backspace() {
        let mut s = TextInputState::new();
        s.insert_char('😀');
        assert_eq!(s.text(), "😀");
        assert_eq!(s.cursor(), 4);
        s.backspace();
        assert_eq!(s.text(), "");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn utf8_word_motion_safe() {
        let mut s = TextInputState::with_text("héllo wörld");
        s.home(false);
        s.move_word_right(false);
        assert!(s.text().is_char_boundary(s.cursor()));
        // Lands at the start of "wörld".
        assert_eq!(&s.text()[s.cursor()..], "wörld");
    }
}
