//! Top toolbar — app title, undo/redo, boolean ops, align/distribute, export/import.
//!
//! Icon buttons use `prism_ui::tool_button` / `prism_ui::icon` from the design
//! system.  Text-only controls (zoom, status) remain as compact label buttons.

use gpui::{
    div, px, rgb, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::align::{Align, Distribute};
use crate::boolean::BoolOp;

use crate::app_state::{Action, App};
use crate::panels::{ui_colors, Icon};
use prism_ui::colors;
use crate::Contour;

pub fn render(app: &App, cx: &mut Context<Contour>) -> impl IntoElement {
    let tool = app.active.label();
    let count = app.doc.shapes.len();
    let zoom_pct = (app.view.scale * 100.0).round() as i32;

    let can_undo = app.history.can_undo();
    let can_redo = app.history.can_redo();
    let can_bool = app.can_boolean();
    let can_align = app.can_align();
    let can_dist = app.can_distribute();

    // The toolbar is split into two flex groups so it can never overflow the
    // window:
    //   * `left` — the dense, variable-length cluster of icon buttons. It is the
    //     flex child that grows (`flex_1`) and is allowed to shrink below its
    //     content width (`min_w(0)`); `overflow_x_scroll` clips any overflow to
    //     the group bounds and makes the trailing buttons reachable by wheel /
    //     trackpad scroll instead of bleeding past the right window edge.
    //   * `right` — zoom / tool / status controls, pinned to the right and never
    //     shrunk (`flex_shrink_0`) so they stay visible.
    let left = div()
        .id("toolbar-left")
        .flex_1()
        .min_w(px(0.0))
        .overflow_x_scroll()
        .flex()
        .items_center()
        .gap_4()
        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).flex_shrink_0().child("Contour"))
        .child(
            div()
                .flex_shrink_0()
                .text_color(colors::text_secondary())
                .text_size(px(12.0))
                .child("GPUI preview"),
        )
        // Edit menu: Undo / Redo (also Cmd+Z / Cmd+Shift+Z). Greyed when there
        // is nothing to undo / redo, mirroring the egui app's Edit menu.
        .child(icon_btn("undo", Icon::Undo, can_undo, cx, |root, cx| {
            root.app.apply(Action::Undo);
            cx.notify();
        }))
        .child(icon_btn("redo", Icon::Redo, can_redo, cx, |root, cx| {
            root.app.apply(Action::Redo);
            cx.notify();
        }))
        // Pathfinder (contextual — greyed unless 2 shapes selected).
        .child(div().w(px(1.0)).h(px(18.0)).flex_shrink_0().bg(colors::surface_border()))
        .child(icon_bool_btn("bool-union",     Icon::Link,   can_bool, BoolOp::Union,      cx))
        .child(icon_bool_btn("bool-subtract",  Icon::Unlink, can_bool, BoolOp::Difference, cx))
        .child(icon_bool_btn("bool-intersect", Icon::Remove, can_bool, BoolOp::Intersect,  cx))
        .child(icon_bool_btn("bool-exclude",   Icon::Close,  can_bool, BoolOp::Exclude,    cx))
        // Align + Distribute (contextual — greyed unless ≥2/≥3 selected).
        .child(div().w(px(1.0)).h(px(18.0)).flex_shrink_0().bg(colors::surface_border()))
        .child(icon_align_btn("al-l",  Icon::AlignLeft,   can_align, Align::Left,    cx))
        .child(icon_align_btn("al-ch", Icon::AlignCenter, can_align, Align::CenterH, cx))
        .child(icon_align_btn("al-r",  Icon::AlignRight,  can_align, Align::Right,   cx))
        .child(icon_align_btn("al-t",  Icon::AlignTop,    can_align, Align::Top,     cx))
        .child(icon_align_btn("al-cv", Icon::AlignMiddle, can_align, Align::CenterV, cx))
        .child(icon_align_btn("al-b",  Icon::AlignBottom, can_align, Align::Bottom,  cx))
        .child(icon_dist_btn("ds-h",  Icon::DistributeH, can_dist, Distribute::CentersH,     cx))
        .child(icon_dist_btn("ds-v",  Icon::DistributeV, can_dist, Distribute::CentersV,     cx))
        .child(icon_dist_btn("ds-hg", Icon::DistributeH, can_dist, Distribute::HorizontalGap, cx))
        .child(icon_dist_btn("ds-vg", Icon::DistributeV, can_dist, Distribute::VerticalGap,   cx))
        .child(div().w(px(1.0)).h(px(18.0)).flex_shrink_0().bg(colors::surface_border()))
        .child(icon_align_ab_btn("ab-l",  Icon::AlignLeft,   can_align, Align::Left,    cx))
        .child(icon_align_ab_btn("ab-ch", Icon::AlignCenter, can_align, Align::CenterH, cx))
        .child(icon_align_ab_btn("ab-r",  Icon::AlignRight,  can_align, Align::Right,   cx))
        .child(icon_align_ab_btn("ab-t",  Icon::AlignTop,    can_align, Align::Top,     cx))
        .child(icon_align_ab_btn("ab-cv", Icon::AlignMiddle, can_align, Align::CenterV, cx))
        .child(icon_align_ab_btn("ab-b",  Icon::AlignBottom, can_align, Align::Bottom,  cx))
        // Isolation mode breadcrumb (Wave 11): shown when a group is isolated.
        .children(if app.isolation_group.is_some() {
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(22.0))
                    .bg(colors::accent())
                    .rounded_md()
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .child("Isolation Mode")
                    .child(
                        div()
                            .id("exit-isolation")
                            .px_2()
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .bg(rgb(0x9b79f7))
                            .rounded_md()
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::ExitIsolation);
                                cx.notify();
                            }))
                            .child("Exit"),
                    ),
            )
        } else {
            None
        })
        .child(div().w(px(1.0)).h(px(18.0)).flex_shrink_0().bg(colors::surface_border()))
        .child(zoom_btn("outline-stroke", "Outline", cx, |root, cx| {
            if let Some(idx) = root.app.selected {
                root.app.apply(Action::OutlineStroke(idx));
                cx.notify();
            }
        }))
        .child(zoom_btn("expand-appearance", "Expand", cx, |root, cx| {
            if let Some(idx) = root.app.selected {
                root.app.apply(Action::ExpandAppearance(idx));
                cx.notify();
            }
        }))
        // --- Batch 5: vector-authoring ops ---
        .child(div().w(px(1.0)).h(px(18.0)).flex_shrink_0().bg(colors::surface_border()))
        // Variable-width: bake the selected path's stroke profile into a tapered
        // filled outline.
        .child(zoom_btn("outline-width", "Width▶", cx, |root, cx| {
            if let Some(idx) = root.app.selected {
                root.app.apply(Action::OutlineWidthProfile(idx));
                cx.notify();
            }
        }))
        // Roughen the selection (distort): size 6, detail 2.
        .child(zoom_btn("roughen", "Roughen", cx, |root, cx| {
            if !root.app.selection.is_empty() {
                root.app.apply(Action::RoughenPath { size: 6.0, detail: 2 });
                cx.notify();
            }
        }))
        // Gradient Mesh: seed a 4×4 editable mesh on the selection (toggle clear).
        .child(zoom_btn("make-mesh", "Mesh", cx, |root, cx| {
            if root.app.mesh_points.is_empty() {
                root.app.apply(Action::MakeMeshGradient);
            } else {
                root.app.apply(Action::ClearMeshGradient);
            }
            cx.notify();
        }))
        // Perspective: bake the active free-distort homography into the selection.
        .child(zoom_btn("apply-persp", "Persp▶", cx, |root, cx| {
            if let Some(idx) = root.app.selected {
                root.app.apply(Action::ApplyPerspectiveDistort { shape_id: idx });
                cx.notify();
            }
        }));

    // Right group: zoom / tool / status — pinned right, never shrunk.
    let right = div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_4()
        // Zoom controls: −  NNN%  ＋  Reset. The buttons anchor zoom to the
        // viewport-local origin (`anchor: None, viewport: (0,0)`), keeping the
        // artboard's placed corner stable; cursor-anchored zoom is on scroll.
        .child(zoom_btn("zoom-out", "−", cx, |root, cx| {
            root.app.apply(Action::ZoomBy {
                factor: 0.8,
                anchor: None,
                viewport: (0.0, 0.0),
            });
            cx.notify();
        }))
        .child(
            div()
                .min_w(px(44.0))
                .flex()
                .justify_center()
                .text_color(colors::text_secondary())
                .text_size(px(12.0))
                .child(format!("{zoom_pct}%")),
        )
        .child(zoom_btn("zoom-in", "＋", cx, |root, cx| {
            root.app.apply(Action::ZoomBy {
                factor: 1.25,
                anchor: None,
                viewport: (0.0, 0.0),
            });
            cx.notify();
        }))
        .child(zoom_btn("zoom-reset", "Reset", cx, |root, cx| {
            root.app.apply(Action::ResetView);
            cx.notify();
        }))
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(12.0))
                .child(format!("Tool: {tool}")),
        )
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(12.0))
                .child(format!("{count} shapes")),
        )
        .child({
            let msg = app.status_message.as_ref().and_then(|(m, t)| {
                if t.elapsed().as_secs() < 3 { Some(m.clone()) } else { None }
            });
            div()
                .text_color(rgb(0x7aa2f7))
                .text_size(px(11.0))
                .px_2()
                .children(msg)
        });

    // Outer row: clip to the toolbar bounds so nothing can paint past the
    // window's right edge, then lay out the scrollable left group and the
    // pinned right group side by side.
    div()
        .size_full()
        .flex()
        .items_center()
        .gap_4()
        .px_4()
        .overflow_hidden()
        .text_color(colors::text_primary())
        .child(left)
        .child(right)
}

