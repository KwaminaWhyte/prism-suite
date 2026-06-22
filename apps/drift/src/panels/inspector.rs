//! Property Inspector panel — shows the active layer's transform values.
//!
//! Displays position (x/y), scale (x/y), rotation, and opacity for the
//! currently selected layer.  If no layer is selected, shows a prompt.

use crate::app_state::App;
use crate::Drift;
use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement, Styled};
use prism_ui::{colors, font_size};

pub fn render_inspector(app: &App, _cx: &mut Context<Drift>) -> impl IntoElement {
    div()
        .id("inspector-panel")
        .w_full()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(colors::surface_border())
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
