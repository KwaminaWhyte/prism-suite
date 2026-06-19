//! Vanishing Point overlay toolbar — perspective-aware clone/stamp controls.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, App, VanishingToolMode};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let open = app.vanishing_point_open;
    let mode = app.vanishing_tool_mode;
    let plane_count = app.vanishing_planes.len();

    div()
        .w_full()
        .flex()
        .flex_col()
        .bg(colors::surface_raised())
        .when(open, |d| {
            d.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(36.0))
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(
                        div()
                            .text_color(colors::text_primary())
                            .text_size(px(font_size::XS))
                            .child("Vanishing Point"),
                    )
                    .child(mode_btn("Define", VanishingToolMode::DefiningPlane, 0, mode, cx))
                    .child(mode_btn("Stamp",  VanishingToolMode::Stamping,      1, mode, cx))
                    .child(mode_btn("Paste",  VanishingToolMode::Pasting,       2, mode, cx))
                    .child(
                        div()
                            .id("vp-add-plane")
                            .cursor_pointer()
                            .px_2()
                            .h(px(22.0))
                            .rounded_md()
                            .bg(colors::surface_overlay())
                            .border_1()
                            .border_color(colors::surface_border())
                            .text_color(colors::text_secondary())
                            .text_size(px(font_size::XS))
                            .flex()
                            .items_center()
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                let cw = root.app.host.doc_w as f32;
                                let ch = root.app.host.doc_h as f32;
                                let (cx2, cy) = (cw * 0.5, ch * 0.5);
                                let d = cw.min(ch) * 0.2;
                                root.app.apply(Action::AddVanishingPlane {
                                    corners: [
                                        [cx2 - d, cy - d],
                                        [cx2 + d, cy - d],
                                        [cx2 + d, cy + d],
                                        [cx2 - d, cy + d],
                                    ],
                                });
                                cx.notify();
                            }))
                            .child("+ Plane"),
                    )
                    .child(
                        div()
                            .text_color(colors::text_secondary())
                            .text_size(px(font_size::XS))
                            .child(format!("{plane_count} plane{}", if plane_count == 1 { "" } else { "s" })),
                    )
                    .child(
                        div()
                            .id("vp-close")
                            .cursor_pointer()
                            .px_2()
                            .text_color(colors::text_secondary())
                            .text_size(px(font_size::XS))
                            .hover(|s| s.bg(colors::tool_hover()))
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::CloseVanishingPoint);
                                cx.notify();
                            }))
                            .child("✕"),
                    ),
            )
        })
}

fn mode_btn(
    label: &'static str,
    target: VanishingToolMode,
    idx: u64,
    current: VanishingToolMode,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let active = current == target;
    div()
        .id(("vp-mode", idx))
        .cursor_pointer()
        .px_2()
        .h(px(22.0))
        .rounded_md()
        .bg(if active { colors::surface_overlay() } else { colors::surface_raised() })
        .border_1()
        .border_color(if active { colors::accent() } else { colors::surface_border() })
        .text_color(if active { colors::text_primary() } else { colors::text_secondary() })
        .text_size(px(font_size::XS))
        .flex()
        .items_center()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::SetVanishingToolMode(target));
            cx.notify();
        }))
        .child(label)
}
