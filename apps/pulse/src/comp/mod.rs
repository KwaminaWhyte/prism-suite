//! Pulse's motion document model.
//!
//! A [`Comp`] is a composition: a fixed-size canvas with a duration and frame
//! rate, holding an ordered stack of [`PulseLayer`]s. Each layer carries seven
//! animatable properties — anchor x/y, position x/y, scale, rotation, opacity —
//! stored as [`Track`]s of [`Keyframe`](keyframe::Keyframe)s, and may be **parented** to another
//! layer (inheriting its transform). Scale and rotation pivot about the layer's
//! **anchor point**; a layer's resolved [`Affine2`] world matrix folds its own
//! transform under its parent chain.
//!
//! Sampling: between two bracketing keyframes the value is interpolated
//! according to the *outgoing* keyframe's [`Interp`] mode — linear, hold
//! (stepped), or a temporal cubic-Bézier **ease** (After-Effects style, with
//! editable in/out handles). Before the first key it holds the first value,
//! after the last it holds the last value (constant extrapolation). An empty
//! track returns the property's sensible default.
//!
//! Layer paint order is bottom-up: index 0 is drawn first (back), the last
//! index on top. Colors are straight sRGB RGBA in `[f32; 4]` so they round-trip
//! cleanly through egui's color picker and JSON.

use serde::{Deserialize, Serialize};

mod blend;
mod camera;
mod comp_build;
mod comp_query;
mod distort;
mod effect;
mod effect_browser;
mod effect_browser_registry;
pub(crate) mod expr;
mod fonts;
mod footage;
mod generate;
mod generate_math;
mod grading;
mod key;
mod keyframe;
mod light;
mod marker;
mod mask;
mod matte;
mod motion_blur;
mod motion_path;
mod precomp;
mod preset;
mod roving;
mod shape;
mod spatial;
mod stylize;
mod text;
mod text_glyphs;
mod text_outline;
mod puppet;
mod time_remap;
mod transform;

pub use blend::{blend_label, blend_over, BlendMode, BlendRgba, LayerBlend};
// `Camera` is the comp's camera (live UI + renderer); `rotate_orientation` and
// `Projected` are the camera's pure-math API, re-exported for the unit tests (the
// live renderer reaches them through the `camera::` / `Camera` methods), so they
// are allowed unused in the bin build.
pub use camera::Camera;
#[allow(unused_imports)]
pub use camera::{rotate_orientation, Projected};
// `Light` / `LightKind` are the comp's lights (live UI + renderer);
// `illumination` / `layer_normal` are the pure shading API, re-exported for the
// unit tests (the live renderer reaches them through `Comp::layer_light_factor`).
pub use light::{Light, LightKind};
#[allow(unused_imports)]
pub use light::{illumination, layer_normal};
pub use distort::{apply_distort_effects, apply_displacement_map, DistortEffect, PolarKind};
pub use effect::{
    apply_effects, apply_effects_masked, blend_masked, Effect, EffectMask, LayerKind,
};
pub use effect_browser::{filter_grouped, BrowserEntry, Category as EffectCategory, NewEffect, Stack, REGISTRY as EFFECT_REGISTRY};
pub use expr::{last_error as expr_last_error, ExprCtx};
pub use fonts::{families as font_families, is_available as font_is_available};
pub use footage::{
    source_from_path, AlphaMode, DecodedFrame, FootageLayer, FootageSource, FrameBlend, FrameCache,
};
pub use generate::{CellType, FractalType, GenerateEffect, Overflow, RampShape};
pub use key::{apply_key_effects, KeyEffect};
pub use keyframe::{Ease, Handle, Interp, Track};
pub use marker::{next_marker_time, prev_marker_time, Marker, WorkArea};
pub use mask::{mask_stack_coverage, Mask, MaskMode, MaskVertex};
pub use matte::MatteMode;
pub use motion_blur::{MotionBlur, Prop};
// The motion-path sampler is the deliverable's pure spatial-curve API: rendering
// uses it via `motion_path::` internally, and it's re-exported for the upcoming
// editable on-canvas path overlay (and the unit tests). Allowed unused until the
// overlay UI consumes it.
#[allow(unused_imports)]
pub use motion_path::{auto_orient_deg, sample_path, PathSample};
pub use precomp::{PrecompLayer, Project};
pub use preset::AnimationPreset;
// The roving re-timer is the deliverable's pure constant-velocity API: position
// sampling uses it via `roving::` internally, and `roved_times` / `RoveKey` are
// re-exported for the unit tests (and a future speed-graph readout). Allowed
// unused until a non-test caller consumes them directly.
#[allow(unused_imports)]
pub use roving::{has_roving, roved_times, roved_tracks, RoveKey};
// `PresetTrack` / `PropTag` are the public field types of an `AnimationPreset`
// (re-exported for API completeness + the unit tests); the live UI touches a
// preset only through `capture` / `apply`, so allow them unused in the bin build.
#[allow(unused_imports)]
pub use preset::{PresetTrack, PropTag};
pub use shape::{Fill, ShapeItem, ShapeLayer, ShapePrimitive, ShapeRepeater, Stroke, TrimPaths};
pub use spatial::{apply_spatial_effects, gaussian_blur, RadialKind, SpatialEffect};
pub use stylize::{apply_stylize_effects, StylizeEffect};
pub use text::{TextAlign, TextLayer};
pub use puppet::{PinId, PuppetPin};
pub use time_remap::TimeRemap;
pub use transform::{Affine2, Transform};

