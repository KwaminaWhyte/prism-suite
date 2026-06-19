//! Comp **lights** and the pure **Lambert shading** that lights 3-D layers.
//!
//! Pulse's compositor is a flat 2-D rasterizer; [`camera`](super::camera) lifts
//! layers into 3-D and projects them. This module adds the other half of a 3-D
//! look — **lighting**. A comp carries a list of [`Light`]s (a `serde`-defaulted
//! empty list, so a pre-lighting `.pulse` file loads with none). A 3-D layer that
//! opts in (`accepts_lights`) has its pixels **modulated by an RGB illumination
//! factor**: an **ambient** floor plus, for each **point** light, a Lambert
//! diffuse term `color × intensity × max(0, N·L)`.
//!
//! ## Coordinate frame
//!
//! Light positions live in the same comp 3-D space as the camera and layers
//! (origin at comp center, `+x` right, `+y` **down**, `+z` **into** the screen —
//! see [`camera`](super::camera)). A flat un-oriented 3-D layer at `z = 0` faces
//! the camera (which sits on `-z`), so its surface **normal** is `[0, 0, -1]`
//! (toward the viewer); the layer's X/Y/Z **orientation** rotates that normal the
//! same way it rotates the layer's plane.
//!
//! ## Back-compat
//!
//! The whole feature is **opt-in twice**: a comp with no lights, or a layer with
//! `accepts_lights = false` (the default), is never modulated — the illumination
//! factor is exactly `[1, 1, 1]`, so the layer renders byte-identically to today.
//! Lighting only ever multiplies a layer's *own* pixels in its isolated buffer;
//! it never touches 2-D layers, the accumulator, or anything that doesn't opt in.
//!
//! ## Distance falloff
//!
//! [`Point`](LightKind::Point) and [`Spot`](LightKind::Spot) lights carry an
//! optional [`falloff`](Light::falloff) radius. With the default `0.0` there is
//! **no attenuation** (the light reaches infinitely far, matching the original
//! Point shading byte-for-byte). With a positive radius the contribution is
//! attenuated by a smooth inverse-square-ish factor `(r / (r + d))²` where `d` is
//! the surface→light distance and `r` the falloff radius — `1.0` at the light,
//! decaying toward `0` with distance.
//!
//! ## Out of scope (follow-ups)
//!
//! **Shadows** (shadow catcher) and **specular** material response (Blinn-Phong
//! highlight) are noted in `PLAN.md` and not implemented here.

use serde::{Deserialize, Serialize};

/// The kind of a comp [`Light`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightKind {
    /// A uniform, position-independent fill that lights every surface equally —
    /// the diffuse **floor**. (No `N·L` term; ignores [`Light::position`].)
    Ambient,
    /// An omnidirectional point source at [`Light::position`]: shades a surface by
    /// Lambert diffuse `max(0, N·L)` where `L` points from the surface toward the
    /// light. Honors optional distance [`falloff`](Light::falloff).
    Point,
    /// A cone-restricted source at [`Light::position`] aimed along
    /// [`Light::direction`]: like [`Point`](LightKind::Point) but the intensity is
    /// gated by a **cone**. Inside the inner cone (half-angle
    /// [`cone_angle`](Light::cone_angle)) it's full; across the
    /// [`penumbra`](Light::penumbra) ring it ramps smoothly to zero; outside it's
    /// dark. Honors optional distance [`falloff`](Light::falloff).
    Spot,
    /// A **directional** (parallel) source: all rays share the fixed world
    /// direction [`Light::direction`], independent of position. Lambert diffuse
    /// `max(0, N·L)` with `L = −direction` (the direction toward the light). No
    /// distance falloff (the source is infinitely far).
    Parallel,
}

impl LightKind {
    /// A short human label for the UI / pickers.
    pub fn label(self) -> &'static str {
        match self {
            LightKind::Ambient => "Ambient",
            LightKind::Point => "Point",
            LightKind::Spot => "Spot",
            LightKind::Parallel => "Parallel",
        }
    }
}

