use super::*;

/// A destructive filter/adjustment the host can apply to the active layer through
/// the engine's existing passes. Each variant carries its (default) params and
/// maps to one `CanvasHost::apply_*` call — which forwards to the matching
/// `prism_canvas::CanvasGpu::apply_*`. Reuses the engine pipeline verbatim; the
/// host adds no filter math. Params mirror the egui app's defaults
/// (`pigment-app/src/app/retouch.rs`).
#[derive(Clone, Copy, Debug)]
pub enum Filter {
    /// Separable Gaussian blur (engine kind 1).
    GaussianBlur { radius: f32 },
    /// Separable box blur (engine kind 5).
    BoxBlur { radius: f32 },
    /// Unsharp-style sharpen (engine kind 2).
    Sharpen { amount: f32 },
    /// Posterize: quantize to N levels (engine `apply_posterize`).
    Posterize { levels: u32 },
    /// Threshold to black/white at a luma cutoff (engine `apply_threshold`).
    Threshold { level: f32 },
    /// Find Edges (engine stylize kind 13).
    FindEdges { width: f32 },
    /// Emboss (engine stylize kind 14).
    Emboss { amount: f32, width: f32 },
    /// Add monochrome gaussian noise (engine `apply_noise`).
    AddNoise { amount: f32 },
    /// Unsharp mask: blur then sharpen (amount × (original − blurred)).
    UnsharpMask { radius: f32, amount: f32, threshold: f32 },
    /// Radial spin blur around canvas center (proxy via gaussian blur).
    RadialBlur { amount: f32 },
    /// Lens correction: barrel/pincushion distortion + vignette.
    LensCorrection { barrel: f32, pincushion: f32, vignette: f32 },
    // --- Batch 5: additional CPU raster filters ---
    /// Directional motion blur: `angle` degrees, `distance` px smear.
    MotionBlur { angle: f32, distance: f32 },
    /// Twirl distort: swirl `angle` degrees about the centre, falling off to
    /// zero at `radius` (fraction 0..1 of the half-diagonal).
    Twirl { angle: f32, radius: f32 },
    /// Pinch / Bulge distort: signed `amount` (-1 bulge .. +1 pinch) within
    /// `radius` (fraction 0..1 of the half-diagonal).
    Pinch { amount: f32, radius: f32 },
    /// Solarize: invert tones above `threshold` (0..1) per channel.
    Solarize { threshold: f32 },
    /// Glowing Edges: Sobel edge glow; `width` step, `intensity` brightness.
    GlowingEdges { width: f32, intensity: f32 },
    /// High Pass: subtract a Gaussian blur from the original, shift to 0.5 grey.
    /// `radius` is the blur radius in px; output is a detail-isolation layer.
    HighPass { radius: f32 },
    /// Smart Sharpen: unsharp mask with configurable noise reduction pre-pass.
    SmartSharpen { amount: f32, radius: f32, reduce_noise: f32, mode: SmartSharpenMode },
    /// Reduce Noise: strength-controlled smoothing with detail/colour preservation.
    ReduceNoise { strength: f32, preserve_details: f32, reduce_color_noise: f32, sharpen_details: f32 },
}

/// Blur model used by Smart Sharpen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartSharpenMode {
    GaussianBlur,
    LensBlur,
    MotionBlur,
}

// ---- Batch 4 extended: HDR Tone Mapping -----------------------------------------

/// Tone-mapping operator used when converting HDR/EXR content to display range.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ToneMapMethod {
    /// Simple Reinhard (per-channel normalization). Default.
    #[default]
    Reinhard,
    /// Filmic S-curve (approximates film response).
    Filmic,
    /// ACES Cg reference transform.
    AcesCg,
    /// Pure exposure adjustment (no curve shaping).
    Exposure,
}

// ---- Batch 4 extended: Neural Filters -------------------------------------------

/// A single entry in the neural-filter stack.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NeuralFilter {
    pub kind: NeuralFilterKind,
    /// Blend/effect strength in 0..=1.
    pub strength: f32,
    pub enabled: bool,
}

impl Default for NeuralFilter {
    fn default() -> Self {
        Self {
            kind: NeuralFilterKind::SkinSmoothing,
            strength: 0.5,
            enabled: true,
        }
    }
}