// Per-layer colour grading (TextAnimator / Lumetri / Color Finesse) lives in
// `grading.rs` (extracted for the workspace size rule); re-export so existing
// `crate::comp::{TextAnimator, LumetriColor, ColorFinesse, ColorFinesseRange}`
// paths keep resolving.
pub use grading::{ColorFinesse, ColorFinesseRange, LumetriColor, TextAnimator};

fn default_time_stretch() -> f32 { 1.0 }
fn default_puppet_density() -> u8 { 4 }

/// One animated layer: a solid color rect transformed by its tracks, optionally
/// **parented** to another layer (whose transform it inherits).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PulseLayer {
    pub name: String,
    /// What this layer *is* (solid / null / adjustment). `serde`-defaulted to
    /// `Solid` so pre-layer-kind `.pulse` files still load as solids.
    #[serde(default)]
    pub kind: LayerKind,
    /// **Per-layer blend mode** (After Effects' layer blending-mode dropdown):
    /// how this layer's pixels combine with the composite beneath it. Reuses the
    /// suite's shared 18-mode [`BlendMode`] set (`prism-core`), evaluated by the
    /// CPU compositor's [`blend_over`]. Wrapped in [`LayerBlend`] so a missing
    /// field `serde`-defaults to [`BlendMode::Normal`] — pre-blend-mode `.pulse`
    /// files still load and render byte-identically (Normal == source-over).
    #[serde(default)]
    pub blend: LayerBlend,
    /// **Per-layer motion-blur** switch (After Effects' layer MB toggle). A
    /// layer is motion-blurred only when both this and the comp's
    /// [`MotionBlur::enabled`] master switch are on. `serde`-defaulted to `false`
    /// so pre-motion-blur `.pulse` files still load.
    #[serde(default)]
    pub motion_blur: bool,
    /// **Auto-orient along path** (After Effects' *Orient Along Path*). When set,
    /// the layer's effective rotation follows the **tangent** of its animated
    /// position path — the layer turns to face its direction of travel — composed
    /// with (added to) its keyframed Rotation. The path heading comes from the
    /// pure [`sample_path`] over the `x` / `y` tracks. `serde`-defaulted to `false`
    /// so pre-auto-orient `.pulse` files load with it off and render unchanged.
    #[serde(default)]
    pub auto_orient: bool,
    /// **3-D layer** switch (After Effects' per-layer 3D toggle). When set, the
    /// layer gains a **Z position** ([`z`](Self::z)) and an X/Y/Z **orientation**
    /// ([`orient_x`](Self::orient_x) / [`orient_y`](Self::orient_y) /
    /// [`orient_z`](Self::orient_z)), and is placed through the comp
    /// [`Camera`](crate::comp::Camera) by **perspective projection** (so depth
    /// shrinks/enlarges it) and **painter's z-sorted** by camera-space depth
    /// against the other 3-D layers. With it off the layer is a flat 2-D layer
    /// exactly as before. `serde`-defaulted to `false` so pre-3-D `.pulse` files
    /// load as 2-D and render byte-identically.
    #[serde(default)]
    pub threed: bool,
    /// **Accepts lights** (After Effects' per-layer *Material Options ▸ Accepts
    /// Lights*). When set on a **3-D layer**, the comp's [`Light`]s shade the
    /// layer's pixels — an ambient floor plus per-light Lambert diffuse, applied
    /// as an RGB multiplier over the layer's isolated buffer (see
    /// [`Comp::layer_light_factor`]). `serde`-defaulted to `false` and a no-op on
    /// 2-D layers, so pre-lighting `.pulse` files (and any layer that doesn't opt
    /// in) render **byte-identically** to today.
    #[serde(default)]
    pub accepts_lights: bool,
    /// Solid swatch color (straight sRGB RGBA, 0..=1) for the v0 preview.
    pub color: [f32; 4],
    pub visible: bool,
    /// Non-destructive, ordered **effect stack**. For a solid layer the stack
    /// processes the layer's own pixels; for an adjustment layer it processes
    /// the composite of everything below. `serde`-defaulted to empty for old
    /// projects.
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// **Effect mask** (After Effects' *Compositing Options ▸ effect mask*):
    /// limits where the per-pixel color-correction [`effects`](Self::effects)
    /// stack applies, blending the effected pixel back toward the original by a
    /// per-pixel mask coverage (`out = lerp(orig, effected, coverage)`). Reuses
    /// the layer-[`Mask`] geometry/coverage (feather / expansion / invert /
    /// opacity). `serde`-defaulted to a disabled, empty mask so pre-effect-mask
    /// `.pulse` files load with the effect applying everywhere (unchanged).
    #[serde(default)]
    pub effect_mask: EffectMask,
    /// Non-destructive, ordered **spatial effect stack** (whole-buffer passes:
    /// Gaussian / Box / Directional / Radial Blur, Drop Shadow, Glow). Applied to the layer's isolated
    /// rendered buffer *after* its per-pixel color-correction stack, masks, and
    /// track matte. `serde`-defaulted to empty so pre-spatial-effect `.pulse`
    /// files still load.
    #[serde(default)]
    pub spatial_effects: Vec<SpatialEffect>,
    /// Non-destructive, ordered **distort effect stack** (whole-buffer
    /// coordinate-remap passes: Corner Pin, Transform, Mirror, Polar
    /// Coordinates). Applied to the layer's isolated rendered buffer in the same
    /// finishing step as the spatial stack — *after* its color-correction stack,
    /// masks, and track matte, and *after* the spatial passes — so a distort
    /// warps the already-blurred/shadowed/glowed buffer (matching After Effects'
    /// top-down effect order, distort below blur). `serde`-defaulted to empty so
    /// pre-distort `.pulse` files still load.
    #[serde(default)]
    pub distort_effects: Vec<DistortEffect>,
    /// Non-destructive, ordered **key effect stack** (whole-buffer
    /// alpha-affecting passes: Color Key, Luma Key, Chroma Key, Spill
    /// Suppression, Matte Choke). Applied to the layer's isolated rendered buffer
    /// in the finishing step *after* its color-correction stack, masks, and track
    /// matte, but *before* the spatial passes — so a key carves the matte first
    /// and a later Gaussian Blur can soften the keyed edge (matching AE's
    /// keyer-then-blur matte-refine order). `serde`-defaulted to empty so
    /// pre-keying `.pulse` files still load.
    #[serde(default)]
    pub key_effects: Vec<KeyEffect>,
    /// Non-destructive, ordered **stylize effect stack** (whole-buffer
    /// look-shaping passes: Find Edges, Mosaic). Applied to the layer's isolated
    /// rendered buffer in the finishing step *after* its color-correction stack,
    /// masks, track matte, key passes, and spatial passes, but *before* the
    /// distort passes — so a stylize reshapes the already-blurred/glowed buffer and
    /// a later distort can warp the stylized result (matching After Effects'
    /// top-down effect order, distort below stylize). `serde`-defaulted to empty so
    /// pre-stylize `.pulse` files still load.
    #[serde(default)]
    pub stylize_effects: Vec<StylizeEffect>,
    /// Optional **generate** (whole-buffer fill) effect — currently **Fractal
    /// Noise**. Unlike the colour/spatial stacks (which read the layer's pixels),
    /// a generate effect *replaces* them: it synthesises the layer's content from
    /// its parameters + the pixel position, filling the layer's quad before the
    /// masks / matte / spatial passes apply. A layer carries at most one (a second
    /// fill would just override the first), so it's an `Option`, not a `Vec`.
    /// `serde`-defaulted to `None` so pre-generate `.pulse` files still load.
    #[serde(default)]
    pub generate: Option<GenerateEffect>,
    /// **Evolution** track for the [`generate`](Self::generate) fill — the key
    /// motion-design knob. Fractal Noise's other params are plain scalars (matching
    /// how the colour / spatial effect stacks expose params), but *evolution* is
    /// what flows the field over time, so it gets a full keyframable [`Track`].
    /// When this track has keys it **overrides** the generate's static `evolution`
    /// field at the sampled time (and is expression-able via the track); empty, the
    /// static field is used. `serde`-defaulted to empty so pre-generate `.pulse`
    /// files load unchanged.
    #[serde(default)]
    pub generate_evolution: Track,
    /// Parent layer index, if this layer is parented. A child inherits its
    /// parent's full transform (position, scale, rotation, anchor) but **not**
    /// its opacity (matching After Effects). `serde`-defaulted so pre-parenting
    /// `.pulse` files still load as unparented.
    #[serde(default)]
    pub parent: Option<usize>,
    /// **Track matte** mode. When active, the layer directly *above* this one in
    /// the stack defines this layer's per-pixel transparency and is itself
    /// removed from normal compositing (matching After Effects). `serde`-defaulted
    /// to [`MatteMode::None`] so pre-matte `.pulse` files still load.
    #[serde(default)]
    pub matte: MatteMode,
    /// **Masks**: closed Bézier paths (layer-local) that carve the layer's
    /// coverage. Folded top-down into a single coverage multiplier on the
    /// layer's alpha (see [`mask_stack_coverage`]). `serde`-defaulted to empty
    /// so pre-mask `.pulse` files still load unmasked.
    #[serde(default)]
    pub masks: Vec<Mask>,
    /// **Shape** content (rectangles / ellipses / polygons / stars with fills
    /// and strokes), drawn only when [`kind`](Self::kind) is
    /// [`LayerKind::Shape`]. `serde`-defaulted to empty so pre-shape `.pulse`
    /// files still load.
    #[serde(default)]
    pub shape: ShapeLayer,
    /// **Text** content (a string drawn with the built-in stroke font), drawn
    /// only when [`kind`](Self::kind) is [`LayerKind::Text`]. `serde`-defaulted
    /// so pre-text `.pulse` files still load.
    #[serde(default)]
    pub text: TextLayer,
    /// **Footage** content (a still image or numbered image sequence on disk),
    /// drawn only when [`kind`](Self::kind) is [`LayerKind::Footage`].
    /// `serde`-defaulted so pre-footage `.pulse` files still load with no source.
    #[serde(default)]
    pub footage: FootageLayer,
    /// **Precomp** reference (target comp id + a time-offset shift), drawn only
    /// when [`kind`](Self::kind) is [`LayerKind::Precomp`]: the referenced comp is
    /// rendered recursively at the mapped time and composited into this layer's
    /// quad. `serde`-defaulted so pre-precomp `.pulse` files still load with no
    /// reference.
    #[serde(default)]
    pub precomp: PrecompLayer,
    /// **Time remap** (After Effects' *Enable Time Remap*): an optional enable
    /// switch + a keyframable scalar track of *source* times. When enabled on a
    /// time-based layer (footage image-sequence / precomp), the source is sampled
    /// at the remapped time instead of the comp time — letting the user freeze /
    /// reverse / retime playback. `serde`-defaulted to disabled (empty track) so
    /// pre-time-remap `.pulse` files load and sample their source unchanged.
    #[serde(default)]
    pub time_remap: TimeRemap,
    /// **Puppet warp pins** (layer-local anchor points for IDW puppet deformation).
    /// `serde`-defaulted to empty so pre-puppet `.pulse` files still load.
    #[serde(default)]
    pub puppet_pins: Vec<PuppetPin>,
    /// **Solo flag**: when true and any layer in the comp is solo, only solo layers
    /// (and their parent chains) render. `serde`-defaulted to false.
    #[serde(default)]
    pub solo: bool,
    /// **Shy flag**: hides this layer from the layers panel when `comp.hide_shy` is set.
    /// `serde`-defaulted to false.
    #[serde(default)]
    pub shy: bool,
    /// **Text Animator** (After Effects' Text Animator): per-character offset /
    /// scale / rotation / opacity applied to the fraction of characters in the
    /// selector range. Only meaningful when `kind == LayerKind::Text`. `serde`-
    /// defaulted to `None` so pre-animator `.pulse` files load unchanged.
    #[serde(default)]
    pub text_animator: Option<TextAnimator>,
    /// **Lumetri Color** grade: an optional per-layer color grade (exposure /
    /// contrast / highlights / shadows / temperature / saturation). Applied after
    /// the layer's normal effect stack; `None` = bypass. `serde`-defaulted so
    /// pre-Lumetri `.pulse` files load with no grade.
    #[serde(default)]
    pub lumetri: Option<LumetriColor>,
    /// **Layer markers** (After Effects' layer markers): labelled points/spans
    /// pinned to this layer's timeline. Pure timeline metadata — drawn on the
    /// layer's lane and used by time navigation; they carry no pixels.
    /// `serde`-defaulted to empty so pre-marker `.pulse` files still load.
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// **In-point**: time (seconds) at which this layer becomes visible. `None`
    /// means the layer is visible from the start of the comp. `serde`-defaulted
    /// to `None` so pre-in-point `.pulse` files load unchanged.
    #[serde(default)]
    pub in_point: Option<f32>,
    /// **Out-point**: time (seconds) after which this layer is no longer visible.
    /// `None` means the layer is visible until the end of the comp. `serde`-defaulted
    /// to `None` so pre-out-point `.pulse` files load unchanged.
    #[serde(default)]
    pub out_point: Option<f32>,
    /// **Per-layer Color Finesse**: Master + 6 tonal-range (Reds/Yellows/Greens/
    /// Cyans/Blues/Magentas) Hue/Saturation/Lightness adjustment, applied after
    /// the color-correction stack. `serde`-defaulted to `None` so pre-finesse
    /// `.pulse` files load and render unchanged.
    #[serde(default)]
    pub color_finesse: Option<ColorFinesse>,
    // Animated properties. An empty track means "use the default constant".
    /// Anchor-point offset from the layer's geometric center (comp px). The
    /// pivot for scale/rotation and the local point aligned to `(x, y)`.
    #[serde(default)]
    pub anchor_x: Track,
    #[serde(default)]
    pub anchor_y: Track,
    pub x: Track,
    pub y: Track,
    pub scale: Track,
    pub rotation: Track,
    pub opacity: Track,
    /// **Z position** (comp px; `+z` recedes into the screen). Only meaningful
    /// when [`threed`](Self::threed) is set. `serde`-defaulted to empty so
    /// pre-3-D `.pulse` files load with `Z = 0` (coplanar with the comp).
    #[serde(default)]
    pub z: Track,
    /// **Orientation** about the layer's anchor (degrees), the 3-D twin of the
    /// 2-D Rotation. `orient_z` is an extra in-plane roll on top of `rotation`;
    /// `orient_x` / `orient_y` tilt / pan the layer out of the comp plane. Only
    /// meaningful when [`threed`](Self::threed) is set. `serde`-defaulted to
    /// empty (no orientation) so pre-3-D `.pulse` files load coplanar.
    #[serde(default)]
    pub orient_x: Track,
    #[serde(default)]
    pub orient_y: Track,
    #[serde(default)]
    pub orient_z: Track,

    // --- Batch 3 extended fields ---
    /// **Rotobrush strokes**: frame-keyed foreground/background paint strokes used
    /// by the rotobrush segmentation stub. `serde`-defaulted to empty so pre-rotobrush
    /// `.pulse` files still load.
    #[serde(default)]
    pub rotobrush_strokes: Vec<crate::app_state::RotobrushStroke>,
    /// How many frames ahead the last rotobrush propagation was applied.
    /// `serde`-defaulted to 0.
    #[serde(default)]
    pub rotobrush_propagated_frames: u32,
    /// **Time stretch factor** (After Effects' *Time Stretch*). A value of `1.0`
    /// means real-time; `2.0` plays the layer at half speed; `0.5` at double speed.
    /// `serde`-defaulted to `1.0` via a helper so old files load at normal speed.
    #[serde(default = "default_time_stretch")]
    pub time_stretch: f32,
    /// **Audio fade-in duration** in seconds. `serde`-defaulted to `0.0`.
    #[serde(default)]
    pub audio_fade_in: f32,
    /// **Audio fade-out duration** in seconds. `serde`-defaulted to `0.0`.
    #[serde(default)]
    pub audio_fade_out: f32,
    /// **Puppet mesh density** (1-D resolution of the deformation grid; minimum 2).
    /// `serde`-defaulted to `4` via a helper.
    #[serde(default = "default_puppet_density")]
    pub puppet_mesh_density: u8,
    /// **Echo effect** config. `None` means no echo. `serde`-defaulted to `None`.
    #[serde(default)]
    pub echo: Option<crate::app_state::EchoConfig>,
}

