//! **Mask editor** — data-model actions over a layer's Bézier mask stack plus a
//! deterministic rasterizer that bakes the combined masks to an alpha matte.
//!
//! The mask *geometry* (closed Bézier paths, modes, feather / expansion /
//! invert / opacity, even-odd fill, and the per-pixel coverage fold) lives in
//! the engine: [`crate::comp::Mask`] / [`crate::comp::MaskMode`] /
//! [`crate::comp::mask_stack_coverage`]. This file is the **app_state seam**: it
//! turns panel [`Action`]s into edits on a layer's `masks: Vec<Mask>` and offers
//! a CPU rasterizer that flattens the stack into a `width × height` alpha matte
//! (the "combined masks → alpha matte" the mask panel previews / the compositor
//! could bake). The rasterizer is split into a free function so the sampling
//! math is unit-testable without an `App`.

use super::{App, Action};
use crate::comp::{Mask, MaskMode};
use crate::gizmo::LAYER_HALF_FRAC;

/// Box-blur radius (in matte pixels) applied per unit of mask feather when
/// baking the matte. The engine's per-pixel `coverage_at` already ramps the
/// edge across `feather` comp-px; this extra separable box blur softens the
/// *baked* matte further (and matches AE's "Mask Feather" reading more closely
/// for the previewed alpha), scaled so `feather == 0` is a hard, blur-free edge.
const FEATHER_BLUR_PER_PX: f32 = 0.25;

impl App {
    /// Apply a mask-editor [`Action`]. Dispatched from [`App::apply`] for all
    /// `*Mask*` variants. Every arm marks the host dirty when the composite
    /// changes (mask geometry / mode / feather all affect the rendered alpha).
    pub(super) fn apply_masks(&mut self, action: Action) {
        let ci = self.active_comp_index();
        match action {
            Action::AddRectMask { layer_id } => {
                let (hw, hh) = self.layer_mask_half_extents();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    let mut m = Mask::rect(hw, hh);
                    m.name = format!("Mask {}", l.masks.len() + 1);
                    l.masks.push(m);
                    self.host.mark_dirty();
                }
            }
            Action::AddEllipseMask { layer_id } => {
                let (hw, hh) = self.layer_mask_half_extents();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    let mut m = Mask::ellipse(hw, hh);
                    m.name = format!("Mask {}", l.masks.len() + 1);
                    l.masks.push(m);
                    self.host.mark_dirty();
                }
            }
            Action::RemoveMask { layer_id, mask_idx } => {
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if mask_idx < l.masks.len() {
                        l.masks.remove(mask_idx);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AddMaskVertex { layer_id, mask_idx, x, y } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    m.vertices.push(crate::comp::MaskVertex::corner(x, y));
                    self.host.mark_dirty();
                }
            }
            Action::InsertMaskVertex { layer_id, mask_idx, index, x, y } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    let at = index.min(m.vertices.len());
                    m.vertices.insert(at, crate::comp::MaskVertex::corner(x, y));
                    self.host.mark_dirty();
                }
            }
            Action::RemoveMaskVertex { layer_id, mask_idx, vert_idx } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    if vert_idx < m.vertices.len() {
                        m.vertices.remove(vert_idx);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetMaskVertex { layer_id, mask_idx, vert_idx, x, y } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    if let Some(v) = m.vertices.get_mut(vert_idx) {
                        v.x = x;
                        v.y = y;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetMaskVertexHandles { layer_id, mask_idx, vert_idx, in_x, in_y, out_x, out_y } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    if let Some(v) = m.vertices.get_mut(vert_idx) {
                        v.in_x = in_x;
                        v.in_y = in_y;
                        v.out_x = out_x;
                        v.out_y = out_y;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetMaskMode { layer_id, mask_idx, mode } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    m.mode = mode;
                    self.host.mark_dirty();
                }
            }
            Action::SetMaskFeather { layer_id, mask_idx, feather } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    m.feather = feather.max(0.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetMaskOpacity { layer_id, mask_idx, opacity } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    m.opacity = opacity.clamp(0.0, 1.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetMaskExpansion { layer_id, mask_idx, expansion } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    m.expansion = expansion;
                    self.host.mark_dirty();
                }
            }
            Action::SetMaskInverted { layer_id, mask_idx, inverted } => {
                if let Some(m) = self.mask_mut(layer_id, mask_idx) {
                    m.inverted = inverted;
                    self.host.mark_dirty();
                }
            }
            _ => unreachable!("apply_masks called with wrong action"),
        }
    }

    /// Borrow a layer's `mask_idx`-th mask mutably in the active comp, if both
    /// the layer and the mask exist.
    fn mask_mut(&mut self, layer_id: usize, mask_idx: usize) -> Option<&mut Mask> {
        let ci = self.active_comp_index();
        self.project.comps[ci]
            .layers
            .get_mut(layer_id)
            .and_then(|l| l.masks.get_mut(mask_idx))
    }

    /// The layer-local half-extents a default mask (rect / ellipse) is sized to:
    /// the same `LAYER_HALF_FRAC` of the comp the gizmo/renderer use for a
    /// layer's base quad — so a fresh mask hugs the layer.
    fn layer_mask_half_extents(&self) -> (f32, f32) {
        let ci = self.active_comp_index();
        let comp = &self.project.comps[ci];
        (
            comp.width as f32 * LAYER_HALF_FRAC,
            comp.height as f32 * LAYER_HALF_FRAC,
        )
    }

    /// Bake the active comp's `layer_id` mask stack into a `width × height`
    /// **alpha matte** (`Vec<f32>` row-major, `[0, 1]`). Returns `None` when the
    /// layer is gone or has no active masks (callers should treat that as a
    /// fully-opaque matte). The matte is sampled in **layer-local** space (the
    /// frame the masks live in), so it is independent of the layer's world
    /// transform — exactly the matte a mask preview shows.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn rasterize_layer_masks(&self, layer_id: usize, width: usize, height: usize) -> Option<Vec<f32>> {
        let ci = self.active_comp_index();
        let layer = self.project.comps[ci].layers.get(layer_id)?;
        if !layer.has_active_masks() {
            return None;
        }
        let (hw, hh) = self.layer_mask_half_extents();
        Some(rasterize_mask_matte(&layer.masks, width, height, hw, hh))
    }
}

