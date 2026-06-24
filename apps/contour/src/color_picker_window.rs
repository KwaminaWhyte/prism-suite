//! Color Picker — a floating OS-level child window.
//!
//! Holds a [`WeakEntity<Contour>`] (the welcome / Reel pattern). It edits
//! `app.color_picker` (the live RGB/HSB/CMYK/hex model) via the Batch-13 actions
//! and applies the result to the current selection:
//!
//! - RGB sliders   → [`Action::SetPickerRgb`]
//! - HSB sliders   → [`Action::SetPickerHsb`]
//! - CMYK sliders  → [`Action::SetPickerCmyk`]
//! - hex swatches  → [`Action::SetPickerHex`]
//! - "Apply"       → [`Action::ApplyPickerToSelection`]
//!
//! Each channel is a `−  value  +` stepper (no text-input subsystem); editing one
//! model re-derives the others in `app_state`, so the panel always reflects the
//! canonical RGBA after any change.

use gpui::{
    rgb, ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::prefs_color::ColorPicker;
use crate::app_state::Action;
use crate::Contour;

pub struct ColorPickerView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
}

impl Focusable for ColorPickerView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ColorPickerView {
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

    fn snapshot(&self, cx: &mut Context<Self>) -> ColorPicker {
        self.app_entity
            .upgrade()
            .map(|e| e.read(cx).app.color_picker)
            .unwrap_or_default()
    }
}

/// Preset hex swatches for quick selection.
const SWATCHES: [&str; 8] = [
    "#000000", "#FFFFFF", "#F05555", "#F5A623", "#3DC97E", "#2E7D5E", "#7C5AF5", "#3B82F6",
];

impl Render for ColorPickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picker = self.snapshot(cx);
        let [r, g, b, _a] = picker.rgba;
        let (h, s, br) = picker.hsb();
        let (c, m, y, k) = picker.cmyk();
        let hex = picker.hex();

        // Preview swatch of the current colour.
        let preview_hex = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
        let preview = div()
            .w_full().h(px(40.0))
            .rounded(px(4.0))
            .bg(rgb(preview_hex))
            .border_1().border_color(colors::surface_border());

        // RGB steppers (0..1, step 1/255).
        let rgb_block = div()
            .flex().flex_col().gap(px(2.0))
            .child(section_label("RGB"))
            .child(self.channel_row("cp-r", "R", r, 1.0 / 255.0, cx, {
                move |v| Action::SetPickerRgb { r: v.clamp(0.0, 1.0), g, b }
            }))
            .child(self.channel_row("cp-g", "G", g, 1.0 / 255.0, cx, {
                move |v| Action::SetPickerRgb { r, g: v.clamp(0.0, 1.0), b }
            }))
            .child(self.channel_row("cp-b", "B", b, 1.0 / 255.0, cx, {
                move |v| Action::SetPickerRgb { r, g, b: v.clamp(0.0, 1.0) }
            }));

        // HSB steppers (H 0..360 step 1°, S/B 0..1 step 0.01).
        let hsb_block = div()
            .flex().flex_col().gap(px(2.0))
            .child(section_label("HSB"))
            .child(self.channel_row("cp-h", "H", h / 360.0, 1.0 / 360.0, cx, {
                move |v| Action::SetPickerHsb { h: (v * 360.0).rem_euclid(360.0), s, b: br }
            }))
            .child(self.channel_row("cp-s", "S", s, 0.01, cx, {
                move |v| Action::SetPickerHsb { h, s: v.clamp(0.0, 1.0), b: br }
            }))
            .child(self.channel_row("cp-v", "B", br, 0.01, cx, {
                move |v| Action::SetPickerHsb { h, s, b: v.clamp(0.0, 1.0) }
            }));

        // CMYK steppers (0..1 step 0.01).
        let cmyk_block = div()
            .flex().flex_col().gap(px(2.0))
            .child(section_label("CMYK"))
            .child(self.channel_row("cp-c", "C", c, 0.01, cx, {
                move |v| Action::SetPickerCmyk { c: v.clamp(0.0, 1.0), m, y, k }
            }))
            .child(self.channel_row("cp-m", "M", m, 0.01, cx, {
                move |v| Action::SetPickerCmyk { c, m: v.clamp(0.0, 1.0), y, k }
            }))
            .child(self.channel_row("cp-y", "Y", y, 0.01, cx, {
                move |v| Action::SetPickerCmyk { c, m, y: v.clamp(0.0, 1.0), k }
            }))
            .child(self.channel_row("cp-k", "K", k, 0.01, cx, {
                move |v| Action::SetPickerCmyk { c, m, y, k: v.clamp(0.0, 1.0) }
            }));

        // Hex swatch grid + current hex readout.
        let swatch_grid: Vec<gpui::AnyElement> = SWATCHES
            .iter()
            .enumerate()
            .map(|(i, sw)| {
                let sw = *sw;
                let val = u32::from_str_radix(sw.trim_start_matches('#'), 16).unwrap_or(0);
                div()
                    .id(("cp-sw", i))
                    .w(px(24.0)).h(px(24.0))
                    .rounded(px(3.0))
                    .bg(rgb(val))
                    .border_1().border_color(colors::surface_border())
                    .cursor_pointer()
                    .hover(|s| s.border_color(colors::accent()))
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.dispatch(cx, Action::SetPickerHex(sw.to_string()));
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();

        let hex_block = div()
            .flex().flex_col().gap(px(4.0))
            .child(section_label("HEX"))
            .child(
                div().text_color(colors::text_primary()).text_size(px(font_size::SM))
                    .child(hex.clone()),
            )
            .child(div().flex().flex_row().flex_wrap().gap(px(4.0)).children(swatch_grid));

        let apply_btn = div()
            .id("cp-apply")
            .w_full()
            .py(px(7.0))
            .flex().items_center().justify_center()
            .rounded(px(4.0)).bg(colors::accent())
            .text_color(colors::text_primary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::accent_hover()))
            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                this.dispatch(cx, Action::ApplyPickerToSelection);
                cx.notify();
            }))
            .child("Apply to Selection");

        div()
            .id("color-picker-root")
            .size_full()
            .flex().flex_col().gap(px(8.0))
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .p(px(14.0))
            .overflow_y_scroll()
            .child(preview)
            .child(rgb_block)
            .child(hsb_block)
            .child(cmyk_block)
            .child(hex_block)
            .child(apply_btn)
    }
}

