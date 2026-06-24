//! **3D lights & materials** — point / spot / ambient lights with a Lambert +
//! Blinn-Phong shading model for a 3D layer's surface, implemented as
//! deterministic CPU kernels.
//!
//! A composition can hold several [`Light3D`]s (position, intensity, color, and
//! a kind that selects point / spot / ambient falloff). Each layer carries a
//! [`Material3D`] (ambient / diffuse / specular coefficients + shininess). The
//! [`shade`] function evaluates the classic local-illumination sum at a surface
//! point given its normal, the eye direction, and the light set.
//!
//! All math is self-contained `[f32; 3]` vector ops in free functions (no
//! dependency on the engine's `comp::Light`), so the shading is unit-testable
//! without an `App`. The `App` impl holds the comp light list + per-layer
//! materials and the panel actions — app-side storage, engine untouched.

use std::collections::HashMap;

use super::{App, Action};

// ── Tiny vec3 helpers (deterministic, no external math crate) ─────────────────

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
/// Normalize, returning `[0,0,0]` for a (near-)zero vector.
pub fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = length(a);
    if l < 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        scale(a, 1.0 / l)
    }
}

// ── Light & material model ────────────────────────────────────────────────────

/// What kind of light a [`Light3D`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    /// Omnidirectional point light with inverse-square falloff.
    Point,
    /// Cone light: point falloff gated by a cone around `spot_dir`.
    Spot,
    /// Uniform fill with no position dependence (only `ambient` matters).
    Ambient,
}

/// A single comp light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light3D {
    pub kind: LightKind,
    /// World position (comp space).
    pub position: [f32; 3],
    /// Linear RGB color.
    pub color: [f32; 3],
    /// Overall intensity multiplier.
    pub intensity: f32,
    /// Direction the spot points (unit-ish; normalized on use). Spot only.
    pub spot_dir: [f32; 3],
    /// Spot cone half-angle in degrees. Spot only.
    pub cone_angle_deg: f32,
    /// Spot edge softness (degrees of penumbra). Spot only.
    pub cone_feather_deg: f32,
}

impl Default for Light3D {
    fn default() -> Self {
        Self {
            kind: LightKind::Point,
            position: [0.0, 0.0, 500.0],
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            spot_dir: [0.0, 0.0, -1.0],
            cone_angle_deg: 45.0,
            cone_feather_deg: 10.0,
        }
    }
}

/// Per-layer material (Blinn-Phong coefficients).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material3D {
    /// Fraction of ambient light reflected.
    pub ambient: f32,
    /// Diffuse (Lambert) coefficient.
    pub diffuse: f32,
    /// Specular (Blinn-Phong) coefficient.
    pub specular: f32,
    /// Specular exponent (higher = tighter highlight).
    pub shininess: f32,
}

impl Default for Material3D {
    fn default() -> Self {
        Self { ambient: 0.1, diffuse: 0.8, specular: 0.4, shininess: 16.0 }
    }
}

/// The spot cone attenuation in `[0,1]` for a light→surface direction `l` (from
/// the *surface toward the light*). `1` inside the cone core, ramping to `0`
/// across the feather, `0` outside. Returns `1` for non-spot lights.
pub fn spot_attenuation(light: Light3D, surface_to_light: [f32; 3]) -> f32 {
    if light.kind != LightKind::Spot {
        return 1.0;
    }
    let dir = normalize(light.spot_dir);
    // The ray from the light to the surface is the negation of surface→light.
    let to_surface = scale(normalize(surface_to_light), -1.0);
    let cos_angle = dot(dir, to_surface).clamp(-1.0, 1.0);
    let core = light.cone_angle_deg.to_radians().cos();
    let edge = (light.cone_angle_deg + light.cone_feather_deg.max(0.0))
        .to_radians()
        .cos();
    if cos_angle >= core {
        1.0
    } else if cos_angle <= edge {
        0.0
    } else {
        // Ramp from edge (0) to core (1).
        ((cos_angle - edge) / (core - edge)).clamp(0.0, 1.0)
    }
}

/// Distance falloff (inverse-square, normalized so 1 unit ⇒ factor 1). Ambient
/// lights have no falloff (return 1).
pub fn distance_falloff(light: Light3D, distance: f32) -> f32 {
    if light.kind == LightKind::Ambient {
        return 1.0;
    }
    let d = distance.max(1.0);
    1.0 / (d * d) * 10_000.0 // reference radius 100 units ⇒ ~1
}