impl PulseLayer {
    /// A new layer with the given name and color and all-empty tracks.
    pub fn new(name: impl Into<String>, color: [f32; 4]) -> Self {
        Self {
            name: name.into(),
            kind: LayerKind::Solid,
            blend: LayerBlend::default(),
            motion_blur: false,
            auto_orient: false,
            threed: false,
            accepts_lights: false,
            color,
            visible: true,
            effects: Vec::new(),
            effect_mask: EffectMask::default(),
            spatial_effects: Vec::new(),
            distort_effects: Vec::new(),
            key_effects: Vec::new(),
            stylize_effects: Vec::new(),
            generate: None,
            generate_evolution: Track::default(),
            parent: None,
            matte: MatteMode::None,
            masks: Vec::new(),
            shape: ShapeLayer::default(),
            text: TextLayer::default(),
            footage: FootageLayer::default(),
            precomp: PrecompLayer::default(),
            time_remap: TimeRemap::default(),
            puppet_pins: Vec::new(),
            solo: false,
            shy: false,
            text_animator: None,
            lumetri: None,
            markers: Vec::new(),
            in_point: None,
            out_point: None,
            color_finesse: None,
            anchor_x: Track::default(),
            anchor_y: Track::default(),
            x: Track::default(),
            y: Track::default(),
            scale: Track::default(),
            rotation: Track::default(),
            opacity: Track::default(),
            z: Track::default(),
            orient_x: Track::default(),
            orient_y: Track::default(),
            orient_z: Track::default(),
            rotobrush_strokes: Vec::new(),
            rotobrush_propagated_frames: 0,
            time_stretch: 1.0,
            audio_fade_in: 0.0,
            audio_fade_out: 0.0,
            puppet_mesh_density: 4,
            echo: None,
        }
    }

