//! Graphic Styles panel — grid of style thumbnails, click to apply.
use gpui::{div, px, IntoElement, ParentElement, Styled, InteractiveElement, StatefulInteractiveElement};
use prism_ui::colors;
use crate::app_state::{Action, App};
use crate::Contour;

pub fn render(app: &App, cx: &mut gpui::Context<Contour>) -> impl IntoElement {
    let styles: Vec<_> = app.doc.graphic_styles.list.iter().map(|s| {
        let id = s.id;
        let swatch = s.appearance.fills.first()
            .map(|f| f.paint.swatch())
            .or_else(|| s.appearance.strokes.first().map(|st| st.paint.swatch()))
            .unwrap_or([0.5, 0.5, 0.5, 1.0]);
        (id, s.name.clone(), swatch)
    }).collect();

    let new_btn = div()
        .id("gs-new")
        .cursor_pointer()
        .p_1()
        .text_size(px(11.0))
        .text_color(colors::text_secondary())
        .bg(colors::surface_raised())
        .rounded_sm()
        .child("+ New Style")
        .on_click(cx.listener(|root, _ev, _win, cx| {
            root.app.apply(Action::SaveGraphicStyle("New Style".to_string()));
            cx.notify();
        }));

    let mut grid_items = Vec::new();
    for (id, name, color) in styles {
        let [r, g, b, _a] = color;
        let ru = (r * 255.0) as u8;
        let gu = (g * 255.0) as u8;
        let bu = (b * 255.0) as u8;
        let fill_color = gpui::rgb(((ru as u32) << 16) | ((gu as u32) << 8) | (bu as u32));
        let item = div()
            .id(("gs-item", id))
            .cursor_pointer()
            .flex()
            .flex_col()
            .items_center()
            .gap_px()
            .p_1()
            .w(px(56.0))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::ApplyGraphicStyle(id));
                cx.notify();
            }))
            .child(
                div()
                    .w(px(40.0))
                    .h(px(40.0))
                    .rounded_sm()
                    .bg(fill_color)
                    .border_1()
                    .border_color(colors::surface_border())
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(colors::text_secondary())
                    .w(px(52.0))
                    .child(name)
            );
        grid_items.push(item.into_any_element());
    }

    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .child(
            div()
                .text_size(px(11.0))
                .text_color(colors::text_secondary())
                .child("Graphic Styles")
        )
        .child(new_btn)
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_1()
                .children(grid_items)
        )
}
