//! [`Comp`](super::Comp) **query / resolution** methods extracted from `comp/mod.rs`
//! (workspace size rule): the world-matrix / parent-chain walk, the 3-D
//! projection + depth-sort + lighting + depth-of-field resolution, the
//! expression-aware transform / opacity / source-time samplers, the matte-source
//! lookups, the work-area clamp, and marker navigation.
//!
//! This is a second `impl Comp` block — Rust allows the inherent impl to span
//! files in the same crate — so the methods stay on `Comp` exactly as before.
//! `use super::*` brings the `comp` module's types and the sibling `camera` /
//! `light` / `motion_path` helpers into scope, so the bodies are byte-identical to
//! when they lived inline.

use super::*;

impl Comp {
    /// The **world** affine matrix of layer `idx` at time `t`: its own local
    /// transform composed under every ancestor's transform (parent applied
    /// outermost), mapping the layer's local-space points into final comp space.
    ///
    /// Walks the parent chain defensively: out-of-range or self-referential
    /// parents are ignored, and a `visited` set breaks any cycle (a corrupt
    /// project can't hang the renderer), so the worst case is a finite, bounded
    /// walk producing the longest acyclic prefix.
    pub fn world_matrix(&self, idx: usize, t: f32) -> Affine2 {
        let mut visited = vec![false; self.layers.len()];
        let mut cur = idx;
        let mut m = Affine2::IDENTITY;
        loop {
            let Some(layer) = self.layers.get(cur) else {
                break;
            };
            if visited[cur] {
                break; // cycle guard
            }
            visited[cur] = true;
            // Parent applies outermost: world = parent_world · ... · local. Each
            // layer in the chain samples its own transform with **its own**
            // expression context (its index), so an expression on a parent drives
            // the child through the chain exactly as in After Effects.
            m = self.oriented_transform(layer, cur, t).local_matrix().then(m);
            match layer.parent {
                Some(p) if p != cur && p < self.layers.len() => cur = p,
                _ => break,
            }
        }
        m
    }

    /// Layer `idx`'s sampled **Z position** (comp px) at time `t`. `0.0` for a
    /// 2-D layer or a missing index, so a flat layer stays on the comp plane.
    pub fn layer_z(&self, idx: usize, t: f32) -> f32 {
        match self.layers.get(idx) {
            Some(l) if l.threed => l.z.sample(t, 0.0),
            _ => 0.0,
        }
    }

    /// Layer `idx`'s sampled X/Y/Z **orientation** (degrees) at time `t`. All
    /// zero for a 2-D layer or a missing index.
    fn layer_orientation(&self, idx: usize, t: f32) -> (f32, f32, f32) {
        match self.layers.get(idx) {
            Some(l) if l.threed => (
                l.orient_x.sample(t, 0.0),
                l.orient_y.sample(t, 0.0),
                l.orient_z.sample(t, 0.0),
            ),
            _ => (0.0, 0.0, 0.0),
        }
    }

    /// Whether layer `idx` is a **3-D layer** (projected through the camera and
    /// z-sorted). `false` for a 2-D layer or a missing index.
    pub fn layer_is_3d(&self, idx: usize) -> bool {
        self.layers.get(idx).is_some_and(|l| l.threed)
    }

    /// Layer `idx`'s **world placement point** in 3-D comp space at time `t`: the
    /// comp-space position its anchor maps to (the local matrix's translation),
    /// lifted to depth by its sampled Z. For a 2-D layer Z is `0`. Used by the
    /// z-sort (its projected depth) and as the pivot the 3-D orientation +
    /// perspective are applied about.
    fn layer_pivot_3d(&self, idx: usize, t: f32) -> [f32; 3] {
        let world = self.world_matrix(idx, t);
        let (px, py) = world.apply(0.0, 0.0); // local origin → comp space
        [px, py, self.layer_z(idx, t)]
    }