impl ColorPickerView {
    /// A `label  −  value  +` channel stepper. `make(value)` produces the action
    /// for the new (already-stepped) channel value in `0..1`.
    fn channel_row(
        &self,
        id: &'static str,
        label: &'static str,
        value: f32,
        step: f32,
        cx: &mut Context<Self>,
        make: impl Fn(f32) -> Action + Clone + 'static,
    ) -> impl IntoElement {
        let dec = make.clone();
        let inc = make;
        let dec_v = value - step;
        let inc_v = value + step;
        div()
            .flex().flex_row().items_center().justify_between()
            .py(px(2.0))
            .child(
                div().w(px(18.0)).text_color(colors::text_secondary())
                    .text_size(px(font_size::SM)).child(label),
            )
            .child(
                div()
                    .flex().flex_row().items_center().gap(px(4.0))
                    .child(
                        div()
                            .id((id, 0usize))
                            .w(px(20.0)).h(px(18.0))
                            .flex().items_center().justify_center()
                            .rounded(px(3.0)).bg(colors::surface_raised())
                            .text_color(colors::text_primary()).text_size(px(13.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                                this.dispatch(cx, dec(dec_v));
                                cx.notify();
                            }))
                            .child("\u{2212}"),
                    )
                    .child(
                        div()
                            .px(px(6.0)).py(px(1.0))
                            .min_w(px(52.0))
                            .flex().justify_center()
                            .rounded(px(3.0)).bg(colors::surface_overlay())
                            .text_color(colors::text_primary()).text_size(px(font_size::XS))
                            .child(format!("{value:.3}")),
                    )
                    .child(
                        div()
                            .id((id, 1usize))
                            .w(px(20.0)).h(px(18.0))
                            .flex().items_center().justify_center()
                            .rounded(px(3.0)).bg(colors::surface_raised())
                            .text_color(colors::text_primary()).text_size(px(13.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                                this.dispatch(cx, inc(inc_v));
                                cx.notify();
                            }))
                            .child("+"),
                    ),
            )
    }
}

fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .mt(px(4.0))
        .text_size(px(font_size::XS))
        .text_color(colors::text_secondary())
        .font_weight(gpui::FontWeight::BOLD)
        .child(text)
}
