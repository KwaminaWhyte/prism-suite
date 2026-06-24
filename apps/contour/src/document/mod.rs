//! Contour's vector document model.
//!
//! A document is an ordered `Vec<Shape>` (paint order: index 0 painted first,
//! last index on top). All coordinates are in *document space*; the canvas maps
//! them to screen via pan/zoom. Colors are straight sRGB RGBA in `[f32; 4]`
//! (matching egui's `Rgba`/`Color32` channel order) so they round-trip cleanly
//! through the color pickers and JSON.

mod path;
mod style;
mod shape_methods_meta;
mod shape_methods_geom;
#[cfg(test)]
mod tests;

pub use path::{
    anchors_in_rect, bez_path, flatten, handle_at, handle_endpoints, nearest_segment,
    point_in_rings, rects_intersect, FillRule, SubPath,
};
pub use style::{Arrowhead, LineCap, LineJoin, StrokeAlign, StrokeStyle};

use crate::text::TextParams;

use crate::appearance::Appearance;
use crate::artboard::{self, Artboard};
use crate::gradient::Gradient;
use crate::graphic_styles::GraphicStyles;
use crate::liveshape::LiveShape;
use crate::placed_image::PlacedImages;
use crate::swatches::{self, Swatches};
use crate::symbols::Symbols;
use crate::transform::Affine;
use kurbo::Shape as KurboShape;
use prism_core::geometry::Rect as CoreRect;
use serde::{Deserialize, Serialize};

/// Default for the additive `visible` field so pre-existing `.contour` files
/// (which lack it) deserialize as visible.
fn default_true() -> bool {
    true
}

/// A read-only view of one editable sub-contour: its anchor `points`, per-anchor
/// out-tangent `handles`, and whether it is `closed`. Returned by
/// [`Shape::contour`] so the Direct-Select tool treats a `Path` and each
/// sub-path of a `Compound` uniformly.
pub type ContourRef<'a> = (&'a [(f32, f32)], &'a [(f32, f32)], bool);

/// A mutable view of one editable sub-contour (anchor points, out-tangent
/// handles, `closed`). Returned by [`Shape::contour_mut`].
pub type ContourMut<'a> = (&'a mut Vec<(f32, f32)>, &'a mut Vec<(f32, f32)>, bool);