/// **Shade** a surface point: evaluate Lambert diffuse + Blinn-Phong specular for
/// every light, plus a single ambient term, and return the linear RGB result.
///
/// - `point` — surface position (comp space).
/// - `normal` — surface normal (need not be unit; normalized here).
/// - `eye` — camera/eye position (for the specular half-vector).
/// - `base_color` — the surface albedo (the layer's pixel color).
pub fn shade(
    point: [f32; 3],
    normal: [f32; 3],
    eye: [f32; 3],
    base_color: [f32; 3],
    material: Material3D,
    lights: &[Light3D],
) -> [f32; 3] {
    let n = normalize(normal);
    let view = normalize(sub(eye, point));
    let mut result = [0.0_f32; 3];

    for &light in lights {
        if light.kind == LightKind::Ambient {
            // Uniform ambient fill.
            let amb = scale(light.color, light.intensity * material.ambient);
            result = add(result, [amb[0] * base_color[0], amb[1] * base_color[1], amb[2] * base_color[2]]);
            continue;
        }

        let to_light = sub(light.position, point);
        let dist = length(to_light);
        let l = normalize(to_light);
        let n_dot_l = dot(n, l).max(0.0);
        if n_dot_l <= 0.0 {
            continue; // facing away — no diffuse/specular contribution.
        }

        let falloff = distance_falloff(light, dist);
        let spot = spot_attenuation(light, to_light);
        let radiance = scale(light.color, light.intensity * falloff * spot);

        // Lambert diffuse.
        let diff = n_dot_l * material.diffuse;
        result = add(
            result,
            [
                radiance[0] * base_color[0] * diff,
                radiance[1] * base_color[1] * diff,
                radiance[2] * base_color[2] * diff,
            ],
        );

        // Blinn-Phong specular (half-vector).
        let half = normalize(add(l, view));
        let n_dot_h = dot(n, half).max(0.0);
        let spec = n_dot_h.powf(material.shininess.max(1.0)) * material.specular;
        result = add(result, scale(radiance, spec));
    }

    result
}

impl App {
    /// Apply a 3D-lighting [`Action`]. Dispatched from [`App::apply`].
    pub(super) fn apply_lighting3d(&mut self, action: Action) {
        match action {
            Action::AddLight3D { kind } => {
                self.lights3d.push(Light3D { kind, ..Light3D::default() });
                self.host.mark_dirty();
            }
            Action::RemoveLight3D { index } => {
                if index < self.lights3d.len() {
                    self.lights3d.remove(index);
                    self.host.mark_dirty();
                }
            }
            Action::SetLight3DPosition { index, pos } => {
                if let Some(l) = self.lights3d.get_mut(index) {
                    l.position = pos;
                    self.host.mark_dirty();
                }
            }
            Action::SetLight3DColor { index, color } => {
                if let Some(l) = self.lights3d.get_mut(index) {
                    l.color = [color[0].max(0.0), color[1].max(0.0), color[2].max(0.0)];
                    self.host.mark_dirty();
                }
            }
            Action::SetLight3DIntensity { index, intensity } => {
                if let Some(l) = self.lights3d.get_mut(index) {
                    l.intensity = intensity.max(0.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetLight3DCone { index, angle_deg, feather_deg } => {
                if let Some(l) = self.lights3d.get_mut(index) {
                    l.cone_angle_deg = angle_deg.clamp(0.0, 180.0);
                    l.cone_feather_deg = feather_deg.max(0.0);
                    self.host.mark_dirty();
                }
            }
            Action::SetMaterial3D { layer_id, param, value } => {
                let m = self.materials3d.entry(layer_id).or_default();
                match param {
                    "ambient" => m.ambient = value.clamp(0.0, 1.0),
                    "diffuse" => m.diffuse = value.clamp(0.0, 1.0),
                    "specular" => m.specular = value.clamp(0.0, 1.0),
                    "shininess" => m.shininess = value.max(1.0),
                    _ => {}
                }
                self.host.mark_dirty();
            }
            _ => unreachable!("apply_lighting3d called with wrong action"),
        }
    }

    /// Shade a layer's pixel with the comp light set + the layer's material.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shade_layer_point(
        &self,
        layer_id: usize,
        point: [f32; 3],
        normal: [f32; 3],
        eye: [f32; 3],
        base_color: [f32; 3],
    ) -> [f32; 3] {
        let mat = self.materials3d.get(&layer_id).copied().unwrap_or_default();
        shade(point, normal, eye, base_color, mat, &self.lights3d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point_light_at(pos: [f32; 3]) -> Light3D {
        Light3D { kind: LightKind::Point, position: pos, intensity: 1.0, ..Default::default() }
    }

    #[test]
    fn blinn_phong_brighter_facing_the_light() {
        // A surface whose normal points at the light is brighter than one angled
        // away. Light directly above (+Z), surface at origin.
        let light = point_light_at([0.0, 0.0, 100.0]);
        let mat = Material3D { ambient: 0.0, diffuse: 1.0, specular: 0.0, shininess: 16.0 };
        let eye = [0.0, 0.0, 100.0];
        let facing = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], eye, [1.0, 1.0, 1.0], mat, &[light]);
        let tilted = shade([0.0, 0.0, 0.0], normalize([1.0, 0.0, 0.3]), eye, [1.0, 1.0, 1.0], mat, &[light]);
        assert!(facing[0] > tilted[0], "facing light is brighter: {facing:?} vs {tilted:?}");
    }

    #[test]
    fn surface_facing_away_gets_no_diffuse() {
        let light = point_light_at([0.0, 0.0, 100.0]);
        let mat = Material3D { ambient: 0.0, diffuse: 1.0, specular: 0.0, shininess: 16.0 };
        // Normal points away from the light (−Z).
        let c = shade([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0], mat, &[light]);
        assert!(c[0].abs() < 1e-5, "back-facing ⇒ no light, got {c:?}");
    }

    #[test]
    fn ambient_lights_everything_uniformly() {
        let amb = Light3D { kind: LightKind::Ambient, color: [1.0, 1.0, 1.0], intensity: 1.0, ..Default::default() };
        let mat = Material3D { ambient: 0.5, diffuse: 0.0, specular: 0.0, shininess: 1.0 };
        // Even a back-facing surface receives ambient.
        let c = shade([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, 1.0], [0.6, 0.6, 0.6], mat, &[amb]);
        assert!((c[0] - 0.3).abs() < 1e-5, "ambient = base*ambient*intensity, got {c:?}");
    }

    #[test]
    fn specular_highlight_present_at_mirror_angle() {
        // Light and eye both above the surface ⇒ a strong specular highlight.
        let light = point_light_at([0.0, 0.0, 100.0]);
        let mat = Material3D { ambient: 0.0, diffuse: 0.0, specular: 1.0, shininess: 8.0 };
        let c = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0], mat, &[light]);
        assert!(c[0] > 0.0, "specular highlight present, got {c:?}");
    }

