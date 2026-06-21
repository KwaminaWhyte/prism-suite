//! Layers panel — scrollable list of timeline layers with visibility / lock toggles.
//!
//! Interactions wired:
//!   - eye icon  → `Action::SetLayerVisible { id, visible: !visible }`
//!   - lock icon → `Action::SetLayerLocked { id, locked: !locked }`
//!   - row click → `Action::SetActiveLayer(id)`
//!   - "+" btn   → `Action::AddLayer { name, kind }`

use crate::app_state::{Action, App, LayerKind};
use crate::Drift;
use gpui::{div, px, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size};

pub fn render_layers(app: &App, cx: &mut Context<Drift>) -> impl IntoElement {
    let active = app.active_layer;

    div()
        .id("layers-panel")
        .w(px(240.0))
        .h_full()
        .bg(colors::surface_raised())
        .border_r_1()
        .border_color(colors::surface_border())
        .flex()
        .flex_col()
        .overflow_y_scroll()
        // Header
        .child(
            div()
                .w_full()
                .h(px(32.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(colors::surface_border())
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child("LAYERS"),
                )
                // Add layer button
                .child(
                    div()
                        .id("add-layer")
                        .w(px(20.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(font_size::SM))
                        .text_color(colors::text_secondary())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _ev, _win, cx| {
                            this.app.apply(Action::AddLayer {
                                name: "New Layer".to_string(),
                                kind: LayerKind::Vector,
                            });
                            cx.notify();
                        }))
                        .child("+"),
                ),
        )
        // Layer rows — reversed so topmost layer is first
        .children(app.layers.iter().rev().map(|layer| {
            let layer_id = layer.id;
            let is_active = active == Some(layer_id);
            let visible = layer.visible;
            let locked = layer.locked;
            let name = layer.name.clone();
            let kind_str = match layer.kind {
                LayerKind::Vector => "V",
                LayerKind::Bitmap => "B",
                LayerKind::Audio => "A",
                LayerKind::Camera => "C",
                LayerKind::Guide => "G",
                LayerKind::Null => "N",
            };

            div()
                .id(layer_id)
                .w_full()
                .h(px(32.0))
                .px_2()
                .flex()
                .items_center()
                .gap_1()
                .bg(if is_active {
                    colors::surface_overlay()
                } else {
                    colors::surface_raised()
                })
                .border_b_1()
                .border_color(colors::surface_border())
                .cursor_pointer()
                // Visibility eye
                .child(
                    div()
                        .id(SharedString::from(format!("vis-{layer_id}")))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(9.0))
                        .text_color(if visible {
                            colors::text_primary()
                        } else {
                            colors::text_disabled()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::SetLayerVisible {
                                id: layer_id,
                                visible: !visible,
                            });
                            cx.notify();
                        }))
                        .child(if visible { "●" } else { "○" }),
                )
                // Lock icon
                .child(
                    div()
                        .id(SharedString::from(format!("lock-{layer_id}")))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(9.0))
                        .text_color(if locked {
                            colors::accent()
                        } else {
                            colors::text_disabled()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::SetLayerLocked {
                                id: layer_id,
                                locked: !locked,
                            });
                            cx.notify();
                        }))
                        .child(if locked { "L" } else { "l" }),
                )
                // Kind badge
                .child(
                    div()
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(8.0))
                        .text_color(colors::text_secondary())
                        .bg(colors::surface_bg())
                        .rounded(px(2.0))
                        .child(kind_str),
                )
                // Layer name
                .child(
                    div()
                        .flex_1()
                        .text_size(px(font_size::SM))
                        .text_color(if is_active {
                            colors::text_primary()
                        } else {
                            colors::text_secondary()
                        })
                        .overflow_hidden()
                        .child(name),
                )
                // Select on row click
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    this.app.apply(Action::SetActiveLayer(layer_id));
                    cx.notify();
                }))
        }))
}
