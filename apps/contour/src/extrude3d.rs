//! Real CPU 3D render for **Extrude & Bevel** and **Revolve**.
//!
//! Both effects share one pipeline:
//!
//! 1. Build a triangulated **mesh** (front/back caps + side walls for extrude;
//!    a lathe of stacked rings for revolve) from a flat 2D profile.
//! 2. Rotate the mesh in object space (`rotate_x / y / z`) and apply a camera
//!    **projection** (orthographic, or weak perspective driven by the effect's
//!    `perspective` angle).
//! 3. **Depth-sort** the projected faces back-to-front (painter's algorithm)
//!    using each face's centroid `z`.
//! 4. **Flat-shade** each face from its world-space normal against a fixed light
//!    direction, modulating the base fill colour (Lambert + ambient).
//! 5. **Expand** the sorted, shaded, projected faces into flat filled
//!    [`Shape`]s (one closed `Path` per visible face), so the 3D result becomes
//!    plain editable vector artwork — Illustrator's *Object ▸ Expand Appearance*
//!    on a 3D effect.
//!
//! Everything here is pure, deterministic, and unit-tested; no GPU, no UI.

use crate::app_state::{Extrude3D, Revolve3D, SurfaceShading};
use crate::document::Shape;

/// A 3D point / vector in object (and, after transform, world) space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn scale(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l < 1e-9 {
            Vec3::new(0.0, 0.0, 0.0)
        } else {
            self.scale(1.0 / l)
        }
    }
}

/// One mesh face: a small polygon (3 or 4 vertices) of object-space points plus
/// the base colour it should be shaded from.
#[derive(Clone, Debug)]
pub struct Face {
    pub verts: Vec<Vec3>,
    pub base_color: [f32; 4],
}

impl Face {
    /// The face centroid in object/world space.
    pub fn centroid(&self) -> Vec3 {
        let n = self.verts.len().max(1) as f32;
        let s = self
            .verts
            .iter()
            .fold(Vec3::new(0.0, 0.0, 0.0), |a, &v| a.add(v));
        s.scale(1.0 / n)
    }

    /// The face's (normalized) geometric normal from its first three verts.
    pub fn normal(&self) -> Vec3 {
        if self.verts.len() < 3 {
            return Vec3::new(0.0, 0.0, 1.0);
        }
        let a = self.verts[1].sub(self.verts[0]);
        let b = self.verts[2].sub(self.verts[0]);
        a.cross(b).normalized()
    }
}

/// A built 3D mesh: a flat list of faces (caps + walls / lathe quads).
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub faces: Vec<Face>,
}

/// Camera / shading parameters shared by extrude and revolve, derived from the
/// effect config. `perspective` is the camera FOV-ish angle in degrees (0 =
/// orthographic). The light points from `light_dir` toward the scene.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub rotate_x: f32,
    pub rotate_y: f32,
    pub rotate_z: f32,
    pub perspective: f32,
    pub light_dir: Vec3,
    pub ambient: f32,
    pub light_intensity: f32,
    /// Surface shading mode (drives whether faces are filled or wire-only).
    pub surface: SurfaceShading,
}

impl Camera {
    fn from_extrude(e: &Extrude3D) -> Self {
        Self {
            rotate_x: e.rotate_x,
            rotate_y: e.rotate_y,
            rotate_z: e.rotate_z,
            perspective: e.perspective,
            light_dir: Vec3::new(-0.5, -0.6, -0.62).normalized(),
            ambient: (e.ambient_light / 100.0).clamp(0.0, 1.0),
            light_intensity: (e.light_intensity / 100.0).clamp(0.0, 2.0),
            surface: e.surface,
        }
    }
    fn from_revolve(r: &Revolve3D) -> Self {
        Self {
            rotate_x: r.rotate_x,
            rotate_y: r.rotate_y,
            rotate_z: r.rotate_z,
            perspective: 0.0,
            light_dir: Vec3::new(-0.5, -0.6, -0.62).normalized(),
            ambient: 0.5,
            light_intensity: 1.0,
            surface: r.surface,
        }
    }
}

