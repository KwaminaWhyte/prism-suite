use super::*;

/// A single editable control in a Motion Graphics Template (Essential Graphics).
#[derive(Clone, Debug)]
pub enum MoGrtControl {
    /// An editable text string.
    Text { label: String, value: String },
    /// An editable RGBA color (straight sRGB, 0–1).
    Color { label: String, value: [f32; 4] },
    /// A numeric slider with a bounded range.
    Slider { label: String, value: f32, min: f32, max: f32 },
}

impl MoGrtControl {
    /// The label shown in the Essential Graphics panel.
    pub fn label(&self) -> &str {
        match self {
            MoGrtControl::Text { label, .. } => label,
            MoGrtControl::Color { label, .. } => label,
            MoGrtControl::Slider { label, .. } => label,
        }
    }
}

/// A Motion Graphics Template: a named collection of editable [`MoGrtControl`]s
/// that surface text / color / slider properties for quick reuse (After Effects'
/// Essential Graphics / `.mogrt` format).
#[derive(Clone, Debug, Default)]
pub struct MotionGraphicTemplate {
    pub name: String,
    pub controls: Vec<MoGrtControl>,
}

/// Echo (motion-trail) effect configuration for a layer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EchoConfig {
    /// Time offset between successive echoes (seconds, e.g. 0.1).
    pub delay_seconds: f32,
    /// Number of echoes to produce (1-10).
    pub count: u8,
    /// Opacity decay per echo (0.0 = no decay, 1.0 = fully transparent after 1 echo).
    pub decay: f32,
    /// Blend mode: 0 = composite-in-time, 1 = add, 2 = screen.
    pub blend_mode: u8,
}

impl Default for EchoConfig {
    fn default() -> Self {
        Self { delay_seconds: 0.1, count: 3, decay: 0.5, blend_mode: 0 }
    }
}

// --- Batch 5 effect enums ---

#[derive(Clone, Debug)]
pub enum RadialBlurKind { Spin, Zoom }

#[derive(Clone, Debug)]
pub enum SmartBlurMode { Normal, EdgeOnly, Overlay }

#[derive(Clone, Debug)]
pub enum GlowColors { Original, AAndB }

#[derive(Clone, Debug)]
pub enum GlowChannel { AlphaChannel, Luminance }

#[derive(Clone, Debug)]
pub enum TilingMode { Tile, StagedTile, Kaleidoscope }

#[derive(Clone, Debug)]
pub enum CylinderRenderMode { Full, Outside, Inside }

#[derive(Clone, Debug)]
pub enum ChannelSource { Red, Green, Blue, Alpha, Full, Luminance }

#[derive(Clone, Debug)]
pub enum CalcOperation {
    Add, Subtract, Difference, Multiply, Screen, Overlay,
    HardLight, SoftLight, Darken, Lighten, Exclusion,
}

#[derive(Clone, Debug)]
pub enum CellPatternKind {
    Bubbles, Crystals, HqCrystals, Mixed, Tubular, HqTubular,
    Plates, HqPlates, Checkerboard, HexTiles,
}

#[derive(Clone, Debug)]
pub enum OverflowMode { Clip, WrapAround, HardClamp }

#[derive(Clone, Debug)]
pub enum GradientEffectKind { Linear, Radial }

#[derive(Clone, Debug)]
pub enum StrokePath { AllMaskPaths, Reveal, Transparent }

#[derive(Clone, Debug)]
pub enum StrokeComposite { Over, In, Below }

