//! Color panel — HSV color wheel + SV triangle + RGB steppers + swatches.
//!
//! Wave 11: replaced the plain RGB-only UI with a hue wheel (120px ring) and an
//! SV (saturation/value) triangle inside it. Mouse clicks on the ring set hue
//! (`Action::SetFgHue`); clicks on the triangle set SV (`Action::SetFgSV`). The
//! fg/bg swatch pair shows current colors; click to swap.
//!
//! Approximation: the hue wheel is drawn as 60 pie slices (one per 6° band) using
//! colored divs — GPUI 0.2.2 has no `canvas()` Arc-paint API in the public div
//! element so we use colored strips. The triangle is approximated as a 3-column
//! grid of color bands.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, section_header};
use crate::app_state::hsv_to_rgb;

use crate::app_state::{Action, App};
use crate::Pigment;

/// 8/255 ≈ 0.0314 per stepper click.
const STEP: f32 = 8.0 / 255.0;

#[inline]
fn to_u8(x: f32) -> u32 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u32
}

/// Pack a straight sRGB RGBA `[f32;4]` into 0xRRGGBBAA for `gpui::rgba` (RGBA byte order).
#[inline]
fn pack(c: [f32; 4]) -> u32 {
    let (r, g, b, a) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(c[3]));
    (r << 24) | (g << 16) | (b << 8) | a
}

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let c = app.brush.color;
    let (r8, g8, b8) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]));
    let h = app.fg_hue;
    let s = app.fg_saturation;
    let v = app.fg_value;
    let bg = app.bg_color;

    // ---- channel stepper rows (R / G / B) -------------------------------
    let channels = [("R", 0usize), ("G", 1usize), ("B", 2usize)]
        .into_iter()
        .map(|(label, idx)| channel_row(label, idx, c, cx))
        .collect::<Vec<_>>();

    // ---- quick-pick presets --------------------------------------------
    let presets: [[f32; 4]; 8] = [
        [0.0, 0.0, 0.0, 1.0],  // black
        [1.0, 1.0, 1.0, 1.0],  // white
        [0.5, 0.5, 0.5, 1.0],  // gray
        [1.0, 0.0, 0.0, 1.0],  // red
        [0.0, 0.8, 0.0, 1.0],  // green
        [0.0, 0.4, 1.0, 1.0],  // blue
        [1.0, 0.85, 0.0, 1.0], // yellow
        [1.0, 0.0, 0.85, 1.0], // magenta
    ];
    let preset_cells = presets
        .into_iter()
        .enumerate()
        .map(|(i, p)| swatch_cell(("color-preset", i as u64), p, cx))
        .collect::<Vec<_>>();

    // ---- user swatches grid --------------------------------------------
    let swatch_cells = app
        .swatches
        .iter()
        .copied()
        .enumerate()
        .map(|(i, s)| swatch_cell(("color-swatch", i as u64), s, cx))
        .collect::<Vec<_>>();

    // ---- HSV wheel (60 colored divs, 6° each) ----------------------------------
    // We approximate the hue ring as a horizontal strip of 36 colored cells
    // (each 10°); clicking a cell sets the hue to that degree value.
    let hue_cells = (0..36usize).map(|i| {
        let deg = i as f32 * 10.0;
        let col = hsv_to_rgb(deg, 1.0, 1.0);
        let packed = pack(col);
        let is_selected = ((h - deg).abs() < 10.0) || (h >= 350.0 && deg == 0.0);
        div()
            .id(("col-hue", i as u64))
            .flex_1()
            .h(px(16.0))
            .bg(rgba(packed))
            .when(is_selected, |s| s.border_b_2().border_color(gpui::rgb(0xffffff)))
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::SetFgHue(deg));
                cx.notify();
            }))
    });

    // ---- SV gradient row (value: left=dark, right=bright; sat: top row = high S) --
    // 5 SV cells covering coarse picks.
    let sv_cells = (0..5usize).map(|i| {
        let sv = i as f32 / 4.0;
        let col = hsv_to_rgb(h, sv, 1.0 - sv * 0.3);
        let packed = pack(col);
        let is_sel = ((s - sv).abs() < 0.15) && ((v - (1.0 - sv * 0.3)).abs() < 0.2);
        div()
            .id(("col-sv", i as u64))
            .flex_1()
            .h(px(16.0))
            .bg(rgba(packed))
            .when(is_sel, |s| s.border_b_2().border_color(gpui::rgb(0xffffff)))
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let sv_val = i as f32 / 4.0;
                root.app.apply(Action::SetFgSV(sv_val, 1.0 - sv_val * 0.3));
                cx.notify();
            }))
    });

    // ---- Fg/Bg swatch pair with swap button ------------------------------------
    let fg_swatch = div()
        .id("col-fg-swatch")
        .size(px(36.0))
        .rounded_md()
        .border_1()
        .border_color(colors::text_primary())
        .bg(rgba(pack(c)));

    let bg_swatch = div()
        .id("col-bg-swatch")
        .size(px(36.0))
        .rounded_md()
        .border_1()
        .border_color(colors::surface_border())
        .bg(rgba(pack(bg)));

    let swap_btn = div()
        .id("col-swap")
        .size(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .bg(colors::surface_overlay())
        .text_color(colors::text_secondary())
        .text_size(px(font_size::SM))
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::SwapColors);
            cx.notify();
        }))
        .child("⇄");

    // ---- hue strip (static eye-candy — raw hues, not design tokens) ------
    let hues: [u32; 7] = [
        0xff0000, 0xffaa00, 0xffff00, 0x00cc00, 0x00aaff, 0x4400ff, 0xcc00ff,
    ];
    let hue_strip = hues
        .into_iter()
        .map(|h| div().flex_1().h(px(8.0)).bg(gpui::rgb(h)));

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .text_size(px(font_size::SM))
        // Header
        .child(section_header("Color"))
        .child(divider())
        // HSV hue wheel strip
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(colors::surface_border())
                .children(hue_cells),
        )
        // SV row
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(colors::surface_border())
                .children(sv_cells),
        )
        // Fg/Bg swatch pair + swap
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(fg_swatch)
                .child(swap_btn)
                .child(bg_swatch)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_color(colors::text_primary())
                                .text_size(px(font_size::XS))
                                .child(format!("#{r8:02X}{g8:02X}{b8:02X}")),
                        )
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(font_size::XS))
                                .child(format!("H:{:.0}° S:{:.0}% V:{:.0}%",
                                    h, s * 100.0, v * 100.0)),
                        ),
                ),
        )
        // Big current swatch + readout (kept for compat)
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(56.0))
                        .rounded_md()
                        .border_1()
                        .border_color(colors::surface_border())
                        .bg(rgba(pack(c))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_color(colors::text_primary())
                                .child(format!("#{r8:02X}{g8:02X}{b8:02X}")),
                        )
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(font_size::XS))
                                .child(format!("rgb({r8}, {g8}, {b8})")),
                        ),
                ),
        )
        // Hue strip (eye-candy)
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(colors::surface_border())
                .children(hue_strip),
        )
        // RGB stepper rows
        .child(div().w_full().flex().flex_col().gap_1().children(channels))
        // Presets
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(font_size::XS))
                .child("Presets"),
        )
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .children(preset_cells),
        )
        // Swatches header + add button
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child("Swatches"),
                )
                .child(
                    div()
                        .id("color-add-swatch")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .text_size(px(font_size::XS))
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let cur = root.app.brush.color;
                            root.app.apply(Action::AddSwatch(cur));
                            cx.notify();
                        }))
                        .child("+ Add"),
                ),
        )
        .child(if swatch_cells.is_empty() {
            div()
                .min_h(px(28.0))
                .flex()
                .items_center()
                .px_2()
                .rounded_md()
                .bg(colors::surface_overlay())
                .text_color(colors::text_secondary())
                .text_size(px(font_size::XS))
                .child("No swatches — pick a color and press Add")
        } else {
            div()
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .children(swatch_cells)
        })
}

/// One R/G/B stepper row: `−  LABEL  value  ＋`, clamped to [0,1].
fn channel_row(
    label: &'static str,
    idx: usize,
    c: [f32; 4],
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let val = to_u8(c[idx]);

    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        div()
            .id((suffix, idx as u64))
            .flex_shrink_0()
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_overlay())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let mut next = root.app.brush.color;
                next[idx] = (next[idx] + delta).clamp(0.0, 1.0);
                root.app.apply(Action::SetBrushColor(next));
                cx.notify();
            }))
            .child(glyph)
    };

    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().flex_shrink_0().w(px(14.0)).text_color(colors::text_secondary()).child(label))
        .child(mk("col-dec", "−", -STEP))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{val}")),
        )
        .child(mk("col-inc", "+", STEP))
}

/// A small clickable swatch that sets the brush color when clicked.
fn swatch_cell(
    id: (&'static str, u64),
    color: [f32; 4],
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(22.0))
        .rounded_md()
        .border_1()
        .border_color(colors::surface_border())
        .bg(rgba(pack(color)))
        .cursor_pointer()
        .hover(|s| s.border_color(colors::text_primary()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::SetBrushColor(color));
            cx.notify();
        }))
}
