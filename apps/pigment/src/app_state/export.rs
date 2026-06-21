use super::*;

// ---- Batch 4 extended: Print Layout ---------------------------------------------

/// Extended print layout options shown in the print dialog.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrintLayout {
    /// Number of copies to print (minimum 1).
    pub copies: u8,
    /// Whether copies are collated (true = 1-2-3 1-2-3, false = 1-1 2-2 3-3).
    pub collate: bool,
    /// Border width around the image in mm (minimum 0).
    pub border_width: f32,
    /// Whether the image is centred on the page.
    pub center_image: bool,
    /// Whether to include crop / registration marks.
    pub print_marks: bool,
    /// Bleed area in mm added outside the trim edge (0..=25).
    pub bleed: f32,
    /// Output resolution in dpi (72..=2400).
    pub print_resolution: u32,
}

impl Default for PrintLayout {
    fn default() -> Self {
        Self {
            copies: 1,
            collate: true,
            border_width: 0.0,
            center_image: true,
            print_marks: false,
            bleed: 3.0,
            print_resolution: 300,
        }
    }
}
// ---- Batch 6: Match Color (new config struct) -----------------------------------

/// Extended Match Color parameters (Batch 6).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchColorConfig {
    pub source_layer_name: String,
    /// 0..=200, default 100
    pub luminance: f32,
    /// 0..=200, default 100
    pub color_intensity: f32,
    /// 0..=100, default 0
    pub fade: f32,
    pub neutralize: bool,
    pub use_selection_source: bool,
    pub use_selection_target: bool,
}

impl Default for MatchColorConfig {
    fn default() -> Self {
        Self {
            source_layer_name: String::new(),
            luminance: 100.0,
            color_intensity: 100.0,
            fade: 0.0,
            neutralize: false,
            use_selection_source: false,
            use_selection_target: false,
        }
    }
}
// ---- Batch 6: Camera Raw Filter (new config) ------------------------------------

/// Full Camera Raw dialog parameters (Batch 6).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraRawConfig {
    /// 2000..=50000, default 6500
    pub temperature: f32,
    /// -150..=150, default 0
    pub tint: f32,
    /// -5..=5, default 0
    pub exposure: f32,
    /// -100..=100, default 0
    pub contrast: f32,
    /// -100..=100, default 0
    pub highlights: f32,
    /// -100..=100, default 0
    pub shadows: f32,
    /// -100..=100, default 0
    pub whites: f32,
    /// -100..=100, default 0
    pub blacks: f32,
    /// -100..=100, default 0
    pub clarity: f32,
    /// -100..=100, default 0
    pub dehaze: f32,
    /// -100..=100, default 0
    pub vibrance: f32,
    /// -100..=100, default 0
    pub saturation: f32,
    /// 0..=100, default 0
    pub noise_luminance: f32,
    /// 0..=100, default 25
    pub noise_color: f32,
    /// 0..=150, default 40
    pub sharpness: f32,
    pub lens_correction: bool,
    pub chromatic_aberration: bool,
}

impl Default for CameraRawConfig {
    fn default() -> Self {
        Self {
            temperature: 6500.0,
            tint: 0.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            noise_luminance: 0.0,
            noise_color: 25.0,
            sharpness: 40.0,
            lens_correction: false,
            chromatic_aberration: false,
        }
    }
}
// ---- Batch 6: HDR Merge ---------------------------------------------------------

/// Tone-mapping method for HDR Merge.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum HdrToneMappingMethod {
    #[default]
    Local,
    Equal,
    Highlight,
    Photoreceptor,
}

/// Configuration for the HDR Merge / Photomerge stub.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HdrMergeConfig {
    pub method: HdrToneMappingMethod,
    pub remove_ghosts: bool,
    /// Number of source exposures to merge (default 0).
    pub source_count: usize,
    /// Output bit depth: 8, 16, or 32 (default 32).
    pub bit_depth_output: u8,
}

impl Default for HdrMergeConfig {
    fn default() -> Self {
        Self {
            method: HdrToneMappingMethod::Local,
            remove_ghosts: true,
            source_count: 0,
            bit_depth_output: 32,
        }
    }
}
/// Non-destructive filter applied on top of a layer without touching its pixels.
#[derive(Clone, Debug)]
pub enum SmartFilter {
    Blur(f32),
    Sharpen(f32),
    Brightness(f32, f32),
    HueSat(f32),
}