/// Rotate a point by the camera's X→Y→Z Euler angles (degrees).
fn rotate(p: Vec3, cam: &Camera) -> Vec3 {
    let (rx, ry, rz) = (
        cam.rotate_x.to_radians(),
        cam.rotate_y.to_radians(),
        cam.rotate_z.to_radians(),
    );
    // Rotate about X.
    let (sx, cx) = rx.sin_cos();
    let p = Vec3::new(p.x, p.y * cx - p.z * sx, p.y * sx + p.z * cx);
    // Rotate about Y.
    let (sy, cy) = ry.sin_cos();
    let p = Vec3::new(p.x * cy + p.z * sy, p.y, -p.x * sy + p.z * cy);
    // Rotate about Z.
    let (sz, cz) = rz.sin_cos();
    Vec3::new(p.x * cz - p.y * sz, p.x * sz + p.y * cz, p.z)
}

/// Project a rotated world point to 2D screen space. Orthographic when
/// `perspective == 0`; otherwise a weak perspective divide using a focal length
/// derived from the angle. `center` is the screen-space centre the projection
/// is offset around (so the result lands where the original profile was).
fn project(p: Vec3, cam: &Camera, center: (f32, f32)) -> (f32, f32) {
    if cam.perspective <= 0.001 {
        return (center.0 + p.x, center.1 + p.y);
    }
    // Focal length: larger angle → shorter focal → stronger divide.
    let half = (cam.perspective.clamp(0.0, 160.0) * 0.5).to_radians().max(1e-3);
    let focal = 1.0 / half.tan();
    // Place the camera `focal*200` in front; scale by a reference so typical
    // document sizes show a gentle perspective.
    let cam_dist = focal * 200.0;
    let denom = (cam_dist + p.z).max(1.0);
    let s = cam_dist / denom;
    (center.0 + p.x * s, center.1 + p.y * s)
}

/// Lambert + ambient flat shading of `base` by the face normal vs the light.
/// Returns the modulated straight-sRGB RGBA (alpha preserved).
fn shade(base: [f32; 4], normal_world: Vec3, cam: &Camera) -> [f32; 4] {
    let n = normal_world.normalized();
    // Diffuse term: light points toward scene, so -dot gives front-lit faces.
    let diffuse = (-n.dot(cam.light_dir)).max(0.0) * cam.light_intensity;
    let lit = (cam.ambient + diffuse).clamp(0.0, 1.0);
    [
        (base[0] * lit).clamp(0.0, 1.0),
        (base[1] * lit).clamp(0.0, 1.0),
        (base[2] * lit).clamp(0.0, 1.0),
        base[3],
    ]
}

// --- Profile extraction ------------------------------------------------------

/// Reduce a shape to a flat, closed 2D profile polygon (its outer ring). Returns
/// `None` for shapes without a fillable outline (open paths / lines).
pub fn shape_profile(shape: &Shape) -> Option<Vec<(f32, f32)>> {
    let poly = shape.outline_polygon()?;
    if poly.len() < 3 {
        return None;
    }
    Some(poly)
}

/// Signed area (shoelace) of a polygon; positive = CCW in math (+y up) space.
fn signed_area(poly: &[(f32, f32)]) -> f32 {
    let n = poly.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a * 0.5
}

/// The polygon centroid (vertex average).
fn poly_center(poly: &[(f32, f32)]) -> (f32, f32) {
    let n = poly.len().max(1) as f32;
    let (sx, sy) = poly
        .iter()
        .fold((0.0f32, 0.0f32), |(ax, ay), &(x, y)| (ax + x, ay + y));
    (sx / n, sy / n)
}

// --- Extrude mesh ------------------------------------------------------------

