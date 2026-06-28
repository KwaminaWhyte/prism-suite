//! Real, focusable editable text inputs owned by the [`Drift`] root view.
//!
//! All `prism_ui::TextField` / `prism_ui::TextArea` entities live here on
//! [`TextFields`] and are created once via [`Drift::ensure_text_fields`] (which
//! must run during `render`, not `Drift::new`, because each field's callbacks
//! capture a `WeakEntity<Drift>` that only exists once the entity is created).
//! Panels are stateless and receive clones; every field routes its edits back
//! into [`App::apply`] through that weak handle.
//!
//! Field groups:
//! * **Multi-line editors** ([`TextArea`]): the AI motion prompt, the AI-script
//!   prompt, and the Rhai/JS script source editor. Plain Enter inserts a
//!   newline; Cmd/Ctrl+Enter submits (the script editor's submit re-applies the
//!   source so the user can "run" it from the keyboard).
//! * **Numeric fields** ([`TextField`]): the active layer's transform
//!   (x / y / scale x / scale y / rotation / opacity), the keyframe value at the
//!   playhead, and the document settings (fps / width / height / duration). Each
//!   parses with a [`crate::numeric`] helper on submit and dispatches the
//!   matching `Action::Set*`.

use gpui::{AppContext, Context, Entity, WeakEntity};
use prism_ui::{TextArea, TextField};

use crate::app_state::{self, Action, LayerTransform};
use crate::numeric;
use crate::Drift;

/// Identifies a typeable numeric field so a single seed/commit path can serve
/// all of them. Layer-transform variants resolve against the active layer; the
/// keyframe variant writes a keyframe at the current playhead; document
/// variants edit `app.document`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericField {
    PositionX,
    PositionY,
    ScaleX,
    ScaleY,
    Rotation,
    Opacity,
    /// Opacity keyframe value at the current playhead frame.
    KeyframeValue,
    DocFps,
    DocWidth,
    DocHeight,
    DocDuration,
}

impl NumericField {
    /// The current display string for this field, read from app state. Used to
    /// seed the `TextField` when the user begins editing it.
    pub fn current_value(self, app: &app_state::App) -> String {
        let t = app
            .active_layer
            .and_then(|lid| app.transforms.get(&lid))
            .cloned()
            .unwrap_or_else(LayerTransform::new);
        match self {
            NumericField::PositionX => format!("{:.1}", t.x),
            NumericField::PositionY => format!("{:.1}", t.y),
            NumericField::ScaleX => format!("{:.2}", t.scale_x),
            NumericField::ScaleY => format!("{:.2}", t.scale_y),
            NumericField::Rotation => format!("{:.1}", t.rotation),
            NumericField::Opacity => format!("{:.0}", t.opacity * 100.0),
            NumericField::KeyframeValue => {
                let v = crate::view_render::kf_interpolate(
                    &app.keyframes,
                    app.active_layer.unwrap_or(usize::MAX),
                    "opacity",
                    app.current_frame,
                    t.opacity,
                );
                format!("{v:.3}")
            }
            NumericField::DocFps => format!("{:.0}", app.document.fps),
            NumericField::DocWidth => format!("{}", app.document.width),
            NumericField::DocHeight => format!("{}", app.document.height),
            NumericField::DocDuration => format!("{}", app.document.duration_frames),
        }
    }

