use super::{App, Action};
use super::state_machine::StateTransitionTrigger;

/// Runtime state for a running state machine instance.
#[derive(Clone, Debug)]
pub struct SmRuntime {
    pub machine_id: usize,
    pub current_state_id: usize,
    pub elapsed_frames: usize,
    pub pending_trigger: Option<StateTransitionTrigger>,
}

impl App {
    pub(super) fn apply_sm_eval(&mut self, action: Action) {
        match action {
            Action::InitSmRuntime { machine_id } => {
                if let Some(sm) = self.state_machines.iter().find(|m| m.id == machine_id) {
                    let initial = sm.initial_state;
                    self.sm_runtimes.retain(|r| r.machine_id != machine_id);
                    self.sm_runtimes.push(SmRuntime {
                        machine_id,
                        current_state_id: initial,
                        elapsed_frames: 0,
                        pending_trigger: None,
                    });
                }
            }
            Action::TickSmRuntime { machine_id } => {
                if let Some(rt) = self.sm_runtimes.iter_mut().find(|r| r.machine_id == machine_id) {
                    rt.elapsed_frames += 1;
                    rt.pending_trigger = None;
                }
            }
            Action::TriggerSmInput { machine_id, trigger } => {
                let current_state = self
                    .sm_runtimes
                    .iter()
                    .find(|r| r.machine_id == machine_id)
                    .map(|r| r.current_state_id);
                if let Some(cstate) = current_state {
                    if let Some(sm) = self.state_machines.iter().find(|m| m.id == machine_id) {
                        let next = sm
                            .transitions
                            .iter()
                            .find(|t| t.from_state == cstate && t.trigger == trigger)
                            .map(|t| t.to_state);
                        if let Some(next_id) = next {
                            if let Some(rt) = self.sm_runtimes.iter_mut().find(|r| r.machine_id == machine_id) {
                                rt.current_state_id = next_id;
                                rt.elapsed_frames = 0;
                                rt.pending_trigger = Some(trigger);
                            }
                        }
                    }
                }
            }
            Action::StopSmRuntime { machine_id } => {
                self.sm_runtimes.retain(|r| r.machine_id != machine_id);
            }
            Action::ResetSmRuntime { machine_id } => {
                if let Some(sm) = self.state_machines.iter().find(|m| m.id == machine_id) {
                    let initial = sm.initial_state;
                    if let Some(rt) = self.sm_runtimes.iter_mut().find(|r| r.machine_id == machine_id) {
                        rt.current_state_id = initial;
                        rt.elapsed_frames = 0;
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::state_machine::StateTransitionTrigger;

    fn app() -> App {
        App::new()
    }

    /// Helper: create a machine with two states and a click transition.
    fn setup_machine(a: &mut App) -> (usize, usize, usize) {
        a.apply(Action::CreateStateMachine("Test".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Walk".to_string(),
            start_frame: 61,
            end_frame: 120,
        });
        let s0 = a.state_machines[0].states[0].id;
        let s1 = a.state_machines[0].states[1].id;
        a.apply(Action::SetInitialState { machine_id: mid, state_id: s0 });
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s0,
            to: s1,
            trigger: StateTransitionTrigger::OnClick,
            duration: 5,
        });
        (mid, s0, s1)
    }

    #[test]
    fn test_init_runtime() {
        let mut a = app();
        let (mid, s0, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        assert_eq!(a.sm_runtimes.len(), 1);
        assert_eq!(a.sm_runtimes[0].current_state_id, s0);
        assert_eq!(a.sm_runtimes[0].elapsed_frames, 0);
        assert!(a.sm_runtimes[0].pending_trigger.is_none());
    }

    #[test]
    fn test_init_replaces_existing_runtime() {
        let mut a = app();
        let (mid, s0, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        a.apply(Action::InitSmRuntime { machine_id: mid });
        assert_eq!(a.sm_runtimes.len(), 1);
        assert_eq!(a.sm_runtimes[0].elapsed_frames, 0);
        assert_eq!(a.sm_runtimes[0].current_state_id, s0);
    }

    #[test]
    fn test_tick_increments_elapsed() {
        let mut a = app();
        let (mid, _, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        assert_eq!(a.sm_runtimes[0].elapsed_frames, 3);
    }

    #[test]
    fn test_tick_clears_pending_trigger() {
        let mut a = app();
        let (mid, _, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput { machine_id: mid, trigger: StateTransitionTrigger::OnClick });
        // pending_trigger is set after transition
        a.apply(Action::TickSmRuntime { machine_id: mid });
        assert!(a.sm_runtimes[0].pending_trigger.is_none());
    }

    #[test]
    fn test_trigger_transitions_state() {
        let mut a = app();
        let (mid, _s0, s1) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput { machine_id: mid, trigger: StateTransitionTrigger::OnClick });
        assert_eq!(a.sm_runtimes[0].current_state_id, s1);
    }

    #[test]
    fn test_trigger_resets_elapsed_frames() {
        let mut a = app();
        let (mid, _, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput { machine_id: mid, trigger: StateTransitionTrigger::OnClick });
        assert_eq!(a.sm_runtimes[0].elapsed_frames, 0);
    }

    #[test]
    fn test_trigger_sets_pending_trigger() {
        let mut a = app();
        let (mid, _, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput { machine_id: mid, trigger: StateTransitionTrigger::OnClick });
        assert_eq!(a.sm_runtimes[0].pending_trigger, Some(StateTransitionTrigger::OnClick));
    }

    #[test]
    fn test_no_match_trigger_is_noop() {
        let mut a = app();
        let (mid, s0, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        // OnHover has no transition defined — should be a no-op
        a.apply(Action::TriggerSmInput { machine_id: mid, trigger: StateTransitionTrigger::OnHover });
        assert_eq!(a.sm_runtimes[0].current_state_id, s0);
        assert!(a.sm_runtimes[0].pending_trigger.is_none());
    }

    #[test]
    fn test_stop_removes_runtime() {
        let mut a = app();
        let (mid, _, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        assert_eq!(a.sm_runtimes.len(), 1);
        a.apply(Action::StopSmRuntime { machine_id: mid });
        assert!(a.sm_runtimes.is_empty());
    }

    #[test]
    fn test_reset_returns_to_initial() {
        let mut a = app();
        let (mid, s0, _) = setup_machine(&mut a);
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput { machine_id: mid, trigger: StateTransitionTrigger::OnClick });
        a.apply(Action::TickSmRuntime { machine_id: mid });
        a.apply(Action::ResetSmRuntime { machine_id: mid });
        assert_eq!(a.sm_runtimes[0].current_state_id, s0);
        assert_eq!(a.sm_runtimes[0].elapsed_frames, 0);
    }

    #[test]
    fn test_condition_trigger_transition() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Cond".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S0".to_string(),
            start_frame: 0,
            end_frame: 30,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S1".to_string(),
            start_frame: 31,
            end_frame: 60,
        });
        let s0 = a.state_machines[0].states[0].id;
        let s1 = a.state_machines[0].states[1].id;
        a.apply(Action::SetInitialState { machine_id: mid, state_id: s0 });
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s0,
            to: s1,
            trigger: StateTransitionTrigger::OnCondition("speed > 0".to_string()),
            duration: 2,
        });
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput {
            machine_id: mid,
            trigger: StateTransitionTrigger::OnCondition("speed > 0".to_string()),
        });
        assert_eq!(a.sm_runtimes[0].current_state_id, s1);
    }