    /// A new layer of the given kind, name, and color (empty tracks/effects).
    pub fn of_kind(kind: LayerKind, name: impl Into<String>, color: [f32; 4]) -> Self {
        Self {
            kind,
            ..Self::new(name, color)
        }
    }

    /// Borrow the track for `prop`.
    pub fn track(&self, prop: Prop) -> &Track {
        match prop {
            Prop::AnchorX => &self.anchor_x,
            Prop::AnchorY => &self.anchor_y,
            Prop::X => &self.x,
            Prop::Y => &self.y,
            Prop::Scale => &self.scale,
            Prop::Rotation => &self.rotation,
            Prop::Opacity => &self.opacity,
        }
    }

    /// Mutably borrow the track for `prop`.
    pub fn track_mut(&mut self, prop: Prop) -> &mut Track {
        match prop {
            Prop::AnchorX => &mut self.anchor_x,
            Prop::AnchorY => &mut self.anchor_y,
            Prop::X => &mut self.x,
            Prop::Y => &mut self.y,
            Prop::Scale => &mut self.scale,
            Prop::Rotation => &mut self.rotation,
            Prop::Opacity => &mut self.opacity,
        }
    }

    /// Whether this layer has **roving** position keys, so position sampling must
    /// re-time the `x` / `y` tracks for constant velocity (see [`roving`]).
    pub fn has_roving_position(&self) -> bool {
        roving::has_roving(&self.x, &self.y)
    }

