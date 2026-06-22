//! Property Inspector panel — shows and edits the active layer's transform values.
//!
//! Each numeric row is clickable: clicking enters an edit mode where keystrokes
//! accumulate in `Drift::field_buffer`.  Press Enter to apply, Escape to cancel.

use crate::app_state::App;
use crate::{Drift, InspectorField};
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use prism_ui::{colors, font_size};

/// Color swatch colors keyed by layer index (matches the main canvas palette).
fn layer_color_hex(idx: usize) -> (u32, &'static str) {
    match idx % 6 {
        0 => (0x6366f1, "#6366f1"),
        1 => (0x22d3ee, "#22d3ee"),
        2 => (0xf59e0b, "#f59e0b"),
        3 => (0x10b981, "#10b981"),
        4 => (0xf43f5e, "#f43f5e"),
        _ => (0xa78bfa, "#a78bfa"),
    }
}

pub fn render_inspector(
    app: &App,
    editing_field: Option<&InspectorField>,
    field_buffer: &str,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    // Resolve the active layer index (for color lookup).
    let active_layer_idx = app
        .active_layer
        .and_then(|lid| app.layers.iter().position(|l| l.id == lid));

    let (swatch_color, hex_str) = match active_layer_idx {
        Some(idx) => layer_color_hex(idx),
        None => (0x6366f1, "#6366f1"),
    };

    div()
        .id("inspector-panel")
        .w_full()
        .border_t_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        // Header
        .child(
            div()
                .w_full()
                .h(px(28.0))
                .px_3()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("INSPECTOR"),
                ),
        )
        // Body (layer name, transforms)
        .child(inspector_body(app, editing_field, field_buffer, cx))
        // Fill Color row
        .child(
            div()
                .px_3()
                .py(px(4.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("Fill Color:"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(20.0))
                                .h(px(20.0))
                                .rounded(px(2.0))
                                .border_1()
                                .border_color(colors::surface_border())
                                .bg(gpui::rgb(swatch_color)),
                        )
                        .child(
                            div()
                                .text_size(px(font_size::XS))
                                .text_color(colors::text_primary())
                                .child(hex_str),
                        ),
                ),
        )
        // Edit hint
        .child(
            div()
                .px_3()
                .py(px(2.0))
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child("Click a value to edit · Enter to apply"),
                ),
        )
}

fn inspector_body(
    app: &App,
    editing_field: Option<&InspectorField>,
    field_buffer: &str,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    if let Some(layer_id) = app.active_layer {
        let layer_name = app
            .layers
            .iter()
            .find(|l| l.id == layer_id)
            .map(|l| l.name.clone())
            .unwrap_or_else(|| format!("Layer {layer_id}"));

        let t = app.transforms.get(&layer_id);
        let (x, y, sx, sy, rot, opacity) = t
            .map(|t| (t.x, t.y, t.scale_x, t.scale_y, t.rotation, t.opacity))
            .unwrap_or((0.0, 0.0, 1.0, 1.0, 0.0, 1.0));

        div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .gap_1()
            // Layer name (read-only)
            .child(prop_row("Layer", layer_name))
            // Position X — editable
            .child(editable_row(
                "Pos X",
                format!("{x:.1}"),
                InspectorField::PositionX,
                editing_field == Some(&InspectorField::PositionX),
                field_buffer,
                cx,
            ))
            // Position Y — editable
            .child(editable_row(
                "Pos Y",
                format!("{y:.1}"),
                InspectorField::PositionY,
                editing_field == Some(&InspectorField::PositionY),
                field_buffer,
                cx,
            ))
            // Scale X — editable
            .child(editable_row(
                "Scale X",
                format!("{sx:.2}"),
                InspectorField::ScaleX,
                editing_field == Some(&InspectorField::ScaleX),
                field_buffer,
                cx,
            ))
            // Scale Y — editable
            .child(editable_row(
                "Scale Y",
                format!("{sy:.2}"),
                InspectorField::ScaleY,
                editing_field == Some(&InspectorField::ScaleY),
                field_buffer,
                cx,
            ))
            // Rotation — editable
            .child(editable_row(
                "Rotation",
                format!("{rot:.1}°"),
                InspectorField::Rotation,
                editing_field == Some(&InspectorField::Rotation),
                field_buffer,
                cx,
            ))
            // Opacity — editable (displayed as 0–100 %)
            .child(editable_row(
                "Opacity",
                format!("{:.0}%", opacity * 100.0),
                InspectorField::Opacity,
                editing_field == Some(&InspectorField::Opacity),
                field_buffer,
                cx,
            ))
    } else {
        div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .child(
                div()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_disabled())
                    .child("Select a layer to inspect"),
            )
    }
}

/// A read-only label + value row (used for non-numeric fields like layer name).
fn prop_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .w_full()
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(label),
        )
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_primary())
                .child(value),
        )
}

/// An editable label + value row.  Click to enter edit mode; text is highlighted
/// in amber and shows a blinking-style `|` cursor.  Press Enter/Esc in `on_key`
/// to commit or cancel (handled in `Drift::on_key`).
fn editable_row(
    label: &'static str,
    display_value: String,
    field: InspectorField,
    is_editing: bool,
    buffer: &str,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    let shown = if is_editing {
        format!("{buffer}|")
    } else {
        display_value
    };

    div()
        .w_full()
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(label),
        )
        .child(
            div()
                .id(SharedString::from(format!("insp-{label}")))
                .text_size(px(font_size::XS))
                .text_color(if is_editing {
                    gpui::rgb(0xfbbf24) // amber
                } else {
                    colors::text_primary()
                })
                .when(is_editing, |el| el.bg(gpui::rgba(0xfbbf2422)))
                .px(px(4.0))
                .rounded(px(2.0))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    // Pre-fill the buffer with the current value.
                    let pre = this.app.active_layer
                        .and_then(|lid| this.app.transforms.get(&lid))
                        .map(|t| match field {
                            InspectorField::PositionX => format!("{:.1}", t.x),
                            InspectorField::PositionY => format!("{:.1}", t.y),
                            InspectorField::ScaleX    => format!("{:.2}", t.scale_x),
                            InspectorField::ScaleY    => format!("{:.2}", t.scale_y),
                            InspectorField::Rotation  => format!("{:.1}", t.rotation),
                            InspectorField::Opacity   => format!("{:.0}", t.opacity * 100.0),
                        })
                        .unwrap_or_default();
                    this.editing_field = Some(field);
                    this.field_buffer = pre;
                    cx.notify();
                }))
                .child(shown),
        )
}
