//! Advanced filter apply arms: Surface Blur and Path Blur.
//!
//! These follow the `filters.rs` read → transform → upload pattern: pull the
//! active layer's premultiplied RGBA f32 buffer, run the pure CPU filter from
//! `crate::filters_extra`, and upload the result back. No engine/shader change.

use super::{Action, App};

impl App {
    pub(super) fn apply_filters_advanced(&mut self, action: Action) {
        match action {
            Action::ApplySurfaceBlur { radius, threshold } => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let r = crate::filters_extra::surface_blur(&px, dw, dh, radius, threshold);
                    self.host.upload_layer_f32(layer, &r);
                    self.status_message = Some("Surface Blur applied".to_string());
                }
            }
            Action::ApplyPathBlur { segments, strength } => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let r = crate::filters_extra::path_blur(&px, dw, dh, &segments, strength);
                    self.host.upload_layer_f32(layer, &r);
                    self.status_message = Some("Path Blur applied".to_string());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters_extra::PathSegment;

    #[test]
    fn surface_blur_action_runs() {
        let mut app = App::new();
        app.apply(Action::ApplySurfaceBlur { radius: 3.0, threshold: 0.1 });
        // No panic; status set when a layer was present.
        let _ = app.status_message.clone();
    }

    #[test]
    fn path_blur_action_runs() {
        let mut app = App::new();
        let segs = vec![PathSegment { start: [0.0, 0.0], end: [10.0, 0.0] }];
        app.apply(Action::ApplyPathBlur { segments: segs, strength: 1.0 });
        let _ = app.status_message.clone();
    }

    #[test]
    fn path_blur_empty_segments_no_panic() {
        let mut app = App::new();
        app.apply(Action::ApplyPathBlur { segments: vec![], strength: 1.0 });
    }
}