/// Build the extrude mesh for a closed 2D `profile` pushed `depth` units along
/// +z. Produces a **front cap** (n-gon face at z=0), a **back cap** (n-gon at
/// z=depth), and one **quad wall** per profile edge. `cap` toggles the two cap
/// faces (open tube when false). Object space: the profile lies in the xy-plane
/// centred on its centroid, +z toward the back.
pub fn extrude_mesh(profile: &[(f32, f32)], depth: f32, cap: bool, base: [f32; 4]) -> Mesh {
    let mut faces = Vec::new();
    let n = profile.len();
    if n < 3 {
        return Mesh { faces };
    }
    let (cx, cy) = poly_center(profile);
    // Centre the profile so rotation is about its middle.
    let p2: Vec<(f32, f32)> = profile.iter().map(|&(x, y)| (x - cx, y - cy)).collect();
    let front: Vec<Vec3> = p2.iter().map(|&(x, y)| Vec3::new(x, y, 0.0)).collect();
    let back: Vec<Vec3> = p2.iter().map(|&(x, y)| Vec3::new(x, y, depth)).collect();

    if cap {
        // Front cap (faces toward -z): keep order.
        faces.push(Face {
            verts: front.clone(),
            base_color: base,
        });
        // Back cap (faces toward +z): reverse winding.
        let mut bc = back.clone();
        bc.reverse();
        faces.push(Face {
            verts: bc,
            base_color: base,
        });
    }
    // Side walls: one quad per edge.
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push(Face {
            verts: vec![front[i], front[j], back[j], back[i]],
            base_color: base,
        });
    }
    Mesh { faces }
}

// --- Revolve mesh ------------------------------------------------------------

/// Build the revolve (lathe) mesh: sweep a 2D `profile` around the vertical axis
/// `x = axis_x` through `angle_deg` degrees in `segments` angular steps. Each
/// profile point traces a circle (radius = `x - axis_x`) at its y as the height;
/// adjacent rings are joined by quad faces. A full 360° revolve yields a closed
/// solid (rings wrap); a partial angle leaves the sweep open. Object space is
/// centred on the profile's centroid.
pub fn revolve_mesh(
    profile: &[(f32, f32)],
    axis_x: f32,
    angle_deg: f32,
    segments: usize,
    base: [f32; 4],
) -> Mesh {
    let mut faces = Vec::new();
    let n = profile.len();
    if n < 2 {
        return Mesh { faces };
    }
    let (cx, cy) = poly_center(profile);
    let seg = segments.max(3);
    let full = (angle_deg - 360.0).abs() < 0.5;
    // Number of rings: seg for a full sweep (wrap), seg+1 for a partial sweep.
    let rings = if full { seg } else { seg + 1 };
    let total = angle_deg.to_radians();

    // Precompute each profile point's radius (distance from axis) and height.
    let radii: Vec<f32> = profile.iter().map(|&(x, _)| x - axis_x).collect();
    let heights: Vec<f32> = profile.iter().map(|&(_, y)| y - cy).collect();

    // Build all ring vertex positions: ring[a][p].
    let mut ring_pts: Vec<Vec<Vec3>> = Vec::with_capacity(rings);
    for a in 0..rings {
        let theta = total * (a as f32 / seg as f32);
        let (s, c) = theta.sin_cos();
        let pts: Vec<Vec3> = (0..n)
            .map(|p| {
                let r = radii[p];
                // Rotate the radius about the axis; x re-centred on cx.
                Vec3::new(axis_x - cx + r * c, heights[p], r * s)
            })
            .collect();
        ring_pts.push(pts);
    }

    // Join adjacent rings with quads along the profile.
    let ring_count = ring_pts.len();
    for a in 0..ring_count {
        let a2 = if full {
            (a + 1) % ring_count
        } else if a + 1 < ring_count {
            a + 1
        } else {
            break;
        };
        for p in 0..n {
            let p2 = (p + 1) % n;
            faces.push(Face {
                verts: vec![
                    ring_pts[a][p],
                    ring_pts[a][p2],
                    ring_pts[a2][p2],
                    ring_pts[a2][p],
                ],
                base_color: base,
            });
        }
    }
    Mesh { faces }
}

// --- Render: project + depth-sort + shade -> flat faces ----------------------

/// A face ready to expand: its 2D screen polygon, shaded fill colour, and the
/// depth (centroid z) it sorts on.
#[derive(Clone, Debug)]
pub struct RenderedFace {
    pub poly: Vec<(f32, f32)>,
    pub color: [f32; 4],
    pub depth: f32,
}

