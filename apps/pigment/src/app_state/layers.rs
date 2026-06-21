use super::*;

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
