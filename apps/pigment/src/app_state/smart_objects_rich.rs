//! Rich (non-destructive) embedded Smart Objects.
//!
//! The lightweight [`SmartObject`](super::SmartObject) in `smart_objects.rs`
//! tracks bookkeeping (id / name / dirty flag). This module adds the *actual*
//! non-destructive content: an [`EmbeddedSmartObject`] stores the source pixels
//! captured when a layer was wrapped, plus a re-applicable
//! [`SmartTransform`](self) (scale / rotation / skew / translate) and an ordered
//! [`SmartFilterStack`](self). "Update / rasterize" re-renders
//! `source → transform → filters` into a fresh output buffer entirely on the CPU.
//!
//! The render is deterministic and GPU-free (it reuses the pure-function CPU
//! filters in `crate::filters`), so the whole pipeline is unit-testable without a
//! wgpu adapter. When a GPU device *is* present, [`App::update_smart_object`]
//! also uploads the rendered pixels into the bound layer; without one, it still
//! produces and stores the rendered buffer.
//!
//! Pixel format throughout: straight (un-premultiplied) sRGB-ish RGBA `f32`,
//! row-major, length `w*h*4`. We keep colour straight here (not premultiplied)
//! because the affine warp samples and the bookkeeping are simpler and the only
//! consumers are tests + an optional upload that converts as needed.

use super::{Action, App};

/// A re-applicable affine transform for a Smart Object's source. Forward map:
/// translate ∘ rotate ∘ skew ∘ scale about the source centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmartTransform {
    /// Uniform scale (1.0 = identity). Applied to both axes.
    pub scale: f32,
    /// Rotation in degrees (clockwise, about the centre).
    pub rotation_deg: f32,
    /// Horizontal skew in degrees.
    pub skew_x_deg: f32,
    /// Vertical skew in degrees.
    pub skew_y_deg: f32,
    /// Translation in output pixels (x, y).
    pub translate: [f32; 2],
}

impl Default for SmartTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation_deg: 0.0,
            skew_x_deg: 0.0,
            skew_y_deg: 0.0,
            translate: [0.0, 0.0],
        }
    }
}

impl SmartTransform {
    pub fn is_identity(&self) -> bool {
        *self == SmartTransform::default()
    }

    /// The 2×2 *forward* matrix (rotation ∘ skew ∘ scale), row-major
    /// `[a, b, c, d]` mapping source-centred coords to output-centred coords.
    fn forward_matrix(&self) -> [f32; 4] {
        let s = self.scale.max(1e-4);
        let r = self.rotation_deg.to_radians();
        let (cr, sr) = (r.cos(), r.sin());
        let kx = self.skew_x_deg.clamp(-89.0, 89.0).to_radians().tan();
        let ky = self.skew_y_deg.clamp(-89.0, 89.0).to_radians().tan();
        // scale
        let (sa, sb, sc, sd) = (s, 0.0, 0.0, s);
        // skew * scale
        let (ka, kb, kc, kd) = (
            sa + kx * sc,
            sb + kx * sd,
            ky * sa + sc,
            ky * sb + sd,
        );
        // rotate * (skew*scale)
        [
            cr * ka - sr * kc,
            cr * kb - sr * kd,
            sr * ka + cr * kc,
            sr * kb + cr * kd,
        ]
    }

    /// Invert a 2×2 matrix `[a,b,c,d]`; returns identity if singular.
    fn invert(m: [f32; 4]) -> [f32; 4] {
        let det = m[0] * m[3] - m[1] * m[2];
        if det.abs() < 1e-8 {
            return [1.0, 0.0, 0.0, 1.0];
        }
        let inv = 1.0 / det;
        [m[3] * inv, -m[1] * inv, -m[2] * inv, m[0] * inv]
    }
}

/// One non-destructive filter applied during a Smart Object render. Mirrors the
/// `crate::filters` pure functions so the stack is GPU-free and deterministic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmartFilter {
    /// Solarize (invert tones above `threshold` 0..1).
    Solarize { threshold: f32 },
    /// High Pass detail isolation, `radius` px.
    HighPass { radius: f32 },
    /// Glowing Edges (`width` step, `intensity` brightness).
    GlowingEdges { width: f32, intensity: f32 },
    /// Directional Motion Blur (`angle` deg, `distance` px).
    MotionBlur { angle: f32, distance: f32 },
    /// Twirl about the centre (`angle` deg over `radius` fraction).
    Twirl { angle: f32, radius: f32 },
}

