//! Small pure geometry/collection helpers used by the root `App` (split out of
//! `mod.rs` to keep it under the ~1000-line rule). No GPUI / app types.

/// Even-odd point-in-polygon test for the (possibly rotated/sheared) layer quad,
/// used by the preview's click-to-select. Mirrors the engine's private
/// `gizmo::point_in_quad`.
pub(super) fn point_in_quad(p: (f32, f32), quad: &[(f32, f32); 4]) -> bool {
    let mut inside = false;
    let mut j = quad.len() - 1;
    for i in 0..quad.len() {
        let (xi, yi) = quad[i];
        let (xj, yj) = quad[j];
        let intersect = ((yi > p.1) != (yj > p.1))
            && (p.0 < (xj - xi) * (p.1 - yi) / (yj - yi + f32::EPSILON.copysign(yj - yi)) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Remove the element at `i` from `v` if in range, returning whether anything
/// was removed (so the caller only marks the host dirty on a real change).
pub(super) fn vec_remove<T>(v: &mut Vec<T>, i: usize) -> bool {
    if i < v.len() {
        v.remove(i);
        true
    } else {
        false
    }
}
