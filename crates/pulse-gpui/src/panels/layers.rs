//! Layers panel — one row per layer in the active comp (top row = topmost layer).
//!
//! Wave 11: layer parenting (pick-whip, indent by depth, clear-parent ×),
//! pre-compose (breadcrumb, "Pre-comp selected" button), sub-comp navigation,
//! and NULL badge for Null-kind layers.

use gpui::{
    div, px, rgb, svg, white, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;
use std::collections::HashMap;

use prism_ui::{colors, Icon, badge, section_header};
use pulse_app::comp::LayerKind;

use crate::app_state::{Action, App};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

fn parent_depth(layer_parents: &HashMap<usize, usize>, idx: usize) -> u32 {
    let mut depth = 0u32;
    let mut cur = idx;
    let mut visited = std::collections::HashSet::new();
    while let Some(&parent) = layer_parents.get(&cur) {
        if visited.contains(&parent) { break; }
        visited.insert(parent);
        depth += 1;
        cur = parent;
        if depth > 10 { break; }
    }
    depth
}

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];
    let selected = app.selected_layer;
    let picking_active = app.picking_parent_for.is_some();
    let picking_for = app.picking_parent_for;

    // Sub-comp view: show breadcrumb + sub-comp layers
    if let Some(sci) = app.active_sub_comp {
        let sub = &app.sub_comps[sci];
        let sub_name = sub.name.clone();
        let sub_layer_count = sub.layers.len();
        let sub_rows: Vec<gpui::AnyElement> = (0..sub_layer_count)
            .rev()
            .map(|i| {
                let l = &sub.layers[i];
                let chip = {
                    let c = l.color;
                    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
                    (to8(c[0]) << 16) | (to8(c[1]) << 8) | to8(c[2])
                };
                div()
                    .id(("sub-layer-row", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .bg(colors::surface_raised())
                    .text_color(white())
                    .child(div().w(px(10.0)).h(px(10.0)).rounded_sm().bg(rgb(chip)))
                    .child(div().flex_1().child(l.name.clone()))
                    .when(l.kind == LayerKind::Null, |d| d.child(badge("NULL")))
                    .when(l.kind == LayerKind::Guide, |d| d.child(badge("GUIDE")))
                    .into_any_element()
            })
            .collect();

        return div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(section_header(format!("Sub-comp: {}", sub_name))),
            )
            .child(
                div()
                    .id("sub-comp-back")
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .bg(rgb(BG_ACTIVE))
                    .child("< Main Comp")
                    .on_click(cx.listener(|root, _ev, _win, cx| {
                        root.app.apply(Action::CloseSubComp);
                        cx.notify();
                    })),
            )
            .children(sub_rows);
    }

    let n = comp.layers.len();
    let hide_shy = comp.hide_shy;

    // Pre-comp selected button
    let pre_comp_btn: Option<gpui::AnyElement> = selected.map(|sel| {
        div()
            .id("pre-comp-selected-btn")
            .px_3()
            .py_1()
            .cursor_pointer()
            .text_color(colors::text_primary())
            .text_size(px(10.0))
            .bg(colors::surface_raised())
            .child("Pre-comp selected")
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let n_sub = root.app.sub_comps.len() + 1;
                root.app.apply(Action::PreCompose(vec![sel], format!("Pre-comp {}", n_sub)));
                cx.notify();
            }))
            .into_any_element()
    });

    // Cancel pick-whip button
    let cancel_pick: Option<gpui::AnyElement> = if picking_active {
        Some(
            div()
                .id("cancel-pick-whip")
                .px_3()
                .py_1()
                .cursor_pointer()
                .text_color(rgb(0xe06c75u32))
                .text_size(px(10.0))
                .bg(colors::surface_raised())
                .child("Cancel pick-whip")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::SetPickingParent(None));
                    cx.notify();
                }))
                .into_any_element()
        )
    } else {
        None
    };

    let rows: Vec<gpui::AnyElement> = (0..n)
        .rev()
        .filter(|&i| !(hide_shy && comp.layers[i].shy))
        .map(|i| {
            let l = &comp.layers[i];
            let is_selected = Some(i) == selected;
            let is_picking_this = picking_for == Some(i);
            let has_parent = app.layer_parents.contains_key(&i);
            let depth = parent_depth(&app.layer_parents, i);
            let indent = depth as f32 * 16.0;
            let is_precomp_placeholder = app.pre_comp_layers.contains_key(&i);
            let is_matte_src = comp.is_matte_source(i);
            let matte_label = if l.matte.is_active() { Some(l.matte.label()) } else { None };
            let is_solo = l.solo;
            let is_shy = l.shy;

            // row_bg is used as a u32 via rgb() below; BG_ACTIVE stays as literal.
            let row_bg_color: gpui::Rgba = if is_picking_this {
                rgb(0x2d4a7a)
            } else if is_selected {
                rgb(BG_ACTIVE)
            } else {
                colors::surface_raised()
            };

            let chip = {
                let c = l.color;
                let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
                (to8(c[0]) << 16) | (to8(c[1]) << 8) | to8(c[2])
            };

            let row = div()
                .id(("layer-row", i))
                .flex()
                .items_center()
                .gap_2()
                .pl(px(indent + 12.0))
                .pr(px(4.0))
                .py_2()
                .bg(row_bg_color)
                .text_color(white())
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    // Pick-whip: if active and this is a different layer, set parent
                    if let Some(child_idx) = root.app.picking_parent_for {
                        if child_idx != i {
                            root.app.apply(Action::SetParent(child_idx, i));
                            cx.notify();
                            return;
                        }
                    }
                    // Pre-comp placeholder click → open sub-comp
                    if let Some(&sci) = root.app.pre_comp_layers.get(&i) {
                        root.app.apply(Action::OpenSubComp(sci));
                        cx.notify();
                        return;
                    }
                    root.app.apply(Action::SelectLayer(i));
                    cx.notify();
                }))
                .child(
                    div()
                        .id(("layer-vis", i))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleLayerVisible(i));
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path(if l.visible { Icon::Eye.path() } else { Icon::EyeOff.path() })
                                .w(px(14.0))
                                .h(px(14.0))
                                .text_color(if l.visible { colors::text_primary() } else { colors::text_disabled() }),
                        ),
                )
                .child(div().flex_shrink_0().w(px(10.0)).h(px(10.0)).rounded_sm().bg(rgb(chip)))
                // Solo button
                .child(
                    div()
                        .flex_shrink_0()
                        .id(("layer-solo", i))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_size(px(9.0))
                        .text_color(if is_solo { colors::text_primary() } else { colors::text_disabled() })
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleSolo(i));
                            cx.notify();
                        }))
                        .child(if is_solo { "◉" } else { "○" })
                )
                // Shy button
                .child(
                    div()
                        .flex_shrink_0()
                        .id(("layer-shy", i))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_size(px(9.0))
                        .text_color(if is_shy { colors::text_primary() } else { colors::text_disabled() })
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleShy(i));
                            cx.notify();
                        }))
                        .child(if is_shy { "H" } else { "h" })
                )
                .child(div().flex_1().child(l.name.clone()))
                .when(l.kind == LayerKind::Null, |d| d.child(div().flex_shrink_0().child(badge("NULL"))))
                .when(l.kind == LayerKind::Guide, |d| d.child(
                    div()
                        .flex_shrink_0()
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(rgb(0x2a2a6au32))
                        .text_color(rgb(0x8888ffu32))
                        .text_size(px(9.0))
                        .child("GUIDE")
                ))
                .when(is_precomp_placeholder, |d| d.child(div().flex_shrink_0().child(badge("PRE"))))
                .when(l.threed, |d| d.child(div().flex_shrink_0().child(badge("3D"))))
                // Track-matte source "T" badge
                .when(is_matte_src, |d| d.child(
                    div()
                        .flex_shrink_0()
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(rgb(0x2a6a2au32))
                        .text_color(rgb(0x88ff88u32))
                        .text_size(px(9.0))
                        .child("T")
                ))
                // Matte mode label badge
                .when(matte_label.is_some(), |d| {
                    let label = matte_label.unwrap_or_default();
                    d.child(
                        div()
                            .flex_shrink_0()
                            .px(px(4.0))
                            .py(px(1.0))
                            .rounded_sm()
                            .bg(rgb(0x5a2a6au32))
                            .text_color(rgb(0xcc88ffu32))
                            .text_size(px(9.0))
                            .child(label)
                    )
                })
                .when(has_parent, |d| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .id(("layer-clear-parent", i))
                            .w(px(16.0))
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .bg(rgb(0x3e3e3eu32))
                            .text_color(colors::text_secondary())
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                root.app.apply(Action::ClearParent(i));
                                cx.notify();
                            }))
                    )
                })
                .child(
                    div()
                        .flex_shrink_0()
                        .id(("layer-pick-whip", i))
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(if picking_active && !is_picking_this { rgb(0x2ecc71u32) } else { colors::surface_raised() })
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .child("○")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            if let Some(child_idx) = root.app.picking_parent_for {
                                if child_idx != i {
                                    root.app.apply(Action::SetParent(child_idx, i));
                                } else {
                                    root.app.apply(Action::SetPickingParent(None));
                                }
                            } else {
                                root.app.apply(Action::SetPickingParent(Some(i)));
                            }
                            cx.notify();
                        }))
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(format!("{:?}", l.kind)),
                );

            row.into_any_element()
        })
        .collect();

    let hide_shy_now = comp.hide_shy;
    let shy_toggle = div()
        .id("toggle-hide-shy")
        .cursor_pointer()
        .px(px(6.0))
        .py(px(2.0))
        .rounded_sm()
        .text_size(px(9.0))
        .text_color(if hide_shy_now { colors::text_primary() } else { colors::text_disabled() })
        .bg(if hide_shy_now { rgb(0x3a3a3au32) } else { colors::surface_raised() })
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::ToggleHideShy);
            cx.notify();
        }))
        .child("Shy");

    div()
        .flex_1()
        .flex()
        .flex_col()
        .child(
            div()
                .border_b_1()
                .border_color(colors::surface_border())
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().child(section_header(format!("Layers — {}", comp.name))))
                .child(shy_toggle),
        )
        .children(cancel_pick)
        .children(pre_comp_btn)
        .children(rows)
}
