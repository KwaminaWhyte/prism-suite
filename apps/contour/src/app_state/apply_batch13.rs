//! Batch 13 apply logic: **real** 3D Extrude / Revolve expand (CPU render →
//! flat vector faces), the **Gradient Mesh** object (create / edit / tessellate),
//! multi-format **export** (PNG / SVG / EPS / PDF), and the **Colour Picker** +
//! **Preferences** data models.
//!
//! All geometry / conversion logic lives in the dedicated top-level modules
//! ([`crate::extrude3d`], [`crate::gradient_mesh`], [`crate::export_formats`])
//! and in [`super::prefs_color`]; this file only wires `Action`s into document
//! mutations, checkpoints, selection, and the picker / prefs state.

use super::{Action, App};
use crate::document::Shape;
use crate::extrude3d;
use crate::gradient_mesh::GradientMesh;
use crate::export_formats::{self, ExportFormat};

impl App {
    /// Tail dispatcher for the Batch-13 actions, reached from the
    /// `apply_waves_wn` catch-all.
    pub(super) fn apply_batch13(&mut self, action: Action) {
        match action {
            // --- 3D expand (real render) ---
            Action::ExpandExtrudeRender { shape_id } => self.expand_extrude_render(shape_id),
            Action::ExpandRevolveRender { shape_id } => self.expand_revolve_render(shape_id),

            // --- Gradient mesh object ---
            Action::CreateGradientMeshObject { rows, cols } => {
                self.create_gradient_mesh_object(rows, cols)
            }
            Action::SetGradientMeshNodeColor { row, col, color } => {
                if let Some(m) = self.gradient_mesh_obj.as_mut() {
                    m.set_color(row, col, color);
                }
            }
            Action::MoveGradientMeshNode { row, col, x, y } => {
                if let Some(m) = self.gradient_mesh_obj.as_mut() {
                    m.move_node(row, col, (x, y));
                }
            }
            Action::AddGradientMeshRow { after } => {
                if let Some(m) = self.gradient_mesh_obj.as_mut() {
                    m.add_row_after(after);
                }
            }
            Action::AddGradientMeshColumn { after } => {
                if let Some(m) = self.gradient_mesh_obj.as_mut() {
                    m.add_col_after(after);
                }
            }
            Action::ClearGradientMeshObject => {
                self.gradient_mesh_obj = None;
            }

            // --- Export ---
            Action::ExportDocument { path, format } => self.export_document_to(&path, format),

            // --- Colour picker ---
            Action::SetPickerRgb { r, g, b } => self.color_picker.set_rgb(r, g, b),
            Action::SetPickerHsb { h, s, b } => self.color_picker.set_hsb(h, s, b),
            Action::SetPickerCmyk { c, m, y, k } => self.color_picker.set_cmyk(c, m, y, k),
            Action::SetPickerHex(hex) => {
                self.color_picker.set_hex(&hex);
            }
            Action::ApplyPickerToSelection => self.apply_picker_to_selection(),

            // --- Preferences ---
            Action::SetPrefUndoLevels(n) => {
                self.preferences.undo_levels = n.clamp(1, 1000);
            }
            Action::SetPrefSnapToPoint(v) => self.preferences.snap_to_point = v,
            Action::SetPrefSnapToGrid(v) => self.preferences.snap_to_grid = v,
            Action::SetPrefShowGrid(v) => self.preferences.show_grid = v,
            Action::SetPrefGridSpacing(v) => {
                self.preferences.grid_spacing = v.clamp(1.0, 10000.0);
            }
            Action::SetPrefUnit(u) => self.preferences.unit = u,
            Action::LoadPreferencesJson(json) => {
                let mut p = super::prefs_color::Preferences::from_json(&json);
                p.sanitize();
                self.preferences = p;
            }

            _ => {}
        }
    }

    /// **Object ▸ Expand** a 3D Extrude: render the stored extrude config for
    /// `shape_id` into flat shaded vector faces, replacing the source shape with
    /// the back-to-front face batch. No-op without a stored extrude or a fillable
    /// profile. One undo step.
    pub(super) fn expand_extrude_render(&mut self, shape_id: usize) {
        let Some(cfg) = self.extrude_3d.get(&shape_id).cloned() else {
            return;
        };
        let Some(shape) = self.doc.shapes.get(shape_id) else {
            return;
        };
        let faces = extrude3d::expand_extrude(shape, &cfg);
        if faces.is_empty() {
            return;
        }
        self.checkpoint();
        // Replace the source shape with its expanded faces (front-most first in
        // paint order is appended last, so the back-to-front list paints right).
        self.doc.shapes.remove(shape_id);
        self.extrude_3d.remove(&shape_id);
        let first = self.doc.shapes.len();
        // Insert at the source position to preserve z-order roughly.
        for (i, f) in faces.into_iter().enumerate() {
            self.doc.shapes.insert(shape_id + i, f);
        }
        let _ = first;
        self.select_single(shape_id);
        self.host.mark_dirty();
    }