/// One drawable vector primitive.
///
/// Every variant carries an additive `visible` flag (`#[serde(default)]`) so
/// older documents keep loading. The `Path` variant additionally carries an
/// additive `handles` list describing per-anchor cubic-bezier tangents.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Shape {
    Rect {
        rect: [f32; 4],
        fill: [f32; 4],
        /// Optional multi-stop gradient that overrides `fill` when present.
        /// Additive (`#[serde(default)]`), so older files load as a solid fill.
        #[serde(default)]
        fill_gradient: Option<Gradient>,
        stroke: [f32; 4],
        stroke_w: f32,
        #[serde(default)]
        stroke_style: StrokeStyle,
        /// Optional stacked [`Appearance`] (multiple fills/strokes) that, when
        /// `Some`, overrides the single `fill`/`stroke` fields on every render
        /// surface. Additive (`#[serde(default)]` → `None`), so older files load
        /// with their single fill/stroke and render unchanged.
        #[serde(default)]
        appearance: Option<Appearance>,
        #[serde(default = "default_true")]
        visible: bool,
        /// Group membership: shapes sharing a `Some(id)` form one group and are
        /// selected / moved / transformed as a unit. Additive
        /// (`#[serde(default)]` → `None`), so older files load ungrouped.
        #[serde(default)]
        group: Option<u64>,
        /// Clip-set membership: shapes sharing a `Some(id)` form one clipping
        /// mask, one of them flagged [`mask`](Self). Additive (`#[serde(default)]`
        /// → `None`), so older files load unclipped.
        #[serde(default)]
        clip: Option<u64>,
        /// Whether this shape is the *masking path* of its clip set. Additive
        /// (`#[serde(default)]` → `false`).
        #[serde(default)]
        mask: bool,
        /// Opacity-mask set membership: shapes sharing a `Some(id)` form one
        /// opacity-mask group, one of them flagged [`omask_path`](Self) as the
        /// luminance mask. Additive (`#[serde(default)]` → `None`), so older
        /// files load unmasked.
        #[serde(default)]
        omask: Option<u64>,
        /// Whether this shape is the *luminance mask* of its opacity-mask set.
        /// Additive (`#[serde(default)]` → `false`).
        #[serde(default)]
        omask_path: bool,
        /// Invert the opacity mask (black reveals, white hides) for the masked
        /// content of this set. Additive (`#[serde(default)]` → `false`).
        #[serde(default)]
        omask_invert: bool,
        /// Blend-set membership: shapes sharing a `Some(id)` form one blend run
        /// (the two ends plus the generated steps). Additive (`#[serde(default)]`
        /// → `None`), so older files load un-blended.
        #[serde(default)]
        blend: Option<u64>,
        /// Whether this shape is a *generated* intermediate step of its blend set
        /// (vs. one of the two original ends). Release deletes the steps and keeps
        /// the ends. Additive (`#[serde(default)]` → `false`).
        #[serde(default)]
        blend_step: bool,
        /// Optional Layers-panel display name. Additive (`#[serde(default)]` →
        /// `None`), so older files load with the generic type label.
        #[serde(default)]
        name: Option<String>,
        /// Whether the shape is **locked**: it renders normally but cannot be
        /// selected, hit-tested, or edited. Additive (`#[serde(default)]` →
        /// `false`), so older files load unlocked.
        #[serde(default)]
        locked: bool,
        /// Optional Layers-panel colour swatch (the row tint Illustrator gives a
        /// layer). Additive (`#[serde(default)]` → `None`).
        #[serde(default)]
        layer_color: Option<[f32; 4]>,
        #[serde(default)]
        envelope_mesh: Option<crate::envelope::EnvelopeMesh>,
    },
    Ellipse {
        rect: [f32; 4],
        fill: [f32; 4],
        #[serde(default)]
        fill_gradient: Option<Gradient>,
        stroke: [f32; 4],
        stroke_w: f32,
        #[serde(default)]
        stroke_style: StrokeStyle,
        #[serde(default)]
        appearance: Option<Appearance>,
        #[serde(default = "default_true")]
        visible: bool,
        #[serde(default)]
        group: Option<u64>,
        #[serde(default)]
        clip: Option<u64>,
        #[serde(default)]
        mask: bool,
        #[serde(default)]
        omask: Option<u64>,
        #[serde(default)]
        omask_path: bool,
        #[serde(default)]
        omask_invert: bool,
        #[serde(default)]
        blend: Option<u64>,
        #[serde(default)]
        blend_step: bool,
        /// Optional Layers-panel display name. Additive (`#[serde(default)]` →
        /// `None`), so older files load with the generic type label.
        #[serde(default)]
        name: Option<String>,
        /// Whether the shape is **locked**: it renders normally but cannot be
        /// selected, hit-tested, or edited. Additive (`#[serde(default)]` →
        /// `false`), so older files load unlocked.
        #[serde(default)]
        locked: bool,
        /// Optional Layers-panel colour swatch (the row tint Illustrator gives a
        /// layer). Additive (`#[serde(default)]` → `None`).
        #[serde(default)]
        layer_color: Option<[f32; 4]>,
        #[serde(default)]
        envelope_mesh: Option<crate::envelope::EnvelopeMesh>,
    },
    Line {
        p0: (f32, f32),
        p1: (f32, f32),
        stroke: [f32; 4],
        stroke_w: f32,
        #[serde(default)]
        stroke_style: StrokeStyle,
        #[serde(default)]
        appearance: Option<Appearance>,
        #[serde(default = "default_true")]
        visible: bool,
        #[serde(default)]
        group: Option<u64>,
        #[serde(default)]
        clip: Option<u64>,
        #[serde(default)]
        mask: bool,
        #[serde(default)]
        omask: Option<u64>,
        #[serde(default)]
        omask_path: bool,
        #[serde(default)]
        omask_invert: bool,
        #[serde(default)]
        blend: Option<u64>,
        #[serde(default)]
        blend_step: bool,
        /// Optional Layers-panel display name. Additive (`#[serde(default)]` →
        /// `None`), so older files load with the generic type label.
        #[serde(default)]
        name: Option<String>,
        /// Whether the shape is **locked**: it renders normally but cannot be
        /// selected, hit-tested, or edited. Additive (`#[serde(default)]` →
        /// `false`), so older files load unlocked.
        #[serde(default)]
        locked: bool,
        /// Optional Layers-panel colour swatch (the row tint Illustrator gives a
        /// layer). Additive (`#[serde(default)]` → `None`).
        #[serde(default)]
        layer_color: Option<[f32; 4]>,
        #[serde(default)]
        envelope_mesh: Option<crate::envelope::EnvelopeMesh>,
    },
    Path {
        points: Vec<(f32, f32)>,
        closed: bool,
        fill: [f32; 4],
        #[serde(default)]
        fill_gradient: Option<Gradient>,
        stroke: [f32; 4],
        stroke_w: f32,
        #[serde(default)]
        stroke_style: StrokeStyle,
        /// Per-anchor *out-tangent* handle, stored as an offset (delta) from the
        /// anchor in document space. The in-tangent is the mirror (`-offset`),
        /// giving a smooth symmetric handle. `(0.0, 0.0)` means a corner anchor
        /// (the adjacent segments are straight lines).
        ///
        /// Additive: defaults to empty, in which case the path is a polyline and
        /// loads identically to the v0 model.
        #[serde(default)]
        handles: Vec<(f32, f32)>,
        /// Optional **live-shape** parameters (polygon / star). When `Some`, the
        /// `points` / `handles` above are *generated* from these parameters
        /// (about the points' bounding-box centre) and the inspector edits the
        /// count / radius / inner-ratio to regenerate them live, like text type's
        /// `params` → `glyphs`. Additive (`#[serde(default)]` → `None`), so a
        /// hand-drawn path and every older file load as a plain (non-live) path.
        #[serde(default)]
        live: Option<LiveShape>,
        #[serde(default)]
        appearance: Option<Appearance>,
        #[serde(default = "default_true")]
        visible: bool,
        #[serde(default)]
        group: Option<u64>,
        #[serde(default)]
        clip: Option<u64>,
        #[serde(default)]
        mask: bool,
        #[serde(default)]
        omask: Option<u64>,
        #[serde(default)]
        omask_path: bool,
        #[serde(default)]
        omask_invert: bool,
        #[serde(default)]
        blend: Option<u64>,
        #[serde(default)]
        blend_step: bool,
        /// Optional Layers-panel display name. Additive (`#[serde(default)]` →
        /// `None`), so older files load with the generic type label.
        #[serde(default)]
        name: Option<String>,
        /// Whether the shape is **locked**: it renders normally but cannot be
        /// selected, hit-tested, or edited. Additive (`#[serde(default)]` →
        /// `false`), so older files load unlocked.
        #[serde(default)]
        locked: bool,
        /// Optional Layers-panel colour swatch (the row tint Illustrator gives a
        /// layer). Additive (`#[serde(default)]` → `None`).
        #[serde(default)]
        layer_color: Option<[f32; 4]>,
        #[serde(default)]
        envelope_mesh: Option<crate::envelope::EnvelopeMesh>,
    },
    /// A **compound path**: one object that keeps several sub-contours (an outer
    /// ring plus inner holes, or several disjoint regions) together, filled as a
    /// unit under a [`FillRule`] (even-odd carves holes, non-zero absorbs same-
    /// wound nesting). This is the document model's real answer to a Pathfinder
    /// result that has holes — instead of expanding it into separate ring shapes,
    /// the holes stay sub-contours of one path. Renders / hit-tests / serializes
    /// as one object, and an `appearance` stack (when present) paints over the
    /// whole compound outline.
    Compound {
        subpaths: Vec<SubPath>,
        #[serde(default)]
        fill_rule: FillRule,
        fill: [f32; 4],
        #[serde(default)]
        fill_gradient: Option<Gradient>,
        stroke: [f32; 4],
        stroke_w: f32,
        #[serde(default)]
        stroke_style: StrokeStyle,
        #[serde(default)]
        appearance: Option<Appearance>,
        #[serde(default = "default_true")]
        visible: bool,
        #[serde(default)]
        group: Option<u64>,
        #[serde(default)]
        clip: Option<u64>,
        #[serde(default)]
        mask: bool,
        #[serde(default)]
        omask: Option<u64>,
        #[serde(default)]
        omask_path: bool,
        #[serde(default)]
        omask_invert: bool,
        #[serde(default)]
        blend: Option<u64>,
        #[serde(default)]
        blend_step: bool,
        /// Optional Layers-panel display name. Additive (`#[serde(default)]` →
        /// `None`), so older files load with the generic type label.
        #[serde(default)]
        name: Option<String>,
        /// Whether the shape is **locked**: it renders normally but cannot be
        /// selected, hit-tested, or edited. Additive (`#[serde(default)]` →
        /// `false`), so older files load unlocked.
        #[serde(default)]
        locked: bool,
        /// Optional Layers-panel colour swatch (the row tint Illustrator gives a
        /// layer). Additive (`#[serde(default)]` → `None`).
        #[serde(default)]
        layer_color: Option<[f32; 4]>,
        #[serde(default)]
        envelope_mesh: Option<crate::envelope::EnvelopeMesh>,
    },
    /// A **point-type** object: an editable string plus its font parameters,
    /// rendered as real glyph outlines. The editable model is `params` (text +
    /// size + alignment) anchored at `origin`; the laid-out glyph contours are
    /// cached in `glyphs` (one closed [`SubPath`] per glyph contour, in document
    /// space) so every render surface, geometry query, and the boolean / clip
    /// pipeline treat a text object exactly like a [`Compound`](Self::Compound)
    /// path. Editing any of `params` / `origin` re-runs [`crate::text::layout`]
    /// to refresh `glyphs` (see [`Shape::text_relayout`]). `Object ▸ Type ▸
    /// Convert to Outlines` lifts `glyphs` into a real `Compound`.
    Text {
        /// The editable text + font parameters. Re-laying out on edit refreshes
        /// the `glyphs` cache.
        params: TextParams,
        /// Document-space anchor: the top-left of the first line's em box (where
        /// the Type tool's click lands).
        origin: (f32, f32),
        /// Cached laid-out glyph outlines (one closed contour each). Derived from
        /// `params` + `origin`; rebuilt on edit and re-derivable on load, so it is
        /// serialized for forward-compat but never trusted over a relayout.
        #[serde(default)]
        glyphs: Vec<SubPath>,
        fill: [f32; 4],
        #[serde(default)]
        fill_gradient: Option<Gradient>,
        stroke: [f32; 4],
        stroke_w: f32,
        #[serde(default)]
        stroke_style: StrokeStyle,
        #[serde(default)]
        appearance: Option<Appearance>,
        #[serde(default = "default_true")]
        visible: bool,
        #[serde(default)]
        group: Option<u64>,
        #[serde(default)]
        clip: Option<u64>,
        #[serde(default)]
        mask: bool,
        #[serde(default)]
        omask: Option<u64>,
        #[serde(default)]
        omask_path: bool,
        #[serde(default)]
        omask_invert: bool,
        #[serde(default)]
        blend: Option<u64>,
        #[serde(default)]
        blend_step: bool,
        /// Optional Layers-panel display name. Additive (`#[serde(default)]` →
        /// `None`), so a text object falls back to its string / the type label.
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        layer_color: Option<[f32; 4]>,
        #[serde(default)]
        envelope_mesh: Option<crate::envelope::EnvelopeMesh>,
    },
}


