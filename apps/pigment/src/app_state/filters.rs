use super::*;

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
