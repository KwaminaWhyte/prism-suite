//! Per-layer rasterization passes split by group for the workspace size rule:
//! [`composite`] holds the content-compositing passes (shape / text / footage /
//! precomp / motion-blur / layer / generate) and [`apply`] holds the finishing
//! passes (adjustment / matte / masks / key / spatial / DoF / stylize / distort /
//! puppet). Both are re-exported here so `render/mod.rs` keeps importing the pass
//! functions from `passes::…` exactly as when they lived in `passes.rs`.

mod apply;
mod composite;

pub(crate) use apply::{
    apply_adjustment, apply_distort, apply_dof, apply_key, apply_masks, apply_puppet_warp,
    apply_spatial, apply_stylize, apply_track_matte,
};
pub(crate) use composite::{
    composite_footage, composite_generate, composite_layer, composite_motion_blur,
    composite_precomp, composite_shape, composite_text, decode_footage,
};
