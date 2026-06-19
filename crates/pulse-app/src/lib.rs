//! Pulse — motion-graphics / compositing app, app #3 of the Prism creative
//! suite.
//!
//! The crate is split into a `lib` (the document model, software compositor,
//! and the egui app itself behind `egui-ui` feature) and a thin `bin`
//! (`main.rs`) so the GPUI host in the sibling `pulse-gpui` crate can reuse
//! the comp model and CPU compositor without pulling in egui.

// egui 0.34 deprecates several menu/panel aliases mid-cycle; silence the churn.
#![allow(deprecated)]

// Pure logic — always compiled.
pub mod comp;
pub mod gizmo;
pub mod render;

// egui UI — only when the `egui-ui` feature is enabled.
#[cfg(feature = "egui-ui")]
pub mod app;
#[cfg(feature = "egui-ui")]
pub mod graph;
#[cfg(feature = "egui-ui")]
pub mod icons;
#[cfg(feature = "egui-ui")]
pub mod onion;
#[cfg(feature = "egui-ui")]
pub mod preview;
#[cfg(feature = "egui-ui")]
pub mod theme;
#[cfg(feature = "egui-ui")]
pub mod timeline;