/// Batch 5 built-in effect applied to a layer (pure data, no GPU execution).
#[derive(Clone, Debug)]
pub enum Batch5Effect {
    MotionBlur { angle: f32, distance: f32 },
    RadialBlur { amount: f32, center_x: f32, center_y: f32, kind: RadialBlurKind },
    SmartBlur { radius: f32, threshold: f32, mode: SmartBlurMode },
    Glow { threshold: f32, radius: f32, intensity: f32, glow_colors: GlowColors, glow_channel: GlowChannel },
    GlowingEdges { edge_width: u32, edge_brightness: u32, smoothness: u32 },
    CcComposite { opacity: f32, composite_on_original: bool },
    CcBendIt { start: (f32, f32), end: (f32, f32), bend: f32 },
    CcRepeTile { expand_right: f32, expand_left: f32, expand_up: f32, expand_down: f32, tiling: TilingMode },
    CcCylinder { radius: f32, rotation_x: f32, rotation_y: f32, render: CylinderRenderMode },
    CcSphere { radius: f32, rotation_x: f32, rotation_y: f32, render: CylinderRenderMode },
    CcPixelPolly { gravity: f32, force: f32, grid_spacing: u32 },
    CcRainfall { drops: u32, speed: f32, wind: f32, spread: f32, color: String },
    CcSnow { flakes: u32, speed: f32, wind: f32, size: f32 },
    CcToner { highlights: String, shadows: String, balance: f32 },
    PosterizeTime { frame_rate: f32 },
    TimeDisplacement { max_displacement: f32, time_layer: Option<usize> },
    SetChannels { red: ChannelSource, green: ChannelSource, blue: ChannelSource, alpha: ChannelSource },
    Blend { layer_to_blend: Option<usize>, mode: String, opacity: f32, if_layer_absent: bool },
    Calculations {
        input_a_layer: Option<usize>,
        input_a_channel: ChannelSource,
        input_b_layer: Option<usize>,
        input_b_channel: ChannelSource,
        operation: CalcOperation,
        opacity: f32,
        preserve_transparency: bool,
    },
    CellPattern { pattern: CellPatternKind, size: f32, feather: f32, offset_x: f32, offset_y: f32, overflow: OverflowMode },
    Checkerboard { anchor: (f32, f32), size: f32, feather: f32, color_a: String, color_b: String, width: f32 },
    CircleBurst { center: (f32, f32), thickness: f32, soft: f32, color: String, start_radius: f32 },
    Gradient { start: (f32, f32), end: (f32, f32), kind: GradientEffectKind, start_color: String, end_color: String },
    Grid { anchor: (f32, f32), size: (f32, f32), border: f32, feather: f32, color: String, invert: bool },
    Stroke { path: StrokePath, all_masks: bool, color: String, brush_size: f32, softness: f32, opacity: f32, composite: StrokeComposite },
}