/// A shape's index in the document paired with its bounding box. Used by the
/// align/distribute layer, which needs to map per-box translation deltas back to
/// the originating shape.
#[derive(Clone, Copy, Debug)]
pub struct ShapeBounds {
    pub index: usize,
    pub rect: CoreRect,
}

/// A ruler guide: an infinite straight line at a fixed document coordinate,
/// either vertical (constant `x`) or horizontal (constant `y`). Dragged out of
/// the rulers and used as a snap target. Stored on the [`Document`] so guides
/// persist in `.contour` files.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Guide {
    /// A vertical line at this `x` (in document units).
    Vertical(f32),
    /// A horizontal line at this `y` (in document units).
    Horizontal(f32),
}

/// Default artboard size (document units) for a fresh document and for the
/// single board a pre-artboards `.contour` file loads with — matching the
/// app's former single-`Size` artboard.
pub const DEFAULT_ARTBOARD: [f32; 2] = [1000.0, 700.0];

/// One default artboard for documents that predate the `artboards` field. A
/// pre-artboards `.contour` always rendered one 1000×700 board at the origin, so
/// `#[serde(default)]` reconstructs exactly that.
fn default_artboards() -> Vec<Artboard> {
    vec![Artboard::new(
        artboard::default_name(0),
        [0.0, 0.0, DEFAULT_ARTBOARD[0], DEFAULT_ARTBOARD[1]],
    )]
}

