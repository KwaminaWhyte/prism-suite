//! Symbols panel — browse, create, and place reusable symbol definitions.
//!
//! Uses the real `contour_app::symbols::Symbols` library (Batch 2). Creating a
//! symbol captures the selection's shapes; placing one resolves the master and
//! inserts the resulting shapes at the canvas centre.
//!
//! All mutations route through `Action::DefineSymbol` / `PlaceSymbol2` /
//! `EditSymbol2` / `DeleteSymbol` → `App::apply`.

use gpui::{
    div, px, rgb, rgba, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App};
use prism_ui::colors;
use crate::Contour;

const THUMB_COLORS: &[[f32; 3]] = &[
    [0.4, 0.6, 1.0],
    [1.0, 0.5, 0.3],
    [0.4, 0.9, 0.5],
    [0.9, 0.4, 0.9],
    [0.9, 0.9, 0.3],
    [0.3, 0.9, 0.9],
];

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let has_selection = !app.selection.is_empty();
    let n = app.symbol_lib.len();

    let create_fg = if has_selection { colors::text_primary() } else { colors::text_secondary() };
    let create_btn = div()
        .id("sym-create")
        .px_2()
        .h(px(20.0))
        .flex()
        .items_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .text_color(create_fg)
        .text_size(px(11.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            if !root.app.selection.is_empty() {
                root.app.apply(Action::DefineSymbol("Symbol".to_string()));
                cx.notify();
            }
        }))
        .child("+ Create");

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .flex_1()
                .text_color(colors::text_primary())
                .child(format!("Symbols ({n})")),
        )
        .child(create_btn);

    let rows = app
        .symbol_lib
        .list
        .iter()
        .enumerate()
        .map(|(i, sym)| {
            let id = sym.id;
            let name = sym.name.clone();
            let shape_count = sym.shapes.len();
            let [tr, tg, tb] = THUMB_COLORS[i % THUMB_COLORS.len()];
            let thumb_color =
                rgba(((255u32) << 24) | (((tr * 255.0) as u32) << 16) | (((tg * 255.0) as u32) << 8) | ((tb * 255.0) as u32));

            let place_btn = div()
                .id(("sym-place", id))
                .px_2()
                .h(px(18.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .border_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::PlaceSymbol2(id));
                    cx.notify();
                }))
                .child("Place");

            let del_btn = div()
                .id(("sym-del", id))
                .px_2()
                .h(px(18.0))
                .flex()
                .items_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .border_1()
                .border_color(colors::surface_border())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::DeleteSymbol(id));
                    cx.notify();
                }))
                .child("✕");

            div()
                .id(("sym-row", id))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(colors::surface_border())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::EditSymbol2(id));
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(32.0))
                        .h(px(32.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0x555555))
                        .bg(thumb_color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xffffff))
                        .text_size(px(10.0))
                        .child("□"),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_color(colors::text_primary())
                                .text_size(px(11.0))
                                .child(name),
                        )
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(10.0))
                                .child(format!("{} shape(s)", shape_count)),
                        ),
                )
                .child(place_btn)
                .child(del_btn)
        })
        .collect::<Vec<_>>();

    let empty_hint = if n == 0 {
        Some(
            div()
                .p_3()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child("Select shapes, then click + Create."),
        )
    } else {
        None
    };

    div()
        .flex()
        .flex_col()
        .bg(colors::surface_bg())
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .children(empty_hint)
        .children(rows)
}
