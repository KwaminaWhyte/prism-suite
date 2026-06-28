//! Phase 8 — Blur / Sharpen / Distort gallery apply arms.
//!
//! Wires the pure CPU algorithms in `crate::filters_gallery` into the standard
//! `read_layer_f32 → pure fn → snapshot_layer → upload_layer_f32` idiom (the same
//! one `healing.rs` / `liquify.rs` / `filters_advanced.rs` use). All filter math
//! lives in `filters_gallery.rs`; this module only marshals the active layer's
//! pixels through it, so each apply is one COW undo step. No engine/shader change.

use super::{Action, App};

/// A blur / sharpen op from the Phase 8 gallery, carried by
/// [`Action::ApplyGalleryBlur`]. One enum keeps the (already large) `Action` enum
/// from gaining a variant per filter; the displacement map is separate because it
/// needs a second source layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GalleryBlur {
    /// Separable Gaussian blur of standard deviation `sigma` px.
    Gaussian { sigma: f32 },
    /// Separable running-sum box blur of integer `radius` px.
    Box { radius: u32 },
    /// Multi-pass box-blur approximation of a Gaussian (`passes` ≥ 1).
    BoxGaussian { sigma: f32, passes: u32 },
    /// Directional motion blur: `angle_deg`, smear `length` px.
    Motion { angle_deg: f32, length: f32 },
    /// Zoom blur toward/away from a centre, `amount` 0..1 over `samples` taps.
    Zoom { cx: f32, cy: f32, amount: f32, samples: u32 },
    /// Spin (rotational) blur around a centre, sweeping `angle_deg`.
    Spin { cx: f32, cy: f32, angle_deg: f32, samples: u32 },
    /// Unsharp mask: blur `sigma`, boost `amount`, gate `threshold`.
    UnsharpMask { sigma: f32, amount: f32, threshold: f32 },
    /// Edge-aware smart sharpen: blur `sigma`, boost `amount`, variance gate
    /// `threshold`.
    SmartSharpen { sigma: f32, amount: f32, threshold: f32 },
}

impl GalleryBlur {
    /// Short human-readable label for the history / status bar.
    fn label(&self) -> &'static str {
        match self {
            GalleryBlur::Gaussian { .. } => "Gaussian Blur",
            GalleryBlur::Box { .. } => "Box Blur",
            GalleryBlur::BoxGaussian { .. } => "Box Blur (Gaussian)",
            GalleryBlur::Motion { .. } => "Motion Blur",
            GalleryBlur::Zoom { .. } => "Zoom Blur",
            GalleryBlur::Spin { .. } => "Spin Blur",
            GalleryBlur::UnsharpMask { .. } => "Unsharp Mask",
            GalleryBlur::SmartSharpen { .. } => "Smart Sharpen",
        }
    }

    /// Run the op against `px` (`w*h*4` premultiplied RGBA f32) → new buffer.
    fn run(&self, px: &[f32], w: u32, h: u32) -> Vec<f32> {
        use crate::filters_gallery as fg;
        match *self {
            GalleryBlur::Gaussian { sigma } => fg::gaussian_blur(px, w, h, sigma),
            GalleryBlur::Box { radius } => fg::box_blur(px, w, h, radius),
            GalleryBlur::BoxGaussian { sigma, passes } => {
                fg::box_blur_gaussian(px, w, h, sigma, passes as usize)
            }
            GalleryBlur::Motion { angle_deg, length } => fg::motion_blur(px, w, h, angle_deg, length),
            GalleryBlur::Zoom { cx, cy, amount, samples } => {
                fg::zoom_blur(px, w, h, [cx, cy], amount, samples)
            }
            GalleryBlur::Spin { cx, cy, angle_deg, samples } => {
                fg::spin_blur(px, w, h, [cx, cy], angle_deg, samples)
            }
            GalleryBlur::UnsharpMask { sigma, amount, threshold } => {
                fg::unsharp_mask(px, w, h, sigma, amount, threshold)
            }
            GalleryBlur::SmartSharpen { sigma, amount, threshold } => {
                fg::smart_sharpen(px, w, h, sigma, amount, threshold)
            }
        }
    }
}

impl App {
    pub(super) fn apply_filters_blur(&mut self, action: Action) {
        match action {
            Action::ApplyGalleryBlur(op) => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let out = op.run(&px, dw, dh);
                    self.host.snapshot_layer(layer, op.label());
                    self.host.upload_layer_f32(layer, &out);
                    self.status_message = Some(format!("{} applied", op.label()));
                }
            }
            Action::ApplyDisplacementMap { source, scale_x, scale_y } => {
                let Some(target) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                // Read the displacement source first, then the target.
                let Some(disp) = self.host.read_layer_f32(source) else {
                    return;
                };
                if let Some(px) = self.host.read_layer_f32(target) {
                    let out = crate::filters_gallery::displacement_map(
                        &px, dw, dh, &disp, dw, dh, scale_x, scale_y,
                    );
                    self.host.snapshot_layer(target, "Displace");
                    self.host.upload_layer_f32(target, &out);
                    self.status_message = Some("Displacement map applied".into());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_blur_action_runs_no_panic() {
        let mut app = App::new();
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::Gaussian { sigma: 2.0 }));
        let _ = app.status_message.clone();
    }

    #[test]
    fn box_blur_action_runs() {
        let mut app = App::new();
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::Box { radius: 3 }));
        let _ = app.status_message.clone();
    }

    #[test]
    fn motion_blur_action_runs() {
        let mut app = App::new();
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::Motion { angle_deg: 45.0, length: 8.0 }));
        let _ = app.status_message.clone();
    }

    #[test]
    fn unsharp_action_runs() {
        let mut app = App::new();
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::UnsharpMask {
            sigma: 1.5,
            amount: 1.0,
            threshold: 0.0,
        }));
        let _ = app.status_message.clone();
    }

    #[test]
    fn smart_sharpen_action_runs() {
        let mut app = App::new();
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::SmartSharpen {
            sigma: 1.5,
            amount: 1.0,
            threshold: 0.05,
        }));
        let _ = app.status_message.clone();
    }

    #[test]
    fn zoom_and_spin_actions_run() {
        let mut app = App::new();
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::Zoom {
            cx: 8.0,
            cy: 8.0,
            amount: 0.3,
            samples: 8,
        }));
        app.apply(Action::ApplyGalleryBlur(GalleryBlur::Spin {
            cx: 8.0,
            cy: 8.0,
            angle_deg: 15.0,
            samples: 8,
        }));
        let _ = app.status_message.clone();
    }

    #[test]
    fn displacement_action_runs_no_panic() {
        let mut app = App::new();
        // With a single default layer, source == target is fine; just exercise the
        // dispatch path without panicking.
        if let Some(layer) = app.paint_target() {
            app.apply(Action::ApplyDisplacementMap {
                source: layer,
                scale_x: 4.0,
                scale_y: 4.0,
            });
        }
        let _ = app.status_message.clone();
    }

    #[test]
    fn gallery_label_matches_variant() {
        assert_eq!(GalleryBlur::Gaussian { sigma: 1.0 }.label(), "Gaussian Blur");
        assert_eq!(GalleryBlur::SmartSharpen { sigma: 1.0, amount: 1.0, threshold: 0.0 }.label(), "Smart Sharpen");
    }
}
