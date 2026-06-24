//! **Gradient Mesh** — a grid of colour nodes over a shape's bounds with smooth
//! colour interpolation, Illustrator's *Object ▸ Create Gradient Mesh*.
//!
//! The model is a `rows × cols` grid of [`MeshNode`]s, each carrying a document-
//! space position and an RGBA colour. Colour is sampled across the mesh by
//! **bilinear interpolation** inside each grid cell (the four corner nodes), and
//! a whole cell's interior can be reconstructed from its corners as a degenerate
//! **Coons patch** (here, bilinear, since edges are straight). The mesh
//! **tessellates** into filled triangles (two per cell) for a flat-shaded preview
//! that any rasterizer / vector renderer can draw, plus a point-sampler for
//! arbitrary colour lookups.
//!
//! Pure, deterministic, unit-tested — no GPU, no UI.

use serde::{Deserialize, Serialize};

/// One control point of a gradient mesh: a document-space position and an RGBA
/// colour (straight sRGB, matching the document convention).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshNode {
    pub pos: (f32, f32),
    pub color: [f32; 4],
}

impl MeshNode {
    pub fn new(pos: (f32, f32), color: [f32; 4]) -> Self {
        Self { pos, color }
    }
}

/// A `rows × cols` gradient mesh. Nodes are stored row-major: node `(r, c)` is at
/// `nodes[r * cols + c]`. There must be at least a 2×2 grid (one cell) for the
/// mesh to interpolate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientMesh {
    pub rows: usize,
    pub cols: usize,
    pub nodes: Vec<MeshNode>,
}

/// One tessellated triangle: three positions and three matching colours, to be
/// flat- or Gouraud-shaded by the consumer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshTri {
    pub pos: [(f32, f32); 3],
    pub color: [[f32; 4]; 3],
}

