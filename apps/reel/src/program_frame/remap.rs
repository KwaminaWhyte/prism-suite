//! Source-time / time-remap helpers split out of `program_frame/mod.rs`
//! (file-size rule): the cubic-bezier speed-curve mapping and the speed-key
//! trapezoidal integrator that map a timeline time to a clip-local source time.
//! Both are re-exported from the parent module so the sampler keeps calling them
//! unqualified.

/// Map local_t (0..duration) → source time using a cubic bezier speed curve.
/// The bezier maps normalized [0,1] timeline → [0,1] source fraction.
pub(crate) fn bezier_source_time(local_t: f32, duration: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    if duration <= 0.0 { return 0.0; }
    let norm_t = (local_t / duration).clamp(0.0, 1.0);
    // Cubic bezier: B(t) = (1-t)^3*p0 + 3*(1-t)^2*t*p1 + 3*(1-t)*t^2*p2 + t^3*p3
    let u = 1.0 - norm_t;
    let frac = u*u*u*p0 + 3.0*u*u*norm_t*p1 + 3.0*u*norm_t*norm_t*p2 + norm_t*norm_t*norm_t*p3;
    frac.clamp(0.0, 1.0) * duration
}

/// Integrate a clip's `time_remap_speed_keys` — `(timeline_t, speed_factor)`
/// pairs — into the source time at timeline time `t`. The keys describe playback
/// *speed* (1.0 = real-time, 2.0 = double-speed, 0.0 = freeze); the source time
/// at `t` is `source_in` plus the area under the speed curve from the clip's
/// start to `t`, where the speed curve is linearly interpolated between keys.
///
/// A flat `0.0` segment integrates to zero added source time → a freeze-frame
/// (the source time stops advancing while the playhead moves). Before the first
/// key the first key's speed holds; after the last key the last key's speed
/// holds (Premiere's hold-extrapolation). Returns a source time clamped to ≥0.
///
/// The trapezoidal area of one `[t0,t1]` segment with speeds `[s0,s1]` is
/// `(s0 + s1) * 0.5 * (t1 - t0)`; the partial segment up to `t` uses the
/// interpolated speed at `t`.
pub fn remapped_source_time(keys: &[(f32, f32)], source_in: f32, start: f32, t: f32) -> f32 {
    if keys.is_empty() {
        return (source_in + (t - start).max(0.0)).max(0.0);
    }
    // Before the first key: hold the first key's speed back toward the start.
    let first = keys[0];
    if t <= first.0 {
        let dt = (t - start).max(0.0).min((first.0 - start).max(0.0));
        return (source_in + first.1.max(0.0) * dt).max(0.0);
    }
    // Segment from the clip start to the first key (hold the first key's speed).
    let mut src = source_in + first.1.max(0.0) * (first.0 - start).max(0.0);
    for w in keys.windows(2) {
        let (t0, s0) = w[0];
        let (t1, s1) = w[1];
        let s0 = s0.max(0.0);
        let s1 = s1.max(0.0);
        if t >= t1 {
            // Full trapezoidal segment.
            src += (s0 + s1) * 0.5 * (t1 - t0).max(0.0);
        } else if t > t0 {
            // Partial segment up to `t`: interpolate the speed at `t`.
            let frac = (t - t0) / (t1 - t0).max(1e-9);
            let s_t = s0 + frac * (s1 - s0);
            src += (s0 + s_t) * 0.5 * (t - t0);
            return src.max(0.0);
        }
    }
    // After the last key: hold the last key's speed.
    let last = keys[keys.len() - 1];
    if t > last.0 {
        src += last.1.max(0.0) * (t - last.0);
    }
    src.max(0.0)
}
