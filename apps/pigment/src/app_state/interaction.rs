//! Pointer/stroke interaction helpers for `App` (tool drag dispatch + the
//! brush/clone/heal/dodge/smudge/liquify stroke machinery, fill/gradient/shape,
//! the selection-tool ops, and autosave/print).
//!
//! Split out of `app_state/mod.rs` as a pure mechanical refactor (no behavior
//! change). These are the helpers `App::apply`, the drag dispatch, and the
//! render path call; bodies are unchanged. `paint_target` and
//! `sync_host_order_dirty` are widened to `pub(crate)` because sibling domain
//! modules call them; everything else keeps its original visibility.

use super::*;

impl App {
    // ---- Painting (brush / eraser strokes) -----------------------------------

    /// Whether the active tool paints (Brush or Eraser). Any other tool makes the
    /// stroke methods no-op this pass — other tools are wired in later waves.
    fn is_paint_tool(&self) -> bool {
        matches!(self.active, Tool::Brush | Tool::Eraser)
    }

    /// The layer to paint into: the active layer, falling back to the top layer
    /// (Vec back = top of stack) so a fresh doc with no explicit selection still
    /// paints somewhere sensible.
    pub(crate) fn paint_target(&self) -> Option<LayerId> {
        self.doc
            .active_layer
            .or_else(|| self.doc.layers.layers.last().map(|l| l.id))
    }

    /// Whether the current stroke should paint the active layer's MASK instead of
    /// its pixels: mask-edit mode is on AND the paint target carries a mask.
    /// Mirrors the egui app's `paint_mask` gate
    /// (`edit_mask && masked_layers.contains(active)`).
    fn paint_into_mask(&self) -> bool {
        self.edit_mask
            && self
                .paint_target()
                .is_some_and(|id| self.masked_layers.contains(&id))
    }

    /// Dab spacing in doc px, mirroring the egui app
    /// (`view.rs`: `(brush_size * 0.15).max(0.75)`).
    fn dab_spacing(&self) -> f32 {
        (self.brush.size * 0.15).max(0.75)
    }

    /// Build a `Dab` at `doc` from the current brush, mirroring the egui app's
    /// `dab_at` (`pigment-app/src/app/state.rs`): radius = size * 0.5 (size is a
    /// diameter), hardness clamped to 0.99, color = per-channel `srgb_to_linear`
    /// of the straight-sRGB brush color (straight linear, NOT premultiplied —
    /// the dab shader premultiplies). Alpha is the brush opacity. `size_scale` is
    /// the velocity taper (we pass 1.0; speed dynamics are a later wave).
    fn dab_at(&self, doc: [f32; 2], size_scale: f32) -> Dab {
        let c = self.brush.color;
        Dab {
            center: doc,
            radius: (self.brush.size * 0.5 * size_scale).max(0.5),
            hardness: self.brush.hardness.clamp(0.0, 0.99),
            color: [
                srgb_to_linear(c[0]),
                srgb_to_linear(c[1]),
                srgb_to_linear(c[2]),
                self.brush.opacity,
            ],
        }
    }

    /// Begin a stroke at `doc` (doc px): reset the residual, stamp one dab, and
    /// remember the position. No-op for non-paint tools. Mirrors the egui app's
    /// stroke-begin branch (one dab, `stroke_residual = 0`).
    pub fn begin_stroke(&mut self, doc: [f32; 2]) {
        if !self.is_paint_tool() {
            return;
        }
        let Some(layer) = self.paint_target() else {
            return;
        };
        let erase = self.active == Tool::Eraser;
        let into_mask = self.paint_into_mask();
        self.stroke_residual = 0.0;
        let mut dab = self.dab_at(doc, 1.0);
        // Painting a mask reveals (white); the eraser hides. (The egui app forces
        // the dab color to white for a non-erase mask stroke.)
        if into_mask && !erase {
            dab.color = [1.0, 1.0, 1.0, dab.color[3]];
        }
        // Snapshot the layer at stroke start so the whole stroke is one undo step
        // (mask edits aren't snapshotted yet — match the egui app).
        self.host.paint_dabs(layer, &[dab], erase, !into_mask, into_mask);
        self.stroke_last = Some(doc);
    }