/// The AI-powered filter kind (stubs — no actual inference; GPU pass planned).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum NeuralFilterKind {
    #[default]
    SkinSmoothing,
    SmartPortrait,
    StyleTransfer,
    Colorize,
    SuperZoom,
    JpegArtifactRemoval,
    NoiseReduction,
    DepthBlur,
}

impl App {
    pub(super) fn apply_filters(&mut self, action: Action) {
        match action {
            Action::ApplyFilter(filter) => {
                let Some(layer) = self.paint_target() else {
                    return;
                };
                match filter {
                    Filter::GaussianBlur { radius } => {
                        self.host.apply_filter(layer, 1, radius, 0.0);
                    }
                    Filter::BoxBlur { radius } => {
                        self.host.apply_filter(layer, 5, radius, 0.0);
                    }
                    Filter::Sharpen { amount } => {
                        self.host.apply_filter(layer, 2, 0.0, amount);
                    }
                    Filter::Posterize { levels } => {
                        self.host.apply_posterize(layer, levels);
                    }
                    Filter::Threshold { level } => {
                        self.host.apply_threshold(layer, level);
                    }
                    Filter::FindEdges { width } => {
                        self.host.apply_stylize(layer, 13, 0.0, width, [0.0; 2]);
                    }
                    Filter::Emboss { amount, width } => {
                        self.host.apply_stylize(layer, 14, amount, width, [1.0, 1.0]);
                    }
                    Filter::AddNoise { amount } => {
                        self.host.apply_noise(layer, amount, true, true, 1.0);
                    }
                    Filter::UnsharpMask { radius, amount, .. } => {
                        self.host.apply_filter(layer, 1, radius, 0.0);
                        self.host.apply_filter(layer, 2, radius, amount);
                    }
                    Filter::RadialBlur { amount } => {
                        let r = (amount / 30.0).max(0.5);
                        self.host.apply_filter(layer, 1, r, 0.0);
                    }
                    Filter::LensCorrection { barrel, pincushion, vignette } => {
                        self.apply(Action::ApplyLensCorrection { barrel, pincushion, vignette });
                    }
                    Filter::MotionBlur { angle, distance } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::motion_blur(&px, dw, dh, angle, distance);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Motion Blur applied".to_string());
                        }
                    }
                    Filter::Twirl { angle, radius } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::twirl(&px, dw, dh, angle, radius);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Twirl applied".to_string());
                        }
                    }
                    Filter::Pinch { amount, radius } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::pinch(&px, dw, dh, amount, radius);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Pinch applied".to_string());
                        }
                    }
                    Filter::Solarize { threshold } => {
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::solarize(&px, threshold);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Solarize applied".to_string());
                        }
                    }
                    Filter::GlowingEdges { width, intensity } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::glowing_edges(&px, dw, dh, width, intensity);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Glowing Edges applied".to_string());
                        }
                    }
                    Filter::HighPass { radius } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::high_pass(&px, dw, dh, radius);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("High Pass applied".to_string());
                        }
                    }
                    Filter::SmartSharpen { amount, radius, reduce_noise, mode: _ } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::smart_sharpen(&px, dw, dh, amount, radius, reduce_noise);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Smart Sharpen applied".to_string());
                        }
                    }
                    Filter::ReduceNoise { strength, preserve_details, reduce_color_noise, sharpen_details } => {
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        if let Some(px) = self.host.read_layer_f32(layer) {
                            let r = crate::filters::reduce_noise(&px, dw, dh, strength, preserve_details, reduce_color_noise, sharpen_details);
                            self.host.upload_layer_f32(layer, &r);
                            self.status_message = Some("Reduce Noise applied".to_string());
                        }
                    }
                }
            }
            Action::AddSmartFilter(id, sf) => {
                self.smart_filters.entry(id).or_default().push(sf);
            }
            Action::RemoveSmartFilter(id, idx) => {
                if let Some(v) = self.smart_filters.get_mut(&id) {
                    if idx < v.len() { v.remove(idx); }
                }
            }
            Action::EditSmartFilter(id, idx, sf) => {
                if let Some(v) = self.smart_filters.get_mut(&id) {
                    if idx < v.len() { v[idx] = sf; }
                }
            }
            Action::OpenFilterGallery => { self.filter_gallery_open = true; }
            Action::CloseFilterGallery => { self.filter_gallery_open = false; }
            Action::ToggleFilterGallery => { self.filter_gallery_open = !self.filter_gallery_open; }
            Action::ContentAwareFill => {
                let Some(layer) = self.paint_target() else { return; };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let mask = self.host.read_selection_or_empty();
                    let filled = crate::content_aware::content_aware_fill(&px, dw, dh, &mask);
                    self.host.upload_layer_f32(layer, &filled);
                    self.status_message = Some("Content-Aware Fill applied".to_string());
                }
            }
            Action::ApplyLensCorrection { barrel, pincushion, vignette } => {
                let Some(layer) = self.paint_target() else { return; };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let result = crate::lens_correction::apply_lens_correction(
                        &px, dw, dh, barrel, pincushion, vignette,
                    );
                    self.host.upload_layer_f32(layer, &result);
                    self.status_message = Some("Lens Correction applied".to_string());
                }
            }
            Action::PerspectiveWarp { src_pts, dst_pts } => {
                let Some(layer) = self.paint_target() else { return; };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                if let Some(px) = self.host.read_layer_f32(layer) {
                    let result = crate::perspective_warp::apply_perspective_warp(
                        &px, dw, dh, src_pts, dst_pts,
                    );
                    self.host.upload_layer_f32(layer, &result);
                    self.status_message = Some("Perspective Warp applied".to_string());
                }
            }
            Action::ToggleLensCorrection => {
                self.lens_correction_open = !self.lens_correction_open;
            }
            Action::SetLensParam(key, val) => {
                match key {
                    "barrel"     => self.lens_barrel     = val,
                    "pincushion" => self.lens_pincushion = val,
                    "vignette"   => self.lens_vignette   = val,
                    _ => {}
                }
            }
            Action::ApplyToneMap { method, exposure, gamma } => {
                self.last_tone_map = Some(method);
                let _ = (exposure, gamma);
            }
            Action::SetToneMapPreview(b) => {
                self.tone_map_preview = b;
            }
            Action::ToggleNeuralFiltersPanel => {
                self.neural_filters_panel_open = !self.neural_filters_panel_open;
            }
            Action::AddNeuralFilter(k) => {
                self.neural_filters.push(NeuralFilter { kind: k, ..Default::default() });
            }
            Action::RemoveNeuralFilter(i) => {
                if i < self.neural_filters.len() {
                    self.neural_filters.remove(i);
                }
            }
            Action::SetNeuralFilterStrength { idx, strength } => {
                if let Some(f) = self.neural_filters.get_mut(idx) {
                    f.strength = strength.clamp(0.0, 1.0);
                }
            }
            Action::ToggleNeuralFilter(i) => {
                if i < self.neural_filters.len() {
                    self.neural_filters[i].enabled = !self.neural_filters[i].enabled;
                }
            }
            Action::ApplyNeuralFilters => {
                self.last_neural_apply_count = self.neural_filters.iter().filter(|f| f.enabled).count();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_gaussian_blur() {
        let f = Filter::GaussianBlur { radius: 5.0 };
        match f {
            Filter::GaussianBlur { radius } => assert_eq!(radius, 5.0),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_smart_sharpen_mode_variants() {
        let _ = SmartSharpenMode::GaussianBlur;
        let _ = SmartSharpenMode::LensBlur;
        let _ = SmartSharpenMode::MotionBlur;
    }

    #[test]
    fn test_tone_map_method_variants() {
        let _ = ToneMapMethod::Reinhard;
        let _ = ToneMapMethod::Filmic;
    }

    #[test]
    fn test_neural_filter_default() {
        let nf = NeuralFilter { kind: NeuralFilterKind::SkinSmoothing, ..Default::default() };
        assert!(nf.enabled);
        assert_eq!(nf.strength, 0.5);
    }

    #[test]
    fn test_neural_filter_kind_variants() {
        let _ = NeuralFilterKind::SuperZoom;
        let _ = NeuralFilterKind::NoiseReduction;
        let _ = NeuralFilterKind::DepthBlur;
    }

    #[test]
    fn test_apply_filter_gaussian_blur() {
        let mut app = App::new();
        app.apply(Action::ApplyFilter(Filter::GaussianBlur { radius: 3.0 }));
    }

    #[test]
    fn test_add_neural_filter() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SkinSmoothing));
        assert_eq!(app.neural_filters.len(), 1);
    }

    #[test]
    fn test_remove_neural_filter() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SkinSmoothing));
        app.apply(Action::RemoveNeuralFilter(0));
        assert_eq!(app.neural_filters.len(), 0);
    }
}
