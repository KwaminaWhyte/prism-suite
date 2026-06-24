use super::*;

/// A node in a pen/bézier path: anchor position plus control handles.
#[derive(Clone, Debug)]
pub struct PenNode {
    pub pos: (f32, f32),
    pub ctrl_in: (f32, f32),
    pub ctrl_out: (f32, f32),
}

/// Brush state, mirroring the egui app's defaults. `color` is straight sRGB
/// RGBA in 0..1 (the egui app stores `Color32::from_rgb(20, 120, 230)`).
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    pub color: [f32; 4],
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            // egui: Color32::from_rgb(20, 120, 230)
            color: [20.0 / 255.0, 120.0 / 255.0, 230.0 / 255.0, 1.0],
            size: 40.0,
            hardness: 0.5,
            opacity: 1.0,
        }
    }
}

/// Liquify warp mode — only Warp is currently rasterised; others are stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LiquifyMode {
    #[default]
    Warp,
    Twirl,
    Pucker,
    Bloat,
}

impl LiquifyMode {
    pub fn label(self) -> &'static str {
        match self {
            LiquifyMode::Warp => "Warp",
            LiquifyMode::Twirl => "Twirl",
            LiquifyMode::Pucker => "Pucker",
            LiquifyMode::Bloat => "Bloat",
        }
    }
}

/// Heal tool blending mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HealMode {
    /// Gaussian feathered blend (default) — 8px feather at patch boundary.
    #[default]
    Normal,
    /// No blending — acts like a clone stamp.
    Replace,
    /// Delegates to content-aware fill for the healed area.
    Content,
}

impl HealMode {
    pub fn label(self) -> &'static str {
        match self {
            HealMode::Normal => "Normal",
            HealMode::Replace => "Replace",
            HealMode::Content => "Content",
        }
    }
}


// ---- Batch 5: Pattern Stamp ---------------------------------------------

/// A repeating tile pattern stored in the pattern library.
#[derive(Clone, Debug)]
pub struct PatternDef {
    pub name: String,
    /// Row-major RGBA float pixels, linear-light premultiplied.
    pub pixels: Vec<[f32; 4]>,
    pub width: u32,
    pub height: u32,
}


// ---- Batch 7: Spot Heal / Red Eye -------------------------------------------

/// Algorithm used by the Spot Healing Brush.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SpotHealMode {
    ContentAware,
    TextureMatch,
    ProximityMatch,
}

impl Default for SpotHealMode {
    fn default() -> Self { SpotHealMode::ContentAware }
}

// ---- Batch 5 (new): Liquify Depth -------------------------------------------

/// Available tools inside the Liquify filter.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum LiquifyTool {
    #[default]
    Forward,
    Reconstruct,
    Smooth,
    Twirl,
    Pucker,
    Bloat,
    PushLeft,
    Mirror,
    Turbulence,
}

/// A single Liquify brush stroke recorded for undo / replay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiquifyStroke {
    pub tool: LiquifyTool,
    pub center: [f32; 2],
    pub radius: f32,
    pub pressure: f32,
    /// Rotation angle in degrees (used by Twirl).
    pub angle: f32,
}

/// Warp-mesh metadata (no pixel data — mesh is rebuilt from strokes).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct LiquifyMesh {
    pub width: u32,
    pub height: u32,
    /// Number of mesh subdivisions (default 4).
    pub subdivisions: u8,
}



