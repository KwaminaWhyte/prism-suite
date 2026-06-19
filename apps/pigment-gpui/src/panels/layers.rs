//! Layers panel — a full Affinity/Photoshop-style layer stack.
//!
//! One row per layer (top row = topmost layer; the layer vec is bottom-up so we
//! iterate reversed). Every interaction is the same Action round-trip described
//! in `panels/mod.rs`: a stateful element (`.id(..)`) gets an `.on_click` that
//! calls `root.app.apply(action)` then `cx.notify()`.
//!
//! Wired interactions:
//!   - visibility eye icon → `Action::ToggleLayerVisible(id)`
//!   - row body            → `Action::SelectLayer(id)`
//!   - blend label         → `Action::SetLayerBlend(id, next)` (cycles variants)
//!   - opacity − / +       → `Action::SetLayerOpacity(id, clamp(o ± 0.1))`
//!   - footer ↑ / ↓        → `Action::MoveLayer { id, up }`
//!   - footer trash icon   → `Action::DeleteLayer(id)`

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_core::{Adjustment, BlendMode, LayerId, LayerKind};
use prism_ui::{colors, divider, font_size, icon_colored, section_header, spacing, Icon};

use crate::app_state::{Action, App};
use crate::panels::adjust_edit::adjustment_editor;
use crate::Pigment;

