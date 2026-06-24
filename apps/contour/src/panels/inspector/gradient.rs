//! Inspector — gradient type/editor sections. Split out of `inspector.rs`.

use super::*;

/// "Add Gradient" button — seeds a Linear gradient from the shape's current solid fill.
pub(super) fn add_gradient_button(shape_id: usize, cx: &mut Context<Contour>) -> impl IntoElement {
    use crate::app_state::Action;
    use crate::gradient::GradientKind;
    div()
        .flex()
        .px_2()
        .pb_1()
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(colors::surface_raised())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |root, _, _, cx| {
                    root.app.apply(Action::SetGradientType {
                        shape_id,
                        kind: GradientKind::Linear,
                    });
                    cx.notify();
                }))
                .child("+ Add Gradient"),
        )
}

/// Gradient type toggle: Linear / Radial / Angle buttons (Wave 11).
pub(super) fn gradient_type_section(
    shape_id: usize,
    current: GradientKind,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let kinds = [GradientKind::Linear, GradientKind::Radial, GradientKind::Angle];
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(ui_section_header("Gradient Type"))
        .child(ui_divider())
        .child(
            div()
                .flex()
                .gap_1()
                .px_3()
                .py_1()
                .children(kinds.into_iter().enumerate().map(|(i, kind)| {
                    let is_active = kind == current;
                    let bg = if is_active { colors::accent() } else { colors::surface_raised() };
                    div()
                        .id(("grad-type", i))
                        .px_2()
                        .h(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(bg)
                        .rounded_md()
                        .text_color(colors::text_primary())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::SetGradientType { shape_id, kind });
                            cx.notify();
                        }))
                        .child(kind.label())
                })),
        )
}

