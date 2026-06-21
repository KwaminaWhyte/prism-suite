use super::*;

impl App {
    pub(super) fn apply_text_anim(&mut self, action: Action) {
        match action {
            // --- Batch 6: Essential Graphics / MoGrt ---
            Action::ToggleMoGrtPanel => {
                self.mogrt_panel_open = !self.mogrt_panel_open;
            }
            Action::AddMoGrtTemplate(name) => {
                self.mogrt_templates.push(MotionGraphicTemplate {
                    name,
                    controls: Vec::new(),
                });
            }
            Action::RemoveMoGrtTemplate(idx) => {
                if idx < self.mogrt_templates.len() {
                    self.mogrt_templates.remove(idx);
                    if let Some(sel) = self.mogrt_selected {
                        if sel >= self.mogrt_templates.len() {
                            self.mogrt_selected = self.mogrt_templates.len().checked_sub(1);
                        }
                    }
                }
            }
            Action::SelectMoGrtTemplate(idx) => {
                if idx < self.mogrt_templates.len() {
                    self.mogrt_selected = Some(idx);
                }
            }
            Action::AddMoGrtControl { template_idx, control } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    t.controls.push(control);
                }
            }
            Action::RemoveMoGrtControl { template_idx, control_idx } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if control_idx < t.controls.len() {
                        t.controls.remove(control_idx);
                    }
                }
            }
            Action::SetMoGrtTextValue { template_idx, control_idx, value } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if let Some(MoGrtControl::Text { value: v, .. }) = t.controls.get_mut(control_idx) {
                        *v = value;
                    }
                }
            }
            Action::SetMoGrtColorValue { template_idx, control_idx, value } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if let Some(MoGrtControl::Color { value: v, .. }) = t.controls.get_mut(control_idx) {
                        *v = value;
                    }
                }
            }
            Action::SetMoGrtSliderValue { template_idx, control_idx, value } => {
                if let Some(t) = self.mogrt_templates.get_mut(template_idx) {
                    if let Some(MoGrtControl::Slider { value: v, min, max, .. }) = t.controls.get_mut(control_idx) {
                        *v = value.clamp(*min, *max);
                    }
                }
            }
            Action::ExportMoGrt { template_idx, path } => {
                if let Some(t) = self.mogrt_templates.get(template_idx) {
                    let mut controls_json = Vec::new();
                    for c in &t.controls {
                        let entry = match c {
                            MoGrtControl::Text { label, value } => {
                                format!(r#"{{"type":"text","label":{:?},"value":{:?}}}"#, label, value)
                            }
                            MoGrtControl::Color { label, value } => {
                                format!(r#"{{"type":"color","label":{:?},"value":[{},{},{},{}]}}"#,
                                    label, value[0], value[1], value[2], value[3])
                            }
                            MoGrtControl::Slider { label, value, min, max } => {
                                format!(r#"{{"type":"slider","label":{:?},"value":{},"min":{},"max":{}}}"#,
                                    label, value, min, max)
                            }
                        };
                        controls_json.push(entry);
                    }
                    let json = format!(
                        r#"{{"name":{:?},"controls":[{}]}}"#,
                        t.name,
                        controls_json.join(",")
                    );
                    let _ = std::fs::write(path, json);
                }
            }

            // --- Batch 5: Audio Spectrum / Waveform Effects ---
            Action::SetAudioVisMode(m) => {
                self.audio_spectrum_config.mode = m;
            }
            Action::SetAudioVisLayer(l) => {
                self.audio_spectrum_config.audio_layer = l;
            }
            Action::SetAudioStartFreq(v) => {
                self.audio_spectrum_config.start_freq = v.clamp(1.0, 22000.0);
            }
            Action::SetAudioEndFreq(v) => {
                let min = self.audio_spectrum_config.start_freq + 1.0;
                self.audio_spectrum_config.end_freq = v.clamp(min, 22000.0);
            }
            Action::SetAudioMaxHeight(v) => {
                self.audio_spectrum_config.max_height = v.clamp(1.0, 2000.0);
            }
            Action::SetAudioVisSide(s) => {
                self.audio_spectrum_config.side = s;
            }
            Action::SetAudioSoftness(v) => {
                self.audio_spectrum_config.softness = v.clamp(0.0, 100.0);
            }
            Action::SetAudioMirror(b) => {
                self.audio_spectrum_config.mirror = b;
            }
            Action::SetAudioDisplayedSamples(n) => {
                self.audio_spectrum_config.displayed_samples = n.clamp(2, 4096);
            }
            Action::SetAudioFrequencyBands(n) => {
                self.audio_spectrum_config.frequency_bands = n.clamp(2, 1024);
            }
            Action::SetAudioThickness(v) => {
                self.audio_spectrum_config.thickness = v.clamp(0.1, 100.0);
            }
            Action::SetAudioDigital(b) => {
                self.audio_spectrum_config.digital = b;
            }
            Action::ApplyAudioSpectrumEffect { layer_id } => {
                self.audio_spectrum_layer = Some(layer_id);
            }

            // --- Batch 7: TextAnimator (app-level) ---
            Action::AddTextAnimatorExt { layer_id } => {
                self.text_anim_counter += 1;
                let id = self.text_anim_counter;
                self.text_animators.push(TextAnimator {
                    id,
                    layer_id,
                    name: format!("Animator {}", id),
                    preset: None,
                    properties: TextAnimProperty::default(),
                    range: TextAnimRange::default(),
                });
            }
            Action::RemoveTextAnimatorExt(id) => {
                self.text_animators.retain(|a| a.id != id);
            }
            Action::ApplyTextAnimPreset { animator_id, preset } => {
                if let Some(anim) = self.text_animators.iter_mut().find(|a| a.id == animator_id) {
                    anim.preset = Some(preset);
                }
            }
            Action::SetTextAnimRange { animator_id, start, end } => {
                if let Some(anim) = self.text_animators.iter_mut().find(|a| a.id == animator_id) {
                    anim.range.start = start.clamp(0.0, 100.0);
                    anim.range.end = end.clamp(0.0, 100.0);
                }
            }
            Action::SetTextAnimRangeUnits { animator_id, units } => {
                if let Some(anim) = self.text_animators.iter_mut().find(|a| a.id == animator_id) {
                    anim.range.units = units;
                }
            }
            Action::SetTextAnimBasedOn { animator_id, based_on } => {
                if let Some(anim) = self.text_animators.iter_mut().find(|a| a.id == animator_id) {
                    anim.range.based_on = based_on;
                }
            }

            // --- Batch 7: EssentialGraphics / MoGRT v2 ---
            Action::OpenEssentialGraphics => {
                self.essential_graphics_open = true;
            }
            Action::CloseEssentialGraphics => {
                self.essential_graphics_open = false;
            }
            Action::CreateMogrTemplate { name, composition_id } => {
                self.mogrt_counter += 1;
                let id = self.mogrt_counter;
                self.mogrt_templates_v2.push(MogrTemplate {
                    id,
                    name,
                    description: String::new(),
                    params: Vec::new(),
                    composition_id: Some(composition_id),
                    is_responsive: false,
                });
            }
            Action::AddMogrParam { template_id, param } => {
                if let Some(t) = self.mogrt_templates_v2.iter_mut().find(|t| t.id == template_id) {
                    t.params.push(param);
                }
            }
            Action::SetMogrParamValue { template_id, param_id, value } => {
                if let Some(t) = self.mogrt_templates_v2.iter_mut().find(|t| t.id == template_id) {
                    if let Some(p) = t.params.iter_mut().find(|p| p.id == param_id) {
                        p.value = value;
                    }
                }
            }
            Action::ExportMogrt { template_id } => {
                if let Some(t) = self.mogrt_templates_v2.iter_mut().find(|t| t.id == template_id) {
                    t.is_responsive = true;
                }
            }
            Action::DeleteMogrTemplate(tid) => {
                self.mogrt_templates_v2.retain(|t| t.id != tid);
            }

            _ => unreachable!("apply_text_anim called with wrong action"),
        }
    }
}
