//! The GPU compositor for [`CanvasGpu`]. All logic is inherent methods on
//! `CanvasGpu` (the type and its fields are defined in the crate root), split by
//! concern across sibling modules:
//! - [`filters_core`] — `filter_pass*` primitives + smart-filter source/restore
//! - [`filters_apply`] — the per-filter destructive `apply_*` methods
//! - [`composite`] — the ping-pong layer compositor + readback + display bind group
//!
//! Splitting only moves method bodies between files; the public surface of
//! `CanvasGpu` is unchanged.

mod composite;
mod filters_apply;
mod filters_core;
