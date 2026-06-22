//! Hardware synchronization domain for Tone.
//!
//! Manages MIDI clock (internal/external source, send/receive, PPQ) and
//! Ableton Link (tempo sync with network peers, quantum, start/stop sync).

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum MidiClockSource {
    Internal,
    External,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MidiClockStatus {
    Stopped,
    Running,
    Paused,
}

#[derive(Clone, Debug)]
pub struct MidiClockConfig {
    pub source: MidiClockSource,
    pub status: MidiClockStatus,
    pub bpm: f32,
    pub send_clock: bool,
    pub receive_clock: bool,
    pub ppq: u8,
}

impl Default for MidiClockConfig {
    fn default() -> Self {
        Self {
            source: MidiClockSource::Internal,
            status: MidiClockStatus::Stopped,
            bpm: 120.0,
            send_clock: false,
            receive_clock: false,
            ppq: 24,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AbletonLinkConfig {
    pub enabled: bool,
    pub bpm: f32,
    pub peers: u8,
    pub quantum: f32,
    pub start_stop_sync: bool,
}

impl Default for AbletonLinkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bpm: 120.0,
            peers: 0,
            quantum: 4.0,
            start_stop_sync: false,
        }
    }
}

// ─── App impl ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_hardware_sync(&mut self, action: Action) {
        match action {
            Action::SetMidiClockSource { source } => {
                self.midi_clock.source = source;
            }

            Action::SetMidiClockBpm { bpm } => {
                self.midi_clock.bpm = bpm.clamp(20.0, 300.0);
            }

            Action::ToggleSendClock => {
                self.midi_clock.send_clock = !self.midi_clock.send_clock;
            }

            Action::ToggleReceiveClock => {
                self.midi_clock.receive_clock = !self.midi_clock.receive_clock;
            }

            Action::StartMidiClock => {
                self.midi_clock.status = MidiClockStatus::Running;
            }

            Action::StopMidiClock => {
                self.midi_clock.status = MidiClockStatus::Stopped;
            }

            Action::SetHardwareLinkEnabled { enabled } => {
                self.ableton_link.enabled = enabled;
            }

            Action::SetLinkBpm { bpm } => {
                self.ableton_link.bpm = bpm.clamp(20.0, 300.0);
            }

            Action::UpdateLinkPeers { count } => {
                self.ableton_link.peers = count;
            }

            Action::SetLinkQuantum { quantum } => {
                self.ableton_link.quantum = quantum.max(0.5);
            }

            Action::ToggleLinkStartStop => {
                self.ableton_link.start_stop_sync = !self.ableton_link.start_stop_sync;
            }

            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn midi_clock_defaults() {
        let app = fresh();
        assert_eq!(app.midi_clock.source, MidiClockSource::Internal);
        assert_eq!(app.midi_clock.status, MidiClockStatus::Stopped);
        assert_eq!(app.midi_clock.bpm, 120.0);
        assert!(!app.midi_clock.send_clock);
        assert!(!app.midi_clock.receive_clock);
        assert_eq!(app.midi_clock.ppq, 24);
    }

    #[test]
    fn ableton_link_defaults() {
        let app = fresh();
        assert!(!app.ableton_link.enabled);
        assert_eq!(app.ableton_link.bpm, 120.0);
        assert_eq!(app.ableton_link.peers, 0);
        assert_eq!(app.ableton_link.quantum, 4.0);
        assert!(!app.ableton_link.start_stop_sync);
    }

    #[test]
    fn set_midi_clock_source_external() {
        let mut app = fresh();
        app.apply(Action::SetMidiClockSource { source: MidiClockSource::External });
        assert_eq!(app.midi_clock.source, MidiClockSource::External);
    }

    #[test]
    fn set_midi_clock_bpm_clamped() {
        let mut app = fresh();
        app.apply(Action::SetMidiClockBpm { bpm: 500.0 });
        assert_eq!(app.midi_clock.bpm, 300.0);
        app.apply(Action::SetMidiClockBpm { bpm: 5.0 });
        assert_eq!(app.midi_clock.bpm, 20.0);
    }

    #[test]
    fn toggle_send_clock() {
        let mut app = fresh();
        app.apply(Action::ToggleSendClock);
        assert!(app.midi_clock.send_clock);
        app.apply(Action::ToggleSendClock);
        assert!(!app.midi_clock.send_clock);
    }

    #[test]
    fn toggle_receive_clock() {
        let mut app = fresh();
        app.apply(Action::ToggleReceiveClock);
        assert!(app.midi_clock.receive_clock);
        app.apply(Action::ToggleReceiveClock);
        assert!(!app.midi_clock.receive_clock);
    }

    #[test]
    fn start_stop_midi_clock() {
        let mut app = fresh();
        app.apply(Action::StartMidiClock);
        assert_eq!(app.midi_clock.status, MidiClockStatus::Running);
        app.apply(Action::StopMidiClock);
        assert_eq!(app.midi_clock.status, MidiClockStatus::Stopped);
    }

    #[test]
    fn set_hardware_link_enabled() {
        let mut app = fresh();
        app.apply(Action::SetHardwareLinkEnabled { enabled: true });
        assert!(app.ableton_link.enabled);
        app.apply(Action::SetHardwareLinkEnabled { enabled: false });
        assert!(!app.ableton_link.enabled);
    }

    #[test]
    fn set_link_bpm_clamped() {
        let mut app = fresh();
        app.apply(Action::SetLinkBpm { bpm: 1000.0 });
        assert_eq!(app.ableton_link.bpm, 300.0);
        app.apply(Action::SetLinkBpm { bpm: 10.0 });
        assert_eq!(app.ableton_link.bpm, 20.0);
    }

    #[test]
    fn update_link_peers() {
        let mut app = fresh();
        app.apply(Action::UpdateLinkPeers { count: 3 });
        assert_eq!(app.ableton_link.peers, 3);
    }

    #[test]
    fn set_link_quantum_min_clamped() {
        let mut app = fresh();
        app.apply(Action::SetLinkQuantum { quantum: 0.1 });
        assert_eq!(app.ableton_link.quantum, 0.5);
        app.apply(Action::SetLinkQuantum { quantum: 8.0 });
        assert_eq!(app.ableton_link.quantum, 8.0);
    }

    #[test]
    fn toggle_link_start_stop() {
        let mut app = fresh();
        app.apply(Action::ToggleLinkStartStop);
        assert!(app.ableton_link.start_stop_sync);
        app.apply(Action::ToggleLinkStartStop);
        assert!(!app.ableton_link.start_stop_sync);
    }
}