/// All blend modes in stack order, used to cycle and to label.
const BLEND_MODES: [BlendMode; 18] = [
    BlendMode::Normal,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::LinearDodge,
    BlendMode::LinearBurn,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

/// The blend mode that follows `m` when the label is clicked (wraps around).
fn next_blend(m: BlendMode) -> BlendMode {
    let i = BLEND_MODES.iter().position(|&b| b == m).unwrap_or(0);
    BLEND_MODES[(i + 1) % BLEND_MODES.len()]
}

/// The badge text for a non-raster layer kind (adjustment layers show their
/// adjustment name). `None` for raster (which keeps the click-to-cycle blend
/// label instead).
fn layer_kind_label(kind: &LayerKind) -> Option<String> {
    match kind {
        LayerKind::Adjustment(a) => Some(a.name().to_string()),
        _ => None,
    }
}

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let active = app.doc.active_layer;
    let layers = &app.doc.layers.layers;
    let count = layers.len();
    let active_is_smart = active.is_some_and(|id| app.smart_objects.contains_key(&id));

    // Active-layer summary (blend + opacity) shown in the header.
    let summary = active
        .and_then(|aid| layers.iter().find(|l| l.id == aid))
        .map(|l| format!("{:?} · {:.0}%", l.blend, l.opacity * 100.0));

    // Top row = topmost layer: the layer vec is bottom-up, so iterate reversed.
    let rows = layers
        .iter()
        .rev()
        .map(|l| {
            let id = l.id;
            let is_active = Some(id) == active;
            let visible = l.visible;
            let opacity = l.opacity;
            let blend = l.blend;
            let name = l.name.clone();
            // Adjustment layers carry their kind name + the `Adjustment` (cloned so
            // the active-row editor can build nudged copies). A small mask dot marks
            // masked layers.
            let adjustment: Option<Adjustment> = match &l.kind {
                LayerKind::Adjustment(a) => Some(a.clone()),
                _ => None,
            };
            let kind_label = layer_kind_label(&l.kind);
            let has_mask = app.masked_layers.contains(&id);
            let is_smart = app.smart_objects.contains_key(&id);
            // Smart filter sub-items for this layer.
            let sf_stack: Vec<_> = app.smart_filters.get(&id)
                .cloned()
                .unwrap_or_default();
            let sf_count = sf_stack.len();
            let has_style = app.layer_styles.contains_key(&id);
            let is_clipped = app.clipping_masks.contains(&id);

            let vis_icon = if visible { Icon::Eye } else { Icon::EyeOff };
            let vis_color = if visible {
                colors::text_primary()
            } else {
                colors::text_disabled()
            };
            let name_color = if visible {
                colors::text_primary()
            } else {
                colors::text_secondary()
            };
            let row_bg = if is_active {
                colors::surface_overlay()
            } else {
                colors::surface_raised()
            };

            // The adjustment param editor renders only under the ACTIVE adjustment
            // row (built before the row so the row closure can move what it needs).
            let editor = if is_active {
                adjustment.as_ref().map(|a| adjustment_editor(id, a, app, cx))
            } else {
                None
            };

            let main_row = div()
                .id(("lyr-row", id.0))
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .bg(row_bg)
                .border_b_1()
                .border_color(colors::surface_bg())
                .when(is_active, |s| s.border_l_2().border_color(colors::accent()))
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                // Row body click → select this layer.
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SelectLayer(id));
                    cx.notify();
                }))
                // Visibility eye icon — its own click target.
                .child(
                    div()
                        .id(("lyr-vis", id.0))
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleLayerVisible(id));
                            cx.notify();
                        }))
                        .child(icon_colored(vis_icon, 12.0, vis_color)),
                )
                // Thumbnail placeholder.
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded_sm()
                        .bg(colors::surface_bg())
                        .border_1()
                        .border_color(colors::surface_border()),
                )
                // Name + (kind badge for adjustments) + blend label + mask dot.
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .text_color(name_color)
                                        .text_size(px(font_size::MD))
                                        .child(name),
                                )
                                // Mask present marker (a small "M" chip).
                                .when(has_mask, |s| {
                                    s.child(
                                        div()
                                            .px_1()
                                            .rounded_sm()
                                            .bg(colors::surface_border())
                                            .text_color(colors::text_secondary())
                                            .text_size(px(font_size::XS))
                                            .child("M"),
                                    )
                                })
                                // Smart Object badge.
                                .when(is_smart, |s| {
                                    s.child(
                                        div()
                                            .px_1()
                                            .rounded_sm()
                                            .bg(colors::surface_overlay())
                                            .text_color(colors::accent())
                                            .text_size(px(font_size::XS))
                                            .child("SO"),
                                    )
                                })
                                // Layer effects / fx badge (Wave 11).
                                .when(has_style, |s| {
                                    s.child(
                                        div()
                                            .id(("lyr-fx", id.0))
                                            .px_1()
                                            .rounded_sm()
                                            .bg(colors::surface_overlay())
                                            .text_color(colors::text_secondary())
                                            .text_size(px(font_size::XS))
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(colors::accent()))
                                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                                root.app.apply(Action::OpenStylePanel(id));
                                                cx.notify();
                                            }))
                                            .child("fx"),
                                    )
                                })
                                // Clipping mask indent arrow (Wave 11).
                                .when(is_clipped, |s| {
                                    s.child(
                                        div()
                                            .px_1()
                                            .text_color(colors::accent())
                                            .text_size(px(font_size::XS))
                                            .child("↳"),
                                    )
                                }),
                        )
                        .child(
                            // Adjustment rows show their kind name (no blend cycle);
                            // raster rows keep the click-to-cycle blend label. One
                            // stateful div either way so the arms share a type.
                            div()
                                .id(("lyr-blend", id.0))
                                .text_size(px(font_size::XS))
                                .map(|s| match &kind_label {
                                    Some(k) => {
                                        s.text_color(colors::accent()).child(k.clone())
                                    }
                                    None => s
                                        .text_color(colors::text_secondary())
                                        .cursor_pointer()
                                        .hover(|s| s.text_color(colors::text_primary()))
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            root.app.apply(Action::SetLayerBlend(
                                                id,
                                                next_blend(blend),
                                            ));
                                            cx.notify();
                                        }))
                                        .child(format!("{:?}", blend)),
                                }),
                        ),
                )
                // Opacity steppers: −  NN%  +
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id(("lyr-opdn", id.0))
                                .w(px(16.0))
                                .h(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_primary())
                                .text_size(px(font_size::MD))
                                .cursor_pointer()
                                .hover(|s| s.bg(colors::accent()))
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    let v = (opacity - 0.1).clamp(0.0, 1.0);
                                    root.app.apply(Action::SetLayerOpacity(id, v));
                                    cx.notify();
                                }))
                                .child("−"),
                        )
                        .child(
                            div()
                                .w(px(30.0))
                                .text_color(colors::text_secondary())
                                .text_size(px(font_size::XS))
                                .child(format!("{:.0}%", opacity * 100.0)),
                        )
                        .child(
                            div()
                                .id(("lyr-opup", id.0))
                                .w(px(16.0))
                                .h(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_primary())
                                .text_size(px(font_size::MD))
                                .cursor_pointer()
                                .hover(|s| s.bg(colors::accent()))
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    let v = (opacity + 0.1).clamp(0.0, 1.0);
                                    root.app.apply(Action::SetLayerOpacity(id, v));
                                    cx.notify();
                                }))
                                .child("+"),
                        ),
                );

            // Build smart filter sub-rows (indented, shown for every layer that has filters).
            let mut sf_rows: Vec<gpui::AnyElement> = Vec::new();
            for (fi, sf) in sf_stack.iter().enumerate() {
                let label = sf.label();
                let row_id = format!("sf-{}-{}", id.0, fi);
                let remove_id = format!("sf-rm-{}-{}", id.0, fi);
                // Capture for closures.
                let layer_id = id;
                let idx = fi;
                let sf_row = div()
                    .id(gpui::SharedString::from(row_id))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .pl(px(28.0))
                    .pr_2()
                    .py(px(1.5))
                    .bg(colors::surface_bg())
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(colors::text_secondary())
                            .child("↳"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.0))
                            .text_color(colors::text_secondary())
                            .child(label),
                    )
                    .child(
                        div()
                            .id(gpui::SharedString::from(remove_id))
                            .w(px(14.0))
                            .h(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .text_size(px(9.0))
                            .text_color(colors::text_disabled())
                            .cursor_pointer()
                            .hover(|s| s.text_color(colors::text_primary()))
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                root.app.apply(Action::RemoveSmartFilter(layer_id, idx));
                                cx.notify();
                            }))
                            .child("×"),
                    );
                sf_rows.push(sf_row.into_any_element());
            }
            let _ = sf_count; // suppress unused warning

            // Stack the main row with the (optional) active-adjustment editor and smart filter sub-rows.
            div()
                .flex()
                .flex_col()
                .child(main_row)
                .when_some(editor, |s, ed| s.child(ed))
                .children(sf_rows)
        })
        .collect::<Vec<_>>();

    // Footer action buttons operate on the active layer.
    let active_has_mask = active.is_some_and(|id| app.masked_layers.contains(&id));
    let active_is_clipped = active.is_some_and(|id| app.clipping_masks.contains(&id));
    let active_has_style = active.is_some_and(|id| app.layer_styles.contains_key(&id));
    let footer = footer_buttons(active, active_has_mask, app.edit_mask, active_is_smart,
        active_is_clipped, active_has_style, cx);

    div()
        .w_full()
        .flex()
        .flex_col()
        .bg(colors::surface_raised())
        .text_color(colors::text_primary())
        // Header: "Layers" section header + count + active-layer summary.
        .child(section_header("Layers"))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px(px(spacing::MD))
                .py(px(spacing::XS))
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child(format!("({count})")),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child(summary.unwrap_or_default()),
                ),
        )
        .child(divider())
        .children(rows)
        .child(footer)
}

