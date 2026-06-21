use super::*;

/// Built-in text animation presets.
#[derive(Clone, Debug, PartialEq)]
pub enum TextAnimPreset {
    FlyInFromLeft,
    FlyInFromRight,
    FadeIn,
    FadeOut,
    Typewriter,
    Wiggle,
    ScaleUp,
    Bounce,
    SpiralIn,
    Blur,
}

/// Which properties a text animator affects.
#[derive(Clone, Debug, Default)]
pub struct TextAnimProperty {
    pub anchor_point: bool,
    pub position: bool,
    pub scale: bool,
    pub rotation: bool,
    pub opacity: bool,
    pub fill_color: bool,
    pub stroke_color: bool,
    pub blur: bool,
}

/// Range selector for a text animator.
#[derive(Clone, Debug)]
pub struct TextAnimRange {
    /// 0..=100
    pub start: f32,
    pub end: f32,
    pub offset: f32,
    /// "Percentage" or "Index"
    pub units: String,
    /// "Characters", "Words", or "Lines"
    pub based_on: String,
}

impl Default for TextAnimRange {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 100.0,
            offset: 0.0,
            units: "Percentage".to_string(),
            based_on: "Characters".to_string(),
        }
    }
}

/// An app-level text animator (separate from comp-model TextAnimator).
#[derive(Clone, Debug)]
pub struct TextAnimator {
    pub id: usize,
    pub layer_id: usize,
    pub name: String,
    pub preset: Option<TextAnimPreset>,
    pub properties: TextAnimProperty,
    pub range: TextAnimRange,
}

/// Parameter kind in a MOGRT template.
#[derive(Clone, Debug, PartialEq)]
pub enum MogrParamKind {
    Text,
    Color,
    Number,
    Bool,
    Dropdown,
    Slider,
}

/// A single editable parameter in a MOGRT template.
#[derive(Clone, Debug)]
pub struct MogrParam {
    pub id: String,
    pub label: String,
    pub kind: MogrParamKind,
    /// Value serialized as string for simplicity.
    pub value: String,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub options: Vec<String>,
}

/// A Motion Graphics Template (MOGRT) for the Essential Graphics panel.
#[derive(Clone, Debug)]
pub struct MogrTemplate {
    pub id: usize,
    pub name: String,
    pub description: String,
    pub params: Vec<MogrParam>,
    pub composition_id: Option<usize>,
    pub is_responsive: bool,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_text_animator_ext() {
        let mut app = App::new();
        assert!(app.text_animators.is_empty());
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        assert_eq!(app.text_animators.len(), 1);
        assert_eq!(app.text_animators[0].layer_id, 0);
        assert_eq!(app.text_animators[0].name, "Animator 1");
        assert!(app.text_animators[0].preset.is_none());
    }