    /// The **camera-space depth** of 3-D layer `idx` at time `t` — the value the
    /// painter's z-sort orders by (larger = farther from the camera, drawn
    /// first). Falls back to `0.0` for a 2-D / missing layer.
    pub fn layer_depth(&self, idx: usize, t: f32) -> f32 {
        if !self.layer_is_3d(idx) {
            return 0.0;
        }
        let p = self.layer_pivot_3d(idx, t);
        self.camera
            .project(p[0], p[1], p[2], self.height as f32)
            .depth
    }

    /// The **illumination factor** (a per-channel RGB multiplier) the comp's
    /// [`lights`](Self::lights) apply to 3-D layer `idx` at time `t`, or `None`
    /// when the layer is unlit and must render unchanged.
    ///
    /// Returns `None` (no modulation) unless the layer is a **3-D layer** with
    /// [`accepts_lights`](PulseLayer) set **and** the comp has lights — so a
    /// pre-lighting comp, a 2-D layer, or a layer that doesn't opt in keeps its
    /// exact pixels (the back-compat contract). When lit, the factor is the pure
    /// [`light::illumination`] of the comp's lights at the layer's world position,
    /// against the layer's surface [`normal`](light::layer_normal) derived from
    /// its X/Y/Z orientation.
    pub fn layer_light_factor(&self, idx: usize, t: f32) -> Option<[f32; 3]> {
        if self.lights.is_empty() || !self.layer_is_3d(idx) {
            return None;
        }
        let layer = self.layers.get(idx)?;
        if !layer.accepts_lights {
            return None;
        }
        let (ox, oy, oz) = self.layer_orientation(idx, t);
        let normal = light::layer_normal(ox, oy, oz);
        let surface = self.layer_pivot_3d(idx, t);
        Some(light::illumination(&self.lights, surface, normal))
    }

    /// The **depth-of-field blur radius** (comp px) the camera applies to 3-D
    /// layer `idx` at time `t`, or `None` when the layer must render perfectly
    /// sharp.
    ///
    /// Returns `None` (no blur) unless the camera's
    /// [`dof_enabled`](Camera::dof_enabled) is set **and** the layer is a **3-D
    /// layer** — so a 2-D layer, or any comp with DoF off (the default), keeps its
    /// exact pixels (the back-compat contract). When DoF is on, the radius is the
    /// pure [`Camera::coc_blur_radius`] of the layer's camera-space
    /// [`depth`](Self::layer_depth); an in-focus layer yields `Some(0.0)` (a
    /// no-op blur) so the caller can treat "3-D + DoF" uniformly.
    pub fn layer_dof_blur(&self, idx: usize, t: f32) -> Option<f32> {
        if !self.camera.dof_enabled || !self.layer_is_3d(idx) {
            return None;
        }
        let depth = self.layer_depth(idx, t);
        Some(self.camera.coc_blur_radius(depth, self.height as f32))
    }

