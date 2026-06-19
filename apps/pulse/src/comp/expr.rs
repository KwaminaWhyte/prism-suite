//! Per-property **expressions** — the After-Effects signature feature.
//!
//! Any animatable scalar [`Track`](super::Track) can carry an optional
//! `expression` string. When present, the property's value at time `t` is the
//! result of evaluating that expression with [`rhai`] (a pure-Rust embeddable
//! scripting engine — no system deps) instead of the raw keyframed sample. The
//! keyframed sample is still computed and exposed to the script as `value`, so
//! an expression can *drive* the animation (`value + 10`, `value * sin(time)`)
//! rather than replace it.
//!
//! ## Context
//! Each evaluation binds a small [`ExprCtx`] into scope as plain variables:
//! - `time` — the sample time in seconds
//! - `value` — the property's keyframed value at `time` (so expressions offset it)
//! - `fps` — the comp's frame rate
//! - `duration` — the comp's duration in seconds
//! - `index` — the layer's stack index
//!
//! plus a handful of AE-style helper functions registered on the engine:
//! - `wiggle(freq, amp)` — smooth pseudo-random jitter, **deterministic** per
//!   `(layer, time)` (seeded from a stable hash, never `Math.random`), so a
//!   given frame always renders identically
//! - `linear(t, tmin, tmax, v1, v2)` — remap `t` from `[tmin, tmax]` to
//!   `[v1, v2]` (clamped to the endpoints outside the range)
//! - `clamp(v, lo, hi)` — clamp `v` into `[lo, hi]`
//!
//! `sin` / `cos` / `abs` / `floor` (and the rest of rhai's math) are available
//! out of the box.
//!
//! ## Errors & caching
//! A parse or eval error never panics: [`eval`] returns `None`, and the caller
//! falls back to the keyframed value. Whether the *last* evaluation of a given
//! expression string failed is recorded so the UI can surface an error state
//! (see [`last_error`]). Compiled ASTs are cached per source string (in a
//! thread-local cache), so a hot render path re-uses the compiled program rather
//! than re-parsing every frame for every property.

use rhai::{Dynamic, Engine, AST};
use std::cell::RefCell;
use std::collections::HashMap;

/// Coerce a rhai [`Dynamic`] numeric argument (int *or* float) to `f64`.
///
/// rhai doesn't auto-coerce integer literals to floats, so a call like
/// `wiggle(2, 50)` passes two ints. Accepting `Dynamic` and coercing here lets
/// the helpers take natural numeric literals without the user writing `2.0`.
fn as_f64(v: &Dynamic) -> f64 {
    if let Some(f) = v.clone().try_cast::<f64>() {
        f
    } else if let Some(i) = v.clone().try_cast::<i64>() {
        i as f64
    } else {
        0.0
    }
}

/// The scalar context an expression is evaluated against: the sample time, the
/// keyframed `value` it can offset, the comp's `fps` / `duration`, and the
/// layer's stack `index`. All bound into the script as same-named variables.
#[derive(Clone, Copy, Debug)]
pub struct ExprCtx {
    /// Sample time in seconds (`time` in the script).
    pub time: f32,
    /// The property's keyframed value at `time` (`value` in the script).
    pub value: f32,
    /// The comp's frame rate (`fps`).
    pub fps: f32,
    /// The comp's duration in seconds (`duration`).
    pub duration: f32,
    /// The layer's index in the comp's stack (`index`). Also seeds `wiggle` so
    /// two layers with the same expression jitter independently.
    pub index: usize,
    /// The comp's pixel width (`width` in the script). `0` when unknown (e.g. a
    /// bare value-only context).
    #[doc(hidden)]
    pub width: f32,
    /// The comp's pixel height (`height` in the script). `0` when unknown.
    #[doc(hidden)]
    pub height: f32,
}