    /// Parse the typed `text` and dispatch the matching `Set*` action against
    /// `app`. No-op (keeping the previous value) when the text doesn't parse.
    pub fn commit(self, text: &str, app: &mut app_state::App) {
        match self {
            NumericField::PositionX => {
                if let (Some(id), Some(v)) = (app.active_layer, numeric::parse_position(text)) {
                    let y = app.transforms.get(&id).map(|t| t.y).unwrap_or(0.0);
                    app.apply(Action::SetLayerPosition { id, x: v, y });
                }
            }
            NumericField::PositionY => {
                if let (Some(id), Some(v)) = (app.active_layer, numeric::parse_position(text)) {
                    let x = app.transforms.get(&id).map(|t| t.x).unwrap_or(0.0);
                    app.apply(Action::SetLayerPosition { id, x, y: v });
                }
            }
            NumericField::ScaleX => {
                if let (Some(id), Some(v)) = (app.active_layer, numeric::parse_scale(text)) {
                    let sy = app.transforms.get(&id).map(|t| t.scale_y).unwrap_or(1.0);
                    app.apply(Action::SetLayerScale { id, x: v, y: sy });
                }
            }
            NumericField::ScaleY => {
                if let (Some(id), Some(v)) = (app.active_layer, numeric::parse_scale(text)) {
                    let sx = app.transforms.get(&id).map(|t| t.scale_x).unwrap_or(1.0);
                    app.apply(Action::SetLayerScale { id, x: sx, y: v });
                }
            }
            NumericField::Rotation => {
                if let (Some(id), Some(v)) = (app.active_layer, numeric::parse_rotation(text)) {
                    app.apply(Action::SetLayerRotation { id, degrees: v });
                }
            }
            NumericField::Opacity => {
                if let (Some(id), Some(v)) =
                    (app.active_layer, numeric::parse_opacity_percent(text))
                {
                    app.apply(Action::SetLayerOpacity { id, opacity: v });
                }
            }
            NumericField::KeyframeValue => {
                if let (Some(id), Some(v)) =
                    (app.active_layer, numeric::parse_keyframe_value(text))
                {
                    let frame = app.current_frame;
                    app.apply(Action::SetPropertyAtFrame {
                        layer_id: id,
                        property: "opacity".to_string(),
                        frame,
                        value: v,
                    });
                }
            }
            NumericField::DocFps => {
                if let Some(v) = numeric::parse_fps(text) {
                    app.apply(Action::SetDocumentFps(v));
                }
            }
            NumericField::DocWidth => {
                if let Some(v) = numeric::parse_dimension(text) {
                    app.apply(Action::SetDocumentWidth(v));
                }
            }
            NumericField::DocHeight => {
                if let Some(v) = numeric::parse_dimension(text) {
                    app.apply(Action::SetDocumentHeight(v));
                }
            }
            NumericField::DocDuration => {
                if let Some(v) = numeric::parse_duration_frames(text) {
                    app.apply(Action::SetDocumentDuration(v));
                }
            }
        }
    }
}

/// The set of real, focusable editable text inputs owned by the root view.
pub struct TextFields {
    // ── Multi-line editors ──────────────────────────────────────────────────
    /// AnimateDiff motion prompt (multi-line) — `SetAiMotionPrompt`.
    pub motion_prompt: Entity<TextArea>,
    /// Natural-language AI-script prompt (multi-line) — `SetAiScriptPrompt`.
    pub script_prompt: Entity<TextArea>,
    /// Editable script source (multi-line) — `SetScriptSource`; Cmd/Ctrl+Enter
    /// re-applies (runs) the current source.
    pub script_source: Entity<TextArea>,

    // ── Single-line text ────────────────────────────────────────────────────
    /// Inline layer-rename field — `RenameLayer`.
    pub layer_rename: Entity<TextField>,

    // ── Numeric fields (parse + dispatch on submit) ─────────────────────────
    pub pos_x: Entity<TextField>,
    pub pos_y: Entity<TextField>,
    pub scale_x: Entity<TextField>,
    pub scale_y: Entity<TextField>,
    pub rotation: Entity<TextField>,
    pub opacity: Entity<TextField>,
    pub keyframe_value: Entity<TextField>,
    pub doc_fps: Entity<TextField>,
    pub doc_width: Entity<TextField>,
    pub doc_height: Entity<TextField>,
    pub doc_duration: Entity<TextField>,
}