impl App {
    pub(super) fn apply_effects_chain(&mut self, action: Action) {
        match action {
            Action::ToggleEffectBrowser => {
                self.effect_browser_open = !self.effect_browser_open;
            }
            Action::SetEffectQuery(q) => {
                self.effect_query = q;
            }
            Action::AddEffect(entry) => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    match entry.instantiate() {
                        NewEffect::Color(e) => layer.effects.push(e),
                        NewEffect::Spatial(e) => layer.spatial_effects.push(e),
                        NewEffect::Distort(e) => layer.distort_effects.push(e),
                        NewEffect::Stylize(e) => layer.stylize_effects.push(e),
                        NewEffect::Keying(e) => layer.key_effects.push(e),
                        NewEffect::Generate(e) => layer.generate = Some(e),
                    }
                    self.host.mark_dirty();
                }
            }
            Action::RemoveEffect { stack, index } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let removed = match stack {
                        EffectStack::Color => vec_remove(&mut layer.effects, index),
                        EffectStack::Spatial => vec_remove(&mut layer.spatial_effects, index),
                        EffectStack::Distort => vec_remove(&mut layer.distort_effects, index),
                        EffectStack::Stylize => vec_remove(&mut layer.stylize_effects, index),
                        EffectStack::Keying => vec_remove(&mut layer.key_effects, index),
                        EffectStack::Generate => layer.generate.take().is_some(),
                    };
                    if removed {
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetEffectParam { stack, index, param, value } => {
                let Some(i) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(i) {
                    let edited = match stack {
                        EffectStack::Color => layer
                            .effects
                            .get_mut(index)
                            .map(|e| effect_params::set_color(e, param, value))
                            .is_some(),
                        EffectStack::Spatial => layer
                            .spatial_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_spatial(e, param, value))
                            .is_some(),
                        EffectStack::Distort => layer
                            .distort_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_distort(e, param, value))
                            .is_some(),
                        EffectStack::Stylize => layer
                            .stylize_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_stylize(e, param, value))
                            .is_some(),
                        EffectStack::Keying => layer
                            .key_effects
                            .get_mut(index)
                            .map(|e| effect_params::set_key(e, param, value))
                            .is_some(),
                        EffectStack::Generate => layer
                            .generate
                            .as_mut()
                            .map(|e| effect_params::set_generate(e, param, value))
                            .is_some(),
                    };
                    if edited {
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AddGpuiEffect(kind) => {
                let Some(li) = self.selected_layer else { return };
                let ci = self.active_comp_index();
                if self.project.comps[ci].layers.get(li).is_none() { return }
                let effect = GpuiEffect::default_for_kind(kind);
                self.gpui_effects.entry(li).or_default().push(effect);
                self.host.mark_dirty();
            }
            Action::RemoveGpuiEffect(ei) => {
                let Some(li) = self.selected_layer else { return };
                let stack = self.gpui_effects.entry(li).or_default();
                if ei < stack.len() {
                    stack.remove(ei);
                    self.host.mark_dirty();
                }
            }
            Action::SetMosaicBlock { effect_idx, block } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let GpuiEffect::Mosaic { block: b } = e { *b = block.clamp(1, 128); }
                    self.host.mark_dirty();
                }
            }
            Action::SetChromaOffset { effect_idx, offset } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let GpuiEffect::ChromaticAberration { offset: o } = e { *o = offset; }
                    self.host.mark_dirty();
                }
            }
            Action::SetEffectIntensity { effect_idx, intensity } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    let i = intensity.clamp(0.0, 1.0);
                    match e {
                        GpuiEffect::Vignette { intensity: iv, .. } => *iv = i,
                        GpuiEffect::Noise { intensity: ni } => *ni = i,
                        _ => {}
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetVignetteRadius { effect_idx, radius } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let GpuiEffect::Vignette { radius: r, .. } = e { *r = radius.clamp(0.5, 1.0); }
                    self.host.mark_dirty();
                }
            }
            Action::ToggleGpuiEffectExpand(ei) => {
                let li = self.selected_layer.unwrap_or(0);
                let key = (li, ei);
                let cur = *self.gpui_effects_expanded.get(&key).unwrap_or(&false);
                self.gpui_effects_expanded.insert(key, !cur);
            }
            Action::SetColorBalanceShadows { effect_idx, channel, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::ColorBalance { shadows_r, shadows_g, shadows_b, .. } = e {
                        match channel { 0 => *shadows_r = value, 1 => *shadows_g = value, _ => *shadows_b = value }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetColorBalanceMidtones { effect_idx, channel, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::ColorBalance { midtones_r, midtones_g, midtones_b, .. } = e {
                        match channel { 0 => *midtones_r = value, 1 => *midtones_g = value, _ => *midtones_b = value }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetColorBalanceHighlights { effect_idx, channel, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::ColorBalance { highlights_r, highlights_g, highlights_b, .. } = e {
                        match channel { 0 => *highlights_r = value, 1 => *highlights_g = value, _ => *highlights_b = value }
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsInBlack { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { in_black, .. } = e { *in_black = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsInWhite { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { in_white, .. } = e { *in_white = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsGamma { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { gamma, .. } = e { *gamma = value.max(0.01); }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsOutBlack { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { out_black, .. } = e { *out_black = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetLevelsOutWhite { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::Levels { out_white, .. } = e { *out_white = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetHueShift { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::HueSaturation { hue_shift, .. } = e { *hue_shift = value; }
                    self.host.mark_dirty();
                }
            }
            Action::SetSaturation { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::HueSaturation { saturation, .. } = e { *saturation = value.clamp(0.0, 2.0); }
                    self.host.mark_dirty();
                }
            }
            Action::SetLightness { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::HueSaturation { lightness, .. } = e { *lightness = value.clamp(-1.0, 1.0); }
                    self.host.mark_dirty();
                }
            }
            Action::SetNoiseFrequency { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::FractalNoise { frequency, .. } = e { *frequency = value.max(0.001); }
                    self.host.mark_dirty();
                }
            }
            Action::SetNoiseEvolution { effect_idx, value } => {
                let Some(li) = self.selected_layer else { return };
                if let Some(e) = self.gpui_effects.get_mut(&li).and_then(|v| v.get_mut(effect_idx)) {
                    if let crate::gpui_effects::GpuiEffect::FractalNoise { evolution, .. } = e { *evolution = value; }
                    self.host.mark_dirty();
                }
            }
            Action::AddDisplacementMap { layer_idx, map_layer, scale_x, scale_y } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    layer.distort_effects.push(
                        crate::comp::DistortEffect::DisplacementMap {
                            map_layer,
                            scale_x,
                            scale_y,
                        }
                    );
                    self.host.mark_dirty();
                }
            }
            Action::SetDisplaceScale { layer_idx, effect_idx, scale_x, scale_y } => {
                let ci = self.active_comp_index();
                if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                    if let Some(crate::comp::DistortEffect::DisplacementMap { scale_x: sx, scale_y: sy, .. })
                        = layer.distort_effects.get_mut(effect_idx)
                    {
                        *sx = scale_x;
                        *sy = scale_y;
                        self.host.mark_dirty();
                    }
                }
            }

            // Layer-data effect arms (text animator, shape trim/repeater,
            // Lumetri Color, Color Finesse) live in `effects_apply.rs` to keep
            // this file under the 1000-line limit.
            _ => self.apply_effects_apply(action),
        }
    }
}
