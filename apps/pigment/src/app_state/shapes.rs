// apps/pigment/src/app_state/shapes.rs
use super::{App, Action};

/// Extended shape primitives beyond Rectangle/Ellipse (which live in prism_core).
#[derive(Clone, Debug, PartialEq)]
pub enum ExtendedShapeKind {
    Polygon { sides: u32, radius: f32, corner_radius: f32 },
    Star { points: u32, inner_radius: f32, outer_radius: f32, rotation: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32 },
    RoundedRect { width: f32, height: f32, corner_radius: f32 },
    Triangle { base: f32, height: f32, rotation: f32 },
    Arrow { length: f32, head_width: f32, head_length: f32, shaft_width: f32, direction: f32 },
    SpeechBubble { width: f32, height: f32, tail_x: f32, tail_y: f32, tail_width: f32, corner_radius: f32 },
    BooleanResult { source_ids: Vec<usize>, op: BooleanOp },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap { Butt, Round, Square }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin { Miter, Round, Bevel }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp { Unite, Subtract, Intersect, Exclude }

/// Per-shape-layer state stored in App::extended_shapes keyed by layer_id (usize).
#[derive(Clone, Debug)]
pub struct ExtendedShapeLayer {
    pub shape: ExtendedShapeKind,
    pub fill_color: [u8; 4],
    pub stroke_color: [u8; 4],
    pub stroke_width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    /// If true, this layer was hidden because it was consumed by a BooleanResult.
    pub hidden_by_boolean: bool,
}

impl Default for ExtendedShapeLayer {
    fn default() -> Self {
        Self {
            shape: ExtendedShapeKind::RoundedRect { width: 100.0, height: 60.0, corner_radius: 8.0 },
            fill_color: [80, 120, 200, 255],
            stroke_color: [0, 0, 0, 255],
            stroke_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            hidden_by_boolean: false,
        }
    }
}

// ---- PSD export config --------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PsdEncoding { Raw, Rle }

#[derive(Clone, Debug)]
pub struct PsdExportConfig {
    pub path: String,
    pub maximize_compatibility: bool,
    pub embed_color_profile: bool,
    pub encoding: PsdEncoding,
    pub merge_alpha: bool,
    pub include_metadata: bool,
    pub resolution: f32,
}

impl Default for PsdExportConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            maximize_compatibility: true,
            embed_color_profile: true,
            encoding: PsdEncoding::Rle,
            merge_alpha: false,
            include_metadata: true,
            resolution: 72.0,
        }
    }
}

// ---- Extended layer style types -----------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ContourType { #[default] Linear, Gaussian, SineWave, Ring, RingDouble, Step, Rounded }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BevelDirection { #[default] Up, Down }

#[derive(Clone, Debug)]
pub struct SatinEffect {
    pub blend_mode: String,
    pub color: [u8; 4],
    pub opacity: u8,
    pub angle: f32,
    pub distance: u32,
    pub size: u32,
    pub contour: ContourType,
    pub anti_aliased: bool,
    pub invert: bool,
}