impl SmartFilter {
    /// Apply this filter to a *premultiplied* RGBA f32 buffer of size `w*h`.
    fn apply(&self, px: &[f32], w: u32, h: u32) -> Vec<f32> {
        match *self {
            SmartFilter::Solarize { threshold } => crate::filters::solarize(px, threshold),
            SmartFilter::HighPass { radius } => crate::filters::high_pass(px, w, h, radius),
            SmartFilter::GlowingEdges { width, intensity } => {
                crate::filters::glowing_edges(px, w, h, width, intensity)
            }
            SmartFilter::MotionBlur { angle, distance } => {
                crate::filters::motion_blur(px, w, h, angle, distance)
            }
            SmartFilter::Twirl { angle, radius } => crate::filters::twirl(px, w, h, angle, radius),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SmartFilter::Solarize { .. } => "Solarize",
            SmartFilter::HighPass { .. } => "High Pass",
            SmartFilter::GlowingEdges { .. } => "Glowing Edges",
            SmartFilter::MotionBlur { .. } => "Motion Blur",
            SmartFilter::Twirl { .. } => "Twirl",
        }
    }
}

/// Ordered, re-applicable filter stack.
pub type SmartFilterStack = Vec<SmartFilter>;

/// A fully non-destructive embedded Smart Object: the captured source pixels plus
/// the live transform + filter stack, and a cache of the most recent render.
#[derive(Debug, Clone)]
pub struct EmbeddedSmartObject {
    pub id: usize,
    pub name: String,
    /// The layer this Smart Object renders back into (its raw `LayerId` u64).
    pub bound_layer: u64,
    /// Source width/height in pixels.
    pub src_w: u32,
    pub src_h: u32,
    /// Captured source pixels: straight RGBA f32, row-major, `src_w*src_h*4`.
    pub source: Vec<f32>,
    /// Live non-destructive transform.
    pub transform: SmartTransform,
    /// Live non-destructive filter stack.
    pub filters: SmartFilterStack,
    /// Whether the cached render is stale (transform/filters changed since render).
    pub dirty: bool,
    /// Last rendered output (straight RGBA f32, same dims as source).
    pub rendered: Vec<f32>,
}

impl EmbeddedSmartObject {
    /// Wrap a captured source buffer into a fresh Smart Object (identity transform,
    /// no filters). The initial render equals the source.
    pub fn from_source(id: usize, name: impl Into<String>, bound_layer: u64, w: u32, h: u32, source: Vec<f32>) -> Self {
        let rendered = source.clone();
        Self {
            id,
            name: name.into(),
            bound_layer,
            src_w: w,
            src_h: h,
            source,
            transform: SmartTransform::default(),
            filters: Vec::new(),
            dirty: false,
            rendered,
        }
    }

    /// Re-render `source → transform → filters` into a fresh buffer (same dims),
    /// store it in `rendered`, clear `dirty`, and return a reference to it.
    pub fn render(&mut self) -> &[f32] {
        let warped = warp_affine(&self.source, self.src_w, self.src_h, &self.transform);
        // Filters operate on premultiplied color; convert in/out around the stack.
        let mut buf = if self.filters.is_empty() {
            warped
        } else {
            let mut pm = straight_to_premul(&warped);
            for f in &self.filters {
                pm = f.apply(&pm, self.src_w, self.src_h);
            }
            premul_to_straight(&pm)
        };
        // Defensive: keep the output length exact.
        buf.truncate((self.src_w * self.src_h * 4) as usize);
        self.rendered = buf;
        self.dirty = false;
        &self.rendered
    }
}

/// Affine-warp a straight RGBA f32 source about its centre using `xf`'s forward
/// matrix + translation, sampling with clamped bilinear into a same-sized output.
/// Identity transform is a fast clone.
fn warp_affine(src: &[f32], w: u32, h: u32, xf: &SmartTransform) -> Vec<f32> {
    if xf.is_identity() {
        return src.to_vec();
    }
    let inv = SmartTransform::invert(xf.forward_matrix());
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    let (tx, ty) = (xf.translate[0], xf.translate[1]);
    let mut out = vec![0.0f32; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            // Output coords centred, minus translate, mapped back through inverse.
            let ox = x as f32 + 0.5 - cx - tx;
            let oy = y as f32 + 0.5 - cy - ty;
            let sx = inv[0] * ox + inv[1] * oy + cx;
            let sy = inv[2] * ox + inv[3] * oy + cy;
            let c = bilinear_straight(src, w, h, sx - 0.5, sy - 0.5);
            let i = ((y * w + x) * 4) as usize;
            out[i] = c[0];
            out[i + 1] = c[1];
            out[i + 2] = c[2];
            out[i + 3] = c[3];
        }
    }
    out
}

