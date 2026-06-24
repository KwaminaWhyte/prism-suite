//! Deterministic noise + cellular math shared by [`GenerateEffect`](super::GenerateEffect).
//!
//! Pure helper functions (fBm, Voronoi/cellular field, 3-D gradient noise, and the
//! hashing/interpolation primitives they rest on) extracted from `generate.rs` so
//! neither file exceeds the workspace size rule. Behaviour is unchanged — these are
//! the exact functions `generate.rs` used to define inline; they stay deterministic
//! (`splitmix64`-seeded) so a frame renders identically on every pass.

use super::generate::{CellType, FractalType};

pub const MAX_OCTAVES: u32 = 10;

/// Linear interpolation of two RGB triples.
pub(super) fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// A blend-sharpness bias about 0.5: `amount == 1` is the identity, `> 1` pushes
/// `t` toward the ends (sharper corners), `< 1` toward 0.5 (softer). Symmetric so
/// 0 and 1 are fixed points.
pub(super) fn bias(t: f32, amount: f32) -> f32 {
    let k = amount.max(0.0);
    if (k - 1.0).abs() < 1e-6 {
        return t;
    }
    // Raise the half-range to the `amount` power: `> 1` steepens the centre
    // transition (pushing values toward 0 / 1 — sharper corners), `< 1` flattens
    // it (toward 0.5 — softer).
    if t < 0.5 {
        0.5 * (2.0 * t).powf(k)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - t)).powf(k)
    }
}