    #[test]
    fn test_wrong_condition_string_is_noop() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Cond".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S0".to_string(),
            start_frame: 0,
            end_frame: 30,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S1".to_string(),
            start_frame: 31,
            end_frame: 60,
        });
        let s0 = a.state_machines[0].states[0].id;
        let s1 = a.state_machines[0].states[1].id;
        a.apply(Action::SetInitialState { machine_id: mid, state_id: s0 });
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s0,
            to: s1,
            trigger: StateTransitionTrigger::OnCondition("speed > 0".to_string()),
            duration: 2,
        });
        a.apply(Action::InitSmRuntime { machine_id: mid });
        a.apply(Action::TriggerSmInput {
            machine_id: mid,
            trigger: StateTransitionTrigger::OnCondition("different condition".to_string()),
        });
        assert_eq!(a.sm_runtimes[0].current_state_id, s0);
    }

    #[test]
    fn test_multiple_machines_independent() {
        let mut a = app();
        // Machine A
        a.apply(Action::CreateStateMachine("A".to_string()));
        let mid_a = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid_a,
            name: "A0".to_string(),
            start_frame: 0,
            end_frame: 10,
        });
        let sa0 = a.state_machines[0].states[0].id;
        a.apply(Action::SetInitialState { machine_id: mid_a, state_id: sa0 });

        // Machine B
        a.apply(Action::CreateStateMachine("B".to_string()));
        let mid_b = a.state_machines[1].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid_b,
            name: "B0".to_string(),
            start_frame: 0,
            end_frame: 10,
        });
        let sb0 = a.state_machines[1].states[0].id;
        a.apply(Action::SetInitialState { machine_id: mid_b, state_id: sb0 });

        a.apply(Action::InitSmRuntime { machine_id: mid_a });
        a.apply(Action::InitSmRuntime { machine_id: mid_b });
        assert_eq!(a.sm_runtimes.len(), 2);

        a.apply(Action::TickSmRuntime { machine_id: mid_a });
        a.apply(Action::TickSmRuntime { machine_id: mid_a });

        let rt_a = a.sm_runtimes.iter().find(|r| r.machine_id == mid_a).unwrap();
        let rt_b = a.sm_runtimes.iter().find(|r| r.machine_id == mid_b).unwrap();
        assert_eq!(rt_a.elapsed_frames, 2);
        assert_eq!(rt_b.elapsed_frames, 0);
    }
}
