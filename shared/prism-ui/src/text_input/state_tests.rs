//! Unit tests for [`TextInputState`](super::TextInputState).
//!
//! Split out of `state.rs` to keep that source file under the ~1000-line
//! limit. Included as the `tests` child module of `state` via `#[path]`, so
//! `use super::*` here resolves to the `state` module and its private items.

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

    // ── Multi-line: line_start / line_end ─────────────────────────────────

    #[test]
    fn line_bounds_single_line() {
        let s = TextInputState::with_text("hello");
        assert_eq!(s.line_start(0), 0);
        assert_eq!(s.line_start(5), 0);
        assert_eq!(s.line_end(0), 5);
        assert_eq!(s.line_end(5), 5);
    }

    #[test]
    fn line_bounds_multi_line() {
        // "ab\ncde\nf"  bytes: a0 b1 \n2 c3 d4 e5 \n6 f7
        let s = TextInputState::with_text("ab\ncde\nf");
        // First line "ab": [0, 2)
        assert_eq!(s.line_start(0), 0);
        assert_eq!(s.line_start(2), 0);
        assert_eq!(s.line_end(0), 2);
        assert_eq!(s.line_end(2), 2);
        // Second line "cde": [3, 6)
        assert_eq!(s.line_start(3), 3);
        assert_eq!(s.line_start(5), 3);
        assert_eq!(s.line_end(3), 6);
        assert_eq!(s.line_end(6), 6);
        // Third line "f": [7, 8)
        assert_eq!(s.line_start(7), 7);
        assert_eq!(s.line_end(7), 8);
    }

    #[test]
    fn line_bounds_at_newline_boundaries() {
        // For a byte sitting on the '\n' (offset 2), it belongs to the line
        // that ends *at* that newline.
        let s = TextInputState::with_text("ab\ncd");
        assert_eq!(s.line_start(2), 0); // the '\n' at 2 is end of line 0
        assert_eq!(s.line_end(2), 2);
        // The char right after the '\n' starts line 1.
        assert_eq!(s.line_start(3), 3);
        assert_eq!(s.line_end(3), 5);
    }

    #[test]
    fn line_count_and_lines() {
        assert_eq!(TextInputState::new().line_count(), 1);
        assert_eq!(TextInputState::with_text("abc").line_count(), 1);
        assert_eq!(TextInputState::with_text("a\nb\nc").line_count(), 3);
        // Trailing newline yields a final empty line.
        let s = TextInputState::with_text("a\nb\n");
        assert_eq!(s.line_count(), 3);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines, vec!["a", "b", ""]);
    }

    // ── Multi-line: vertical motion preserves column ──────────────────────

    #[test]
    fn move_up_preserves_column() {
        // Two lines of equal length; start at col 2 on line 1, go up to col 2.
        let mut s = TextInputState::with_text("abcd\nefgh");
        // Cursor is at end (offset 9 = col 4 of line 1).
        s.line_home(false); // start of line 1 (offset 5)
        s.move_right(false);
        s.move_right(false); // col 2 -> offset 7 ('g' position)
        assert_eq!(s.cursor(), 7);
        s.move_up(false); // line 0, col 2 -> offset 2
        assert_eq!(s.cursor(), 2);
    }

    #[test]
    fn move_down_preserves_column() {
        let mut s = TextInputState::with_text("abcd\nefgh");
        s.home(false); // offset 0, col 0 line 0
        s.move_right(false);
        s.move_right(false);
        s.move_right(false); // col 3 -> offset 3
        assert_eq!(s.cursor(), 3);
        s.move_down(false); // line 1 col 3 -> offset 5 + 3 = 8
        assert_eq!(s.cursor(), 8);
    }

    #[test]
    fn move_up_at_first_line_clamps_to_start() {
        let mut s = TextInputState::with_text("abcd\nefgh");
        s.home(false);
        s.move_right(false);
        s.move_right(false); // col 2 on line 0 -> offset 2
        s.move_up(false); // already first line -> clamp to 0
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn move_down_at_last_line_clamps_to_end() {
        let mut s = TextInputState::with_text("abcd\nefgh");
        s.line_home(false); // start of line 1, offset 5
        s.move_right(false); // col 1 -> offset 6
        s.move_down(false); // already last line -> clamp to len (9)
        assert_eq!(s.cursor(), 9);
    }

    #[test]
    fn move_down_clamps_column_to_shorter_line() {
        // Line 0 is long, line 1 is short: col should clamp to line 1's length.
        let mut s = TextInputState::with_text("abcdef\nxy");
        s.home(false);
        for _ in 0..5 {
            s.move_right(false); // col 5 -> offset 5
        }
        assert_eq!(s.cursor(), 5);
        s.move_down(false); // line 1 only has 2 chars -> clamp to its end (9)
        assert_eq!(s.cursor(), 9);
        assert_eq!(s.column(), 2);
    }

    #[test]
    fn move_up_clamps_column_to_shorter_line() {
        let mut s = TextInputState::with_text("xy\nabcdef");
        s.end(false); // offset 9, col 6 on line 1
        s.move_up(false); // line 0 only has 2 chars -> clamp to its end (2)
        assert_eq!(s.cursor(), 2);
        assert_eq!(s.column(), 2);
    }

    #[test]
    fn move_up_down_with_shift_extends_selection() {
        let mut s = TextInputState::with_text("abcd\nefgh");
        s.home(false); // offset 0
        s.move_down(true); // extend down to line 1 col 0 -> offset 5
        assert_eq!(s.cursor(), 5);
        assert_eq!(s.selection_range(), Some((0, 5)));
        s.move_up(true); // back up -> offset 0, empty selection cleared
        assert_eq!(s.cursor(), 0);
        assert!(!s.has_selection());
    }

    // ── Multi-line: newline insertion ─────────────────────────────────────

    #[test]
    fn insert_newline_splits_line() {
        let mut s = TextInputState::with_text("abcd");
        s.home(false);
        s.move_right(false);
        s.move_right(false); // cursor between b and c
        s.insert_newline();
        assert_eq!(s.text(), "ab\ncd");
        assert_eq!(s.cursor(), 3); // just after the '\n'
        assert_eq!(s.line_count(), 2);
    }

    #[test]
    fn insert_char_newline_equivalent() {
        let mut s = TextInputState::with_text("xy");
        s.insert_char('\n');
        assert_eq!(s.text(), "xy\n");
        assert_eq!(s.line_count(), 2);
        assert_eq!(s.cursor(), 3);
    }

    // ── Multi-line: line_home / line_end_move ─────────────────────────────

    #[test]
    fn line_home_and_end_move_are_line_relative() {
        let mut s = TextInputState::with_text("ab\ncde");
        s.end(false); // offset 6 (end of line 1)
        s.line_home(false);
        assert_eq!(s.cursor(), 3); // start of line 1, not 0
        s.line_end_move(false);
        assert_eq!(s.cursor(), 6); // end of line 1
        // Single-line `home` still goes to the very start.
        s.home(false);
        assert_eq!(s.cursor(), 0);
    }

    // ── Multi-line: UTF-8 safety ──────────────────────────────────────────

    #[test]
    fn multibyte_vertical_motion_never_splits_char() {
        // Each line mixes ASCII and 2-byte / 4-byte chars.
        // line0 = "héllo" (h, é=2B, l, l, o) -> 6 bytes, 5 chars
        // line1 = "wörld" (w, ö=2B, r, l, d) -> 6 bytes, 5 chars
        let mut s = TextInputState::with_text("héllo\nwörld");
        // Put cursor at col 3 of line 1.
        s.line_home(false); // start of line 1 (offset 7)
        s.move_right(false);
        s.move_right(false);
        s.move_right(false); // col 3
        assert!(s.text().is_char_boundary(s.cursor()));
        s.move_up(false); // to line 0 col 3
        assert!(s.text().is_char_boundary(s.cursor()));
        assert_eq!(s.column(), 3);
        s.move_down(false); // back to line 1 col 3
        assert!(s.text().is_char_boundary(s.cursor()));
        assert_eq!(s.column(), 3);
    }

    #[test]
    fn emoji_lines_vertical_motion_safe() {
        // 4-byte emoji on each line, differing lengths to force clamping.
        let mut s = TextInputState::with_text("😀😀😀\n😀");
        s.end(false); // last line, after its single emoji (col 1)
        // Walk up: line 0 has 3 emoji, col clamps to <= line length, no panic.
        s.move_up(false);
        assert!(s.text().is_char_boundary(s.cursor()));
        s.move_down(false);
        assert!(s.text().is_char_boundary(s.cursor()));
        // Down on last line clamps to end.
        assert_eq!(s.cursor(), s.text().len());
    }

    #[test]
    fn line_start_end_clamp_out_of_range() {
        let s = TextInputState::with_text("ab\ncd");
        // Beyond len is clamped to len's line.
        assert_eq!(s.line_start(999), 3);
        assert_eq!(s.line_end(999), 5);
    }
