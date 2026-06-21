/// A color-harmony rule describing how guide colors relate to the key color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorHarmonyRule {
    #[default]
    Complementary,
    Analogous,
    Triadic,
    SplitComplementary,
    Tetradic,
    Monochromatic,
}

/// Color Guide panel state: a harmony rule + derived swatch palette.
#[derive(Clone, Debug, Default)]
pub struct ColorGuide {
    pub rule: ColorHarmonyRule,
    /// Derived swatches (up to 6 RGBA colors) generated from the key color + rule.
    pub swatches: Vec<[f32; 4]>,
}

impl ColorGuide {
    /// Recompute swatches from `key_color` (linear sRGB) and the current harmony rule.
    pub fn recompute(&mut self, key: [f32; 4]) {
        use std::f32::consts::PI;
        let [r, g, b, a] = key;
        // Convert to HSL (simple, non-perceptual).
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) * 0.5;
        let s = if (max - min).abs() < 1e-6 {
            0.0
        } else {
            let d = max - min;
            if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) }
        };
        let h = if (max - min).abs() < 1e-6 {
            0.0
        } else if max == r {
            ((g - b) / (max - min)).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / (max - min) + 2.0) / 6.0
        } else {
            ((r - g) / (max - min) + 4.0) / 6.0
        };

        let offsets: &[f32] = match self.rule {
            ColorHarmonyRule::Complementary => &[0.0, 0.5],
            ColorHarmonyRule::Analogous => &[0.0, 1.0/12.0, -1.0/12.0],
            ColorHarmonyRule::Triadic => &[0.0, 1.0/3.0, 2.0/3.0],
            ColorHarmonyRule::SplitComplementary => &[0.0, 5.0/12.0, 7.0/12.0],
            ColorHarmonyRule::Tetradic => &[0.0, 0.25, 0.5, 0.75],
            ColorHarmonyRule::Monochromatic => &[0.0, 0.1, -0.1, 0.2, -0.2],
        };

        self.swatches = offsets.iter().map(|&off| {
            let h2 = (h + off).rem_euclid(1.0);
            hsl_to_rgb(h2, s, l, a)
        }).collect();
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32, a: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c * 0.5;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m, a]
}

/// Which distort-warp tool is active (Scallop / Crystallize / Wrinkle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WarpToolKind {
    #[default]
    Scallop,
    Crystallize,
    Wrinkle,
}
