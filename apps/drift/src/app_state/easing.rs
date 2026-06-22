use super::{App, Action};

/// Where a step jump lands within the interval.
#[derive(Clone, Debug, PartialEq)]
pub enum StepPosition {
    Start,
    End,
    Both,
    None,
}

/// The mathematical form of an easing curve.
#[derive(Clone, Debug, PartialEq)]
pub enum EasingCurveKind {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bounce { amplitude: f32, period: f32 },
    Elastic { amplitude: f32, period: f32 },
    Spring { stiffness: f32, damping: f32, initial_velocity: f32 },
    Steps { count: u32, position: StepPosition },
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
}

/// A named easing curve (user-defined or built-in).
#[derive(Clone, Debug)]
pub struct EasingCurve {
    pub id: usize,
    pub name: String,
    pub kind: EasingCurveKind,
}

/// IDs below this threshold are built-in and cannot be removed.
const BUILTIN_COUNT: usize = 7;

impl App {
    /// Seed the built-in easing curves.  Called from `App::new()`.
    pub fn seed_easing_curves(&mut self) {
        self.easing_curves = vec![
            EasingCurve { id: 0, name: "Linear".to_string(),     kind: EasingCurveKind::Linear },
            EasingCurve { id: 1, name: "EaseIn".to_string(),     kind: EasingCurveKind::EaseIn },
            EasingCurve { id: 2, name: "EaseOut".to_string(),    kind: EasingCurveKind::EaseOut },
            EasingCurve { id: 3, name: "EaseInOut".to_string(),  kind: EasingCurveKind::EaseInOut },
            EasingCurve {
                id: 4,
                name: "Bounce".to_string(),
                kind: EasingCurveKind::Bounce { amplitude: 1.0, period: 0.3 },
            },
            EasingCurve {
                id: 5,
                name: "Elastic".to_string(),
                kind: EasingCurveKind::Elastic { amplitude: 1.0, period: 0.3 },
            },
            EasingCurve {
                id: 6,
                name: "Spring".to_string(),
                kind: EasingCurveKind::Spring { stiffness: 200.0, damping: 20.0, initial_velocity: 0.0 },
            },
        ];
        self.next_easing_id = BUILTIN_COUNT;
    }

    pub fn apply_easing(&mut self, action: Action) {
        match action {
            Action::AddEasingCurve { name, kind } => {
                let id = self.next_easing_id;
                self.next_easing_id += 1;
                self.easing_curves.push(EasingCurve { id, name, kind });
            }
            Action::RemoveEasingCurve { curve_id } => {
                // Guard: do not allow removal of built-in curves (id < BUILTIN_COUNT).
                if curve_id < BUILTIN_COUNT {
                    return;
                }
                self.easing_curves.retain(|c| c.id != curve_id);
            }
            Action::RenameEasingCurve { curve_id, name } => {
                if let Some(c) = self.easing_curves.iter_mut().find(|c| c.id == curve_id) {
                    c.name = name;
                }
            }
            Action::SetKeyframeEasingCurve { keyframe_id, curve_id } => {
                if self.easing_curves.iter().any(|c| c.id == curve_id) {
                    self.keyframe_easing_map.insert(keyframe_id, curve_id);
                }
            }
            _ => {}
        }
    }

