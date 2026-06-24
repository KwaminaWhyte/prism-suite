//! Preferences — a floating OS-level child window.
//!
//! Holds a [`WeakEntity<Contour>`] (the welcome / Reel pattern). It edits
//! `app.preferences` via the Batch-13 actions:
//!
//! - undo levels    → [`Action::SetPrefUndoLevels`]
//! - snap to point  → [`Action::SetPrefSnapToPoint`]
//! - snap to grid   → [`Action::SetPrefSnapToGrid`]
//! - show grid      → [`Action::SetPrefShowGrid`]
//! - grid spacing   → [`Action::SetPrefGridSpacing`]
//! - unit           → [`Action::SetPrefUnit`]
//!
//! Toggles flip booleans; steppers nudge numbers; unit chips set the enum. The
//! "Done" button just closes the window — edits are live (no apply step).

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::prefs_color::{Preferences, PrefUnit};
use crate::app_state::Action;
use crate::Contour;

pub struct PreferencesView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
}

impl Focusable for PreferencesView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PreferencesView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Contour>) -> Self {
        Self { focus, app_entity }
    }

    fn dispatch(&self, cx: &mut Context<Self>, action: Action) {
        if let Some(entity) = self.app_entity.upgrade() {
            entity.update(cx, |c, cx| {
                c.app.apply(action);
                cx.notify();
            });
        }
    }

    fn snapshot(&self, cx: &mut Context<Self>) -> Preferences {
        self.app_entity
            .upgrade()
            .map(|e| e.read(cx).app.preferences.clone())
            .unwrap_or_default()
    }
}

const UNITS: [(PrefUnit, &str); 6] = [
    (PrefUnit::Pixels, "Pixels"),
    (PrefUnit::Points, "Points"),
    (PrefUnit::Picas, "Picas"),
    (PrefUnit::Inches, "Inches"),
    (PrefUnit::Millimeters, "mm"),
    (PrefUnit::Centimeters, "cm"),
];

impl Render for PreferencesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self.snapshot(cx);

        // --- Undo levels stepper (clamped 1..1000 in app_state, step 10) ---
        let undo = prefs.undo_levels;
        let undo_dec = undo.saturating_sub(10).max(1);
        let undo_inc = undo + 10;
        let undo_stepper = self.stepper(
            "pf-undo",
            format!("{undo}"),
            cx,
            move |this, cx| this.dispatch(cx, Action::SetPrefUndoLevels(undo_dec)),
            move |this, cx| this.dispatch(cx, Action::SetPrefUndoLevels(undo_inc)),
        );

        // --- Grid spacing stepper (clamped 1..10000, step 4 pt) ---
        let spacing = prefs.grid_spacing;
        let sp_dec = (spacing - 4.0).max(1.0);
        let sp_inc = spacing + 4.0;
        let spacing_stepper = self.stepper(
            "pf-spacing",
            format!("{spacing:.0} pt"),
            cx,
            move |this, cx| this.dispatch(cx, Action::SetPrefGridSpacing(sp_dec)),
            move |this, cx| this.dispatch(cx, Action::SetPrefGridSpacing(sp_inc)),
        );

        // --- Toggles ---
        let snap_point = prefs.snap_to_point;
        let snap_point_toggle = self.toggle("pf-snap-pt", snap_point, cx, move |this, cx| {
            this.dispatch(cx, Action::SetPrefSnapToPoint(!snap_point));
        });
        let snap_grid = prefs.snap_to_grid;
        let snap_grid_toggle = self.toggle("pf-snap-grid", snap_grid, cx, move |this, cx| {
            this.dispatch(cx, Action::SetPrefSnapToGrid(!snap_grid));
        });
        let show_grid = prefs.show_grid;
        let show_grid_toggle = self.toggle("pf-show-grid", show_grid, cx, move |this, cx| {
            this.dispatch(cx, Action::SetPrefShowGrid(!show_grid));
        });

        // --- Unit chips ---
        let unit_chips: Vec<gpui::AnyElement> = UNITS
            .iter()
            .enumerate()
            .map(|(i, (unit, label))| {
                let unit = *unit;
                let active = prefs.unit == unit;
                div()
                    .id(("pf-unit", i))
                    .px(px(12.0)).py(px(5.0))
                    .rounded(px(4.0))
                    .bg(if active { colors::accent() } else { colors::surface_raised() })
                    .border_1()
                    .border_color(if active { colors::accent() } else { colors::surface_border() })
                    .text_color(colors::text_primary()).text_size(px(font_size::SM))
                    .cursor_pointer()
                    .hover(|s| if active { s } else { s.bg(colors::tool_hover()) })
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.dispatch(cx, Action::SetPrefUnit(unit));
                        cx.notify();
                    }))
                    .child(*label)
                    .into_any_element()
            })
            .collect();

        let close_btn = div()
            .id("pf-done")
            .px(px(16.0)).py(px(6.0))
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
                    .font_weight(gpui::FontWeight::BOLD)
                    .mb(px(12.0))
                    .child("Preferences"),
            )
            .child(section_label("GENERAL"))
            .child(pref_row("Undo levels", undo_stepper.into_any_element()))
            // Snapping
            .child(section_label("SNAPPING"))
            .child(pref_row("Snap to point", snap_point_toggle.into_any_element()))
            .child(pref_row("Snap to grid", snap_grid_toggle.into_any_element()))
            // Grid
            .child(section_label("GRID"))
            .child(pref_row("Show grid", show_grid_toggle.into_any_element()))
            .child(pref_row("Grid spacing", spacing_stepper.into_any_element()))
            // Units
            .child(section_label("UNITS"))
            .child(div().flex().flex_row().flex_wrap().gap(px(6.0)).children(unit_chips))
            .child(div().flex_1())
            .child(
                div().flex().flex_row().justify_end().mt(px(16.0)).child(close_btn),
            )
    }
}