/// A small text toolbar button that runs `on_click`.
fn zoom_btn(
    id: &'static str,
    glyph: &'static str,
    cx: &mut Context<Contour>,
    on_click: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(22.0))
        .px_2()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .text_size(px(12.0))
        .cursor_pointer()
        .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)))
        .child(glyph)
}

/// An icon toolbar button. `enabled` controls whether it is interactive or muted.
/// The SVG icon comes from `prism_ui::Icon`.
fn icon_btn(
    id: &'static str,
    icon: Icon,
    enabled: bool,
    cx: &mut Context<Contour>,
    on_click: impl Fn(&mut Contour, &mut Context<Contour>) + 'static,
) -> impl IntoElement {
    let color = if enabled {
        ui_colors::text_primary()
    } else {
        ui_colors::text_disabled()
    };
    let mut btn = div()
        .id(id)
        .w(px(28.0))
        .h(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            svg()
                .path(icon.path())
                .w(px(14.0))
                .h(px(14.0))
                .text_color(color),
        );
    if enabled {
        btn = btn
            .cursor_pointer()
            .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)));
    }
    btn
}

/// A pathfinder boolean-op icon button.
fn icon_bool_btn(
    id: &'static str,
    icon: Icon,
    enabled: bool,
    op: BoolOp,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let color = if enabled {
        ui_colors::text_primary()
    } else {
        ui_colors::text_disabled()
    };
    let mut btn = div()
        .id(id)
        .w(px(28.0))
        .h(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            svg()
                .path(icon.path())
                .w(px(14.0))
                .h(px(14.0))
                .text_color(color),
        );
    if enabled {
        btn = btn.cursor_pointer().on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::Boolean(op));
            cx.notify();
        }));
    }
    btn
}

