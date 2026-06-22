//! Property Inspector panel — shows the active layer's transform values.
//!
//! Displays position (x/y), scale (x/y), rotation, and opacity for the
//! currently selected layer.  If no layer is selected, shows a prompt.
//! Inspector panel — shows fill color swatch and layer properties for the
//! active layer.  Rendered below the AI panel or as a separate section.

use crate::app_state::App;
use crate::Drift;
use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement, Styled};
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

pub fn render_inspector(app: &App, _cx: &mut Context<Drift>) -> impl IntoElement {
    // Resolve the active layer index (for color lookup).
    let active_layer_idx = app
        .active_layer
        .and_then(|lid| app.layers.iter().position(|l| l.id == lid));

    let (swatch_color, hex_str) = match active_layer_idx {
        Some(idx) => layer_color_hex(idx),
        None => (0x6366f1, "#6366f1"),
    };

    let active_layer_name = app
        .active_layer
        .and_then(|lid| app.layers.iter().find(|l| l.id == lid))
        .map(|l| l.name.clone())
        .unwrap_or_else(|| "—".to_string());

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
        // Body
        .child(inspector_body(app))
        // Active layer name
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
                        .child("Layer"),
                )
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .child(active_layer_name),
                ),
        )
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
                        // Color swatch
                        .child(
                            div()
                                .w(px(20.0))
                                .h(px(20.0))
                                .rounded(px(2.0))
                                .border_1()
                                .border_color(colors::surface_border())
                                .bg(gpui::rgb(swatch_color)),
                        )
                        // Hex display
                        .child(
                            div()
                                .text_size(px(font_size::XS))
                                .text_color(colors::text_primary())
                                .child(hex_str),
                        ),
                ),
        )
        // Copy hex hint
        .child(
            div()
                .px_3()
                .py(px(2.0))
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child("Copy Hex"),
                ),
        )
}

fn inspector_body(app: &App) -> impl IntoElement {
    if let Some(layer_id) = app.active_layer {
        // Try to find the layer name for the header.
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
            // Layer name line
            .child(prop_row(
                "Layer",
                layer_name,
            ))
            // Position
            .child(prop_row(
                "Position",
                format!("X: {x:.1}  Y: {y:.1}"),
            ))
            // Scale
            .child(prop_row(
                "Scale",
                format!("X: {sx:.2}  Y: {sy:.2}"),
            ))
            // Rotation
            .child(prop_row(
                "Rotation",
                format!("{rot:.1}°"),
            ))
            // Opacity — displayed as 0–100 %
            .child(prop_row(
                "Opacity",
                format!("{:.0}%", opacity * 100.0),
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

/// A single labeled value row.
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
