use super::*;

/// Script language used by the expression engine.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ExprLang {
    #[default]
    JavaScript,
    Python,
}


/// Expression control layer kind (Wave 14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExprControlKind {
    Slider,
    Angle,
    Checkbox,
    Color,
    Point,
}

impl ExprControlKind {
    /// Short badge label for the UI (used by the expr_controls panel).
    pub fn label(self) -> &'static str {
        match self {
            ExprControlKind::Slider => "SL",
            ExprControlKind::Angle => "∠",
            ExprControlKind::Checkbox => "☑",
            ExprControlKind::Color => "◉",
            ExprControlKind::Point => "↖",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ExprControlKind::Slider => "Slider Control",
            ExprControlKind::Angle => "Angle Control",
            ExprControlKind::Checkbox => "Checkbox Control",
            ExprControlKind::Color => "Color Control",
            ExprControlKind::Point => "Point Control",
        }
    }

    /// All control kinds, for Add-Control buttons.
    pub const ALL: [ExprControlKind; 5] = [
        ExprControlKind::Slider,
        ExprControlKind::Angle,
        ExprControlKind::Checkbox,
        ExprControlKind::Color,
        ExprControlKind::Point,
    ];
}

/// Runtime value for an expression control layer.
#[derive(Clone, Debug)]
pub enum ExprControlValue {
    Slider(f32),
    Angle(f32),
    Checkbox(bool),
    Color([f32; 4]),
    Point(f32, f32),
}

/// Expression control layer state (one per ExpressionControl layer).
#[derive(Clone, Debug)]
pub struct ExprControl {
    pub kind: ExprControlKind,
    pub value: ExprControlValue,
}

impl ExprControl {
    pub fn default_for(kind: ExprControlKind) -> Self {
        let value = match kind {
            ExprControlKind::Slider => ExprControlValue::Slider(50.0),
            ExprControlKind::Angle => ExprControlValue::Angle(0.0),
            ExprControlKind::Checkbox => ExprControlValue::Checkbox(false),
            ExprControlKind::Color => ExprControlValue::Color([1.0, 1.0, 1.0, 1.0]),
            ExprControlKind::Point => ExprControlValue::Point(0.0, 0.0),
        };
        ExprControl { kind, value }
    }
}

impl App {
    pub(super) fn apply_expressions(&mut self, action: Action) {
        match action {
            // --- Wave 14: Expression controls ---
            Action::AddExpressionControl(kind) => {
                let ci = self.active_comp_index();
                let name = kind.name().to_string();
                let mut layer = crate::comp::PulseLayer::of_kind(
                    crate::comp::LayerKind::ExpressionControl,
                    name,
                    [0.3, 0.5, 0.9, 1.0],
                );
                layer.visible = false;
                let idx = self.project.comps[ci].layers.len();
                self.project.comps[ci].layers.push(layer);
                self.expr_controls.insert(idx, ExprControl::default_for(kind));
            }
            Action::SetExprControlValue { layer_idx, value } => {
                if let Some(ctrl) = self.expr_controls.get_mut(&layer_idx) {
                    ctrl.value = value;
                }
            }

            // --- Batch 4: Expression Engine depth ---
            Action::SetExpressionEnabled { layer_id, prop, enabled } => {
                self.expr_enabled.insert((layer_id, prop), enabled);
            }
            Action::AddExpressionError { layer_id, prop, error } => {
                self.expr_errors.insert((layer_id, prop), error);
            }
            Action::ClearExpressionErrors { layer_id } => {
                self.expr_errors.retain(|k, _| k.0 != layer_id);
            }
            Action::SetExpressionLanguage(l) => {
                self.expr_language = l;
            }
            Action::EvaluateExpression { layer_id, prop: _, at_time } => {
                self.last_expr_result = Some(at_time * layer_id as f32);
            }

            _ => unreachable!("apply_expressions called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr_language_set() {
        let mut app = App::new();
        assert_eq!(app.expr_language, ExprLang::JavaScript);
        app.apply(Action::SetExpressionLanguage(ExprLang::Python));
        assert_eq!(app.expr_language, ExprLang::Python);
    }

    #[test]
    fn test_expr_enable_disable() {
        let mut app = App::new();
        app.apply(Action::SetExpressionEnabled { layer_id: 0, prop: "X".to_string(), enabled: true });
        assert_eq!(app.expr_enabled.get(&(0, "X".to_string())), Some(&true));
        app.apply(Action::SetExpressionEnabled { layer_id: 0, prop: "X".to_string(), enabled: false });
        assert_eq!(app.expr_enabled.get(&(0, "X".to_string())), Some(&false));
    }

    #[test]
    fn test_expr_clear_errors_for_layer() {
        let mut app = App::new();
        app.apply(Action::AddExpressionError { layer_id: 0, prop: "X".to_string(), error: "err1".to_string() });
        app.apply(Action::AddExpressionError { layer_id: 0, prop: "Y".to_string(), error: "err2".to_string() });
        app.apply(Action::AddExpressionError { layer_id: 1, prop: "X".to_string(), error: "err3".to_string() });
        assert_eq!(app.expr_errors.len(), 3);
        app.apply(Action::ClearExpressionErrors { layer_id: 0 });
        assert_eq!(app.expr_errors.len(), 1);
        assert!(app.expr_errors.contains_key(&(1, "X".to_string())));
    }

    #[test]
    fn test_expr_evaluate_result() {
        let mut app = App::new();
        app.apply(Action::EvaluateExpression { layer_id: 2, prop: "Scale".to_string(), at_time: 3.0 });
        // Stub: result = at_time * layer_id
        assert_eq!(app.last_expr_result, Some(6.0));
    }
}