    /// Layer `idx`'s resolved comp-space [`Affine2`] for the rasterizer at time
    /// `t`, **including 3-D projection** when the layer is a 3-D layer.
    ///
    /// For a **2-D layer** this is exactly [`world_matrix`](Self::world_matrix) —
    /// byte-for-byte the prior behavior, so 2-D-only comps are unchanged.
    ///
    /// For a **3-D layer** the layer's flat quad (its 2-D world placement) is
    /// lifted into 3-D about its anchor by the X/Y/Z **orientation**, offset to
    /// its **Z** depth, and the three reference points — the anchor origin and
    /// the local `+x` / `+y` unit edges — are **projected through the camera**.
    /// The affine that maps those local reference points to their projected
    /// comp-space images is returned. With an un-oriented layer this is an exact
    /// uniform-scale-plus-translate perspective placement; with orientation it is
    /// the **best-fit affine** of the projected quad (a documented approximation —
    /// full per-pixel perspective-warp rasterization is a follow-up). At `Z = 0`
    /// with no orientation under the default camera it is the identity over the
    /// 2-D matrix, preserving back-compat.
    ///
    /// Returns `None` when the layer projects to a degenerate (zero-area or
    /// behind-camera) quad, in which case the caller draws nothing.
    pub fn layer_world(&self, idx: usize, t: f32) -> Option<Affine2> {
        let base = self.world_matrix(idx, t);
        if !self.layer_is_3d(idx) {
            return Some(base);
        }
        let (ox, oy, oz) = self.layer_orientation(idx, t);
        let z = self.layer_z(idx, t);
        let comp_h = self.height as f32;
        // Project a local point: take its 2-D comp-space image (anchor-relative
        // through the base matrix), rotate that offset about the anchor in 3-D by
        // the orientation, lift to the layer's Z, then perspective-project.
        let pivot = base.apply(0.0, 0.0); // comp-space anchor position (2-D)
        let project_local = |lx: f32, ly: f32| -> (f32, f32) {
            let (wx, wy) = base.apply(lx, ly);
            // Offset of this point from the anchor, in the layer's (already
            // scaled/rotated) comp-space frame — treated as the layer's local
            // plane (z = 0 before orientation).
            let (dx, dy) = (wx - pivot.0, wy - pivot.1);
            let (rx, ry, rz) = camera::rotate_orientation(dx, dy, 0.0, ox, oy, oz);
            let wp = [pivot.0 + rx, pivot.1 + ry, z + rz];
            self.camera.project(wp[0], wp[1], wp[2], comp_h).screen
        };
        // Fit the affine from three reference local points: origin + the two unit
        // edges. `local_matrix` maps these, so their projected images define the
        // affine columns directly.
        let o = project_local(0.0, 0.0);
        let ux = project_local(1.0, 0.0);
        let uy = project_local(0.0, 1.0);
        let a = ux.0 - o.0;
        let b = ux.1 - o.1;
        let c = uy.0 - o.0;
        let d = uy.1 - o.1;
        // Degenerate (collapsed) projection: nothing to draw.
        if (a * d - b * c).abs() < 1e-9 {
            return None;
        }
        Some(Affine2 {
            a,
            b,
            c,
            d,
            tx: o.0,
            ty: o.1,
        })
    }

    /// The comp's layer indices in **draw order** at time `t`: the 2-D stacking
    /// order (index 0 first / behind, last on top) with the **3-D layers
    /// painter's-sorted by camera-space depth** among themselves — farther 3-D
    /// layers drawn first.
    ///
    /// 2-D layers keep their exact stack positions; each contiguous run is left
    /// as-is. The 3-D layers are gathered, **stably** sorted by descending depth
    /// (farther first), and slotted back into the 3-D slots of the stack. With no
    /// 3-D layers this yields `0..len` unchanged, so the draw loop and output are
    /// byte-identical to before.
    pub fn draw_order(&self, t: f32) -> Vec<usize> {
        let n = self.layers.len();
        let order: Vec<usize> = (0..n).collect();
        // Fast path: no 3-D layers → identity order (back-compat).
        if !order.iter().any(|&i| self.layer_is_3d(i)) {
            return order;
        }
        // Collect the 3-D slot positions and the 3-D indices.
        let slots: Vec<usize> = order.iter().copied().filter(|&i| self.layer_is_3d(i)).collect();
        let mut threed: Vec<usize> = slots.clone();
        // Painter's order: farther (larger depth) first. Stable sort keeps the
        // original stacking order as the tie-break.
        threed.sort_by(|&x, &y| {
            let dx = self.layer_depth(x, t);
            let dy = self.layer_depth(y, t);
            dy.partial_cmp(&dx).unwrap_or(std::cmp::Ordering::Equal)
        });
        // Re-emit: 2-D layers in place, 3-D slots filled by the sorted 3-D list.
        let mut out = order.clone();
        for (slot, &li) in slots.iter().zip(threed.iter()) {
            out[*slot] = li;
        }
        out
    }

    /// The expression-evaluation context for layer `idx` at time `t`: the comp's
    /// `fps` / `duration` and the layer's stack index. `value` is filled in per
    /// property by the track sampler (overridden to the keyframed sample).
    pub fn expr_ctx(&self, idx: usize, t: f32) -> ExprCtx {
        ExprCtx {
            time: t,
            value: 0.0,
            fps: self.fps,
            duration: self.duration,
            index: idx,
            width: self.width as f32,
            height: self.height as f32,
        }
    }

