//! **Puppet-pin mesh deformation** — Pulse's real (non-stub) implementation of
//! After Effects' *Puppet* tool.
//!
//! A layer is covered by a **triangulated deformation mesh** (a uniform grid
//! clipped to the layer's bounding box — robust and degenerate-free). The user
//! drops [`MeshPin`]s anywhere on the mesh; dragging a pin away from its rest
//! position warps the whole mesh. The math is **pure and deterministic** (no
//! RNG, no clock, no iteration-order dependence), so it lives as a set of free
//! functions that can be unit-tested without an `App`:
//!
//! * [`grid_mesh`] builds the rest mesh (vertices + triangles) over a bounds.
//! * [`deform_mesh`] displaces every vertex by a **weighted handle deformation**:
//!   each vertex's displacement is the normalized inverse-distance-squared blend
//!   (Shepard interpolation) of the pin displacements, with a "rest anchor"
//!   ground weight so a *single* pin produces a smooth radial falloff rather than
//!   a rigid translation. A pin's `stiffness` scales its weight, so a high-
//!   stiffness pin holding `delta = 0` keeps its neighborhood rigid (the *Starch*
//!   pin), and `stiffness = 0` disables a pin.
//! * [`sample_warp`] maps an arbitrary point through the deformation by finding
//!   its enclosing **rest** triangle, taking barycentric coordinates, and
//!   reconstructing the point in the matching **deformed** triangle.
//!
//! The `App` impl below holds a per-layer mesh map and the panel actions
//! (mirroring `particles.rs` — app-side state that doesn't touch the engine's
//! effect stacks, so the engine crate stays unchanged). Like particles, these
//! edits are **not** undoable.

#![allow(dead_code)]

use super::{Action, App};

// ── Constants ────────────────────────────────────────────────────────────────

/// Numerical floor mixed into squared distances so a vertex sitting exactly on a
/// pin (or the falloff radius) never divides by zero. Tiny enough that "pin on a
/// vertex" still reproduces the pin's displacement to ~1e-7.
const EPS: f32 = 1e-6;

// ── Types ────────────────────────────────────────────────────────────────────

/// A single puppet pin attached to a mesh. `rest` is where the pin was dropped
/// (its anchor in the undeformed mesh); `pos` is its current position. The pin's
/// **displacement** that drives the warp is `pos - rest`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshPin {
    /// Stable per-mesh id (monotonic; assigned by [`DeformMesh::add_pin`]).
    pub id: u64,
    /// Rest (anchor) position in layer-local pixels.
    pub rest: [f32; 2],
    /// Current position in layer-local pixels (dragging this warps the mesh).
    pub pos: [f32; 2],
    /// Weight stiffness multiplier (≥ 0). `1.0` is normal; higher = more
    /// influence (a *Starch* pin holding `delta = 0` keeps its area rigid);
    /// `0.0` disables the pin.
    pub stiffness: f32,
}

/// One pin's contribution to the deformation, as consumed by [`deform_mesh`].
/// Decoupled from [`MeshPin`] so the pure deformer can be driven from any source
/// (handles, tests, an external rig…), not just a [`DeformMesh`]'s own pins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinDisplacement {
    /// The pin's rest/anchor position (where its influence is centered).
    pub rest: [f32; 2],
    /// The displacement to apply (`pos - rest`).
    pub delta: [f32; 2],
    /// Weight stiffness multiplier (≥ 0); see [`MeshPin::stiffness`].
    pub stiffness: f32,
}

/// A triangulated deformation mesh over a layer's bounding box.
#[derive(Clone, Debug, PartialEq)]
pub struct DeformMesh {
    /// The layer this mesh belongs to (app-side key).
    pub layer_id: usize,
    /// The bounding box the grid was built over: `[min_x, min_y, max_x, max_y]`.
    pub bounds: [f32; 4],
    /// Grid resolution = cells per axis (≥ 1). Vertices per axis = `resolution+1`.
    pub resolution: u32,
    /// Rest vertex positions in layer-local pixels (row-major, `(res+1)²` of them).
    pub vertices: Vec<[f32; 2]>,
    /// Triangle vertex-index triples (CCW), `2·resolution²` of them.
    pub triangles: Vec<[usize; 3]>,
    /// The pins driving this mesh.
    pub pins: Vec<MeshPin>,
    /// Inverse-distance-weighting "rest anchor" radius (px). At distance `falloff`
    /// from a lone pin a vertex follows it ~half-way; larger = stiffer/broader
    /// influence. Defaults to half the bounds diagonal in [`grid_mesh`].
    pub falloff: f32,
    /// Next pin id to hand out (monotonic, deterministic).
    pub next_pin_id: u64,
}

// ── Pure math helpers ────────────────────────────────────────────────────────