impl Default for SatinEffect {
    fn default() -> Self {
        Self {
            blend_mode: "Multiply".to_string(),
            color: [0, 0, 0, 255],
            opacity: 50,
            angle: 19.0,
            distance: 11,
            size: 14,
            contour: ContourType::Linear,
            anti_aliased: false,
            invert: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ColorOverlay {
    pub blend_mode: String,
    pub color: [u8; 4],
    pub opacity: u8,
}

impl Default for ColorOverlay {
    fn default() -> Self {
        Self {
            blend_mode: "Normal".to_string(),
            color: [255, 0, 0, 255],
            opacity: 100,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GradientOverlayStyle { #[default] Linear, Radial, Angle, Reflected, Diamond }

#[derive(Clone, Debug)]
pub struct GradientOverlay {
    pub blend_mode: String,
    pub opacity: u8,
    pub gradient_id: Option<usize>,
    pub style: GradientOverlayStyle,
    pub align_with_layer: bool,
    pub angle: f32,
    pub scale: u32,
    pub reverse: bool,
    pub dither: bool,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for GradientOverlay {
    fn default() -> Self {
        Self {
            blend_mode: "Normal".to_string(),
            opacity: 100,
            gradient_id: None,
            style: GradientOverlayStyle::Linear,
            align_with_layer: true,
            angle: 90.0,
            scale: 100,
            reverse: false,
            dither: false,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PatternOverlay {
    pub blend_mode: String,
    pub opacity: u8,
    pub pattern_id: Option<usize>,
    pub scale: u32,
    pub link_with_layer: bool,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for PatternOverlay {
    fn default() -> Self {
        Self {
            blend_mode: "Normal".to_string(),
            opacity: 100,
            pattern_id: None,
            scale: 100,
            link_with_layer: true,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    DropShadow, InnerShadow, OuterGlow, InnerGlow,
    BevelEmboss, Satin, ColorOverlay, GradientOverlay, PatternOverlay, Stroke,
}

// ---- App apply helpers --------------------------------------------------

impl App {
    /// Apply shape-related actions.
    pub(super) fn apply_shapes(&mut self, action: Action) {
        match action {
            Action::AddPolygonLayer { sides, radius, corner_radius, color } => {
                let id = self.doc.layers.add_raster("Polygon");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                let raw_id = id.0 as usize;
                self.extended_shapes.insert(raw_id, ExtendedShapeLayer {
                    shape: ExtendedShapeKind::Polygon { sides: sides.max(3), radius, corner_radius: corner_radius.max(0.0) },
                    fill_color: color,
                    ..Default::default()
                });
                self.sync_host_order_dirty();
            }
            Action::AddStarLayer { points, inner_radius, outer_radius, color } => {
                let id = self.doc.layers.add_raster("Star");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                let raw_id = id.0 as usize;
                self.extended_shapes.insert(raw_id, ExtendedShapeLayer {
                    shape: ExtendedShapeKind::Star { points: points.max(3), inner_radius, outer_radius, rotation: 0.0 },
                    fill_color: color,
                    ..Default::default()
                });
                self.sync_host_order_dirty();
            }
            Action::AddLineLayer { x1, y1, x2, y2, width, color } => {
                let id = self.doc.layers.add_raster("Line");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                let raw_id = id.0 as usize;
                self.extended_shapes.insert(raw_id, ExtendedShapeLayer {
                    shape: ExtendedShapeKind::Line { x1, y1, x2, y2, width: width.max(0.1) },
                    fill_color: color,
                    ..Default::default()
                });
                self.sync_host_order_dirty();
            }
            Action::AddRoundedRectLayer { width, height, corner_radius, color } => {
                let id = self.doc.layers.add_raster("Rounded Rectangle");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                let raw_id = id.0 as usize;
                self.extended_shapes.insert(raw_id, ExtendedShapeLayer {
                    shape: ExtendedShapeKind::RoundedRect { width, height, corner_radius: corner_radius.max(0.0) },
                    fill_color: color,
                    ..Default::default()
                });
                self.sync_host_order_dirty();
            }
            Action::AddTriangleLayer { base, height, rotation, color } => {
                let id = self.doc.layers.add_raster("Triangle");
                self.host.ensure_layer(id);
                self.doc.active_layer = Some(id);
                let raw_id = id.0 as usize;
                self.extended_shapes.insert(raw_id, ExtendedShapeLayer {
                    shape: ExtendedShapeKind::Triangle { base, height, rotation },
                    fill_color: color,
                    ..Default::default()
                });
                self.sync_host_order_dirty();
            }
            Action::SetShapeSides { layer_id, sides } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    if let ExtendedShapeKind::Polygon { sides: ref mut s_sides, .. } = s.shape {
                        *s_sides = sides.max(3);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetShapeCornerRadius { layer_id, radius } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    match &mut s.shape {
                        ExtendedShapeKind::Polygon { corner_radius, .. } => *corner_radius = radius.max(0.0),
                        ExtendedShapeKind::RoundedRect { corner_radius, .. } => *corner_radius = radius.max(0.0),
                        _ => {}
                    }
                    self.host.mark_dirty();
                }
            }
            Action::SetStarPoints { layer_id, points } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    if let ExtendedShapeKind::Star { points: ref mut pts, .. } = s.shape {
                        *pts = points.max(3);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetStarInnerRadius { layer_id, inner_radius } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    if let ExtendedShapeKind::Star { inner_radius: ref mut ir, .. } = s.shape {
                        *ir = inner_radius.max(0.0);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetLineWidth { layer_id, width } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    if let ExtendedShapeKind::Line { width: ref mut w, .. } = s.shape {
                        *w = width.max(0.1);
                        self.host.mark_dirty();
                    }
                }
            }
            Action::SetLineCap { layer_id, cap } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    s.line_cap = cap;
                    self.host.mark_dirty();
                }
            }
            Action::SetLineJoin { layer_id, join } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    s.line_join = join;
                    self.host.mark_dirty();
                }
            }
            Action::SetShapeStroke { layer_id, color, width } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    s.stroke_color = color;
                    s.stroke_width = width.max(0.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetShapeFill { layer_id, color } => {
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    s.fill_color = color;
                    self.host.mark_dirty();
                }
            }
            // Boolean shape ops
            Action::BooleanShapeOp { layer_ids, op } => {
                self.apply_boolean_op(&layer_ids.clone(), op);
            }
            Action::ExpandStroke { layer_id } => {
                // State model: record that the stroke was expanded (actual math at render time)
                if let Some(s) = self.extended_shapes.get_mut(&layer_id) {
                    let old_stroke = s.stroke_width;
                    s.stroke_width = 0.0; // stroke consumed into fill
                    // Record stroke expansion as a no-op fill change (state only)
                    let _ = old_stroke;
                    self.host.mark_dirty();
                }
            }
            Action::FlattenToPixels { layer_ids } => {
                // State model: remove extended_shape entries for the listed ids
                for id in &layer_ids {
                    self.extended_shapes.remove(id);
                }
                self.host.mark_dirty();
            }
            // Clipping masks (extended)
            Action::SetClippingMask { layer_id, clipped } => {
                if clipped {
                    self.clipping_masks.insert(layer_id);
                } else {
                    self.clipping_masks.remove(&layer_id);
                }
                self.host.mark_dirty();
            }
            Action::CreateClippingMask { layer_id } => {
                self.clipping_masks.insert(layer_id);
                self.host.mark_dirty();
            }
            Action::ReleaseClippingMask { layer_id } => {
                self.clipping_masks.remove(&layer_id);
                self.host.mark_dirty();
            }
            // Extended layer styles
            Action::SetSatinEffect { layer_id, config } => {
                let entry = self.satin_effects.entry(layer_id).or_default();
                *entry = config;
                self.host.mark_dirty();
            }
            Action::SetExtendedColorOverlay { layer_id, config } => {
                let entry = self.color_overlays.entry(layer_id).or_default();
                *entry = config;
                self.host.mark_dirty();
            }
            Action::SetGradientOverlay { layer_id, config } => {
                let entry = self.gradient_overlays.entry(layer_id).or_default();
                *entry = config;
                self.host.mark_dirty();
            }
            Action::SetPatternOverlay { layer_id, config } => {
                let entry = self.pattern_overlays.entry(layer_id).or_default();
                *entry = config;
                self.host.mark_dirty();
            }
            Action::SetLayerStyleBlendMode { layer_id, style, blend_mode } => {
                match style {
                    StyleKind::Satin => {
                        self.satin_effects.entry(layer_id).or_default().blend_mode = blend_mode;
                    }
                    StyleKind::ColorOverlay => {
                        self.color_overlays.entry(layer_id).or_default().blend_mode = blend_mode;
                    }
                    StyleKind::GradientOverlay => {
                        self.gradient_overlays.entry(layer_id).or_default().blend_mode = blend_mode;
                    }
                    StyleKind::PatternOverlay => {
                        self.pattern_overlays.entry(layer_id).or_default().blend_mode = blend_mode;
                    }
                    _ => {}
                }
                self.host.mark_dirty();
            }
            Action::SetLayerStyleOpacity { layer_id, style, opacity } => {
                let clamped = opacity.clamp(0, 100);
                match style {
                    StyleKind::Satin => {
                        self.satin_effects.entry(layer_id).or_default().opacity = clamped;
                    }
                    StyleKind::ColorOverlay => {
                        self.color_overlays.entry(layer_id).or_default().opacity = clamped;
                    }
                    StyleKind::GradientOverlay => {
                        self.gradient_overlays.entry(layer_id).or_default().opacity = clamped;
                    }
                    StyleKind::PatternOverlay => {
                        self.pattern_overlays.entry(layer_id).or_default().opacity = clamped;
                    }
                    _ => {}
                }
                self.host.mark_dirty();
            }
            Action::CopyLayerStylesExt { from_layer_id } => {
                let satin = self.satin_effects.get(&from_layer_id).cloned();
                let color_ov = self.color_overlays.get(&from_layer_id).cloned();
                let grad_ov = self.gradient_overlays.get(&from_layer_id).cloned();
                let pat_ov = self.pattern_overlays.get(&from_layer_id).cloned();
                self.style_clipboard_satin = satin;
                self.style_clipboard_color_overlay = color_ov;
                self.style_clipboard_gradient_overlay = grad_ov;
                self.style_clipboard_pattern_overlay = pat_ov;
            }
            Action::PasteLayerStylesExt { to_layer_ids } => {
                for id in to_layer_ids {
                    if let Some(s) = &self.style_clipboard_satin {
                        self.satin_effects.insert(id, s.clone());
                    }
                    if let Some(c) = &self.style_clipboard_color_overlay {
                        self.color_overlays.insert(id, c.clone());
                    }
                    if let Some(g) = &self.style_clipboard_gradient_overlay {
                        self.gradient_overlays.insert(id, g.clone());
                    }
                    if let Some(p) = &self.style_clipboard_pattern_overlay {
                        self.pattern_overlays.insert(id, p.clone());
                    }
                }
                self.host.mark_dirty();
            }
            Action::ClearLayerStylesExt { layer_id } => {
                self.satin_effects.remove(&layer_id);
                self.color_overlays.remove(&layer_id);
                self.gradient_overlays.remove(&layer_id);
                self.pattern_overlays.remove(&layer_id);
                self.host.mark_dirty();
            }
            // PSD export config
            Action::SetPsdExportPath(path) => {
                self.psd_export_config.path = path;
            }
            Action::SetPsdMaximizeCompatibility(v) => {
                self.psd_export_config.maximize_compatibility = v;
            }
            Action::SetPsdEncoding(enc) => {
                self.psd_export_config.encoding = enc;
            }
            Action::SetPsdEmbedColorProfile(v) => {
                self.psd_export_config.embed_color_profile = v;
            }
            Action::ExportAsPsd { path } => {
                self.psd_export_config.path = path;
                // Stub: record last export
                self.last_psd_export_path = Some(self.psd_export_config.path.clone());
            }
            _ => {}
        }
    }

    /// Apply boolean shape operation: hide source layers, create a new merged layer.
    pub fn apply_boolean_op(&mut self, layer_ids: &[usize], op: BooleanOp) {
        if layer_ids.len() < 2 {
            return;
        }
        // Hide source layers (don't delete, for undo)
        for &raw_id in layer_ids {
            if let Some(s) = self.extended_shapes.get_mut(&raw_id) {
                s.hidden_by_boolean = true;
            }
        }
        // Create a new result layer
        let result_id = self.doc.layers.add_raster("Boolean Shape");
        self.host.ensure_layer(result_id);
        self.doc.active_layer = Some(result_id);
        let raw_result = result_id.0 as usize;
        self.extended_shapes.insert(raw_result, ExtendedShapeLayer {
            shape: ExtendedShapeKind::BooleanResult {
                source_ids: layer_ids.to_vec(),
                op,
            },
            fill_color: [80, 120, 200, 255],
            ..Default::default()
        });
        self.sync_host_order_dirty();
    }

    /// Returns all layer ids that form a clipping group above `layer_id`.
    /// The base is the first non-clipped layer at or below `layer_id`.
    pub fn clipping_group_for_layer(&self, layer_id: prism_core::LayerId) -> Vec<prism_core::LayerId> {
        let layers = &self.doc.layers.layers;
        // Find the position of layer_id
        let Some(pos) = layers.iter().position(|l| l.id == layer_id) else {
            return Vec::new();
        };
        // Collect contiguous clipped layers above
        let mut group = Vec::new();
        let mut i = pos + 1;
        while i < layers.len() {
            let lid = layers[i].id;
            if self.clipping_masks.contains(&lid) {
                group.push(lid);
                i += 1;
            } else {
                break;
            }
        }
        group
    }
}