    /// Evaluate an easing curve at parameter `t` ∈ [0.0, 1.0].
    pub fn sample_easing(&self, kind: &EasingCurveKind, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match kind {
            EasingCurveKind::Linear => t,
            EasingCurveKind::EaseIn => t * t,
            EasingCurveKind::EaseOut => t * (2.0 - t),
            EasingCurveKind::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            EasingCurveKind::Bounce { amplitude, period: _ } => {
                // Simple out-bounce approximation.
                let amp = *amplitude;
                let t2 = 1.0 - t;
                let out = if t2 < 1.0 / 2.75 {
                    7.5625 * t2 * t2
                } else if t2 < 2.0 / 2.75 {
                    let t2 = t2 - 1.5 / 2.75;
                    7.5625 * t2 * t2 + 0.75
                } else if t2 < 2.5 / 2.75 {
                    let t2 = t2 - 2.25 / 2.75;
                    7.5625 * t2 * t2 + 0.9375
                } else {
                    let t2 = t2 - 2.625 / 2.75;
                    7.5625 * t2 * t2 + 0.984375
                };
                amp * (1.0 - out)
            }
            EasingCurveKind::Elastic { amplitude, period } => {
                if t == 0.0 {
                    return 0.0;
                }
                if t == 1.0 {
                    return 1.0;
                }
                let p = *period;
                let a = amplitude.max(1.0);
                let s = p / 4.0;
                let t = t - 1.0;
                -(a * 2.0_f32.powf(10.0 * t)
                    * ((t - s) * (2.0 * std::f32::consts::PI / p)).sin())
            }
            EasingCurveKind::Spring {
                stiffness,
                damping,
                initial_velocity,
            } => {
                // Simple Euler spring-damper integration from t=0 to t.
                let steps = 100_u32;
                let dt = t / steps as f32;
                let mut pos = 0.0_f32;
                let mut vel = *initial_velocity;
                let m = 1.0_f32;
                for _ in 0..steps {
                    let force = stiffness * (1.0 - pos) - damping * vel;
                    vel += (force / m) * dt;
                    pos += vel * dt;
                }
                pos.clamp(0.0, 2.0) // spring may overshoot briefly
            }
            EasingCurveKind::Steps { count, position } => {
                let n = (*count).max(1) as f32;
                let floored = (t * n).floor();
                let v = match position {
                    StepPosition::Start => (floored + 1.0) / n,
                    StepPosition::End => floored / n,
                    StepPosition::Both => (floored + 0.5) / n,
                    StepPosition::None => floored / n,
                };
                v.clamp(0.0, 1.0)
            }
            EasingCurveKind::CubicBezier { x1, y1, x2, y2 } => {
                // Newton-Raphson to find the t parameter for the x component, then
                // evaluate y.
                cubic_bezier_eval(*x1, *y1, *x2, *y2, t)
            }
        }
    }
}

