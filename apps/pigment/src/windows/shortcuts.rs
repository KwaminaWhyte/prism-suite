//! Keyboard-shortcuts editor — floating OS-level window. Lists the live
//! `app.shortcut_map` bindings from the main `Pigment` entity and emits
//! `RemapShortcut` / `UnbindShortcut` / `ResetShortcuts`.
//!
//! Rebinding flow (no text field needed): click a command row to *arm* it for
//! capture, then press any key combination — the window captures the keystroke,
//! turns it into a canonical chord string (e.g. `"Cmd+Shift+S"`) and emits
//! `RemapShortcut { command, chord }`. The map's conflict check (in
//! `app_state::shortcuts`) refuses collisions and sets a status message.

use std::collections::BTreeMap;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use prism_ui::{colors, font_size};

use crate::app_state::Action;
use crate::windows::window_header;
use crate::Pigment;

pub struct ShortcutsView {
    pub focus: FocusHandle,
    app_entity: WeakEntity<Pigment>,
    /// The command currently armed for capture; the next keystroke rebinds it.
    capturing: Option<String>,
}

impl ShortcutsView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Pigment>) -> Self {
        Self {
            focus,
            app_entity,
            capturing: None,
        }
    }

    /// Translate a key event into a canonical chord string, or `None` for a
    /// bare modifier press (which we ignore so the user can hold modifiers).
    fn chord_from_event(ev: &KeyDownEvent) -> Option<String> {
        let ks = &ev.keystroke;
        let key = ks.key.as_str();
        // Ignore lone modifier keys — wait for a real base key.
        if matches!(
            key,
            "left_shift"
                | "right_shift"
                | "left_control"
                | "right_control"
                | "left_alt"
                | "right_alt"
                | "left_cmd"
                | "right_cmd"
                | "shift"
                | "control"
                | "alt"
                | "cmd"
                | "platform"
                | "function"
        ) {
            return None;
        }
        let m = &ks.modifiers;
        let mut parts: Vec<String> = Vec::new();
        if m.platform {
            parts.push("Cmd".into());
        }
        if m.control {
            parts.push("Ctrl".into());
        }
        if m.alt {
            parts.push("Alt".into());
        }
        if m.shift {
            parts.push("Shift".into());
        }
        // Normalize the base key: single chars upper-case, named keys title-cased.
        let base = if key.chars().count() == 1 {
            key.to_ascii_uppercase()
        } else {
            // "escape" -> "Escape", "f1" -> "F1", "delete" -> "Delete"
            let mut c = key.chars();
            match c.next() {
                Some(first) => {
                    let mut s = first.to_ascii_uppercase().to_string();
                    s.push_str(&c.as_str().to_lowercase());
                    s
                }
                None => return None,
            }
        };
        parts.push(base);
        Some(parts.join("+"))
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        // Escape cancels capture.
        if ev.keystroke.key == "escape" {
            self.capturing = None;
            cx.notify();
            return;
        }
        let Some(command) = self.capturing.clone() else {
            return;
        };
        let Some(chord) = Self::chord_from_event(ev) else {
            return;
        };
        if let Some(e) = self.app_entity.upgrade() {
            e.update(cx, |root, cx| {
                root.app.apply(Action::RemapShortcut {
                    command: command.clone(),
                    chord: chord.clone(),
                });
                cx.notify();
            });
        }
        self.capturing = None;
        cx.notify();
    }
}

impl Focusable for ShortcutsView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ShortcutsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Snapshot bindings (sorted by command id for a stable list).
        let bindings: BTreeMap<String, String> = match self.app_entity.upgrade() {
            Some(e) => e
                .read(cx)
                .app
                .shortcut_map
                .bindings
                .iter()
                .map(|(k, v)| (k.clone(), v.to_text()))
                .collect(),
            None => BTreeMap::new(),
        };

        let capturing = self.capturing.clone();
        let entity = self.app_entity.clone();

        // Keep keyboard focus so key presses reach the capture handler.
        window.focus(&self.focus);

        let rows = bindings.into_iter().enumerate().map(|(i, (command, chord))| {
            let is_capturing = capturing.as_deref() == Some(command.as_str());
            let cmd_for_arm = command.clone();
            let cmd_for_unbind = command.clone();
            let e_unbind = entity.clone();

            let chord_cell = div()
                .id(("sc-chord", i as u64))
                .min_w(px(150.0))
                .px_2()
                .py(px(3.0))
                .rounded_md()
                .text_size(px(font_size::SM))
                .cursor_pointer()
                .map(|d| {
                    if is_capturing {
                        d.bg(colors::accent())
                            .text_color(colors::text_primary())
                            .child("Press keys…")
                    } else {
                        d.bg(colors::surface_overlay())
                            .text_color(colors::text_primary())
                            .child(chord.clone())
                    }
                })
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    // Toggle capture for this command.
                    this.capturing = if this.capturing.as_deref() == Some(cmd_for_arm.as_str()) {
                        None
                    } else {
                        Some(cmd_for_arm.clone())
                    };
                    cx.notify();
                }));

            let unbind_btn = div()
                .id(("sc-unbind", i as u64))
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .text_color(colors::text_secondary())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |_this, _ev, _win, cx| {
                    if let Some(en) = e_unbind.upgrade() {
                        let cmd = cmd_for_unbind.clone();
                        en.update(cx, |root, cx| {
                            root.app.apply(Action::UnbindShortcut(cmd));
                            cx.notify();
                        });
                    }
                    cx.notify();
                }))
                .child("✕");

            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .py(px(3.0))
                .child(
                    div()
                        .text_size(px(font_size::SM))
                        .text_color(colors::text_secondary())
                        .child(command),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(chord_cell)
                        .child(unbind_btn),
                )
        });

        let e_reset = entity.clone();
        let reset_btn = div()
            .id("sc-reset")
            .px_3()
            .py(px(5.0))
            .rounded_md()
            .bg(colors::surface_overlay())
            .text_size(px(font_size::SM))
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_border()))
            .on_click(cx.listener(move |this, _ev, _win, cx| {
                if let Some(en) = e_reset.upgrade() {
                    en.update(cx, |root, cx| {
                        root.app.apply(Action::ResetShortcuts);
                        cx.notify();
                    });
                }
                this.capturing = None;
                cx.notify();
            }))
            .child("Reset to Defaults");

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| this.on_key(ev, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .child(window_header("Keyboard Shortcuts", |_ev, win, _cx| {
                win.remove_window();
            }))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_disabled())
                    .child("Click a chord to arm it, then press the new key combination. Esc cancels."),
            )
            .child(
                div()
                    .id("sc-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_4()
                    .py_1()
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .child(reset_btn),
            )
    }
}
