//! Layers panel — one row per shape (top row = topmost / last-painted shape).
//!
//! Uses `prism_ui::section_header`, `prism_ui::divider`, and icon buttons from
//! the design system for visibility toggle (Eye/EyeOff), add (Add), and delete
//! (Trash).  The Action round-trip is identical to the previous pass.

use gpui::{
    div, px, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::app_state::{Action, App};
use crate::panels::{ui_colors, ui_divider, ui_section_header, Icon};
use prism_ui::colors;
use crate::Contour;

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let selected = app.selected;
    let n = app.doc.shapes.len();

    // Top row = topmost shape: the shape vec is painted bottom-up (index 0 first),
    // so iterate reversed to put the last-painted shape at the top of the list.
    let rows = app
        .doc
        .shapes
        .iter()
        .enumerate()
        .rev()
        .map(|(i, s)| {
            let is_active = Some(i) == selected;
            let visible = s.visible();
            // Use Eye / EyeOff icon to indicate layer visibility.
            let eye_icon = if visible { Icon::Eye } else { Icon::EyeOff };
            let eye_color = if visible {
                ui_colors::text_primary()
            } else {
                ui_colors::text_disabled()
            };
            let row_bg = if is_active {
                colors::surface_overlay()
            } else {
                colors::surface_raised()
            };
            let name = s.display_name();

            div()
                .id(("shape-row", i))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .bg(row_bg)
                .text_color(ui_colors::text_primary())
                .cursor_pointer()
                // Row body click → select this shape.
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SelectShape(i));
                    cx.notify();
                }))
                .child(
                    // Visibility toggle — own click target, eye icon from prism-ui.
                    div()
                        .id(("shape-vis", i))
                        .w(px(20.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleShapeVisible(i));
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path(eye_icon.path())
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(eye_color),
                        ),
                )
                .child(div().flex_1().child(name))
                .child(
                    div()
                        .text_color(ui_colors::text_secondary())
                        .text_size(px(10.0))
                        .child(s.label().to_string()),
                )
        })
        .collect::<Vec<_>>();

    div()
        .flex_1()
        .flex()
        .flex_col()
        // Section header from the design system.
        .child(ui_section_header(format!("Layers ({n})")))
        .child(ui_divider())
        .children(rows)
}