    /// A layer's expression-aware [`Transform`] at time `t`, with **auto-orient
    /// along path** folded in: when the layer's [`auto_orient`](PulseLayer::auto_orient)
    /// flag is set, its motion-path travel heading (the tangent of its `x` / `y`
    /// position curve — see [`sample_path`]) is *added* to the keyframed rotation,
    /// so the layer turns to face its direction of travel while still honouring its
    /// own Rotation. With the flag off this is exactly `layer.transform_ctx(...)`,
    /// so non-oriented layers are untouched. The heading uses the keyframed
    /// position (matching the rendered path); a stationary point contributes `0°`.
    fn oriented_transform(&self, layer: &PulseLayer, idx: usize, t: f32) -> Transform {
        let mut tf = layer.transform_ctx(self.expr_ctx(idx, t));
        if layer.auto_orient {
            // Auto-orient reads the **effective** position path so the heading
            // follows the roving-re-timed motion (constant velocity) when any
            // interior position key roves; otherwise the originals are borrowed
            // unchanged.
            let (rx, ry) = layer.position_tracks();
            tf.rotation_deg += motion_path::auto_orient_deg(
                &rx,
                &ry,
                t,
                Prop::X.default_value(),
                Prop::Y.default_value(),
            );
        }
        tf
    }

    /// Layer `idx`'s sampled [`Transform`] at time `t`, **expression-aware**
    /// (each transform property evaluates its expression against the layer's
    /// context). The renderer/preview use this instead of [`PulseLayer::transform`]
    /// so expressions drive position / scale / rotation / anchor / opacity.
    pub fn layer_transform(&self, idx: usize, t: f32) -> Transform {
        match self.layers.get(idx) {
            Some(layer) => self.oriented_transform(layer, idx, t),
            None => Transform {
                anchor_x: 0.0,
                anchor_y: 0.0,
                x: 0.0,
                y: 0.0,
                scale: 1.0,
                rotation_deg: 0.0,
                opacity: 1.0,
            },
        }
    }

    /// Layer `idx`'s sampled (and clamped) **opacity** at time `t`, expression-
    /// aware — the value the rasterizers scale coverage by. `0.0` for a missing
    /// layer. Reads it off the resolved [`Transform`] so it always matches
    /// [`layer_transform`](Self::layer_transform).
    pub fn layer_opacity(&self, idx: usize, t: f32) -> f32 {
        if self.layers.get(idx).is_none() {
            return 0.0;
        }
        self.layer_transform(idx, t).opacity
    }

    /// The **source time** layer `idx` should sample its time-based source at,
    /// given comp time `t`. When the layer's [`TimeRemap`] is active this is the
    /// remap track's (expression-aware) value at `t`; otherwise it is `t`
    /// unchanged (identity — every non-remapped layer behaves exactly as before).
    ///
    /// The renderer routes footage frame-indexing and precomp recursion through
    /// this so an enabled remap freezes / reverses / retimes the source. A missing
    /// layer returns `t` (identity).
    pub fn layer_source_time(&self, idx: usize, t: f32) -> f32 {
        match self.layers.get(idx) {
            Some(layer) => layer.time_remap.source_time_ctx(self.expr_ctx(idx, t)),
            None => t,
        }
    }

    /// Sample layer `idx`'s property `prop` at time `t`, expression-aware. Used by
    /// the UI to show the live (expression-resolved) value. `default_value` for a
    /// missing layer.
    pub fn layer_value(&self, idx: usize, prop: Prop, t: f32) -> f32 {
        self.layers
            .get(idx)
            .map(|l| l.value_ctx(prop, self.expr_ctx(idx, t)))
            .unwrap_or_else(|| prop.default_value())
    }

