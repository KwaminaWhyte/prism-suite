//! Inspector panel — edit the selected shape's appearance.
//!
//! Fully wired: every clickable routes through `root.app.apply(...) + cx.notify()`
//! (the single choke point — see `panels/mod.rs`). For the selected shape it shows
//! and EDITS:
//!   - fill colour   → `Action::SetFillColor`
//!   - stroke colour → `Action::SetStrokeColor`
//!   - stroke width  → `Action::SetStrokeWidth`
//!   - opacity       → `Action::SetOpacity` (the fill-alpha channel; Contour's
//!     legacy shape model has no separate opacity field)
//!
//! gpui 0.2.2 has no native slider, so numeric/colour editing uses compact `−/＋`
//! stepper rows plus clickable preset swatches — the same idiom the Pigment host's
//! Color panel uses. Pen/path editing, gradients, text, boolean ops and transforms
//! are out of scope for this pass.

use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, rgb, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, ScrollWheelEvent,
};

use crate::gradient::{Gradient, GradientKind};
use crate::liveshape::{LiveShape, MAX_SIDES, MIN_SIDES};
use crate::text::TextAlign;

use crate::app_state::{Action, App, WidthProfilePreset};
use crate::panels::{ui_divider, ui_section_header};
use prism_ui::colors;
use crate::Contour;

const SWATCH_BORDER: u32 = 0x555555;

/// 8/255 ≈ 0.0314 per colour-channel stepper click.
const COLOR_STEP: f32 = 8.0 / 255.0;
/// Stroke-width step (document units) per click.
const WIDTH_STEP: f32 = 0.5;
/// Opacity step per click.
const OPACITY_STEP: f32 = 0.05;

/// Which colour an edit targets — fill or stroke.
#[derive(Clone, Copy)]
enum Target {
    Fill,
    Stroke,
}

mod color;
mod dimensions;
mod live;
mod text;
mod gradient;
mod appearance;

use color::color_section;
use dimensions::{dimensions_section, opacity_row, rotation_row, width_row};
use live::live_shape_section;
use text::{text_on_path_section, text_section};
use gradient::{add_gradient_button, gradient_editor_section, gradient_type_section};
use appearance::{appearance_section, graphic_styles_section, width_preset_section};

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