    /// The layer's **effective** `x` / `y` position tracks at sample time: the
    /// roving-re-timed copies when any interior position key roves (constant
    /// velocity along the motion path — see [`roving::roved_tracks`]), otherwise
    /// the originals borrowed unchanged. Returned as a [`Cow`] so the common
    /// no-roving case borrows with zero allocation and stays byte-identical.
    ///
    /// [`Cow`]: std::borrow::Cow
    fn position_tracks(&self) -> (std::borrow::Cow<'_, Track>, std::borrow::Cow<'_, Track>) {
        use std::borrow::Cow;
        if self.has_roving_position() {
            let (rx, ry) = roving::roved_tracks(&self.x, &self.y);
            (Cow::Owned(rx), Cow::Owned(ry))
        } else {
            (Cow::Borrowed(&self.x), Cow::Borrowed(&self.y))
        }
    }

    /// Sample one property at time `t`, ignoring any expression (keyframes only).
    ///
    /// For the spatial **position** properties (`X` / `Y`) the sample honours
    /// **roving keys**: when an interior position key roves, the `x` / `y` tracks
    /// are re-timed for constant velocity along the motion path before sampling.
    pub fn value(&self, prop: Prop, t: f32) -> f32 {
        match prop {
            Prop::X | Prop::Y if self.has_roving_position() => {
                let (rx, ry) = self.position_tracks();
                let track = if prop == Prop::X { rx } else { ry };
                track.sample(t, prop.default_value())
            }
            _ => self.track(prop).sample(t, prop.default_value()),
        }
    }

