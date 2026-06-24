//! Inline numeric editing state for the root [`Contour`] view.
//!
//! The inspector / character panels are stateless render fns (`(app, cx)`), so
//! the focused [`prism_ui::TextField`] used to *type* a numeric value lives here
//! on the root view (the same pattern as [`rename_edit`](crate::rename_edit)).
//!
//! Only one numeric field is editable at a time — the one the user clicked to
//! start typing. A panel renders that field in place of the static value display
//! when [`NumericEdit::target`] matches its row; on submit (Enter) the field's
//! string is parsed via [`parse_dimension`](crate::app_state::parse_dimension)
//! and dispatched as the matching [`Action`], then the session ends.
//!
//! Steppers (`− value ＋`) stay as the live display / coarse adjustment; the
//! field is the precise-entry path Illustrator users expect.

use gpui::{px, AppContext, Context, Entity, Focusable, Window};
use prism_ui::TextField;

use crate::app_state::{parse_dimension, Action};
use crate::Contour;

/// Which numeric inspector value an in-progress inline edit targets. Each variant
/// maps a parsed value onto an existing `Action` when the field is submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericTarget {
    /// Selection bounding-box left edge (document X).
    X,
    /// Selection bounding-box top edge (document Y).
    Y,
    /// Selection bounding-box width.
    Width,
    /// Selection bounding-box height.
    Height,
    /// Stroke width of the selected shape.
    StrokeWidth,
    /// Fill-alpha opacity of the selected shape, as a 0–100 percentage.
    Opacity,
    /// Rotate the selection by the typed number of degrees (relative; the field
    /// displays `0` because the model stores no absolute per-shape rotation).
    Rotation,
    /// Font size of the primary selected text object (points).
    FontSize,
}

impl NumericTarget {
    /// Build the [`Action`] that applies `value` (already parsed) for this target.
    /// Returns `None` when the parsed value can't sensibly drive this target.
    pub fn action(self, value: f32) -> Option<Action> {
        Some(match self {
            NumericTarget::X => Action::SetSelectionX(value),
            NumericTarget::Y => Action::SetSelectionY(value),
            NumericTarget::Width => Action::SetSelectionWidth(value),
            NumericTarget::Height => Action::SetSelectionHeight(value),
            NumericTarget::StrokeWidth => Action::SetStrokeWidth(value.max(0.0)),
            // Field shows a percentage; the action wants a 0–1 fraction.
            NumericTarget::Opacity => Action::SetOpacity((value / 100.0).clamp(0.0, 1.0)),
            NumericTarget::Rotation => {
                if value == 0.0 {
                    return None;
                }
                Action::RotateSelection(value)
            }
            // `SetFontSize` sets the document default *and* re-sizes the selected
            // text object, so one target covers both the Character panel and an
            // active type selection.
            NumericTarget::FontSize => Action::SetFontSize(value.clamp(1.0, 2000.0)),
        })
    }
}

/// An active inline numeric edit: which value + its focused text field.
pub struct NumericEdit {
    pub target: NumericTarget,
    pub field: Entity<TextField>,
}

impl Contour {
    /// Begin (or restart) an inline numeric edit of `target`, seeding the field
    /// with `current` (the current display string) and focusing it. The field's
    /// submit parses the string and dispatches the matching action, then ends the
    /// session.
    pub(crate) fn begin_numeric_edit(
        &mut self,
        target: NumericTarget,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.weak_entity();
        let field = cx.new(|cx| {
            TextField::new(cx)
                .width(px(64.0))
                .initial_value(current)
                .on_submit(move |text, _win, cx| {
                    let parsed = parse_dimension(text);
                    let _ = weak.update(cx, |c, cx| {
                        if let Some(value) = parsed {
                            if let Some(action) = target.action(value) {
                                c.app.apply(action);
                            }
                        }
                        c.numeric_edit = None;
                        cx.notify();
                    });
                })
        });
        window.focus(&field.read(cx).focus_handle(cx));
        self.numeric_edit = Some(NumericEdit { target, field });
        cx.notify();
    }
}
