//! Apply arms for the TextField-driven editable-name / text-content actions.
//!
//! These back the *real typing* inputs (`prism_ui::TextField`) added to
//! Contour's floating windows and dock panels:
//!
//! - [`Action::RenameLayer`]          → shape (Layers panel) name
//! - [`Action::RenameSymbol`]         → symbol master name
//! - [`Action::SetTextObjectContent`] → a text object's full string
//!
//! Document-setup dimensions / bleed, the colour-picker hex field, the export
//! path, and artboard rename reuse pre-existing actions
//! (`SetDocSetupSize` / `SetDocSetupBleed` / `SetPickerHex` / `ExportDocument` /
//! `RenameArtboard`) — only typing the value into them is new, so no apply
//! changes were needed there.
//!
//! This handler is the **terminal** stage of the dispatch chain: `apply_batch12`
//! routes its previously-final catch-all here, and this file owns the real
//! `_ => {}` no-op.

use super::*;

/// Smallest width / height a typed dimension may collapse a selection to — keeps
/// a `ScaleSelection` factor finite and the shape pickable.
pub(crate) const MIN_DIM: f32 = 0.01;

/// Parse a free-form numeric string into an `f32`, tolerating a leading `+`,
/// surrounding whitespace, and a trailing unit / symbol suffix (`px`, `pt`, `%`,
/// `°`, `mm`, `in`, …). Returns `None` for empty input or when no leading number
/// is present. This backs every typeable numeric inspector field.
///
/// Examples: `"12"` → `12.0`, `" 3.5 px "` → `3.5`, `"50%"` → `50.0`,
/// `"-7pt"` → `-7.0`, `"+4"` → `4.0`, `"abc"` → `None`.
pub(crate) fn parse_f32(s: &str) -> Option<f32> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // Keep the longest leading prefix that still parses as a float, growing the
    // candidate one char-boundary at a time so a trailing unit ("px", "°", …) is
    // dropped without prescribing a fixed unit list.
    let mut best: Option<f32> = None;
    for (i, c) in t.char_indices() {
        let end = i + c.len_utf8();
        if let Ok(v) = t[..end].trim_start_matches('+').parse::<f32>() {
            best = Some(v);
        }
    }
    best
}

/// Parse a dimension (width / height / position / size) string. Reuses
/// [`parse_f32`] then rejects NaN / infinity so a bad token never reaches the
/// affine math. Negative values are allowed (X / Y may be negative); callers
/// that need a positive value clamp it (see [`MIN_DIM`]).
pub(crate) fn parse_dimension(s: &str) -> Option<f32> {
    parse_f32(s).filter(|v| v.is_finite())
}

