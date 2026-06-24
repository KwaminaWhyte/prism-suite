//! Reusable text-input widget for the Prism suite.
//!
//! Split into two layers:
//! * [`state`] — [`TextInputState`], the pure, GPUI-free, fully unit-tested
//!   editing model (text buffer + cursor + selection, all UTF-8 safe).
//! * [`field`] — [`TextField`], a focusable single-line GPUI view that wraps the
//!   state core, handles key events, and renders text + caret + selection using
//!   prism-ui's design tokens.

pub mod field;
pub mod state;

pub use field::TextField;
pub use state::TextInputState;