    /// Continue the stroke to `doc` (doc px): place dabs every `spacing` px from
    /// the last position to `doc`, carrying `stroke_residual` across segments so
    /// spacing stays continuous (the egui model). No-op for non-paint tools or if
    /// no stroke is in progress.
    pub fn continue_stroke(&mut self, doc: [f32; 2]) {
        if !self.is_paint_tool() {
            return;
        }
        let Some(last) = self.stroke_last else {
            return;
        };
        let Some(layer) = self.paint_target() else {
            return;
        };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 {
            return;
        }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = self.dab_spacing();
        let erase = self.active == Tool::Eraser;
        let into_mask = self.paint_into_mask();
        let mut dabs: Vec<Dab> = Vec::new();
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            let mut dab = self.dab_at(p, 1.0);
            if into_mask && !erase {
                dab.color = [1.0, 1.0, 1.0, dab.color[3]];
            }
            dabs.push(dab);
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);

        // Continuation dabs land in the same undo step opened at stroke start.
        self.host.paint_dabs(layer, &dabs, erase, false, into_mask);
    }

    /// End the stroke: clear the in-progress position and residual.
    pub fn end_stroke(&mut self) {
        self.stroke_last = None;
        self.stroke_residual = 0.0;
    }

    // ---- Clone stamp ---------------------------------------------------------
    //
    // Mirrors the egui app's Clone tool (`view.rs`): Alt-click sets the source
    // anchor; a subsequent drag locks `clone_offset = dest − source` at the first
    // dab and copies pixels from a frozen source snapshot along the stroke. The
    // engine (`paint_clone_dabs`) owns the copy/falloff; the host only snapshots
    // the source and feeds dabs + offset.

    /// Clone pointer-down: Alt-press sets the source anchor (no paint); otherwise,
    /// if a source is set, freeze it, snapshot the layer for undo, lock the offset,
    /// and stamp the first dab. If no source has been set yet (and alt is not held),
    /// the first click auto-sets the source anchor so the user can immediately drag
    /// to clone on the next interaction — no modifier key required for initial setup.
    fn begin_clone(&mut self, doc: [f32; 2], alt: bool) {
        if alt || self.clone_source.is_none() {
            // Explicit Alt+click OR no source set yet → establish source anchor, no paint.
            self.clone_source = Some(doc);
            return;
        }
        let Some(src) = self.clone_source else { return };
        let Some(layer) = self.paint_target() else { return };
        // Freeze the source + open one undo step for the whole clone stroke.
        self.host.snapshot_layer(layer, "Clone Stamp");
        self.host.snapshot_clone_source(layer);
        self.clone_offset = [doc[0] - src[0], doc[1] - src[1]];
        self.stroke_residual = 0.0;
        let dab = self.dab_at(doc, 1.0);
        self.host.paint_clone_dabs(layer, &[dab], self.clone_offset);
        self.stroke_last = Some(doc);
    }

    /// Clone pointer-drag: stamp dabs every `spacing` px from the last position,
    /// copying from the frozen source at the locked offset. No-op if no stroke is
    /// in progress (e.g. an alt-press with no drag).
    fn continue_clone(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else {
            return;
        };
        let Some(layer) = self.paint_target() else {
            return;
        };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 {
            return;
        }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = self.dab_spacing();
        let mut dabs: Vec<Dab> = Vec::new();
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            dabs.push(self.dab_at(p, 1.0));
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
        self.host.paint_clone_dabs(layer, &dabs, self.clone_offset);
    }

    /// End a clone stroke (same bookkeeping reset as a brush stroke).
    fn end_clone(&mut self) {
        self.stroke_last = None;
        self.stroke_residual = 0.0;
    }

    // ---- Healing brush -------------------------------------------------------
    //
    // Mirrors the Clone stamp flow but routes to `CanvasHost::heal_at` which
    // forces dab hardness to 0.0 for a soft Gaussian-weighted blend. Alt-click
    // sets the source anchor (same as Clone); a subsequent drag copies softly.

    /// Heal pointer-down: Alt-press sets the source anchor (no paint); otherwise
    /// freezes the source, opens one undo step, locks the offset, and stamps the
    /// first soft dab. If no source anchor has been set yet, the first click
    /// auto-sets it (same fallback as Clone) so Alt is not strictly required.
    fn begin_heal(&mut self, doc: [f32; 2], alt: bool) {
        if alt || self.clone_source.is_none() {
            self.clone_source = Some(doc);
            return;
        }
        let Some(src) = self.clone_source else { return };
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Heal");
        self.host.snapshot_clone_source(layer);
        self.clone_offset = [doc[0] - src[0], doc[1] - src[1]];
        self.stroke_residual = 0.0;
        match self.heal_mode {
            HealMode::Content => {
                self.apply(Action::ContentAwareFill);
                return;
            }
            HealMode::Replace => {
                let dab = self.heal_dab_at(doc);
                self.host.paint_clone_dabs(layer, &[dab], self.clone_offset);
            }
            HealMode::Normal => {
                let dab = self.heal_dab_at(doc);
                self.host.heal_at(layer, &[dab], self.clone_offset);
            }
        }
        self.stroke_last = Some(doc);
    }

    /// Heal pointer-drag: stamp soft dabs every `spacing` px from the last
    /// position, copying from the frozen source at the locked offset.
    fn continue_heal(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 { return; }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = (self.heal_radius as f32 * 0.15).max(0.75);
        let mut dabs: Vec<Dab> = Vec::new();
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            dabs.push(self.heal_dab_at(p));
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
        match self.heal_mode {
            HealMode::Replace => self.host.paint_clone_dabs(layer, &dabs, self.clone_offset),
            _ => self.host.heal_at(layer, &dabs, self.clone_offset),
        }
    }

    /// End a heal stroke (same bookkeeping reset as a brush/clone stroke).
    fn end_heal(&mut self) {
        self.stroke_last = None;
        self.stroke_residual = 0.0;
    }

    /// Build a dab sized to `heal_radius` for the healing brush.
    fn heal_dab_at(&self, doc: [f32; 2]) -> Dab {
        let c = self.brush.color;
        Dab {
            center: doc,
            radius: (self.heal_radius as f32).max(0.5),
            hardness: match self.heal_mode {
                HealMode::Normal => 0.0,
                HealMode::Replace => self.brush.hardness.clamp(0.0, 0.99),
                HealMode::Content => 0.0,
            },
            color: [
                srgb_to_linear(c[0]),
                srgb_to_linear(c[1]),
                srgb_to_linear(c[2]),
                self.brush.opacity,
            ],
        }
    }

    /// Write a minimal autosave JSON to `~/.local/share/prism/pigment_autosave.json`.
    pub fn do_autosave(&mut self) {
        let Some(path) = autosave_path() else { return };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!("autosave dir create failed: {e}");
                return;
            }
        }
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
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "size": { "width": self.doc.size.width, "height": self.doc.size.height },
            "layers": layers,
        });
        match serde_json::to_string_pretty(&json) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    log::warn!("autosave write failed: {e}");
                } else {
                    self.last_autosave = Some(std::time::Instant::now());
                    log::info!("autosave written to {:?}", path);
                }
            }
            Err(e) => log::warn!("autosave serialize failed: {e}"),
        }
    }

    /// Call once per frame. Triggers `do_autosave` if the configured interval has elapsed.
    pub fn maybe_autosave(&mut self) {
        if self.autosave_interval_secs == 0 {
            return;
        }
        let elapsed = match self.last_autosave {
            Some(t) => t.elapsed().as_secs(),
            None => self.autosave_interval_secs,
        };
        if elapsed >= self.autosave_interval_secs {
            self.do_autosave();
        }
    }

    /// Write a minimal single-page PDF embedding the composited canvas as JPEG,
    /// save to a temp file, and open with the OS viewer.
    pub fn do_print(&mut self) {
        let Some(flat) = self.host.read_composite_f32() else {
            self.status_message = Some("Print failed: could not read canvas".to_string());
            return;
        };
        let (dw, dh) = (self.host.doc_w, self.host.doc_h);
        let mut rgb8: Vec<u8> = Vec::with_capacity((dw * dh * 3) as usize);
        for i in 0..(dw * dh) as usize {
            let r = flat[i * 4];
            let g = flat[i * 4 + 1];
            let b = flat[i * 4 + 2];
            let a = flat[i * 4 + 3];
            let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
            rgb8.push(((r * inv).clamp(0.0, 1.0) * 255.0).round() as u8);
            rgb8.push(((g * inv).clamp(0.0, 1.0) * 255.0).round() as u8);
            rgb8.push(((b * inv).clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        let mut jpeg_buf = Vec::new();
        {
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_buf, 90);
            if let Err(e) = enc.encode(&rgb8, dw, dh, image::ExtendedColorType::Rgb8) {
                self.status_message = Some(format!("Print failed (JPEG encode): {e}"));
                return;
            }
        }
        let pdf = build_minimal_pdf(&jpeg_buf, dw, dh, self.print_landscape);
        let tmp = std::env::temp_dir().join("pigment_print.pdf");
        if let Err(e) = std::fs::write(&tmp, &pdf) {
            self.status_message = Some(format!("Print failed (PDF write): {e}"));
            return;
        }
        let viewer = if cfg!(target_os = "macos") { "open" } else if cfg!(target_os = "windows") { "explorer" } else { "xdg-open" };
        match std::process::Command::new(viewer).arg(&tmp).spawn() {
            Ok(_) => {
                self.status_message = Some(format!("Print PDF opened: {}", tmp.display()));
                self.show_print_dialog = false;
            }
            Err(e) => {
                self.status_message = Some(format!("Print failed (open): {e}"));
            }
        }
    }

    // ---- Dodge / Burn tool --------------------------------------------------

    fn begin_dodge_burn(&mut self, doc: [f32; 2], strength: f32) {
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Dodge/Burn");
        let radius = self.dodge_size * 0.5;
        self.host.dodge_burn_dab(layer, doc[0], doc[1], radius, strength);
        self.stroke_last = Some(doc);
        self.stroke_residual = 0.0;
    }

    fn continue_dodge_burn(&mut self, doc: [f32; 2], strength: f32) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 { return; }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = (self.dodge_size * 0.15).max(0.75);
        let radius = self.dodge_size * 0.5;
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            self.host.dodge_burn_dab(layer, p[0], p[1], radius, strength);
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
    }

    // ---- Liquify warp tool --------------------------------------------------

    fn begin_liquify(&mut self, doc: [f32; 2]) {
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Liquify");
        self.stroke_last = Some(doc);
    }

    fn continue_liquify(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let dx = doc[0] - last[0];
        let dy = doc[1] - last[1];
        if dx.hypot(dy) < 0.5 { return; }
        let radius = self.dodge_size * 1.5;
        self.host.liquify_warp(layer, doc[0], doc[1], dx, dy, radius, 0.4);
        self.stroke_last = Some(doc);
    }

    // ---- Smudge tool --------------------------------------------------------

    fn begin_smudge(&mut self, doc: [f32; 2]) {
        let Some(layer) = self.paint_target() else { return };
        self.host.snapshot_layer(layer, "Smudge");
        let radius = self.dodge_size * 0.5;
        self.host.smudge_dab(layer, doc[0], doc[1], radius, self.smudge_strength);
        self.stroke_last = Some(doc);
        self.stroke_residual = 0.0;
    }

    fn continue_smudge(&mut self, doc: [f32; 2]) {
        let Some(last) = self.stroke_last else { return };
        let Some(layer) = self.paint_target() else { return };
        let seg = [doc[0] - last[0], doc[1] - last[1]];
        let dist = (seg[0] * seg[0] + seg[1] * seg[1]).sqrt();
        if dist <= 1e-3 { return; }
        let dir = [seg[0] / dist, seg[1] / dist];
        let spacing = (self.dodge_size * 0.15).max(0.75);
        let radius = self.dodge_size * 0.5;
        let mut t = self.stroke_residual;
        while t <= dist {
            let p = [last[0] + dir[0] * t, last[1] + dir[1] * t];
            self.host.smudge_dab(layer, p[0], p[1], radius, self.smudge_strength);
            t += spacing;
        }
        self.stroke_residual = t - dist;
        self.stroke_last = Some(doc);
    }

    // ---- Pointer drag dispatch (non-paint tools) -----------------------------
    //
    // The root view routes ALL canvas pointer events through `begin_drag` /
    // `continue_drag` / `end_drag`. Each dispatches on the active tool: paint
    // tools fall through to the existing stroke path; Selection routes to the
    // marquee path; Move/Transform route to the live-affine path. This keeps the
    // root view's three handlers tool-agnostic (it just maps window→doc px and
    // calls these), exactly mirroring how the brush is wired.

    /// Pointer-down at `doc` (doc px); `alt`/`shift` carry the Option/Alt and Shift
    /// modifiers. The Clone tool uses `alt` to set its source anchor; the selection
    /// tools use both to pick the combine mode (Shift = add, Alt = subtract,
    /// Shift+Alt = intersect, else replace). Dispatches on the active tool.
    pub fn begin_drag(&mut self, doc: [f32; 2], alt: bool, shift: bool) {
        self.last_drag = Some(doc);
        match self.active {
            Tool::Brush | Tool::Eraser => self.begin_stroke(doc),
            Tool::Clone => self.begin_clone(doc, alt),
            Tool::Heal => self.begin_heal(doc, alt),
            Tool::Dodge => self.begin_dodge_burn(doc, self.dodge_strength.abs()),
            Tool::Burn => self.begin_dodge_burn(doc, -self.dodge_strength.abs()),
            Tool::Smudge => self.begin_smudge(doc),
            Tool::Liquify => self.begin_liquify(doc),
            Tool::Crop => {
                self.crop_rect = Some([doc[0], doc[1], doc[0], doc[1]]);
            }
            Tool::Text => self.place_text(doc),
            Tool::SelectRect | Tool::SelectEllipse => {
                self.sel_drag_start = Some(doc);
                self.begin_selection_op(shift, alt);
                // A bare press starts an empty marquee; the drag fills it in.
            }
            Tool::Lasso => {
                self.begin_selection_op(shift, alt);
                self.lasso_points.clear();
                self.lasso_points.push(doc);
            }
            Tool::MagicWand => {
                // A single click flood-selects from the seed immediately.
                self.begin_selection_op(shift, alt);
                self.magic_wand_at(doc);
            }
            Tool::Move | Tool::MoveLayer | Tool::Transform => {
                self.xform_drag_start = Some(doc);
                self.xform_translate = [0.0, 0.0];
                self.xform_scale = 1.0;
            }
            // Paint-bucket: a single click fills from the seed immediately.
            Tool::Fill => self.fill_at(doc),
            // Pen tool: each click adds an anchor node.
            Tool::Pen => self.apply(Action::PenAddNode((doc[0], doc[1]))),
            // Gradient / shapes anchor here; the drag end applies them.
            Tool::Gradient => self.grad_drag_start = Some(doc),
            Tool::ShapeRect | Tool::ShapeEllipse => self.shape_drag_start = Some(doc),
            Tool::Slice => self.slice_drag_start = Some(doc),
            _ => {}
        }
    }

    /// Pointer-drag to `doc` (doc px). Dispatches on the active tool.
    pub fn continue_drag(&mut self, doc: [f32; 2]) {
        self.last_drag = Some(doc);
        match self.active {
            Tool::Brush | Tool::Eraser => self.continue_stroke(doc),
            Tool::Clone => self.continue_clone(doc),
            Tool::Heal => self.continue_heal(doc),
            Tool::Dodge => self.continue_dodge_burn(doc, self.dodge_strength.abs()),
            Tool::Burn => self.continue_dodge_burn(doc, -self.dodge_strength.abs()),
            Tool::Smudge => self.continue_smudge(doc),
            Tool::Liquify => self.continue_liquify(doc),
            Tool::Crop => {
                if let Some(r) = self.crop_rect.as_mut() {
                    r[2] = doc[0];
                    r[3] = doc[1];
                }
            }
            Tool::SelectRect | Tool::SelectEllipse => {
                if let Some(start) = self.sel_drag_start {
                    self.preview_marquee(start, doc);
                }
            }
            Tool::Lasso => {
                // Append a point once we've moved a couple px (matches the egui
                // app's 2px threshold) so the polygon stays light.
                if self
                    .lasso_points
                    .last()
                    .is_none_or(|l| (l[0] - doc[0]).hypot(l[1] - doc[1]) > 2.0)
                {
                    self.lasso_points.push(doc);
                }
            }
            Tool::Move | Tool::MoveLayer | Tool::Transform => {
                let Some(start) = self.xform_drag_start else {
                    return;
                };
                // Transform: vertical drag uniformly scales about the canvas
                // center (the egui app gates scale behind Shift; here the
                // Transform tool itself selects scale, Move/MoveLayer select translate).
                if self.active == Tool::Transform {
                    let dy = doc[1] - start[1];
                    self.xform_scale = (1.0 - dy * 0.005).clamp(0.05, 20.0);
                } else {
                    let mut tx = doc[0] - start[0];
                    let mut ty = doc[1] - start[1];
                    if self.snap_to_grid {
                        let g = self.grid_size;
                        tx = (tx / g).round() * g;
                        ty = (ty / g).round() * g;
                    }
                    self.xform_translate = [tx, ty];
                }
                self.push_live_xform();
            }
            _ => {}
        }
    }

    /// Pointer-up. Dispatches on the active tool, finalizing the interaction.
    pub fn end_drag(&mut self) {
        match self.active {
            Tool::Brush | Tool::Eraser => self.end_stroke(),
            Tool::Clone => self.end_clone(),
            Tool::Heal => self.end_heal(),
            Tool::Dodge | Tool::Burn | Tool::Smudge => {
                self.stroke_last = None;
                self.stroke_residual = 0.0;
            }
            Tool::SelectRect | Tool::SelectEllipse => {
                self.sel_drag_start = None;
                self.sel_base.clear();
            }
            Tool::Lasso => {
                self.commit_lasso();
                self.lasso_points.clear();
                self.sel_base.clear();
            }
            Tool::MagicWand => {
                self.sel_base.clear();
            }
            Tool::Move | Tool::MoveLayer | Tool::Transform => {
                if self.xform_drag_start.is_some() {
                    // Keep the affine live for the bake (the engine reads the
                    // last `set_layer_transform`), bake it into pixels, then
                    // reset the live transform to identity.
                    self.push_live_xform();
                    if let Some(layer) = self.paint_target() {
                        self.host.bake_layer_xform(layer);
                    }
                    self.host.set_layer_xform(None, [1.0, 0.0, 0.0, 1.0], [0.0; 2]);
                    self.host.mark_dirty();
                    self.xform_drag_start = None;
                    self.xform_translate = [0.0, 0.0];
                    self.xform_scale = 1.0;
                }
            }
            Tool::Gradient => {
                if let Some(start) = self.grad_drag_start.take() {
                    self.apply_gradient(start, self.last_drag.unwrap_or(start));
                }
            }
            Tool::Slice => {
                if let Some(start) = self.slice_drag_start.take() {
                    let end = self.last_drag.unwrap_or(start);
                    let x = start[0].min(end[0]);
                    let y = start[1].min(end[1]);
                    let w = (start[0] - end[0]).abs();
                    let h = (start[1] - end[1]).abs();
                    if w > 2.0 && h > 2.0 {
                        self.apply(Action::AddSlice([x, y, w, h]));
                    }
                }
            }
            Tool::ShapeRect | Tool::ShapeEllipse => {
                if let Some(start) = self.shape_drag_start.take() {
                    let end = self.last_drag.unwrap_or(start);
                    let kind = if self.active == Tool::ShapeEllipse {
                        ShapeKind::Ellipse
                    } else {
                        ShapeKind::Rectangle
                    };
                    self.draw_shape(kind, start, end);
                }
            }
            _ => {}
        }
        self.last_drag = None;
    }

    /// Push the accumulated translate/scale to the host as a live affine on the
    /// active layer (or top layer fallback), so the next composite previews the
    /// Move/Transform without baking. Mirrors the egui app's `compute_xform`.
    fn push_live_xform(&mut self) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let (m, off) = compute_xform(
            self.xform_translate,
            self.xform_scale,
            self.doc.size.width as f32,
            self.doc.size.height as f32,
        );
        self.host.set_layer_xform(Some(layer), m, off);
    }

    // ---- Core canvas tools (fill / gradient / shape) -------------------------
    //
    // Thin wrappers that resolve the paint target + tool params, then forward to
    // the matching `CanvasHost` method (which owns the read → engine-rasterize →
    // source-over → upload through the shared `prism_core::{fill,gradient,shape}`
    // — no raster math lives here). Mirror the egui app's `do_fill`/`do_gradient`
    // and vector-shape rasterization.

    /// Paint-bucket fill from `doc` (doc px): flood the active layer at the seed
    /// within the current tolerance and write the brush color into the matched
    /// (and selected) pixels. No-op off-canvas or with no paint target.
    fn fill_at(&mut self, doc: [f32; 2]) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let (dw, dh) = (self.doc.size.width, self.doc.size.height);
        if dw == 0 || dh == 0 {
            return;
        }
        let sx = (doc[0].floor() as i64).clamp(0, dw as i64 - 1) as u32;
        let sy = (doc[1].floor() as i64).clamp(0, dh as i64 - 1) as u32;
        let mut color = self.brush.color;
        color[3] = self.brush.opacity;
        self.host
            .fill_at(layer, (sx, sy), color, self.fill_tolerance, self.fill_contiguous);
    }

    /// Apply a linear gradient along `p0 → p1` (doc px) to the active layer. The
    /// gradient runs from the brush color (opaque) to the brush color
    /// (transparent) — the egui app's default Foreground→Transparent rail.
    fn apply_gradient(&mut self, p0: [f32; 2], p1: [f32; 2]) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let mut c0 = self.brush.color;
        c0[3] = self.brush.opacity;
        let c1 = [self.brush.color[0], self.brush.color[1], self.brush.color[2], 0.0];
        self.host
            .apply_gradient(layer, p0, p1, c0, c1, self.gradient_dither);
    }

    /// Draw a filled `kind` shape spanning the `start → end` bbox (doc px) into
    /// the active layer, using the brush color (alpha = brush opacity). No-op for
    /// a degenerate (zero-area) drag.
    fn draw_shape(&mut self, kind: ShapeKind, start: [f32; 2], end: [f32; 2]) {
        let Some(layer) = self.paint_target() else {
            return;
        };
        let rect = [
            start[0].min(end[0]),
            start[1].min(end[1]),
            (start[0] - end[0]).abs(),
            (start[1] - end[1]).abs(),
        ];
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return;
        }
        let mut color = self.brush.color;
        color[3] = self.brush.opacity;
        self.host.draw_shape(layer, kind, rect, color);
    }

    // ---- Selection tools (marquee / ellipse / lasso / magic-wand) ------------
    //
    // All four reuse the shared engine ops — NOTHING is reimplemented here:
    //   • Rect/Ellipse marquee → `SelectionOp::Marquee` (engine rasterizes).
    //   • Lasso → `prism_core::raster::polygon_mask` (CPU) → `upload_selection`.
    //   • Magic wand → composite read-back + `prism_core::fill::flood_fill_mask`.
    // For the lasso/wand the freshly-computed mask is combined with the op's base
    // snapshot via `prism_core::raster::combine` (Shift adds, Alt subtracts, both
    // intersects), mirroring the egui app's `commit_selection`.

    /// Snapshot the current selection + capture the combine mode at op start so the
    /// op can add/subtract/intersect with what was selected. Mirrors the egui app's
    /// `sel_base = read_selection()` + `sel_mode = mode_from_modifiers(..)`.
    fn begin_selection_op(&mut self, shift: bool, alt: bool) {
        self.sel_mode = mode_from_modifiers(shift, alt);
        self.sel_base = self.host.read_selection_or_empty();
    }

    /// Preview/apply a rect or ellipse marquee spanning `start → cur` (doc px). For
    /// a Replace op we can use the fast engine `Marquee` rasterizer directly; for an
    /// add/subtract/intersect we rasterize the shape to a CPU mask and combine it
    /// with the base, then upload. The active tool selects rect vs ellipse.
    fn preview_marquee(&mut self, start: [f32; 2], cur: [f32; 2]) {
        let rect = [
            start[0].min(cur[0]),
            start[1].min(cur[1]),
            (start[0] - cur[0]).abs(),
            (start[1] - cur[1]).abs(),
        ];
        let ellipse = self.active == Tool::SelectEllipse;
        if self.sel_mode == CombineMode::Replace {
            self.apply(Action::SetMarquee { rect, ellipse });
            return;
        }
        // Combine path: rasterize the shape to a mask and merge with the base.
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return;
        }
        let (w, h) = (self.doc.size.width, self.doc.size.height);
        let shape = shape_mask(rect, ellipse, w, h);
        let combined = combine(&self.sel_base, &shape, self.sel_mode);
        self.host.upload_selection_mask(&combined);
        self.bump_selection();
    }

    /// Flush the in-progress lasso polygon to a selection mask (even-odd fill via
    /// the engine's `polygon_mask`), combined with the base per the active mode.
    fn commit_lasso(&mut self) {
        if self.lasso_points.len() < 3 {
            return;
        }
        let (w, h) = (self.doc.size.width, self.doc.size.height);
        let pts: Vec<(f32, f32)> = self.lasso_points.iter().map(|p| (p[0], p[1])).collect();
        let mask = polygon_mask(&pts, w, h);
        let combined = combine(&self.sel_base, &mask, self.sel_mode);
        self.host.upload_selection_mask(&combined);
        self.bump_selection();
    }

    /// Magic-wand at `doc` (doc px): composite the document, flood-select from the
    /// seed within the fill tolerance over the composited pixels (the egui app's
    /// `do_magic_wand`), combine with the base, and upload. No-op off-canvas.
    fn magic_wand_at(&mut self, doc: [f32; 2]) {
        let (w, h) = (self.doc.size.width, self.doc.size.height);
        if w == 0 || h == 0 {
            return;
        }
        let sx = (doc[0].floor() as i64).clamp(0, w as i64 - 1) as u32;
        let sy = (doc[1].floor() as i64).clamp(0, h as i64 - 1) as u32;
        let Some(buf) = self.host.read_composite_f32() else {
            return;
        };
        let mask_b = flood_fill_mask(&buf, w, h, sx, sy, self.fill_tolerance, self.fill_contiguous);
        let mask: Vec<f32> = mask_b.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect();
        let combined = combine(&self.sel_base, &mask, self.sel_mode);
        self.host.upload_selection_mask(&combined);
        self.bump_selection();
    }

    /// Rebuild the host's per-layer draw order from the current document layer
    /// stack and mark the host dirty so the next `image()` re-composites. Call
    /// after any mutation that changes which layers (or with what opacity/blend/
    /// visibility) the compositor draws.
    pub(crate) fn sync_host_order_dirty(&mut self) {
        self.host.sync_order(&self.doc);
        self.host.mark_dirty();
    }

}
