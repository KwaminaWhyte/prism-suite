//! Recolor Artwork panel — swap fill colours across the current selection.
//!
//! Shows when `App::recolor_panel_open`. Finds the unique fill colours in the
//! selected shapes, presents them as clickable swatches. Clicking a swatch
//! selects it; R/G/B steppers appear so the user can pick a replacement; "Apply"
//! emits `Action::RecolorSelected { old_color, new_color }` which replaces every
//! matching fill in the selection. "Close" / "✕" hides the panel.

use gpui::{
    div, px, rgb, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App};
use prism_ui::colors;
use crate::Contour;

// Local accent for the Apply button (blue, not prism purple).
const ACCENT: u32 = 0x3b82f6;

/// Pack straight sRGB RGBA `[f32;4]` → `0xAARRGGBB` for `gpui::rgba`.
fn pack(c: [f32; 4]) -> u32 {
    let to_u8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    let (r, g, b, a) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(c[3]));
    (a << 24) | (r << 16) | (g << 8) | b
}

fn to_u8(x: f32) -> u32 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u32
}

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    if !app.recolor_panel_open {
        return div().id("recolor-hidden");
    }

    // Collect unique fill colours from the selection (dedup by u32 key).
    let mut seen: Vec<u32> = Vec::new();
    let mut unique_colors: Vec<[f32; 4]> = Vec::new();
    for &i in &app.selection {
        if let Some(shape) = app.doc.shapes.get(i) {
            if let Some(fc) = shape.fill_color() {
                let key = pack(fc);
                if !seen.contains(&key) {
                    seen.push(key);
                    unique_colors.push(fc);
                }
            }
        }
    }

    // Close button.
    let close_btn = div()
        .id("rc-close")
        .px_2()
        .h(px(20.0))
        .flex()
        .items_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_secondary())
        .text_size(px(11.0))
        .cursor_pointer()
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::CloseRecolorPanel);
            cx.notify();
        }))
        .child("✕");

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(12.0)).child("Recolor Artwork"))
        .child(close_btn);

    // Swatches row.
    let swatches = unique_colors
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let selected = app.recolor_selected_color.map_or(false, |sel| {
                pack(sel) == pack(c)
            });
            let border_col = if selected { ACCENT } else { 0x555555u32 }; // local blue accent for recolor
            div()
                .id(("rc-swatch", i as u64))
                .size(px(24.0))
                .rounded_md()
                .border_2()
                .border_color(rgb(border_col))
                .bg(rgba(pack(c)))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SelectRecolorColor(c));
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();

    let swatches_row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_2()
        .p_3()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .w_full()
                .child("Fill colours in selection:"),
        )
        .children(swatches);

    // If no shapes are selected with fills, show a hint.
    let content = if unique_colors.is_empty() {
        div()
            .p_3()
            .text_color(colors::text_secondary())
            .text_size(px(11.0))
            .child("Select shapes with fill colours to recolor.")
    } else {
        div().child(swatches_row)
    };

    // Color editor for the selected swatch.
    let editor = app.recolor_selected_color.map(|sel| {
        let color_editor = color_steppers(sel, cx);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .pb_3()
            .child(
                div()
                    .text_color(colors::text_secondary())
                    .text_size(px(10.0))
                    .child("New colour:"),
            )
            .child(color_editor)
    });

    // Apply button — only shown when a swatch is selected.
    let apply_btn = app.recolor_selected_color.map(|old_color| {
        div()
            .id("rc-apply")
            .mx_3()
            .mb_3()
            .h(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(rgb(ACCENT))
            .text_color(rgb(0xffffff))
            .text_size(px(12.0))
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                if let Some(new_color) = root.app.recolor_selected_color {
                    root.app.apply(Action::RecolorSelected { old_color, new_color });
                    cx.notify();
                }
            }))
            .child("Apply")
    });

    div()
        .id("recolor-panel")
        .flex()
        .flex_col()
        .bg(colors::surface_bg())
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(content)
        .children(editor)
        .children(apply_btn)
}

/// R/G/B channel steppers for the colour being edited. Emits
/// `Action::SelectRecolorColor` (updates the working copy in place).
fn color_steppers(c: [f32; 4], cx: &mut Context<Contour>) -> impl IntoElement {
    const STEP: f32 = 8.0 / 255.0;
    let channels = [("R", 0usize), ("G", 1usize), ("B", 2usize)]
        .into_iter()
        .map(|(label, idx)| {
            let val = to_u8(c[idx]);
            let dec_id = ("rc-ch-dec", idx as u64);
            let inc_id = ("rc-ch-inc", idx as u64);
            let dec = div()
                .id(dec_id)
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .border_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    if let Some(mut cur) = root.app.recolor_selected_color {
                        cur[idx] = (cur[idx] - STEP).clamp(0.0, 1.0);
                        root.app.apply(Action::SelectRecolorColor(cur));
                        cx.notify();
                    }
                }))
                .child("−");
            let inc = div()
                .id(inc_id)
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .border_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    if let Some(mut cur) = root.app.recolor_selected_color {
                        cur[idx] = (cur[idx] + STEP).clamp(0.0, 1.0);
                        root.app.apply(Action::SelectRecolorColor(cur));
                        cx.notify();
                    }
                }))
                .child("＋");
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().w(px(14.0)).text_color(colors::text_secondary()).text_size(px(11.0)).child(label))
                .child(dec)
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_center()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(format!("{val}")),
                )
                .child(inc)
        })
        .collect::<Vec<_>>();

    div().flex().flex_col().gap_1().children(channels)
}
