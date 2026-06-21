use super::{App, Action};

/// Triggers that drive transitions between animation states.
#[derive(Clone, Debug, PartialEq)]
pub enum StateTransitionTrigger {
    OnClick,
    OnHover,
    OnComplete,
    OnCondition(String),
}

/// One named state in an animation state machine.
#[derive(Clone, Debug)]
pub struct AnimationState {
    pub id: usize,
    pub name: String,
    pub start_frame: usize,
    pub end_frame: usize,
    pub looping: bool,
}

/// A directed edge between two states with a trigger and blend duration.
#[derive(Clone, Debug)]
pub struct StateTransition {
    pub from_state: usize,
    pub to_state: usize,
    pub trigger: StateTransitionTrigger,
    pub duration_frames: usize,
}

/// A named state machine grouping states and transitions.
#[derive(Clone, Debug)]
pub struct StateMachine {
    pub id: usize,
    pub name: String,
    pub states: Vec<AnimationState>,
    pub transitions: Vec<StateTransition>,
    pub initial_state: usize,
}

impl App {
    pub fn apply_state_machine(&mut self, action: Action) {
        match action {
            Action::CreateStateMachine(name) => {
                let id = self.state_machine_counter;
                self.state_machine_counter += 1;
                self.state_machines.push(StateMachine {
                    id,
                    name,
                    states: Vec::new(),
                    transitions: Vec::new(),
                    initial_state: 0,
                });
            }
            Action::AddAnimationState { machine_id, name, start_frame, end_frame } => {
                let state_id = self.anim_state_counter;
                self.anim_state_counter += 1;
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.states.push(AnimationState {
                        id: state_id,
                        name,
                        start_frame,
                        end_frame,
                        looping: false,
                    });
                }
            }
            Action::DeleteAnimationState { machine_id, state_id } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.states.retain(|s| s.id != state_id);
                    sm.transitions
                        .retain(|t| t.from_state != state_id && t.to_state != state_id);
                }
            }
            Action::AddStateTransition { machine_id, from, to, trigger, duration } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.transitions.push(StateTransition {
                        from_state: from,
                        to_state: to,
                        trigger,
                        duration_frames: duration,
                    });
                }
            }
            Action::DeleteStateTransition { machine_id, from, to } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.transitions
                        .retain(|t| !(t.from_state == from && t.to_state == to));
                }
            }
            Action::SetInitialState { machine_id, state_id } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    sm.initial_state = state_id;
                }
            }
            Action::SetStateLoop { machine_id, state_id, looping } => {
                if let Some(sm) = self.state_machines.iter_mut().find(|m| m.id == machine_id) {
                    if let Some(s) = sm.states.iter_mut().find(|s| s.id == state_id) {
                        s.looping = looping;
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
    use super::StateTransitionTrigger;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_create_state_machine() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        assert_eq!(a.state_machines.len(), 1);
        assert_eq!(a.state_machines[0].name, "Main");
    }

    #[test]
    fn test_add_animation_state() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        assert_eq!(a.state_machines[0].states.len(), 1);
        assert_eq!(a.state_machines[0].states[0].name, "Idle");
    }

    #[test]
    fn test_delete_animation_state() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        let sid = a.state_machines[0].states[0].id;
        a.apply(Action::DeleteAnimationState { machine_id: mid, state_id: sid });
        assert!(a.state_machines[0].states.is_empty());
    }

    #[test]
    fn test_add_state_transition() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
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
        let sid0 = a.state_machines[0].states[0].id;
        let sid1 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: sid0,
            to: sid1,
            trigger: StateTransitionTrigger::OnClick,
            duration: 5,
        });
        assert_eq!(a.state_machines[0].transitions.len(), 1);
        assert_eq!(a.state_machines[0].transitions[0].duration_frames, 5);
    }

    #[test]
    fn test_delete_state_transition() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
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
        let sid0 = a.state_machines[0].states[0].id;
        let sid1 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: sid0,
            to: sid1,
            trigger: StateTransitionTrigger::OnHover,
            duration: 3,
        });
        a.apply(Action::DeleteStateTransition { machine_id: mid, from: sid0, to: sid1 });
        assert!(a.state_machines[0].transitions.is_empty());
    }

    #[test]
    fn test_set_initial_state() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Walk".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        let sid = a.state_machines[0].states[0].id;
        a.apply(Action::SetInitialState { machine_id: mid, state_id: sid });
        assert_eq!(a.state_machines[0].initial_state, sid);
    }

    #[test]
    fn test_set_state_loop() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Main".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "Idle".to_string(),
            start_frame: 0,
            end_frame: 60,
        });
        let sid = a.state_machines[0].states[0].id;
        a.apply(Action::SetStateLoop { machine_id: mid, state_id: sid, looping: true });
        assert!(a.state_machines[0].states[0].looping);
    }

    #[test]
    fn test_state_machine_on_condition_trigger() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("Logic".to_string()));
        let mid = a.state_machines[0].id;
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S1".to_string(),
            start_frame: 0,
            end_frame: 30,
        });
        a.apply(Action::AddAnimationState {
            machine_id: mid,
            name: "S2".to_string(),
            start_frame: 31,
            end_frame: 60,
        });
        let s1 = a.state_machines[0].states[0].id;
        let s2 = a.state_machines[0].states[1].id;
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s1,
            to: s2,
            trigger: StateTransitionTrigger::OnCondition("speed > 0".to_string()),
            duration: 2,
        });
        let t = &a.state_machines[0].transitions[0];
        assert_eq!(t.trigger, StateTransitionTrigger::OnCondition("speed > 0".to_string()));
    }

    #[test]
    fn test_delete_state_removes_transitions() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("M".to_string()));
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
        a.apply(Action::AddStateTransition {
            machine_id: mid,
            from: s0,
            to: s1,
            trigger: StateTransitionTrigger::OnComplete,
            duration: 0,
        });
        a.apply(Action::DeleteAnimationState { machine_id: mid, state_id: s0 });
        assert!(a.state_machines[0].transitions.is_empty());
    }

    #[test]
    fn test_state_machine_multiple() {
        let mut a = app();
        a.apply(Action::CreateStateMachine("SM1".to_string()));
        a.apply(Action::CreateStateMachine("SM2".to_string()));
        assert_eq!(a.state_machines.len(), 2);
        assert_ne!(a.state_machines[0].id, a.state_machines[1].id);
    }
}
