//! Inline editing state for the root [`Contour`] view: renaming Layers /
//! Symbols / Artboards rows, and editing a text object's content — both backed
//! by a real focused [`prism_ui::TextField`].
//!
//! The panels are stateless render fns (`(app, cx)`), so the editing `TextField`
//! entities live here on the root view. A panel renders the field (passed down
//! from `Contour::render`) in place of the static name when its row is the
//! active target; the field's `on_submit` / `on_change` dispatches the matching
//! [`Action`] back through `app.apply` via a `WeakEntity<Contour>` — the same
//! round-trip the welcome window uses.

use gpui::{px, AppContext, Context, Entity, Focusable, Window};
use prism_ui::TextField;

use crate::app_state::Action;
use crate::Contour;

/// What an in-progress inline rename targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameTarget {
    /// A Layers-panel row = shape at paint index.
    Layer(usize),
    /// A symbol master by library id.
    Symbol(u64),
    /// An artboard entry at index.
    Artboard(usize),
}

/// An active inline-rename session: which row + its focused text field.
pub struct RenameEdit {
    pub target: RenameTarget,
    pub field: Entity<TextField>,
}

/// An active text-object content edit: the object's paint index + its field.
pub struct TextEdit {
    pub idx: usize,
    pub field: Entity<TextField>,
}

impl Contour {
    /// Begin (or restart) an inline rename of `target`, seeding the field with
    /// `current` and focusing it. The field's submit dispatches the matching
    /// rename action and ends the session.
    pub(crate) fn begin_rename(
        &mut self,
        target: RenameTarget,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.weak_entity();
        let field = cx.new(|cx| {
            TextField::new(cx)
                .width(px(150.0))
                .initial_value(current)
                .on_submit(move |text, _win, cx| {
                    let name = text.to_string();
                    let _ = weak.update(cx, |c, cx| {
                        let action = match target {
                            RenameTarget::Layer(idx) => Action::RenameLayer { idx, name },
                            RenameTarget::Symbol(id) => Action::RenameSymbol { id, name },
                            RenameTarget::Artboard(idx) => Action::RenameArtboard { idx, name },
                        };
                        c.app.apply(action);
                        c.rename = None;
                        cx.notify();
                    });
                })
        });
        window.focus(&field.read(cx).focus_handle(cx));
        self.rename = Some(RenameEdit { target, field });
        cx.notify();
    }

    /// Begin a content edit on the text object at paint index `idx`, seeding the
    /// field with its current string and focusing it. Each keystroke dispatches
    /// [`Action::SetTextObjectContent`] so the on-canvas glyphs re-shape live.
    pub(crate) fn begin_text_edit(
        &mut self,
        idx: usize,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.weak_entity();
        let field = cx.new(|cx| {
            TextField::new(cx)
                .width(px(220.0))
                .placeholder("Type text…")
                .initial_value(current)
                .on_change(move |text, _win, cx| {
                    let text = text.to_string();
                    let _ = weak.update(cx, |c, cx| {
                        c.app.apply(Action::SetTextObjectContent { idx, text });
                        cx.notify();
                    });
                })
        });
        window.focus(&field.read(cx).focus_handle(cx));
        self.text_edit = Some(TextEdit { idx, field });
        cx.notify();
    }

    /// The active text-content edit session, if any.
    pub(crate) fn text_edit(&self) -> Option<&TextEdit> {
        self.text_edit.as_ref()
    }

    /// Commit + end the text-content edit session (Escape / tool change / click
    /// away). Routes through `FinishText` so an empty object is discarded.
    pub(crate) fn end_text_edit(&mut self, cx: &mut Context<Self>) {
        if self.text_edit.take().is_some() {
            self.app.apply(Action::FinishText);
            cx.notify();
        }
    }
}