impl TextFields {
    /// Look up the numeric `TextField` entity backing a given [`NumericField`].
    pub fn numeric(&self, which: NumericField) -> &Entity<TextField> {
        match which {
            NumericField::PositionX => &self.pos_x,
            NumericField::PositionY => &self.pos_y,
            NumericField::ScaleX => &self.scale_x,
            NumericField::ScaleY => &self.scale_y,
            NumericField::Rotation => &self.rotation,
            NumericField::Opacity => &self.opacity,
            NumericField::KeyframeValue => &self.keyframe_value,
            NumericField::DocFps => &self.doc_fps,
            NumericField::DocWidth => &self.doc_width,
            NumericField::DocHeight => &self.doc_height,
            NumericField::DocDuration => &self.doc_duration,
        }
    }
}

/// Build one numeric `TextField` whose `on_submit` parses + dispatches the Set*
/// action for `which`, and clears the inspector's "editing" marker.
fn make_numeric_field(
    which: NumericField,
    weak: WeakEntity<Drift>,
    cx: &mut Context<Drift>,
) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .placeholder("value")
            .on_submit(move |text, win, cx| {
                if let Some(d) = weak.upgrade() {
                    let t = text.to_string();
                    d.update(cx, |d, cx| {
                        which.commit(&t, &mut d.app);
                        d.editing_numeric = None;
                        cx.notify();
                    });
                }
                win.refresh();
            })
    })
}

impl Drift {
    /// Lazily create the editable text inputs on first render.
    pub(crate) fn ensure_text_fields(&mut self, cx: &mut Context<Self>) {
        if self.text_fields.is_some() {
            return;
        }
        let weak = cx.entity().downgrade();

        // ── Motion prompt (multi-line) — syncs `app.ai_motion_prompt` ─────────
        let w = weak.clone();
        let motion_prompt = cx.new(|cx| {
            TextArea::new(cx)
                .placeholder("Describe the motion…")
                .rows(3)
                .initial_value(self.app.ai_motion_prompt.clone())
                .on_change(move |text, _win, cx| {
                    if let Some(d) = w.upgrade() {
                        let t = text.to_string();
                        d.update(cx, |d, cx| {
                            d.app.apply(Action::SetAiMotionPrompt(t));
                            cx.notify();
                        });
                    }
                })
        });

        // ── AI script prompt (multi-line) — syncs `app.ai_script_prompt` ──────
        let w = weak.clone();
        let script_prompt = cx.new(|cx| {
            TextArea::new(cx)
                .placeholder("Describe a script to generate…")
                .rows(3)
                .initial_value(self.app.ai_script_prompt.clone())
                .on_change(move |text, _win, cx| {
                    if let Some(d) = w.upgrade() {
                        let t = text.to_string();
                        d.update(cx, |d, cx| {
                            d.app.apply(Action::SetAiScriptPrompt(t));
                            cx.notify();
                        });
                    }
                })
        });

        // ── Script source editor (multi-line) — edits the active script's source.
        // on_change keeps the source synced as you type; Cmd/Ctrl+Enter re-applies
        // (runs) the current source through the same path.
        let w = weak.clone();
        let initial_src = self
            .app
            .scripts
            .first()
            .map(|s| s.source.clone())
            .unwrap_or_default();
        let script_source = cx.new(|cx| {
            TextArea::new(cx)
                // A TextArea renders its placeholder through GPUI's single-line
                // `shape_line`, which panics on `\n`. Keep placeholders inline.
                .placeholder(sanitize_inline("// frame script (JS / Rhai) — Cmd/Ctrl+Enter to run"))
                .rows(6)
                .initial_value(initial_src)
                .on_change({
                    let w = w.clone();
                    move |text, _win, cx| {
                        if let Some(d) = w.upgrade() {
                            apply_script_source(&d, text.to_string(), cx);
                        }
                    }
                })
                .on_submit(move |text, _win, cx| {
                    if let Some(d) = w.upgrade() {
                        apply_script_source(&d, text.to_string(), cx);
                    }
                })
        });

        // ── Inline layer-rename field — commits to the active rename target ───
        let w = weak.clone();
        let layer_rename = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("Layer name")
                .on_submit(move |text, win, cx| {
                    if let Some(d) = w.upgrade() {
                        let t = text.to_string();
                        d.update(cx, |d, cx| {
                            if let Some(id) = d.renaming_layer.take() {
                                if !t.trim().is_empty() {
                                    d.app.apply(Action::RenameLayer { id, name: t });
                                }
                            }
                            cx.notify();
                        });
                    }
                    win.refresh();
                })
        });

        self.text_fields = Some(TextFields {
            motion_prompt,
            script_prompt,
            script_source,
            layer_rename,
            pos_x: make_numeric_field(NumericField::PositionX, weak.clone(), cx),
            pos_y: make_numeric_field(NumericField::PositionY, weak.clone(), cx),
            scale_x: make_numeric_field(NumericField::ScaleX, weak.clone(), cx),
            scale_y: make_numeric_field(NumericField::ScaleY, weak.clone(), cx),
            rotation: make_numeric_field(NumericField::Rotation, weak.clone(), cx),
            opacity: make_numeric_field(NumericField::Opacity, weak.clone(), cx),
            keyframe_value: make_numeric_field(NumericField::KeyframeValue, weak.clone(), cx),
            doc_fps: make_numeric_field(NumericField::DocFps, weak.clone(), cx),
            doc_width: make_numeric_field(NumericField::DocWidth, weak.clone(), cx),
            doc_height: make_numeric_field(NumericField::DocHeight, weak.clone(), cx),
            doc_duration: make_numeric_field(NumericField::DocDuration, weak, cx),
        });
    }
}

