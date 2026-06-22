//! Automation domain — lane types, automation points, automation mode + apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// Which parameter an automation lane controls.
#[derive(Clone, Debug, PartialEq)]
pub enum AutomationParameter {
    Volume,
    Pan,
    Mute,
    Send { send_id: usize, param: SendParam },
    EffectParam { effect_id: usize, param_index: usize },
    CustomParam(String),
}

/// Sub-parameter for a send automation lane.
#[derive(Clone, Debug, PartialEq)]
pub enum SendParam {
    Level,
    Enabled,
}

/// How the automation engine interacts with parameter values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationMode {
    Read,
    Write,
    Touch,
    Latch,
    Off,
}

/// A single automation control point on a lane.
#[derive(Clone, Debug)]
pub struct AutomationPoint {
    pub id: usize,
    /// Position in beats.
    pub beat: f32,
    /// Normalized value 0.0..=1.0.
    pub value: f32,
    /// Curve shape: -1.0 = convex, 0.0 = linear, 1.0 = concave.
    pub curve: f32,
}

/// A complete automation lane for one parameter on one track.
#[derive(Clone, Debug)]
pub struct AutomationLane {
    pub id: usize,
    pub track_id: usize,
    pub parameter: AutomationParameter,
    pub points: Vec<AutomationPoint>,
    pub enabled: bool,
    pub visible: bool,
    pub mode: AutomationMode,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};

