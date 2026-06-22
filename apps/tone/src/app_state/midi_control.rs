//! MIDI control domain — MIDI devices, CC mappings, MIDI learn + apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// Whether a MIDI device sends, receives, or does both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidiDeviceKind {
    Input,
    Output,
    Both,
}

/// A connected MIDI device.
#[derive(Clone, Debug)]
pub struct MidiDevice {
    pub id: usize,
    pub name: String,
    pub kind: MidiDeviceKind,
    pub enabled: bool,
    /// When Some(ch), only pass messages on this channel (1-16). None = all channels.
    pub channel_filter: Option<u8>,
}

/// The target parameter a MIDI mapping controls.
#[derive(Clone, Debug, PartialEq)]
pub enum MappingTarget {
    TrackVolume(usize),
    TrackPan(usize),
    TrackMute(usize),
    TrackSolo(usize),
    MasterVolume,
    PluginParam { plugin_id: usize, param_name: String },
    TransportPlay,
    TransportStop,
    TransportRecord,
}

/// A MIDI CC → parameter mapping.
#[derive(Clone, Debug)]
pub struct MidiMapping {
    pub id: usize,
    /// 0 = all channels, 1–16 = specific channel.
    pub channel: u8,
    /// CC number 0..=127.
    pub cc: u8,
    pub target: MappingTarget,
    pub min_value: f32,
    pub max_value: f32,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_midi_control(&mut self, action: Action) {
        match action {
            Action::AddMidiMapping { channel, cc, target } => {
                let id = self.next_mapping_id;
                self.next_mapping_id += 1;
                self.midi_mappings.push(MidiMapping {
                    id,
                    channel,
                    cc,
                    target,
                    min_value: 0.0,
                    max_value: 1.0,
                });
            }
            Action::RemoveMidiMapping { mapping_id } => {
                self.midi_mappings.retain(|m| m.id != mapping_id);
            }
            Action::SetMappingRange { mapping_id, min, max } => {
                if let Some(m) = self.midi_mappings.iter_mut().find(|m| m.id == mapping_id) {
                    m.min_value = min;
                    m.max_value = max;
                }
            }
            Action::AddMidiDevice { name, kind } => {
                let id = self.next_device_id;
                self.next_device_id += 1;
                self.midi_devices.push(MidiDevice {
                    id,
                    name,
                    kind,
                    enabled: true,
                    channel_filter: None,
                });
            }
            Action::RemoveMidiDevice { device_id } => {
                self.midi_devices.retain(|d| d.id != device_id);
            }
            Action::EnableMidiDevice { device_id, enabled } => {
                if let Some(d) = self.midi_devices.iter_mut().find(|d| d.id == device_id) {
                    d.enabled = enabled;
                }
            }
            Action::StartMidiLearn { target } => {
                self.midi_learn_active = true;
                self.midi_learn_target = Some(target);
            }
            Action::StopMidiLearn => {
                self.midi_learn_active = false;
                self.midi_learn_target = None;
            }
            Action::CompleteMidiLearn { channel, cc } => {
                if let Some(target) = self.midi_learn_target.take() {
                    let id = self.next_mapping_id;
                    self.next_mapping_id += 1;
                    self.midi_mappings.push(MidiMapping {
                        id,
                        channel,
                        cc,
                        target,
                        min_value: 0.0,
                        max_value: 1.0,
                    });
                }
                self.midi_learn_active = false;
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::{MappingTarget, MidiDeviceKind};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn add_mapping() {
        let mut app = fresh();
        app.apply(Action::AddMidiMapping {
            channel: 1,
            cc: 7,
            target: MappingTarget::TrackVolume(0),
        });
        assert_eq!(app.midi_mappings.len(), 1);
        assert_eq!(app.midi_mappings[0].channel, 1);
        assert_eq!(app.midi_mappings[0].cc, 7);
        assert!((app.midi_mappings[0].min_value - 0.0).abs() < 0.001);
        assert!((app.midi_mappings[0].max_value - 1.0).abs() < 0.001);
    }

    #[test]
    fn add_multiple_mappings_increment_id() {
        let mut app = fresh();
        app.apply(Action::AddMidiMapping { channel: 0, cc: 1, target: MappingTarget::MasterVolume });
        app.apply(Action::AddMidiMapping { channel: 0, cc: 2, target: MappingTarget::TrackPan(0) });
        assert_eq!(app.midi_mappings[0].id, 0);
        assert_eq!(app.midi_mappings[1].id, 1);
    }

    #[test]
    fn remove_mapping() {
        let mut app = fresh();
        app.apply(Action::AddMidiMapping { channel: 0, cc: 7, target: MappingTarget::MasterVolume });
        let mid = app.midi_mappings[0].id;
        app.apply(Action::RemoveMidiMapping { mapping_id: mid });
        assert!(app.midi_mappings.is_empty());
    }

    #[test]
    fn remove_nonexistent_mapping_is_noop() {
        let mut app = fresh();
        app.apply(Action::AddMidiMapping { channel: 0, cc: 7, target: MappingTarget::MasterVolume });
        app.apply(Action::RemoveMidiMapping { mapping_id: 9999 });
        assert_eq!(app.midi_mappings.len(), 1);
    }

    #[test]
    fn set_mapping_range() {
        let mut app = fresh();
        app.apply(Action::AddMidiMapping { channel: 0, cc: 7, target: MappingTarget::MasterVolume });
        let mid = app.midi_mappings[0].id;
        app.apply(Action::SetMappingRange { mapping_id: mid, min: 0.2, max: 0.8 });
        assert!((app.midi_mappings[0].min_value - 0.2).abs() < 0.001);
        assert!((app.midi_mappings[0].max_value - 0.8).abs() < 0.001);
    }

    #[test]
    fn add_midi_device() {
        let mut app = fresh();
        app.apply(Action::AddMidiDevice { name: "Keyboard".to_string(), kind: MidiDeviceKind::Input });
        assert_eq!(app.midi_devices.len(), 1);
        assert_eq!(app.midi_devices[0].name, "Keyboard");
        assert!(app.midi_devices[0].enabled);
    }

    #[test]
    fn add_multiple_devices_increment_id() {
        let mut app = fresh();
        app.apply(Action::AddMidiDevice { name: "KB".to_string(), kind: MidiDeviceKind::Input });
        app.apply(Action::AddMidiDevice { name: "Synth".to_string(), kind: MidiDeviceKind::Output });
        assert_eq!(app.midi_devices[0].id, 0);
        assert_eq!(app.midi_devices[1].id, 1);
    }

    #[test]
    fn remove_midi_device() {
        let mut app = fresh();
        app.apply(Action::AddMidiDevice { name: "KB".to_string(), kind: MidiDeviceKind::Input });
        let did = app.midi_devices[0].id;
        app.apply(Action::RemoveMidiDevice { device_id: did });
        assert!(app.midi_devices.is_empty());
    }

    #[test]
    fn enable_midi_device() {
        let mut app = fresh();
        app.apply(Action::AddMidiDevice { name: "KB".to_string(), kind: MidiDeviceKind::Input });
        let did = app.midi_devices[0].id;
        app.apply(Action::EnableMidiDevice { device_id: did, enabled: false });
        assert!(!app.midi_devices[0].enabled);
        app.apply(Action::EnableMidiDevice { device_id: did, enabled: true });
        assert!(app.midi_devices[0].enabled);
    }

    #[test]
    fn midi_learn_cycle() {
        let mut app = fresh();
        assert!(!app.midi_learn_active);
        assert!(app.midi_learn_target.is_none());

        app.apply(Action::StartMidiLearn { target: MappingTarget::TrackVolume(0) });
        assert!(app.midi_learn_active);
        assert_eq!(app.midi_learn_target, Some(MappingTarget::TrackVolume(0)));

        app.apply(Action::CompleteMidiLearn { channel: 1, cc: 7 });
        assert!(!app.midi_learn_active);
        assert!(app.midi_learn_target.is_none());

        assert_eq!(app.midi_mappings.len(), 1);
        assert_eq!(app.midi_mappings[0].channel, 1);
        assert_eq!(app.midi_mappings[0].cc, 7);
        assert_eq!(app.midi_mappings[0].target, MappingTarget::TrackVolume(0));
    }

    #[test]
    fn stop_midi_learn() {
        let mut app = fresh();
        app.apply(Action::StartMidiLearn { target: MappingTarget::MasterVolume });
        assert!(app.midi_learn_active);
        app.apply(Action::StopMidiLearn);
        assert!(!app.midi_learn_active);
        assert!(app.midi_learn_target.is_none());
        // No mapping should have been added
        assert!(app.midi_mappings.is_empty());
    }

    #[test]
    fn complete_midi_learn_without_start_is_noop() {
        let mut app = fresh();
        // No active learn target
        app.apply(Action::CompleteMidiLearn { channel: 0, cc: 10 });
        assert!(app.midi_mappings.is_empty());
    }
}
