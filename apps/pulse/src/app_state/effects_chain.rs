use super::*;

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
            Action::AddTextAnimator(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.text_animator = Some(crate::comp::TextAnimator::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveTextAnimator(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.text_animator = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetTextAnimatorRange { layer_id, start, end } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.range_start = start.clamp(0.0, 1.0);
                        ta.range_end = end.clamp(0.0, 1.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOffsetX { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.offset_x = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOffsetY { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.offset_y = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorRotation { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.rotation_deg = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorScale { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.scale = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetTextAnimatorOpacity { layer_id, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(ta) = &mut l.text_animator {
                        ta.opacity = value;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetShapeTrimPaths { layer_id, start, end, offset } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.shape.trim_paths = Some(crate::comp::TrimPaths {
                        start: start.clamp(0.0, 1.0),
                        end: end.clamp(0.0, 1.0),
                        offset: offset.clamp(0.0, 1.0),
                    });
                    self.host.mark_dirty();
                }
            }
            Action::ClearShapeTrimPaths(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.trim_paths = None;
                    self.host.mark_dirty();
                }
            }
            Action::AddShapeRepeater(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.repeater = Some(crate::comp::ShapeRepeater::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveShapeRepeater(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.shape.repeater = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetRepeaterCopies { layer_id, copies } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.copies = copies.max(1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterOffset { layer_id, x, y } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.offset_x = x;
                        r.offset_y = y;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterRotation { layer_id, deg } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.rotation_deg = deg;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterScale { layer_id, scale } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.scale = scale;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetRepeaterOpacity { layer_id, start, end } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(r) = &mut l.shape.repeater {
                        r.opacity_start = start.clamp(0.0, 1.0);
                        r.opacity_end = end.clamp(0.0, 1.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AddLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.lumetri = Some(crate::comp::LumetriColor::default());
                    self.host.mark_dirty();
                }
            }
            Action::RemoveLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    l.lumetri = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetLumetriParam { layer_id, param, value } => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if let Some(lc) = &mut l.lumetri {
                        match param {
                            "exposure"    => lc.exposure = value,
                            "contrast"    => lc.contrast = value,
                            "highlights"  => lc.highlights = value,
                            "shadows"     => lc.shadows = value,
                            "whites"      => lc.whites = value,
                            "blacks"      => lc.blacks = value,
                            "temperature" => lc.temperature = value,
                            "tint"        => lc.tint = value,
                            "saturation"  => lc.saturation = value,
                            "vibrance"    => lc.vibrance = value,
                            _             => {}
                        }
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ToggleLumetriEnabled(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    if let Some(lc) = &mut l.lumetri {
                        lc.enabled = !lc.enabled;
                        self.host.mark_dirty();
                    }
                }
            }
            Action::ResetLumetriColor(id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(id) {
                    if l.lumetri.is_some() {
                        l.lumetri = Some(crate::comp::LumetriColor::default());
                        self.host.mark_dirty();
                    }
                }
            }
            Action::AddColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if l.color_finesse.is_none() {
                        l.color_finesse = Some(crate::comp::ColorFinesse::default());
                        self.host.mark_dirty();
                    }
                }
            }
            Action::RemoveColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    l.color_finesse = None;
                    self.host.mark_dirty();
                }
            }
            Action::SetColorFinesseEnabled { layer_id, enabled } => {
                let ci = self.active_comp_index();
                if let Some(cf) = self.project.comps[ci]
                    .layers.get_mut(layer_id)
                    .and_then(|l| l.color_finesse.as_mut())
                {
                    cf.enabled = enabled;
                    self.host.mark_dirty();
                }
            }
            Action::SetColorFinesseParam { layer_id, range, prop, value } => {
                let ci = self.active_comp_index();
                if let Some(cf) = self.project.comps[ci]
                    .layers.get_mut(layer_id)
                    .and_then(|l| l.color_finesse.as_mut())
                {
                    let r = match range {
                        "master"   => &mut cf.master,
                        "reds"     => &mut cf.reds,
                        "yellows"  => &mut cf.yellows,
                        "greens"   => &mut cf.greens,
                        "cyans"    => &mut cf.cyans,
                        "blues"    => &mut cf.blues,
                        "magentas" => &mut cf.magentas,
                        _          => return,
                    };
                    match prop {
                        "hue"        => r.hue_shift = value,
                        "saturation" => r.saturation = value,
                        "lightness"  => r.lightness = value,
                        _            => return,
                    }
                    self.host.mark_dirty();
                }
            }
            Action::ResetColorFinesse(layer_id) => {
                let ci = self.active_comp_index();
                if let Some(l) = self.project.comps[ci].layers.get_mut(layer_id) {
                    if l.color_finesse.is_some() {
                        l.color_finesse = Some(crate::comp::ColorFinesse::default());
                        self.host.mark_dirty();
                    }
                }
            }
            _ => unreachable!("apply_effects_chain called with wrong action"),
        }
    }
}
