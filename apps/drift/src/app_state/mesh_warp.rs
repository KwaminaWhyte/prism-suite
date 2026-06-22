use super::{App, Action};

/// A single control point in a warp mesh grid.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpPoint {
    pub col: u32,
    pub row: u32,
    /// Displacement from the original (rest) position.
    pub offset_x: f32,
    pub offset_y: f32,
}

/// A freeform mesh warp applied to a Bitmap layer.
#[derive(Clone, Debug)]
pub struct MeshWarp {
    pub id: usize,
    pub layer_id: usize,
    /// Number of columns in the grid (2..=20).
    pub grid_cols: u32,
    /// Number of rows in the grid (2..=20).
    pub grid_rows: u32,
    /// Displaced control points. Points that have never been moved are omitted
    /// (treat missing points as offset 0,0).
    pub control_points: Vec<WarpPoint>,
    pub enabled: bool,
}

impl App {
    pub fn apply_mesh_warp(&mut self, action: Action) {
        match action {
            Action::AddMeshWarp { layer_id, cols, rows } => {
                // Only one warp per layer; ignore if one already exists.
                if self.mesh_warps.contains_key(&layer_id) {
                    return;
                }
                let id = self.next_warp_id;
                self.next_warp_id += 1;
                let cols = cols.clamp(2, 20);
                let rows = rows.clamp(2, 20);
                self.mesh_warps.insert(
                    layer_id,
                    MeshWarp {
                        id,
                        layer_id,
                        grid_cols: cols,
                        grid_rows: rows,
                        control_points: Vec::new(),
                        enabled: true,
                    },
                );
            }
            Action::RemoveMeshWarp { layer_id } => {
                self.mesh_warps.remove(&layer_id);
            }
            Action::SetWarpPoint { layer_id, col, row, dx, dy } => {
                if let Some(warp) = self.mesh_warps.get_mut(&layer_id) {
                    if let Some(pt) = warp
                        .control_points
                        .iter_mut()
                        .find(|p| p.col == col && p.row == row)
                    {
                        pt.offset_x = dx;
                        pt.offset_y = dy;
                    } else {
                        warp.control_points.push(WarpPoint {
                            col,
                            row,
                            offset_x: dx,
                            offset_y: dy,
                        });
                    }
                }
            }
            Action::ResetWarpPoints { layer_id } => {
                if let Some(warp) = self.mesh_warps.get_mut(&layer_id) {
                    warp.control_points.clear();
                }
            }
            Action::SetMeshWarpEnabled { layer_id, enabled } => {
                if let Some(warp) = self.mesh_warps.get_mut(&layer_id) {
                    warp.enabled = enabled;
                }
            }
            Action::SetMeshWarpGrid { layer_id, cols, rows } => {
                if let Some(warp) = self.mesh_warps.get_mut(&layer_id) {
                    warp.grid_cols = cols.clamp(2, 20);
                    warp.grid_rows = rows.clamp(2, 20);
                    // Changing the grid invalidates all displaced points.
                    warp.control_points.clear();
                }
            }
            _ => {}
        }
    }