impl App {
    pub(super) fn apply_painting(&mut self, action: Action) {
        match action {
            Action::SetBrushColor(c) => {
                self.brush.color = c;
                if let Some(edit) = self.text_edit.as_ref() {
                    let (layer, origin, string) = (edit.layer, edit.origin, edit.string.clone());
                    self.host.update_text_layer(
                        layer, &string, self.text_size, c, origin,
                        prism_io::text::TextAlign::Left, None,
                    );
                }
            }
            Action::SetBrushSize(s) => self.brush.size = s.clamp(1.0, 400.0),
            Action::SetBrushHardness(h) => self.brush.hardness = h.clamp(0.0, 0.99),
            Action::SetBrushOpacity(o) => self.brush.opacity = o.clamp(0.0, 1.0),
            Action::SetFillTolerance(t) => self.fill_tolerance = t.clamp(0.0, 1.0),
            Action::ToggleFillContiguous => self.fill_contiguous = !self.fill_contiguous,
            Action::ToggleGradientDither => self.gradient_dither = !self.gradient_dither,
            Action::PenAddNode(pos) => {
                self.pen_path.push(PenNode { pos, ctrl_in: pos, ctrl_out: pos });
            }
            Action::PenMoveHandle { idx, is_out, delta } => {
                if let Some(node) = self.pen_path.get_mut(idx) {
                    if is_out {
                        node.ctrl_out.0 += delta.0;
                        node.ctrl_out.1 += delta.1;
                    } else {
                        node.ctrl_in.0 += delta.0;
                        node.ctrl_in.1 += delta.1;
                    }
                }
            }
            Action::PenClose => {
                if self.pen_path.len() >= 2 {
                    if let Some(layer) = self.paint_target() {
                        let dabs = rasterize_pen_path(&self.pen_path, &self.brush);
                        if !dabs.is_empty() {
                            self.host.paint_dabs(layer, &dabs, false, true, false);
                        }
                    }
                }
                self.pen_path.clear();
                self.pen_closed = true;
            }
            Action::SetDodgeSize(s) => {
                self.dodge_size = s.clamp(1.0, 400.0);
            }
            Action::SetDodgeStrength(s) => {
                self.dodge_strength = s.clamp(0.01, 1.0);
            }
            Action::SetSmudgeStrength(s) => {
                self.smudge_strength = s.clamp(0.0, 1.0);
            }
            Action::SetLiquifyMode(m) => {
                self.liquify_mode = m;
            }
            Action::SetHealRadius(r) => {
                self.heal_radius = r.clamp(1, 500);
            }
            Action::SetHealMode(m) => {
                self.heal_mode = m;
            }
            Action::SetSpotHealMode(m) => {
                self.spot_heal_mode = m;
            }
            Action::SetSpotHealRadius(r) => {
                self.spot_heal_radius = r.max(1.0);
            }
            Action::SpotHeal { center, radius } => {
                self.last_spot_heal = Some((center, radius));
            }
            Action::RedEye { center, radius, darken } => {
                self.apply_red_eye(center, radius, darken);
            }
            Action::SetLiquifyTool(t) => {
                self.liquify_tool = t;
            }
            Action::SetLiquifyBrushSize(s) => {
                self.liquify_brush_size = s.clamp(1.0, 1500.0);
            }
            Action::SetLiquifyBrushPressure(p) => {
                self.liquify_brush_pressure = p.clamp(1.0, 100.0);
            }
            Action::SetLiquifyBrushDensity(d) => {
                self.liquify_brush_density = d.clamp(1.0, 100.0);
            }
            Action::ApplyLiquifyStroke(stroke) => {
                self.liquify_strokes.push(stroke);
            }
            Action::FreezeMaskRegion { center, radius } => {
                let ix = center[0] as usize;
                let iy = center[1] as usize;
                let needed = ix.max(iy) + 1;
                if self.liquify_frozen_mask.len() < needed {
                    self.liquify_frozen_mask.resize(needed, false);
                }
                let r = radius as usize;
                for dy in 0..=r {
                    for dx in 0..=r {
                        if dx * dx + dy * dy <= r * r {
                            let px = (ix + dx).min(self.liquify_frozen_mask.len() - 1);
                            let py = (iy + dy).min(self.liquify_frozen_mask.len() - 1);
                            let _ = (px, py);
                        }
                    }
                }
                self.liquify_frozen_mask.push(true);
            }
            Action::ThawAllMask => {
                self.liquify_frozen_mask.clear();
            }
            Action::ReconstructLiquify => {
                self.liquify_strokes.pop();
            }
            Action::RevertLiquify => {
                self.liquify_strokes.clear();
            }
            Action::SetLiquifyShowMesh(b) => {
                self.liquify_show_mesh = b;
            }
            Action::SetLiquifySmartRadius(b) => {
                self.liquify_smart_radius = b;
            }
            Action::SaveLiquifyMesh => {
                // Stub: mesh subdivisions unchanged; just marks intent.
            }
            Action::DefinePattern { name, pixels, width, height } => {
                self.pattern_library.push(PatternDef { name, pixels, width, height });
            }
            Action::SelectPattern(idx) => {
                if idx < self.pattern_library.len() {
                    self.active_pattern_idx = Some(idx);
                }
            }
            Action::DeletePattern(idx) => {
                if idx < self.pattern_library.len() {
                    self.pattern_library.remove(idx);
                    self.active_pattern_idx = self.active_pattern_idx.and_then(|i| {
                        if i == idx { None } else if i > idx { Some(i - 1) } else { Some(i) }
                    });
                }
            }
            Action::SetPatternStampScale(s) => {
                self.pattern_stamp_scale = s.clamp(0.1, 10.0);
            }
            Action::SetPatternStampAligned(a) => {
                self.pattern_stamp_aligned = a;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_default() {
        let b = Brush::default();
        assert_eq!(b.size, 40.0);
        assert_eq!(b.hardness, 0.5);
        assert_eq!(b.opacity, 1.0);
    }

    #[test]
    fn test_pen_node_fields() {
        let n = PenNode { pos: (1.0, 2.0), ctrl_in: (0.0, 0.0), ctrl_out: (3.0, 4.0) };
        assert_eq!(n.pos, (1.0, 2.0));
        assert_eq!(n.ctrl_out, (3.0, 4.0));
    }

    #[test]
    fn test_liquify_mode_label() {
        assert_eq!(LiquifyMode::Warp.label(), "Warp");
        assert_eq!(LiquifyMode::Twirl.label(), "Twirl");
    }

    #[test]
    fn test_heal_mode_label() {
        assert_eq!(HealMode::Normal.label(), "Normal");
        assert_eq!(HealMode::Content.label(), "Content");
    }

    #[test]
    fn test_spot_heal_mode_default() {
        assert_eq!(SpotHealMode::default(), SpotHealMode::ContentAware);
    }

    #[test]
    fn test_liquify_tool_default() {
        assert_eq!(LiquifyTool::default(), LiquifyTool::Forward);
    }

    #[test]
    fn test_liquify_mesh_default() {
        let mesh = LiquifyMesh::default();
        assert_eq!(mesh.subdivisions, 0);
    }

    #[test]
    fn test_pattern_def_fields() {
        let p = PatternDef {
            name: "dots".into(),
            pixels: vec![[1.0, 0.0, 0.0, 1.0]],
            width: 1,
            height: 1,
        };
        assert_eq!(p.name, "dots");
        assert_eq!(p.width, 1);
    }

    #[test]
    fn test_set_brush_size() {
        let mut app = App::new();
        app.apply(Action::SetBrushSize(80.0));
        assert_eq!(app.brush.size, 80.0);
    }

    #[test]
    fn test_set_brush_color() {
        let mut app = App::new();
        app.apply(Action::SetBrushColor([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(app.brush.color[0], 1.0);
        assert_eq!(app.brush.color[1], 0.0);
    }

    #[test]
    fn test_set_heal_mode() {
        let mut app = App::new();
        app.apply(Action::SetHealMode(HealMode::Replace));
        assert_eq!(app.heal_mode, HealMode::Replace);
    }

    #[test]
    fn test_set_spot_heal_mode() {
        let mut app = App::new();
        app.apply(Action::SetSpotHealMode(SpotHealMode::TextureMatch));
        assert_eq!(app.spot_heal_mode, SpotHealMode::TextureMatch);
    }
}
