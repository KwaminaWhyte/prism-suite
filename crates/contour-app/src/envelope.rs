use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeMesh {
    pub rows: u32,
    pub cols: u32,
    pub points: Vec<[f32; 2]>,
}

impl EnvelopeMesh {
    pub fn new(rows: u32, cols: u32, bbox: [f32; 4]) -> Self {
        let [x, y, w, h] = bbox;
        let mut points = Vec::with_capacity((rows * cols) as usize);
        for r in 0..rows {
            for c in 0..cols {
                let u = if cols > 1 { c as f32 / (cols - 1) as f32 } else { 0.5 };
                let v = if rows > 1 { r as f32 / (rows - 1) as f32 } else { 0.5 };
                points.push([x + u * w, y + v * h]);
            }
        }
        Self { rows, cols, points }
    }

    pub fn warp(&self, u: f32, v: f32) -> [f32; 2] {
        let rows = self.rows as usize;
        let cols = self.cols as usize;
        if rows < 2 || cols < 2 {
            return [u, v];
        }
        let col_f = (u * (cols - 1) as f32).clamp(0.0, (cols - 1) as f32 - 1e-6);
        let row_f = (v * (rows - 1) as f32).clamp(0.0, (rows - 1) as f32 - 1e-6);
        let c0 = col_f as usize;
        let r0 = row_f as usize;
        let c1 = (c0 + 1).min(cols - 1);
        let r1 = (r0 + 1).min(rows - 1);
        let tc = col_f - c0 as f32;
        let tr = row_f - r0 as f32;
        let p00 = self.points[r0 * cols + c0];
        let p10 = self.points[r0 * cols + c1];
        let p01 = self.points[r1 * cols + c0];
        let p11 = self.points[r1 * cols + c1];
        let x = p00[0] * (1.0 - tc) * (1.0 - tr) + p10[0] * tc * (1.0 - tr) + p01[0] * (1.0 - tc) * tr + p11[0] * tc * tr;
        let y = p00[1] * (1.0 - tc) * (1.0 - tr) + p10[1] * tc * (1.0 - tr) + p01[1] * (1.0 - tc) * tr + p11[1] * tc * tr;
        [x, y]
    }
}
