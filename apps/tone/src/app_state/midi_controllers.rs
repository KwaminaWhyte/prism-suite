//! MIDI controller / pad+knob hardware mapping domain for Tone.
//!
//! Manages hardware controllers (Akai MPC, Novation Launchpad, etc.) with
//! per-pad and per-knob MIDI learn, CC mappings, and activation state.

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerPreset {
    AkaiMpc,
    NovationLaunchpad,
    AbletonPush2,
    NativeKomplete,
    Generic,
}

#[derive(Clone, Debug)]
pub struct PadMapping {
    pub pad_index: u8,
    pub note: u8,
    pub velocity_scale: f32,
    pub channel: u8,
}

#[derive(Clone, Debug)]
pub struct KnobMapping {
    pub knob_index: u8,
    pub cc: u8,
    pub channel: u8,
    pub min_val: f32,
    pub max_val: f32,
}

#[derive(Clone, Debug)]
pub struct HardwareController {
    pub id: usize,
    pub name: String,
    pub preset: ControllerPreset,
    pub pad_mappings: Vec<PadMapping>,
    pub knob_mappings: Vec<KnobMapping>,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MidiLearnCtrlState {
    Idle,
    WaitingPad { pad_index: u8 },
    WaitingKnob { knob_index: u8 },
    Done,
}

// ─── App impl ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_midi_controllers(&mut self, action: Action) {
        match action {
            Action::RegisterController { name, preset } => {
                let id = self.next_controller_id;
                self.next_controller_id += 1;
                self.hardware_controllers.push(HardwareController {
                    id,
                    name,
                    preset,
                    pad_mappings: vec![],
                    knob_mappings: vec![],
                    active: true,
                });
            }

            Action::UnregisterController { controller_id } => {
                self.hardware_controllers.retain(|c| c.id != controller_id);
            }

            Action::SetControllerPreset { controller_id, preset } => {
                if let Some(c) = self.hardware_controllers.iter_mut().find(|c| c.id == controller_id) {
                    c.preset = preset;
                }
            }

            Action::ActivateController { controller_id } => {
                if let Some(c) = self.hardware_controllers.iter_mut().find(|c| c.id == controller_id) {
                    c.active = true;
                }
            }

            Action::DeactivateController { controller_id } => {
                if let Some(c) = self.hardware_controllers.iter_mut().find(|c| c.id == controller_id) {
                    c.active = false;
                }
            }

            Action::StartPadLearn { controller_id: _, pad_index } => {
                self.ctrl_learn_state = MidiLearnCtrlState::WaitingPad { pad_index };
            }

            Action::CompletePadLearn { controller_id, note } => {
                if let MidiLearnCtrlState::WaitingPad { pad_index } = self.ctrl_learn_state {
                    if let Some(c) = self.hardware_controllers.iter_mut().find(|c| c.id == controller_id) {
                        c.pad_mappings.push(PadMapping {
                            pad_index,
                            note,
                            velocity_scale: 1.0,
                            channel: 0,
                        });
                    }
                    self.ctrl_learn_state = MidiLearnCtrlState::Done;
                }
            }

            Action::StartKnobLearn { controller_id: _, knob_index } => {
                self.ctrl_learn_state = MidiLearnCtrlState::WaitingKnob { knob_index };
            }

            Action::CompleteKnobLearn { controller_id, cc } => {
                if let MidiLearnCtrlState::WaitingKnob { knob_index } = self.ctrl_learn_state {
                    if let Some(c) = self.hardware_controllers.iter_mut().find(|c| c.id == controller_id) {
                        c.knob_mappings.push(KnobMapping {
                            knob_index,
                            cc,
                            channel: 0,
                            min_val: 0.0,
                            max_val: 1.0,
                        });
                    }
                    self.ctrl_learn_state = MidiLearnCtrlState::Done;
                }
            }

            Action::CancelCtrlLearn => {
                self.ctrl_learn_state = MidiLearnCtrlState::Idle;
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
    fn register_controller_assigns_id() {
        let mut app = fresh();
        app.apply(Action::RegisterController {
            name: "Akai MPC One".into(),
            preset: ControllerPreset::AkaiMpc,
        });
        assert_eq!(app.hardware_controllers.len(), 1);
        assert_eq!(app.hardware_controllers[0].id, 1);
        assert_eq!(app.next_controller_id, 2);
    }

    #[test]
    fn register_multiple_controllers() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Launchpad".into(), preset: ControllerPreset::NovationLaunchpad });
        app.apply(Action::RegisterController { name: "Push2".into(), preset: ControllerPreset::AbletonPush2 });
        assert_eq!(app.hardware_controllers.len(), 2);
        assert_eq!(app.hardware_controllers[1].id, 2);
    }

    #[test]
    fn unregister_controller() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Generic".into(), preset: ControllerPreset::Generic });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::UnregisterController { controller_id: id });
        assert!(app.hardware_controllers.is_empty());
    }

