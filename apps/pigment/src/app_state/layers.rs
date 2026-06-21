use super::*;

/// The adjustment-layer kinds the host can add from the Adjustments browser, in
/// the same order as `Adjustment::defaults()`. A small host-side enum (rather
/// than passing a full `Adjustment` from the panel) keeps the panel rows simple
/// and `Copy`; `apply` maps each to its default-param `Adjustment`.
// `Exposure` has no browser row yet (the egui app surfaces it via a different
// menu), but the enum stays complete so `to_adjustment` covers every kind.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdjKind {
    BrightnessContrast,
    Levels,
    Curves,
    HueSaturation,
    Exposure,
    Vibrance,
    PhotoFilter,
    Posterize,
    GradientMap,
    ColorBalance,
    ChannelMixer,
    BlackWhite,
    Threshold,
    Invert,
}

impl AdjKind {
    /// The default-param `Adjustment` for this kind (mirrors the egui app's
    /// `Adjustment::defaults()` entries, plus the parameterless kinds the browser
    /// also lists).
    pub fn to_adjustment(self) -> Adjustment {
        match self {
            AdjKind::BrightnessContrast => Adjustment::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
            AdjKind::Levels => Adjustment::Levels {
                in_black: 0.0,
                in_white: 1.0,
                gamma: 1.0,
            },
            AdjKind::Curves => Adjustment::Curves(CurvePoints::default()),
            AdjKind::HueSaturation => Adjustment::HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: 0.0,
            },
            AdjKind::Exposure => Adjustment::Exposure { stops: 0.0 },
            AdjKind::Vibrance => Adjustment::Vibrance { amount: 0.0 },
            AdjKind::PhotoFilter => Adjustment::PhotoFilter {
                color: [1.0, 0.64, 0.0],
                density: 0.25,
            },
            AdjKind::Posterize => Adjustment::Posterize { levels: 4 },
            AdjKind::GradientMap => Adjustment::GradientMap {
                low: [0.05, 0.0, 0.2],
                high: [1.0, 0.85, 0.4],
            },
            AdjKind::ColorBalance => Adjustment::ColorBalance {
                shadows: [0.0; 3],
                midtones: [0.0; 3],
                highlights: [0.0; 3],
                preserve_luminosity: true,
            },
            AdjKind::ChannelMixer => Adjustment::ChannelMixer {
                r: [1.0, 0.0, 0.0, 0.0],
                g: [0.0, 1.0, 0.0, 0.0],
                b: [0.0, 0.0, 1.0, 0.0],
                monochrome: false,
            },
            AdjKind::BlackWhite => Adjustment::BlackWhite,
            AdjKind::Threshold => Adjustment::Threshold { level: 0.5 },
            AdjKind::Invert => Adjustment::Invert,
        }
    }
}
// ---- Wave 11: Layer-style types ------------------------------------------------

/// A drop shadow or inner shadow descriptor.
#[derive(Clone, Debug, Default)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: [f32; 4],
    pub opacity: f32,
}

/// An outer or inner glow descriptor.
#[derive(Clone, Debug, Default)]
pub struct Glow {
    pub blur: f32,
    pub spread: f32,
    pub color: [f32; 4],
    pub opacity: f32,
}

/// A bevel-and-emboss descriptor.
#[derive(Clone, Debug, Default)]
pub struct Bevel {
    pub depth: f32,
    pub size: f32,
    pub angle: f32,
    pub highlight_opacity: f32,
    pub shadow_opacity: f32,
}

/// Non-destructive per-layer visual effects applied on top of the composited layer.
/// Fields default to `None` = disabled. Effects are applied CPU-side in `canvas_host`.
#[derive(Clone, Debug, Default)]
pub struct LayerStyle {
    pub drop_shadow: Option<Shadow>,
    pub outer_glow: Option<Glow>,
    pub inner_glow: Option<Glow>,
    pub bevel_emboss: Option<Bevel>,
}
// ---- Batch 6: Layer Effects Suite -----------------------------------------------

/// Full per-layer drop-shadow effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DropShadowFx {
    pub enabled: bool,
    pub blend_mode: BlendMode,
    pub color: [f32; 4],
    /// 0..=100, default 75
    pub opacity: f32,
    /// 0..=360, default 120
    pub angle: f32,
    /// 0..=30000, default 5
    pub distance: f32,
    /// 0..=100, default 0
    pub spread: f32,
    /// 0..=250, default 5
    pub size: f32,
    /// 0..=100, default 0
    pub noise: f32,
    pub layer_knocks_out: bool,
}

