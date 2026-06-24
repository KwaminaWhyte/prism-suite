//! Property Inspector panel — shows and edits the active layer's transform.
//!
//! Each numeric row is clickable: clicking swaps the static value for a real,
//! focusable [`prism_ui::TextField`] seeded with the current value. Type a new
//! value and press Enter to commit (the field's `on_submit` parses + dispatches
//! the matching `Action::Set*`); the field reverts to the static readout. The
//! `KeyframeValue` row writes an opacity keyframe at the current playhead.

use crate::app_state::App;
use crate::text_fields::{NumericField, TextFields};
use crate::Drift;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size, TextField};

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
    editing: Option<NumericField>,
    fields: &TextFields,
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
        // Body (layer name, transforms, keyframe value)
        .child(inspector_body(app, editing, fields, cx))
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
            div().px_3().py(px(2.0)).child(
                div()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_disabled())
                    .child("Click a value to type · Enter to apply"),
            ),
        )
}

fn inspector_body(
    app: &App,
    editing: Option<NumericField>,
    fields: &TextFields,
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
        let kf_value = NumericField::KeyframeValue.current_value(app);

        div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .gap_1()
            .child(prop_row("Layer", layer_name))
            .child(editable_row("Pos X", format!("{x:.1}"), NumericField::PositionX, editing, fields, cx))
            .child(editable_row("Pos Y", format!("{y:.1}"), NumericField::PositionY, editing, fields, cx))
            .child(editable_row("Scale X", format!("{sx:.2}"), NumericField::ScaleX, editing, fields, cx))
            .child(editable_row("Scale Y", format!("{sy:.2}"), NumericField::ScaleY, editing, fields, cx))
            .child(editable_row("Rotation", format!("{rot:.1}°"), NumericField::Rotation, editing, fields, cx))
            .child(editable_row("Opacity", format!("{:.0}%", opacity * 100.0), NumericField::Opacity, editing, fields, cx))
            // Keyframe value at the current playhead (opacity property).
            .child(editable_row("KF Value", kf_value, NumericField::KeyframeValue, editing, fields, cx))
    } else {
        div().flex().flex_col().px_3().py_2().child(
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

/// An editable label + value row backed by a real [`TextField`].
///
/// When `field` is the one being edited, the row swaps in the live `TextField`;
/// otherwise it shows the static `display_value` and a click seeds + focuses
/// the field, entering edit mode.
pub(super) fn editable_row(
    label: &'static str,
    display_value: String,
    field: NumericField,
    editing: Option<NumericField>,
    fields: &TextFields,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
    let is_editing = editing == Some(field);
    let entity: Entity<TextField> = fields.numeric(field).clone();

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
        .when(is_editing, |el| el.child(div().w(px(96.0)).child(entity.clone())))
        .when(!is_editing, |el| {
            let seed = entity.clone();
            el.child(
                div()
                    .id(SharedString::from(format!("insp-{label}")))
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_primary())
                    .px(px(4.0))
                    .rounded(px(2.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _ev, win, cx| {
                        // Seed the field with the current value, mark it as the
                        // active editing target, and focus it.
                        let pre = field.current_value(&this.app);
                        seed.update(cx, |f, cx| f.set_text(pre, win, cx));
                        this.editing_numeric = Some(field);
                        win.focus(&seed.focus_handle(cx));
                        cx.notify();
                    }))
                    .child(display_value),
            )
        })
}
