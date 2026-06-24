//! Reusable text-input widget for the Prism suite.
//!
//! Split into layers:
//! * [`state`] — [`TextInputState`], the pure, GPUI-free, fully unit-tested
//!   editing model (text buffer + cursor + selection, all UTF-8 safe). It also
//!   exposes line-aware helpers (`line_start`/`line_end`, `move_up`/`move_down`,
//!   `insert_newline`, …) consumed by the multi-line view.
//! * [`field`] — [`TextField`], a focusable single-line GPUI view that wraps the
//!   state core, handles key events, and renders text + caret + selection using
//!   prism-ui's design tokens.
//! * [`area`] — [`TextArea`], the multi-line companion to [`TextField`]: same
//!   state core, vertical caret motion, multi-line rendering, Cmd/Ctrl+Enter
//!   submit (plain Enter inserts a newline).

pub mod area;
pub mod field;
pub mod state;

pub use area::TextArea;
pub use field::TextField;
pub use state::TextInputState;
