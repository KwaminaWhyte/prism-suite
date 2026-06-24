//! Free-transform extension: rotation angle + skew x/y on top of the existing
//! translate + scale transform. Composes all four into the engine's inverse
//! sampling matrix via `super::compute_xform_full`, sets it as the live layer
//! transform, and bakes it into pixels through the same `bake_layer_xform` path
//! the interactive Transform tool uses.

use super::{Action, App};

impl App {
    pub(super) fn apply_transform_extra(&mut self, action: Action) {
        match action {
            Action::SetTransformRotation(deg) => {
                // Wrap to -180..180 for a stable UI value.
                let mut a = deg % 360.0;
                if a > 180.0 {
                    a -= 360.0;
                } else if a < -180.0 {
                    a += 360.0;
                }
                self.xform_rotation_deg = a;
            }
            Action::SetTransformSkew { skew_x, skew_y } => {
                self.xform_skew_x_deg = skew_x.clamp(-89.0, 89.0);
                self.xform_skew_y_deg = skew_y.clamp(-89.0, 89.0);
            }
            Action::ResetFreeTransform => {
                self.xform_translate = [0.0, 0.0];
                self.xform_scale = 1.0;
                self.xform_rotation_deg = 0.0;
                self.xform_skew_x_deg = 0.0;
                self.xform_skew_y_deg = 0.0;
            }
            Action::ApplyFreeTransform => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (w, h) = (self.doc.size.width as f32, self.doc.size.height as f32);
                let (m, off) = super::compute_xform_full(
                    self.xform_translate,
                    self.xform_scale,
                    self.xform_rotation_deg,
                    self.xform_skew_x_deg,
                    self.xform_skew_y_deg,
                    w,
                    h,
                );
                self.host.set_layer_xform(Some(layer), m, off);
                self.host.bake_layer_xform(layer);
                // Clear the live transform back to identity.
                self.host.set_layer_xform(None, [1.0, 0.0, 0.0, 1.0], [0.0; 2]);
                self.host.mark_dirty();
                self.xform_translate = [0.0, 0.0];
                self.xform_scale = 1.0;
                self.xform_rotation_deg = 0.0;
                self.xform_skew_x_deg = 0.0;
                self.xform_skew_y_deg = 0.0;
                self.status_message = Some("Free Transform applied".to_string());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::compute_xform_full;

    #[test]
    fn set_rotation_wraps() {
        let mut app = App::new();
        app.apply(Action::SetTransformRotation(270.0));
        assert!((app.xform_rotation_deg - (-90.0)).abs() < 1e-3);
        app.apply(Action::SetTransformRotation(45.0));
        assert!((app.xform_rotation_deg - 45.0).abs() < 1e-3);
    }

    #[test]
    fn set_skew_clamps() {
        let mut app = App::new();
        app.apply(Action::SetTransformSkew { skew_x: 120.0, skew_y: -200.0 });
        assert!((app.xform_skew_x_deg - 89.0).abs() < 1e-3);
        assert!((app.xform_skew_y_deg - (-89.0)).abs() < 1e-3);
    }

    #[test]
    fn reset_free_transform_identity() {
        let mut app = App::new();
        app.apply(Action::SetTransformRotation(30.0));
        app.apply(Action::SetTransformSkew { skew_x: 10.0, skew_y: 5.0 });
        app.apply(Action::ResetFreeTransform);
        assert_eq!(app.xform_rotation_deg, 0.0);
        assert_eq!(app.xform_skew_x_deg, 0.0);
        assert_eq!(app.xform_skew_y_deg, 0.0);
        assert_eq!(app.xform_scale, 1.0);
    }

    #[test]
    fn identity_xform_is_identity_matrix() {
        // No rotation/skew/translate, scale 1 → sampling matrix is identity,
        // offset zero.
        let (m, off) = compute_xform_full([0.0, 0.0], 1.0, 0.0, 0.0, 0.0, 100.0, 100.0);
        assert!((m[0] - 1.0).abs() < 1e-4);
        assert!(m[1].abs() < 1e-4);
        assert!(m[2].abs() < 1e-4);
        assert!((m[3] - 1.0).abs() < 1e-4);
        assert!(off[0].abs() < 1e-4 && off[1].abs() < 1e-4, "{:?}", off);
    }

    #[test]
    fn scale_two_halves_sampling_matrix() {
        // Forward scale 2 → inverse sampling matrix scales by 0.5.
        let (m, off) = compute_xform_full([0.0, 0.0], 2.0, 0.0, 0.0, 0.0, 100.0, 100.0);
        assert!((m[0] - 0.5).abs() < 1e-4, "{:?}", m);
        assert!((m[3] - 0.5).abs() < 1e-4);
        // Centre maps to centre: off should re-centre at 0.5*(1-0.5) = 0.25.
        assert!((off[0] - 0.25).abs() < 1e-4, "{:?}", off);
    }

    #[test]
    fn rotation_90_swaps_axes() {
        // 90° rotation: forward R maps x->y; inverse sampling matrix has the
        // off-diagonal terms dominant (m00 ~ 0).
        let (m, _off) = compute_xform_full([0.0, 0.0], 1.0, 90.0, 0.0, 0.0, 100.0, 100.0);
        assert!(m[0].abs() < 1e-4, "m00 not ~0: {}", m[0]);
        assert!(m[1].abs() > 0.5 || m[2].abs() > 0.5, "no rotation in matrix: {:?}", m);
    }

    #[test]
    fn skew_x_produces_shear_term() {
        // A horizontal skew introduces a nonzero off-diagonal in the matrix.
        let (m, _off) = compute_xform_full([0.0, 0.0], 1.0, 0.0, 30.0, 0.0, 100.0, 100.0);
        assert!(m[1].abs() > 1e-3, "no skew term: {:?}", m);
    }

    #[test]
    fn apply_free_transform_no_panic_and_resets() {
        let mut app = App::new();
        app.apply(Action::SetTransformRotation(15.0));
        app.apply(Action::SetTransformSkew { skew_x: 5.0, skew_y: 0.0 });
        app.apply(Action::ApplyFreeTransform);
        // After baking, the live params reset to identity.
        assert_eq!(app.xform_rotation_deg, 0.0);
        assert_eq!(app.xform_scale, 1.0);
    }
}
