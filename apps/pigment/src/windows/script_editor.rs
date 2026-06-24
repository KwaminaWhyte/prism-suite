//! Script Editor — floating OS-level window hosting a multi-line `TextArea`
//! for the rhai / line-DSL scripting sandbox (Photoshop *File > Scripts* /
//! Illustrator's scripting console parity).
//!
//! The editor makes the existing scripting feature (`app_state/scripting.rs`)
//! actually usable: the `TextArea`'s content IS the script source — every edit
//! pushes it into the model via [`Action::SetScriptSource`], and **Cmd/Ctrl+Enter**
//! (or the Run button) fires [`Action::RunCurrentScript`], which compiles the
//! source (auto-detecting rhai vs. the line-DSL) and dispatches each command
//! through `App::apply`. The script log (`app.script_log`) is rendered live
//! beneath the editor; "Clear Log" fires [`Action::ClearScriptLog`].
//!
//! The view holds a `WeakEntity<Pigment>` (to dispatch actions) and owns the
//! `Entity<TextArea>` it renders. It seeds the area once from the main entity's
//! `script_source` at construction so re-opening the window restores the last
//! script. It never mutates `App` outside an `entity.update(..)` closure.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use prism_ui::{colors, font_size, TextArea};

use crate::app_state::Action;
use crate::windows::window_header;
use crate::Pigment;

/// A short starter script shown the first time the editor opens (when the
/// model has no source yet) so the user has something runnable immediately.
const STARTER_SCRIPT: &str = "// Pigment scripting — rhai or the line-DSL.\n\
                              // Cmd/Ctrl+Enter or Run to execute.\n\
                              log(\"hello from a script\");\n\
                              set_brush_size(48);\n\
                              set_color(255, 80, 0);";

pub struct ScriptEditorView {
    pub focus: FocusHandle,
    app_entity: WeakEntity<Pigment>,
    /// The multi-line source editor; its content is mirrored into the model on
    /// every change.
    source: Entity<TextArea>,
}

impl ScriptEditorView {
    /// Build the editor, seeding the `TextArea` from the main entity's current
    /// `script_source` (falling back to a starter script when empty). The
    /// area's `on_change` mirrors edits into `SetScriptSource`; `on_submit`
    /// (Cmd/Ctrl+Enter) runs the current script.
    pub fn new(
        focus: FocusHandle,
        app_entity: WeakEntity<Pigment>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Seed from the live model so re-opening keeps the last script.
        let seed = app_entity
            .upgrade()
            .map(|e| e.read(cx).app.script_source.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| STARTER_SCRIPT.to_string());

        let change_entity = app_entity.clone();
        let submit_entity = app_entity.clone();
        let source = cx.new(|cx| {
            TextArea::new(cx)
                .placeholder("Type a script…  (rhai or line-DSL)")
                .initial_value(seed.clone())
                .rows(14)
                .on_change(move |text, _win, app| {
                    let src = text.to_string();
                    if let Some(e) = change_entity.upgrade() {
                        e.update(app, |root, cx| {
                            root.app.apply(Action::SetScriptSource(src));
                            cx.notify();
                        });
                    }
                })
                .on_submit(move |_text, _win, app| {
                    if let Some(e) = submit_entity.upgrade() {
                        e.update(app, |root, cx| {
                            root.app.apply(Action::RunCurrentScript);
                            cx.notify();
                        });
                    }
                })
        });

        // Push the seed into the model immediately so a first-run (before any
        // edit) executes the visible starter text rather than an empty source.
        if let Some(e) = app_entity.upgrade() {
            e.update(cx, |root, _cx| {
                root.app.apply(Action::SetScriptSource(seed));
            });
        }

        Self {
            focus,
            app_entity,
            source,
        }
    }
}

impl Focusable for ScriptEditorView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ScriptEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Snapshot the live script log + status from the main entity (read-only).
        let (log, status): (Vec<String>, Option<String>) = match self.app_entity.upgrade() {
            Some(e) => {
                let app = &e.read(cx).app;
                (app.script_log.clone(), app.status_message.clone())
            }
            None => (Vec::new(), None),
        };

        let entity = self.app_entity.clone();
        let source = self.source.clone();

        // ── Run / Clear buttons ──────────────────────────────────────────
        let run_entity = entity.clone();
        let run_btn = div()
            .id("script-run")
            .px_3()
            .py(px(5.0))
            .rounded_md()
            .bg(colors::accent())
            .text_size(px(font_size::SM))
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::accent_hover()))
            .on_click(cx.listener(move |_this, _ev, _win, cx| {
                if let Some(e) = run_entity.upgrade() {
                    e.update(cx, |root, cx| {
                        root.app.apply(Action::RunCurrentScript);
                        cx.notify();
                    });
                }
                cx.notify();
            }))
            .child("Run  (⌘⏎)");

        let clear_entity = entity.clone();
        let clear_btn = div()
            .id("script-clear-log")
            .px_3()
            .py(px(5.0))
            .rounded_md()
            .bg(colors::surface_overlay())
            .text_size(px(font_size::SM))
            .text_color(colors::text_secondary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_border()))
            .on_click(cx.listener(move |_this, _ev, _win, cx| {
                if let Some(e) = clear_entity.upgrade() {
                    e.update(cx, |root, cx| {
                        root.app.apply(Action::ClearScriptLog);
                        cx.notify();
                    });
                }
                cx.notify();
            }))
            .child("Clear Log");

        let toolbar = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .py_2()
            .child(run_btn)
            .child(clear_btn)
            .when_some(status, |d, s| {
                d.child(
                    div()
                        .flex_1()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child(s),
                )
            });

        // ── Log pane (live) ──────────────────────────────────────────────
        let log_pane = div()
            .id("script-log")
            .w_full()
            .flex_1()
            .min_h(px(80.0))
            .overflow_y_scroll()
            .p_2()
            .rounded_md()
            .bg(colors::surface_overlay())
            .border_1()
            .border_color(colors::surface_border())
            .text_size(px(font_size::XS))
            .text_color(colors::text_secondary())
            .flex()
            .flex_col()
            .gap_1()
            .when(log.is_empty(), |d| {
                d.child(
                    div()
                        .text_color(colors::text_disabled())
                        .child("Log is empty. Run a script to see output."),
                )
            })
            .children(log.iter().enumerate().map(|(i, line)| {
                div()
                    .id(("script-log-line", i))
                    .child(format!("› {line}"))
            }));

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .child(window_header("Script Editor", |_ev, win, _cx| {
                win.remove_window();
            }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .px_4()
                    .py_3()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(font_size::XS))
                            .text_color(colors::text_disabled())
                            .child("SCRIPT SOURCE"),
                    )
                    // The multi-line editor — its content is the script source.
                    .child(div().w_full().child(source))
                    .child(toolbar)
                    .child(
                        div()
                            .text_size(px(font_size::XS))
                            .text_color(colors::text_disabled())
                            .child("LOG"),
                    )
                    .child(log_pane),
            )
    }
}