/// The footer for the active layer: a move/delete row plus a mask row (add or
/// delete a mask + toggle mask-edit mode). When no layer is active the buttons
/// render disabled (dim, no listener). `has_mask` / `edit_mask` describe the
/// active layer's mask state.
fn footer_buttons(
    active: Option<LayerId>,
    has_mask: bool,
    edit_mask: bool,
    is_smart: bool,
    is_clipped: bool,
    has_style: bool,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let has_active = active.is_some();
    let fg = if has_active {
        colors::text_primary()
    } else {
        colors::text_disabled()
    };

    // Each icon button is always a `Stateful<Div>` (an `.id(..)` is required
    // before `.on_click`); the click listener is attached only when a layer is active.
    let icon_btn = |key: &'static str, ico: Icon| {
        div()
            .id(key)
            .w(px(28.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .bg(colors::surface_overlay())
            .text_color(fg)
            .when(has_active, |s| {
                s.cursor_pointer().hover(|s| s.bg(colors::tool_hover()))
            })
            .child(icon_colored(ico, 13.0, fg))
    };

    // A wider labeled pill button (mask row), enabled only when `enabled`.
    let pill = |key: &'static str, lbl: &'static str, enabled: bool, accent: bool| {
        let (bg, txt) = if !enabled {
            (colors::surface_overlay(), colors::text_disabled())
        } else if accent {
            (colors::accent(), colors::text_primary())
        } else {
            (colors::surface_overlay(), colors::text_primary())
        };
        div()
            .id(key)
            .h(px(20.0))
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .bg(bg)
            .text_size(px(font_size::XS))
            .text_color(txt)
            .when(enabled, |s| {
                s.cursor_pointer().hover(|s| s.bg(colors::tool_hover()))
            })
            .child(lbl)
    };

    let move_row = div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .child(divider())
        .child(
            icon_btn("lyr-move-up", Icon::ArrowUp).when_some(active, |s, id| {
                s.on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::MoveLayer { id, up: true });
                    cx.notify();
                }))
            }),
        )
        .child(
            icon_btn("lyr-move-dn", Icon::ArrowDown).when_some(active, |s, id| {
                s.on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::MoveLayer { id, up: false });
                    cx.notify();
                }))
            }),
        )
        .child(div().flex_1())
        .child(
            icon_btn("lyr-delete", Icon::Trash).when_some(active, |s, id| {
                s.on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::DeleteLayer(id));
                    cx.notify();
                }))
            }),
        );

    // Mask row: Add Mask (when none) / Delete Mask (when present) + Edit Mask
    // toggle. All gated on an active layer; the toggle is gated on a mask.
    let add_or_del = if has_mask {
        pill("lyr-mask-del", "Delete Mask", has_active, false).when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::DeleteMask(id));
                cx.notify();
            }))
        })
    } else {
        pill("lyr-mask-add", "Add Mask", has_active, false).when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::AddMask(id));
                cx.notify();
            }))
        })
    };
    let edit_toggle = pill(
        "lyr-mask-edit",
        if edit_mask { "Editing Mask" } else { "Edit Mask" },
        has_mask,
        edit_mask,
    )
    .when(has_mask, |s| {
        s.on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ToggleEditMask);
            cx.notify();
        }))
    });

    let mask_row = div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(colors::surface_border())
        .child(add_or_del)
        .child(edit_toggle)
        .child(div().flex_1());

    // Smart Object row: convert/rasterize, only when an active layer exists.
    let so_convert = if is_smart {
        pill("lyr-so-rast", "Rasterize", has_active, false).when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(crate::app_state::Action::RasterizeSmartObject(id));
                cx.notify();
            }))
        })
    } else {
        pill("lyr-so-conv", "→ Smart Obj", has_active, false).when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(crate::app_state::Action::ConvertToSmartObject(id));
                cx.notify();
            }))
        })
    };
    let so_edit = pill("lyr-so-edit", "Edit SO", is_smart, false).when(is_smart, |s| {
        s.when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(crate::app_state::Action::EditSmartObject(id));
                cx.notify();
            }))
        })
    });
    let so_row = div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(colors::surface_border())
        .child(so_convert)
        .child(so_edit)
        .child(div().flex_1());

    // ---- Wave 11: Layer Style + Clipping Mask row --------------------------------
    let style_btn = {
        let lbl = if has_style { "Layer Style…●" } else { "Layer Style…" };
        pill("lyr-style-btn", lbl, has_active, false).when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::OpenStylePanel(id));
                cx.notify();
            }))
        })
    };
    let clip_lbl = if is_clipped { "Remove Clip" } else { "Create Clip" };
    let clip_btn = pill("lyr-clip-btn", clip_lbl, has_active, is_clipped)
        .when_some(active, |s, id| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::ToggleClippingMask(id));
                cx.notify();
            }))
        });

    let wave11_row = div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(colors::surface_border())
        .child(style_btn)
        .child(clip_btn)
        .child(div().flex_1());

    // Smart filter footer: "Add SF" adds a default Blur(2.0) SF to active layer.
    let add_sf_btn = pill("lyr-add-sf", "Add SF", has_active, false).when_some(active, |s, id| {
        s.on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::AddSmartFilter(id, crate::app_state::SmartFilter::Blur(2.0)));
            cx.notify();
        }))
    });
    let sf_footer_row = div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(colors::surface_border())
        .child(add_sf_btn)
        .child(div().flex_1());

    div()
        .flex()
        .flex_col()
        .child(divider())
        .child(move_row)
        .child(mask_row)
        .child(so_row)
        .child(wave11_row)
        .child(sf_footer_row)
}
