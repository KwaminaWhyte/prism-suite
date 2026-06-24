//! Layers panel — scrollable list of timeline layers with visibility / lock toggles.
//!
//! Interactions wired:
//!   - eye icon  → `Action::SetLayerVisible { id, visible: !visible }`
//!   - lock icon → `Action::SetLayerLocked { id, locked: !locked }`
//!   - row click → `Action::SetActiveLayer(id)`
//!   - "+" btn   → `Action::AddLayer { name, kind }`

use crate::app_state::{Action, App, LayerKind};
use crate::Drift;
use gpui::prelude::FluentBuilder;
use gpui::{div, svg, px, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled};
use prism_ui::{colors, font_size, Icon, TextField};

/// Render the layers panel. `renaming_layer` is the layer currently being
/// renamed inline (if any); `rename_field` is the shared editable [`TextField`]
/// shown in place of that layer's name label.
pub fn render_layers(
    app: &App,
    renaming_layer: Option<usize>,
    rename_field: Entity<TextField>,
    cx: &mut Context<Drift>,
) -> impl IntoElement {
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
                // Add layer buttons: V = Vector, B = Bitmap
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            div()
                                .id("add-vector-layer")
                                .w(px(22.0))
                                .h(px(20.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(font_size::XS))
                                .text_color(colors::text_secondary())
                                .bg(colors::surface_overlay())
                                .rounded(px(2.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _ev, _win, cx| {
                                    this.app.apply(Action::AddLayer {
                                        name: "Vector Layer".to_string(),
                                        kind: LayerKind::Vector,
                                    });
                                    cx.notify();
                                }))
                                .child(svg().path(Icon::Pen.path()).w(px(11.0)).h(px(11.0)).text_color(colors::text_secondary())),
                        )
                        .child(
                            div()
                                .id("add-bitmap-layer")
                                .w(px(22.0))
                                .h(px(20.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(colors::surface_overlay())
                                .rounded(px(2.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _ev, _win, cx| {
                                    this.app.apply(Action::AddLayer {
                                        name: "Bitmap Layer".to_string(),
                                        kind: LayerKind::Bitmap,
                                    });
                                    cx.notify();
                                }))
                                .child(svg().path(Icon::Layers.path()).w(px(11.0)).h(px(11.0)).text_color(colors::text_secondary())),
                        ),
                ),
        )
        // Layer rows — reversed so topmost layer is first
        .children(app.layers.iter().rev().map(|layer| {
            let layer_id = layer.id;
            let is_active = active == Some(layer_id);
            let visible = layer.visible;
            let locked = layer.locked;
            let name = layer.name.clone();
            let is_renaming = renaming_layer == Some(layer_id);
            let rf = rename_field.clone();
            // Derive a display color from the layer's color_tag string.
            let tag_color: gpui::Rgba = match layer.color_tag.as_str() {
                "red"    => gpui::rgba(0xef4444ff),
                "orange" => gpui::rgba(0xf97316ff),
                "yellow" => gpui::rgba(0xeab308ff),
                "green"  => gpui::rgba(0x22c55eff),
                "blue"   => gpui::rgba(0x3b82f6ff),
                "purple" => gpui::rgba(0xa855f7ff),
                "pink"   => gpui::rgba(0xec4899ff),
                _        => gpui::rgba(0x44444488),
            };
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
                // Color tag dot
                .child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(tag_color)
                        .flex_shrink_0(),
                )
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
                        .child(svg().path(if visible { Icon::Eye.path() } else { Icon::EyeOff.path() }).w(px(11.0)).h(px(11.0)).text_color(if visible { colors::text_primary() } else { colors::text_disabled() })),
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
                        .child(svg().path(if locked { Icon::Lock.path() } else { Icon::Unlock.path() }).w(px(11.0)).h(px(11.0)).text_color(if locked { colors::accent() } else { colors::text_disabled() })),
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
                // Layer name — double-click to rename inline via a real TextField.
                .child(
                    div()
                        .id(SharedString::from(format!("name-{layer_id}")))
                        .flex_1()
                        .overflow_hidden()
                        .when(is_renaming, |el: gpui::Stateful<gpui::Div>| el.child(rf.clone()))
                        .when(!is_renaming, |el: gpui::Stateful<gpui::Div>| {
                            let rf_start = rf.clone();
                            let label = name.clone();
                            let seed_name = name.clone();
                            el.text_size(px(font_size::SM))
                                .text_color(if is_active {
                                    colors::text_primary()
                                } else {
                                    colors::text_secondary()
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, win, cx| {
                                    if ev.click_count() >= 2 {
                                        // Begin inline rename: seed the field with the
                                        // current name and focus it.
                                        this.renaming_layer = Some(layer_id);
                                        let n = seed_name.clone();
                                        rf_start.update(cx, |field, cx| {
                                            field.set_text(n, win, cx);
                                        });
                                        win.focus(&rf_start.focus_handle(cx));
                                        cx.notify();
                                    } else {
                                        this.app.apply(Action::SetActiveLayer(layer_id));
                                        cx.notify();
                                    }
                                }))
                                .child(label)
                        }),
                )
                // Delete button [×]
                .child(
                    div()
                        .id(("layer-del", layer_id))
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(9.0))
                        .text_color(colors::text_disabled())
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.app.apply(Action::DeleteLayer(layer_id));
                            cx.notify();
                        }))
                        .child(svg().path(Icon::Trash.path()).w(px(11.0)).h(px(11.0)).text_color(colors::text_disabled())),
                )
                // Select on row click
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    this.app.apply(Action::SetActiveLayer(layer_id));
                    cx.notify();
                }))
        }))
}