impl GradientMesh {
    /// Build a fresh `rows × cols` mesh covering `bbox` `[x, y, w, h]`. Every node
    /// is the same `base` colour (a flat fill the user then recolours node by
    /// node). `rows`/`cols` are clamped to at least 2 so there is always one cell.
    pub fn new(bbox: [f32; 4], rows: usize, cols: usize, base: [f32; 4]) -> Self {
        let rows = rows.max(2);
        let cols = cols.max(2);
        let [x, y, w, h] = bbox;
        let mut nodes = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let px = x + w * (c as f32 / (cols - 1) as f32);
                let py = y + h * (r as f32 / (rows - 1) as f32);
                nodes.push(MeshNode::new((px, py), base));
            }
        }
        Self { rows, cols, nodes }
    }

    /// The flat index of node `(r, c)`, or `None` if out of range.
    pub fn index(&self, r: usize, c: usize) -> Option<usize> {
        (r < self.rows && c < self.cols).then(|| r * self.cols + c)
    }

    /// Read-only access to node `(r, c)`.
    pub fn node(&self, r: usize, c: usize) -> Option<&MeshNode> {
        self.index(r, c).map(|i| &self.nodes[i])
    }

    /// Set the colour of node `(r, c)`. Returns `true` if the node exists.
    pub fn set_color(&mut self, r: usize, c: usize, color: [f32; 4]) -> bool {
        match self.index(r, c) {
            Some(i) => {
                self.nodes[i].color = color;
                true
            }
            None => false,
        }
    }

    /// Move node `(r, c)` to a new document-space position. Returns `true` if the
    /// node exists.
    pub fn move_node(&mut self, r: usize, c: usize, pos: (f32, f32)) -> bool {
        match self.index(r, c) {
            Some(i) => {
                self.nodes[i].pos = pos;
                true
            }
            None => false,
        }
    }

    /// Insert a new **row** of nodes after row `r`, interpolating each new node's
    /// position and colour as the midpoint of the nodes directly above and below
    /// it (Illustrator adds a mesh line between two existing lines). No-op if `r`
    /// is the last row or out of range.
    pub fn add_row_after(&mut self, r: usize) -> bool {
        if r + 1 >= self.rows {
            return false;
        }
        let mut new_row = Vec::with_capacity(self.cols);
        for c in 0..self.cols {
            let a = self.nodes[r * self.cols + c];
            let b = self.nodes[(r + 1) * self.cols + c];
            new_row.push(MeshNode::new(
                mid(a.pos, b.pos),
                lerp4(a.color, b.color, 0.5),
            ));
        }
        // Splice the new row in after row r.
        let insert_at = (r + 1) * self.cols;
        for (i, node) in new_row.into_iter().enumerate() {
            self.nodes.insert(insert_at + i, node);
        }
        self.rows += 1;
        true
    }

    /// Insert a new **column** of nodes after column `c`, interpolating each new
    /// node from its left/right neighbours. No-op if `c` is the last column.
    pub fn add_col_after(&mut self, c: usize) -> bool {
        if c + 1 >= self.cols {
            return false;
        }
        // Build the whole new node grid (cols+1 wide) row by row.
        let mut out = Vec::with_capacity(self.rows * (self.cols + 1));
        for r in 0..self.rows {
            for cc in 0..self.cols {
                out.push(self.nodes[r * self.cols + cc]);
                if cc == c {
                    let a = self.nodes[r * self.cols + cc];
                    let b = self.nodes[r * self.cols + cc + 1];
                    out.push(MeshNode::new(mid(a.pos, b.pos), lerp4(a.color, b.color, 0.5)));
                }
            }
        }
        self.nodes = out;
        self.cols += 1;
        true
    }

    /// Sample the mesh colour at grid parameters `(u, v)` in `[0, 1]²` (`u` across
    /// columns, `v` down rows) by bilinear interpolation within the containing
    /// cell. The whole-mesh bilerp is a degenerate Coons patch (straight edges),
    /// the exact reconstruction Illustrator uses for a freshly-made flat mesh.
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if self.rows < 2 || self.cols < 2 {
            return self.nodes.first().map(|n| n.color).unwrap_or([0.0; 4]);
        }
        let u = u.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        // Locate the cell and the local fraction inside it.
        let fc = u * (self.cols - 1) as f32;
        let fr = v * (self.rows - 1) as f32;
        let c0 = (fc.floor() as usize).min(self.cols - 2);
        let r0 = (fr.floor() as usize).min(self.rows - 2);
        let tu = fc - c0 as f32;
        let tv = fr - r0 as f32;
        let c00 = self.nodes[r0 * self.cols + c0].color;
        let c01 = self.nodes[r0 * self.cols + c0 + 1].color;
        let c10 = self.nodes[(r0 + 1) * self.cols + c0].color;
        let c11 = self.nodes[(r0 + 1) * self.cols + c0 + 1].color;
        let top = lerp4(c00, c01, tu);
        let bot = lerp4(c10, c11, tu);
        lerp4(top, bot, tv)
    }

    /// Tessellate the mesh into filled triangles (two per cell), each carrying its
    /// three corner colours so a renderer can Gouraud- or flat-shade the patch.
    /// `(rows-1) * (cols-1) * 2` triangles in row-major cell order.
    pub fn tessellate(&self) -> Vec<MeshTri> {
        let mut tris = Vec::new();
        if self.rows < 2 || self.cols < 2 {
            return tris;
        }
        for r in 0..self.rows - 1 {
            for c in 0..self.cols - 1 {
                let n00 = self.nodes[r * self.cols + c];
                let n01 = self.nodes[r * self.cols + c + 1];
                let n10 = self.nodes[(r + 1) * self.cols + c];
                let n11 = self.nodes[(r + 1) * self.cols + c + 1];
                // Two triangles: (00, 01, 11) and (00, 11, 10).
                tris.push(MeshTri {
                    pos: [n00.pos, n01.pos, n11.pos],
                    color: [n00.color, n01.color, n11.color],
                });
                tris.push(MeshTri {
                    pos: [n00.pos, n11.pos, n10.pos],
                    color: [n00.color, n11.color, n10.color],
                });
            }
        }
        tris
    }
}

