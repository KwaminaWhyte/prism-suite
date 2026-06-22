//! Plugins domain — VST/AU/CLAP plugin instances, builtin synths + apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// The plugin format / standard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginFormat {
    Vst2,
    Vst3,
    Au,
    Clap,
    Builtin,
}

/// A loaded plugin instance on a track.
#[derive(Clone, Debug)]
pub struct PluginInstance {
    pub id: usize,
    pub track_id: usize,
    pub name: String,
    pub format: PluginFormat,
    pub path: String,
    pub enabled: bool,
    pub preset_name: String,
    pub param_values: Vec<(String, f32)>,
    pub is_instrument: bool,
    pub latency_samples: u32,
    pub plugin_window_open: bool,
}

/// Sampler loop mode.
#[derive(Clone, Debug, PartialEq)]
pub enum SamplerLoop {
    None,
    Forward,
    PingPong,
}

/// The drum kit preset for the drum machine builtin.
#[derive(Clone, Debug, PartialEq)]
pub enum DrumKit {
    Default,
    Electronic,
    Acoustic,
    Custom(String),
}

/// The kind of builtin synthesizer.
#[derive(Clone, Debug)]
pub enum BuiltinSynthKind {
    SimpleSine,
    PolySubtract { cutoff: f32, resonance: f32, adsr: [f32; 4] },
    Sampler { sample_path: Option<String>, loop_mode: SamplerLoop },
    DrumMachine { kit: DrumKit },
}

/// A builtin synthesizer assigned to a track.
#[derive(Clone, Debug)]
pub struct BuiltinSynth {
    pub kind: BuiltinSynthKind,
    pub track_id: usize,
    pub octave_offset: i32,
    pub volume: f32,
    pub mono: bool,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_plugins(&mut self, action: Action) {
        match action {
            Action::AddPlugin { track_id, name, format, path, is_instrument } => {
                let id = self.next_plugin_id;
                self.next_plugin_id += 1;
                self.plugin_instances.push(PluginInstance {
                    id,
                    track_id,
                    name,
                    format,
                    path,
                    enabled: true,
                    preset_name: String::new(),
                    param_values: Vec::new(),
                    is_instrument,
                    latency_samples: 0,
                    plugin_window_open: false,
                });
            }
            Action::RemovePlugin { plugin_id } => {
                self.plugin_instances.retain(|p| p.id != plugin_id);
            }
            Action::TogglePlugin { plugin_id } => {
                if let Some(p) = self.plugin_instances.iter_mut().find(|p| p.id == plugin_id) {
                    p.enabled = !p.enabled;
                }
            }
            Action::SetPluginPreset { plugin_id, preset_name } => {
                if let Some(p) = self.plugin_instances.iter_mut().find(|p| p.id == plugin_id) {
                    p.preset_name = preset_name;
                }
            }
            Action::SetPluginParam { plugin_id, param_name, value } => {
                if let Some(p) = self.plugin_instances.iter_mut().find(|p| p.id == plugin_id) {
                    if let Some(pv) = p.param_values.iter_mut().find(|(n, _)| *n == param_name) {
                        pv.1 = value;
                    } else {
                        p.param_values.push((param_name, value));
                    }
                }
            }
            Action::OpenPluginWindow { plugin_id } => {
                if let Some(p) = self.plugin_instances.iter_mut().find(|p| p.id == plugin_id) {
                    p.plugin_window_open = true;
                }
            }
            Action::ClosePluginWindow { plugin_id } => {
                if let Some(p) = self.plugin_instances.iter_mut().find(|p| p.id == plugin_id) {
                    p.plugin_window_open = false;
                }
            }
            Action::AddBuiltinSynth { track_id, kind } => {
                self.builtin_synths.push(BuiltinSynth {
                    kind,
                    track_id,
                    octave_offset: 0,
                    volume: 1.0,
                    mono: false,
                });
            }
            Action::RemoveBuiltinSynth { track_id } => {
                self.builtin_synths.retain(|s| s.track_id != track_id);
            }
            Action::SetBuiltinSynthOctave { track_id, octave } => {
                if let Some(s) = self.builtin_synths.iter_mut().find(|s| s.track_id == track_id) {
                    s.octave_offset = octave.clamp(-4, 4);
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::{BuiltinSynthKind, PluginFormat};

    fn fresh() -> App {
        App::new()
    }

    fn make_plugin(app: &mut App, track_id: usize) -> usize {
        let before = app.plugin_instances.len();
        app.apply(Action::AddPlugin {
            track_id,
            name: "TestPlugin".to_string(),
            format: PluginFormat::Vst3,
            path: "/path/to/plugin.vst3".to_string(),
            is_instrument: false,
        });
        app.plugin_instances[before].id
    }

    #[test]
    fn add_plugin() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        assert_eq!(app.plugin_instances.len(), 1);
        assert_eq!(app.plugin_instances[0].id, pid);
        assert!(app.plugin_instances[0].enabled);
        assert!(!app.plugin_instances[0].plugin_window_open);
    }

    #[test]
    fn add_multiple_plugins_increment_id() {
        let mut app = fresh();
        let p1 = make_plugin(&mut app, 0);
        let p2 = make_plugin(&mut app, 0);
        assert_ne!(p1, p2);
    }

    #[test]
    fn remove_plugin() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        app.apply(Action::RemovePlugin { plugin_id: pid });
        assert!(app.plugin_instances.is_empty());
    }

    #[test]
    fn remove_nonexistent_plugin_is_noop() {
        let mut app = fresh();
        make_plugin(&mut app, 0);
        app.apply(Action::RemovePlugin { plugin_id: 9999 });
        assert_eq!(app.plugin_instances.len(), 1);
    }

    #[test]
    fn toggle_plugin() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        assert!(app.plugin_instances[0].enabled);
        app.apply(Action::TogglePlugin { plugin_id: pid });
        assert!(!app.plugin_instances[0].enabled);
        app.apply(Action::TogglePlugin { plugin_id: pid });
        assert!(app.plugin_instances[0].enabled);
    }

    #[test]
    fn set_plugin_preset() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        app.apply(Action::SetPluginPreset { plugin_id: pid, preset_name: "Warm Bass".to_string() });
        assert_eq!(app.plugin_instances[0].preset_name, "Warm Bass");
    }

