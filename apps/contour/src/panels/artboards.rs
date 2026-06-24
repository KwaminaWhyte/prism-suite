//! Artboards panel — list, select, rename, duplicate, and delete named artboards.
//!
//! All mutations route through the `AddArtboard2` / `RemoveArtboard` /
//! `RenameArtboard` / `SelectArtboard` / `DuplicateArtboard` actions.

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App};
use crate::panels::{ui_divider, ui_section_header};
use crate::rename_edit::{RenameEdit, RenameTarget};
use prism_ui::colors;
use crate::Contour;

pub fn render(
    app: &App,
    rename: Option<&RenameEdit>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let n = app.artboard_entries.len();
    let active_id = app.active_artboard;

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
                .child(format!("Artboards ({n})")),
        );

    let rows = app
        .artboard_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let id = entry.id;
            let name = entry.name.clone();
            let [_x, _y, w, h] = entry.rect;
            let is_active = active_id == Some(id);
            let is_renaming = matches!(
                rename.map(|r| r.target),
                Some(RenameTarget::Artboard(t)) if t == i
            );
            // Editable artboard name: focused TextField when renaming, else a
            // label that starts a rename on double-click.
            let name_el: gpui::AnyElement = if is_renaming {
                rename
                    .map(|r| r.field.clone().into_any_element())
                    .unwrap_or_else(|| div().child(name.clone()).into_any_element())
            } else {
                let start_name = name.clone();
                div()
                    .id(("ab-name", id))
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .child(name.clone())
                    .on_click(cx.listener(move |root, ev: &gpui::ClickEvent, win, cx| {
                        if ev.click_count() >= 2 {
                            root.begin_rename(
                                RenameTarget::Artboard(i),
                                start_name.clone(),
                                win,
                                cx,
                            );
                        }
                    }))
                    .into_any_element()
            };
            let bg = if is_active { colors::surface_raised() } else { colors::surface_bg() };

            let dup_btn = div()
                .id(("ab-dup", id))
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
                    root.app.apply(Action::DuplicateArtboard(i));
                    cx.notify();
                }))
                .child("⧉");

            let del_btn = div()
                .id(("ab-del", id))
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
                    root.app.apply(Action::RemoveArtboard(i));
                    cx.notify();
                }))
                .child("✕");

            div()
                .id(("ab-row", id))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .bg(bg)
                .border_b_1()
                .border_color(colors::surface_border())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SelectArtboard(id));
                    cx.notify();
                }))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(name_el)
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(10.0))
                                .child(format!("{:.0}×{:.0}", w, h)),
                        ),
                )
                .child(dup_btn)
                .child(del_btn)
        })
        .collect::<Vec<_>>();

    let empty_hint = if n == 0 {
        Some(
            div()
                .p_3()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child("Drag with the Artboard tool to add artboards."),
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
        .child(ui_section_header("Artboards"))
        .child(ui_divider())
        .child(header)
        .children(empty_hint)
        .children(rows)
}
