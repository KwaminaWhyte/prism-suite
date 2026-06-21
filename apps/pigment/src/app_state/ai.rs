use super::*;

// ---- New Feature: GenerativeFill ---------------------------------------------

/// One pending generative-fill result that the user can cycle through or accept.
#[derive(Debug, Clone)]
pub struct GenerativeFillResult {
    pub layer_id: usize,
    pub prompt: String,
    pub variation_index: usize,
    pub variation_count: usize,
}

// ---- Batch 5 (new): Sky Replacement -----------------------------------------

/// Built-in sky presets for Sky Replacement.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum SkyPreset {
    #[default]
    BlueSky,
    SunsetOrange,
    StormyClouds,
    StarryNight,
    CustomImage,
}

/// All tuning parameters for the Sky Replacement feature.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkyReplaceConfig {
    pub preset: SkyPreset,
    /// Sky brightness 0..=200, default 100.
    pub brightness: f32,
    /// Colour temperature shift −100..=100, default 0.
    pub temperature: f32,
    /// Scale multiplier 0.5..=2.0, default 1.0.
    pub scale: f32,
    pub flip: bool,
    /// Edge fade amount 0..=100, default 20.
    pub fade_edge: f32,
    /// Foreground lighting blend 0..=100, default 50.
    pub foreground_lighting: f32,
    /// When true, output sky + lighting as new layers.
    pub output_new_layers: bool,
}

impl Default for SkyReplaceConfig {
    fn default() -> Self {
        Self {
            preset: SkyPreset::BlueSky,
            brightness: 100.0,
            temperature: 0.0,
            scale: 1.0,
            flip: false,
            fade_edge: 20.0,
            foreground_lighting: 50.0,
            output_new_layers: true,
        }
    }
}

// ---- Batch 5 (new): Select Subject (AI stub) --------------------------------

/// Whether Select Subject inference runs on-device or in Photoshop's cloud.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum SelectSubjectMode {
    #[default]
    Device,
    Cloud,
}

/// Stub result returned after a Select Subject inference pass.
#[derive(Debug, Clone)]
pub struct SelectSubjectResult {
    /// Fraction of canvas covered by the estimated subject mask (0..=1).
    pub coverage: f32,
    /// Model confidence (0..=1).
    pub confidence: f32,
    /// True when the Cloud inference path was used.
    pub cloud_used: bool,
}

impl App {
    pub(super) fn apply_ai(&mut self, action: Action) {
        match action {
            Action::SetGenerativeFillPrompt(prompt) => {
                self.generative_fill_prompt = prompt;
            }
            Action::RunGenerativeFill { layer_id } => {
                let prompt = self.generative_fill_prompt.clone();
                self.generative_fill_results.push(GenerativeFillResult {
                    layer_id,
                    prompt,
                    variation_index: 0,
                    variation_count: 4,
                });
            }
            Action::CycleGenerativeFillVariation { result_index } => {
                if let Some(r) = self.generative_fill_results.get_mut(result_index) {
                    r.variation_index = (r.variation_index + 1) % r.variation_count;
                }
            }
            Action::AcceptGenerativeFill { result_index } => {
                if result_index < self.generative_fill_results.len() {
                    self.generative_fill_results.remove(result_index);
                }
            }
            Action::DiscardGenerativeFill { result_index } => {
                if result_index < self.generative_fill_results.len() {
                    self.generative_fill_results.remove(result_index);
                }
            }
            Action::SetSkyPreset(p) => {
                self.sky_replace_config.preset = p;
            }
            Action::SetSkyBrightness(v) => {
                self.sky_replace_config.brightness = v.clamp(0.0, 200.0);
            }
            Action::SetSkyTemperature(v) => {
                self.sky_replace_config.temperature = v.clamp(-100.0, 100.0);
            }
            Action::SetSkyScale(v) => {
                self.sky_replace_config.scale = v.clamp(0.5, 2.0);
            }
            Action::SetSkyFlip(b) => {
                self.sky_replace_config.flip = b;
            }
            Action::SetSkyFadeEdge(v) => {
                self.sky_replace_config.fade_edge = v.clamp(0.0, 100.0);
            }
            Action::SetSkyForegroundLighting(v) => {
                self.sky_replace_config.foreground_lighting = v.clamp(0.0, 100.0);
            }
            Action::SetSkyOutputNewLayers(b) => {
                self.sky_replace_config.output_new_layers = b;
            }
            Action::ApplySkyReplace => {
                self.sky_replaced = true;
            }
            Action::ToggleSkyReplacePanel => {
                self.sky_replace_panel_open = !self.sky_replace_panel_open;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generative_fill_result_fields() {
        let r = GenerativeFillResult {
            layer_id: 42,
            prompt: "sunset".into(),
            variation_index: 0,
            variation_count: 4,
        };
        assert_eq!(r.layer_id, 42);
        assert_eq!(r.variation_count, 4);
    }

    #[test]
    fn test_sky_preset_default() {
        assert_eq!(SkyPreset::default(), SkyPreset::BlueSky);
    }

    #[test]
    fn test_sky_replace_config_default() {
        let cfg = SkyReplaceConfig::default();
        assert_eq!(cfg.brightness, 100.0);
        assert_eq!(cfg.temperature, 0.0);
        assert_eq!(cfg.scale, 1.0);
        assert!(!cfg.flip);
        assert_eq!(cfg.fade_edge, 20.0);
        assert_eq!(cfg.foreground_lighting, 50.0);
        assert!(cfg.output_new_layers);
    }

    #[test]
    fn test_select_subject_mode_default() {
        assert_eq!(SelectSubjectMode::default(), SelectSubjectMode::Device);
    }

    #[test]
    fn test_run_generative_fill() {
        let mut app = App::new();
        app.apply(Action::SetGenerativeFillPrompt("mountains".into()));
        app.apply(Action::RunGenerativeFill { layer_id: 1 });
        assert_eq!(app.generative_fill_results.len(), 1);
        assert_eq!(app.generative_fill_results[0].prompt, "mountains");
        assert_eq!(app.generative_fill_results[0].variation_count, 4);
    }

    #[test]
    fn test_set_sky_brightness_clamps() {
        let mut app = App::new();
        app.apply(Action::SetSkyBrightness(300.0));
        assert_eq!(app.sky_replace_config.brightness, 200.0);
        app.apply(Action::SetSkyBrightness(-10.0));
        assert_eq!(app.sky_replace_config.brightness, 0.0);
    }
}