/// Collapse any newlines (and carriage returns) in `s` into single spaces so the
/// result is safe to hand to a single-line text shaper.
///
/// GPUI's `shape_line` — used by `TextField`/`TextArea` placeholders and any
/// plain `div().child(text)` label — panics with "text argument should not
/// contain newlines" on a `\n`. Placeholders and inline labels must therefore
/// never carry one; route possibly-multi-line strings through here first.
pub fn sanitize_inline(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

/// Ensure a script exists, then write `source` into the first script (the path
/// shared by the source editor's `on_change` and Cmd/Ctrl+Enter "run").
fn apply_script_source(d: &Entity<Drift>, source: String, cx: &mut gpui::App) {
    d.update(cx, |d, cx| {
        if d.app.scripts.is_empty() {
            d.app.apply(Action::AddScript {
                name: "Generated Script".to_string(),
                language: app_state::ScriptLanguage::JavaScript,
            });
        }
        if let Some(id) = d.app.scripts.first().map(|s| s.id) {
            d.app
                .apply(Action::SetScriptSource { script_id: id, source });
        }
        cx.notify();
    });
}

#[cfg(test)]
mod tests {
    use super::sanitize_inline;

    #[test]
    fn sanitize_inline_strips_newlines() {
        // The script-source placeholder used to carry a `\n`, which panicked
        // GPUI's single-line shaper at startup. Inline strings must be flat.
        let s = sanitize_inline("// frame script (JS / Rhai)\n// Cmd/Ctrl+Enter to run");
        assert!(!s.contains('\n'));
        assert!(!s.contains('\r'));
        assert_eq!(s, "// frame script (JS / Rhai) // Cmd/Ctrl+Enter to run");
    }

    #[test]
    fn sanitize_inline_handles_crlf_and_multiple_lines() {
        let s = sanitize_inline("line1\r\nline2\nline3");
        assert!(!s.contains('\n') && !s.contains('\r'));
        assert_eq!(s, "line1  line2 line3");
    }

    #[test]
    fn sanitize_inline_leaves_plain_text_untouched() {
        assert_eq!(sanitize_inline("no newlines here"), "no newlines here");
    }
}
