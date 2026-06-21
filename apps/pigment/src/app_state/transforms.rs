use super::*;

impl App {
    pub(super) fn apply_transforms(&mut self, action: Action) {
        match action {
            Action::ApplyCrop { x, y, w, h } => {
                let dw = self.host.doc_w;
                let dh = self.host.doc_h;
                let cx = x.min(dw.saturating_sub(1));
                let cy = y.min(dh.saturating_sub(1));
                let cw = w.min(dw - cx).max(1);
                let ch = h.min(dh - cy).max(1);
                self.host.crop_document(&mut self.doc, cx, cy, cw, ch);
                self.crop_rect = None;
                self.status_message = Some(format!("Cropped to {cw}×{ch}"));
            }
            Action::CancelCrop => {
                self.crop_rect = None;
            }
            Action::SetCaCropAngle(a) => {
                self.ca_crop_config.angle = a.clamp(-45.0, 45.0);
            }
            Action::SetCaCropFillMethod(m) => {
                self.ca_crop_config.fill_method = m;
            }
            Action::SetCaCropEnabled(e) => {
                self.ca_crop_config.enabled = e;
            }
            Action::ApplyCaCrop { rect } => {
                self.last_ca_crop_rect = Some(rect);
            }
            _ => {}
        }
    }
}