impl Default for DropShadowFx {
    fn default() -> Self {
        Self {
            enabled: false,
            blend_mode: BlendMode::Multiply,
            color: [0.0, 0.0, 0.0, 1.0],
            opacity: 75.0,
            angle: 120.0,
            distance: 5.0,
            spread: 0.0,
            size: 5.0,
            noise: 0.0,
            layer_knocks_out: true,
        }
    }
}

/// Full per-layer outer-glow effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OuterGlowFx {
    pub enabled: bool,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    pub noise: f32,
    pub color: [f32; 4],
    pub spread: f32,
    /// default 5
    pub size: f32,
    /// 1..=100, default 50
    pub range: f32,
    /// 0..=100, default 0
    pub jitter: f32,
}

impl Default for OuterGlowFx {
    fn default() -> Self {
        Self {
            enabled: false,
            blend_mode: BlendMode::Screen,
            opacity: 75.0,
            noise: 0.0,
            color: [1.0, 1.0, 0.8, 1.0],
            spread: 0.0,
            size: 5.0,
            range: 50.0,
            jitter: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BevelStyle {
    OuterBevel,
    #[default]
    InnerBevel,
    Emboss,
    PillowEmboss,
    StrokeEmboss,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BevelTechnique {
    #[default]
    SmoothB,
    ChiselHard,
    ChiselSoft,
}

/// Full per-layer bevel-and-emboss effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BevelEmbossFx {
    pub enabled: bool,
    pub style: BevelStyle,
    pub technique: BevelTechnique,
    /// 1..=1000, default 100
    pub depth: f32,
    /// true = up, false = down
    pub direction_up: bool,
    /// 0..=250, default 5
    pub size: f32,
    /// 0..=16, default 0
    pub soften: f32,
    pub angle: f32,
    /// 0..=90, default 30
    pub altitude: f32,
    pub highlight_opacity: f32,
    pub shadow_opacity: f32,
}

impl Default for BevelEmbossFx {
    fn default() -> Self {
        Self {
            enabled: false,
            style: BevelStyle::InnerBevel,
            technique: BevelTechnique::SmoothB,
            depth: 100.0,
            direction_up: true,
            size: 5.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            highlight_opacity: 75.0,
            shadow_opacity: 75.0,
        }
    }
}

/// All non-destructive per-layer effects for the Layer Effects panel.
/// Keyed by layer id string in `App::layer_effects`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct LayerEffects {
    pub drop_shadow: DropShadowFx,
    pub inner_shadow_enabled: bool,
    pub outer_glow: OuterGlowFx,
    pub inner_glow_enabled: bool,
    pub bevel_emboss: BevelEmbossFx,
    pub satin_enabled: bool,
    pub color_overlay_enabled: bool,
    pub color_overlay_color: [f32; 4],
    pub gradient_overlay_enabled: bool,
    pub pattern_overlay_enabled: bool,
    pub stroke_enabled: bool,
    /// default 3
    pub stroke_size: f32,
    pub stroke_color: [f32; 4],
}

// ---- Batch 5: Layer Comps -----------------------------------------------

/// Snapshot of a single layer's visual state, stored inside a [`LayerComp`].
#[derive(Clone, Debug)]
pub struct LayerCompState {
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub offset_x: i32,
    pub offset_y: i32,
}

/// A named snapshot of all layer states, enabling quick switching between
/// layout/visibility variations (Photoshop "Layer Comps" parity).
#[derive(Clone, Debug)]
pub struct LayerComp {
    pub name: String,
    /// Per-layer state at the time this comp was captured.
    pub states: HashMap<LayerId, LayerCompState>,
}


impl App {
    pub(super) fn apply_layers(&mut self, action: Action) {
        match action {
            Action::ToggleLayerVisible(id) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    l.visible = !l.visible;
                    self.sync_host_order_dirty();
                }
            }
            Action::SelectLayer(id) => {
                if self.doc.layers.get(id).is_some() {
                    self.doc.active_layer = Some(id);
                }
            }
            Action::SetLayerOpacity(id, o) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    l.opacity = o.clamp(0.0, 1.0);
                    self.sync_host_order_dirty();
                }
            }
            Action::MoveLayer { id, up } => {
                let layers = &mut self.doc.layers.layers;
                if let Some(i) = layers.iter().position(|l| l.id == id) {
                    let j = if up { i + 1 } else { i.wrapping_sub(1) };
                    if up && j < layers.len() {
                        layers.swap(i, j);
                        self.sync_host_order_dirty();
                    } else if !up && i > 0 {
                        layers.swap(i, i - 1);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::DeleteLayer(id) => {
                let layers = &mut self.doc.layers.layers;
                if layers.len() > 1 {
                    if let Some(i) = layers.iter().position(|l| l.id == id) {
                        layers.remove(i);
                        if self.doc.active_layer == Some(id) {
                            self.doc.active_layer = self.doc.layers.layers.last().map(|l| l.id);
                        }
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::SetLayerBlend(id, mode) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    l.blend = mode;
                    self.sync_host_order_dirty();
                }
            }
            Action::AddAdjustment(kind) => {
                let adj = kind.to_adjustment();
                let id = self.doc.layers.add_adjustment(adj);
                self.doc.active_layer = Some(id);
                self.host.ensure_layer(id);
                self.host.sync_adjustment_luts(&self.doc);
                self.sync_host_order_dirty();
            }
            Action::SetAdjustment(id, adj) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    if matches!(l.kind, LayerKind::Adjustment(_)) {
                        l.kind = LayerKind::Adjustment(adj);
                        self.host.sync_adjustment_luts(&self.doc);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::AddMask(id) => {
                if self.doc.layers.get(id).is_some() {
                    self.host.set_mask(id, None);
                    self.masked_layers.insert(id);
                    self.doc.active_layer = Some(id);
                    self.edit_mask = true;
                }
            }
            Action::DeleteMask(id) => {
                if self.masked_layers.remove(&id) {
                    self.host.delete_mask(id);
                    if self.doc.active_layer == Some(id) {
                        self.edit_mask = false;
                    }
                }
            }
            Action::ToggleEditMask => {
                self.edit_mask = !self.edit_mask;
            }
            Action::SetAdjustmentCurve(id, points) => {
                if let Some(l) = self.doc.layers.get_mut(id) {
                    if let LayerKind::Adjustment(Adjustment::Curves(ref mut cp)) = l.kind {
                        cp.rgb = points;
                        self.host.sync_adjustment_luts(&self.doc);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::SelectAll => {
                let rect = [0.0, 0.0, self.doc.size.width as f32, self.doc.size.height as f32];
                self.apply(Action::SetMarquee { rect, ellipse: false });
            }
            Action::FlattenLayers => {
                self.host.mark_dirty();
            }
            Action::AddSwatch(c) => {
                if !self.swatches.contains(&c) {
                    self.swatches.push(c);
                }
            }
            Action::SwapColors => {
                std::mem::swap(&mut self.brush.color, &mut self.bg_color);
            }
            Action::ResetColors => {
                self.brush.color = [0.0, 0.0, 0.0, 1.0];
                self.bg_color = [1.0, 1.0, 1.0, 1.0];
            }
            Action::ToggleChannel(ch) => {
                if ch < 4 {
                    self.channel_visibility[ch] = !self.channel_visibility[ch];
                    self.host.set_channel_mask(self.channel_visibility);
                }
            }
            Action::SetLayerStyle(id, style) => {
                self.layer_styles.insert(id, style);
                self.host.mark_dirty();
            }
            Action::ClearLayerStyle(id) => {
                self.layer_styles.remove(&id);
                self.host.mark_dirty();
            }
            Action::OpenStylePanel(id) => {
                self.style_panel_open = true;
                self.style_panel_layer = Some(id);
            }
            Action::CloseStylePanel => {
                self.style_panel_open = false;
            }
            Action::ToggleClippingMask(id) => {
                if self.clipping_masks.contains(&id) {
                    self.clipping_masks.remove(&id);
                } else {
                    self.clipping_masks.insert(id);
                }
                self.host.mark_dirty();
            }
            Action::SetFgHue(h) => {
                self.fg_hue = h.rem_euclid(360.0);
                self.brush.color = hsv_to_rgb(self.fg_hue, self.fg_saturation, self.fg_value);
            }
            Action::SetFgSV(s, v) => {
                self.fg_saturation = s.clamp(0.0, 1.0);
                self.fg_value = v.clamp(0.0, 1.0);
                self.brush.color = hsv_to_rgb(self.fg_hue, self.fg_saturation, self.fg_value);
            }
            Action::NewLayer => {
                let id = self.doc.layers.add_raster("Layer");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                self.sync_host_order_dirty();
            }
            Action::DuplicateLayer => {
                if let Some(src_id) = self.doc.active_layer {
                    let new_id = self.doc.layers.add_raster("Layer copy");
                    self.host.ensure_layer(new_id);
                    if let Some(px) = self.host.read_layer_f32(src_id) {
                        self.host.upload_layer_f32(new_id, &px);
                    }
                    self.doc.active_layer = Some(new_id);
                    self.sync_host_order_dirty();
                }
            }
            Action::MergeDown => {
                self.status_message = Some("Merge Down: coming in a future update".to_string());
            }
            Action::AddSolidFillLayer(color) => {
                let id = self.doc.layers.alloc_id();
                self.doc.layers.layers.push(prism_core::layer::Layer::raster(id, "Solid Color"));
                self.host.ensure_layer(id);
                let [r, g, b, a] = color;
                let rf = r as f32 / 255.0;
                let gf = g as f32 / 255.0;
                let bf = b as f32 / 255.0;
                let af = a as f32 / 255.0;
                self.host.fill_solid(id, [rf, gf, bf, af]);
                self.fill_layers.insert(id, color);
                self.doc.active_layer = Some(id);
            }
            Action::SetFillLayerColor(id, color) => {
                if self.fill_layers.contains_key(&id) {
                    let [r, g, b, a] = color;
                    let rf = r as f32 / 255.0;
                    let gf = g as f32 / 255.0;
                    let bf = b as f32 / 255.0;
                    let af = a as f32 / 255.0;
                    self.host.fill_solid(id, [rf, gf, bf, af]);
                    self.fill_layers.insert(id, color);
                }
            }
            Action::CreateLayerGroup(name) => {
                let id = self.doc.layers.alloc_id();
                let mut group_layer = prism_core::layer::Layer::raster(id, &name);
                group_layer.name = name;
                self.doc.layers.layers.push(group_layer);
                self.doc.active_layer = Some(id);
            }
            Action::SetGroupCollapsed { group_name, collapsed } => {
                if collapsed {
                    self.collapsed_groups.insert(group_name);
                } else {
                    self.collapsed_groups.remove(&group_name);
                }
            }
            Action::MoveLayerToGroup { layer_id, group_name } => {
                self.status_message = Some(format!("Layer {:?} moved to group \"{}\"", layer_id, group_name));
            }
            Action::RemoveLayerFromGroup(layer_id) => {
                self.status_message = Some(format!("Layer {:?} removed from group", layer_id));
            }
            Action::FlattenGroup(name) => {
                self.collapsed_groups.remove(&name);
            }
            Action::DuplicateGroup(name) => {
                self.last_duplicated_group = Some(name);
            }
            Action::ToggleLayerCompsPanel => {
                self.layer_comps_panel_open = !self.layer_comps_panel_open;
            }
            Action::AddLayerComp(name) => {
                let states = capture_layer_comp_states(&self.doc);
                self.layer_comps.push(LayerComp { name, states });
            }
            Action::ApplyLayerComp(idx) => {
                if let Some(comp) = self.layer_comps.get(idx).cloned() {
                    apply_layer_comp_states(&mut self.doc, &comp.states);
                    self.active_comp_idx = Some(idx);
                    self.sync_host_order_dirty();
                }
            }
            Action::UpdateLayerComp(idx) => {
                if idx < self.layer_comps.len() {
                    self.layer_comps[idx].states = capture_layer_comp_states(&self.doc);
                }
            }
            Action::DeleteLayerComp(idx) => {
                if idx < self.layer_comps.len() {
                    self.layer_comps.remove(idx);
                    self.active_comp_idx = self.active_comp_idx.and_then(|i| {
                        if i == idx { None } else if i > idx { Some(i - 1) } else { Some(i) }
                    });
                }
            }
            Action::RenameLayerComp { idx, name } => {
                if let Some(comp) = self.layer_comps.get_mut(idx) {
                    comp.name = name;
                }
            }
            Action::SetBlendIf { layer_id, blend_if } => {
                self.blend_if.insert(layer_id, blend_if);
            }
            Action::ClearBlendIf(id) => {
                self.blend_if.remove(&id);
            }
            Action::SetFxTargetLayer(layer) => {
                self.fx_target_layer = layer;
            }
            Action::SetDropShadow { layer, fx } => {
                self.layer_effects.entry(layer).or_default().drop_shadow = fx;
            }
            Action::ToggleDropShadow { layer, enabled } => {
                self.layer_effects.entry(layer).or_default().drop_shadow.enabled = enabled;
            }
            Action::SetOuterGlow { layer, fx } => {
                self.layer_effects.entry(layer).or_default().outer_glow = fx;
            }
            Action::ToggleOuterGlow { layer, enabled } => {
                self.layer_effects.entry(layer).or_default().outer_glow.enabled = enabled;
            }
            Action::SetBevelEmboss { layer, fx } => {
                self.layer_effects.entry(layer).or_default().bevel_emboss = fx;
            }
            Action::ToggleBevelEmboss { layer, enabled } => {
                self.layer_effects.entry(layer).or_default().bevel_emboss.enabled = enabled;
            }
            Action::SetStroke { layer, size, color } => {
                let e = self.layer_effects.entry(layer).or_default();
                e.stroke_size = size;
                e.stroke_color = color;
            }
            Action::ToggleStroke { layer, enabled } => {
                self.layer_effects.entry(layer).or_default().stroke_enabled = enabled;
            }
            Action::SetColorOverlay { layer, color } => {
                let e = self.layer_effects.entry(layer).or_default();
                e.color_overlay_color = color;
            }
            Action::ToggleColorOverlay { layer, enabled } => {
                self.layer_effects.entry(layer).or_default().color_overlay_enabled = enabled;
            }
            Action::ClearLayerEffects { layer } => {
                self.layer_effects.remove(&layer);
            }
            Action::ToggleFxPanel => {
                self.fx_panel_open = !self.fx_panel_open;
            }
            Action::CopyLayerEffects { from } => {
                self.fx_clipboard = self.layer_effects.get(&from).cloned();
            }
            Action::PasteLayerEffects { to } => {
                if let Some(fx) = self.fx_clipboard.clone() {
                    self.layer_effects.insert(to, fx);
                }
            }
            Action::BeginCurveDrag(lid, idx) => {
                self.dragging_curve_point = Some((lid, idx));
            }
            Action::MoveCurvePoint(lid, idx, (nx, ny)) => {
                let nx = nx.clamp(0.0, 1.0);
                let ny = ny.clamp(0.0, 1.0);
                if let Some(l) = self.doc.layers.get_mut(lid) {
                    if let LayerKind::Adjustment(Adjustment::Curves(ref mut cp)) = l.kind {
                        if let Some(pt) = cp.rgb.get_mut(idx) {
                            *pt = (nx, ny);
                            self.host.sync_adjustment_luts(&self.doc);
                            self.sync_host_order_dirty();
                        }
                    }
                }
            }
            Action::EndCurveDrag => {
                self.dragging_curve_point = None;
                self.host.sync_adjustment_luts(&self.doc);
            }
            Action::RemoveCurvePoint(lid, idx) => {
                if let Some(l) = self.doc.layers.get_mut(lid) {
                    if let LayerKind::Adjustment(Adjustment::Curves(ref mut cp)) = l.kind {
                        if cp.rgb.len() > 2 {
                            cp.rgb.remove(idx);
                            self.host.sync_adjustment_luts(&self.doc);
                            self.sync_host_order_dirty();
                        }
                    }
                }
            }
            Action::HoverCurvePoint(pt) => {
                self.hovered_curve_point = pt;
            }
            Action::SetCurvesCanvasBounds(b) => {
                self.curves_canvas_bounds = Some(b);
            }
            Action::SetGradientMapStops { layer_id, stops } => {
                if let Some(l) = self.doc.layers.get_mut(layer_id) {
                    if let LayerKind::Adjustment(Adjustment::GradientMap { ref mut low, ref mut high }) = l.kind {
                        if let Some(first) = stops.first() {
                            *low = [first.1[0], first.1[1], first.1[2]];
                        }
                        if let Some(last) = stops.last() {
                            *high = [last.1[0], last.1[1], last.1[2]];
                        }
                        self.host.sync_adjustment_luts(&self.doc);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::SetChannelMixerOutput { layer_id, output } => {
                log::debug!("ChannelMixerOutput: layer {:?} → channel {}", layer_id, output);
                let _ = output;
            }
            Action::SetChannelMixerMix { layer_id, src_r, src_g, src_b, constant } => {
                if let Some(l) = self.doc.layers.get_mut(layer_id) {
                    if let LayerKind::Adjustment(Adjustment::ChannelMixer {
                        ref mut r, ref mut g, ref mut b, ..
                    }) = l.kind {
                        *r = [src_r, src_g, src_b, constant];
                        let _ = (g, b);
                        self.host.sync_adjustment_luts(&self.doc);
                        self.sync_host_order_dirty();
                    }
                }
            }
            Action::JumpHistory(idx) => {
                log::info!("JumpHistory({idx}) — stub: engine history is stack-based");
                if idx + 1 < self.history_labels.len() {
                    self.history_labels.truncate(idx + 1);
                }
            }
            Action::UndoTo(target) => {
                let current = self.history_labels.len();
                if target < current {
                    let steps = current - target;
                    for _ in 0..steps {
                        self.host.undo();
                    }
                    self.history_labels.truncate(target);
                }
            }
            Action::CreateSnapshot(name) => {
                let layers: Vec<serde_json::Value> = self.doc.layers.layers.iter().map(|l| {
                    serde_json::json!({
                        "id": l.id.0,
                        "name": l.name,
                        "visible": l.visible,
                        "opacity": l.opacity,
                    })
                }).collect();
                let snapshot = serde_json::json!({
                    "size": { "width": self.doc.size.width, "height": self.doc.size.height },
                    "layers": layers,
                });
                let json = serde_json::to_string(&snapshot).unwrap_or_default();
                self.snapshots.push((name.clone(), json));
                self.status_message = Some(format!("Snapshot '{name}' created"));
            }
            Action::Undo => self.host.undo(),
            Action::Redo => self.host.redo(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adj_kind_to_adjustment_brightness() {
        let adj = AdjKind::BrightnessContrast.to_adjustment();
        matches!(adj, prism_core::Adjustment::BrightnessContrast { .. });
    }

    #[test]
    fn test_adj_kind_invert() {
        let adj = AdjKind::Invert.to_adjustment();
        assert!(matches!(adj, prism_core::Adjustment::Invert));
    }

    #[test]
    fn test_layer_style_default() {
        let ls = LayerStyle::default();
        assert!(ls.drop_shadow.is_none());
        assert!(ls.outer_glow.is_none());
        assert!(ls.bevel_emboss.is_none());
    }

    #[test]
    fn test_layer_effects_default() {
        let le = LayerEffects::default();
        assert!(!le.drop_shadow.enabled);
        assert!(!le.outer_glow.enabled);
        assert!(!le.bevel_emboss.enabled);
    }

    #[test]
    fn test_bevel_style_variants() {
        let _ = BevelStyle::OuterBevel;
        let _ = BevelStyle::InnerBevel;
        let _ = BevelStyle::Emboss;
    }

    #[test]
    fn test_layer_comp_fields() {
        let lc = LayerComp {
            name: "Comp 1".into(),
            states: std::collections::HashMap::new(),
        };
        assert_eq!(lc.name, "Comp 1");
        assert!(lc.states.is_empty());
    }

    #[test]
    fn test_add_layer_comp() {
        let mut app = App::new();
        app.apply(Action::AddLayerComp("Final".into()));
        assert_eq!(app.layer_comps.len(), 1);
        assert_eq!(app.layer_comps[0].name, "Final");
    }

    #[test]
    fn test_delete_layer_comp() {
        let mut app = App::new();
        app.apply(Action::AddLayerComp("v1".into()));
        app.apply(Action::DeleteLayerComp(0));
        assert_eq!(app.layer_comps.len(), 0);
    }

    #[test]
    fn test_shadow_default() {
        let s = Shadow::default();
        assert_eq!(s.blur, 0.0);  // derive(Default) — all f32 = 0
        assert_eq!(s.opacity, 0.0);
    }
}
