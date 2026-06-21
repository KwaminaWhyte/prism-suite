use super::*;

/// In-progress Text-tool edit: which raster layer holds the glyphs, where it was
/// placed (doc px), and the string typed so far. The host re-rasterizes the layer
/// from `string` on every keystroke (see `App::text_input`).
pub(super) struct TextEdit {
    pub(super) layer: LayerId,
    pub(super) origin: [f32; 2],
    pub(super) string: String,
}

impl App {
    pub(super) fn apply_text(&mut self, action: Action) {
        match action {
            Action::SetTextSize(s) => self.text_size = s.clamp(6.0, 400.0),
            _ => {}
        }
    }

    // ---- Text tool -----------------------------------------------------------
    //
    // Click places a fresh raster text layer at the click point and opens an
    // in-progress edit; subsequent keystrokes (routed through `text_input`) re-
    // rasterize it via `prism_io::text::render_text` through the engine. Mirrors
    // the egui app's Text tool (`add_text` + `sync_generated_layers`), but bakes
    // to a raster layer since the GPUI host has no re-rasterizing text-layer sync.

    /// Whether a Text edit is in progress (used by the host to route keystrokes).
    pub fn text_editing(&self) -> bool {
        self.text_edit.is_some()
    }

    /// Advance the cursor-blink counter. Call once per render frame when a text
    /// edit is active. Returns `true` if the blink phase changed (needs repaint).
    pub fn tick_cursor(&mut self) -> bool {
        self.cursor_blink_tick = self.cursor_blink_tick.wrapping_add(1);
        if self.cursor_blink_tick >= 30 {
            self.cursor_blink_tick = 0;
            self.cursor_blink_on = !self.cursor_blink_on;
            true
        } else {
            false
        }
    }

    /// Doc-px position `[x, y]` of the text cursor (end of string in active edit).
    /// Returns `None` when no text edit is in progress.
    pub fn text_cursor_doc_pos(&self) -> Option<[f32; 2]> {
        let edit = self.text_edit.as_ref()?;
        let char_w = self.text_size * 0.5; // rough monospace approximation
        let x = edit.origin[0] + edit.string.len() as f32 * char_w;
        Some([x, edit.origin[1]])
    }

    /// Place a new (empty) text layer at `doc` and begin an edit there. If an edit
    /// was already open, commit it first (clicking elsewhere starts a new run).
    pub(super) fn place_text(&mut self, doc: [f32; 2]) {
        self.commit_text();
        let id = self.host.rasterize_text_layer(
            &mut self.doc,
            "",
            self.text_size,
            self.brush.color,
            doc,
            prism_io::text::TextAlign::Left,
            None,
        );
        self.text_edit = Some(TextEdit {
            layer: id,
            origin: doc,
            string: String::new(),
        });
    }

    /// Feed a typed character / edit op into the active text run, then re-
    /// rasterize the layer. `ch` is the inserted text (may be multi-byte);
    /// `backspace` removes the last char instead. Returns true if it consumed the
    /// event (so the host can swallow the keystroke). No-op with no active edit.
    pub fn text_input(&mut self, ch: Option<&str>, backspace: bool) -> bool {
        let Some(edit) = self.text_edit.as_mut() else {
            return false;
        };
        if backspace {
            edit.string.pop();
        } else if let Some(s) = ch {
            edit.string.push_str(s);
        } else {
            return false;
        }
        let (layer, origin, string) = (edit.layer, edit.origin, edit.string.clone());
        self.host.update_text_layer(
            layer,
            &string,
            self.text_size,
            self.brush.color,
            origin,
            prism_io::text::TextAlign::Left,
            None,
        );
        true
    }

    /// Commit (finish) the in-progress text edit, if any. Called on Enter/Escape,
    /// a tool switch, or before placing a new run. The rasterized pixels stay; we
    /// just drop the edit handle so further typing doesn't target this layer.
    pub fn commit_text(&mut self) {
        self.text_edit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_text_size_clamps() {
        let mut app = App::new();
        app.apply(Action::SetTextSize(500.0));
        assert_eq!(app.text_size, 400.0);
        app.apply(Action::SetTextSize(1.0));
        assert_eq!(app.text_size, 6.0);
        app.apply(Action::SetTextSize(72.0));
        assert_eq!(app.text_size, 72.0);
    }

    #[test]
    fn test_text_editing_initial() {
        let app = App::new();
        assert!(!app.text_editing());
    }

    #[test]
    fn test_commit_text_clears_edit() {
        let mut app = App::new();
        app.commit_text();
        assert!(!app.text_editing());
    }

    #[test]
    fn test_text_input_no_edit() {
        let mut app = App::new();
        assert!(!app.text_input(Some("hello"), false));
    }

    #[test]
    fn test_tick_cursor() {
        let mut app = App::new();
        // tick_cursor increments counter; blink changes at 30
        let mut changed = false;
        for _ in 0..35 {
            if app.tick_cursor() {
                changed = true;
                break;
            }
        }
        assert!(changed);
    }

    #[test]
    fn test_text_cursor_doc_pos_none() {
        let app = App::new();
        assert!(app.text_cursor_doc_pos().is_none());
    }
}