    #[test]
    fn spot_inside_cone_lights_outside_dark() {
        // A spot pointing straight down −Z; surface below it on the axis is lit,
        // surface far off-axis is not.
        let spot = Light3D {
            kind: LightKind::Spot,
            position: [0.0, 0.0, 100.0],
            spot_dir: [0.0, 0.0, -1.0],
            cone_angle_deg: 20.0,
            cone_feather_deg: 2.0,
            intensity: 1.0,
            ..Default::default()
        };
        let mat = Material3D { ambient: 0.0, diffuse: 1.0, specular: 0.0, shininess: 1.0 };
        let on_axis = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0], mat, &[spot]);
        let off_axis = shade([500.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0], mat, &[spot]);
        assert!(on_axis[0] > 0.0, "on-axis lit, got {on_axis:?}");
        assert!(off_axis[0].abs() < 1e-5, "off-axis dark, got {off_axis:?}");
    }

    #[test]
    fn distance_falloff_decreases_with_distance() {
        let near = point_light_at([0.0, 0.0, 50.0]);
        let far = point_light_at([0.0, 0.0, 400.0]);
        let mat = Material3D { ambient: 0.0, diffuse: 1.0, specular: 0.0, shininess: 1.0 };
        let cn = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 10.0], [1.0, 1.0, 1.0], mat, &[near]);
        let cf = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 10.0], [1.0, 1.0, 1.0], mat, &[far]);
        assert!(cn[0] > cf[0], "nearer light is brighter: {cn:?} vs {cf:?}");
    }

    #[test]
    fn intensity_scales_brightness() {
        let dim = point_light_at([0.0, 0.0, 100.0]);
        let bright = Light3D { intensity: 3.0, ..dim };
        let mat = Material3D { ambient: 0.0, diffuse: 1.0, specular: 0.0, shininess: 1.0 };
        let cd = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0], mat, &[dim]);
        let cb = shade([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0], mat, &[bright]);
        assert!(cb[0] > cd[0] * 2.5, "3x intensity ⇒ ~3x brighter: {cb:?} vs {cd:?}");
    }

    #[test]
    fn normalize_zero_is_zero() {
        assert_eq!(normalize([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn light_and_material_actions() {
        let mut app = App::new();
        app.apply(Action::AddLight3D { kind: LightKind::Point });
        app.apply(Action::AddLight3D { kind: LightKind::Ambient });
        assert_eq!(app.lights3d.len(), 2);
        app.apply(Action::SetLight3DIntensity { index: 0, intensity: -5.0 });
        assert_eq!(app.lights3d[0].intensity, 0.0, "intensity clamps to >= 0");
        app.apply(Action::SetLight3DPosition { index: 0, pos: [1.0, 2.0, 3.0] });
        assert_eq!(app.lights3d[0].position, [1.0, 2.0, 3.0]);
        app.apply(Action::SetLight3DCone { index: 0, angle_deg: 30.0, feather_deg: 5.0 });
        assert_eq!(app.lights3d[0].cone_angle_deg, 30.0);

        app.apply(Action::SetMaterial3D { layer_id: 0, param: "shininess", value: 0.5 });
        assert!(app.materials3d[&0].shininess >= 1.0, "shininess clamps to >= 1");

        app.apply(Action::RemoveLight3D { index: 0 });
        assert_eq!(app.lights3d.len(), 1);
    }

    #[test]
    fn shade_layer_point_through_app() {
        let mut app = App::new();
        app.apply(Action::AddLight3D { kind: LightKind::Point });
        app.apply(Action::SetLight3DPosition { index: 0, pos: [0.0, 0.0, 100.0] });
        let c = app.shade_layer_point(0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 100.0], [1.0, 1.0, 1.0]);
        assert!(c[0] > 0.0, "lit through the app default material, got {c:?}");
    }
}

/// Type alias used by the `App` field declaration in `mod.rs`.
pub type Material3DMap = HashMap<usize, Material3D>;
