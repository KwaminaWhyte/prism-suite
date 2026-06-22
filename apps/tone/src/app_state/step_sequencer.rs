//! Step sequencer domain — step patterns, cells, chain patterns.

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

pub struct StepPattern {
    pub id: usize,
    pub name: String,
    pub track_id: usize,
    pub steps: u32,
    pub step_length: f32,
    pub cells: Vec<StepCell>,
    pub swing: f32,
    pub active: bool,
}

pub struct StepCell {
    pub active: bool,
    pub velocity: u8,
    pub pitch_offset: i8,
    pub probability: f32,
    pub retrigger: u32,
    pub accent: bool,
    pub skip: bool,
}

impl StepCell {
    pub fn default() -> Self {
        Self {
            active: false,
            velocity: 100,
            pitch_offset: 0,
            probability: 1.0,
            retrigger: 0,
            accent: false,
            skip: false,
        }
    }
}

pub struct ChainPattern {
    pub id: usize,
    pub pattern_ids: Vec<usize>,
    pub repeat_count: u32,
    pub loop_chain: bool,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_step_sequencer(&mut self, action: Action) {
        match action {
            Action::AddPattern { name, track_id, steps } => {
                let id = self.next_pattern_id;
                self.next_pattern_id += 1;
                let cells = (0..steps).map(|_| StepCell::default()).collect();
                self.step_patterns.push(StepPattern {
                    id,
                    name,
                    track_id,
                    steps,
                    step_length: 0.25,
                    cells,
                    swing: 0.0,
                    active: true,
                });
            }
            Action::DeletePattern { pattern_id } => {
                self.step_patterns.retain(|p| p.id != pattern_id);
                if self.active_pattern_id == Some(pattern_id) {
                    self.active_pattern_id = None;
                }
            }
            Action::RenamePattern { pattern_id, name } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    p.name = name;
                }
            }
            Action::SetPatternSteps { pattern_id, steps } => {
                if ![8u32, 16, 32].contains(&steps) {
                    return;
                }
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    p.steps = steps;
                    let current_len = p.cells.len() as u32;
                    if steps > current_len {
                        for _ in 0..(steps - current_len) {
                            p.cells.push(StepCell::default());
                        }
                    } else {
                        p.cells.truncate(steps as usize);
                    }
                }
            }
            Action::SetPatternStepLength { pattern_id, step_length } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    p.step_length = step_length;
                }
            }
            Action::SetPatternSwing { pattern_id, swing } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    p.swing = swing.clamp(0.0, 1.0);
                }
            }
            Action::SetStep { pattern_id, step, active } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    if let Some(cell) = p.cells.get_mut(step) {
                        cell.active = active;
                    }
                }
            }
            Action::SetStepVelocity { pattern_id, step, velocity } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    if let Some(cell) = p.cells.get_mut(step) {
                        cell.velocity = velocity;
                    }
                }
            }
            Action::SetStepProbability { pattern_id, step, prob } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    if let Some(cell) = p.cells.get_mut(step) {
                        cell.probability = prob.clamp(0.0, 1.0);
                    }
                }
            }
            Action::SetStepAccent { pattern_id, step, accent } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    if let Some(cell) = p.cells.get_mut(step) {
                        cell.accent = accent;
                    }
                }
            }
            Action::SetStepSkip { pattern_id, step, skip } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    if let Some(cell) = p.cells.get_mut(step) {
                        cell.skip = skip;
                    }
                }
            }
            Action::SetStepRetrigger { pattern_id, step, retrigger } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    if let Some(cell) = p.cells.get_mut(step) {
                        cell.retrigger = retrigger;
                    }
                }
            }
            Action::ClearPattern { pattern_id } => {
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    for cell in p.cells.iter_mut() {
                        cell.active = false;
                    }
                }
            }
            Action::FillPattern { pattern_id, every_n } => {
                if every_n == 0 { return; }
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    for (i, cell) in p.cells.iter_mut().enumerate() {
                        cell.active = (i as u32) % every_n == 0;
                    }
                }
            }
            Action::RandomizePattern { pattern_id, density } => {
                let density_clamped = density.clamp(0.0, 1.0);
                if let Some(p) = self.step_patterns.iter_mut().find(|p| p.id == pattern_id) {
                    for (i, cell) in p.cells.iter_mut().enumerate() {
                        let pseudo = (i as u32).wrapping_mul(2654435769).wrapping_add(1) % 1000;
                        cell.active = pseudo < (density_clamped * 1000.0) as u32;
                    }
                }
            }
            Action::SetActivePattern { pattern_id } => {
                self.active_pattern_id = pattern_id;
            }
            Action::AddChainPattern { pattern_ids } => {
                let id = self.next_chain_id;
                self.next_chain_id += 1;
                self.chain_patterns.push(ChainPattern {
                    id,
                    pattern_ids,
                    repeat_count: 1,
                    loop_chain: false,
                });
            }
            Action::DeleteChainPattern { chain_id } => {
                self.chain_patterns.retain(|c| c.id != chain_id);
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};

    fn fresh() -> App {
        App::new()
    }

    fn add_pattern(app: &mut App, name: &str, steps: u32) -> usize {
        app.apply(Action::AddPattern { name: name.to_string(), track_id: 1, steps });
        app.step_patterns.last().unwrap().id
    }

    #[test]
    fn step_sequencer_defaults() {
        let app = fresh();
        assert!(app.step_patterns.is_empty());
        assert_eq!(app.next_pattern_id, 0);
        assert!(app.active_pattern_id.is_none());
        assert!(app.chain_patterns.is_empty());
        assert_eq!(app.next_chain_id, 0);
    }

    #[test]
    fn add_pattern_creates_cells() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.cells.len(), 16);
        assert_eq!(p.steps, 16);
        assert!((p.step_length - 0.25).abs() < 0.001);
        assert!((p.swing).abs() < 0.001);
        assert!(p.active);
    }

    #[test]
    fn delete_pattern() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::DeletePattern { pattern_id: pid });
        assert!(app.step_patterns.is_empty());
    }

    #[test]
    fn delete_pattern_clears_active() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetActivePattern { pattern_id: Some(pid) });
        app.apply(Action::DeletePattern { pattern_id: pid });
        assert!(app.active_pattern_id.is_none());
    }

    #[test]
    fn rename_pattern() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Old", 16);
        app.apply(Action::RenamePattern { pattern_id: pid, name: "New".to_string() });
        assert_eq!(app.step_patterns.iter().find(|p| p.id == pid).unwrap().name, "New");
    }

    #[test]
    fn set_pattern_steps_valid() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetPatternSteps { pattern_id: pid, steps: 32 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.steps, 32);
        assert_eq!(p.cells.len(), 32);
    }

    #[test]
    fn set_pattern_steps_truncate() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 32);
        app.apply(Action::SetPatternSteps { pattern_id: pid, steps: 8 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.cells.len(), 8);
    }

    #[test]
    fn set_pattern_steps_invalid_noop() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetPatternSteps { pattern_id: pid, steps: 12 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.steps, 16); // unchanged
    }

    #[test]
    fn set_step_active() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetStep { pattern_id: pid, step: 0, active: true });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert!(p.cells[0].active);
    }

    #[test]
    fn set_step_velocity() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetStepVelocity { pattern_id: pid, step: 0, velocity: 80 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.cells[0].velocity, 80);
    }

    #[test]
    fn set_step_probability() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetStepProbability { pattern_id: pid, step: 0, prob: 0.5 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert!((p.cells[0].probability - 0.5).abs() < 0.001);
    }

    #[test]
    fn set_step_accent_and_skip() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetStepAccent { pattern_id: pid, step: 2, accent: true });
        app.apply(Action::SetStepSkip { pattern_id: pid, step: 3, skip: true });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert!(p.cells[2].accent);
        assert!(p.cells[3].skip);
    }

    #[test]
    fn set_step_retrigger() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetStepRetrigger { pattern_id: pid, step: 1, retrigger: 3 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert_eq!(p.cells[1].retrigger, 3);
    }

    #[test]
    fn clear_pattern() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::SetStep { pattern_id: pid, step: 0, active: true });
        app.apply(Action::SetStep { pattern_id: pid, step: 4, active: true });
        app.apply(Action::ClearPattern { pattern_id: pid });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert!(p.cells.iter().all(|c| !c.active));
    }

    #[test]
    fn fill_pattern_every_4() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::FillPattern { pattern_id: pid, every_n: 4 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert!(p.cells[0].active);
        assert!(!p.cells[1].active);
        assert!(!p.cells[2].active);
        assert!(!p.cells[3].active);
        assert!(p.cells[4].active);
        assert!(p.cells[8].active);
        assert!(p.cells[12].active);
    }

    #[test]
    fn randomize_pattern_density_zero() {
        let mut app = fresh();
        let pid = add_pattern(&mut app, "Beat", 16);
        app.apply(Action::RandomizePattern { pattern_id: pid, density: 0.0 });
        let p = app.step_patterns.iter().find(|p| p.id == pid).unwrap();
        assert!(p.cells.iter().all(|c| !c.active));
    }

    #[test]
    fn add_chain_pattern() {
        let mut app = fresh();
        app.apply(Action::AddChainPattern { pattern_ids: vec![0, 1, 2] });
        assert_eq!(app.chain_patterns.len(), 1);
        assert_eq!(app.chain_patterns[0].pattern_ids, vec![0, 1, 2]);
    }

    #[test]
    fn delete_chain_pattern() {
        let mut app = fresh();
        app.apply(Action::AddChainPattern { pattern_ids: vec![0] });
        let cid = app.chain_patterns[0].id;
        app.apply(Action::DeleteChainPattern { chain_id: cid });
        assert!(app.chain_patterns.is_empty());
    }
}
