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
                    // Single-line field: collapse any stray newlines.
                    params.text = text.replace(['\n', '\r'], " ");
                    self.history.begin(&self.doc);
                    if self.doc.shapes[idx].set_text_params(params) {
                        self.host.mark_dirty();
                    }
                    self.history.commit(&self.doc);
                }
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
    fn set_text_object_content_collapses_newlines() {
        let mut a = App::new();
        a.apply(Action::PlaceText { x: 0.0, y: 0.0 });
        let idx = a.editing_text.expect("text placed");
        a.apply(Action::SetTextObjectContent {
            idx,
            text: "line1\nline2\r\nline3".to_string(),
        });
        let params = a.doc.shapes[idx].text_params().expect("text object");
        assert_eq!(params.text, "line1 line2  line3");
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