    /// Whether layer `idx` is rendered with **motion blur**: the comp's master
    /// [`MotionBlur::enabled`] switch is on *and* the layer has its own
    /// per-layer `motion_blur` flag set. A missing index is `false`.
    pub fn layer_motion_blurred(&self, idx: usize) -> bool {
        self.motion_blur.enabled && self.layers.get(idx).is_some_and(|layer| layer.motion_blur)
    }

    /// The index of layer `idx`'s **matte source** — the layer directly above it
    /// in the stack (next-higher index) — when `idx` has an active [`MatteMode`]
    /// and such a layer exists. `None` if the layer has no matte or sits at the
    /// top of the stack (no layer above to borrow).
    pub fn matte_source(&self, idx: usize) -> Option<usize> {
        let layer = self.layers.get(idx)?;
        if !layer.matte.is_active() {
            return None;
        }
        let src = idx + 1;
        (src < self.layers.len()).then_some(src)
    }

    /// Whether layer `idx` is **consumed as a matte source** by the layer
    /// directly below it (so it must not composite on its own). True iff the
    /// layer below (`idx - 1`) has an active matte mode.
    pub fn is_matte_source(&self, idx: usize) -> bool {
        idx.checked_sub(1)
            .and_then(|below| self.layers.get(below))
            .is_some_and(|below| below.matte.is_active())
    }

    /// The comp's **work area** clamped to its own `[0, duration]` timeline — the
    /// range the transport / RAM-preview loop within. Always ordered and inside the
    /// comp (a hand-edited or stale range can never invert or escape).
    ///
    /// As a back-compat / self-heal: the `serde` default empty `[0, 0]` work area
    /// (an old `.pulse` file with no `work_area` field) on a comp with a real
    /// duration is treated as the **whole timeline**, so a pre-work-area project
    /// loops its full length rather than a degenerate zero-length range.
    pub fn clamped_work_area(&self) -> WorkArea {
        let wa = self.work_area.clamped(self.duration);
        if wa == (WorkArea { start: 0.0, end: 0.0 }) && self.duration > 0.0 {
            return WorkArea::full(self.duration);
        }
        wa
    }

    /// All marker times visible for navigation: the comp's own markers plus —
    /// when a layer is selected — that layer's markers (After Effects' "jump to
    /// marker" considers the comp + the active layer's markers). Used by
    /// [`next_marker`](Self::next_marker) / [`prev_marker`](Self::prev_marker).
    fn nav_markers(&self, selected: Option<usize>) -> Vec<Marker> {
        let mut all = self.markers.clone();
        if let Some(layer) = selected.and_then(|i| self.layers.get(i)) {
            all.extend(layer.markers.iter().cloned());
        }
        all
    }

    /// The next marker time strictly after `time` (comp markers + the selected
    /// layer's markers), or `None` when none lies ahead.
    pub fn next_marker(&self, time: f32, selected: Option<usize>) -> Option<f32> {
        next_marker_time(&self.nav_markers(selected), time)
    }

    /// The previous marker time strictly before `time` (comp markers + the
    /// selected layer's markers), or `None` when none lies behind.
    pub fn prev_marker(&self, time: f32, selected: Option<usize>) -> Option<f32> {
        prev_marker_time(&self.nav_markers(selected), time)
    }

    /// Whether making `child` a parent of `parent` is legal: a layer can't
    /// parent to itself, to a missing layer, or to one of its own descendants
    /// (which would create a cycle). Returns `true` when the link is safe.
    pub fn can_parent(&self, child: usize, parent: usize) -> bool {
        if child == parent || parent >= self.layers.len() || child >= self.layers.len() {
            return false;
        }
        // Walk up from `parent`; if we reach `child`, linking would cycle.
        let mut visited = vec![false; self.layers.len()];
        let mut cur = parent;
        loop {
            if cur == child {
                return false;
            }
            if visited[cur] {
                return true; // pre-existing cycle elsewhere; this link is fine
            }
            visited[cur] = true;
            match self.layers[cur].parent {
                Some(p) if p < self.layers.len() => cur = p,
                _ => return true,
            }
        }
    }
}