    #[test]
    fn set_controller_preset() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "My Controller".into(), preset: ControllerPreset::Generic });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::SetControllerPreset { controller_id: id, preset: ControllerPreset::AkaiMpc });
        assert_eq!(app.hardware_controllers[0].preset, ControllerPreset::AkaiMpc);
    }

    #[test]
    fn activate_deactivate_controller() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Push".into(), preset: ControllerPreset::AbletonPush2 });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::DeactivateController { controller_id: id });
        assert!(!app.hardware_controllers[0].active);
        app.apply(Action::ActivateController { controller_id: id });
        assert!(app.hardware_controllers[0].active);
    }

    #[test]
    fn start_pad_learn_sets_state() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "MPC".into(), preset: ControllerPreset::AkaiMpc });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::StartPadLearn { controller_id: id, pad_index: 3 });
        assert_eq!(app.ctrl_learn_state, MidiLearnCtrlState::WaitingPad { pad_index: 3 });
    }

    #[test]
    fn complete_pad_learn_pushes_mapping() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "MPC".into(), preset: ControllerPreset::AkaiMpc });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::StartPadLearn { controller_id: id, pad_index: 2 });
        app.apply(Action::CompletePadLearn { controller_id: id, note: 60 });
        assert_eq!(app.hardware_controllers[0].pad_mappings.len(), 1);
        assert_eq!(app.hardware_controllers[0].pad_mappings[0].note, 60);
        assert_eq!(app.ctrl_learn_state, MidiLearnCtrlState::Done);
    }

    #[test]
    fn start_knob_learn_sets_state() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Komplete".into(), preset: ControllerPreset::NativeKomplete });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::StartKnobLearn { controller_id: id, knob_index: 7 });
        assert_eq!(app.ctrl_learn_state, MidiLearnCtrlState::WaitingKnob { knob_index: 7 });
    }

    #[test]
    fn complete_knob_learn_pushes_mapping() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Komplete".into(), preset: ControllerPreset::NativeKomplete });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::StartKnobLearn { controller_id: id, knob_index: 5 });
        app.apply(Action::CompleteKnobLearn { controller_id: id, cc: 74 });
        assert_eq!(app.hardware_controllers[0].knob_mappings.len(), 1);
        assert_eq!(app.hardware_controllers[0].knob_mappings[0].cc, 74);
        assert_eq!(app.ctrl_learn_state, MidiLearnCtrlState::Done);
    }

    #[test]
    fn cancel_ctrl_learn_resets_state() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Test".into(), preset: ControllerPreset::Generic });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::StartPadLearn { controller_id: id, pad_index: 0 });
        app.apply(Action::CancelCtrlLearn);
        assert_eq!(app.ctrl_learn_state, MidiLearnCtrlState::Idle);
    }

    #[test]
    fn complete_pad_learn_no_op_when_not_waiting() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Test".into(), preset: ControllerPreset::Generic });
        let id = app.hardware_controllers[0].id;
        // No StartPadLearn — state is Idle
        app.apply(Action::CompletePadLearn { controller_id: id, note: 60 });
        assert!(app.hardware_controllers[0].pad_mappings.is_empty());
    }

    #[test]
    fn complete_knob_learn_no_op_when_not_waiting() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Test".into(), preset: ControllerPreset::Generic });
        let id = app.hardware_controllers[0].id;
        app.apply(Action::CompleteKnobLearn { controller_id: id, cc: 10 });
        assert!(app.hardware_controllers[0].knob_mappings.is_empty());
    }

    #[test]
    fn controller_starts_active() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Active".into(), preset: ControllerPreset::Generic });
        assert!(app.hardware_controllers[0].active);
    }

    #[test]
    fn unregister_unknown_controller_is_noop() {
        let mut app = fresh();
        app.apply(Action::RegisterController { name: "Keep".into(), preset: ControllerPreset::Generic });
        app.apply(Action::UnregisterController { controller_id: 9999 });
        assert_eq!(app.hardware_controllers.len(), 1);
    }
}