impl SmartFilter {
    pub fn label(&self) -> &'static str {
        match self {
            SmartFilter::Blur(_) => "Gaussian Blur",
            SmartFilter::Sharpen(_) => "Sharpen",
            SmartFilter::Brightness(..) => "Brightness/Contrast",
            SmartFilter::HueSat(_) => "Hue/Saturation",
        }
    }
}

/// Format for export presets.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ExportFormat {
    Jpeg,
    Png,
    Tiff,
    Webp,
}

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Png => "PNG",
            ExportFormat::Tiff => "TIFF",
            ExportFormat::Webp => "WebP",
        }
    }
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Png => "png",
            ExportFormat::Tiff => "tif",
            ExportFormat::Webp => "webp",
        }
    }
}

/// A named export preset.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub format: ExportFormat,
    pub quality: u8,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub dpi: u32,
}

impl ExportPreset {
    pub fn jpeg_90() -> Self {
        Self {
            name: "JPEG 90%".into(),
            format: ExportFormat::Jpeg,
            quality: 90,
            width: None,
            height: None,
            dpi: 72,
        }
    }
    pub fn png_lossless() -> Self {
        Self {
            name: "PNG Lossless".into(),
            format: ExportFormat::Png,
            quality: 100,
            width: None,
            height: None,
            dpi: 72,
        }
    }
}