/// Clamped bilinear sample of a straight RGBA f32 buffer.
fn bilinear_straight(px: &[f32], w: u32, h: u32, x: f32, y: f32) -> [f32; 4] {
    let (wi, hi) = (w as i32, h as i32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0f32; 4];
    for dy in 0..2i32 {
        for dx in 0..2i32 {
            let sx = (x0 + dx).clamp(0, wi - 1);
            let sy = (y0 + dy).clamp(0, hi - 1);
            let wb = (if dx == 0 { 1.0 - tx } else { tx }) * (if dy == 0 { 1.0 - ty } else { ty });
            let i = ((sy as u32 * w + sx as u32) * 4) as usize;
            for c in 0..4 {
                out[c] += px[i + c] * wb;
            }
        }
    }
    out
}

/// Straight RGBA → premultiplied RGBA.
fn straight_to_premul(px: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0f32; px.len()];
    let n = px.len() / 4;
    for p in 0..n {
        let i = p * 4;
        let a = px[i + 3];
        out[i] = px[i] * a;
        out[i + 1] = px[i + 1] * a;
        out[i + 2] = px[i + 2] * a;
        out[i + 3] = a;
    }
    out
}

/// Premultiplied RGBA → straight RGBA.
fn premul_to_straight(px: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0f32; px.len()];
    let n = px.len() / 4;
    for p in 0..n {
        let i = p * 4;
        let a = px[i + 3];
        if a > 1e-6 {
            out[i] = px[i] / a;
            out[i + 1] = px[i + 1] / a;
            out[i + 2] = px[i + 2] / a;
        }
        out[i + 3] = a;
    }
    out
}

impl App {
    /// Find an embedded Smart Object by id.
    pub fn embedded_so(&self, id: usize) -> Option<&EmbeddedSmartObject> {
        self.embedded_smart_objects.iter().find(|s| s.id == id)
    }

    fn embedded_so_mut(&mut self, id: usize) -> Option<&mut EmbeddedSmartObject> {
        self.embedded_smart_objects.iter_mut().find(|s| s.id == id)
    }

    /// Capture a layer's pixels into a new embedded Smart Object. With a GPU
    /// device the source is read from the bound layer; without one (tests), a
    /// flat transparent buffer of the document size is captured so the pipeline
    /// stays exercisable. Returns the new Smart Object's id.
    pub fn embed_layer_as_smart_object(&mut self, layer: u64, name: impl Into<String>) -> usize {
        let (w, h) = (self.doc.size.width.max(1), self.doc.size.height.max(1));
        let source = self
            .host
            .read_layer_f32(prism_core::LayerId(layer))
            .map(|pm| premul_to_straight(&pm))
            .unwrap_or_else(|| vec![0.0f32; (w * h * 4) as usize]);
        let id = self.embedded_so_counter;
        self.embedded_so_counter += 1;
        self.embedded_smart_objects
            .push(EmbeddedSmartObject::from_source(id, name, layer, w, h, source));
        id
    }

    /// Re-render an embedded Smart Object and (if its dimensions match the host
    /// canvas) upload its result back into the bound layer. Returns whether the
    /// object existed. The upload is skipped when the rendered buffer's size does
    /// not match the current canvas — `upload_layer` writes a full-canvas row
    /// layout, so a mismatched buffer would be rejected by the GPU. Real embeds
    /// capture at the document size and therefore always upload.
    pub fn update_smart_object(&mut self, id: usize) -> bool {
        // Render first (borrow ends), then upload.
        let (layer, premul) = {
            let Some(so) = self.embedded_so_mut(id) else {
                return false;
            };
            let straight = so.render().to_vec();
            (so.bound_layer, straight_to_premul(&straight))
        };
        let canvas_len = (self.host.doc_w as usize) * (self.host.doc_h as usize) * 4;
        if premul.len() == canvas_len {
            self.host
                .upload_layer_f32(prism_core::LayerId(layer), &premul);
        }
        self.status_message = Some(format!("Smart Object {id} updated"));
        true
    }