    #[test]
    fn test_apply_text_anim_preset() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        let id = app.text_animators[0].id;
        app.apply(Action::ApplyTextAnimPreset { animator_id: id, preset: TextAnimPreset::FadeIn });
        assert_eq!(app.text_animators[0].preset, Some(TextAnimPreset::FadeIn));
        app.apply(Action::ApplyTextAnimPreset { animator_id: id, preset: TextAnimPreset::Typewriter });
        assert_eq!(app.text_animators[0].preset, Some(TextAnimPreset::Typewriter));
    }

    #[test]
    fn test_set_text_anim_range() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        let id = app.text_animators[0].id;
        app.apply(Action::SetTextAnimRange { animator_id: id, start: 25.0, end: 75.0 });
        assert!((app.text_animators[0].range.start - 25.0).abs() < 1e-5);
        assert!((app.text_animators[0].range.end - 75.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_text_anim_range_clamp() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        let id = app.text_animators[0].id;
        app.apply(Action::SetTextAnimRange { animator_id: id, start: -10.0, end: 200.0 });
        assert!((app.text_animators[0].range.start - 0.0).abs() < 1e-5);
        assert!((app.text_animators[0].range.end - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_text_anim_units() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        let id = app.text_animators[0].id;
        app.apply(Action::SetTextAnimRangeUnits { animator_id: id, units: "Index".to_string() });
        assert_eq!(app.text_animators[0].range.units, "Index");
    }

    #[test]
    fn test_set_text_anim_based_on() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        let id = app.text_animators[0].id;
        app.apply(Action::SetTextAnimBasedOn { animator_id: id, based_on: "Words".to_string() });
        assert_eq!(app.text_animators[0].range.based_on, "Words");
    }

    #[test]
    fn test_remove_text_animator_ext() {
        let mut app = App::new();
        app.apply(Action::AddTextAnimatorExt { layer_id: 0 });
        app.apply(Action::AddTextAnimatorExt { layer_id: 1 });
        assert_eq!(app.text_animators.len(), 2);
        let id = app.text_animators[0].id;
        app.apply(Action::RemoveTextAnimatorExt(id));
        assert_eq!(app.text_animators.len(), 1);
        assert_eq!(app.text_animators[0].layer_id, 1);
    }

    #[test]
    fn test_essential_graphics_open_close() {
        let mut app = App::new();
        assert!(!app.essential_graphics_open);
        app.apply(Action::OpenEssentialGraphics);
        assert!(app.essential_graphics_open);
        app.apply(Action::CloseEssentialGraphics);
        assert!(!app.essential_graphics_open);
    }

    #[test]
    fn test_create_mogr_template() {
        let mut app = App::new();
        assert!(app.mogrt_templates_v2.is_empty());
        app.apply(Action::CreateMogrTemplate { name: "Lower Third".to_string(), composition_id: 0 });
        assert_eq!(app.mogrt_templates_v2.len(), 1);
        assert_eq!(app.mogrt_templates_v2[0].name, "Lower Third");
        assert_eq!(app.mogrt_templates_v2[0].composition_id, Some(0));
        assert!(!app.mogrt_templates_v2[0].is_responsive);
    }

    #[test]
    fn test_add_mogr_param() {
        let mut app = App::new();
        app.apply(Action::CreateMogrTemplate { name: "T".to_string(), composition_id: 0 });
        let tid = app.mogrt_templates_v2[0].id;
        let param = MogrParam {
            id: "title".to_string(),
            label: "Title".to_string(),
            kind: MogrParamKind::Text,
            value: "Hello".to_string(),
            min: None,
            max: None,
            options: vec![],
        };
        app.apply(Action::AddMogrParam { template_id: tid, param });
        assert_eq!(app.mogrt_templates_v2[0].params.len(), 1);
        assert_eq!(app.mogrt_templates_v2[0].params[0].label, "Title");
    }

    #[test]
    fn test_set_mogr_param_value() {
        let mut app = App::new();
        app.apply(Action::CreateMogrTemplate { name: "T".to_string(), composition_id: 0 });
        let tid = app.mogrt_templates_v2[0].id;
        let param = MogrParam {
            id: "speed".to_string(),
            label: "Speed".to_string(),
            kind: MogrParamKind::Slider,
            value: "50".to_string(),
            min: Some(0.0),
            max: Some(100.0),
            options: vec![],
        };
        app.apply(Action::AddMogrParam { template_id: tid, param });
        app.apply(Action::SetMogrParamValue { template_id: tid, param_id: "speed".to_string(), value: "80".to_string() });
        assert_eq!(app.mogrt_templates_v2[0].params[0].value, "80");
    }

    #[test]
    fn test_export_mogrt_sets_responsive() {
        let mut app = App::new();
        app.apply(Action::CreateMogrTemplate { name: "T".to_string(), composition_id: 0 });
        let tid = app.mogrt_templates_v2[0].id;
        assert!(!app.mogrt_templates_v2[0].is_responsive);
        app.apply(Action::ExportMogrt { template_id: tid });
        assert!(app.mogrt_templates_v2[0].is_responsive);
    }

    #[test]
    fn test_delete_mogr_template() {
        let mut app = App::new();
        app.apply(Action::CreateMogrTemplate { name: "A".to_string(), composition_id: 0 });
        app.apply(Action::CreateMogrTemplate { name: "B".to_string(), composition_id: 0 });
        let tid_a = app.mogrt_templates_v2[0].id;
        app.apply(Action::DeleteMogrTemplate(tid_a));
        assert_eq!(app.mogrt_templates_v2.len(), 1);
        assert_eq!(app.mogrt_templates_v2[0].name, "B");
    }
}