/// Linear interpolation `a + (b - a)·t`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Barycentric coordinates `[u, v, w]` of `p` with respect to triangle
/// `(a, b, c)` (so `p = u·a + v·b + w·c` and `u + v + w = 1`). Returns `None`
/// only for a degenerate (zero-area) triangle. `p` is inside the triangle iff
/// all three coordinates are ≥ 0.
pub fn barycentric(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Option<[f32; 3]> {
    let v0 = [b[0] - a[0], b[1] - a[1]];
    let v1 = [c[0] - a[0], c[1] - a[1]];
    let v2 = [p[0] - a[0], p[1] - a[1]];
    let d00 = v0[0] * v0[0] + v0[1] * v0[1];
    let d01 = v0[0] * v1[0] + v0[1] * v1[1];
    let d11 = v1[0] * v1[0] + v1[1] * v1[1];
    let d20 = v2[0] * v0[0] + v2[1] * v0[1];
    let d21 = v2[0] * v1[0] + v2[1] * v1[1];
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < EPS {
        return None;
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    Some([u, v, w])
}

// ── Mesh construction ─────────────────────────────────────────────────────────

/// Build a uniform grid [`DeformMesh`] over `bounds` (`[min_x, min_y, max_x,
/// max_y]`, auto-ordered) with `resolution` cells per axis (clamped to ≥ 1).
/// Produces `(resolution+1)²` vertices and `2·resolution²` triangles. The
/// [`DeformMesh::falloff`] defaults to half the bounds diagonal.
pub fn grid_mesh(layer_id: usize, bounds: [f32; 4], resolution: u32) -> DeformMesh {
    let res = resolution.max(1);
    let n = (res + 1) as usize; // vertices per axis
    let (min_x, max_x) = (bounds[0].min(bounds[2]), bounds[0].max(bounds[2]));
    let (min_y, max_y) = (bounds[1].min(bounds[3]), bounds[1].max(bounds[3]));

    let mut vertices = Vec::with_capacity(n * n);
    for j in 0..n {
        let fy = j as f32 / res as f32;
        let y = lerp(min_y, max_y, fy);
        for i in 0..n {
            let fx = i as f32 / res as f32;
            vertices.push([lerp(min_x, max_x, fx), y]);
        }
    }

    let mut triangles = Vec::with_capacity(2 * (res * res) as usize);
    for j in 0..res as usize {
        for i in 0..res as usize {
            let v00 = j * n + i;
            let v10 = j * n + i + 1;
            let v01 = (j + 1) * n + i;
            let v11 = (j + 1) * n + i + 1;
            // Two triangles per cell (consistent winding).
            triangles.push([v00, v10, v11]);
            triangles.push([v00, v11, v01]);
        }
    }

    let w = max_x - min_x;
    let h = max_y - min_y;
    let falloff = (0.5 * (w * w + h * h).sqrt()).max(1e-3);

    DeformMesh {
        layer_id,
        bounds: [min_x, min_y, max_x, max_y],
        resolution: res,
        vertices,
        triangles,
        pins: Vec::new(),
        falloff,
        next_pin_id: 1,
    }
}

impl DeformMesh {
    /// Add a pin at `pos` (rest = current = `pos`, stiffness 1.0). Returns its id.
    pub fn add_pin(&mut self, pos: [f32; 2]) -> u64 {
        let id = self.next_pin_id;
        self.next_pin_id += 1;
        self.pins.push(MeshPin { id, rest: pos, pos, stiffness: 1.0 });
        id
    }

    /// Rebuild the grid at a new `resolution`, preserving the pins and id
    /// counter (the bounds — hence vertex layout origin/extent and falloff — are
    /// unchanged).
    pub fn set_resolution(&mut self, resolution: u32) {
        let rebuilt = grid_mesh(self.layer_id, self.bounds, resolution);
        self.vertices = rebuilt.vertices;
        self.triangles = rebuilt.triangles;
        self.resolution = rebuilt.resolution;
        self.falloff = rebuilt.falloff;
    }

    /// The pins as [`PinDisplacement`]s (`delta = pos - rest`) for [`deform_mesh`].
    pub fn pin_displacements(&self) -> Vec<PinDisplacement> {
        self.pins
            .iter()
            .map(|p| PinDisplacement {
                rest: p.rest,
                delta: [p.pos[0] - p.rest[0], p.pos[1] - p.rest[1]],
                stiffness: p.stiffness,
            })
            .collect()
    }

    /// Convenience: deform this mesh by its own pins. Identity when there are no
    /// pins (or all sit at their rest).
    pub fn deformed_vertices(&self) -> Vec<[f32; 2]> {
        deform_mesh(self, &self.pin_displacements())
    }

    /// Find the (first) triangle containing `point` and return its index plus the
    /// barycentric coordinates of `point` within it. `None` if `point` lies
    /// outside every triangle.
    pub fn find_triangle(&self, point: [f32; 2]) -> Option<(usize, [f32; 3])> {
        for (ti, tri) in self.triangles.iter().enumerate() {
            let a = self.vertices[tri[0]];
            let b = self.vertices[tri[1]];
            let c = self.vertices[tri[2]];
            if let Some(bc) = barycentric(point, a, b, c) {
                if bc[0] >= -EPS && bc[1] >= -EPS && bc[2] >= -EPS {
                    return Some((ti, bc));
                }
            }
        }
        None
    }
}

// ── The deformer ──────────────────────────────────────────────────────────────

/// Deform `mesh`'s rest vertices by `disp`, returning the new vertex positions in
/// the same order.
///
/// Each vertex `v` is displaced by a **normalized inverse-distance-squared blend**
/// of the pin displacements (Shepard interpolation) plus a *rest anchor* ground
/// weight:
///
/// ```text
/// w_i      = stiffness_i / (|v - rest_i|² + ε)
/// w_anchor = 1 / (falloff² + ε)            // pulls toward the rest position
/// v' = v + (Σ w_i · delta_i) / (Σ w_i + w_anchor)
/// ```
///
/// The anchor term makes a **single** pin produce a smooth radial falloff
/// (`v' = v + delta · falloff²/(falloff² + d²)`) instead of a rigid translation,
/// while a pin sitting on a vertex still reproduces its displacement (its weight
/// dominates). Pure and deterministic. Returns an exact copy when `disp` is
/// empty.
pub fn deform_mesh(mesh: &DeformMesh, disp: &[PinDisplacement]) -> Vec<[f32; 2]> {
    if disp.is_empty() {
        return mesh.vertices.clone();
    }
    let r = mesh.falloff.max(1e-3);
    let w_anchor = 1.0 / (r * r + EPS);
    mesh.vertices
        .iter()
        .map(|&v| {
            let mut wsum = w_anchor;
            let mut acc = [0.0f32, 0.0f32];
            for d in disp {
                let s = d.stiffness.max(0.0);
                if s == 0.0 {
                    continue;
                }
                let dx = v[0] - d.rest[0];
                let dy = v[1] - d.rest[1];
                let w = s / (dx * dx + dy * dy + EPS);
                wsum += w;
                acc[0] += w * d.delta[0];
                acc[1] += w * d.delta[1];
            }
            [v[0] + acc[0] / wsum, v[1] + acc[1] / wsum]
        })
        .collect()
}

/// Map `point` through the deformation: locate its enclosing **rest** triangle
/// (from `original`), take barycentric coordinates, and reconstruct the point in
/// the matching **deformed** triangle (whose vertices are `deformed`, in the same
/// index order as `original.vertices`).
///
/// `deformed` is the output of [`deform_mesh`]. Points outside the mesh fall back
/// to the nearest triangle (barycentric coordinates clamped to the simplex), so
/// the map is total. Returns `point` unchanged if the meshes are inconsistent or
/// degenerate.
pub fn sample_warp(point: [f32; 2], original: &DeformMesh, deformed: &[[f32; 2]]) -> [f32; 2] {
    if deformed.len() != original.vertices.len() || original.triangles.is_empty() {
        return point;
    }
    let mut best: Option<(usize, [f32; 3])> = None;
    let mut best_score = f32::INFINITY; // smallest "most-negative" bary coord
    for (ti, tri) in original.triangles.iter().enumerate() {
        let a = original.vertices[tri[0]];
        let b = original.vertices[tri[1]];
        let c = original.vertices[tri[2]];
        let Some(bc) = barycentric(point, a, b, c) else {
            continue;
        };
        let most_neg = bc[0].min(bc[1]).min(bc[2]);
        if most_neg >= -EPS {
            // Inside this triangle — reconstruct immediately.
            return map_bary(bc, tri, deformed);
        }
        // Track the closest triangle for the outside-the-mesh fallback.
        if -most_neg < best_score {
            best_score = -most_neg;
            best = Some((ti, bc));
        }
    }
    if let Some((ti, bc)) = best {
        let tri = &original.triangles[ti];
        return map_bary(clamp_bary(bc), tri, deformed);
    }
    point
}

/// Reconstruct a point from barycentric coordinates `bc` over triangle `tri`'s
/// `deformed` vertices.
fn map_bary(bc: [f32; 3], tri: &[usize; 3], deformed: &[[f32; 2]]) -> [f32; 2] {
    let a = deformed[tri[0]];
    let b = deformed[tri[1]];
    let c = deformed[tri[2]];
    [
        bc[0] * a[0] + bc[1] * b[0] + bc[2] * c[0],
        bc[0] * a[1] + bc[1] * b[1] + bc[2] * c[1],
    ]
}

/// Clamp barycentric coordinates into the simplex (each ≥ 0, summing to 1) for
/// the outside-the-mesh fallback in [`sample_warp`].
fn clamp_bary(bc: [f32; 3]) -> [f32; 3] {
    let c0 = bc[0].max(0.0);
    let c1 = bc[1].max(0.0);
    let c2 = bc[2].max(0.0);
    let s = c0 + c1 + c2;
    if s <= EPS {
        return [1.0, 0.0, 0.0];
    }
    [c0 / s, c1 / s, c2 / s]
}

// ── Action layer ─────────────────────────────────────────────────────────────

/// A puppet-mesh sub-action. Defined in this domain file and wrapped by the
/// single [`Action::PuppetMesh`](super::Action::PuppetMesh) variant so the
/// central `Action` enum stays small. All variants are app-side state edits (a
/// per-layer mesh map, not the `Project`), so — like particles — they are **not**
/// undoable.
#[derive(Clone, Debug, PartialEq)]
pub enum PuppetMeshAction {
    /// Build (or replace) a layer's grid deformation mesh over `bounds` with
    /// `resolution` cells per axis.
    AddMesh { layer_id: usize, bounds: [f32; 4], resolution: u32 },
    /// Remove a layer's mesh (and its pins).
    RemoveMesh { layer_id: usize },
    /// Rebuild a layer's grid at a new `resolution`, preserving its pins.
    SetResolution { layer_id: usize, resolution: u32 },
    /// Drop a pin at `pos` (rest = current = `pos`).
    AddPin { layer_id: usize, pos: [f32; 2] },
    /// Move an existing pin's current position (this is what warps the mesh).
    MovePin { layer_id: usize, pin_id: u64, pos: [f32; 2] },
    /// Delete a pin by id.
    DeletePin { layer_id: usize, pin_id: u64 },
    /// Set a pin's stiffness multiplier (clamped ≥ 0); a *Starch* pin.
    SetPinStiffness { layer_id: usize, pin_id: u64, stiffness: f32 },
}

impl App {
    /// Apply a puppet-mesh [`Action`] (dispatched from [`App::apply`] via the
    /// [`Action::PuppetMesh`](super::Action::PuppetMesh) wrapper).
    pub(super) fn apply_puppet_mesh(&mut self, action: Action) {
        let Action::PuppetMesh(pa) = action else {
            unreachable!("apply_puppet_mesh called with wrong action");
        };
        match pa {
            PuppetMeshAction::AddMesh { layer_id, bounds, resolution } => {
                self.puppet_deform_meshes
                    .insert(layer_id, grid_mesh(layer_id, bounds, resolution));
                self.host.mark_dirty();
            }
            PuppetMeshAction::RemoveMesh { layer_id } => {
                self.puppet_deform_meshes.remove(&layer_id);
                self.host.mark_dirty();
            }
            PuppetMeshAction::SetResolution { layer_id, resolution } => {
                if let Some(mesh) = self.puppet_deform_meshes.get_mut(&layer_id) {
                    mesh.set_resolution(resolution);
                    self.host.mark_dirty();
                }
            }
            PuppetMeshAction::AddPin { layer_id, pos } => {
                if let Some(mesh) = self.puppet_deform_meshes.get_mut(&layer_id) {
                    mesh.add_pin(pos);
                    self.host.mark_dirty();
                }
            }
            PuppetMeshAction::MovePin { layer_id, pin_id, pos } => {
                if let Some(mesh) = self.puppet_deform_meshes.get_mut(&layer_id) {
                    if let Some(p) = mesh.pins.iter_mut().find(|p| p.id == pin_id) {
                        p.pos = pos;
                        self.host.mark_dirty();
                    }
                }
            }
            PuppetMeshAction::DeletePin { layer_id, pin_id } => {
                if let Some(mesh) = self.puppet_deform_meshes.get_mut(&layer_id) {
                    mesh.pins.retain(|p| p.id != pin_id);
                    self.host.mark_dirty();
                }
            }
            PuppetMeshAction::SetPinStiffness { layer_id, pin_id, stiffness } => {
                if let Some(mesh) = self.puppet_deform_meshes.get_mut(&layer_id) {
                    if let Some(p) = mesh.pins.iter_mut().find(|p| p.id == pin_id) {
                        p.stiffness = stiffness.max(0.0);
                        self.host.mark_dirty();
                    }
                }
            }
        }
    }

    /// The deformed vertex positions for `layer_id`'s mesh, or `None` if the
    /// layer has no puppet mesh. The compositor/preview calls this to draw the
    /// warped layer.
    pub fn deform_layer_mesh(&self, layer_id: usize) -> Option<Vec<[f32; 2]>> {
        self.puppet_deform_meshes
            .get(&layer_id)
            .map(|m| m.deformed_vertices())
    }

    /// Map a single layer-local `point` through `layer_id`'s puppet deformation,
    /// or `None` if the layer has no mesh. See [`sample_warp`].
    pub fn warp_layer_point(&self, layer_id: usize, point: [f32; 2]) -> Option<[f32; 2]> {
        let mesh = self.puppet_deform_meshes.get(&layer_id)?;
        let deformed = mesh.deformed_vertices();
        Some(sample_warp(point, mesh, &deformed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100×100 mesh centered on the origin (`[-50, 50]²`) with the given
    /// resolution — the canonical fixture for the geometry tests.
    fn centered(res: u32) -> DeformMesh {
        grid_mesh(0, [-50.0, -50.0, 50.0, 50.0], res)
    }

    fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }

    // ── Mesh construction ──────────────────────────────────────────────────────

    #[test]
    fn grid_vertex_and_triangle_counts() {
        let m1 = centered(1);
        assert_eq!(m1.vertices.len(), 4, "res 1 ⇒ 2×2 vertices");
        assert_eq!(m1.triangles.len(), 2, "res 1 ⇒ 2 triangles");
        let m2 = centered(2);
        assert_eq!(m2.vertices.len(), 9);
        assert_eq!(m2.triangles.len(), 8);
        let m4 = centered(4);
        assert_eq!(m4.vertices.len(), 25);
        assert_eq!(m4.triangles.len(), 32);
    }

    #[test]
    fn grid_resolution_clamped_to_one() {
        let m = grid_mesh(0, [0.0, 0.0, 10.0, 10.0], 0);
        assert_eq!(m.resolution, 1);
        assert_eq!(m.vertices.len(), 4);
    }

    #[test]
    fn grid_covers_bounds_corners() {
        let m = centered(4);
        // First vertex is the min corner; last is the max corner.
        assert_eq!(m.vertices[0], [-50.0, -50.0]);
        assert_eq!(*m.vertices.last().unwrap(), [50.0, 50.0]);
    }

    #[test]
    fn grid_normalizes_inverted_bounds() {
        let m = grid_mesh(0, [50.0, 50.0, -50.0, -50.0], 2);
        assert_eq!(m.bounds, [-50.0, -50.0, 50.0, 50.0]);
        assert_eq!(m.vertices[0], [-50.0, -50.0]);
    }

    #[test]
    fn grid_falloff_is_half_diagonal() {
        let m = centered(2);
        let diag = (100.0f32 * 100.0 + 100.0 * 100.0).sqrt();
        assert!((m.falloff - 0.5 * diag).abs() < 1e-3);
    }

    #[test]
    fn every_triangle_indexes_valid_vertices() {
        let m = centered(3);
        for t in &m.triangles {
            for &i in t {
                assert!(i < m.vertices.len());
            }
        }
    }

    // ── Identity ───────────────────────────────────────────────────────────────

    #[test]
    fn zero_displacement_is_identity() {
        let m = centered(3);
        let out = deform_mesh(&m, &[]);
        assert_eq!(out, m.vertices, "no pins ⇒ vertices unchanged");
    }

    #[test]
    fn zero_delta_pins_is_identity() {
        let mut m = centered(3);
        m.add_pin([0.0, 0.0]);
        m.add_pin([25.0, 10.0]); // pins present but not moved
        let out = m.deformed_vertices();
        for (o, v) in out.iter().zip(m.vertices.iter()) {
            assert!(dist(*o, *v) < 1e-4, "pins at rest ⇒ identity");
        }
    }

    // ── Single-pin behavior ────────────────────────────────────────────────────

    #[test]
    fn pin_on_vertex_moves_it_by_its_displacement() {
        let mut m = centered(4);
        // Vertex (0,0) exists at this resolution; pin exactly on it.
        let vi = m.vertices.iter().position(|v| *v == [0.0, 0.0]).unwrap();
        let id = m.add_pin([0.0, 0.0]);
        // Move the pin.
        m.pins.iter_mut().find(|p| p.id == id).unwrap().pos = [12.0, -7.0];
        let out = m.deformed_vertices();
        assert!(dist(out[vi], [12.0, -7.0]) < 1e-2, "vertex follows the pin on it");
    }

    #[test]
    fn single_pin_falloff_near_more_than_far() {
        let mut m = centered(4);
        let id = m.add_pin([0.0, 0.0]);
        m.pins.iter_mut().find(|p| p.id == id).unwrap().pos = [10.0, 0.0];
        let out = m.deformed_vertices();
        let near = m.vertices.iter().position(|v| *v == [25.0, 0.0]).unwrap();
        let far = m.vertices.iter().position(|v| *v == [50.0, 50.0]).unwrap();
        let move_near = dist(out[near], m.vertices[near]);
        let move_far = dist(out[far], m.vertices[far]);
        assert!(move_near > move_far, "near {move_near} should exceed far {move_far}");
        assert!(move_far > 0.0, "far vertex still moves a little");
    }

    #[test]
    fn single_pin_moves_in_delta_direction() {
        let mut m = centered(4);
        let id = m.add_pin([0.0, 0.0]);
        m.pins.iter_mut().find(|p| p.id == id).unwrap().pos = [10.0, 0.0];
        let out = m.deformed_vertices();
        // Every vertex drifts toward +x (same sign as the delta), none against it.
        for (o, v) in out.iter().zip(m.vertices.iter()) {
            assert!(o[0] - v[0] >= -1e-4, "no vertex moves against +x delta");
        }
    }

    #[test]
    fn larger_falloff_means_stiffer_broad_influence() {
        let make = |falloff: f32| {
            let mut m = centered(4);
            m.falloff = falloff;
            let id = m.add_pin([0.0, 0.0]);
            m.pins.iter_mut().find(|p| p.id == id).unwrap().pos = [10.0, 0.0];
            let out = m.deformed_vertices();
            let far = m.vertices.iter().position(|v| *v == [50.0, 50.0]).unwrap();
            dist(out[far], m.vertices[far])
        };
        assert!(make(200.0) > make(20.0), "bigger falloff ⇒ far vertices follow more");
    }

    // ── Two-pin blending ───────────────────────────────────────────────────────

    #[test]
    fn two_pins_blend_symmetrically_at_midpoint() {
        let mut m = centered(4);
        let a = m.add_pin([-25.0, 0.0]);
        let b = m.add_pin([25.0, 0.0]);
        m.pins.iter_mut().find(|p| p.id == a).unwrap().pos = [-25.0 + 10.0, 0.0]; // +x
        m.pins.iter_mut().find(|p| p.id == b).unwrap().pos = [25.0, 10.0]; // +y
        let out = m.deformed_vertices();
        let mid = m.vertices.iter().position(|v| *v == [0.0, 0.0]).unwrap();
        let dx = out[mid][0] - m.vertices[mid][0];
        let dy = out[mid][1] - m.vertices[mid][1];
        assert!((dx - dy).abs() < 1e-3, "equidistant pins blend equally: dx {dx} ≈ dy {dy}");
        assert!(dx > 0.0 && dx < 10.0, "midpoint blends between the two displacements");
    }

    #[test]
    fn two_equal_pins_translate_between_them() {
        let mut m = centered(4);
        let a = m.add_pin([-25.0, 0.0]);
        let b = m.add_pin([25.0, 0.0]);
        // Both pushed +x by 10 ⇒ the span between them translates ~10 in x.
        for id in [a, b] {
            let p = m.pins.iter_mut().find(|p| p.id == id).unwrap();
            p.pos = [p.rest[0] + 10.0, p.rest[1]];
        }
        let out = m.deformed_vertices();
        let mid = m.vertices.iter().position(|v| *v == [0.0, 0.0]).unwrap();
        let dx = out[mid][0] - m.vertices[mid][0];
        assert!(dx > 8.0 && dx <= 10.5, "midpoint of two equal +x pins moves ~10, got {dx}");
    }

    // ── Stiffness / starch ─────────────────────────────────────────────────────

    #[test]
    fn zero_stiffness_pin_has_no_influence() {
        let mut m = centered(3);
        let id = m.add_pin([0.0, 0.0]);
        {
            let p = m.pins.iter_mut().find(|p| p.id == id).unwrap();
            p.pos = [20.0, 20.0];
            p.stiffness = 0.0;
        }
        let out = m.deformed_vertices();
        for (o, v) in out.iter().zip(m.vertices.iter()) {
            assert!(dist(*o, *v) < 1e-4, "stiffness 0 ⇒ pin disabled");
        }
    }

    #[test]
    fn starch_pin_holds_its_region_rigid() {
        // A moved pin near the left edge, and a starch pin (delta 0) near a probe
        // vertex. With the starch pin present the probe should move less.
        let probe_at = [25.0, 0.0];
        let moved = |starch_stiffness: Option<f32>| {
            let mut m = centered(4);
            let mv = m.add_pin([-25.0, 0.0]);
            m.pins.iter_mut().find(|p| p.id == mv).unwrap().pos = [-25.0, 30.0];
            if let Some(s) = starch_stiffness {
                let st = m.add_pin(probe_at); // delta 0, holds the area
                m.pins.iter_mut().find(|p| p.id == st).unwrap().stiffness = s;
            }
            let out = m.deformed_vertices();
            let pi = m.vertices.iter().position(|v| *v == probe_at).unwrap();
            dist(out[pi], m.vertices[pi])
        };
        let free = moved(None);
        let starched = moved(Some(8.0));
        assert!(starched < free, "starch pin reduces motion: {starched} < {free}");
    }

    // ── Determinism ────────────────────────────────────────────────────────────

    #[test]
    fn deform_is_deterministic() {
        let mut m = centered(5);
        m.add_pin([10.0, -10.0]);
        m.add_pin([-30.0, 20.0]);
        m.pins[0].pos = [15.0, -5.0];
        m.pins[1].pos = [-25.0, 25.0];
        assert_eq!(m.deformed_vertices(), m.deformed_vertices());
    }

    // ── Barycentric ────────────────────────────────────────────────────────────

    #[test]
    fn barycentric_inside_sums_to_one() {
        let bc = barycentric([0.25, 0.25], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]).unwrap();
        assert!((bc[0] + bc[1] + bc[2] - 1.0).abs() < 1e-5);
        assert!(bc.iter().all(|&c| c >= -1e-6), "interior point ⇒ all ≥ 0");
    }

    #[test]
    fn barycentric_centroid_is_thirds() {
        let bc = barycentric([1.0 / 3.0, 1.0 / 3.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]).unwrap();
        for c in bc {
            assert!((c - 1.0 / 3.0).abs() < 1e-5);
        }
    }

    #[test]
    fn barycentric_outside_has_negative() {
        let bc = barycentric([2.0, 2.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]).unwrap();
        assert!(bc.iter().any(|&c| c < 0.0), "exterior point ⇒ a negative coord");
    }

    #[test]
    fn barycentric_degenerate_is_none() {
        // Collinear triangle ⇒ zero area ⇒ None.
        assert!(barycentric([0.5, 0.0], [0.0, 0.0], [1.0, 0.0], [2.0, 0.0]).is_none());
    }

    #[test]
    fn find_triangle_locates_interior_and_rejects_exterior() {
        let m = centered(2);
        assert!(m.find_triangle([0.0, 0.0]).is_some(), "origin is inside");
        assert!(m.find_triangle([1000.0, 1000.0]).is_none(), "far point is outside");
    }

    // ── sample_warp ────────────────────────────────────────────────────────────

    #[test]
    fn sample_warp_identity_when_undeformed() {
        let m = centered(3);
        let deformed = m.vertices.clone(); // no deformation
        let p = [13.0, -8.0];
        let out = sample_warp(p, &m, &deformed);
        assert!(dist(out, p) < 1e-4, "rest == deformed ⇒ point unchanged");
    }

    #[test]
    fn sample_warp_translates_interior_points() {
        // A uniform translation of every vertex ⇒ every interior point shifts by
        // the same delta (barycentric weights sum to 1).
        let m = centered(3);
        let delta = [7.0, -4.0];
        let deformed: Vec<[f32; 2]> =
            m.vertices.iter().map(|v| [v[0] + delta[0], v[1] + delta[1]]).collect();
        for p in [[0.0, 0.0], [12.5, 3.0], [-30.0, -20.0]] {
            let out = sample_warp(p, &m, &deformed);
            assert!(dist(out, [p[0] + delta[0], p[1] + delta[1]]) < 1e-3, "p {p:?} → +delta");
        }
    }

    #[test]
    fn sample_warp_maps_triangle_centroid_to_deformed_centroid() {
        let m = centered(2);
        // Deform only triangle 0's three vertices to known new positions.
        let tri = m.triangles[0];
        let mut deformed = m.vertices.clone();
        deformed[tri[0]] = [100.0, 100.0];
        deformed[tri[1]] = [200.0, 110.0];
        deformed[tri[2]] = [150.0, 250.0];
        // Centroid of rest triangle 0 (strictly interior to triangle 0).
        let a = m.vertices[tri[0]];
        let b = m.vertices[tri[1]];
        let c = m.vertices[tri[2]];
        let centroid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
        let expect = [
            (100.0 + 200.0 + 150.0) / 3.0,
            (100.0 + 110.0 + 250.0) / 3.0,
        ];
        let out = sample_warp(centroid, &m, &deformed);
        assert!(dist(out, expect) < 1e-2, "centroid maps to deformed centroid: {out:?} vs {expect:?}");
    }

    #[test]
    fn sample_warp_outside_falls_back_total() {
        let m = centered(2);
        let deformed = m.vertices.clone();
        // Far outside the mesh: returns a finite, sensible value (no panic / NaN).
        let out = sample_warp([1000.0, -1000.0], &m, &deformed);
        assert!(out[0].is_finite() && out[1].is_finite());
    }

    #[test]
    fn sample_warp_bad_lengths_returns_point() {
        let m = centered(2);
        let out = sample_warp([1.0, 2.0], &m, &[[0.0, 0.0]]);
        assert_eq!(out, [1.0, 2.0], "mismatched deformed length ⇒ identity");
    }

    // ── App action layer ───────────────────────────────────────────────────────

    #[test]
    fn app_add_and_remove_mesh() {
        let mut app = App::new();
        assert!(app.deform_layer_mesh(0).is_none());
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [-50.0, -50.0, 50.0, 50.0],
            resolution: 3,
        }));
        assert_eq!(app.deform_layer_mesh(0).unwrap().len(), 16, "4×4 vertices");
        app.apply(Action::PuppetMesh(PuppetMeshAction::RemoveMesh { layer_id: 0 }));
        assert!(app.deform_layer_mesh(0).is_none());
    }

    #[test]
    fn app_add_pins_get_distinct_ids() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 1,
            bounds: [0.0, 0.0, 100.0, 100.0],
            resolution: 2,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 1, pos: [10.0, 10.0] }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 1, pos: [50.0, 50.0] }));
        let mesh = &app.puppet_deform_meshes[&1];
        assert_eq!(mesh.pins.len(), 2);
        assert_ne!(mesh.pins[0].id, mesh.pins[1].id, "ids are distinct");
    }

    #[test]
    fn app_move_pin_deforms_mesh() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [-50.0, -50.0, 50.0, 50.0],
            resolution: 4,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 0, pos: [0.0, 0.0] }));
        let rest = app.deform_layer_mesh(0).unwrap();
        let pin_id = app.puppet_deform_meshes[&0].pins[0].id;
        app.apply(Action::PuppetMesh(PuppetMeshAction::MovePin {
            layer_id: 0,
            pin_id,
            pos: [20.0, 0.0],
        }));
        let moved = app.deform_layer_mesh(0).unwrap();
        let changed = rest.iter().zip(moved.iter()).any(|(a, b)| dist(*a, *b) > 1.0);
        assert!(changed, "moving a pin changes the mesh");
    }

    #[test]
    fn app_delete_pin() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [0.0, 0.0, 100.0, 100.0],
            resolution: 2,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 0, pos: [10.0, 10.0] }));
        let pin_id = app.puppet_deform_meshes[&0].pins[0].id;
        app.apply(Action::PuppetMesh(PuppetMeshAction::DeletePin { layer_id: 0, pin_id }));
        assert!(app.puppet_deform_meshes[&0].pins.is_empty());
    }

    #[test]
    fn app_set_pin_stiffness_clamps_non_negative() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [0.0, 0.0, 100.0, 100.0],
            resolution: 2,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 0, pos: [10.0, 10.0] }));
        let pin_id = app.puppet_deform_meshes[&0].pins[0].id;
        app.apply(Action::PuppetMesh(PuppetMeshAction::SetPinStiffness {
            layer_id: 0,
            pin_id,
            stiffness: -5.0,
        }));
        assert_eq!(app.puppet_deform_meshes[&0].pins[0].stiffness, 0.0);
        app.apply(Action::PuppetMesh(PuppetMeshAction::SetPinStiffness {
            layer_id: 0,
            pin_id,
            stiffness: 4.0,
        }));
        assert_eq!(app.puppet_deform_meshes[&0].pins[0].stiffness, 4.0);
    }

    #[test]
    fn app_set_resolution_changes_vertices_keeps_pins() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [-50.0, -50.0, 50.0, 50.0],
            resolution: 2,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 0, pos: [0.0, 0.0] }));
        assert_eq!(app.deform_layer_mesh(0).unwrap().len(), 9);
        app.apply(Action::PuppetMesh(PuppetMeshAction::SetResolution { layer_id: 0, resolution: 5 }));
        assert_eq!(app.deform_layer_mesh(0).unwrap().len(), 36, "6×6 vertices");
        assert_eq!(app.puppet_deform_meshes[&0].pins.len(), 1, "pins preserved across resize");
    }

    #[test]
    fn app_warp_layer_point_identity_at_rest() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [-50.0, -50.0, 50.0, 50.0],
            resolution: 3,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 0, pos: [0.0, 0.0] }));
        let out = app.warp_layer_point(0, [10.0, 10.0]).unwrap();
        assert!(dist(out, [10.0, 10.0]) < 1e-3, "pin at rest ⇒ point unchanged");
        assert!(app.warp_layer_point(99, [0.0, 0.0]).is_none(), "no mesh ⇒ None");
    }

    #[test]
    fn app_warp_layer_point_follows_pin() {
        let mut app = App::new();
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [-50.0, -50.0, 50.0, 50.0],
            resolution: 4,
        }));
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddPin { layer_id: 0, pos: [0.0, 0.0] }));
        let pin_id = app.puppet_deform_meshes[&0].pins[0].id;
        app.apply(Action::PuppetMesh(PuppetMeshAction::MovePin {
            layer_id: 0,
            pin_id,
            pos: [15.0, 0.0],
        }));
        // A point right at the pin's rest tracks the pin closely.
        let out = app.warp_layer_point(0, [0.0, 0.0]).unwrap();
        assert!(out[0] > 5.0, "warped point follows the pin in +x, got {out:?}");
    }

    #[test]
    fn app_puppet_mesh_actions_are_not_undoable() {
        let mut app = App::new();
        assert!(!app.can_undo());
        app.apply(Action::PuppetMesh(PuppetMeshAction::AddMesh {
            layer_id: 0,
            bounds: [0.0, 0.0, 10.0, 10.0],
            resolution: 2,
        }));
        assert!(!app.can_undo(), "puppet-mesh edits don't push undo history");
    }
}