/// The whole vector document: an ordered list of shapes, any ruler guides, and
/// the artboard stack.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub shapes: Vec<Shape>,
    /// User-placed ruler guides. Additive (`#[serde(default)]`) so pre-existing
    /// `.contour` files — which have no `guides` key — load with none.
    #[serde(default)]
    pub guides: Vec<Guide>,
    /// The artboards. Always at least one (kept non-empty by the editor). A
    /// pre-artboards `.contour` file loads with a single default board, so older
    /// documents round-trip with no visible change. Additive
    /// (`#[serde(default = "default_artboards")]`).
    #[serde(default = "default_artboards")]
    pub artboards: Vec<Artboard>,
    /// Index of the active artboard (frames export / align-to / new-artboard
    /// placement). Clamped into range by the editor. Additive.
    #[serde(default)]
    pub active_artboard: usize,
    /// The document colour palette (the Swatches panel). Additive
    /// (`#[serde(default)]`), so a pre-swatches `.contour` file loads with the
    /// default starter palette; saved palettes round-trip through serde.
    #[serde(default)]
    pub swatches: Swatches,
    /// The document's named-appearance library (the Graphic Styles panel): each
    /// entry captures an [`Appearance`] snapshot a user can apply to a selection.
    /// Additive (`#[serde(default)]`), so a pre-styles `.contour` file loads with
    /// an empty library; saved styles round-trip through serde.
    #[serde(default)]
    pub graphic_styles: GraphicStyles,
    /// The document's **symbols** library and its placed instances (the Symbols
    /// panel): each symbol is a reusable master shape set; each instance is a
    /// reference to a master plus a placement transform, resolved to drawable
    /// shapes at render / export time so editing a master propagates to every
    /// instance. Additive (`#[serde(default)]`), so a pre-symbols `.contour` file
    /// loads with an empty library; saved symbols round-trip through serde.
    #[serde(default)]
    pub symbols: Symbols,
    /// The document's **placed / linked raster images** (the *Place Image*
    /// model): each carries its pixel source (embedded bytes or a disk link), a
    /// placement transform, and an optional clipping path, drawn over the plain
    /// shapes. Additive (`#[serde(default)]`), so a pre-Place `.contour` file
    /// loads with none; saved placed images round-trip through serde.
    #[serde(default)]
    pub placed_images: PlacedImages,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            shapes: Vec::new(),
            guides: Vec::new(),
            artboards: default_artboards(),
            active_artboard: 0,
            swatches: Swatches::default(),
            graphic_styles: GraphicStyles::default(),
            symbols: Symbols::default(),
            placed_images: PlacedImages::default(),
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// The active artboard, clamped so an out-of-range / empty stack still
    /// returns a board. Always `Some` for a well-formed document (the editor
    /// keeps ≥1 artboard); falls back to the first board if the active index is
    /// stale.
    pub fn active_artboard(&self) -> Option<&Artboard> {
        if self.artboards.is_empty() {
            return None;
        }
        let i = self.active_artboard.min(self.artboards.len() - 1);
        self.artboards.get(i)
    }

    /// Re-lay-out every text object's glyph cache from its `params` + `origin`,
    /// so a loaded document (whose cached `glyphs` may be absent / stale / from a
    /// different font build) always renders its text correctly. Cheap (only text
    /// objects do work) and idempotent. Called after deserialize.
    pub fn relayout_text(&mut self) {
        for s in self.shapes.iter_mut() {
            s.text_relayout();
        }
    }

    /// Repair an opened/legacy document: ensure at least one artboard exists and
    /// the active index is in range. Called after deserialize so a hand-edited
    /// or corrupt file can't leave the editor with zero artboards.
    pub fn normalize_artboards(&mut self) {
        if self.artboards.is_empty() {
            self.artboards = default_artboards();
        }
        if self.active_artboard >= self.artboards.len() {
            self.active_artboard = self.artboards.len() - 1;
        }
    }

    /// Remap the colour `old` to `new` across every shape's paint (fill, stroke,
    /// gradient stops). Returns the number of shapes that changed. Drives a
    /// **global swatch** recolour: editing a global swatch hands back its
    /// `(old, new)` pair, and this walks the artwork so every shape painted with
    /// that swatch follows the edit.
    pub fn remap_color(&mut self, old: [f32; 4], new: [f32; 4]) -> usize {
        if swatches::colors_eq(old, new) {
            return 0;
        }
        let mut n = 0;
        for s in self.shapes.iter_mut() {
            if s.remap_color(old, new) {
                n += 1;
            }
        }
        n
    }

    /// A render/export-ready clone with every placed **symbol instance** baked
    /// into `shapes` (resolved against its live master) and the instance list
    /// cleared. Export surfaces (SVG / PNG) flatten through this so an instance
    /// appears in the output exactly as it does on the canvas; the on-screen
    /// editor keeps the live `symbols` model for editing. Cheap when there are no
    /// instances (returns a plain clone).
    pub fn flattened_for_export(&self) -> Document {
        if self.symbols.instances.is_empty() {
            return self.clone();
        }
        let mut out = self.clone();
        for (_, shapes) in self.symbols.resolved_instances() {
            out.shapes.extend(shapes);
        }
        out.symbols.instances.clear();
        out
    }

    /// The shapes to *render*, with clipping masks resolved, paired with the
    /// originating shape's index (so the canvas keeps its selection highlight
    /// mapping). Paint / export iterate this rather than `shapes` directly.
    ///
    /// For each shape:
    /// - **mask path** of a clip set → omitted (an Illustrator clipping path
    ///   paints no fill or stroke once it becomes a mask),
    /// - **clipped content** (a non-mask member of a clip set) → replaced by its
    ///   outline intersected against the mask, as a styled closed `Path`. If the
    ///   content falls entirely outside the mask the shape is omitted; if the mask
    ///   geometry is unusable the original shape is kept unclipped (graceful
    ///   degradation),
    /// - everything else → kept as-is.
    ///
    /// Hidden shapes are still skipped by the caller; this method does not filter
    /// on visibility so callers keep their existing `visible()` checks.
    pub fn render_shapes(&self) -> Vec<(usize, Shape)> {
        let tags: Vec<crate::clip::ClipTag> = self.shapes.iter().map(|s| s.clip_tag()).collect();
        let mut out: Vec<(usize, Shape)> = Vec::with_capacity(self.shapes.len());
        for (i, shape) in self.shapes.iter().enumerate() {
            // An opacity-mask *path* paints nothing on its own: it only supplies
            // luminance to its set's content (applied at raster time by the
            // renderers via [`opacity_mask_of`]).
            if shape.is_omask() {
                continue;
            }
            match shape.clip() {
                None => out.push((i, shape.clone())),
                Some(_) if shape.is_mask() => { /* mask paints nothing */ }
                Some(_) => {
                    // Clip this content shape against its set's mask outline.
                    let clipped = crate::clip::mask_of(&tags, i)
                        .and_then(|m| self.shapes[m].outline_polygon())
                        .and_then(|mask_poly| {
                            shape
                                .outline_polygon()
                                .and_then(|subj| crate::clip::clip_polygon(&subj, &mask_poly))
                        });
                    match clipped {
                        Some(ring) => out.push((i, shape.clone().with_outline(ring))),
                        // No usable mask geometry: keep the content unclipped.
                        // (An empty intersection drops the shape entirely.)
                        None if crate::clip::mask_of(&tags, i)
                            .map(|m| self.shapes[m].outline_polygon().is_none())
                            .unwrap_or(true) =>
                        {
                            out.push((i, shape.clone()))
                        }
                        None => { /* clipped to nothing — omit */ }
                    }
                }
            }
        }
        out
    }

    /// The resolved **opacity mask** for the content shape at `index`, if it
    /// belongs to an opacity-mask set as content (not the mask path itself):
    /// returns the set's luminance-mask shape (cloned) plus its invert flag. The
    /// renderers rasterize this mask's luminance and multiply it into the content
    /// shape's alpha ([`crate::effects::apply_luminance_mask`]). `None` for an
    /// unmasked shape, the mask path itself, or a dangling set.
    pub fn opacity_mask_of(&self, index: usize) -> Option<(Shape, bool)> {
        let s = self.shapes.get(index)?;
        let set = s.omask()?;
        if s.is_omask() {
            return None; // the mask path is not itself masked
        }
        let mask = self
            .shapes
            .iter()
            .find(|o| o.omask() == Some(set) && o.is_omask())?;
        Some((mask.clone(), s.omask_invert()))
    }
}
