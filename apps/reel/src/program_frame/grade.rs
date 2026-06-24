//! The global grade pipeline split out of `program_frame/mod.rs` (file-size
//! rule): [`GlobalGrade`] (3D LUT + white balance + color wheels + RGB/HSL
//! curves applied after the per-clip grade) and the [`kelvin_to_rgb_gain`]
//! white-balance helper. Both are re-exported from the parent module.

/// Global grade applied after per-clip grade: optional 3D LUT trilinear
/// interpolation + white balance per-channel gain, and 3-way color wheels.
/// Passed from the host.
#[derive(Clone)]
pub struct GlobalGrade {
    /// Optional 3D LUT: `(size, table)`. `table.len() == size^3`.
    pub lut: Option<(u32, Vec<[f32; 3]>)>,
    /// Per-channel RGB gain from white balance.
    pub wb_gain: [f32; 3],
    /// 3-way color wheels (Lift / Gamma / Gain). Identity = default.
    pub color_wheels: crate::app_state::ColorWheels,
    /// Per-channel RGB tone curves. Identity = two-point linear.
    pub rgb_curves: crate::app_state::RgbCurves,
    /// HSL secondary curves (hue-vs-hue/sat/luma). Identity = flat.
    pub hsl_curves: crate::app_state::HslCurves,
}

impl GlobalGrade {
    /// The identity global grade (no LUT, no white balance adjustment).
    pub fn identity() -> Self {
        Self {
            lut: None,
            wb_gain: [1.0, 1.0, 1.0],
            color_wheels: crate::app_state::ColorWheels::default(),
            rgb_curves: crate::app_state::RgbCurves::default(),
            hsl_curves: crate::app_state::HslCurves::default(),
        }
    }

    /// True if this is effectively a no-op on any pixel.
    pub fn is_identity(&self) -> bool {
        self.lut.is_none()
            && (self.wb_gain[0] - 1.0).abs() < 1e-4
            && (self.wb_gain[1] - 1.0).abs() < 1e-4
            && (self.wb_gain[2] - 1.0).abs() < 1e-4
            && self.color_wheels.is_identity()
            && self.rgb_curves.is_identity()
            && self.hsl_curves.is_identity()
    }

    /// Apply to one straight-sRGB pixel `rgb` (0..1) in place.
    pub fn apply(&self, rgb: &mut [f32; 3]) {
        for (v, &g) in rgb.iter_mut().zip(self.wb_gain.iter()) {
            *v = (*v * g).clamp(0.0, 1.0);
        }
        if let Some((size, table)) = &self.lut {
            let s = (*size as f32) - 1.0;
            let ri = (rgb[0] * s).clamp(0.0, s);
            let gi = (rgb[1] * s).clamp(0.0, s);
            let bi = (rgb[2] * s).clamp(0.0, s);
            let r0 = ri.floor() as usize;
            let g0 = gi.floor() as usize;
            let b0 = bi.floor() as usize;
            let r1 = (r0 + 1).min(*size as usize - 1);
            let g1 = (g0 + 1).min(*size as usize - 1);
            let b1 = (b0 + 1).min(*size as usize - 1);
            let rf = ri - ri.floor();
            let gf = gi - gi.floor();
            let bf = bi - bi.floor();
            let n = *size as usize;
            let idx = |r: usize, g: usize, b: usize| r + g * n + b * n * n;
            let c000 = table[idx(r0, g0, b0)];
            let c100 = table[idx(r1, g0, b0)];
            let c010 = table[idx(r0, g1, b0)];
            let c110 = table[idx(r1, g1, b0)];
            let c001 = table[idx(r0, g0, b1)];
            let c101 = table[idx(r1, g0, b1)];
            let c011 = table[idx(r0, g1, b1)];
            let c111 = table[idx(r1, g1, b1)];
            for ch in 0..3 {
                let v = c000[ch] * (1.0 - rf) * (1.0 - gf) * (1.0 - bf)
                    + c100[ch] * rf * (1.0 - gf) * (1.0 - bf)
                    + c010[ch] * (1.0 - rf) * gf * (1.0 - bf)
                    + c110[ch] * rf * gf * (1.0 - bf)
                    + c001[ch] * (1.0 - rf) * (1.0 - gf) * bf
                    + c101[ch] * rf * (1.0 - gf) * bf
                    + c011[ch] * (1.0 - rf) * gf * bf
                    + c111[ch] * rf * gf * bf;
                rgb[ch] = v.clamp(0.0, 1.0);
            }
        }
        // Apply 3-way color wheels after LUT.
        self.color_wheels.apply(rgb);
        // Apply per-channel RGB curves after color wheels.
        self.rgb_curves.apply(rgb);
        // Apply HSL secondary curves last (hue-vs-hue/sat/luma).
        self.hsl_curves.apply(rgb);
    }
}

/// Convert color temperature (Kelvin) to per-channel RGB gain multipliers,
/// normalized so 6500K is all-ones. Derived from Tanner Helland's algorithm.
pub fn kelvin_to_rgb_gain(temp: f32) -> [f32; 3] {
    let t = temp.clamp(2000.0, 10000.0) / 100.0;
    let r = if t <= 66.0 {
        1.0
    } else {
        let v = 329.698_727_44 * (t - 60.0).powf(-0.133_204_759_2);
        (v / 255.0).clamp(0.0, 1.0)
    };
    let g = if t <= 66.0 {
        let v = 99.470_802_59 * t.ln() - 161.119_568_17;
        (v / 255.0).clamp(0.0, 1.0)
    } else {
        let v = 288.122_169_93 * (t - 60.0).powf(-0.075_514_849_2);
        (v / 255.0).clamp(0.0, 1.0)
    };
    let b = if t >= 66.0 {
        1.0
    } else if t <= 19.0 {
        0.0
    } else {
        let v = 138.517_730_8 * (t - 10.0).ln() - 305.044_792_17;
        (v / 255.0).clamp(0.0, 1.0)
    };
    // Normalize against 6500K reference.
    let ref_r = {
        let t2 = 65.0_f32;
        329.698_727_44 * (t2 - 60.0).powf(-0.133_204_759_2) / 255.0
    };
    let ref_g = {
        let t2 = 65.0_f32;
        288.122_169_93 * (t2 - 60.0).powf(-0.075_514_849_2) / 255.0
    };
    let ref_b = 1.0_f32;
    [
        (r / ref_r.max(1e-4)).clamp(0.0, 4.0),
        (g / ref_g.max(1e-4)).clamp(0.0, 4.0),
        (b / ref_b).clamp(0.0, 4.0),
    ]
}