/// Rasterize a layer's `masks` into a `width × height` alpha matte (`[0, 1]`,
/// row-major), sampling the mask stack across the layer-local box
/// `[-half_w, half_w] × [-half_h, half_h]`.
///
/// Each output pixel's layer-local coordinate is folded through the engine's
/// [`mask_stack_coverage`] (even-odd / nonzero fill + per-mask
/// feather/expansion/invert/opacity/mode), then the whole matte is softened by a
/// separable **box blur** whose radius scales with the maximum mask feather (so
/// `feather == 0` leaves a crisp matte). Pure + deterministic: identical inputs
/// always yield an identical matte, so it is straightforward to unit-test.
pub fn rasterize_mask_matte(
    masks: &[Mask],
    width: usize,
    height: usize,
    half_w: f32,
    half_h: f32,
) -> Vec<f32> {
    let mut matte = vec![0.0_f32; width.max(1) * height.max(1)];
    if width == 0 || height == 0 {
        return matte;
    }
    // Flatten each mask once (the hot per-pixel loop reuses the polygons).
    let polys: Vec<Vec<(f32, f32)>> = masks.iter().map(|m| m.flatten()).collect();
    for py in 0..height {
        // Map pixel center → layer-local y in [-half_h, half_h].
        let ly = ((py as f32 + 0.5) / height as f32) * 2.0 * half_h - half_h;
        for px in 0..width {
            let lx = ((px as f32 + 0.5) / width as f32) * 2.0 * half_w - half_w;
            matte[py * width + px] = mask_stack_coverage_local(masks, &polys, lx, ly);
        }
    }
    // Feather the baked matte with a box blur sized off the strongest feather.
    let max_feather = masks
        .iter()
        .filter(|m| m.is_active())
        .map(|m| m.feather)
        .fold(0.0_f32, f32::max);
    if max_feather > 0.0 {
        let radius = (max_feather * FEATHER_BLUR_PER_PX).round() as usize;
        if radius > 0 {
            box_blur(&mut matte, width, height, radius);
        }
    }
    matte
}

/// Fold the mask stack's coverage at layer-local `(lx, ly)` into `[0, 1]`,
/// delegating each mask's even-odd coverage + mode combination to the engine
/// (mirrors [`crate::comp::mask_stack_coverage`] but takes the pre-flattened
/// polygons so the matte loop flattens once).
fn mask_stack_coverage_local(masks: &[Mask], polys: &[Vec<(f32, f32)>], lx: f32, ly: f32) -> f32 {
    let mut acc = 0.0;
    let mut any = false;
    for (mask, poly) in masks.iter().zip(polys.iter()) {
        if !mask.is_active() {
            continue;
        }
        any = true;
        let cov = mask.coverage_at(poly, lx, ly);
        acc = mask.mode.combine(acc, cov);
    }
    if any {
        acc
    } else {
        1.0
    }
}