impl App {
    /// Terminal dispatcher for the TextField-driven actions. Reached from
    /// `apply_batch12`'s catch-all; owns the final no-op.
    pub(super) fn apply_textfield(&mut self, action: Action) {
        match action {
            Action::RenameLayer { idx, name } => {
                if idx < self.doc.shapes.len() {
                    self.history.begin(&self.doc);
                    self.doc.shapes[idx].set_name(&name);
                    self.host.mark_dirty();
                    self.history.commit(&self.doc);
                }
            }
            Action::RenameSymbol { id, name } => {
                if self.symbol_lib.rename(id, &name).is_some() {
                    self.host.mark_dirty();
                }
            }
            Action::SetTextObjectContent { idx, text } => {
                if let Some(mut params) = self
                    .doc
                    .shapes
                    .get(idx)
                    .and_then(|s| s.text_params().cloned())
                {
                    // Multi-line type objects: keep the newlines the TextArea
                    // produced, only normalising CRLF / CR to '\n'.
                    params.text = text.replace("\r\n", "\n").replace('\r', "\n");
                    self.history.begin(&self.doc);
                    if self.doc.shapes[idx].set_text_params(params) {
                        self.host.mark_dirty();
                    }
                    self.history.commit(&self.doc);
                }
            }

            // ── Typeable numeric inspector fields ──────────────────────────
            // Each maps an absolute typed value onto the existing relative
            // translate / scale actions, computed against the live selection
            // bounding box (so the field's value lands exactly).
            Action::SetSelectionX(x) => {
                if let Some(b) = self.selection_bbox() {
                    let dx = x - b[0];
                    if dx != 0.0 {
                        self.apply(Action::MoveSelection { dx, dy: 0.0 });
                    }
                }
            }
            Action::SetSelectionY(y) => {
                if let Some(b) = self.selection_bbox() {
                    let dy = y - b[1];
                    if dy != 0.0 {
                        self.apply(Action::MoveSelection { dx: 0.0, dy });
                    }
                }
            }
            Action::SetSelectionWidth(w) => {
                if let Some(b) = self.selection_bbox() {
                    let target = w.max(MIN_DIM);
                    if b[2] > f32::EPSILON {
                        let factor_x = target / b[2];
                        if (factor_x - 1.0).abs() > f32::EPSILON {
                            self.apply(Action::ScaleSelection { factor_x, factor_y: 1.0 });
                        }
                    }
                }
            }
            Action::SetSelectionHeight(h) => {
                if let Some(b) = self.selection_bbox() {
                    let target = h.max(MIN_DIM);
                    if b[3] > f32::EPSILON {
                        let factor_y = target / b[3];
                        if (factor_y - 1.0).abs() > f32::EPSILON {
                            self.apply(Action::ScaleSelection { factor_x: 1.0, factor_y });
                        }
                    }
                }
            }
            Action::SetLayerFilter(s) => {
                // View-only state — no checkpoint / dirty needed.
                self.layer_filter = s;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1×1 default document with one rectangle, returning the App.
    fn app_with_rect() -> App {
        let mut a = App::new();
        a.apply(Action::CreateShape {
            tool: Tool::Rect,
            a: [0.0, 0.0],
            b: [100.0, 80.0],
        });
        a
    }

    #[test]
    fn rename_layer_sets_and_clears_name() {
        let mut a = app_with_rect();
        let idx = a.doc.shapes.len() - 1;

        a.apply(Action::RenameLayer { idx, name: "Logo Frame".to_string() });
        assert_eq!(a.doc.shapes[idx].name(), Some("Logo Frame"));
        assert_eq!(a.doc.shapes[idx].display_name(), "Logo Frame");

        // Blank name clears back to the type label fallback.
        a.apply(Action::RenameLayer { idx, name: "   ".to_string() });
        assert_eq!(a.doc.shapes[idx].name(), None);
    }

    #[test]
    fn rename_layer_out_of_range_is_noop() {
        let mut a = app_with_rect();
        let before = a.doc.shapes.len();
        // Should not panic on an index past the end, and not mutate the doc.
        a.apply(Action::RenameLayer { idx: 999, name: "X".to_string() });
        assert_eq!(a.doc.shapes.len(), before);
    }

    #[test]
    fn rename_layer_is_undoable() {
        let mut a = app_with_rect();
        let idx = a.doc.shapes.len() - 1;
        a.apply(Action::RenameLayer { idx, name: "Renamed".to_string() });
        assert_eq!(a.doc.shapes[idx].name(), Some("Renamed"));
        a.apply(Action::Undo);
        assert_eq!(a.doc.shapes[idx].name(), None);
    }

    #[test]
    fn rename_symbol_updates_library_name() {
        let mut a = app_with_rect();
        // Select the rect and define a symbol from it.
        let idx = a.doc.shapes.len() - 1;
        a.apply(Action::SelectShape(idx));
        a.apply(Action::DefineSymbol("Original".to_string()));
        let id = a.symbol_lib.list.last().expect("a symbol was defined").id;

        a.apply(Action::RenameSymbol { id, name: "Badge".to_string() });
        let sym = a.symbol_lib.get(id).expect("symbol survives rename");
        assert_eq!(sym.name, "Badge");
    }

    #[test]
    fn rename_symbol_unknown_id_is_noop() {
        let mut a = app_with_rect();
        // No symbols defined → unknown id is a clean no-op.
        a.apply(Action::RenameSymbol { id: 4242, name: "Nope".to_string() });
        assert_eq!(a.symbol_lib.len(), 0);
    }

    #[test]
    fn set_text_object_content_replaces_string() {
        let mut a = App::new();
        a.apply(Action::PlaceText { x: 50.0, y: 50.0 });
        let idx = a.editing_text.expect("a text object is being edited");
        a.apply(Action::FinishText); // commits with empty -> would be removed
        // Re-place so we have a persistent object to set content on.
        a.apply(Action::PlaceText { x: 50.0, y: 50.0 });
        let idx2 = a.editing_text.expect("text object placed");
        let _ = idx;

        a.apply(Action::SetTextObjectContent {
            idx: idx2,
            text: "Hello Contour".to_string(),
        });
        let params = a.doc.shapes[idx2]
            .text_params()
            .expect("still a text object");
        assert_eq!(params.text, "Hello Contour");
    }

    #[test]
    fn set_text_object_content_keeps_newlines_normalizes_crlf() {
        // Multi-line type objects: TextArea-produced '\n' are preserved; CRLF and
        // bare CR are normalised to '\n'.
        let mut a = App::new();
        a.apply(Action::PlaceText { x: 0.0, y: 0.0 });
        let idx = a.editing_text.expect("text placed");
        a.apply(Action::SetTextObjectContent {
            idx,
            text: "line1\nline2\r\nline3\rline4".to_string(),
        });
        let params = a.doc.shapes[idx].text_params().expect("text object");
        assert_eq!(params.text, "line1\nline2\nline3\nline4");
    }

    // ── Numeric parse helpers ──────────────────────────────────────────────

    #[test]
    fn parse_f32_plain_and_decimal() {
        assert_eq!(parse_f32("12"), Some(12.0));
        assert_eq!(parse_f32("3.5"), Some(3.5));
        assert_eq!(parse_f32("0"), Some(0.0));
        assert_eq!(parse_f32("-7.25"), Some(-7.25));
    }

    #[test]
    fn parse_f32_tolerates_whitespace_sign_and_units() {
        assert_eq!(parse_f32("  4 "), Some(4.0));
        assert_eq!(parse_f32("+9"), Some(9.0));
        assert_eq!(parse_f32("12px"), Some(12.0));
        assert_eq!(parse_f32("3.5 pt"), Some(3.5));
        assert_eq!(parse_f32("50%"), Some(50.0));
        assert_eq!(parse_f32("45°"), Some(45.0));
        assert_eq!(parse_f32("-7pt"), Some(-7.0));
    }

    #[test]
    fn parse_f32_rejects_garbage_and_empty() {
        assert_eq!(parse_f32(""), None);
        assert_eq!(parse_f32("   "), None);
        assert_eq!(parse_f32("abc"), None);
        assert_eq!(parse_f32("px"), None);
    }

    #[test]
    fn parse_dimension_rejects_non_finite_text() {
        // "inf"/"nan" parse as f32 but must be rejected for geometry.
        assert_eq!(parse_dimension("inf"), None);
        assert_eq!(parse_dimension("NaN"), None);
        assert_eq!(parse_dimension("10mm"), Some(10.0));
        assert_eq!(parse_dimension("-3"), Some(-3.0));
    }

    // ── Typeable numeric inspector actions ─────────────────────────────────

    /// A rect from (10,20) sized 100×80, single-selected; returns (App, bbox).
    fn app_with_selected_rect() -> App {
        let mut a = App::new();
        a.apply(Action::CreateShape {
            tool: Tool::Rect,
            a: [10.0, 20.0],
            b: [110.0, 100.0],
        });
        let idx = a.doc.shapes.len() - 1;
        a.apply(Action::SelectShape(idx));
        a
    }

    #[test]
    fn set_selection_x_moves_left_edge() {
        let mut a = app_with_selected_rect();
        let before = a.selection_bbox().expect("bbox");
        assert_eq!(before[0], 10.0);
        a.apply(Action::SetSelectionX(50.0));
        let after = a.selection_bbox().expect("bbox");
        assert!((after[0] - 50.0).abs() < 1e-3, "x became {}", after[0]);
        // Width / Y unchanged.
        assert!((after[2] - before[2]).abs() < 1e-3);
        assert!((after[1] - before[1]).abs() < 1e-3);
    }

    #[test]
    fn set_selection_y_moves_top_edge() {
        let mut a = app_with_selected_rect();
        a.apply(Action::SetSelectionY(0.0));
        let after = a.selection_bbox().expect("bbox");
        assert!((after[1]).abs() < 1e-3, "y became {}", after[1]);
    }

    #[test]
    fn set_selection_width_scales_to_target() {
        let mut a = app_with_selected_rect();
        a.apply(Action::SetSelectionWidth(200.0));
        let after = a.selection_bbox().expect("bbox");
        assert!((after[2] - 200.0).abs() < 1e-2, "w became {}", after[2]);
    }

    #[test]
    fn set_selection_height_scales_to_target() {
        let mut a = app_with_selected_rect();
        a.apply(Action::SetSelectionHeight(40.0));
        let after = a.selection_bbox().expect("bbox");
        assert!((after[3] - 40.0).abs() < 1e-2, "h became {}", after[3]);
    }

    #[test]
    fn set_selection_width_clamps_to_min_dim() {
        let mut a = app_with_selected_rect();
        // Zero would make the shape vanish; it clamps to MIN_DIM instead.
        a.apply(Action::SetSelectionWidth(0.0));
        let after = a.selection_bbox().expect("bbox");
        assert!(after[2] >= MIN_DIM - 1e-6 && after[2] < 1.0, "w became {}", after[2]);
    }

    #[test]
    fn set_selection_numeric_with_no_selection_is_noop() {
        let mut a = App::new();
        // Clear any seeded selection via the public fields.
        a.selection.clear();
        a.selected = None;
        a.secondary = None;
        // No geometry selected → clean no-op (no panic).
        a.apply(Action::SetSelectionX(99.0));
        a.apply(Action::SetSelectionWidth(99.0));
        assert!(a.selection_bbox().is_none());
    }

    #[test]
    fn set_selection_x_is_undoable() {
        let mut a = app_with_selected_rect();
        a.apply(Action::SetSelectionX(300.0));
        assert!((a.selection_bbox().unwrap()[0] - 300.0).abs() < 1e-3);
        a.apply(Action::Undo);
        assert!((a.selection_bbox().unwrap()[0] - 10.0).abs() < 1e-3);
    }

    // ── Layers filter ──────────────────────────────────────────────────────

    #[test]
    fn set_layer_filter_stores_string() {
        let mut a = App::new();
        assert_eq!(a.layer_filter, "");
        a.apply(Action::SetLayerFilter("logo".to_string()));
        assert_eq!(a.layer_filter, "logo");
        a.apply(Action::SetLayerFilter(String::new()));
        assert_eq!(a.layer_filter, "");
    }

    #[test]
    fn set_text_object_content_on_non_text_is_noop() {
        let mut a = app_with_rect();
        let idx = a.doc.shapes.len() - 1;
        // Setting content on a rectangle does nothing (and never panics).
        a.apply(Action::SetTextObjectContent { idx, text: "x".to_string() });
        assert!(a.doc.shapes[idx].text_params().is_none());
    }
}