/// One comp light. Lights shade only [`accepts_lights`](super::PulseLayer)-opted
/// **3-D layers**; an empty light list (the comp default) leaves every layer
/// unlit (illumination factor `[1, 1, 1]`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// Ambient / point / spot / parallel.
    pub kind: LightKind,
    /// Comp-space position (meaningful for [`Point`](LightKind::Point) and
    /// [`Spot`](LightKind::Spot); ignored by Ambient and Parallel).
    #[serde(default)]
    pub position: [f32; 3],
    /// Light color (linear-ish straight RGB, 0..=1) — multiplied into the
    /// diffuse term per channel so a colored light tints the surface.
    #[serde(default = "Light::default_color")]
    pub color: [f32; 3],
    /// Scalar brightness multiplier (1.0 = full). Scales the whole contribution.
    #[serde(default = "Light::default_intensity")]
    pub intensity: f32,
    /// Aim direction in comp space (used by [`Spot`](LightKind::Spot) — the cone
    /// axis — and [`Parallel`](LightKind::Parallel) — the ray direction). Need
    /// not be normalized; the shader normalizes it. Defaults to `[0, 0, 1]`
    /// (aiming **into** the screen, away from the camera). Ignored by Ambient and
    /// Point.
    #[serde(default = "Light::default_direction")]
    pub direction: [f32; 3],
    /// Spot **inner** cone half-angle in **degrees** (the fully-lit core). Beyond
    /// this, the [`penumbra`](Self::penumbra) ramp begins. Only meaningful for
    /// [`Spot`](LightKind::Spot). Defaults to 30°.
    #[serde(default = "Light::default_cone_angle")]
    pub cone_angle: f32,
    /// Spot **penumbra** width in **degrees** — the soft ring beyond the inner
    /// cone across which intensity ramps smoothly from full to zero. `0.0` gives a
    /// hard edge. Only meaningful for [`Spot`](LightKind::Spot). Defaults to 10°.
    #[serde(default = "Light::default_penumbra")]
    pub penumbra: f32,
    /// Distance **falloff radius** in comp px for [`Point`](LightKind::Point) and
    /// [`Spot`](LightKind::Spot). `0.0` (the default) disables falloff — the light
    /// reaches infinitely far, rendering byte-identically to the pre-falloff
    /// Point. A positive `r` attenuates the contribution by `(r / (r + d))²`
    /// where `d` is the surface→light distance.
    #[serde(default)]
    pub falloff: f32,
}

impl Light {
    fn default_color() -> [f32; 3] {
        [1.0, 1.0, 1.0]
    }

    fn default_intensity() -> f32 {
        1.0
    }

    fn default_direction() -> [f32; 3] {
        [0.0, 0.0, 1.0]
    }

    fn default_cone_angle() -> f32 {
        30.0
    }

    fn default_penumbra() -> f32 {
        10.0
    }

    /// A new ambient light (a flat diffuse floor) of the given color/intensity.
    pub fn ambient(color: [f32; 3], intensity: f32) -> Self {
        Light {
            kind: LightKind::Ambient,
            position: [0.0, 0.0, 0.0],
            color,
            intensity,
            direction: Light::default_direction(),
            cone_angle: Light::default_cone_angle(),
            penumbra: Light::default_penumbra(),
            falloff: 0.0,
        }
    }

    /// A new point light at `position` with the given color/intensity (no falloff).
    pub fn point(position: [f32; 3], color: [f32; 3], intensity: f32) -> Self {
        Light {
            kind: LightKind::Point,
            position,
            color,
            intensity,
            direction: Light::default_direction(),
            cone_angle: Light::default_cone_angle(),
            penumbra: Light::default_penumbra(),
            falloff: 0.0,
        }
    }

    /// A new spot light at `position` aimed along `direction`, with inner-cone
    /// half-angle `cone_angle` and `penumbra` soft edge (both degrees). No falloff.
    pub fn spot(
        position: [f32; 3],
        direction: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        cone_angle: f32,
        penumbra: f32,
    ) -> Self {
        Light {
            kind: LightKind::Spot,
            position,
            color,
            intensity,
            direction,
            cone_angle,
            penumbra,
            falloff: 0.0,
        }
    }

    /// A new parallel (directional) light whose rays travel along `direction`.
    pub fn parallel(direction: [f32; 3], color: [f32; 3], intensity: f32) -> Self {
        Light {
            kind: LightKind::Parallel,
            position: [0.0, 0.0, 0.0],
            color,
            intensity,
            direction,
            cone_angle: Light::default_cone_angle(),
            penumbra: Light::default_penumbra(),
            falloff: 0.0,
        }
    }
}

impl Default for Light {
    fn default() -> Self {
        // A neutral point light — what the "add point light" UI seeds before the
        // user places it (the renderer never uses a defaulted light directly).
        Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0)
    }
}

