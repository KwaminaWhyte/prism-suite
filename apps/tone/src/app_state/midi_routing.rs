use super::{App, Action};

#[derive(Clone, Debug)]
pub struct MidiOutputPort {
    pub id: usize,
    pub name: String,
    pub device: String,
    pub channel: u8,
    pub enabled: bool,
    pub transpose: i8,
    pub velocity_scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VirtualInstrumentKind {
    SubtractiveSynth,
    FmSynth,
    WavetableSynth,
    GranularSynth,
    Sampler,
    DrumMachine,
    Arpeggiator,
    Chord,
}

#[derive(Clone, Debug)]
pub struct VirtualInstrument {
    pub id: usize,
    pub name: String,
    pub kind: VirtualInstrumentKind,
    pub preset_name: String,
    pub volume: f32,
    pub pan: f32,
    pub polyphony: u8,
    pub pitch_bend_range: u8,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArpPattern {
    Up,
    Down,
    UpDown,
    DownUp,
    Random,
    AsPlayed,
    Chord,
}

#[derive(Clone, Debug)]
pub struct ArpConfig {
    pub id: usize,
    pub track_id: usize,
    pub pattern: ArpPattern,
    pub rate: f32,
    pub octave_range: u8,
    pub swing: f32,
    pub gate: f32,
    pub enabled: bool,
}

impl App {
    pub(crate) fn apply_midi_routing(&mut self, action: Action) {
        match action {
            Action::AddMidiOutputPort { name, device, channel } => {
                let id = self.next_midi_port_id;
                self.next_midi_port_id += 1;
                self.midi_output_ports.push(MidiOutputPort {
                    id,
                    name,
                    device,
                    channel: channel.clamp(1, 16),
                    enabled: true,
                    transpose: 0,
                    velocity_scale: 1.0,
                });
            }
            Action::RemoveMidiOutputPort { port_id } => {
                self.midi_output_ports.retain(|p| p.id != port_id);
            }
            Action::SetMidiPortEnabled { port_id, enabled } => {
                if let Some(p) = self.midi_output_ports.iter_mut().find(|p| p.id == port_id) {
                    p.enabled = enabled;
                }
            }
            Action::SetMidiPortTranspose { port_id, semitones } => {
                if let Some(p) = self.midi_output_ports.iter_mut().find(|p| p.id == port_id) {
                    p.transpose = semitones.clamp(-24, 24);
                }
            }
            Action::SetMidiPortVelocityScale { port_id, scale } => {
                if let Some(p) = self.midi_output_ports.iter_mut().find(|p| p.id == port_id) {
                    p.velocity_scale = scale.clamp(0.0, 2.0);
                }
            }
            Action::AddVirtualInstrument { name, kind } => {
                let id = self.next_vi_id;
                self.next_vi_id += 1;
                self.virtual_instruments.push(VirtualInstrument {
                    id,
                    name,
                    kind,
                    preset_name: "Default".to_string(),
                    volume: 1.0,
                    pan: 0.0,
                    polyphony: 8,
                    pitch_bend_range: 2,
                    enabled: true,
                });
            }
            Action::RemoveVirtualInstrument { vi_id } => {
                self.virtual_instruments.retain(|v| v.id != vi_id);
            }
            Action::SetVirtualInstrumentPreset { vi_id, preset } => {
                if let Some(v) = self.virtual_instruments.iter_mut().find(|v| v.id == vi_id) {
                    v.preset_name = preset;
                }
            }
            Action::SetVirtualInstrumentPolyphony { vi_id, voices } => {
                if let Some(v) = self.virtual_instruments.iter_mut().find(|v| v.id == vi_id) {
                    v.polyphony = voices;
                }
            }
            Action::ToggleVirtualInstrument { vi_id } => {
                if let Some(v) = self.virtual_instruments.iter_mut().find(|v| v.id == vi_id) {
                    v.enabled = !v.enabled;
                }
            }
            Action::AddArpeggiator { track_id } => {
                let id = self.next_arp_id;
                self.next_arp_id += 1;
                self.arp_configs.push(ArpConfig {
                    id,
                    track_id,
                    pattern: ArpPattern::Up,
                    rate: 0.25,
                    octave_range: 1,
                    swing: 0.0,
                    gate: 1.0,
                    enabled: true,
                });
            }
            Action::RemoveArpeggiator { arp_id } => {
                self.arp_configs.retain(|a| a.id != arp_id);
            }
            Action::SetArpPattern { arp_id, pattern } => {
                if let Some(a) = self.arp_configs.iter_mut().find(|a| a.id == arp_id) {
                    a.pattern = pattern;
                }
            }
            Action::SetArpRate { arp_id, rate } => {
                if let Some(a) = self.arp_configs.iter_mut().find(|a| a.id == arp_id) {
                    a.rate = rate.max(0.0);
                }
            }
            Action::SetArpOctaveRange { arp_id, octaves } => {
                if let Some(a) = self.arp_configs.iter_mut().find(|a| a.id == arp_id) {
                    a.octave_range = octaves;
                }
            }
            Action::ToggleArpeggiator { arp_id } => {
                if let Some(a) = self.arp_configs.iter_mut().find(|a| a.id == arp_id) {
                    a.enabled = !a.enabled;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App { App::new() }

    #[test]
    fn add_midi_output_port() {
        let mut app = fresh();
        app.apply(Action::AddMidiOutputPort { name: "Out 1".to_string(), device: "IAC Driver".to_string(), channel: 1 });
        assert_eq!(app.midi_output_ports.len(), 1);
        assert_eq!(app.midi_output_ports[0].channel, 1);
        assert!(app.midi_output_ports[0].enabled);
    }

    #[test]
    fn remove_midi_output_port() {
        let mut app = fresh();
        app.apply(Action::AddMidiOutputPort { name: "Out 1".to_string(), device: "IAC".to_string(), channel: 1 });
        let pid = app.midi_output_ports[0].id;
        app.apply(Action::RemoveMidiOutputPort { port_id: pid });
        assert!(app.midi_output_ports.is_empty());
    }

    #[test]
    fn set_midi_port_enabled() {
        let mut app = fresh();
        app.apply(Action::AddMidiOutputPort { name: "p".to_string(), device: "d".to_string(), channel: 1 });
        let pid = app.midi_output_ports[0].id;
        app.apply(Action::SetMidiPortEnabled { port_id: pid, enabled: false });
        assert!(!app.midi_output_ports[0].enabled);
    }

    #[test]
    fn set_midi_port_transpose_clamped() {
        let mut app = fresh();
        app.apply(Action::AddMidiOutputPort { name: "p".to_string(), device: "d".to_string(), channel: 1 });
        let pid = app.midi_output_ports[0].id;
        app.apply(Action::SetMidiPortTranspose { port_id: pid, semitones: 30 });
        assert_eq!(app.midi_output_ports[0].transpose, 24);
    }

    #[test]
    fn set_midi_port_velocity_scale_clamped() {
        let mut app = fresh();
        app.apply(Action::AddMidiOutputPort { name: "p".to_string(), device: "d".to_string(), channel: 1 });
        let pid = app.midi_output_ports[0].id;
        app.apply(Action::SetMidiPortVelocityScale { port_id: pid, scale: 3.0 });
        assert!((app.midi_output_ports[0].velocity_scale - 2.0).abs() < 0.01);
    }

    #[test]
    fn add_virtual_instrument() {
        let mut app = fresh();
        app.apply(Action::AddVirtualInstrument { name: "Synth 1".to_string(), kind: VirtualInstrumentKind::SubtractiveSynth });
        assert_eq!(app.virtual_instruments.len(), 1);
        assert!(app.virtual_instruments[0].enabled);
        assert_eq!(app.virtual_instruments[0].polyphony, 8);
    }

    #[test]
    fn remove_virtual_instrument() {
        let mut app = fresh();
        app.apply(Action::AddVirtualInstrument { name: "VI".to_string(), kind: VirtualInstrumentKind::FmSynth });
        let vid = app.virtual_instruments[0].id;
        app.apply(Action::RemoveVirtualInstrument { vi_id: vid });
        assert!(app.virtual_instruments.is_empty());
    }

    #[test]
    fn set_virtual_instrument_preset() {
        let mut app = fresh();
        app.apply(Action::AddVirtualInstrument { name: "VI".to_string(), kind: VirtualInstrumentKind::Sampler });
        let vid = app.virtual_instruments[0].id;
        app.apply(Action::SetVirtualInstrumentPreset { vi_id: vid, preset: "Grand Piano".to_string() });
        assert_eq!(app.virtual_instruments[0].preset_name, "Grand Piano");
    }

    #[test]
    fn toggle_virtual_instrument() {
        let mut app = fresh();
        app.apply(Action::AddVirtualInstrument { name: "VI".to_string(), kind: VirtualInstrumentKind::DrumMachine });
        let vid = app.virtual_instruments[0].id;
        app.apply(Action::ToggleVirtualInstrument { vi_id: vid });
        assert!(!app.virtual_instruments[0].enabled);
    }

    #[test]
    fn add_arpeggiator() {
        let mut app = fresh();
        app.apply(Action::AddArpeggiator { track_id: 5 });
        assert_eq!(app.arp_configs.len(), 1);
        assert_eq!(app.arp_configs[0].track_id, 5);
        assert_eq!(app.arp_configs[0].pattern, ArpPattern::Up);
    }

    #[test]
    fn remove_arpeggiator() {
        let mut app = fresh();
        app.apply(Action::AddArpeggiator { track_id: 5 });
        let aid = app.arp_configs[0].id;
        app.apply(Action::RemoveArpeggiator { arp_id: aid });
        assert!(app.arp_configs.is_empty());
    }

    #[test]
    fn set_arp_pattern() {
        let mut app = fresh();
        app.apply(Action::AddArpeggiator { track_id: 0 });
        let aid = app.arp_configs[0].id;
        app.apply(Action::SetArpPattern { arp_id: aid, pattern: ArpPattern::UpDown });
        assert_eq!(app.arp_configs[0].pattern, ArpPattern::UpDown);
    }

    #[test]
    fn toggle_arpeggiator() {
        let mut app = fresh();
        app.apply(Action::AddArpeggiator { track_id: 0 });
        let aid = app.arp_configs[0].id;
        app.apply(Action::ToggleArpeggiator { arp_id: aid });
        assert!(!app.arp_configs[0].enabled);
    }
}