    /// Sample one property at time `t`, **evaluating its expression** if one is
    /// set. `ctx` carries the comp/layer context (fps, duration, layer index);
    /// `ctx.time` should be `t`. The keyframed value is exposed to the expression
    /// as `value`; a parse/eval error falls back to the keyframed value.
    ///
    /// For the spatial **position** properties the keyframed value seen by the
    /// expression honours **roving keys** (re-timed for constant velocity),
    /// exactly like [`value`](Self::value).
    pub fn value_ctx(&self, prop: Prop, ctx: ExprCtx) -> f32 {
        match prop {
            Prop::X | Prop::Y if self.has_roving_position() => {
                let (rx, ry) = self.position_tracks();
                let track = if prop == Prop::X { rx } else { ry };
                track.sample_expr(ctx.time, prop.default_value(), ctx)
            }
            _ => self
                .track(prop)
                .sample_expr(ctx.time, prop.default_value(), ctx),
        }
    }

    /// Sample the transform properties at time `t` into a [`Transform`],
    /// ignoring expressions. Kept for callers without comp context (the gizmo's
    /// drag-start snapshot, tests). Expression-aware rendering uses
    /// [`Comp::layer_transform`].
    pub fn transform(&self, t: f32) -> Transform {
        Transform {
            anchor_x: self.value(Prop::AnchorX, t),
            anchor_y: self.value(Prop::AnchorY, t),
            x: self.value(Prop::X, t),
            y: self.value(Prop::Y, t),
            scale: self.value(Prop::Scale, t),
            rotation_deg: self.value(Prop::Rotation, t),
            opacity: self.value(Prop::Opacity, t).clamp(0.0, 1.0),
        }
    }

