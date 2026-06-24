//! Preferences — a floating OS-level child window.
//!
//! Opened from the toolbar's gear button (see `main.rs` / `panels::toolbar`).
//! Like the welcome screen it holds a [`WeakEntity<Reel>`] so its controls
//! dispatch [`Action`]s into the main view and read its state back, then it can
//! close itself.
//!
//! It surfaces the auto-save preferences that exist in `App`
//! ([`Action::SetAutoSaveEnabled`] / [`Action::SetAutoSaveInterval`] /
//! [`Action::TriggerAutoSave`]) plus a keybinding list. Reel has no keymap model
//! yet, so the keybinding section lists the built-in shortcuts read-only with a
//! note; when a `RemapKeybinding`/`ResetKeymap` action lands the rows become
//! editable without restructuring this window.

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::Action;
use crate::Reel;

pub struct PreferencesView {
    focus: FocusHandle,
    app_entity: WeakEntity<Reel>,
}

impl Focusable for PreferencesView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PreferencesView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Reel>) -> Self {
        Self { focus, app_entity }
    }

    /// Dispatch an action into the main Reel view and request a redraw there.
    fn dispatch(&self, cx: &mut Context<Self>, action: Action) {
        if let Some(entity) = self.app_entity.upgrade() {
            entity.update(cx, |reel, cx| {
                reel.app.apply(action);
                cx.notify();
            });
        }
    }

    /// Read a snapshot of the prefs we display from the main view.
    fn snapshot(&self, cx: &mut Context<Self>) -> (bool, u32) {
        self.app_entity
            .upgrade()
            .map(|e| {
                let reel = e.read(cx);
                (reel.app.auto_save_enabled, reel.app.auto_save_interval_sec)
            })
            .unwrap_or((true, 300))
    }
}

/// Built-in shortcuts shown in the (currently read-only) keybinding list.
const KEYBINDINGS: [(&str, &str); 8] = [
    ("Play / Pause", "Space"),
    ("Razor / Cut", "C"),
    ("Selection Tool", "V"),
    ("Mark In", "I"),
    ("Mark Out", "O"),
    ("Add Marker", "M"),
    ("Export", "Cmd+M"),
    ("Save Project", "Cmd+S"),
];

impl Render for PreferencesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (autosave_on, interval) = self.snapshot(cx);

        // ---- Auto-save toggle ----
        let toggle = div()
            .id("pref-autosave-toggle")
            .px(px(12.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .bg(if autosave_on { colors::accent() } else { colors::surface_overlay() })
            .text_color(colors::text_primary())
            .text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(if autosave_on { colors::accent_hover() } else { colors::tool_hover() }))
            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                this.dispatch(cx, Action::SetAutoSaveEnabled(!autosave_on));
                cx.notify();
            }))
            .child(if autosave_on { "Enabled" } else { "Disabled" });

        // ---- Interval stepper (clamped to >=30s in app_state) ----
        let dec_iv = interval.saturating_sub(30).max(30);
        let inc_iv = interval + 30;
        let interval_stepper = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .id("pref-iv-dec")
                    .w(px(20.0)).h(px(18.0))
                    .flex().items_center().justify_center()
                    .rounded(px(3.0)).bg(colors::surface_raised())
                    .text_color(colors::text_primary()).text_size(px(13.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.dispatch(cx, Action::SetAutoSaveInterval(dec_iv));
                        cx.notify();
                    }))
                    .child("\u{2212}"),
            )
            .child(
                div()
                    .px(px(8.0)).py(px(2.0))
                    .min_w(px(64.0))
                    .rounded(px(3.0)).bg(colors::surface_overlay())
                    .text_color(colors::text_primary()).text_size(px(font_size::SM))
                    .child(format!("{interval}s")),
            )
            .child(
                div()
                    .id("pref-iv-inc")
                    .w(px(20.0)).h(px(18.0))
                    .flex().items_center().justify_center()
                    .rounded(px(3.0)).bg(colors::surface_raised())
                    .text_color(colors::text_primary()).text_size(px(13.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.dispatch(cx, Action::SetAutoSaveInterval(inc_iv));
                        cx.notify();
                    }))
                    .child("+"),
            );

        let trigger_now = div()
            .id("pref-autosave-now")
            .px(px(10.0)).py(px(4.0))
            .rounded(px(4.0)).bg(colors::surface_raised())
            .text_color(colors::text_secondary()).text_size(px(font_size::XS))
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                this.dispatch(cx, Action::TriggerAutoSave);
                cx.notify();
            }))
            .child("Save Now");

        let pref_row = |label: &'static str, control: gpui::AnyElement| {
            div()
                .flex().flex_row().items_center().justify_between()
                .py(px(6.0))
                .child(
                    div().text_color(colors::text_secondary()).text_size(px(font_size::SM))
                        .child(label),
                )
                .child(control)
        };

        // ---- General prefs section ----
        let general = div()
            .flex().flex_col()
            .child(
                div().text_size(px(font_size::SM)).text_color(colors::text_secondary())
                    .mb(px(4.0)).child("AUTO-SAVE"),
            )
            .child(pref_row("Auto-save project", toggle.into_any_element()))
            .child(pref_row("Save interval", interval_stepper.into_any_element()))
            .child(pref_row("Manual save", trigger_now.into_any_element()));

        // ---- Keybindings list (read-only for now) ----
        let kb_rows: Vec<gpui::AnyElement> = KEYBINDINGS
            .iter()
            .map(|(action, key)| {
                div()
                    .flex().flex_row().items_center().justify_between()
                    .px(px(8.0)).py(px(4.0))
                    .border_b_1().border_color(colors::surface_border())
                    .child(
                        div().text_color(colors::text_primary()).text_size(px(font_size::SM))
                            .child(*action),
                    )
                    .child(
                        div()
                            .px(px(6.0)).py(px(1.0))
                            .rounded(px(3.0)).bg(colors::surface_overlay())
                            .text_color(colors::text_secondary()).text_size(px(font_size::XS))
                            .child(*key),
                    )
                    .into_any_element()
            })
            .collect();

        let keybindings = div()
            .flex().flex_col()
            .mt(px(12.0))
            .child(
                div().text_size(px(font_size::SM)).text_color(colors::text_secondary())
                    .mb(px(4.0)).child("KEYBOARD SHORTCUTS"),
            )
            .child(
                div()
                    .border_1().border_color(colors::surface_border())
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .children(kb_rows),
            )
            .child(
                div().mt(px(4.0)).text_size(px(font_size::XS)).text_color(colors::text_disabled())
                    .child("Remapping shortcuts is not yet available."),
            );

        let close_btn = div()
            .id("pref-close")
            .px(px(12.0)).py(px(5.0))
            .rounded(px(4.0)).bg(colors::accent())
            .text_color(colors::text_primary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::accent_hover()))
            .on_click(cx.listener(move |_this, _e: &ClickEvent, win, _cx| {
                win.remove_window();
            }))
            .child("Done");

        div()
            .size_full()
            .flex().flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .p(px(20.0))
            .child(
                div()
                    .text_size(px(22.0))
                    .text_color(colors::text_primary())
                    .font_weight(gpui::FontWeight::BOLD)
                    .mb(px(12.0))
                    .child("Preferences"),
            )
            .child(general)
            .child(keybindings)
            .child(
                div()
                    .flex().flex_row().justify_end()
                    .mt(px(16.0))
                    .child(close_btn),
            )
    }
}