/// Midpoint of two points.
fn mid(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Linear interpolation of two RGBA colours.
fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corners() -> GradientMesh {
        // 2×2 mesh with four distinct corner colours over a unit-ish box.
        let mut m = GradientMesh::new([0.0, 0.0, 100.0, 100.0], 2, 2, [0.0, 0.0, 0.0, 1.0]);
        m.set_color(0, 0, [1.0, 0.0, 0.0, 1.0]); // TL red
        m.set_color(0, 1, [0.0, 1.0, 0.0, 1.0]); // TR green
        m.set_color(1, 0, [0.0, 0.0, 1.0, 1.0]); // BL blue
        m.set_color(1, 1, [1.0, 1.0, 1.0, 1.0]); // BR white
        m
    }

    #[test]
    fn new_grid_node_count_and_corners() {
        let m = GradientMesh::new([10.0, 20.0, 80.0, 40.0], 3, 4, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(m.nodes.len(), 12);
        assert_eq!(m.node(0, 0).unwrap().pos, (10.0, 20.0));
        assert_eq!(m.node(2, 3).unwrap().pos, (90.0, 60.0));
    }

    #[test]
    fn new_grid_clamps_to_min_2x2() {
        let m = GradientMesh::new([0.0, 0.0, 10.0, 10.0], 1, 1, [0.0; 4]);
        assert_eq!((m.rows, m.cols), (2, 2));
        assert_eq!(m.nodes.len(), 4);
    }

    #[test]
    fn sample_midpoint_is_average_of_four_corners() {
        let m = corners();
        // Centre of a bilinear patch = average of the four corners.
        let mid = m.sample(0.5, 0.5);
        let expect = [
            (1.0 + 0.0 + 0.0 + 1.0) / 4.0,
            (0.0 + 1.0 + 0.0 + 1.0) / 4.0,
            (0.0 + 0.0 + 1.0 + 1.0) / 4.0,
            1.0,
        ];
        for k in 0..4 {
            assert!((mid[k] - expect[k]).abs() < 1e-5, "channel {k}: {} vs {}", mid[k], expect[k]);
        }
    }

    #[test]
    fn sample_corners_exact() {
        let m = corners();
        assert_eq!(m.sample(0.0, 0.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(m.sample(1.0, 0.0), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(m.sample(0.0, 1.0), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(m.sample(1.0, 1.0), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn sample_top_edge_midpoint() {
        let m = corners();
        // Halfway along the top edge: average of TL red and TR green.
        let c = m.sample(0.5, 0.0);
        assert!((c[0] - 0.5).abs() < 1e-5 && (c[1] - 0.5).abs() < 1e-5 && c[2].abs() < 1e-5);
    }

    #[test]
    fn tessellate_two_tris_per_cell() {
        let m = GradientMesh::new([0.0, 0.0, 100.0, 100.0], 3, 3, [0.2, 0.2, 0.2, 1.0]);
        // 2×2 cells × 2 tris = 8.
        assert_eq!(m.tessellate().len(), 8);
    }

    #[test]
    fn set_and_move_node() {
        let mut m = corners();
        assert!(m.set_color(0, 0, [0.1, 0.2, 0.3, 1.0]));
        assert_eq!(m.node(0, 0).unwrap().color, [0.1, 0.2, 0.3, 1.0]);
        assert!(m.move_node(0, 0, (5.0, 6.0)));
        assert_eq!(m.node(0, 0).unwrap().pos, (5.0, 6.0));
        assert!(!m.set_color(9, 9, [0.0; 4]), "out-of-range node");
    }

    #[test]
    fn add_row_increases_grid_and_interpolates() {
        let mut m = corners();
        assert!(m.add_row_after(0));
        assert_eq!(m.rows, 3);
        assert_eq!(m.nodes.len(), 6);
        // The new middle row, col 0, is the midpoint of red (TL) and blue (BL).
        let n = m.node(1, 0).unwrap();
        assert_eq!(n.pos, (0.0, 50.0));
        assert!((n.color[0] - 0.5).abs() < 1e-5 && (n.color[2] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn add_col_increases_grid() {
        let mut m = corners();
        assert!(m.add_col_after(0));
        assert_eq!(m.cols, 3);
        assert_eq!(m.nodes.len(), 6);
        // New middle col, row 0 = midpoint of red (TL) and green (TR).
        let n = m.node(0, 1).unwrap();
        assert_eq!(n.pos, (50.0, 0.0));
        assert!((n.color[0] - 0.5).abs() < 1e-5 && (n.color[1] - 0.5).abs() < 1e-5);
    }
}
