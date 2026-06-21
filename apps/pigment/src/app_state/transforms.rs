use super::*;

// ---- Batch 5 (new): Content-Aware Crop --------------------------------------

/// Fill method used when Content-Aware Crop extends canvas edges.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub enum CaFillMethod {
    #[default]
    ContentAware,
    EdgeExtend,
    Transparent,
}

/// Parameters for the Content-Aware Crop tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContentAwareCropConfig {
    /// Rotation correction angle in degrees.
    pub angle: f32,
    /// Which algorithm fills the exposed areas.
    pub fill_method: CaFillMethod,
    /// Whether content-aware fill is active during crop.
    pub enabled: bool,
}

impl Default for ContentAwareCropConfig {
    fn default() -> Self {
        Self { angle: 0.0, fill_method: CaFillMethod::ContentAware, enabled: true }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ca_fill_method_default() {
        let m = CaFillMethod::default();
        assert!(matches!(m, CaFillMethod::ContentAware));
    }

    #[test]
    fn test_content_aware_crop_config_default() {
        let cfg = ContentAwareCropConfig::default();
        assert_eq!(cfg.angle, 0.0);
        assert!(cfg.enabled);
        assert!(matches!(cfg.fill_method, CaFillMethod::ContentAware));
    }

    #[test]
    fn test_set_ca_crop_angle_clamps() {
        let mut app = App::new();
        app.apply(Action::SetCaCropAngle(90.0));
        assert_eq!(app.ca_crop_config.angle, 45.0);
        app.apply(Action::SetCaCropAngle(-90.0));
        assert_eq!(app.ca_crop_config.angle, -45.0);
        app.apply(Action::SetCaCropAngle(10.0));
        assert_eq!(app.ca_crop_config.angle, 10.0);
    }

    #[test]
    fn test_set_ca_crop_enabled() {
        let mut app = App::new();
        app.apply(Action::SetCaCropEnabled(false));
        assert!(!app.ca_crop_config.enabled);
        app.apply(Action::SetCaCropEnabled(true));
        assert!(app.ca_crop_config.enabled);
    }

    #[test]
    fn test_cancel_crop_clears_rect() {
        let mut app = App::new();
        app.apply(Action::CancelCrop);
        assert!(app.crop_rect.is_none());
    }
}