    /// Returns `(offset_x, offset_y)` for the given grid cell, or `(0.0, 0.0)`
    /// if the cell has not been displaced.
    pub fn warp_point_at(&self, layer_id: usize, col: u32, row: u32) -> Option<(f32, f32)> {
        let warp = self.mesh_warps.get(&layer_id)?;
        let pt = warp
            .control_points
            .iter()
            .find(|p| p.col == col && p.row == row);
        Some(pt.map(|p| (p.offset_x, p.offset_y)).unwrap_or((0.0, 0.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action, LayerKind};

    fn app() -> App {
        App::new()
    }

    fn layer(a: &mut App) -> usize {
        a.apply(Action::AddLayer {
            name: "Bitmap".to_string(),
            kind: LayerKind::Bitmap,
        });
        a.layers.last().unwrap().id
    }

    #[test]
    fn test_add_mesh_warp() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        assert!(a.mesh_warps.contains_key(&lid));
        let warp = &a.mesh_warps[&lid];
        assert_eq!(warp.grid_cols, 4);
        assert_eq!(warp.grid_rows, 4);
        assert!(warp.enabled);
    }

    #[test]
    fn test_add_mesh_warp_clamps_grid() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 50, rows: 1 });
        let warp = &a.mesh_warps[&lid];
        assert_eq!(warp.grid_cols, 20, "cols clamped to 20");
        assert_eq!(warp.grid_rows, 2, "rows clamped to 2");
    }

    #[test]
    fn test_add_mesh_warp_only_once_per_layer() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        let id_first = a.mesh_warps[&lid].id;
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 8, rows: 8 });
        assert_eq!(a.mesh_warps[&lid].id, id_first, "second add is ignored");
        assert_eq!(a.mesh_warps[&lid].grid_cols, 4, "grid unchanged");
    }

    #[test]
    fn test_remove_mesh_warp() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::RemoveMeshWarp { layer_id: lid });
        assert!(!a.mesh_warps.contains_key(&lid));
    }

    #[test]
    fn test_set_warp_point() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 1, row: 2, dx: 5.0, dy: -3.0 });
        let warp = &a.mesh_warps[&lid];
        let pt = warp.control_points.iter().find(|p| p.col == 1 && p.row == 2).unwrap();
        assert_eq!(pt.offset_x, 5.0);
        assert_eq!(pt.offset_y, -3.0);
    }

    #[test]
    fn test_set_warp_point_updates_existing() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 0, row: 0, dx: 1.0, dy: 2.0 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 0, row: 0, dx: 9.0, dy: 8.0 });
        // Should still be one point
        assert_eq!(a.mesh_warps[&lid].control_points.len(), 1);
        assert_eq!(a.mesh_warps[&lid].control_points[0].offset_x, 9.0);
    }

    #[test]
    fn test_warp_point_at_displaced() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 2, row: 3, dx: 10.0, dy: 5.0 });
        let (dx, dy) = a.warp_point_at(lid, 2, 3).unwrap();
        assert_eq!(dx, 10.0);
        assert_eq!(dy, 5.0);
    }

    #[test]
    fn test_warp_point_at_default_zero() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        let (dx, dy) = a.warp_point_at(lid, 1, 1).unwrap();
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn test_warp_point_at_no_warp_returns_none() {
        let a = app();
        assert!(a.warp_point_at(99, 0, 0).is_none());
    }

    #[test]
    fn test_reset_warp_points() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 0, row: 0, dx: 5.0, dy: 5.0 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 1, row: 1, dx: 3.0, dy: 3.0 });
        a.apply(Action::ResetWarpPoints { layer_id: lid });
        assert!(a.mesh_warps[&lid].control_points.is_empty());
    }

    #[test]
    fn test_set_mesh_warp_enabled() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::SetMeshWarpEnabled { layer_id: lid, enabled: false });
        assert!(!a.mesh_warps[&lid].enabled);
        a.apply(Action::SetMeshWarpEnabled { layer_id: lid, enabled: true });
        assert!(a.mesh_warps[&lid].enabled);
    }

    #[test]
    fn test_set_mesh_warp_grid_clears_points() {
        let mut a = app();
        let lid = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: lid, cols: 4, rows: 4 });
        a.apply(Action::SetWarpPoint { layer_id: lid, col: 0, row: 0, dx: 1.0, dy: 1.0 });
        a.apply(Action::SetMeshWarpGrid { layer_id: lid, cols: 6, rows: 6 });
        assert_eq!(a.mesh_warps[&lid].grid_cols, 6);
        assert!(a.mesh_warps[&lid].control_points.is_empty(), "points cleared on grid change");
    }

    #[test]
    fn test_multiple_warps_independent() {
        let mut a = app();
        let l1 = layer(&mut a);
        let l2 = layer(&mut a);
        a.apply(Action::AddMeshWarp { layer_id: l1, cols: 3, rows: 3 });
        a.apply(Action::AddMeshWarp { layer_id: l2, cols: 5, rows: 5 });
        a.apply(Action::SetWarpPoint { layer_id: l1, col: 0, row: 0, dx: 10.0, dy: 0.0 });
        assert_eq!(a.mesh_warps[&l2].control_points.len(), 0, "l2 unaffected");
    }
}