/// The **surface normal** of a 3-D layer with X/Y/Z **orientation**
/// `(rx, ry, rz)` degrees, in comp space. A flat un-oriented layer faces the
/// camera (which looks down `+z` from `-z`), so its base normal is `[0, 0, -1]`
/// (toward the viewer); the orientation rotates it exactly as it rotates the
/// layer's plane (the same [`rotate_orientation`](super::rotate_orientation)
/// the layer geometry uses). Always returns a unit vector.
pub fn layer_normal(rx_deg: f32, ry_deg: f32, rz_deg: f32) -> [f32; 3] {
    let (nx, ny, nz) = super::rotate_orientation(0.0, 0.0, -1.0, rx_deg, ry_deg, rz_deg);
    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-9);
    [nx / len, ny / len, nz / len]
}

/// Distance attenuation for a light with the given `falloff` radius at distance
/// `d`. `0.0` radius → exactly `1.0` (no falloff, the back-compat default). A
/// positive radius gives a smooth inverse-square-ish `(r / (r + d))²`: `1.0` at
/// the light (`d = 0`) and decaying toward `0` with distance.
fn distance_attenuation(falloff: f32, d: f32) -> f32 {
    if falloff <= 0.0 {
        return 1.0;
    }
    let k = falloff / (falloff + d.max(0.0));
    k * k
}

/// The spot **cone** factor for a ray that makes angle `angle_deg` with the cone
/// axis: `1.0` inside the inner cone (`cone_angle`), a smooth (smoothstep) ramp
/// to `0.0` across the `penumbra` ring beyond it, and `0.0` outside. A `penumbra`
/// of `0.0` gives a hard edge.
fn spot_cone_factor(angle_deg: f32, cone_angle: f32, penumbra: f32) -> f32 {
    let inner = cone_angle.max(0.0);
    let outer = inner + penumbra.max(0.0);
    if angle_deg <= inner {
        1.0
    } else if angle_deg >= outer {
        0.0
    } else {
        // smoothstep from 1 (at inner) to 0 (at outer).
        let t = (angle_deg - inner) / (outer - inner);
        let s = t * t * (3.0 - 2.0 * t);
        1.0 - s
    }
}

/// Total **illumination factor** (a per-channel RGB multiplier) at a surface
/// point `surface` with unit normal `normal`, from a set of `lights`.
///
/// Each light contributes `color · intensity · term`, summed per channel and
/// clamped non-negative:
/// - **Ambient**: `term = 1` (a flat floor; ignores position/normal).
/// - **Point**: `term = max(0, N·L) · atten(d)` with `L = normalize(pos − surface)`,
///   `d = |pos − surface|`, and `atten` the distance [`falloff`](Light::falloff)
///   (`1.0` when falloff is `0`).
/// - **Spot**: like Point, additionally gated by the [`cone`](spot_cone_factor)
///   around [`direction`](Light::direction).
/// - **Parallel**: `term = max(0, N·L)` with `L = −normalize(direction)`,
///   position-independent and with no falloff.
///
/// **Crucially, an empty light list returns `[1, 1, 1]`** (the identity
/// multiplier) — so a comp with no lights, or a layer the caller never calls this
/// for, renders unchanged. This is the back-compat contract.
pub fn illumination(lights: &[Light], surface: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    if lights.is_empty() {
        return [1.0, 1.0, 1.0];
    }
    let mut acc = [0.0f32; 3];
    for light in lights {
        let scale = match light.kind {
            LightKind::Ambient => light.intensity,
            LightKind::Point => {
                let l = [
                    light.position[0] - surface[0],
                    light.position[1] - surface[1],
                    light.position[2] - surface[2],
                ];
                let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
                if len < 1e-9 {
                    // Light sitting on the surface — treat as fully lit (atten 1).
                    light.intensity
                } else {
                    let ndotl = (normal[0] * l[0] + normal[1] * l[1] + normal[2] * l[2]) / len;
                    let atten = distance_attenuation(light.falloff, len);
                    light.intensity * ndotl.max(0.0) * atten
                }
            }
            LightKind::Spot => {
                let l = [
                    light.position[0] - surface[0],
                    light.position[1] - surface[1],
                    light.position[2] - surface[2],
                ];
                let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
                if len < 1e-9 {
                    // On the spot's origin — fully lit, inside the cone trivially.
                    light.intensity
                } else {
                    let ndotl = (normal[0] * l[0] + normal[1] * l[1] + normal[2] * l[2]) / len;
                    // Angle between the cone axis (direction) and the ray from the
                    // light *toward* the surface, which is −L.
                    let dir = normalize(light.direction);
                    let to_surface = [-l[0] / len, -l[1] / len, -l[2] / len];
                    let cos_ang =
                        (dir[0] * to_surface[0] + dir[1] * to_surface[1] + dir[2] * to_surface[2])
                            .clamp(-1.0, 1.0);
                    let angle_deg = cos_ang.acos().to_degrees();
                    let cone = spot_cone_factor(angle_deg, light.cone_angle, light.penumbra);
                    let atten = distance_attenuation(light.falloff, len);
                    light.intensity * ndotl.max(0.0) * cone * atten
                }
            }
            LightKind::Parallel => {
                // Rays travel along `direction`; the direction *toward* the light
                // is its negation. Position-independent, no falloff.
                let d = normalize(light.direction);
                let ndotl = -(normal[0] * d[0] + normal[1] * d[1] + normal[2] * d[2]);
                light.intensity * ndotl.max(0.0)
            }
        };
        acc[0] += light.color[0] * scale;
        acc[1] += light.color[1] * scale;
        acc[2] += light.color[2] * scale;
    }
    [acc[0].max(0.0), acc[1].max(0.0), acc[2].max(0.0)]
}