    /// **Object ▸ Expand** a 3D Revolve: same pipeline as
    /// [`expand_extrude_render`](Self::expand_extrude_render) but driving the
    /// lathe mesh. One undo step.
    pub(super) fn expand_revolve_render(&mut self, shape_id: usize) {
        let Some(cfg) = self.revolve_3d.get(&shape_id).cloned() else {
            return;
        };
        let Some(shape) = self.doc.shapes.get(shape_id) else {
            return;
        };
        let faces = extrude3d::expand_revolve(shape, &cfg);
        if faces.is_empty() {
            return;
        }
        self.checkpoint();
        self.doc.shapes.remove(shape_id);
        self.revolve_3d.remove(&shape_id);
        for (i, f) in faces.into_iter().enumerate() {
            self.doc.shapes.insert(shape_id + i, f);
        }
        self.select_single(shape_id);
        self.host.mark_dirty();
    }

    /// **Object ▸ Create Gradient Mesh** — build a fresh `rows × cols` gradient
    /// mesh over the selected shape's bounds (its fill colour as the flat base),
    /// storing it as the active mesh object. No-op without a selection / bounds.
    pub(super) fn create_gradient_mesh_object(&mut self, rows: usize, cols: usize) {
        let Some(idx) = self.selected else { return };
        let Some(shape) = self.doc.shapes.get(idx) else {
            return;
        };
        let Some(b) = shape.bounds() else { return };
        let base = shape.fill_color().unwrap_or([0.8, 0.8, 0.8, 1.0]);
        let mesh = GradientMesh::new([b.x, b.y, b.w, b.h], rows, cols, base);
        self.gradient_mesh_obj = Some(mesh);
    }

