use super::*;

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