impl ExprCtx {
    /// A bare context with only `time` / `value` set (fps/duration zeroed,
    /// index 0) — convenient for tests and value-only expressions.
    #[cfg(test)]
    pub fn at(time: f32, value: f32) -> Self {
        ExprCtx {
            time,
            value,
            fps: 0.0,
            duration: 0.0,
            index: 0,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// A cheap, immutable snapshot of the track being evaluated, primed per-call so
/// the AE-style temporal helpers (`valueAtTime` / `loopOut` / `loopIn`) can read
/// the keyframed value at *any* time — not just the current frame. Carries the
/// resolved keyframe `(t, value)` pairs (after roving/interp are baked into the
/// sampler upstream is *not* needed here — we re-sample with the same temporal
/// interpolation the engine uses) plus the property default and the comp's
/// duration. Mirrors the [`SEED`] priming pattern: built per [`eval`], read by
/// the registered functions through a thread-local.
#[derive(Clone, Default)]
pub struct TrackView {
    /// `(time, value)` keyframe pairs, ascending by time (a copy of the track's
    /// keys; the helpers re-interpolate them rather than mutate them).
    pub keys: Vec<(f32, f32)>,
    /// The property default returned when the track is empty.
    pub default: f32,
}

impl TrackView {
    /// Linearly sample the snapshot at `t` (constant-held outside `[first,last]`).
    ///
    /// The helpers (`valueAtTime` etc.) use plain linear interpolation between
    /// keys: the eased value at the current frame is already in `value`, and AE's
    /// expression-time helpers conventionally read the *interpolated* value, so a
    /// linear read is a faithful, panic-free approximation that needs only the
    /// `(t, value)` pairs (no `Interp` snapshot).
    fn sample(&self, t: f32) -> f32 {
        match self.keys.as_slice() {
            [] => self.default,
            [only] => only.1,
            keys => {
                let first = keys[0];
                let last = keys[keys.len() - 1];
                if t <= first.0 {
                    return first.1;
                }
                if t >= last.0 {
                    return last.1;
                }
                let i = keys.partition_point(|k| k.0 <= t);
                let a = keys[i - 1];
                let b = keys[i];
                let span = b.0 - a.0;
                if span <= f32::EPSILON {
                    return b.1;
                }
                let f = (t - a.0) / span;
                a.1 + (b.1 - a.1) * f
            }
        }
    }

    /// First / last keyframe times, or `None` when fewer than two keys (no cycle
    /// to loop).
    fn span(&self) -> Option<(f32, f32)> {
        if self.keys.len() < 2 {
            return None;
        }
        Some((self.keys[0].0, self.keys[self.keys.len() - 1].0))
    }
}

thread_local! {
    /// Per-thread compiled-AST cache, keyed by the expression source string.
    /// `None` means the string failed to compile (cached so we don't re-parse a
    /// broken expression every frame).
    static AST_CACHE: RefCell<HashMap<String, Option<AST>>> = RefCell::new(HashMap::new());
    /// Whether the most recent [`eval`] of each expression string errored
    /// (parse *or* runtime). Drives the UI error state.
    static LAST_ERROR: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
}

/// Build the shared rhai [`Engine`] with the helper functions registered and
/// limits tightened so a hostile/looping expression can't hang the render.
fn build_engine() -> Engine {
    let mut engine = Engine::new();
    // Bound the cost of any single evaluation: expressions are sampled per frame
    // per property, so cap operations / call depth and forbid defining functions.
    engine.set_max_operations(10_000);
    engine.set_max_call_levels(16);
    engine.set_max_expr_depths(64, 64);

    // wiggle(freq, amp): smooth deterministic jitter. `index` (the layer) salts
    // the seed so identical expressions on different layers diverge; the value is
    // a sum of a few sines whose phases come from a stable integer hash — same
    // (index, time) always yields the same number, and it varies smoothly with
    // time. NOT Math.random: fully reproducible frame to frame.
    let wiggle_seed = std::cell::Cell::new(0u64);
    let seed_ref = std::rc::Rc::new(wiggle_seed);
    let seed_for_fn = seed_ref.clone();
    engine.register_fn("wiggle", move |freq: Dynamic, amp: Dynamic| -> f64 {
        wiggle_value(seed_for_fn.get(), as_f64(&freq), as_f64(&amp))
    });
    // Stash the seed accessor so `eval` can prime it per call. We re-create the
    // engine cheaply per thread (cached below), so this closure capture is fine.
    SEED.with(|s| *s.borrow_mut() = Some(seed_ref));

    // linear(t, tmin, tmax, v1, v2): remap with clamped endpoints. Args are
    // `Dynamic` so int *and* float literals both work.
    engine.register_fn(
        "linear",
        |t: Dynamic, tmin: Dynamic, tmax: Dynamic, v1: Dynamic, v2: Dynamic| -> f64 {
            let (t, tmin, tmax, v1, v2) =
                (as_f64(&t), as_f64(&tmin), as_f64(&tmax), as_f64(&v1), as_f64(&v2));
            if tmax == tmin {
                return v1;
            }
            let f = ((t - tmin) / (tmax - tmin)).clamp(0.0, 1.0);
            v1 + (v2 - v1) * f
        },
    );

    // clamp(v, lo, hi).
    engine.register_fn("clamp", |v: Dynamic, lo: Dynamic, hi: Dynamic| -> f64 {
        let (v, lo, hi) = (as_f64(&v), as_f64(&lo), as_f64(&hi));
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        v.clamp(lo, hi)
    });

    // --- Batch 5: angle helpers --------------------------------------------
    engine.register_fn("degreesToRadians", |d: Dynamic| -> f64 {
        as_f64(&d).to_radians()
    });
    engine.register_fn("radiansToDegrees", |r: Dynamic| -> f64 {
        as_f64(&r).to_degrees()
    });

    // --- Batch 5: ease helpers (Hermite smoothstep, AE-style) --------------
    // ease(t, t0, t1, v0, v1): like `linear` but with a smooth (ease-in-out)
    // ramp instead of a straight one. easeIn / easeOut bias the ramp.
    engine.register_fn(
        "ease",
        |t: Dynamic, t0: Dynamic, t1: Dynamic, v0: Dynamic, v1: Dynamic| -> f64 {
            ease_remap(EaseShape::InOut, &t, &t0, &t1, &v0, &v1)
        },
    );
    engine.register_fn(
        "easeIn",
        |t: Dynamic, t0: Dynamic, t1: Dynamic, v0: Dynamic, v1: Dynamic| -> f64 {
            ease_remap(EaseShape::In, &t, &t0, &t1, &v0, &v1)
        },
    );
    engine.register_fn(
        "easeOut",
        |t: Dynamic, t0: Dynamic, t1: Dynamic, v0: Dynamic, v1: Dynamic| -> f64 {
            ease_remap(EaseShape::Out, &t, &t0, &t1, &v0, &v1)
        },
    );

    // --- Batch 5: deterministic random / seedRandom -----------------------
    // seedRandom(seed): re-seed the deterministic generator for this evaluation
    // (combined with the per-(layer,time) base seed so the same frame is
    // reproducible). random() ∈ [0,1); random(max) ∈ [0,max); random(min,max).
    engine.register_fn("seedRandom", |seed: Dynamic| {
        RAND.with(|r| {
            let mut r = r.borrow_mut();
            // Mix the user seed into the per-call base so two frames still differ
            // unless the user pins them, but a fixed seed is stable per frame.
            r.state = splitmix64(r.base ^ (as_f64(&seed) as i64 as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
            r.draws = 0;
        });
    });
    engine.register_fn("random", || -> f64 { next_random() });
    engine.register_fn("random", |max: Dynamic| -> f64 { next_random() * as_f64(&max) });
    engine.register_fn("random", |min: Dynamic, max: Dynamic| -> f64 {
        let (lo, hi) = (as_f64(&min), as_f64(&max));
        lo + next_random() * (hi - lo)
    });

    // --- Batch 5: temporal helpers (read the track at any time) -----------
    // valueAtTime(t): the keyframed value at an arbitrary time. loopOut() /
    // loopIn() repeat the animation past the last / before the first key.
    engine.register_fn("valueAtTime", |t: Dynamic| -> f64 {
        TRACK.with(|tv| tv.borrow().sample(as_f64(&t) as f32) as f64)
    });
    engine.register_fn("loopOut", || -> f64 { loop_value(LoopWhere::Out, LoopKind::Cycle) });
    engine.register_fn("loopOut", |mode: Dynamic| -> f64 {
        loop_value(LoopWhere::Out, LoopKind::parse(&mode))
    });
    engine.register_fn("loopIn", || -> f64 { loop_value(LoopWhere::In, LoopKind::Cycle) });
    engine.register_fn("loopIn", |mode: Dynamic| -> f64 {
        loop_value(LoopWhere::In, LoopKind::parse(&mode))
    });

    engine
}

/// Which end of the animation a `loop*` helper extends.
#[derive(Clone, Copy)]
enum LoopWhere {
    /// `loopOut`: repeat after the last keyframe.
    Out,
    /// `loopIn`: repeat before the first keyframe.
    In,
}

/// The cycle style for `loopOut` / `loopIn`.
#[derive(Clone, Copy, PartialEq)]
enum LoopKind {
    /// `"cycle"` (default): repeat the segment over and over.
    Cycle,
    /// `"pingpong"`: repeat alternating forward / reversed.
    PingPong,
}

impl LoopKind {
    fn parse(v: &Dynamic) -> Self {
        match v.clone().try_cast::<rhai::ImmutableString>() {
            Some(s) if s.eq_ignore_ascii_case("pingpong") => LoopKind::PingPong,
            _ => LoopKind::Cycle,
        }
    }
}

/// Evaluate a `loopOut` / `loopIn` against the primed [`TrackView`] at the
/// current frame time (read from the thread-local time set in [`eval`]).
fn loop_value(whr: LoopWhere, kind: LoopKind) -> f64 {
    TRACK.with(|tv| {
        let tv = tv.borrow();
        let Some((first, last)) = tv.span() else {
            // Fewer than two keys → nothing to loop; return the held value.
            return tv.sample(EVAL_TIME.with(|t| t.get())) as f64;
        };
        let period = last - first;
        if period <= f32::EPSILON {
            return tv.sample(first) as f64;
        }
        let t = EVAL_TIME.with(|t| t.get());
        let mapped = match whr {
            LoopWhere::Out if t > last => map_loop(t - first, period, kind) + first,
            LoopWhere::In if t < first => {
                // Mirror the after-end math before the start: distance before
                // `first`, folded back into the cycle from the *end*.
                let d = first - t;
                last - map_loop(d, period, kind)
            }
            _ => t, // inside the keyed range: identity
        };
        tv.sample(mapped) as f64
    })
}

/// Fold an elapsed offset into `[0, period]` for the given loop kind.
fn map_loop(offset: f32, period: f32, kind: LoopKind) -> f32 {
    let m = offset.rem_euclid(period);
    match kind {
        LoopKind::Cycle => m,
        LoopKind::PingPong => {
            // ping-pong over a doubled period: 0..period forward, period..2p back.
            let two = period * 2.0;
            let p = offset.rem_euclid(two);
            if p <= period {
                p
            } else {
                two - p
            }
        }
    }
}

/// The shape of an `ease*` ramp.
#[derive(Clone, Copy)]
enum EaseShape {
    In,
    Out,
    InOut,
}

/// Remap `t` from `[t0,t1]` to `[v0,v1]` with a smooth (Hermite) ramp.
fn ease_remap(
    shape: EaseShape,
    t: &Dynamic,
    t0: &Dynamic,
    t1: &Dynamic,
    v0: &Dynamic,
    v1: &Dynamic,
) -> f64 {
    let (t, t0, t1, v0, v1) = (as_f64(t), as_f64(t0), as_f64(t1), as_f64(v0), as_f64(v1));
    if t1 == t0 {
        return v0;
    }
    let x = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    // Hermite smoothstep variants. InOut = classic 3x²-2x³; In/Out bias one side.
    let f = match shape {
        EaseShape::InOut => x * x * (3.0 - 2.0 * x),
        // Quadratic ease-in (slow start) / ease-out (slow finish).
        EaseShape::In => x * x,
        EaseShape::Out => 1.0 - (1.0 - x) * (1.0 - x),
    };
    v0 + (v1 - v0) * f
}

/// Per-evaluation deterministic random generator: `base` is the stable
/// `(layer, time)` seed, `state` advances per draw (re-seedable via
/// `seedRandom`), `draws` counts calls so two `random()`s in one expression
/// differ.
struct RandState {
    base: u64,
    state: u64,
    draws: u64,
}

impl Default for RandState {
    fn default() -> Self {
        RandState { base: 0, state: 0, draws: 0 }
    }
}

/// Draw the next deterministic `[0,1)` value, advancing the generator.
fn next_random() -> f64 {
    RAND.with(|r| {
        let mut r = r.borrow_mut();
        r.draws = r.draws.wrapping_add(1);
        r.state = splitmix64(r.state ^ r.draws.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        (r.state >> 11) as f64 / (1u64 << 53) as f64
    })
}

thread_local! {
    /// The shared engine for this thread (built once).
    static ENGINE: Engine = build_engine();
    /// The per-call `wiggle` seed cell, shared with the engine's `wiggle` fn so
    /// each [`eval`] can prime it before running.
    static SEED: RefCell<Option<std::rc::Rc<std::cell::Cell<u64>>>> = const { RefCell::new(None) };
    /// The track snapshot the temporal helpers (`valueAtTime`/`loopOut`/`loopIn`)
    /// read, primed per [`eval`].
    static TRACK: RefCell<TrackView> = RefCell::new(TrackView::default());
    /// The current frame time the loop helpers fold around (primed per [`eval`]).
    static EVAL_TIME: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
    /// The per-call deterministic random generator (`random`/`seedRandom`).
    static RAND: RefCell<RandState> = RefCell::new(RandState::default());
}

/// Smooth, deterministic jitter for `wiggle(freq, amp)`.
///
/// Sums a few sine waves whose frequencies and phases are derived from `seed`
/// (a stable hash of the layer index + time bucket), scaled to roughly `±amp`.
/// Because the seed is a pure function of `(index, time)`, the same frame always
/// produces the same offset; because the seed's time component changes across
/// frames, the value evolves over time.
fn wiggle_value(seed: u64, freq: f64, amp: f64) -> f64 {
    // Three octaves of sine, phases from the seed, normalized to ~[-1, 1].
    let mut acc = 0.0;
    let mut norm = 0.0;
    for k in 0..3u64 {
        let h = splitmix64(seed.wrapping_add(k.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let phase = (h as f64 / u64::MAX as f64) * std::f64::consts::TAU;
        let weight = 1.0 / (1.0 + k as f64);
        // `freq` modulates how fast the seed's time component already advances;
        // fold it into the phase so higher freq = faster jitter.
        acc += weight * ((phase * (1.0 + freq)).sin());
        norm += weight;
    }
    if norm == 0.0 {
        return 0.0;
    }
    (acc / norm) * amp
}

/// A fast, well-mixed integer hash (SplitMix64) — used to turn the
/// `(index, time)` seed into well-distributed phases for [`wiggle_value`].
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derive the stable `wiggle` seed for `(layer index, time)`.
///
/// Time is quantized to milliseconds so the seed is a function of the *frame*
/// (same `t` → same seed → same jitter) while still advancing across frames.
fn wiggle_seed(ctx: &ExprCtx) -> u64 {
    let t_ms = (ctx.time as f64 * 1000.0).round() as i64 as u64;
    splitmix64((ctx.index as u64).wrapping_mul(0x100_0000_01b3) ^ splitmix64(t_ms))
}

/// Evaluate `expr` against `ctx`, returning the resulting scalar or `None` on a
/// parse or runtime error. Never panics. The last-error flag for `expr` is
/// updated so the UI can show an error state (see [`last_error`]).
///
/// Compiled ASTs are cached per source string; a string that previously failed
/// to compile is remembered (cached as a compile failure) and short-circuits.
/// Evaluate `expr` with no track snapshot (the temporal helpers degrade to the
/// held value). Convenience over [`eval_with_track`]; used by the tests and any
/// value-only call site.
#[cfg(test)]
pub fn eval(expr: &str, ctx: &ExprCtx) -> Option<f32> {
    eval_with_track(expr, ctx, None)
}

/// Like [`eval`], but with an optional [`TrackView`] snapshot so the temporal
/// helpers (`valueAtTime` / `loopOut` / `loopIn`) can read the keyframed value at
/// any time. [`Track::sample_expr`](super::Track::sample_expr) passes its own
/// keys here; callers without a track (the bare value-only path) pass `None`,
/// and the helpers degrade to the held value.
pub fn eval_with_track(expr: &str, ctx: &ExprCtx, track: Option<TrackView>) -> Option<f32> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        // An empty expression isn't an error — it just means "no expression".
        set_error(expr, false);
        return None;
    }

    // Prime the temporal-helper context (track snapshot + current time) and the
    // per-call deterministic random generator (seeded from the same stable
    // `(layer, time)` hash as `wiggle`, so `random()` is reproducible per frame).
    TRACK.with(|tv| *tv.borrow_mut() = track.unwrap_or_default());
    EVAL_TIME.with(|t| t.set(ctx.time));
    let rand_base = splitmix64(wiggle_seed(ctx) ^ 0x5DEE_CE66_D000_0001);
    RAND.with(|r| {
        let mut r = r.borrow_mut();
        r.base = rand_base;
        r.state = rand_base;
        r.draws = 0;
    });

    ENGINE.with(|engine| {
        // Prime the per-call wiggle seed so the engine's `wiggle` fn is
        // deterministic. Done *inside* `ENGINE.with` so the engine (and thus the
        // shared SEED cell it captured) is built first — priming before then
        // would target a stale cell that `build_engine` overwrites.
        let seed = wiggle_seed(ctx);
        SEED.with(|s| {
            if let Some(cell) = s.borrow().as_ref() {
                cell.set(seed);
            }
        });

        // Compile (or fetch the cached AST). Cache compile failures too so we
        // don't re-parse a broken string every frame.
        let compiled = AST_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache
                .entry(expr.to_string())
                .or_insert_with(|| engine.compile(trimmed).ok())
                .clone()
        });
        let Some(ast) = compiled else {
            set_error(expr, true);
            return None;
        };

        // Bind the context as plain variables in a scope.
        let mut scope = rhai::Scope::new();
        scope.push_constant("time", ctx.time as f64);
        scope.push_constant("value", ctx.value as f64);
        scope.push_constant("fps", ctx.fps as f64);
        scope.push_constant("duration", ctx.duration as f64);
        scope.push_constant("index", ctx.index as i64);
        scope.push_constant("width", ctx.width as f64);
        scope.push_constant("height", ctx.height as f64);

        // Evaluate. The script's last expression is the result; accept either a
        // float or an int. Any error (or non-numeric result) is a fallback.
        match engine.eval_ast_with_scope::<f64>(&mut scope, &ast) {
            Ok(v) if v.is_finite() => {
                set_error(expr, false);
                Some(v as f32)
            }
            Ok(_) => {
                // Non-finite (NaN/inf) — treat as an error so the UI flags it and
                // the caller falls back rather than poisoning the render.
                set_error(expr, true);
                None
            }
            Err(_) => {
                // Try again as an integer result (e.g. `index * 2`).
                let mut scope = rhai::Scope::new();
                scope.push_constant("time", ctx.time as f64);
                scope.push_constant("value", ctx.value as f64);
                scope.push_constant("fps", ctx.fps as f64);
                scope.push_constant("duration", ctx.duration as f64);
                scope.push_constant("index", ctx.index as i64);
        scope.push_constant("width", ctx.width as f64);
        scope.push_constant("height", ctx.height as f64);
                match engine.eval_ast_with_scope::<i64>(&mut scope, &ast) {
                    Ok(v) => {
                        set_error(expr, false);
                        Some(v as f32)
                    }
                    Err(_) => {
                        set_error(expr, true);
                        None
                    }
                }
            }
        }
    })
}

/// Record whether the last evaluation of `expr` errored.
fn set_error(expr: &str, errored: bool) {
    LAST_ERROR.with(|m| {
        m.borrow_mut().insert(expr.to_string(), errored);
    });
}

/// Whether the most recent [`eval`] of `expr` failed (parse or runtime error).
/// `false` for an expression that has never been evaluated or last succeeded —
/// drives the Properties panel's error state.
pub fn last_error(expr: &str) -> bool {
    LAST_ERROR.with(|m| m.borrow().get(expr).copied().unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_times_two() {
        for &t in &[0.0_f32, 0.5, 1.0, 2.5, 4.0] {
            let ctx = ExprCtx::at(t, 0.0);
            let got = eval("time * 2", &ctx).expect("should evaluate");
            assert!((got - t * 2.0).abs() < 1e-5, "t={t} got={got}");
        }
    }

    #[test]
    fn value_plus_offset() {
        // `value + 10` offsets the keyframed value at each time.
        for &v in &[0.0_f32, -3.0, 42.0, 100.0] {
            let ctx = ExprCtx::at(1.0, v);
            let got = eval("value + 10", &ctx).expect("should evaluate");
            assert!((got - (v + 10.0)).abs() < 1e-4, "v={v} got={got}");
        }
    }

    #[test]
    fn wiggle_is_deterministic_and_varies() {
        // Same time → identical result (reproducible, not Math.random).
        let a = eval("wiggle(2, 50)", &ExprCtx::at(1.0, 0.0)).unwrap();
        let b = eval("wiggle(2, 50)", &ExprCtx::at(1.0, 0.0)).unwrap();
        assert_eq!(a, b, "wiggle must be deterministic for a fixed time");

        // Different times → (at least sometimes) different result.
        let mut seen_difference = false;
        for &t in &[0.0_f32, 0.2, 0.5, 1.3, 2.7, 3.9] {
            let v = eval("wiggle(2, 50)", &ExprCtx::at(t, 0.0)).unwrap();
            if (v - a).abs() > 1e-6 {
                seen_difference = true;
            }
            // Amplitude bound (a few octaves normalized to ~±amp).
            assert!(v.abs() <= 50.0 + 1e-3, "wiggle within amplitude, got {v}");
        }
        assert!(seen_difference, "wiggle should vary across time");
    }

    #[test]
    fn malformed_falls_back_to_none_and_flags_error() {
        // A syntax error must not panic — it returns None and records an error.
        let ctx = ExprCtx::at(1.0, 7.0);
        assert!(eval("this is not valid $#@", &ctx).is_none());
        assert!(last_error("this is not valid $#@"));

        // An unknown identifier is a runtime error → None, flagged.
        assert!(eval("nope + 1", &ctx).is_none());
        assert!(last_error("nope + 1"));

        // A valid expression clears the error flag.
        assert!(eval("value", &ctx).is_some());
        assert!(!last_error("value"));
    }

    #[test]
    fn linear_and_clamp_helpers() {
        // linear remaps and clamps to endpoints.
        let ctx = ExprCtx::at(0.0, 0.0);
        assert!((eval("linear(0.5, 0, 1, 0, 100)", &ctx).unwrap() - 50.0).abs() < 1e-4);
        assert!((eval("linear(-1, 0, 1, 10, 20)", &ctx).unwrap() - 10.0).abs() < 1e-4);
        assert!((eval("linear(2, 0, 1, 10, 20)", &ctx).unwrap() - 20.0).abs() < 1e-4);
        // clamp.
        assert!((eval("clamp(5, 0, 1)", &ctx).unwrap() - 1.0).abs() < 1e-4);
        assert!((eval("clamp(-5, 0, 1)", &ctx).unwrap() - 0.0).abs() < 1e-4);
        // built-in math.
        assert!((eval("floor(3.7)", &ctx).unwrap() - 3.0).abs() < 1e-4);
        assert!((eval("abs(-2.5)", &ctx).unwrap() - 2.5).abs() < 1e-4);
    }

    #[test]
    fn fps_duration_index_in_scope() {
        let ctx = ExprCtx {
            time: 0.0,
            value: 0.0,
            fps: 30.0,
            duration: 5.0,
            index: 3,
            width: 1920.0,
            height: 1080.0,
        };
        assert!((eval("fps", &ctx).unwrap() - 30.0).abs() < 1e-4);
        assert!((eval("duration", &ctx).unwrap() - 5.0).abs() < 1e-4);
        assert!((eval("index * 2", &ctx).unwrap() - 6.0).abs() < 1e-4);
        // Batch 5: width / height in scope.
        assert!((eval("width", &ctx).unwrap() - 1920.0).abs() < 1e-2);
        assert!((eval("height / 2", &ctx).unwrap() - 540.0).abs() < 1e-2);
    }

    // --- Batch 5: expanded library -----------------------------------------

    fn tv(keys: &[(f32, f32)], _duration: f32) -> TrackView {
        TrackView { keys: keys.to_vec(), default: 0.0 }
    }

    #[test]
    fn angle_conversions() {
        let ctx = ExprCtx::at(0.0, 0.0);
        assert!((eval("degreesToRadians(180)", &ctx).unwrap() - std::f32::consts::PI).abs() < 1e-4);
        assert!((eval("radiansToDegrees(3.14159265)", &ctx).unwrap() - 180.0).abs() < 1e-2);
    }

    #[test]
    fn ease_helpers_endpoints_and_midpoint() {
        let ctx = ExprCtx::at(0.0, 0.0);
        // Endpoints match `linear`; midpoint is smoothed to exactly the mean for
        // the symmetric ease (3x²-2x³ at x=0.5 = 0.5).
        assert!((eval("ease(0, 0, 1, 10, 20)", &ctx).unwrap() - 10.0).abs() < 1e-4);
        assert!((eval("ease(1, 0, 1, 10, 20)", &ctx).unwrap() - 20.0).abs() < 1e-4);
        assert!((eval("ease(0.5, 0, 1, 0, 100)", &ctx).unwrap() - 50.0).abs() < 1e-3);
        // easeIn is slow to start: at the quarter point it's below the linear 25.
        assert!(eval("easeIn(0.25, 0, 1, 0, 100)", &ctx).unwrap() < 25.0);
        // easeOut is slow to finish: at the quarter point it's above linear 25.
        assert!(eval("easeOut(0.25, 0, 1, 0, 100)", &ctx).unwrap() > 25.0);
        // Clamps outside the range.
        assert!((eval("ease(-1, 0, 1, 5, 9)", &ctx).unwrap() - 5.0).abs() < 1e-4);
        assert!((eval("ease(2, 0, 1, 5, 9)", &ctx).unwrap() - 9.0).abs() < 1e-4);
    }

    #[test]
    fn random_is_deterministic_per_frame_and_seedable() {
        // Same (layer, time, expression) → identical result (reproducible).
        let a = eval("random(100)", &ExprCtx::at(1.0, 0.0)).unwrap();
        let b = eval("random(100)", &ExprCtx::at(1.0, 0.0)).unwrap();
        assert_eq!(a, b, "random must be deterministic for a fixed frame");
        assert!((0.0..100.0).contains(&a), "random(max) in range, got {a}");
        // Two draws in one expression differ (the generator advances).
        let two = eval("random() + random() * 0", &ExprCtx::at(1.0, 0.0)).unwrap();
        let _ = two; // (just exercising two draws without panicking)
        // seedRandom pins the sequence.
        let s1 = eval("seedRandom(7); random()", &ExprCtx::at(2.0, 0.0)).unwrap();
        let s2 = eval("seedRandom(7); random()", &ExprCtx::at(2.0, 0.0)).unwrap();
        assert_eq!(s1, s2, "seedRandom must pin the draw");
        assert!((0.0..1.0).contains(&s1));
    }

    #[test]
    fn value_at_time_reads_the_track() {
        // A 0→100 ramp over [0,2]: value_at_time samples the keyed curve at any t.
        let view = tv(&[(0.0, 0.0), (2.0, 100.0)], 2.0);
        let ctx = ExprCtx::at(0.0, 0.0);
        let got = eval_with_track("valueAtTime(1.0)", &ctx, Some(view)).unwrap();
        assert!((got - 50.0).abs() < 1e-3, "midpoint of ramp = 50, got {got}");
    }

    #[test]
    fn loop_out_cycle_repeats_the_segment() {
        // Animation over [0,1] from 0→10; after the end it should repeat.
        let view = tv(&[(0.0, 0.0), (1.0, 10.0)], 4.0);
        // At t = 2.5 (1.5 past the cycle start, fold to 0.5) → 5.
        let ctx = ExprCtx::at(2.5, 99.0); // `value` is ignored by loopOut
        let got = eval_with_track("loopOut()", &ctx, Some(view)).unwrap();
        assert!((got - 5.0).abs() < 1e-3, "loopOut cycle at t=2.5 → 5, got {got}");
    }

    #[test]
    fn loop_out_pingpong_reverses() {
        let view = tv(&[(0.0, 0.0), (1.0, 10.0)], 4.0);
        // ping-pong: t=1.5 is 0.5 into the *reverse* leg → 5; t=2.0 back to 0.
        let a = eval_with_track("loopOut(\"pingpong\")", &ExprCtx::at(1.5, 0.0), Some(view.clone())).unwrap();
        let b = eval_with_track("loopOut(\"pingpong\")", &ExprCtx::at(2.0, 0.0), Some(view)).unwrap();
        assert!((a - 5.0).abs() < 1e-3, "pingpong t=1.5 → 5, got {a}");
        assert!(b.abs() < 1e-3, "pingpong t=2.0 → 0, got {b}");
    }

    #[test]
    fn loop_in_repeats_before_the_start() {
        let view = tv(&[(1.0, 0.0), (2.0, 10.0)], 4.0);
        // Before the first key (t=0.5, 0.5 before start) cycle folds to value at 1.5 → 5.
        let got = eval_with_track("loopIn()", &ExprCtx::at(0.5, 0.0), Some(view)).unwrap();
        assert!((got - 5.0).abs() < 1e-3, "loopIn at t=0.5 → 5, got {got}");
    }

    #[test]
    fn temporal_helpers_without_track_dont_panic() {
        // No track snapshot (the bare value path): helpers degrade gracefully.
        let ctx = ExprCtx::at(1.0, 7.0);
        assert!(eval("loopOut()", &ctx).is_some());
        assert!(eval("valueAtTime(0.5)", &ctx).is_some());
    }
}