    #[test]
    fn set_plugin_param_adds_new() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        app.apply(Action::SetPluginParam { plugin_id: pid, param_name: "cutoff".to_string(), value: 0.7 });
        assert_eq!(app.plugin_instances[0].param_values.len(), 1);
        assert_eq!(app.plugin_instances[0].param_values[0].0, "cutoff");
        assert!((app.plugin_instances[0].param_values[0].1 - 0.7).abs() < 0.001);
    }

    #[test]
    fn set_plugin_param_updates_existing() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        app.apply(Action::SetPluginParam { plugin_id: pid, param_name: "cutoff".to_string(), value: 0.7 });
        app.apply(Action::SetPluginParam { plugin_id: pid, param_name: "cutoff".to_string(), value: 0.9 });
        assert_eq!(app.plugin_instances[0].param_values.len(), 1);
        assert!((app.plugin_instances[0].param_values[0].1 - 0.9).abs() < 0.001);
    }

    #[test]
    fn open_plugin_window() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        app.apply(Action::OpenPluginWindow { plugin_id: pid });
        assert!(app.plugin_instances[0].plugin_window_open);
    }

    #[test]
    fn close_plugin_window() {
        let mut app = fresh();
        let pid = make_plugin(&mut app, 0);
        app.apply(Action::OpenPluginWindow { plugin_id: pid });
        app.apply(Action::ClosePluginWindow { plugin_id: pid });
        assert!(!app.plugin_instances[0].plugin_window_open);
    }

    #[test]
    fn add_builtin_synth() {
        let mut app = fresh();
        app.apply(Action::AddBuiltinSynth { track_id: 1, kind: BuiltinSynthKind::SimpleSine });
        assert_eq!(app.builtin_synths.len(), 1);
        assert_eq!(app.builtin_synths[0].track_id, 1);
        assert_eq!(app.builtin_synths[0].octave_offset, 0);
        assert!((app.builtin_synths[0].volume - 1.0).abs() < 0.001);
    }

    #[test]
    fn remove_builtin_synth() {
        let mut app = fresh();
        app.apply(Action::AddBuiltinSynth { track_id: 1, kind: BuiltinSynthKind::SimpleSine });
        app.apply(Action::RemoveBuiltinSynth { track_id: 1 });
        assert!(app.builtin_synths.is_empty());
    }

    #[test]
    fn set_builtin_synth_octave_clamped() {
        let mut app = fresh();
        app.apply(Action::AddBuiltinSynth { track_id: 0, kind: BuiltinSynthKind::SimpleSine });
        app.apply(Action::SetBuiltinSynthOctave { track_id: 0, octave: 10 });
        assert!(app.builtin_synths[0].octave_offset <= 4);
        app.apply(Action::SetBuiltinSynthOctave { track_id: 0, octave: -10 });
        assert!(app.builtin_synths[0].octave_offset >= -4);
    }

    #[test]
    fn set_builtin_synth_octave_normal() {
        let mut app = fresh();
        app.apply(Action::AddBuiltinSynth { track_id: 0, kind: BuiltinSynthKind::SimpleSine });
        app.apply(Action::SetBuiltinSynthOctave { track_id: 0, octave: 2 });
        assert_eq!(app.builtin_synths[0].octave_offset, 2);
    }
}
