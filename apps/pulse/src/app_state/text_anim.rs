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

            _ => unreachable!("apply_text_anim called with wrong action"),
        }
    }
}