    /// Apply the current colour picker colour as the fill of every selected
    /// shape. One undo step when there is a selection.
    pub(super) fn apply_picker_to_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let color = self.color_picker.rgba;
        self.checkpoint();
        for &i in &self.selection {
            if let Some(s) = self.doc.shapes.get_mut(i) {
                s.set_fill_color(color);
            }
        }
        self.host.mark_dirty();
    }

    /// Export the document (cropped to the first artboard, or the document bounds)
    /// to `path` in `format`, writing the bytes to disk. Records the outcome in
    /// the status message. Never panics.
    pub(super) fn export_document_to(&mut self, path: &str, format: ExportFormat) {
        let ab = self.export_artboard_rect();
        let result = export_formats::export_document(&self.doc, ab, format);
        let msg = match result {
            Some(bytes) => match std::fs::write(path, bytes.as_bytes()) {
                Ok(()) => format!("Exported {} → {}", format.extension().to_uppercase(), path),
                Err(e) => format!("Export failed: {}", e),
            },
            None => "Export failed: empty render".to_string(),
        };
        self.status_message = Some((msg, std::time::Instant::now()));
    }

    /// The artboard rectangle exports crop to: the first extended artboard if any,
    /// else the union of all shape bounds, else a default page.
    fn export_artboard_rect(&self) -> [f32; 4] {
        if let Some(a) = self.artboards_ex.first() {
            return [a.x, a.y, a.width, a.height];
        }
        // Union of shape bounds.
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        let mut any = false;
        for s in &self.doc.shapes {
            if let Some(b) = s.bounds() {
                any = true;
                min_x = min_x.min(b.x);
                min_y = min_y.min(b.y);
                max_x = max_x.max(b.x + b.w);
                max_y = max_y.max(b.y + b.h);
            }
        }
        if any {
            [min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0)]
        } else {
            [0.0, 0.0, 612.0, 792.0]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{Extrude3D, Revolve3D};
    use crate::document::Shape;

    fn app_with_square() -> App {
        let mut app = App::new();
        app.doc.shapes.clear();
        app.doc.shapes.push(Shape::rect(
            [0.0, 0.0, 100.0, 100.0],
            [0.5, 0.5, 0.8, 1.0],
            [0.0, 0.0, 0.0, 0.0],
            0.0,
        ));
        app.select_single(0);
        app
    }

    #[test]
    fn expand_extrude_replaces_with_faces() {
        let mut app = app_with_square();
        app.extrude_3d.insert(0, Extrude3D::default());
        app.apply(Action::ExpandExtrudeRender { shape_id: 0 });
        // The single rect is replaced by several flat faces.
        assert!(app.doc.shapes.len() > 1, "extrude expanded into many faces");
        assert!(app.history.can_undo(), "expand is undoable");
        assert!(!app.extrude_3d.contains_key(&0), "config consumed on expand");
        for s in &app.doc.shapes {
            assert!(matches!(s, Shape::Path { .. }), "faces are paths");
        }
    }

    #[test]
    fn expand_extrude_noop_without_config() {
        let mut app = app_with_square();
        app.apply(Action::ExpandExtrudeRender { shape_id: 0 });
        assert_eq!(app.doc.shapes.len(), 1, "no config → no expand");
        assert!(!app.history.can_undo());
    }

    #[test]
    fn expand_revolve_replaces_with_faces() {
        let mut app = app_with_square();
        app.revolve_3d.insert(0, Revolve3D::default());
        app.apply(Action::ExpandRevolveRender { shape_id: 0 });
        assert!(app.doc.shapes.len() > 1, "revolve expanded into a lathe");
        assert!(!app.revolve_3d.contains_key(&0));
    }

    #[test]
    fn create_gradient_mesh_object_over_selection() {
        let mut app = app_with_square();
        app.apply(Action::CreateGradientMeshObject { rows: 3, cols: 3 });
        let m = app.gradient_mesh_obj.as_ref().expect("mesh created");
        assert_eq!((m.rows, m.cols), (3, 3));
        assert_eq!(m.nodes.len(), 9);
        // Node 0 sits at the shape's bbox origin.
        assert_eq!(m.node(0, 0).unwrap().pos, (0.0, 0.0));
    }

    #[test]
    fn gradient_mesh_set_color_and_clear() {
        let mut app = app_with_square();
        app.apply(Action::CreateGradientMeshObject { rows: 2, cols: 2 });
        app.apply(Action::SetGradientMeshNodeColor {
            row: 0,
            col: 0,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        assert_eq!(
            app.gradient_mesh_obj.as_ref().unwrap().node(0, 0).unwrap().color,
            [1.0, 0.0, 0.0, 1.0]
        );
        app.apply(Action::ClearGradientMeshObject);
        assert!(app.gradient_mesh_obj.is_none());
    }

    #[test]
    fn picker_actions_update_state_and_apply() {
        let mut app = app_with_square();
        app.apply(Action::SetPickerHex("#FF8000".to_string()));
        assert_eq!(app.color_picker.hex(), "#FF8000");
        app.apply(Action::ApplyPickerToSelection);
        let fill = app.doc.shapes[0].fill_color().unwrap();
        assert!((fill[0] - 1.0).abs() < 1e-3 && fill[2].abs() < 1e-3);
        assert!(app.history.can_undo());
    }

    #[test]
    fn prefs_actions_update_state() {
        let mut app = app_with_square();
        app.apply(Action::SetPrefUndoLevels(50));
        assert_eq!(app.preferences.undo_levels, 50);
        app.apply(Action::SetPrefSnapToGrid(true));
        assert!(app.preferences.snap_to_grid);
        app.apply(Action::SetPrefGridSpacing(36.0));
        assert_eq!(app.preferences.grid_spacing, 36.0);
    }

    #[test]
    fn load_preferences_json_sanitizes() {
        let mut app = app_with_square();
        let json = r#"{"undo_levels":99999,"snap_to_point":false,"snap_to_grid":true,"show_grid":true,"grid_spacing":-3.0,"grid_subdivisions":0,"snap_tolerance":4.0,"unit":"Inches"}"#;
        app.apply(Action::LoadPreferencesJson(json.to_string()));
        assert_eq!(app.preferences.undo_levels, 1000, "clamped");
        assert_eq!(app.preferences.grid_spacing, 1.0, "clamped");
        assert!(app.preferences.snap_to_grid);
    }

    #[test]
    fn export_eps_writes_file() {
        let mut app = app_with_square();
        let dir = std::env::temp_dir();
        let path = dir.join(format!("contour_test_{}.eps", std::process::id()));
        let p = path.to_string_lossy().to_string();
        app.apply(Action::ExportDocument {
            path: p.clone(),
            format: ExportFormat::Eps,
        });
        let written = std::fs::read_to_string(&path).expect("eps written");
        assert!(written.starts_with("%!PS"), "exported real EPS");
        let _ = std::fs::remove_file(&path);
        assert!(app.status_message.is_some());
    }
}