/// In-place **separable box blur** of a single-channel `width × height` buffer,
/// `radius` pixels each side, edges clamped. Two passes (horizontal then
/// vertical) give an approximate Gaussian — the standard cheap mask-feather
/// softener. A `radius == 0` is a no-op.
fn box_blur(buf: &mut [f32], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let win = (radius * 2 + 1) as f32;
    // Horizontal pass into a scratch buffer.
    let mut tmp = vec![0.0_f32; buf.len()];
    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let mut sum = 0.0;
            for k in 0..=(radius * 2) {
                let sx = (x + k).saturating_sub(radius).min(width - 1);
                sum += buf[row + sx];
            }
            tmp[row + x] = sum / win;
        }
    }
    // Vertical pass back into `buf`.
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for k in 0..=(radius * 2) {
                let sy = (y + k).saturating_sub(radius).min(height - 1);
                sum += tmp[sy * width + x];
            }
            buf[y * width + x] = sum / win;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh app whose layer 0 starts with an **empty** mask stack (the demo
    /// project seeds a soft oval on layer 0; clearing it gives the mask-editor
    /// tests a known-empty baseline).
    fn app_with_layer() -> App {
        let mut app = App::new();
        let ci = app.active_comp_index();
        app.project.comps[ci].layers[0].masks.clear();
        app
    }

    #[test]
    fn add_rect_mask_sizes_and_names() {
        let mut app = app_with_layer();
        app.apply(Action::AddRectMask { layer_id: 0 });
        let ci = app.active_comp_index();
        let masks = &app.project.comps[ci].layers[0].masks;
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0].vertices.len(), 4, "rect mask has 4 corners");
        assert_eq!(masks[0].name, "Mask 1");
        assert!(app.can_undo());
    }

    #[test]
    fn add_ellipse_then_remove() {
        let mut app = app_with_layer();
        app.apply(Action::AddEllipseMask { layer_id: 0 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].masks.len(), 1);
        assert_eq!(app.project.comps[ci].layers[0].masks[0].vertices.len(), 4);
        app.apply(Action::RemoveMask { layer_id: 0, mask_idx: 0 });
        let ci = app.active_comp_index();
        assert!(app.project.comps[ci].layers[0].masks.is_empty());
    }

    #[test]
    fn vertex_add_move_remove() {
        let mut app = app_with_layer();
        app.apply(Action::AddRectMask { layer_id: 0 });
        app.apply(Action::AddMaskVertex { layer_id: 0, mask_idx: 0, x: 10.0, y: 20.0 });
        let ci = app.active_comp_index();
        let m = &app.project.comps[ci].layers[0].masks[0];
        assert_eq!(m.vertices.len(), 5);
        assert!((m.vertices[4].x - 10.0).abs() < 1e-4);

        app.apply(Action::SetMaskVertex { layer_id: 0, mask_idx: 0, vert_idx: 4, x: 33.0, y: -7.0 });
        let ci = app.active_comp_index();
        let v = app.project.comps[ci].layers[0].masks[0].vertices[4];
        assert!((v.x - 33.0).abs() < 1e-4 && (v.y + 7.0).abs() < 1e-4);

        app.apply(Action::RemoveMaskVertex { layer_id: 0, mask_idx: 0, vert_idx: 4 });
        let ci = app.active_comp_index();
        assert_eq!(app.project.comps[ci].layers[0].masks[0].vertices.len(), 4);
    }

    #[test]
    fn insert_vertex_at_index() {
        let mut app = app_with_layer();
        app.apply(Action::AddRectMask { layer_id: 0 });
        app.apply(Action::InsertMaskVertex { layer_id: 0, mask_idx: 0, index: 1, x: 5.0, y: 6.0 });
        let ci = app.active_comp_index();
        let m = &app.project.comps[ci].layers[0].masks[0];
        assert_eq!(m.vertices.len(), 5);
        assert!((m.vertices[1].x - 5.0).abs() < 1e-4);
    }

    #[test]
    fn set_mode_feather_opacity_expansion_invert() {
        let mut app = app_with_layer();
        app.apply(Action::AddRectMask { layer_id: 0 });
        app.apply(Action::SetMaskMode { layer_id: 0, mask_idx: 0, mode: MaskMode::Subtract });
        app.apply(Action::SetMaskFeather { layer_id: 0, mask_idx: 0, feather: 8.0 });
        app.apply(Action::SetMaskOpacity { layer_id: 0, mask_idx: 0, opacity: 0.5 });
        app.apply(Action::SetMaskExpansion { layer_id: 0, mask_idx: 0, expansion: -4.0 });
        app.apply(Action::SetMaskInverted { layer_id: 0, mask_idx: 0, inverted: true });
        let ci = app.active_comp_index();
        let m = &app.project.comps[ci].layers[0].masks[0];
        assert_eq!(m.mode, MaskMode::Subtract);
        assert!((m.feather - 8.0).abs() < 1e-4);
        assert!((m.opacity - 0.5).abs() < 1e-4);
        assert!((m.expansion + 4.0).abs() < 1e-4);
        assert!(m.inverted);
    }

    #[test]
    fn feather_and_opacity_clamp() {
        let mut app = app_with_layer();
        app.apply(Action::AddRectMask { layer_id: 0 });
        app.apply(Action::SetMaskFeather { layer_id: 0, mask_idx: 0, feather: -3.0 });
        app.apply(Action::SetMaskOpacity { layer_id: 0, mask_idx: 0, opacity: 2.0 });
        let ci = app.active_comp_index();
        let m = &app.project.comps[ci].layers[0].masks[0];
        assert!((m.feather).abs() < 1e-4, "feather clamps to >= 0");
        assert!((m.opacity - 1.0).abs() < 1e-4, "opacity clamps to <= 1");
    }

    #[test]
    fn rasterize_add_mask_center_inside_corner_outside() {
        // A small centered rect mask: the matte is ~1 in the middle and 0 in the
        // far corners (the rect only covers the central LAYER_HALF_FRAC region).
        let masks = vec![Mask::rect(40.0, 40.0)];
        let m = rasterize_mask_matte(&masks, 64, 64, 100.0, 100.0);
        let center = m[32 * 64 + 32];
        let corner = m[0]; // top-left pixel: layer-local ≈ (-100, -100), outside ±40
        assert!(center > 0.9, "center inside the add-mask is opaque, got {center}");
        assert!(corner < 0.1, "corner outside the mask is transparent, got {corner}");
    }

    #[test]
    fn rasterize_inverted_flips_coverage() {
        let mut mask = Mask::rect(40.0, 40.0);
        mask.inverted = true;
        let m = rasterize_mask_matte(&[mask], 64, 64, 100.0, 100.0);
        let center = m[32 * 64 + 32];
        let corner = m[0];
        assert!(center < 0.1, "inverted: center is transparent, got {center}");
        assert!(corner > 0.9, "inverted: corner is opaque, got {corner}");
    }

    #[test]
    fn rasterize_subtract_knocks_out_overlap() {
        // Add a big rect, then subtract a smaller centered one: the center is
        // knocked out (transparent) while a mid-radius point stays covered.
        let masks = vec![Mask::rect(90.0, 90.0), {
            let mut s = Mask::rect(30.0, 30.0);
            s.mode = MaskMode::Subtract;
            s
        }];
        let m = rasterize_mask_matte(&masks, 64, 64, 100.0, 100.0);
        let center = m[32 * 64 + 32]; // inside the subtract hole
        assert!(center < 0.1, "subtract knocks out the center, got {center}");
    }

    #[test]
    fn rasterize_none_active_is_full() {
        // No active masks → fully-opaque matte (the unmasked case).
        let masks: Vec<Mask> = Vec::new();
        let m = rasterize_mask_matte(&masks, 8, 8, 50.0, 50.0);
        assert!(m.iter().all(|&v| (v - 1.0).abs() < 1e-6), "empty stack = full coverage");
    }

    #[test]
    fn rasterize_through_app_returns_none_without_masks() {
        // Layer 0 with no active masks bakes to no matte (the unmasked case).
        let app = app_with_layer();
        assert!(app.rasterize_layer_masks(0, 16, 16).is_none());
    }

    #[test]
    fn rasterize_through_app_bakes_matte() {
        let mut app = App::new();
        app.apply(Action::AddRectMask { layer_id: 0 });
        let m = app.rasterize_layer_masks(0, 32, 32).expect("matte baked");
        assert_eq!(m.len(), 32 * 32);
        // The default rect mask hugs the layer (LAYER_HALF_FRAC), so the matte
        // center is opaque.
        assert!(m[16 * 32 + 16] > 0.9);
    }

    #[test]
    fn feather_softens_baked_edge() {
        // A feathered mask's baked matte has intermediate values near the edge
        // (the box blur spreads coverage), unlike the hard 0/1 of feather 0.
        let mut mask = Mask::rect(50.0, 50.0);
        mask.feather = 30.0;
        let m = rasterize_mask_matte(&[mask], 64, 64, 100.0, 100.0);
        // Some pixel must be a soft partial (strictly between 0 and 1).
        assert!(
            m.iter().any(|&v| v > 0.05 && v < 0.95),
            "feathered matte has soft partial-coverage pixels"
        );
    }
}
