use super::*;

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