    /// Sample the transform properties at time `t` into a [`Transform`],
    /// **evaluating each property's expression** against `ctx` (one per
    /// property — each sees its own keyframed value as `value`).
    pub fn transform_ctx(&self, ctx: ExprCtx) -> Transform {
        Transform {
            anchor_x: self.value_ctx(Prop::AnchorX, ctx),
            anchor_y: self.value_ctx(Prop::AnchorY, ctx),
            x: self.value_ctx(Prop::X, ctx),
            y: self.value_ctx(Prop::Y, ctx),
            scale: self.value_ctx(Prop::Scale, ctx),
            rotation_deg: self.value_ctx(Prop::Rotation, ctx),
            opacity: self.value_ctx(Prop::Opacity, ctx).clamp(0.0, 1.0),
        }
    }

    /// This layer's resolved [`BlendMode`] (how it composites over the layers
    /// beneath it). [`BlendMode::Normal`] means plain source-over.
    pub fn blend_mode(&self) -> BlendMode {
        self.blend.0
    }

    /// Whether this layer has at least one **active** mask (so the renderer must
    /// run the per-pixel mask-coverage pass for it).
    pub fn has_active_masks(&self) -> bool {
        self.masks.iter().any(Mask::is_active)
    }

    /// The pre-flattened **effect-mask region polygon** (layer-local), or an
    /// empty `Vec` when the effect mask is inactive (disabled / no shape) — so the
    /// per-pixel effect-stack passes flatten it once per frame, not per pixel, and
    /// pass it to [`apply_effects_masked`]. An empty polygon makes the masked
    /// apply fall back to the unmasked path (effect everywhere).
    pub fn effect_mask_poly(&self) -> Vec<(f32, f32)> {
        if self.effect_mask.is_active() {
            self.effect_mask.region.flatten()
        } else {
            Vec::new()
        }
    }

    /// Whether this layer has any **spatial effects** (Gaussian Blur / Drop
    /// Shadow / Glow), so the renderer must route it through an isolated buffer
    /// to run the whole-buffer passes.
    pub fn has_spatial_effects(&self) -> bool {
        !self.spatial_effects.is_empty()
    }

    /// Whether this layer has any **distort effects** (Corner Pin / Transform /
    /// Mirror / Polar Coordinates), so the renderer must route it through an
    /// isolated buffer to run the whole-buffer coordinate-remap passes.
    pub fn has_distort_effects(&self) -> bool {
        !self.distort_effects.is_empty()
    }

    /// Whether this layer has any **key effects** (Color / Luma / Chroma Key,
    /// Spill Suppression, Matte Choke), so the renderer must route it through an
    /// isolated buffer to run the whole-buffer alpha-affecting passes.
    pub fn has_key_effects(&self) -> bool {
        !self.key_effects.is_empty()
    }

    /// Whether this layer has any **stylize effects** (Find Edges / Mosaic), so
    /// the renderer must route it through an isolated buffer to run the
    /// whole-buffer look-shaping passes.
    pub fn has_stylize_effects(&self) -> bool {
        !self.stylize_effects.is_empty()
    }

    /// The layer's generate fill with its **evolution** resolved at time `t`: if
    /// the [`generate_evolution`](Self::generate_evolution) track has keys, the
    /// generate's `evolution` field is replaced by the sampled track value (so the
    /// field flows over time); otherwise the generate's static `evolution` is kept.
    /// `None` when the layer has no generate fill.
    pub fn generate_at(&self, t: f32) -> Option<GenerateEffect> {
        let mut g = self.generate?;
        if !self.generate_evolution.keys.is_empty() {
            // Evolution is the Fractal-Noise / Cell-Pattern motion knob; the colour
            // generators have no evolution axis, so the track is a no-op for them.
            match &mut g {
                GenerateEffect::FractalNoise { evolution, .. }
                | GenerateEffect::CellPattern { evolution, .. } => {
                    *evolution = self.generate_evolution.sample(t, *evolution);
                }
                _ => {}
            }
        }
        Some(g)
    }

