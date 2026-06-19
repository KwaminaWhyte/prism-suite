//! Layer Comps panel — snapshot/restore layer visibility, opacity, blend, offset.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, Div, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, section_header};

use crate::app_state::{Action, App};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let comps = &app.layer_comps;
    let active_idx = app.active_comp_idx;

    let rows: Vec<_> = comps
        .iter()
        .enumerate()
        .map(|(i, comp)| {
            let name = comp.name.clone();
            let is_active = active_idx == Some(i);
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(colors::surface_border())
                .bg(if is_active { colors::surface_overlay() } else { colors::surface_raised() })
                .child(
                    div()
                        .id(("lc-apply", i as u64))
                        .cursor_pointer()
                        .px_2()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ApplyLayerComp(i));
                            cx.notify();
                        }))
                        .child("▶"),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(font_size::XS))
                        .child(name.clone()),
                )
                .child(
                    div()
                        .id(("lc-upd", i as u64))
                        .cursor_pointer()
                        .px_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::UpdateLayerComp(i));
                            cx.notify();
                        }))
                        .child("↺"),
                )
                .child(
                    div()
                        .id(("lc-del", i as u64))
                        .cursor_pointer()
                        .px_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::DeleteLayerComp(i));
                            cx.notify();
                        }))
                        .child("✕"),
                )
        })
        .collect();

    let comp_count = comps.len();

    div()
        .w_full()
        .flex()
        .flex_col()
        .bg(colors::surface_raised())
        .child(section_header("Layer Comps"))
        .child(
            div()
                .px_2()
                .py_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(font_size::XS))
                        .child(format!("{comp_count} comp{}", if comp_count == 1 { "" } else { "s" })),
                )
                .child(
                    div()
                        .id("lc-add")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_overlay())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .text_size(px(font_size::XS))
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let n = root.app.layer_comps.len() + 1;
                            root.app.apply(Action::AddLayerComp(format!("Comp {n}")));
                            cx.notify();
                        }))
                        .child("+ New Comp"),
                ),
        )
        .child(divider())
        .children(rows)
        .when(comps.is_empty(), |s: Div| {
            s.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .child("No comps yet — click '+ New Comp' to snapshot layer states"),
            )
        })
}