/// Normalize a vector; returns `[0, 0, 0]` for a (near-)zero input.
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-9 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn no_lights_is_identity() {
        let f = illumination(&[], [0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        assert_eq!(f, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn unoriented_layer_faces_the_camera() {
        // Flat layer → normal toward the viewer (−z).
        let n = layer_normal(0.0, 0.0, 0.0);
        assert!(approx(n[0], 0.0) && approx(n[1], 0.0) && approx(n[2], -1.0));
    }

    #[test]
    fn facing_light_is_brighter_than_back_facing() {
        // A point light in front of the comp (toward the camera, −z).
        let light = Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0);
        let facing = layer_normal(0.0, 0.0, 0.0); // toward −z, toward the light
        let away = layer_normal(180.0, 0.0, 0.0); // flipped → +z, away
        let lit = illumination(&[light], [0.0, 0.0, 0.0], facing)[0];
        let dark = illumination(&[light], [0.0, 0.0, 0.0], away)[0];
        assert!(lit > dark, "facing {lit} should beat back-facing {dark}");
        assert!(approx(lit, 1.0), "head-on N·L = 1 → full: {lit}");
        assert!(approx(dark, 0.0), "back-facing clamps to 0: {dark}");
    }

    #[test]
    fn ambient_is_a_flat_floor_regardless_of_normal() {
        let amb = Light::ambient([1.0, 1.0, 1.0], 0.3);
        let a = illumination(&[amb], [0.0, 0.0, 0.0], layer_normal(0.0, 0.0, 0.0));
        let b = illumination(&[amb], [10.0, 5.0, 7.0], layer_normal(90.0, 45.0, 0.0));
        assert!(approx(a[0], 0.3) && approx(b[0], 0.3));
        assert_eq!(a, b);
    }

    #[test]
    fn intensity_and_color_scale_the_contribution() {
        let n = layer_normal(0.0, 0.0, 0.0);
        let p = [0.0, 0.0, 0.0];
        // Intensity 2 doubles a head-on contribution.
        let f = illumination(&[Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 2.0)], p, n);
        assert!(approx(f[0], 2.0));
        // A red light only tints red.
        let r = illumination(&[Light::point([0.0, 0.0, -500.0], [1.0, 0.0, 0.0], 1.0)], p, n);
        assert!(approx(r[0], 1.0) && approx(r[1], 0.0) && approx(r[2], 0.0));
    }

    #[test]
    fn ambient_plus_point_adds() {
        let n = layer_normal(0.0, 0.0, 0.0);
        let lights = [
            Light::ambient([1.0, 1.0, 1.0], 0.2),
            Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0),
        ];
        let f = illumination(&lights, [0.0, 0.0, 0.0], n);
        assert!(approx(f[0], 1.2), "ambient 0.2 + head-on point 1.0: {}", f[0]);
    }

    #[test]
    fn serde_round_trip() {
        let lights = vec![
            Light::ambient([0.1, 0.2, 0.3], 0.4),
            Light::point([1.0, 2.0, 3.0], [0.5, 0.6, 0.7], 1.5),
        ];
        let json = serde_json::to_string(&lights).unwrap();
        let back: Vec<Light> = serde_json::from_str(&json).unwrap();
        assert_eq!(lights, back);
    }

    #[test]
    fn serde_legacy_point_defaults_color_and_intensity() {
        // A hand-written / legacy point light with only a kind+position fills the
        // serde-defaulted color (white) + intensity (1.0).
        let l: Light = serde_json::from_str(r#"{"kind":"Point","position":[1.0,2.0,3.0]}"#).unwrap();
        assert_eq!(l.color, [1.0, 1.0, 1.0]);
        assert_eq!(l.intensity, 1.0);
        assert_eq!(l.position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn determinism() {
        let lights = [
            Light::ambient([0.3, 0.3, 0.3], 0.5),
            Light::point([100.0, -50.0, -300.0], [0.9, 0.8, 1.0], 1.2),
            Light::spot([0.0, 0.0, -500.0], [0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0, 20.0, 8.0),
            Light::parallel([0.3, 0.2, 1.0], [1.0, 1.0, 1.0], 0.7),
        ];
        let n = layer_normal(20.0, -35.0, 10.0);
        let p = [12.0, -7.0, 40.0];
        let a = illumination(&lights, p, n);
        let b = illumination(&lights, p, n);
        assert_eq!(a, b);
    }

    // ---- Distance falloff -------------------------------------------------

    #[test]
    fn default_falloff_matches_old_point_byte_identically() {
        // A point with falloff 0 (the default) must equal the historical
        // no-falloff shading at any distance — the back-compat contract.
        let n = layer_normal(0.0, 0.0, 0.0);
        let p = [0.0, 0.0, 0.0];
        for z in [-100.0f32, -500.0, -2000.0, -9999.0] {
            let l = Light::point([0.0, 0.0, z], [1.0, 1.0, 1.0], 1.0);
            assert_eq!(l.falloff, 0.0, "default falloff is none");
            let f = illumination(&[l], p, n);
            // Old math: intensity * max(0, N·L), with N·L = 1 head-on → 1.0.
            assert!(approx(f[0], 1.0), "no-falloff head-on stays 1.0: {}", f[0]);
        }
    }

    #[test]
    fn falloff_attenuates_with_distance() {
        let n = layer_normal(0.0, 0.0, 0.0);
        let p = [0.0, 0.0, 0.0];
        // Same head-on geometry, falloff radius 500.
        let near = Light::spot_or_point_with_falloff(-250.0, 500.0);
        let far = Light::spot_or_point_with_falloff(-1000.0, 500.0);
        let n_lit = illumination(&[near], p, n)[0];
        let f_lit = illumination(&[far], p, n)[0];
        assert!(n_lit > f_lit, "near {n_lit} brighter than far {f_lit}");
        assert!(n_lit < 1.0, "falloff dims even the near light: {n_lit}");
        // At distance == radius, atten = (500/1000)^2 = 0.25.
        let at_r = Light::spot_or_point_with_falloff(-500.0, 500.0);
        assert!(approx(illumination(&[at_r], p, n)[0], 0.25));
    }

    #[test]
    fn distance_attenuation_helper() {
        assert!(approx(distance_attenuation(0.0, 1234.0), 1.0)); // disabled
        assert!(approx(distance_attenuation(100.0, 0.0), 1.0)); // at the light
        assert!(approx(distance_attenuation(100.0, 100.0), 0.25)); // d == r
        assert!(distance_attenuation(100.0, 1e6) < 1e-6); // far away → ~0
    }

    // ---- Spot cone --------------------------------------------------------

    #[test]
    fn spot_inside_cone_is_full_outside_is_dark() {
        // Spot at -z aiming +z (into screen), surface at origin facing -z.
        let n = layer_normal(0.0, 0.0, 0.0);
        let spot = Light::spot([0.0, 0.0, -500.0], [0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0, 20.0, 0.0);
        // Surface on-axis → angle 0, inside the 20° cone → full (N·L = 1).
        let on_axis = illumination(&[spot], [0.0, 0.0, 0.0], n)[0];
        assert!(approx(on_axis, 1.0), "on-axis full: {on_axis}");
        // Surface far off-axis (x = 1000, light 500 deep → ~63° off axis) → dark.
        let off = illumination(&[spot], [1000.0, 0.0, 0.0], n)[0];
        assert!(approx(off, 0.0), "outside cone dark: {off}");
    }

    #[test]
    fn spot_penumbra_ramps_between_full_and_dark() {
        // Inner 10°, penumbra 20° → soft ring 10°..30°. Midpoint (20°) ramps.
        let inner = spot_cone_factor(5.0, 10.0, 20.0);
        let mid = spot_cone_factor(20.0, 10.0, 20.0);
        let outer = spot_cone_factor(35.0, 10.0, 20.0);
        assert!(approx(inner, 1.0), "inside inner cone full: {inner}");
        assert!(approx(outer, 0.0), "past outer edge dark: {outer}");
        assert!(mid > 0.0 && mid < 1.0, "penumbra midpoint partial: {mid}");
        // Smoothstep at the exact midpoint t=0.5 → 1 - 0.5 = 0.5.
        assert!(approx(mid, 0.5), "smoothstep midpoint ≈ 0.5: {mid}");
        // Monotone decreasing across the ring.
        assert!(spot_cone_factor(12.0, 10.0, 20.0) > spot_cone_factor(28.0, 10.0, 20.0));
    }

    #[test]
    fn spot_hard_edge_with_zero_penumbra() {
        assert!(approx(spot_cone_factor(19.9, 20.0, 0.0), 1.0));
        assert!(approx(spot_cone_factor(20.1, 20.0, 0.0), 0.0));
    }

    // ---- Parallel (directional) ------------------------------------------

    #[test]
    fn parallel_is_position_independent() {
        let n = layer_normal(0.0, 0.0, 0.0);
        // Rays travel +z (into screen); the surface normal faces -z → toward the
        // light → fully lit, identically everywhere.
        let par = Light::parallel([0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0);
        let a = illumination(&[par], [0.0, 0.0, 0.0], n)[0];
        let b = illumination(&[par], [9999.0, -4321.0, 8765.0], n)[0];
        assert!(approx(a, 1.0), "head-on parallel full: {a}");
        assert_eq!(a, b, "parallel light ignores position");
    }

    #[test]
    fn parallel_back_facing_is_dark() {
        // Same rays (+z) but a layer flipped to face +z → away from the light.
        let par = Light::parallel([0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0);
        let away = layer_normal(180.0, 0.0, 0.0);
        let f = illumination(&[par], [0.0, 0.0, 0.0], away)[0];
        assert!(approx(f, 0.0), "back-facing parallel dark: {f}");
    }

    #[test]
    fn parallel_direction_need_not_be_normalized() {
        let n = layer_normal(0.0, 0.0, 0.0);
        let unit = illumination(&[Light::parallel([0.0, 0.0, 1.0], [1.0; 3], 1.0)], [0.0; 3], n);
        let scaled = illumination(&[Light::parallel([0.0, 0.0, 7.0], [1.0; 3], 1.0)], [0.0; 3], n);
        assert_eq!(unit, scaled);
    }

    // ---- serde of the new fields -----------------------------------------

    #[test]
    fn serde_round_trip_spot_and_parallel() {
        let lights = vec![
            Light::spot([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.5, 0.6, 0.7], 1.3, 25.0, 12.0),
            {
                let mut p = Light::point([4.0, 5.0, 6.0], [1.0, 1.0, 1.0], 1.0);
                p.falloff = 800.0;
                p
            },
            Light::parallel([0.1, -0.2, 0.9], [0.9, 0.9, 1.0], 0.8),
        ];
        let json = serde_json::to_string(&lights).unwrap();
        let back: Vec<Light> = serde_json::from_str(&json).unwrap();
        assert_eq!(lights, back);
    }

    #[test]
    fn serde_legacy_point_defaults_new_fields() {
        // A pre-spot/parallel point light (only kind+position) fills every new
        // field with its default — and renders identically (falloff 0).
        let l: Light = serde_json::from_str(r#"{"kind":"Point","position":[1.0,2.0,3.0]}"#).unwrap();
        assert_eq!(l.direction, [0.0, 0.0, 1.0]);
        assert_eq!(l.cone_angle, 30.0);
        assert_eq!(l.penumbra, 10.0);
        assert_eq!(l.falloff, 0.0);
    }

    impl Light {
        /// Test helper: a head-on point light at `[0,0,z]` with the given falloff.
        fn spot_or_point_with_falloff(z: f32, falloff: f32) -> Self {
            let mut l = Light::point([0.0, 0.0, z], [1.0, 1.0, 1.0], 1.0);
            l.falloff = falloff;
            l
        }
    }
}