    /// Whether this layer is a [`LayerKind::Shape`] with at least one shape
    /// item to draw.
    pub fn has_shape(&self) -> bool {
        self.kind == LayerKind::Shape && !self.shape.is_empty()
    }

    /// Whether this layer is a [`LayerKind::Text`] with text to draw.
    pub fn has_text(&self) -> bool {
        self.kind == LayerKind::Text && !self.text.is_empty()
    }

    /// Whether this layer is a [`LayerKind::Footage`] with a source set.
    pub fn has_footage(&self) -> bool {
        self.kind == LayerKind::Footage && self.footage.is_set()
    }

    /// Whether this layer is a [`LayerKind::Precomp`] with a comp referenced.
    pub fn has_precomp(&self) -> bool {
        self.kind == LayerKind::Precomp && self.precomp.is_set()
    }
}

/// One composition: a sized, timed canvas and its layer stack. A document is a
/// [`Project`] of these; a [`LayerKind::Precomp`] layer references another comp
/// in the same project by [`id`](Self::id).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comp {
    /// Stable identifier within the project — the target a
    /// [`PrecompLayer`](precomp::PrecompLayer) references. `serde`-defaulted to
    /// `0` so old single-comp `.pulse` files (no id) load; the project assigns a
    /// real id on import (see [`Project::from_comp`]).
    #[serde(default)]
    pub id: u64,
    /// A short display name for the comp (shown in the precomp picker / comp
    /// list). `serde`-defaulted so old `.pulse` files load with an empty name
    /// (the UI falls back to a generated label).
    #[serde(default)]
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub duration: f32,
    pub fps: f32,
    /// Composition **motion-blur** settings (master switch + shutter
    /// angle/phase + sample count). `serde`-defaulted so pre-motion-blur
    /// `.pulse` files still load with motion blur off.
    #[serde(default)]
    pub motion_blur: MotionBlur,
    /// **Composition markers** (After Effects' comp markers): labelled
    /// points/spans on the comp timeline, drawn on the ruler and used by time
    /// navigation. `serde`-defaulted to empty so pre-marker `.pulse` files still
    /// load.
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// The **work area** — the `[start, end]` sub-range of the timeline that
    /// bounds RAM-preview / playback / render. `serde`-defaulted to the empty full
    /// range; a loaded comp expands it to its own duration (see [`Comp::new`] and
    /// the project loader), and the renderer/transport clamp it to the comp every
    /// use, so a pre-work-area `.pulse` file behaves as the whole timeline.
    #[serde(default)]
    pub work_area: WorkArea,
    /// The composition **camera** that 3-D layers are projected through. The
    /// `serde` default is the [`Camera::default`] free camera placed so the
    /// `z = 0` plane fills the frame at unit scale — which reproduces today's
    /// flat 2-D look exactly, so a pre-camera `.pulse` file (and any comp with
    /// only 2-D layers) renders byte-identically. The default position is sized
    /// to the comp height when a comp is created (see [`Comp::new`]); the
    /// projection always recomputes the focal distance from the live FOV + comp
    /// height, so even a serde-default camera projects correctly.
    #[serde(default)]
    pub camera: Camera,
    /// The composition **lights** that shade [`accepts_lights`](PulseLayer)-opted
    /// 3-D layers (ambient floor + point Lambert diffuse). `serde`-defaulted to
    /// an **empty** list so a pre-lighting `.pulse` file loads with no lights —
    /// and a comp with no lights leaves every layer's illumination factor
    /// `[1, 1, 1]`, so output is byte-identical to today.
    #[serde(default)]
    pub lights: Vec<Light>,
    pub layers: Vec<PulseLayer>,
    /// **Hide shy layers** toggle: when true, layers with `shy = true` are hidden
    /// from the layers panel. `serde`-defaulted to false.
    #[serde(default)]
    pub hide_shy: bool,
}

// `Comp::new` (the seeded demo composition), `empty_like`, and `display_name`
// live in `comp_build.rs` (extracted for the workspace size rule) as a first
// `impl Comp` block.

// The `Comp` query / resolution methods (world matrix, 3-D projection, depth
// sort, lighting, DoF, expression-aware samplers, matte lookups, work-area
// clamp, marker navigation) live in `comp_query.rs` (extracted for the
// workspace size rule) as a second `impl Comp` block.

impl Default for Comp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