/// The gradient-stop editor section: a horizontal bar visualising the gradient,
/// stop diamonds to select a stop, and controls to add / move / delete / recolour
/// the selected stop.
pub(super) fn gradient_editor_section(
    shape_id: usize,
    gradient: &Gradient,
    selected_stop: Option<usize>,
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    const BAR_W: f32 = 220.0;
    const BAR_H: f32 = 16.0;
    const STOP_STEP: f32 = 0.05;

    // ---- Gradient bar: divide into segments coloured by interpolated sample --
    let sorted = gradient.sorted_stops();
    let n_seg = (sorted.len() * 4).max(8).min(32) as usize;
    let segments = (0..n_seg)
        .map(|k| {
            let t = k as f32 / n_seg as f32;
            let c = gradient.color_at(t);
            div()
                .flex_1()
                .h(px(BAR_H))
                .bg(rgba(pack_rgba(c)))
        })
        .collect::<Vec<_>>();

    let bar = div()
        .id(("grad-bar", shape_id as u64))
        .w(px(BAR_W))
        .h(px(BAR_H))
        .flex()
        .flex_row()
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(SWATCH_BORDER))
        .cursor_pointer()
        // Click on the bar → add a stop at the clicked position (approximated
        // as 0.5 since GPUI 0.2.x click events don't expose local offset).
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            if let Some(g) = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()) {
                let color = g.color_at(0.5);
                root.app.apply(Action::AddGradientStop {
                    shape_id,
                    pos: 0.5,
                    color,
                });
                cx.notify();
            }
        }))
        .children(segments);

    // ---- Stop diamonds (small coloured squares below the bar) ---------------
    let stop_diamonds = sorted
        .iter()
        .enumerate()
        .map(|(si, stop)| {
            let is_sel = Some(si) == selected_stop;
            let border_col = if is_sel { colors::accent() } else { rgb(0x555555u32) };
            div()
                .id(("grad-stop", si as u64))
                .size(px(12.0))
                .rounded_sm()
                .border_2()
                .border_color(border_col)
                .bg(rgba(pack_rgba(stop.color)))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let next = if root.app.selected_gradient_stop == Some(si) {
                        None
                    } else {
                        Some(si)
                    };
                    root.app.apply(Action::SelectGradientStop(next));
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();

    let stops_row = div()
        .flex()
        .flex_row()
        .gap_1()
        .items_center()
        .children(stop_diamonds);

    // ---- Control buttons (move left/right, add, delete) --------------------
    let move_left = step_button(
        ("gd-ml", shape_id as u64),
        "◀",
        cx,
        move |root, cx| {
            if let Some(si) = root.app.selected_gradient_stop {
                let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(g) = g {
                    if let Some(s) = g.stops.get(si) {
                        let new_pos = (s.offset - STOP_STEP).max(0.0);
                        root.app.apply(Action::MoveGradientStop { shape_id, stop_idx: si, new_pos });
                        cx.notify();
                    }
                }
            }
        },
    );
    let move_right = step_button(
        ("gd-mr", shape_id as u64),
        "▶",
        cx,
        move |root, cx| {
            if let Some(si) = root.app.selected_gradient_stop {
                let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                if let Some(g) = g {
                    if let Some(s) = g.stops.get(si) {
                        let new_pos = (s.offset + STOP_STEP).min(1.0);
                        root.app.apply(Action::MoveGradientStop { shape_id, stop_idx: si, new_pos });
                        cx.notify();
                    }
                }
            }
        },
    );

    let n_stops = gradient.stops.len();
    let can_delete = n_stops > 2 && selected_stop.is_some();
    let del_fg = if can_delete { colors::text_primary() } else { colors::text_secondary() };
    let del_btn = {
        let mut b = div()
            .id(("gd-del", shape_id as u64))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(del_fg)
            .text_size(px(11.0))
            .child("✕");
        if can_delete {
            let si = selected_stop.unwrap();
            b = b.cursor_pointer().on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::DeleteGradientStop { shape_id, stop_idx: si });
                root.app.apply(Action::SelectGradientStop(None));
                cx.notify();
            }));
        }
        b
    };

    let controls = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(div().flex_1().text_color(colors::text_secondary()).text_size(px(10.0)).child("Stops:"))
        .child(move_left)
        .child(move_right)
        .child(del_btn);

    // ---- Colour steppers for the selected stop ----------------------------
    let stop_color_editor = selected_stop.and_then(|si| {
        gradient.stops.get(si).map(|stop| {
            let c = stop.color;
            let channels = [("R", 0usize), ("G", 1usize), ("B", 2usize)]
                .into_iter()
                .map(|(lbl, ch)| {
                    let val = (c[ch].clamp(0.0, 1.0) * 255.0).round() as u32;
                    const CSTEP: f32 = 8.0 / 255.0;
                    let dec = div()
                        .id(("gd-ch-dec", (si * 3 + ch) as u64))
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::surface_overlay()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                            if let Some(g) = g {
                                if let Some(s) = g.stops.get(si) {
                                    let mut nc = s.color;
                                    nc[ch] = (nc[ch] - CSTEP).clamp(0.0, 1.0);
                                    root.app.apply(Action::SetGradientStopColor { shape_id, stop_idx: si, color: nc });
                                    cx.notify();
                                }
                            }
                        }))
                        .child("−");
                    let inc = div()
                        .id(("gd-ch-inc", (si * 3 + ch) as u64))
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .border_1()
                        .border_color(colors::surface_border())
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::surface_overlay()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let g = root.app.doc.shapes.get(shape_id).and_then(|s| s.fill_gradient()).cloned();
                            if let Some(g) = g {
                                if let Some(s) = g.stops.get(si) {
                                    let mut nc = s.color;
                                    nc[ch] = (nc[ch] + CSTEP).clamp(0.0, 1.0);
                                    root.app.apply(Action::SetGradientStopColor { shape_id, stop_idx: si, color: nc });
                                    cx.notify();
                                }
                            }
                        }))
                        .child("＋");
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().w(px(14.0)).text_color(colors::text_secondary()).child(lbl))
                        .child(dec)
                        .child(div().flex_1().flex().justify_center().text_color(colors::text_primary()).child(format!("{val}")))
                        .child(inc)
                })
                .collect::<Vec<_>>();
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_color(colors::text_secondary()).text_size(px(10.0)).child("Stop colour:"))
                .children(channels)
        })
    });

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header("Gradient Stops"))
        .child(ui_divider())
        .child(bar)
        .child(stops_row)
        .child(controls)
        .children(stop_color_editor)
}