/// Evaluate a CSS cubic-bezier(x1,y1,x2,y2) at input `tx` using Newton-Raphson.
fn cubic_bezier_eval(x1: f32, y1: f32, x2: f32, y2: f32, tx: f32) -> f32 {
    // Coefficients for x(t) = 3*C1*t + 3*C2*t^2 + C3*t^3
    let cx = 3.0 * x1;
    let bx = 3.0 * (x2 - x1) - cx;
    let ax = 1.0 - cx - bx;

    let cy = 3.0 * y1;
    let by = 3.0 * (y2 - y1) - cy;
    let ay = 1.0 - cy - by;

    let bezier_x = |t: f32| ((ax * t + bx) * t + cx) * t;
    let bezier_y = |t: f32| ((ay * t + by) * t + cy) * t;
    let deriv_x  = |t: f32| (3.0 * ax * t + 2.0 * bx) * t + cx;

    // Newton-Raphson: find t such that bezier_x(t) == tx.
    let mut t = tx;
    for _ in 0..8 {
        let x_err = bezier_x(t) - tx;
        let d = deriv_x(t);
        if d.abs() < 1e-9 {
            break;
        }
        t -= x_err / d;
    }
    bezier_y(t.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{EasingCurveKind, StepPosition};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_built_in_curves_seeded_on_new() {
        let a = app();
        assert_eq!(a.easing_curves.len(), 7, "7 built-in curves");
        assert!(a.easing_curves.iter().any(|c| c.name == "Linear"));
        assert!(a.easing_curves.iter().any(|c| c.name == "Spring"));
    }

    #[test]
    fn test_linear_sampling() {
        let a = app();
        let kind = EasingCurveKind::Linear;
        assert_eq!(a.sample_easing(&kind, 0.0), 0.0);
        assert_eq!(a.sample_easing(&kind, 1.0), 1.0);
        assert!((a.sample_easing(&kind, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_ease_in_at_boundaries() {
        let a = app();
        let kind = EasingCurveKind::EaseIn;
        assert_eq!(a.sample_easing(&kind, 0.0), 0.0);
        assert_eq!(a.sample_easing(&kind, 1.0), 1.0);
    }

    #[test]
    fn test_ease_in_midpoint_less_than_half() {
        let a = app();
        // EaseIn (t^2) at t=0.5 → 0.25, which is less than 0.5
        let v = a.sample_easing(&EasingCurveKind::EaseIn, 0.5);
        assert!(v < 0.5, "EaseIn is slow at start: got {v}");
    }

    #[test]
    fn test_ease_out_at_boundaries() {
        let a = app();
        let kind = EasingCurveKind::EaseOut;
        assert_eq!(a.sample_easing(&kind, 0.0), 0.0);
        assert_eq!(a.sample_easing(&kind, 1.0), 1.0);
    }

    #[test]
    fn test_ease_out_midpoint_greater_than_half() {
        let a = app();
        // EaseOut at t=0.5 → 0.75, which is > 0.5
        let v = a.sample_easing(&EasingCurveKind::EaseOut, 0.5);
        assert!(v > 0.5, "EaseOut is fast at start: got {v}");
    }

    #[test]
    fn test_ease_in_out_at_boundaries() {
        let a = app();
        let kind = EasingCurveKind::EaseInOut;
        assert_eq!(a.sample_easing(&kind, 0.0), 0.0);
        assert_eq!(a.sample_easing(&kind, 1.0), 1.0);
    }

    #[test]
    fn test_ease_in_out_symmetric_at_midpoint() {
        let a = app();
        let v = a.sample_easing(&EasingCurveKind::EaseInOut, 0.5);
        assert!((v - 0.5).abs() < 1e-5, "EaseInOut is symmetric at t=0.5");
    }

    #[test]
    fn test_steps_end_at_boundaries() {
        let a = app();
        let kind = EasingCurveKind::Steps { count: 4, position: StepPosition::End };
        assert_eq!(a.sample_easing(&kind, 0.0), 0.0);
        assert_eq!(a.sample_easing(&kind, 1.0), 1.0);
    }

    #[test]
    fn test_cubic_bezier_at_boundaries() {
        let a = app();
        let kind = EasingCurveKind::CubicBezier { x1: 0.25, y1: 0.1, x2: 0.25, y2: 1.0 };
        let v0 = a.sample_easing(&kind, 0.0);
        let v1 = a.sample_easing(&kind, 1.0);
        assert!(v0.abs() < 0.01, "bezier at t=0 ≈ 0");
        assert!((v1 - 1.0).abs() < 0.01, "bezier at t=1 ≈ 1");
    }

    #[test]
    fn test_add_easing_curve() {
        let mut a = app();
        a.apply(Action::AddEasingCurve {
            name: "MyEase".to_string(),
            kind: EasingCurveKind::EaseIn,
        });
        assert_eq!(a.easing_curves.len(), 8);
        assert!(a.easing_curves.iter().any(|c| c.name == "MyEase"));
    }

    #[test]
    fn test_remove_user_easing_curve() {
        let mut a = app();
        a.apply(Action::AddEasingCurve {
            name: "Custom".to_string(),
            kind: EasingCurveKind::Linear,
        });
        let cid = a.easing_curves.last().unwrap().id;
        a.apply(Action::RemoveEasingCurve { curve_id: cid });
        assert!(!a.easing_curves.iter().any(|c| c.id == cid));
    }

    #[test]
    fn test_cannot_remove_builtin_curve() {
        let mut a = app();
        // Try to remove built-in "Linear" (id=0)
        a.apply(Action::RemoveEasingCurve { curve_id: 0 });
        assert!(a.easing_curves.iter().any(|c| c.id == 0), "built-in must survive");
    }

    #[test]
    fn test_rename_easing_curve() {
        let mut a = app();
        a.apply(Action::RenameEasingCurve { curve_id: 0, name: "Flat".to_string() });
        assert_eq!(a.easing_curves[0].name, "Flat");
    }

    #[test]
    fn test_set_keyframe_easing_curve() {
        let mut a = app();
        // Map keyframe 42 to built-in EaseOut (id=2)
        a.apply(Action::SetKeyframeEasingCurve { keyframe_id: 42, curve_id: 2 });
        assert_eq!(a.keyframe_easing_map.get(&42), Some(&2));
    }

    #[test]
    fn test_set_keyframe_easing_curve_invalid_ignored() {
        let mut a = app();
        a.apply(Action::SetKeyframeEasingCurve { keyframe_id: 1, curve_id: 9999 });
        assert!(a.keyframe_easing_map.get(&1).is_none());
    }

    #[test]
    fn test_spring_easing_moves_toward_one() {
        let a = app();
        let kind = EasingCurveKind::Spring {
            stiffness: 200.0,
            damping: 20.0,
            initial_velocity: 0.0,
        };
        let v = a.sample_easing(&kind, 1.0);
        // Spring should be somewhere close to 1.0 at t=1 (may overshoot slightly)
        assert!(v > 0.5, "spring should be well past 0 at t=1, got {v}");
    }
}