/// An align icon button.
fn icon_align_btn(
    id: &'static str,
    icon: Icon,
    enabled: bool,
    op: Align,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let color = if enabled {
        ui_colors::text_primary()
    } else {
        ui_colors::text_disabled()
    };
    let mut btn = div()
        .id(id)
        .w(px(28.0))
        .h(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            svg()
                .path(icon.path())
                .w(px(14.0))
                .h(px(14.0))
                .text_color(color),
        );
    if enabled {
        btn = btn.cursor_pointer().on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::AlignSelection(op));
            cx.notify();
        }));
    }
    btn
}

/// An align-to-artboard icon button.
fn icon_align_ab_btn(
    id: &'static str,
    icon: Icon,
    enabled: bool,
    op: Align,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let color = if enabled {
        ui_colors::text_primary()
    } else {
        ui_colors::text_disabled()
    };
    let mut btn = div()
        .id(id)
        .w(px(28.0))
        .h(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::accent())
        .child(
            svg()
                .path(icon.path())
                .w(px(14.0))
                .h(px(14.0))
                .text_color(color),
        );
    if enabled {
        btn = btn.cursor_pointer().on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::AlignToArtboard { alignment: op });
            cx.notify();
        }));
    }
    btn
}

/// A distribute icon button.
fn icon_dist_btn(
    id: &'static str,
    icon: Icon,
    enabled: bool,
    op: Distribute,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let color = if enabled {
        ui_colors::text_primary()
    } else {
        ui_colors::text_disabled()
    };
    let mut btn = div()
        .id(id)
        .w(px(28.0))
        .h(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            svg()
                .path(icon.path())
                .w(px(14.0))
                .h(px(14.0))
                .text_color(color),
        );
    if enabled {
        btn = btn.cursor_pointer().on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::DistributeSelection(op));
            cx.notify();
        }));
    }
    btn
}