impl PreferencesView {
    /// A `−  value  +` stepper. `on_dec` / `on_inc` dispatch the clamped step.
    fn stepper(
        &self,
        id: &'static str,
        value_label: String,
        cx: &mut Context<Self>,
        on_dec: impl Fn(&mut PreferencesView, &mut Context<PreferencesView>) + 'static,
        on_inc: impl Fn(&mut PreferencesView, &mut Context<PreferencesView>) + 'static,
    ) -> impl IntoElement {
        div()
            .flex().flex_row().items_center().gap(px(4.0))
            .child(
                div()
                    .id((id, 0usize))
                    .w(px(22.0)).h(px(20.0))
                    .flex().items_center().justify_center()
                    .rounded(px(3.0)).bg(colors::surface_raised())
                    .text_color(colors::text_primary()).text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::tool_hover()))
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        on_dec(this, cx);
                        cx.notify();
                    }))
                    .child("\u{2212}"),
            )
            .child(
                div()
                    .px(px(8.0)).py(px(2.0))
                    .min_w(px(72.0))
                    .flex().justify_center()
                    .rounded(px(3.0)).bg(colors::surface_overlay())
                    .text_color(colors::text_primary()).text_size(px(font_size::SM))
                    .child(value_label),
            )
            .child(
                div()
                    .id((id, 1usize))
                    .w(px(22.0)).h(px(20.0))
                    .flex().items_center().justify_center()
                    .rounded(px(3.0)).bg(colors::surface_raised())
                    .text_color(colors::text_primary()).text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::tool_hover()))
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        on_inc(this, cx);
                        cx.notify();
                    }))
                    .child("+"),
            )
    }

    /// A boolean toggle pill (Enabled / Disabled).
    fn toggle(
        &self,
        id: &'static str,
        on: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut PreferencesView, &mut Context<PreferencesView>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .px(px(12.0)).py(px(4.0))
            .rounded(px(4.0))
            .bg(if on { colors::accent() } else { colors::surface_overlay() })
            .text_color(colors::text_primary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(if on { colors::accent_hover() } else { colors::tool_hover() }))
            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                on_click(this, cx);
                cx.notify();
            }))
            .child(if on { "On" } else { "Off" })
    }
}

fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .mt(px(12.0))
        .mb(px(4.0))
        .text_size(px(font_size::SM))
        .text_color(colors::text_secondary())
        .child(text)
}

fn pref_row(label: &'static str, control: gpui::AnyElement) -> impl IntoElement {
    div()
        .flex().flex_row().items_center().justify_between()
        .py(px(6.0))
        .child(
            div().text_color(colors::text_secondary()).text_size(px(font_size::SM))
                .child(label),
        )
        .child(control)
}