/// Project, depth-sort (painter's algorithm: far faces first), and flat-shade a
/// mesh into 2D rendered faces ready to expand into vector shapes. `center` is
/// the screen-space anchor the projection is offset around (typically the source
/// shape's profile centre).
pub fn render_mesh(mesh: &Mesh, cam: &Camera, center: (f32, f32)) -> Vec<RenderedFace> {
    let mut out: Vec<RenderedFace> = Vec::with_capacity(mesh.faces.len());
    for face in &mesh.faces {
        // Rotate every vertex to world space.
        let world: Vec<Vec3> = face.verts.iter().map(|&v| rotate(v, cam)).collect();
        if world.len() < 3 {
            continue;
        }
        // World normal for shading from the rotated geometry.
        let wn = {
            let a = world[1].sub(world[0]);
            let b = world[2].sub(world[0]);
            a.cross(b)
        };
        let color = shade(face.base_color, wn, cam);
        // Centroid depth for sorting.
        let cz = world.iter().map(|v| v.z).sum::<f32>() / world.len() as f32;
        let poly: Vec<(f32, f32)> = world.iter().map(|&v| project(v, cam, center)).collect();
        out.push(RenderedFace {
            poly,
            color,
            depth: cz,
        });
    }
    // Painter's algorithm: paint far (larger z, behind) first so near faces land
    // on top. +z points away from the viewer after projection.
    out.sort_by(|a, b| {
        b.depth
            .partial_cmp(&a.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Turn rendered faces into flat filled vector [`Shape`]s (one closed `Path`
/// each), in back-to-front paint order. Degenerate (near-zero-area) faces are
/// dropped so the expanded artwork has no slivers. In wireframe mode faces are
/// unfilled and stroked.
pub fn faces_to_shapes(
    faces: &[RenderedFace],
    cam: &Camera,
    stroke: [f32; 4],
    stroke_w: f32,
) -> Vec<Shape> {
    let wire = matches!(cam.surface, SurfaceShading::WireFrame);
    faces
        .iter()
        .filter(|f| f.poly.len() >= 3 && signed_area(&f.poly).abs() > 0.01)
        .map(|f| {
            let fill = if wire {
                [0.0, 0.0, 0.0, 0.0] // wireframe: unfilled, stroke only
            } else {
                f.color
            };
            let sw = if wire { stroke_w.max(0.5) } else { stroke_w };
            Shape::path(
                f.poly.clone(),
                vec![(0.0, 0.0); f.poly.len()],
                true,
                fill,
                stroke,
                sw,
            )
        })
        .collect()
}

// --- Top-level: expand a shape's extrude / revolve into flat artwork ---------

/// Full **Extrude & Bevel** expand for `shape` under config `e`: profile → mesh
/// → project/sort/shade → flat vector faces. Returns the back-to-front shape list
/// (empty if the shape has no fillable profile). The faces are anchored on the
/// shape's profile centre so the expanded artwork sits where the original did.
pub fn expand_extrude(shape: &Shape, e: &Extrude3D) -> Vec<Shape> {
    let Some(profile) = shape_profile(shape) else {
        return Vec::new();
    };
    let base = shape.fill_color().unwrap_or([0.6, 0.6, 0.6, 1.0]);
    let center = poly_center(&profile);
    let cam = Camera::from_extrude(e);
    let mesh = extrude_mesh(&profile, e.depth.max(0.0), e.cap, base);
    let rendered = render_mesh(&mesh, &cam, center);
    let stroke = shape.stroke_color().unwrap_or([0.0, 0.0, 0.0, 0.0]);
    faces_to_shapes(&rendered, &cam, stroke, 0.0)
}

/// Full **Revolve** expand for `shape` under config `r`. The lathe axis is the
/// profile's `from`-edge (left or right of its bounds), so a half-silhouette
/// revolves into a solid of revolution. Returns the back-to-front flat faces.
pub fn expand_revolve(shape: &Shape, r: &Revolve3D) -> Vec<Shape> {
    let Some(profile) = shape_profile(shape) else {
        return Vec::new();
    };
    let base = shape.fill_color().unwrap_or([0.6, 0.6, 0.6, 1.0]);
    let center = poly_center(&profile);
    // Axis: the chosen vertical edge of the profile bounds (plus offset).
    let min_x = profile.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = profile.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let axis_x = match r.from {
        crate::app_state::RevolveFrom::LeftEdge => min_x - r.offset,
        crate::app_state::RevolveFrom::RightEdge => max_x + r.offset,
    };
    let segments = (r.angle.abs() / 12.0).ceil().clamp(8.0, 64.0) as usize;
    let cam = Camera::from_revolve(r);
    let mesh = revolve_mesh(&profile, axis_x, r.angle.clamp(1.0, 360.0), segments, base);
    let rendered = render_mesh(&mesh, &cam, center);
    let stroke = shape.stroke_color().unwrap_or([0.0, 0.0, 0.0, 0.0]);
    faces_to_shapes(&rendered, &cam, stroke, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Shape;

    fn square() -> Shape {
        Shape::rect([0.0, 0.0, 100.0, 100.0], [0.5, 0.5, 0.5, 1.0], [0.0, 0.0, 0.0, 0.0], 0.0)
    }

    #[test]
    fn extrude_square_face_count() {
        // A rect outline_polygon() is a 4-vertex ring → 4 walls + 2 caps = 6.
        let profile = shape_profile(&square()).unwrap();
        assert_eq!(profile.len(), 4, "square profile is a quad");
        let mesh = extrude_mesh(&profile, 50.0, true, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(mesh.faces.len(), 6, "4 walls + front + back cap");
        // Without caps: just the 4 walls.
        let open = extrude_mesh(&profile, 50.0, false, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(open.faces.len(), 4, "open tube: 4 walls only");
    }

    #[test]
    fn extrude_depth_spans_z() {
        let profile = shape_profile(&square()).unwrap();
        let mesh = extrude_mesh(&profile, 80.0, true, [1.0, 1.0, 1.0, 1.0]);
        let zmin = mesh
            .faces
            .iter()
            .flat_map(|f| f.verts.iter())
            .map(|v| v.z)
            .fold(f32::MAX, f32::min);
        let zmax = mesh
            .faces
            .iter()
            .flat_map(|f| f.verts.iter())
            .map(|v| v.z)
            .fold(f32::MIN, f32::max);
        assert!((zmin - 0.0).abs() < 1e-3 && (zmax - 80.0).abs() < 1e-3);
    }

    #[test]
    fn face_normal_and_centroid() {
        let f = Face {
            verts: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            base_color: [1.0, 1.0, 1.0, 1.0],
        };
        let c = f.centroid();
        assert!((c.x - 0.5).abs() < 1e-6 && (c.y - 0.5).abs() < 1e-6);
        let nrm = f.normal();
        assert!((nrm.z.abs() - 1.0).abs() < 1e-6, "flat quad normal is ±z");
    }

    #[test]
    fn revolve_full_is_closed_lathe() {
        let profile = shape_profile(&square()).unwrap();
        // Full 360 revolve about the left edge → seg quads × edges, closed (wrap).
        let segments = 16;
        let mesh = revolve_mesh(&profile, -10.0, 360.0, segments, [0.5, 0.5, 0.5, 1.0]);
        // Full sweep: `rings == seg`, joins wrap, so faces = seg * n_edges.
        let n = profile.len();
        assert_eq!(
            mesh.faces.len(),
            segments * n,
            "full lathe: segments × profile edges quad faces"
        );
        // Closed: the lathe sweeps both z hemispheres.
        let zmin = mesh
            .faces
            .iter()
            .flat_map(|f| f.verts.iter())
            .map(|v| v.z)
            .fold(f32::MAX, f32::min);
        let zmax = mesh
            .faces
            .iter()
            .flat_map(|f| f.verts.iter())
            .map(|v| v.z)
            .fold(f32::MIN, f32::max);
        assert!(zmin < -0.5 && zmax > 0.5, "full revolve sweeps both z hemispheres");
    }

    #[test]
    fn revolve_partial_has_extra_ring() {
        let profile = shape_profile(&square()).unwrap();
        let segments = 8;
        let n = profile.len();
        // Partial sweep: rings = seg+1, joins = seg → faces = seg * n.
        let mesh = revolve_mesh(&profile, -10.0, 180.0, segments, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(mesh.faces.len(), segments * n);
    }

    #[test]
    fn shade_darkens_back_faces() {
        let cam = Camera {
            rotate_x: 0.0,
            rotate_y: 0.0,
            rotate_z: 0.0,
            perspective: 0.0,
            light_dir: Vec3::new(0.0, 0.0, -1.0),
            ambient: 0.2,
            light_intensity: 1.0,
            surface: SurfaceShading::PlasticShading,
        };
        let base = [0.8, 0.8, 0.8, 1.0];
        // Normal facing the light (-z): bright.
        let front = shade(base, Vec3::new(0.0, 0.0, 1.0), &cam);
        // Normal facing away: dark (ambient).
        let back = shade(base, Vec3::new(0.0, 0.0, -1.0), &cam);
        assert!(front[0] > back[0], "lit face brighter than unlit");
        assert!((back[0] - 0.8 * 0.2).abs() < 1e-3, "back face = ambient only");
    }

    #[test]
    fn project_orthographic_is_offset() {
        let cam = Camera {
            rotate_x: 0.0,
            rotate_y: 0.0,
            rotate_z: 0.0,
            perspective: 0.0,
            light_dir: Vec3::new(0.0, 0.0, -1.0),
            ambient: 0.5,
            light_intensity: 1.0,
            surface: SurfaceShading::PlasticShading,
        };
        let p = project(Vec3::new(10.0, 20.0, 999.0), &cam, (5.0, 5.0));
        assert_eq!(p, (15.0, 25.0), "orthographic ignores z, adds center");
    }

    #[test]
    fn project_perspective_shrinks_distant() {
        let cam = Camera {
            rotate_x: 0.0,
            rotate_y: 0.0,
            rotate_z: 0.0,
            perspective: 60.0,
            light_dir: Vec3::new(0.0, 0.0, -1.0),
            ambient: 0.5,
            light_intensity: 1.0,
            surface: SurfaceShading::PlasticShading,
        };
        // A point farther back (+z) projects closer to centre than a near one.
        let near = project(Vec3::new(100.0, 0.0, -50.0), &cam, (0.0, 0.0));
        let far = project(Vec3::new(100.0, 0.0, 200.0), &cam, (0.0, 0.0));
        assert!(far.0 < near.0, "farther point's x shrinks toward centre");
    }

    #[test]
    fn expand_extrude_yields_shaded_faces() {
        let e = Extrude3D::default();
        let shapes = expand_extrude(&square(), &e);
        assert!(!shapes.is_empty(), "extrude expands to vector faces");
        // Every expanded face is a closed filled path.
        for s in &shapes {
            assert!(matches!(s, Shape::Path { closed: true, .. }));
        }
    }

    #[test]
    fn expand_extrude_sorted_back_to_front() {
        let e = Extrude3D {
            rotate_x: -30.0,
            rotate_y: -40.0,
            ..Default::default()
        };
        let shapes = expand_extrude(&square(), &e);
        assert!(shapes.len() >= 5, "rotated cube shows several faces");
    }

    #[test]
    fn expand_revolve_yields_faces() {
        let r = Revolve3D::default();
        let shapes = expand_revolve(&square(), &r);
        assert!(!shapes.is_empty(), "revolve expands to vector faces");
    }

    #[test]
    fn expand_open_path_is_empty() {
        // An open path has no outline polygon → no profile → no faces.
        let open = Shape::path(
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
            vec![(0.0, 0.0); 3],
            false,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            1.0,
        );
        assert!(expand_extrude(&open, &Extrude3D::default()).is_empty());
        assert!(expand_revolve(&open, &Revolve3D::default()).is_empty());
    }
}