    pub(super) fn apply_smart_objects_rich(&mut self, action: Action) {
        match action {
            Action::EmbedSmartObject { layer_id } => {
                let id = self.embed_layer_as_smart_object(layer_id as u64, format!("Smart Object {}", self.embedded_so_counter));
                self.status_message = Some(format!("Embedded Smart Object {id}"));
            }
            Action::SetSmartObjectScale { so_id, scale } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    so.transform.scale = scale.clamp(0.05, 16.0);
                    so.dirty = true;
                }
            }
            Action::SetSmartObjectRotation { so_id, degrees } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    let mut a = degrees % 360.0;
                    if a > 180.0 {
                        a -= 360.0;
                    } else if a < -180.0 {
                        a += 360.0;
                    }
                    so.transform.rotation_deg = a;
                    so.dirty = true;
                }
            }
            Action::SetSmartObjectSkew { so_id, skew_x, skew_y } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    so.transform.skew_x_deg = skew_x.clamp(-89.0, 89.0);
                    so.transform.skew_y_deg = skew_y.clamp(-89.0, 89.0);
                    so.dirty = true;
                }
            }
            Action::SetSmartObjectTranslate { so_id, x, y } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    so.transform.translate = [x, y];
                    so.dirty = true;
                }
            }
            Action::AddSmartObjectFilter { so_id, filter } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    so.filters.push(filter);
                    so.dirty = true;
                }
            }
            Action::ClearSmartObjectFilters { so_id } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    so.filters.clear();
                    so.dirty = true;
                }
            }
            Action::ResetSmartObjectTransform { so_id } => {
                if let Some(so) = self.embedded_so_mut(so_id) {
                    so.transform = SmartTransform::default();
                    so.dirty = true;
                }
            }
            Action::UpdateSmartObject { so_id } => {
                self.update_smart_object(so_id);
            }
            Action::BakeSmartObject { so_id } => {
                // Render once, upload, then remove the embedded object (its pixels
                // are now committed to the layer — non-destructive editing ends).
                self.update_smart_object(so_id);
                self.embedded_smart_objects.retain(|s| s.id != so_id);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::Action;

    /// A small distinctive checker source: red top-left quadrant, rest opaque grey.
    fn checker_source(w: u32, h: u32) -> Vec<f32> {
        let mut v = vec![0.0f32; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let red = x < w / 2 && y < h / 2;
                v[i] = if red { 1.0 } else { 0.5 };
                v[i + 1] = if red { 0.0 } else { 0.5 };
                v[i + 2] = if red { 0.0 } else { 0.5 };
                v[i + 3] = 1.0;
            }
        }
        v
    }

    #[test]
    fn transform_default_is_identity() {
        assert!(SmartTransform::default().is_identity());
        let t = SmartTransform { scale: 2.0, ..Default::default() };
        assert!(!t.is_identity());
    }

    #[test]
    fn identity_render_equals_source() {
        let src = checker_source(8, 8);
        let mut so = EmbeddedSmartObject::from_source(0, "x", 1, 8, 8, src.clone());
        let out = so.render().to_vec();
        assert_eq!(out, src);
        assert!(!so.dirty);
    }

    #[test]
    fn render_output_dims_preserved() {
        let src = checker_source(10, 6);
        let mut so = EmbeddedSmartObject::from_source(0, "x", 1, 10, 6, src);
        so.transform.scale = 1.7;
        so.transform.rotation_deg = 33.0;
        let out = so.render().to_vec();
        assert_eq!(out.len(), 10 * 6 * 4);
    }

    #[test]
    fn rotation_180_flips_red_quadrant() {
        // A 180° rotation moves the top-left red quadrant to the bottom-right.
        let (w, h) = (8u32, 8u32);
        let src = checker_source(w, h);
        let mut so = EmbeddedSmartObject::from_source(0, "x", 1, w, h, src);
        so.transform.rotation_deg = 180.0;
        let out = so.render();
        // Sample bottom-right pixel — should now be red-ish.
        let i = (((h - 1) * w + (w - 1)) * 4) as usize;
        assert!(out[i] > 0.8, "expected red at BR after 180°: {:?}", &out[i..i + 4]);
        assert!(out[i + 1] < 0.2);
    }

    #[test]
    fn scale_changes_content() {
        let src = checker_source(16, 16);
        let mut so = EmbeddedSmartObject::from_source(0, "x", 1, 16, 16, src.clone());
        so.transform.scale = 2.0;
        let out = so.render().to_vec();
        assert_ne!(out, src, "2x scale must change the rendered buffer");
        assert_eq!(out.len(), src.len());
    }

    #[test]
    fn filter_stack_changes_output() {
        let src = checker_source(8, 8);
        let mut so = EmbeddedSmartObject::from_source(0, "x", 1, 8, 8, src.clone());
        so.filters.push(SmartFilter::Solarize { threshold: 0.3 });
        let out = so.render().to_vec();
        assert_ne!(out, src);
    }

    #[test]
    fn embed_creates_object_with_doc_dims() {
        let mut app = App::new();
        let layer = app.doc.active_layer.unwrap().0;
        let id = app.embed_layer_as_smart_object(layer, "SO");
        let so = app.embedded_so(id).unwrap();
        assert_eq!(so.src_w, app.doc.size.width);
        assert_eq!(so.src_h, app.doc.size.height);
        assert_eq!(so.bound_layer, layer);
    }

    #[test]
    fn action_embed_and_transform_marks_dirty() {
        let mut app = App::new();
        app.apply(Action::EmbedSmartObject { layer_id: 0 });
        let id = app.embedded_smart_objects[0].id;
        app.apply(Action::SetSmartObjectScale { so_id: id, scale: 3.0 });
        let so = app.embedded_so(id).unwrap();
        assert_eq!(so.transform.scale, 3.0);
        assert!(so.dirty);
    }

    #[test]
    fn action_scale_clamped() {
        let mut app = App::new();
        app.apply(Action::EmbedSmartObject { layer_id: 0 });
        let id = app.embedded_smart_objects[0].id;
        app.apply(Action::SetSmartObjectScale { so_id: id, scale: 999.0 });
        assert_eq!(app.embedded_so(id).unwrap().transform.scale, 16.0);
    }

    #[test]
    fn action_add_filter_and_clear() {
        let mut app = App::new();
        app.apply(Action::EmbedSmartObject { layer_id: 0 });
        let id = app.embedded_smart_objects[0].id;
        app.apply(Action::AddSmartObjectFilter { so_id: id, filter: SmartFilter::HighPass { radius: 2.0 } });
        assert_eq!(app.embedded_so(id).unwrap().filters.len(), 1);
        app.apply(Action::ClearSmartObjectFilters { so_id: id });
        assert!(app.embedded_so(id).unwrap().filters.is_empty());
    }

    #[test]
    fn action_reset_transform() {
        let mut app = App::new();
        app.apply(Action::EmbedSmartObject { layer_id: 0 });
        let id = app.embedded_smart_objects[0].id;
        app.apply(Action::SetSmartObjectRotation { so_id: id, degrees: 90.0 });
        app.apply(Action::ResetSmartObjectTransform { so_id: id });
        assert!(app.embedded_so(id).unwrap().transform.is_identity());
    }

    #[test]
    fn action_rotation_wraps() {
        let mut app = App::new();
        app.apply(Action::EmbedSmartObject { layer_id: 0 });
        let id = app.embedded_smart_objects[0].id;
        app.apply(Action::SetSmartObjectRotation { so_id: id, degrees: 270.0 });
        assert!((app.embedded_so(id).unwrap().transform.rotation_deg - (-90.0)).abs() < 1e-3);
    }

    #[test]
    fn update_renders_and_clears_dirty() {
        let mut app = App::new();
        // Build a Smart Object directly with a real source so render is meaningful.
        let src = checker_source(8, 8);
        app.embedded_smart_objects
            .push(EmbeddedSmartObject::from_source(0, "x", 0, 8, 8, src));
        app.embedded_so_counter = 1;
        app.apply(Action::SetSmartObjectScale { so_id: 0, scale: 1.5 });
        assert!(app.embedded_so(0).unwrap().dirty);
        // update_smart_object renders; without a GPU the upload is a silent no-op.
        let existed = app.update_smart_object(0);
        assert!(existed);
        assert!(!app.embedded_so(0).unwrap().dirty);
    }

    #[test]
    fn bake_removes_object() {
        let mut app = App::new();
        let src = checker_source(4, 4);
        app.embedded_smart_objects
            .push(EmbeddedSmartObject::from_source(0, "x", 0, 4, 4, src));
        app.embedded_so_counter = 1;
        app.apply(Action::BakeSmartObject { so_id: 0 });
        assert!(app.embedded_so(0).is_none());
    }

    #[test]
    fn premul_round_trip() {
        let straight = vec![1.0, 0.5, 0.25, 0.5, 0.2, 0.4, 0.6, 1.0];
        let back = premul_to_straight(&straight_to_premul(&straight));
        for (a, b) in straight.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }
}