impl App {
    pub(super) fn apply_automation(&mut self, action: Action) {
        match action {
            Action::CreateAutomationLane { track_id, parameter } => {
                let id = self.next_lane_id;
                self.next_lane_id += 1;
                self.automation_lanes.push(AutomationLane {
                    id,
                    track_id,
                    parameter,
                    points: Vec::new(),
                    enabled: true,
                    visible: true,
                    mode: AutomationMode::Read,
                });
            }
            Action::DeleteAutomationLane { lane_id } => {
                self.automation_lanes.retain(|l| l.id != lane_id);
            }
            Action::AddAutomationPoint { lane_id, beat, value } => {
                let point_id = self.next_auto_point_id;
                self.next_auto_point_id += 1;
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    let point = AutomationPoint {
                        id: point_id,
                        beat,
                        value: value.clamp(0.0, 1.0),
                        curve: 0.0,
                    };
                    lane.points.push(point);
                    lane.points.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
                }
            }
            Action::RemoveAutomationPoint { lane_id, point_id } => {
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    lane.points.retain(|p| p.id != point_id);
                }
            }
            Action::MoveAutomationPoint { lane_id, point_id, beat, value } => {
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    if let Some(p) = lane.points.iter_mut().find(|p| p.id == point_id) {
                        p.beat = beat;
                        p.value = value.clamp(0.0, 1.0);
                    }
                    lane.points.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
                }
            }
            Action::SetAutomationCurve { lane_id, point_id, curve } => {
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    if let Some(p) = lane.points.iter_mut().find(|p| p.id == point_id) {
                        p.curve = curve.clamp(-1.0, 1.0);
                    }
                }
            }
            Action::SetAutomationLaneEnabled { lane_id, enabled } => {
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    lane.enabled = enabled;
                }
            }
            Action::SetAutomationLaneVisible { lane_id, visible } => {
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    lane.visible = visible;
                }
            }
            Action::SetAutomationMode(mode) => {
                self.automation_record_mode = mode;
            }
            Action::ClearAutomationLane { lane_id } => {
                if let Some(lane) = self.automation_lanes.iter_mut().find(|l| l.id == lane_id) {
                    lane.points.clear();
                }
            }
            _ => {}
        }
    }

    /// Sample the automation value at the given beat position using linear
    /// interpolation with curve bias. Returns None if the lane is not found or has no points.
    pub fn sample_automation_at(&self, lane_id: usize, beat: f32) -> Option<f32> {
        let lane = self.automation_lanes.iter().find(|l| l.id == lane_id)?;
        if lane.points.is_empty() {
            return None;
        }
        // Before all points: return first value.
        if beat <= lane.points[0].beat {
            return Some(lane.points[0].value);
        }
        // After all points: return last value.
        let last = lane.points.last().unwrap();
        if beat >= last.beat {
            return Some(last.value);
        }
        // Find surrounding pair.
        let mut lo = &lane.points[0];
        let mut hi = &lane.points[1];
        for i in 1..lane.points.len() {
            if lane.points[i].beat >= beat {
                lo = &lane.points[i - 1];
                hi = &lane.points[i];
                break;
            }
        }
        let span = hi.beat - lo.beat;
        if span <= 0.0 {
            return Some(lo.value);
        }
        let t = (beat - lo.beat) / span;
        // Apply curve bias using lo's curve value.
        // curve > 0: ease-in; curve < 0: ease-out.
        let t_curved = if lo.curve.abs() < 1e-6 {
            t
        } else if lo.curve > 0.0 {
            t.powf(1.0 + lo.curve)
        } else {
            1.0 - (1.0 - t).powf(1.0 - lo.curve)
        };
        Some(lo.value + (hi.value - lo.value) * t_curved)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::{AutomationMode, AutomationParameter};

    fn fresh() -> App {
        App::new()
    }

    fn make_lane(app: &mut App, track_id: usize) -> usize {
        let before = app.automation_lanes.len();
        app.apply(Action::CreateAutomationLane {
            track_id,
            parameter: AutomationParameter::Volume,
        });
        app.automation_lanes[before].id
    }

    #[test]
    fn create_lane() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        assert_eq!(app.automation_lanes.len(), 1);
        assert_eq!(app.automation_lanes[0].id, lid);
        assert_eq!(app.automation_lanes[0].track_id, 0);
        assert!(app.automation_lanes[0].enabled);
        assert!(app.automation_lanes[0].visible);
    }

    #[test]
    fn create_multiple_lanes_increments_id() {
        let mut app = fresh();
        let l1 = make_lane(&mut app, 0);
        let l2 = make_lane(&mut app, 1);
        assert_ne!(l1, l2);
        assert_eq!(app.automation_lanes.len(), 2);
    }

    #[test]
    fn delete_lane() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::DeleteAutomationLane { lane_id: lid });
        assert!(app.automation_lanes.is_empty());
    }

    #[test]
    fn delete_nonexistent_lane_is_noop() {
        let mut app = fresh();
        make_lane(&mut app, 0);
        app.apply(Action::DeleteAutomationLane { lane_id: 999 });
        assert_eq!(app.automation_lanes.len(), 1);
    }

    #[test]
    fn add_points_sorted_by_beat() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 4.0, value: 0.8 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 1.0, value: 0.2 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 2.0, value: 0.5 });
        let pts = &app.automation_lanes[0].points;
        assert_eq!(pts.len(), 3);
        assert!(pts[0].beat <= pts[1].beat);
        assert!(pts[1].beat <= pts[2].beat);
    }

    #[test]
    fn add_point_value_clamped() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 2.5 });
        assert!(app.automation_lanes[0].points[0].value <= 1.0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 1.0, value: -0.5 });
        assert!(app.automation_lanes[0].points.iter().find(|p| p.beat == 1.0).unwrap().value >= 0.0);
    }

    #[test]
    fn remove_point() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.5 });
        let pid = app.automation_lanes[0].points[0].id;
        app.apply(Action::RemoveAutomationPoint { lane_id: lid, point_id: pid });
        assert!(app.automation_lanes[0].points.is_empty());
    }

    #[test]
    fn move_point_resorts() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.0 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 2.0, value: 1.0 });
        let pid = app.automation_lanes[0].points[0].id;
        app.apply(Action::MoveAutomationPoint { lane_id: lid, point_id: pid, beat: 5.0, value: 0.9 });
        let pts = &app.automation_lanes[0].points;
        assert!(pts[0].beat <= pts[1].beat);
        let moved = pts.iter().find(|p| p.id == pid).unwrap();
        assert!((moved.beat - 5.0).abs() < 0.001);
        assert!((moved.value - 0.9).abs() < 0.001);
    }

    #[test]
    fn move_point_value_clamped() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.5 });
        let pid = app.automation_lanes[0].points[0].id;
        app.apply(Action::MoveAutomationPoint { lane_id: lid, point_id: pid, beat: 1.0, value: 3.0 });
        let pts = &app.automation_lanes[0].points;
        assert!(pts[0].value <= 1.0);
    }

    #[test]
    fn set_curve_clamped() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.5 });
        let pid = app.automation_lanes[0].points[0].id;
        app.apply(Action::SetAutomationCurve { lane_id: lid, point_id: pid, curve: 5.0 });
        assert!(app.automation_lanes[0].points[0].curve <= 1.0);
        app.apply(Action::SetAutomationCurve { lane_id: lid, point_id: pid, curve: -5.0 });
        assert!(app.automation_lanes[0].points[0].curve >= -1.0);
    }

    #[test]
    fn enable_disable_lane() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        assert!(app.automation_lanes[0].enabled);
        app.apply(Action::SetAutomationLaneEnabled { lane_id: lid, enabled: false });
        assert!(!app.automation_lanes[0].enabled);
        app.apply(Action::SetAutomationLaneEnabled { lane_id: lid, enabled: true });
        assert!(app.automation_lanes[0].enabled);
    }

    #[test]
    fn visible_toggle() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::SetAutomationLaneVisible { lane_id: lid, visible: false });
        assert!(!app.automation_lanes[0].visible);
        app.apply(Action::SetAutomationLaneVisible { lane_id: lid, visible: true });
        assert!(app.automation_lanes[0].visible);
    }

    #[test]
    fn set_automation_mode() {
        let mut app = fresh();
        assert_eq!(app.automation_record_mode, AutomationMode::Read);
        app.apply(Action::SetAutomationMode(AutomationMode::Write));
        assert_eq!(app.automation_record_mode, AutomationMode::Write);
        app.apply(Action::SetAutomationMode(AutomationMode::Off));
        assert_eq!(app.automation_record_mode, AutomationMode::Off);
    }

    #[test]
    fn clear_automation_lane() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.0 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 1.0, value: 0.5 });
        assert!(!app.automation_lanes[0].points.is_empty());
        app.apply(Action::ClearAutomationLane { lane_id: lid });
        assert!(app.automation_lanes[0].points.is_empty());
    }

    #[test]
    fn sample_before_all_points_returns_first() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 2.0, value: 0.8 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 4.0, value: 0.4 });
        let v = app.sample_automation_at(lid, 0.0).unwrap();
        assert!((v - 0.8).abs() < 0.001);
    }

    #[test]
    fn sample_after_all_points_returns_last() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.2 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 2.0, value: 0.9 });
        let v = app.sample_automation_at(lid, 10.0).unwrap();
        assert!((v - 0.9).abs() < 0.001);
    }

    #[test]
    fn sample_at_exact_first_point() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.3 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 4.0, value: 0.7 });
        let v = app.sample_automation_at(lid, 0.0).unwrap();
        assert!((v - 0.3).abs() < 0.001);
    }

    #[test]
    fn sample_midpoint_linear() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 0.0, value: 0.0 });
        app.apply(Action::AddAutomationPoint { lane_id: lid, beat: 4.0, value: 1.0 });
        // At the midpoint with linear curve (0.0), value should be ~0.5
        let v = app.sample_automation_at(lid, 2.0).unwrap();
        assert!((v - 0.5).abs() < 0.01);
    }

    #[test]
    fn sample_nonexistent_lane_returns_none() {
        let app = fresh();
        assert!(app.sample_automation_at(999, 0.0).is_none());
    }

    #[test]
    fn sample_empty_lane_returns_none() {
        let mut app = fresh();
        let lid = make_lane(&mut app, 0);
        assert!(app.sample_automation_at(lid, 0.0).is_none());
    }
}