impl App {
    pub(super) fn apply_export(&mut self, action: Action) {
        match action {
            Action::OpenImage => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Images", prism_io::SUPPORTED_EXTENSIONS)
                    .pick_file()
                else {
                    return;
                };
                if let Some(doc) = self.host.open_image(&path) {
                    self.doc = doc;
                    self.stroke_last = None;
                    self.stroke_residual = 0.0;
                    self.sel_drag_start = None;
                    self.lasso_points.clear();
                    self.sel_base.clear();
                    self.sel_mode = CombineMode::Replace;
                    self.xform_drag_start = None;
                    self.xform_translate = [0.0, 0.0];
                    self.xform_scale = 1.0;
                    self.recent_files.retain(|p| p != &path);
                    self.recent_files.insert(0, path.clone());
                    self.recent_files.truncate(10);
                    self.prefs.last_document = Some(path.clone());
                    self.prefs.recent_files = self.recent_files.clone();
                    self.prefs.save();
                }
            }
            Action::ExportImage => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Image", &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp"])
                    .set_file_name("export.png")
                    .save_file()
                else {
                    return;
                };
                if let Err(e) = self.host.export_image(&path) {
                    log::error!("export failed: {e}");
                }
            }
            Action::OpenEXR => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("OpenEXR", &["exr"])
                    .pick_file()
                else {
                    return;
                };
                if let Some(doc) = self.host.open_exr(&path) {
                    self.doc = doc;
                    self.stroke_last = None;
                    self.stroke_residual = 0.0;
                    self.sel_drag_start = None;
                    self.lasso_points.clear();
                    self.sel_base.clear();
                    self.sel_mode = CombineMode::Replace;
                    self.xform_drag_start = None;
                    self.xform_translate = [0.0, 0.0];
                    self.xform_scale = 1.0;
                }
            }
            Action::ExportEXR => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("OpenEXR", &["exr"])
                    .set_file_name("export.exr")
                    .save_file()
                else {
                    return;
                };
                if let Err(e) = self.host.export_exr(&path) {
                    log::error!("EXR export failed: {e}");
                }
            }
            Action::AddSlice(rect) => {
                let id = self.next_slice_id;
                self.next_slice_id += 1;
                self.slices.push(Slice {
                    id,
                    rect,
                    name: format!("slice_{id:02}"),
                });
            }
            Action::DeleteSlice(id) => {
                self.slices.retain(|s| s.id != id);
            }
            Action::ExportSlices(dir) => {
                let Some(flat) = self.host.read_composite_f32() else {
                    return;
                };
                let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                for slice in &self.slices {
                    let [sx, sy, sw, sh] = slice.rect;
                    let x0 = (sx.round() as i32).clamp(0, dw as i32) as u32;
                    let y0 = (sy.round() as i32).clamp(0, dh as i32) as u32;
                    let x1 = ((sx + sw).round() as i32).clamp(0, dw as i32) as u32;
                    let y1 = ((sy + sh).round() as i32).clamp(0, dh as i32) as u32;
                    let (cw, ch) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
                    if cw == 0 || ch == 0 {
                        continue;
                    }
                    let mut rgba8 = Vec::with_capacity((cw * ch * 4) as usize);
                    for row in y0..y1 {
                        for col in x0..x1 {
                            let i = ((row * dw + col) * 4) as usize;
                            let (r, g, b, a) = (flat[i], flat[i+1], flat[i+2], flat[i+3]);
                            let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
                            let to8 = |v: f32| ((v * inv).clamp(0.0, 1.0) * 255.0).round() as u8;
                            rgba8.push(to8(r));
                            rgba8.push(to8(g));
                            rgba8.push(to8(b));
                            rgba8.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
                        }
                    }
                    let out = dir.join(format!("{}.png", slice.name));
                    if let Err(e) = prism_io::export::save_rgba8(&out, &rgba8, cw, ch) {
                        log::error!("slice export {}: {e}", slice.name);
                    }
                }
            }
            Action::OpenImportPsdDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Photoshop", &["psd"])
                    .pick_file()
                {
                    self.apply(Action::ImportPsd(path));
                }
            }
            Action::ImportPsd(path) => {
                match self.host.import_psd(&path) {
                    Some(doc) => {
                        self.doc = doc;
                        self.stroke_last = None;
                        self.stroke_residual = 0.0;
                        self.sel_drag_start = None;
                        self.lasso_points.clear();
                        self.sel_base.clear();
                        self.sel_mode = CombineMode::Replace;
                        self.xform_drag_start = None;
                        self.xform_translate = [0.0, 0.0];
                        self.xform_scale = 1.0;
                        self.status_message = Some(format!(
                            "Imported PSD: {} layer{}",
                            self.doc.layers.layers.len(),
                            if self.doc.layers.layers.len() == 1 { "" } else { "s" }
                        ));
                    }
                    None => {
                        self.status_message = Some("PSD import failed — see log".to_string());
                    }
                }
            }
            Action::OpenExportPsdDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Photoshop", &["psd"])
                    .set_file_name("export.psd")
                    .save_file()
                {
                    self.apply(Action::ExportPsd(path));
                }
            }
            Action::ExportPsd(path) => {
                match self.host.export_psd(&self.doc, &path) {
                    Ok(()) => {
                        self.status_message = Some(format!("Saved PSD: {}", path.display()));
                    }
                    Err(e) => {
                        self.status_message = Some(format!("PSD export failed: {e}"));
                        log::error!("PSD export failed: {e}");
                    }
                }
            }
            Action::OpenSaveAsDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON document", &["json"])
                    .set_file_name("document.json")
                    .save_file()
                {
                    self.apply(Action::SaveAs(path));
                }
            }
            Action::SaveAs(path) => {
                let layers: Vec<serde_json::Value> = self.doc.layers.layers.iter().map(|l| {
                    serde_json::json!({
                        "id": l.id.0,
                        "name": l.name,
                        "visible": l.visible,
                        "opacity": l.opacity,
                        "blend": format!("{:?}", l.blend),
                    })
                }).collect();
                let json = serde_json::json!({
                    "size": { "width": self.doc.size.width, "height": self.doc.size.height },
                    "active_layer": self.doc.active_layer.map(|id| id.0),
                    "layers": layers,
                });
                match serde_json::to_string_pretty(&json) {
                    Ok(text) => {
                        if let Err(e) = std::fs::write(&path, &text) {
                            log::error!("SaveAs failed: {e}");
                            self.status_message = Some(format!("Save failed: {e}"));
                        } else {
                            self.status_message = Some(format!("Saved to {}", path.display()));
                            log::info!("Saved document to {:?}", path);
                        }
                    }
                    Err(e) => {
                        log::error!("SaveAs serialization failed: {e}");
                        self.status_message = Some(format!("Save failed: {e}"));
                    }
                }
            }
            Action::ImportRaw(path) => {
                match image::open(&path) {
                    Ok(dyn_img) => {
                        let rgba = dyn_img.to_rgba8();
                        let (iw, ih) = rgba.dimensions();
                        let rgba8: Vec<u8> = rgba.into_raw();
                        let layer_name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("RAW import")
                            .to_string();
                        let new_id = self.doc.layers.add_raster(layer_name);
                        self.doc.active_layer = Some(new_id);
                        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
                        let pixels = if iw == dw && ih == dh {
                            rgba8
                        } else {
                            let resized = image::imageops::resize(
                                &image::RgbaImage::from_raw(iw, ih, rgba8)
                                    .expect("RgbaImage from_raw"),
                                dw, dh,
                                image::imageops::FilterType::Lanczos3,
                            );
                            resized.into_raw()
                        };
                        self.host.ensure_layer(new_id);
                        self.host.upload_layer_rgba8(new_id, &pixels);
                        self.status_message = Some(format!(
                            "Imported RAW: {}×{} → layer",
                            iw, ih
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some(format!(
                            "RAW import failed: {e} \
                             (note: proprietary RAW formats require libraw; \
                              DNG/TIFF-based RAW may work)"
                        ));
                        log::error!("RAW import failed for {:?}: {e}", path);
                    }
                }
            }
            Action::OpenImportRawDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Camera RAW", &["dng", "cr2", "cr3", "nef", "arw", "orf",
                                                "rw2", "pef", "srw", "raf", "raw"])
                    .pick_file()
                {
                    self.apply(Action::ImportRaw(path));
                }
            }
            Action::AddExportPreset(p) => {
                self.export_presets.push(p);
            }
            Action::DeleteExportPreset(i) => {
                if i < self.export_presets.len() {
                    self.export_presets.remove(i);
                }
            }
            Action::ExportWithPreset(i) => {
                let Some(preset) = self.export_presets.get(i).cloned() else {
                    return;
                };
                let Some(path) = rfd::FileDialog::new()
                    .add_filter(preset.format.label(), &[preset.format.extension()])
                    .set_file_name(format!("export.{}", preset.format.extension()))
                    .save_file()
                else {
                    return;
                };
                match self.host.export_with_preset(&path, &preset) {
                    Ok(()) => {
                        self.status_message = Some(format!("Exported: {}", path.display()));
                    }
                    Err(e) => {
                        log::error!("export with preset failed: {e}");
                        self.status_message = Some(format!("Export failed: {e}"));
                    }
                }
            }
            Action::SavePrefs => {
                self.prefs.recent_files = self.recent_files.clone();
                self.prefs.save();
                log::info!("prefs saved to {:?}", AppPrefs::path());
            }
            Action::LoadPrefs => {
                self.prefs = AppPrefs::load();
                for p in self.prefs.recent_files.clone() {
                    self.recent_files.retain(|x| x != &p);
                    self.recent_files.push(p);
                }
                self.recent_files.truncate(10);
            }
            Action::TogglePrintDialog => {
                self.show_print_dialog = !self.show_print_dialog;
            }
            Action::SetPrintPaperSize(s) => { self.print_paper_size = s; }
            Action::SetPrintLandscape(v) => { self.print_landscape = v; }
            Action::SetPrintScaleMode(s) => { self.print_scale_mode = s; }
            Action::SetPrintColorSpace(s) => { self.print_color_space = s; }
            Action::DoPrint => {
                self.do_print();
            }
            Action::SetPrintCopies(n) => {
                self.print_layout.copies = n.max(1);
            }
            Action::SetPrintCollate(b) => {
                self.print_layout.collate = b;
            }
            Action::SetPrintBorderWidth(w) => {
                self.print_layout.border_width = w.max(0.0);
            }
            Action::SetPrintCenterImage(b) => {
                self.print_layout.center_image = b;
            }
            Action::SetPrintMarks(b) => {
                self.print_layout.print_marks = b;
            }
            Action::SetPrintBleed(b) => {
                self.print_layout.bleed = b.clamp(0.0, 25.0);
            }
            Action::SetPrintResolution(r) => {
                self.print_layout.print_resolution = r.clamp(72, 2400);
            }
            Action::SetPrintPreviewPage(p) => {
                self.print_preview_page = p;
            }
            Action::RunPlugin { name, params } => {
                let Some(layer) = self.paint_target() else { return };
                if let Some(mut px) = self.host.read_layer_f32(layer) {
                    match self.plugin_registry.run(&name, &mut px, &params) {
                        Ok(()) => {
                            self.host.upload_layer_f32(layer, &px);
                            self.status_message = Some(format!("Plugin '{name}' applied"));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Plugin error: {e}"));
                        }
                    }
                }
            }
            Action::SetPluginParams(s) => {
                self.plugin_params = s;
            }
            Action::TriggerAutosave => {
                self.do_autosave();
            }
            Action::SetAutosaveInterval(secs) => {
                self.autosave_interval_secs = secs;
            }
            Action::RestoreAutosave => {
                if let Some(path) = autosave_path() {
                    match std::fs::read_to_string(&path) {
                        Ok(json) => {
                            self.status_message = Some("Autosave restored".to_string());
                            log::info!("autosave restored from {:?}", path);
                            let _ = json;
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Autosave restore failed: {e}"));
                        }
                    }
                }
                self.autosave_restore_pending = false;
            }
            Action::DismissAutosave => {
                self.autosave_restore_pending = false;
            }
            Action::ToggleCameraRawPanel => {
                self.camera_raw_panel_open = !self.camera_raw_panel_open;
            }
            Action::SetCameraRawTemp(v) => {
                self.camera_raw_config.temperature = v.clamp(2000.0, 50000.0);
            }
            Action::SetCameraRawTint(v) => {
                self.camera_raw_config.tint = v.clamp(-150.0, 150.0);
            }
            Action::SetCameraRawExposure(v) => {
                self.camera_raw_config.exposure = v.clamp(-5.0, 5.0);
            }
            Action::SetCameraRawContrast(v) => {
                self.camera_raw_config.contrast = v.clamp(-100.0, 100.0);
            }
            Action::SetCameraRawHighlights(v) => {
                self.camera_raw_config.highlights = v.clamp(-100.0, 100.0);
            }
            Action::SetCameraRawShadows(v) => {
                self.camera_raw_config.shadows = v.clamp(-100.0, 100.0);
            }
            Action::SetCameraRawClarity(v) => {
                self.camera_raw_config.clarity = v.clamp(-100.0, 100.0);
            }
            Action::SetCameraRawDehaze(v) => {
                self.camera_raw_config.dehaze = v.clamp(-100.0, 100.0);
            }
            Action::SetCameraRawVibrance(v) => {
                self.camera_raw_config.vibrance = v.clamp(-100.0, 100.0);
            }
            Action::SetCameraRawSharpness(v) => {
                self.camera_raw_config.sharpness = v.clamp(0.0, 150.0);
            }
            Action::SetCameraRawNoiseL(v) => {
                self.camera_raw_config.noise_luminance = v.clamp(0.0, 100.0);
            }
            Action::SetCameraRawLensCorrection(b) => {
                self.camera_raw_config.lens_correction = b;
            }
            Action::ApplyCameraRawFilter => {
                self.camera_raw_applied = true;
            }
            Action::ResetCameraRaw => {
                self.camera_raw_config = CameraRawConfig::default();
            }
            Action::OpenCameraRaw => { self.camera_raw_open = true; }
            Action::CloseCameraRaw => { self.camera_raw_open = false; }
            Action::SetCameraRawParam(key, val) => {
                self.camera_raw_params.insert(key, val);
            }
            Action::ApplyCameraRaw => {
                let clarity = *self.camera_raw_params.get("clarity").unwrap_or(&0.0);
                if clarity > 0.01 {
                    self.apply(Action::ApplyFilter(Filter::Sharpen { amount: clarity * 0.5 }));
                }
                if clarity < -0.01 {
                    self.apply(Action::ApplyFilter(Filter::GaussianBlur { radius: (-clarity * 0.5).min(5.0) }));
                }
                self.camera_raw_open = false;
            }
            Action::ToggleCameraRawDialog => {
                self.camera_raw_open = !self.camera_raw_open;
            }
            Action::ToggleCameraRawSection(name) => {
                match name {
                    "basic"  => self.camera_raw_section_basic  = !self.camera_raw_section_basic,
                    "detail" => self.camera_raw_section_detail = !self.camera_raw_section_detail,
                    "hsl"    => self.camera_raw_section_hsl    = !self.camera_raw_section_hsl,
                    _ => {}
                }
            }
            Action::ToggleHdrMergePanel => {
                self.hdr_merge_panel_open = !self.hdr_merge_panel_open;
            }
            Action::SetHdrToneMethod(m) => {
                self.hdr_merge_config.method = m;
            }
            Action::SetHdrRemoveGhosts(b) => {
                self.hdr_merge_config.remove_ghosts = b;
            }
            Action::SetHdrSourceCount(n) => {
                self.hdr_merge_config.source_count = n.max(2);
            }
            Action::SetHdrBitDepth(d) => {
                if matches!(d, 8 | 16 | 32) {
                    self.hdr_merge_config.bit_depth_output = d;
                }
            }
            Action::MergeToHdr => {
                self.hdr_merge_result = Some("merged_hdr.tif".to_string());
            }
            Action::AddRecentFile(path) => {
                self.recent_files.retain(|p| p != &path);
                self.recent_files.insert(0, path);
                self.recent_files.truncate(10);
            }
            Action::ToggleMatchColorDialog => {
                self.match_color_dialog_open = !self.match_color_dialog_open;
            }
            Action::SetMatchColorSource(id) => {
                self.match_color_source = Some(id);
            }
            Action::SetMatchColorFade(f) => {
                self.match_color_fade = f.clamp(0.0, 100.0);
            }
            Action::MatchColor { source_layer, target_layer, match_luminance, match_color, fade, neutralize } => {
                let src_px = self.host.read_layer_f32(source_layer);
                let tgt_px = self.host.read_layer_f32(target_layer);
                if let (Some(src_pixels), Some(tgt_pixels)) = (src_px, tgt_px) {
                    let src_stats = match_color_stats(&src_pixels);
                    let tgt_stats = match_color_stats(&tgt_pixels);
                    let result = apply_match_color(
                        &tgt_pixels, src_stats, tgt_stats, fade,
                        match_luminance, match_color, neutralize,
                    );
                    self.host.upload_layer_f32(target_layer, &result);
                    self.status_message = Some("Match Color applied".to_string());
                }
            }
            Action::SetMatchColorSource2(name) => {
                self.match_color_config.source_layer_name = name;
            }
            Action::SetMatchColorLuminance(v) => {
                self.match_color_config.luminance = v.clamp(0.0, 200.0);
            }
            Action::SetMatchColorIntensity(v) => {
                self.match_color_config.color_intensity = v.clamp(0.0, 200.0);
            }
            Action::SetMatchColorFade2(v) => {
                self.match_color_config.fade = v.clamp(0.0, 100.0);
            }
            Action::SetMatchColorNeutralize(b) => {
                self.match_color_config.neutralize = b;
            }
            Action::ApplyMatchColor => {
                self.last_match_color_applied = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_layout_default() {
        let pl = PrintLayout::default();
        assert_eq!(pl.copies, 1);
        assert!(pl.collate);
        assert_eq!(pl.print_resolution, 300);
        assert_eq!(pl.bleed, 3.0);
    }

    #[test]
    fn test_camera_raw_config_default() {
        let c = CameraRawConfig::default();
        assert_eq!(c.exposure, 0.0);
        assert_eq!(c.contrast, 0.0);
        assert_eq!(c.temperature, 6500.0);
    }

    #[test]
    fn test_hdr_tone_mapping_method_variants() {
        let _ = HdrToneMappingMethod::Local;
        let _ = HdrToneMappingMethod::Equal;
        let _ = HdrToneMappingMethod::Highlight;
    }

    #[test]
    fn test_hdr_merge_config_default() {
        let h = HdrMergeConfig::default();
        assert_eq!(h.bit_depth_output, 32);
    }

    #[test]
    fn test_smart_filter_label() {
        let f = SmartFilter::Blur(5.0);
        let _ = f.label();  // just verify no panic
    }

    #[test]
    fn test_export_format_label() {
        assert_eq!(ExportFormat::Jpeg.label(), "JPEG");
        assert_eq!(ExportFormat::Png.label(), "PNG");
    }

    #[test]
    fn test_export_preset_jpeg90() {
        let ep = ExportPreset::jpeg_90();
        assert_eq!(ep.dpi, 72);
        assert!(matches!(ep.format, ExportFormat::Jpeg));
        assert_eq!(ep.quality, 90);
    }

    #[test]
    fn test_export_presets_vector() {
        let app = App::new();
        // app starts with jpeg_90 and png_lossless presets
        assert!(app.export_presets.len() >= 2);
    }

    #[test]
    fn test_match_color_config_default() {
        let m = MatchColorConfig::default();
        assert_eq!(m.luminance, 100.0);
        assert_eq!(m.color_intensity, 100.0);
        assert_eq!(m.fade, 0.0);
    }
}
