//! Contour — vector graphics editor, app #2 of the Prism creative suite.
//!
//! Pure-logic library: document model, CPU rasterizer, and all editor logic.
//! The GPUI host (`contour-gpui`) depends on this crate as a library. The old
//! eframe/egui binary has been removed; the GPUI binary is the shipping app.

#![allow(deprecated)]

pub mod ai_eps;
pub mod align;
pub mod appearance;
pub mod arrange;
pub mod artboard;
pub mod blend;
pub mod boolean;
pub mod clip;
pub mod clipboard;
pub mod document;
pub mod effects;
pub mod envelope;
pub mod export;
pub mod eyedropper;
pub mod fonts;
pub mod gradient;
pub mod graphic_styles;
pub mod mesh_gradient;
pub mod group;
pub mod history;
pub mod layers;
pub mod liveshape;
pub mod opacity_mask;
pub mod pathedit;
pub mod perspective;
pub mod placed_image;
pub mod recolor;
pub mod text_on_path;
pub mod shapebuilder;
pub mod snap;
pub mod stroke;
pub mod swatches;
pub mod symbols;
pub mod text;
pub mod trace;
pub mod transform;
pub mod workspace;