/// A deterministic pseudo-random value in `[0,1)` for a local pixel position +
/// channel salt — used for ramp scatter / gradient jitter so the dither is
/// stable per (pixel, frame) (never `rand` / `Math.random`).
pub(super) fn hash_unit(x: f32, y: f32, salt: u32) -> f32 {
    let xi = (x * 16.0).floor() as i64 as u64;
    let yi = (y * 16.0).floor() as i64 as u64;
    let mut h = xi.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= yi.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (salt as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    let m = splitmix64(h);
    (m >> 11) as f32 / (1u64 << 53) as f32
}

/// Fractional Brownian motion: sum `octaves` of gradient noise, each at a higher
/// frequency (`lacunarity`) and lower amplitude (`persistence`) than the last.
///
/// `(x, y)` are the noise-space coordinates; `z` is the **evolution** axis (a
/// third noise dimension that animates the field). `seed` salts the gradient
/// hash. For [`FractalType::Turbulent`] the absolute value of each octave is
/// summed (ridged / billowy); otherwise the signed octaves are summed (smooth).
/// The result is normalized by the total amplitude so it stays in a stable range
/// (~`[-1,1]` basic, ~`[0,1]` turbulent) regardless of octave count / persistence.
#[allow(clippy::too_many_arguments)]
pub(super) fn fbm(
    x: f32,
    y: f32,
    z: f32,
    seed: u32,
    octaves: u32,
    persistence: f32,
    lacunarity: f32,
    fractal_type: FractalType,
) -> f32 {
    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for o in 0..octaves {
        // Salt each octave's hash so octaves are independent fields (not just a
        // scaled copy of octave 0).
        let oseed = seed.wrapping_add(o.wrapping_mul(0x9E37_79B9));
        let n = gradient_noise_3d(x * freq, y * freq, z * freq, oseed);
        let shaped = match fractal_type {
            FractalType::Basic => n,
            FractalType::Turbulent => n.abs(),
        };
        sum += shaped * amp;
        norm += amp;
        freq *= lacunarity;
        amp *= persistence;
    }
    if norm <= 0.0 {
        return 0.0;
    }
    sum / norm
}

/// Evaluate the **cellular / Voronoi** field at cell-space `(x, y)` with the
/// `z` (evolution) axis flowing the feature points, seeded by `seed`, shaped by
/// `cell_type` into a value in `~[0,1]`.
///
/// One **feature point** lives in each integer cell of the lattice, placed at the
/// cell origin plus a deterministic per-cell jitter scaled by `disorder` (0 = a
/// regular grid, 1 = anywhere in the cell). The 3×3 neighbourhood of the sample
/// point is scanned for the nearest (**F1**) and second-nearest (**F2**) feature
/// points (Euclidean), and the cell that owns F1 is remembered so per-cell tones
/// (Plates) are stable. Because the jitter + per-cell tone come from a stable
/// integer hash of `(cell, seed)` plus the evolution offset, the field is **fully
/// deterministic** — the same `(x, y, z, seed, disorder, cell_type)` always
/// yields the same value — and flows smoothly as `z` (evolution) sweeps.
pub(super) fn cellular(x: f32, y: f32, z: f32, seed: u32, disorder: f32, cell_type: CellType) -> f32 {
    // Static Plates is the steady, non-evolving plate field: pin its evolution
    // axis to 0 so neither the feature-point layout nor the per-cell tone moves as
    // evolution sweeps. Every other type rides the live `z`.
    let z = if cell_type == CellType::StaticPlates {
        0.0
    } else {
        z
    };
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let mut f1 = f32::INFINITY;
    let mut f2 = f32::INFINITY;
    // The cell that owns the nearest feature point (for per-cell Plates tone).
    let (mut best_cx, mut best_cy) = (xi, yi);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (gx, gy) = (xi + dx, yi + dy);
            // Per-cell jittered feature point: the cell origin plus a hashed
            // offset in [0,1)² scaled by `disorder`. The evolution `z` drifts the
            // offset (a third hash axis) so the points flow as it sweeps.
            let jx = cell_hash_unit(gx, gy, seed, 0, z);
            let jy = cell_hash_unit(gx, gy, seed, 1, z);
            let fx = gx as f32 + 0.5 + (jx - 0.5) * disorder;
            let fy = gy as f32 + 0.5 + (jy - 0.5) * disorder;
            let d = ((x - fx).powi(2) + (y - fy).powi(2)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
                best_cx = gx;
                best_cy = gy;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    match cell_type {
        // Rounded bubbles: smooth F1 (dark at the point, bright into the cell).
        CellType::Bubbles => smoothstep01(f1).clamp(0.0, 1.0),
        // Faceted crystals: the raw F1 distance (hard angular facets).
        CellType::Crystals => f1.clamp(0.0, 1.0),
        // Flat plates: one tone per cell, keyed off the owning cell's hash. The
        // layout (and so which cell owns each pixel) wobbles with evolution.
        // Static Plates lands here too but with `z` pinned to 0 above, so it holds.
        CellType::Plates | CellType::StaticPlates => cell_hash_unit(best_cx, best_cy, seed, 2, z),
        // Borders / web: F2 − F1 is large deep inside a cell and falls to ~0 along
        // the boundaries between cells (where the two nearest points tie). Invert
        // it so the value is ~0 in the cell interiors and rises to a bright ridge
        // *along the borders* — the cell-web look.
        CellType::Borders => (1.0 - (f2 - f1)).clamp(0.0, 1.0),
    }
}

/// A `smoothstep`-shaped ramp of `t` over `[0,1]` (`3t² − 2t³`), clamped at the
/// ends — rounds the Bubbles cell so it eases dark→bright from the feature point.
pub(super) fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A deterministic pseudo-random value in `[0,1)` for an integer **cell**
/// `(cx, cy)` + `seed` + channel `salt`, drifted by the evolution `z` (quantised
/// so it varies smoothly-ish as `z` sweeps but stays a stable hash). Used to place
/// each cell's feature point and pick its plate tone — never `rand`.
pub(super) fn cell_hash_unit(cx: i32, cy: i32, seed: u32, salt: u32, z: f32) -> f32 {
    // Fold the continuous evolution into the integer hash: scale + floor so it
    // changes the field as `z` moves while keeping the result a pure function of
    // the inputs (so a frame renders identically every pass).
    let zi = (z * 16.0).floor() as i64 as u64;
    let mut h = (cx as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (cy as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (seed as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= (salt as u64).wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= zi.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    let m = splitmix64(h);
    (m >> 11) as f32 / (1u64 << 53) as f32
}

/// 3-D value-gradient noise at `(x, y, z)`, seeded by `seed`, in roughly
/// `[-1, 1]`.
///
/// This is Perlin-style **gradient** noise: at each integer lattice corner a
/// pseudo-random gradient vector (derived by hashing the corner + seed) is dotted
/// with the offset to the sample point, and the eight corner contributions are
/// smoothly (quintic-fade) interpolated. Because the gradients come from a stable
/// integer hash of `(corner, seed)`, the field is **fully deterministic** — the
/// same `(x, y, z, seed)` always yields the same value — and continuous, so it
/// flows smoothly as `z` (evolution) sweeps.
pub fn gradient_noise_3d(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let zi = z.floor();
    let xf = x - xi;
    let yf = y - yi;
    let zf = z - zi;
    let (ix, iy, iz) = (xi as i32, yi as i32, zi as i32);

    let u = fade(xf);
    let v = fade(yf);
    let w = fade(zf);

    // Corner gradient · offset for each of the 8 lattice corners.
    let g = |cx: i32, cy: i32, cz: i32, fx: f32, fy: f32, fz: f32| {
        grad(hash3(ix + cx, iy + cy, iz + cz, seed), fx, fy, fz)
    };
    let n000 = g(0, 0, 0, xf, yf, zf);
    let n100 = g(1, 0, 0, xf - 1.0, yf, zf);
    let n010 = g(0, 1, 0, xf, yf - 1.0, zf);
    let n110 = g(1, 1, 0, xf - 1.0, yf - 1.0, zf);
    let n001 = g(0, 0, 1, xf, yf, zf - 1.0);
    let n101 = g(1, 0, 1, xf - 1.0, yf, zf - 1.0);
    let n011 = g(0, 1, 1, xf, yf - 1.0, zf - 1.0);
    let n111 = g(1, 1, 1, xf - 1.0, yf - 1.0, zf - 1.0);

    // Trilinear interpolation with the faded weights.
    let nx00 = lerp(n000, n100, u);
    let nx10 = lerp(n010, n110, u);
    let nx01 = lerp(n001, n101, u);
    let nx11 = lerp(n011, n111, u);
    let nxy0 = lerp(nx00, nx10, v);
    let nxy1 = lerp(nx01, nx11, v);
    lerp(nxy0, nxy1, w)
}

/// Quintic fade curve `6t⁵ − 15t⁴ + 10t³` (Perlin's improved-noise smoothstep):
/// zero first/second derivative at the ends, so octaves tile without creases.
pub(super) fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Linear interpolation.
pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Pick one of 16 evenly-spread gradient directions from a hash and dot it with
/// the offset `(x, y, z)` — Perlin's improved-noise gradient selection.
pub(super) fn grad(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    // Ken Perlin's improved-noise gradient set (12 edge vectors of a cube,
    // reused to fill 16 hash buckets).
    match hash & 15 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        11 => -y - z,
        12 => y + x,
        13 => -y + z,
        14 => y - x,
        _ => -y - z,
    }
}

/// A stable, well-mixed integer hash of an integer lattice corder `(x, y, z)` +
/// `seed`, via SplitMix64 (the same hash family `wiggle` seeds from). Pure — the
/// same inputs always give the same hash, so the noise field is deterministic.
pub(super) fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= (z as u32 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= (seed as u64).wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    (splitmix64(h) & 0xFFFF_FFFF) as u32
}

/// A fast, well-mixed 64-bit integer hash (SplitMix64) — turns the packed lattice
/// corner + seed into a well-distributed gradient bucket.
pub(super) fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