pub fn render(
    app: &App,
    numeric: Option<&crate::numeric_edit::NumericEdit>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let header = div()
        .flex()
        .flex_col()
        .child(ui_section_header("Inspector"))
        .child(ui_divider());

    // No selection → an empty-state hint. Selection is set by clicking the canvas
    // or a Layers row.
    let Some(idx) = app.selected else {
        return div()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .child(header)
            .child(
                div()
                    .p_3()
                    .text_color(colors::text_secondary())
                    .text_size(px(11.0))
                    .child("No selection — click a shape on the canvas."),
            );
    };

    let Some(shape) = app.doc.shapes.get(idx) else {
        // Stale selection (shouldn't happen — apply keeps it in range), degrade
        // to the empty state rather than panic.
        return div()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .child(header)
            .child(div().p_3().text_color(colors::text_secondary()).child("No selection."));
    };

    let fill = shape.fill_color();
    let stroke = shape.stroke_color();
    let stroke_w = shape.stroke_width();
    // Opacity mirrors the fill-alpha channel (see module docs); shapes without a
    // fill region (Line) expose no opacity control.
    let opacity = fill.map(|c| c[3]);

    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .bg(colors::surface_bg())
        .text_color(colors::text_primary())
        .text_size(px(11.0))
        // Shape type label.
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(format!("{} (#{idx})", shape.label())),
        );

    // ---- Dimensions (typeable X / Y / W / H + rotation) ----------------
    if let Some(bbox) = app.selection_bbox() {
        body = body.child(dimensions_section(bbox, numeric, cx));
    }

    // ---- Live Shape section (primary polygon / star only) --------------
    if let Some(live) = app.primary_live_shape() {
        body = body.child(live_shape_section(live, cx));
    }

    // ---- Type section (primary text object only) -----------------------
    if let Some(params) = app.primary_text_params() {
        body = body.child(text_section(
            params.font_size,
            params.align,
            params.font_family.clone(),
            app.font_dropdown_open,
            cx,
        ));
    }

    // ---- Fill section (shapes with a fill region only) -----------------
    if let Some(c) = fill {
        body = body.child(color_section("Fill", Target::Fill, c, cx));
    }

    // ---- Gradient: seed button (solid fill, no gradient) or full editor ----
    if shape.fill_gradient().is_none() && fill.is_some() {
        body = body.child(add_gradient_button(idx, cx));
    } else if let Some(gradient) = shape.fill_gradient() {
        body = body.child(gradient_type_section(idx, gradient.kind, cx));
        body = body.child(gradient_editor_section(idx, gradient, app.selected_gradient_stop, cx));
    }

    // ---- Text on Path (Wave 9): shown when selection is text + path ----
    if app.selection.len() == 2 {
        let text_idx = app.selection.iter().copied().find(|&i| {
            app.doc.shapes.get(i).map(|s| s.text_params().is_some()).unwrap_or(false)
        });
        let path_idx = app.selection.iter().copied().find(|&i| {
            app.doc
                .shapes
                .get(i)
                .map(|s| matches!(s, crate::document::Shape::Path { .. }))
                .unwrap_or(false)
        });
        if let (Some(tid), Some(pid)) = (text_idx, path_idx) {
            let attached = app.text_on_path.contains_key(&tid);
            let top_params = app.text_on_path_params.get(&tid).copied().unwrap_or_default();
            body = body.child(text_on_path_section(tid, pid, attached, top_params, cx));
        }
    }

    // ---- Stroke section (every variant has a stroke) -------------------
    if let Some(c) = stroke {
        body = body.child(color_section("Stroke", Target::Stroke, c, cx));
    }

    // ---- Stroke width --------------------------------------------------
    body = body.child(width_row(stroke_w, numeric, cx));

    // ---- Opacity -------------------------------------------------------
    if let Some(o) = opacity {
        body = body.child(opacity_row(o, numeric, cx));
    }

    // ---- Rotation (relative; typeable) ---------------------------------
    body = body.child(rotation_row(numeric, cx));

    // ---- Appearance panel (fills / strokes / effects stack) ------------
    body = body.child(appearance_section(shape, cx));

    // ---- Graphic styles ------------------------------------------------
    body = body.child(graphic_styles_section(&app.doc.graphic_styles, cx));

    // ---- Width profile presets -----------------------------------------
    body = body.child(width_preset_section(cx));

    div().flex().flex_col().child(header).child(body)
}

/// A `−  LABEL  value  ＋` row shell shared by the live-shape and text steppers.
fn param_row(
    label: &'static str,
    value: String,
    dec: impl IntoElement,
    inc: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(40.0)).text_color(colors::text_secondary()).child(label))
        .child(dec)
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(value),
        )
        .child(inc)
}

/// A 20px square stepper button shell with the panel's hover idiom. `on_click`
/// routes through the choke point.
fn step_button(
    id: (&'static str, u64),
    glyph: &'static str,
    cx: &mut Context<Contour>,
    on_click: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
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
        .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)))
        .child(glyph)
}

/// Stable id-prefix for a section's preset swatches (avoids Fill/Stroke clashes).
fn preset_id(section: &str) -> &'static str {
    match section {
        "Fill" => "insp-fill-preset",
        _ => "insp-stroke-preset",
    }
}

// ============================================================================
// Text on Path (Wave 9)
// ============================================================================

/// Pack straight sRGB RGBA `[f32;4]` → `0xAARRGGBB` for `gpui::rgba`.
fn pack_rgba(c: [f32; 4]) -> u32 {
    let to_u8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    let (r, g, b, a) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(c[3]));
    (r << 24) | (g << 16) | (b << 8) | a
}

// ── Appearance panel ──────────────────────────────────────────────────────────
